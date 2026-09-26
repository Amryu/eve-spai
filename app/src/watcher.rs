use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::geo::Systems;
use crate::intel::{self, IntelState, Movement};

const POLL: Duration = Duration::from_millis(1500);
const FIRST_SIGHT_BACKLOG: usize = 20;
const MAX_MOVE_JUMPS: u32 = 15;
const AMEND_GRACE: i64 = 60;
const REVIVAL_TTL_SECS: i64 = 30 * 86400;

pub type SharedRevivals = Arc<Mutex<HashMap<String, i64>>>;

/// Lines handed to the watcher as if EVE had written them to the named channel's log: the alert
/// rules' test intel.
pub type SharedInject = Arc<Mutex<Vec<(String, crate::chatlog::ChatMessage)>>>;

fn revival_refresh(current_until: Option<i64>, triggered: bool, now: i64) -> Option<i64> {
    let already = current_until.is_some_and(|u| u > now);
    (already || triggered).then_some(now + REVIVAL_TTL_SECS)
}

/// The rescue state handle, degraded to `()` when the FC rescue feature is off. Keeping one
/// signature avoids `#[cfg]`-ing the arguments at the call site, which Rust does not allow.
#[cfg(feature = "fleet")]
pub type RescueHandle = Arc<Mutex<crate::rescue::RescueState>>;
#[cfg(not(feature = "fleet"))]
pub type RescueHandle = ();

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    chat_dir: PathBuf,
    channels: Vec<String>,
    systems: Arc<Systems>,
    ships: Arc<HashMap<String, (i64, String)>>,
    pilots: crate::pilot::SharedPilots,
    state: Arc<Mutex<IntelState>>,
    sightings: crate::intel::SharedSightings,
    activity: crate::activity::SharedActivity,
    revivals: SharedRevivals,
    rescue: RescueHandle,
    rescue_channel: String,
    ship_groups: Arc<HashMap<String, String>>,
    inject: SharedInject,
    ctx: egui::Context,
) {
    let _ = std::thread::Builder::new().name("intel-watcher".into()).spawn(move || {
        let channels: Vec<String> = channels.iter().map(|c| c.to_lowercase()).collect();
        let rescue_channel = rescue_channel.to_lowercase();
        // Where each log has been read to, and the channel its header named.
        let mut tails: HashMap<PathBuf, (u64, String)> = HashMap::new();
        let mut file_sigs: HashMap<PathBuf, (u64, i64)> = HashMap::new();
        let mut last_system: HashMap<String, (i64, String, Vec<String>)> = HashMap::new();
        // One connection for the watcher's lifetime: opening one per message runs the schema
        // migration under the intel lock and can stall the UI thread.
        let db = crate::store::Store::open().ok();
        let known_regions = systems.region_names();
        let mut channel_regions: HashMap<String, Vec<String>> = HashMap::new();
        loop {
            scan(
                &chat_dir,
                &channels,
                &systems,
                &ships,
                &pilots,
                &state,
                &sightings,
                &activity,
                &revivals,
                &rescue,
                &rescue_channel,
                &ship_groups,
                &inject,
                &ctx,
                &mut tails,
                &mut file_sigs,
                &mut last_system,
                db.as_ref(),
                &known_regions,
                &mut channel_regions,
            );
            std::thread::sleep(POLL);
        }
    });
}

#[allow(clippy::too_many_arguments)]
// rescue/rescue_channel/ship_groups feed only the FC-rescue branch below.
#[cfg_attr(not(feature = "fleet"), allow(unused_variables))]
fn scan(
    chat_dir: &PathBuf,
    channels: &[String],
    systems: &Systems,
    ships: &HashMap<String, (i64, String)>,
    pilots: &crate::pilot::SharedPilots,
    state: &Mutex<IntelState>,
    sightings: &crate::intel::SharedSightings,
    activity: &crate::activity::SharedActivity,
    revivals: &SharedRevivals,
    rescue: &RescueHandle,
    rescue_channel: &str,
    ship_groups: &HashMap<String, String>,
    inject: &SharedInject,
    ctx: &egui::Context,
    tails: &mut HashMap<PathBuf, (u64, String)>,
    file_sigs: &mut HashMap<PathBuf, (u64, i64)>,
    last_system: &mut HashMap<String, (i64, String, Vec<String>)>,
    db: Option<&crate::store::Store>,
    known_regions: &std::collections::HashSet<String>,
    channel_regions: &mut HashMap<String, Vec<String>>,
) {
    let mut any_new = false;
    let mut batches: Vec<(crate::chatlog::ChatMeta, Vec<crate::chatlog::ChatMessage>)> = Vec::new();
    for (channel, m) in std::mem::take(&mut *inject.lock().unwrap()) {
        match batches.iter_mut().find(|(meta, _)| meta.channel == channel) {
            Some((_, v)) => v.push(m),
            None => batches.push((crate::chatlog::ChatMeta { channel }, vec![m])),
        }
    }

    for entry in std::fs::read_dir(chat_dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        // New lines are detected by the size of an open handle: on Windows the DirEntry size and
        // mtime of a log EVE holds open lag by minutes. The DirEntry mtime only skips clearly
        // inactive old logs without opening them.
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|md| md.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if !tails.contains_key(&path)
            && mtime != 0
            && chrono::Utc::now().timestamp() - mtime > 12 * 3600
        {
            continue;
        }
        let sig = crate::logpaths::real_len(&path).map(|len| (len, mtime));
        if let Some(sig) = sig {
            if tails.contains_key(&path) && file_sigs.get(&path) == Some(&sig) {
                continue;
            }
        }
        let (offset, known_channel) = tails.get(&path).cloned().unwrap_or_default();
        let first_sight = offset == 0;
        let Some((header, mut messages, next_offset)) = crate::chatlog::read_tail(&path, offset) else {
            continue;
        };
        let channel = header.map(|h| h.channel).unwrap_or(known_channel);
        if channel.is_empty() {
            continue;
        }
        let meta = crate::chatlog::ChatMeta { channel };
        // Recorded before any filter: a log for a channel we do not follow used to be re-read and
        // re-parsed in full every poll, because nothing ever marked it as seen.
        tails.insert(path.clone(), (next_offset, meta.channel.clone()));
        if let Some(sig) = sig {
            file_sigs.insert(path.clone(), sig);
        }
        // A log first seen mid-session is history, not news: only its tail is worth reading out.
        if first_sight && messages.len() > FIRST_SIGHT_BACKLOG {
            messages.drain(..messages.len() - FIRST_SIGHT_BACKLOG);
        }
        batches.push((meta, messages));
    }

    for (meta, messages) in batches {
        #[cfg(feature = "fleet")]
        let is_rescue =
            !rescue_channel.is_empty() && meta.channel.eq_ignore_ascii_case(rescue_channel);
        #[cfg(not(feature = "fleet"))]
        let is_rescue = false;
        if !is_rescue && !channels.is_empty() && !channels.contains(&meta.channel.to_lowercase()) {
            continue;
        }
        #[cfg(feature = "fleet")]
        if is_rescue {
            if !messages.is_empty() {
                let now = chrono::Utc::now().timestamp();
                let mut events = Vec::new();
                {
                    // Brief intel lock only for cross-channel line dedup; released before the
                    // rescue lock so lock order stays intel -> rescue, never the reverse.
                    let mut st = state.lock().unwrap();
                    for m in &messages {
                        if m.author.eq_ignore_ascii_case("EVE System") {
                            continue;
                        }
                        if st.duplicate_line(&meta.channel, &m.timestamp, &m.author, &m.text) {
                            continue;
                        }
                        let received = intel::parse_eve_time(&m.timestamp).unwrap_or(now);
                        let (author, text) = (m.author.clone(), m.text.clone());
                        // Pure parser under catch_unwind: a bad line drops one event, never the app.
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            crate::rescue::parse_event(&author, &text, received, systems, ship_groups)
                        })) {
                            Ok(ev) => events.push(ev),
                            Err(_) => {
                                eprintln!("[watcher] rescue parser panicked, skipping: {:?}", m.text)
                            }
                        }
                    }
                }
                if !events.is_empty() {
                    let mut r = rescue.lock().unwrap();
                    for ev in events {
                        r.push_event(ev);
                    }
                    drop(r);
                    any_new = true;
                }
            }
            continue;
        }
        let regions = channel_regions
            .entry(meta.channel.clone())
            .or_insert_with(|| {
                messages
                    .iter()
                    .find(|m| {
                        m.author.eq_ignore_ascii_case("EVE System")
                            && m.text.contains("Channel MOTD:")
                    })
                    .map(|m| intel::parse_motd_regions(&m.text, known_regions))
                    .unwrap_or_default()
            })
            .clone();

        if !messages.is_empty() {
            let now = chrono::Utc::now().timestamp();
            let (known, denied) = {
                let c = pilots.lock().unwrap();
                (c.confirmed(), c.denied())
            };
            let mut st = state.lock().unwrap();
            for m in &messages {
                if m.author.eq_ignore_ascii_case("EVE System") {
                    continue;
                }
                if st.duplicate_line(&meta.channel, &m.timestamp, &m.author, &m.text) {
                    continue;
                }
                let received = intel::parse_eve_time(&m.timestamp).unwrap_or(now);
                let context = last_system.get(&meta.channel).map(|(id, _, _)| *id);
                // The parser is pure (no shared locks), so catching a panic here leaves `st` intact
                // and just drops the offending line, instead of aborting the whole app.
                let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    intel::analyze_ctx(
                        &m.text, systems, ships, &known, received, &meta.channel, &m.author, context,
                        &regions, &denied,
                    )
                }));
                let mut report = match parsed {
                    Ok(r) => r,
                    Err(_) => {
                        eprintln!("[watcher] parser panicked on line, skipping: {:?}", m.text);
                        continue;
                    }
                };

                if !report.pilots.is_empty() {
                    let mut cache = pilots.lock().unwrap();
                    for name in &report.pilots {
                        let confirmed = matches!(cache.get(name), Some(Some(_)));
                        cache.queue(name);
                        if !confirmed {
                            for w in crate::pilot::resolvable_windows(name) {
                                cache.queue(&w);
                            }
                        }
                    }
                    if cfg!(debug_assertions) {
                        eprintln!("[pilot] parsed '{}': pilots={:?}", m.author, report.pilots);
                    }
                }

                if !report.pilots.is_empty() && !report.systems.is_empty() {
                    let mut sight = sightings.lock().unwrap();
                    for name in &report.pilots {
                        for sys in &report.systems {
                            sight.record(name, sys.id, report.received);
                        }
                    }
                }

                if st.try_amend(&report, AMEND_GRACE, systems) {
                    continue;
                }

                if report.systems.is_empty()
                    && report.gates.is_empty()
                    && !intel::has_held_system(&report, systems)
                {
                    if !report.pilots.is_empty() || !report.ships.is_empty() {
                        st.stash_orphan(report, AMEND_GRACE, now);
                    }
                    continue;
                }

                if !report.systems.is_empty() {
                    st.reverse_amend(&mut report, AMEND_GRACE);
                }

                if !report.clear {
                    if let Some(sys) = report.primary_system() {
                        let (pid, pname) = (sys.id, sys.name.clone());
                        let cur_pilots: Vec<String> =
                            report.pilots.iter().map(|p| p.to_lowercase()).collect();
                        if let Some((prev_id, prev_name, prev_pilots)) = last_system.get(&meta.channel)
                        {
                            let same_pilot =
                                cur_pilots.iter().any(|p| prev_pilots.contains(p));
                            if *prev_id != pid && same_pilot {
                                report.movement = Some(Movement {
                                    from: prev_name.clone(),
                                    jumps: systems.jumps(*prev_id, pid, MAX_MOVE_JUMPS),
                                });
                            }
                        }
                        last_system.insert(meta.channel.clone(), (pid, pname, cur_pilots));
                    }
                }

                if report.wormhole {
                    if let Some(sys) = report.primary_system() {
                        use crate::wormholes::DestClass;
                        let cat = report.wh_type.as_deref().and_then(crate::wormholes::lookup_type);
                        let dest = match cat.map(|w| w.dest()) {
                            Some(d) if !matches!(d, DestClass::Unknown) => d,
                            _ => report.wh_dest.unwrap_or(DestClass::Unknown),
                        };
                        let wh = crate::wormholes::Wormhole {
                            id: 0,
                            system_id: sys.id,
                            signature: report.wh_sig.clone(),
                            wh_type: report.wh_type.clone(),
                            dest,
                            dest_system_id: None,
                            dest_signature: None,
                            dest_wh_type: None,
                            size: cat.and_then(|w| w.size()),
                            is_drifter: cat.is_some_and(|w| w.is_drifter()) || report.wh_drifter,
                            reported_at: received,
                            explicit_expiry: report.wh_eol.then_some(received + 4 * 3600),
                            source: crate::wormholes::Source::Intel,
                            updated_at: received,
                            ..Default::default()
                        };
                        if let Some(store) = db {
                            store.upsert_wormhole(&wh);
                        }
                    }
                }

                st.push(report);
            }
            st.prune(3600, now);
            drop(st);
            sightings.lock().unwrap().prune(now);
            any_new = true;
        }
    }

    demote_pass(pilots, activity, sightings, revivals, state, db, ctx);

    if any_new {
        ctx.request_repaint();
    }
}

fn feed_pilot_names(reports: &[intel::IntelReport]) -> std::collections::HashSet<String> {
    reports.iter().flat_map(|r| r.pilots.iter()).map(|n| n.to_lowercase()).collect()
}

/// Lock discipline: `intel_state`, `pilots`, `activity`, `sightings`, and `revivals` are taken only
/// as brief leaf locks (lock, read/clone, drop) and never held while another is acquired.
#[allow(clippy::too_many_arguments)]
fn demote_pass(
    pilots: &crate::pilot::SharedPilots,
    activity: &crate::activity::SharedActivity,
    sightings: &crate::intel::SharedSightings,
    revivals: &SharedRevivals,
    state: &Mutex<IntelState>,
    db: Option<&crate::store::Store>,
    ctx: &egui::Context,
) {
    let now = chrono::Utc::now().timestamp();
    let feed_names = {
        let st = state.lock().unwrap();
        feed_pilot_names(&st.reports)
    };
    let (candidates, old_flagged): (Vec<(String, i64)>, std::collections::HashSet<String>) = {
        let c = pilots.lock().unwrap();
        let candidates = feed_names
            .iter()
            .filter_map(|n| match c.get(n) {
                Some(Some(id)) => Some((n.clone(), id)),
                _ => None,
            })
            .collect();
        (candidates, c.flagged())
    };
    if candidates.is_empty() {
        return;
    }
    let acts: HashMap<i64, Option<crate::activity::Activity>> = {
        let mut a = activity.lock().unwrap();
        candidates
            .iter()
            .map(|&(_, id)| {
                a.want(id);
                (id, a.get(id))
            })
            .collect()
    };
    let triggered: HashMap<String, bool> = {
        let s = sightings.lock().unwrap();
        candidates.iter().map(|(name, _)| (name.clone(), s.revived(name, now))).collect()
    };
    let mut revival_updates: Vec<(String, i64)> = Vec::new();
    let new_flagged: std::collections::HashSet<String> = {
        let mut rev = revivals.lock().unwrap();
        rev.retain(|_, until| *until > now);
        let mut flagged = old_flagged.clone();
        for (name, id) in &candidates {
            let Some(a) = acts.get(id).copied().flatten() else {
                continue;
            };
            let hit = triggered.get(name).copied().unwrap_or(false);
            let revived = match revival_refresh(rev.get(name).copied(), hit, now) {
                Some(until) => {
                    rev.insert(name.clone(), until);
                    revival_updates.push((name.clone(), until));
                    true
                }
                None => false,
            };
            if crate::activity::demote_decision(
                a.active_recent,
                a.birthday,
                now,
                revived,
                a.last_corp_change,
            ) {
                flagged.insert(name.clone());
            } else {
                flagged.remove(name);
            }
        }
        flagged
    };
    if let Some(store) = db {
        for (name, until) in &revival_updates {
            store.set_revival(name, *until);
        }
    }
    let changed = old_flagged != new_flagged;
    pilots.lock().unwrap().set_activity_flagged(new_flagged);
    if changed {
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intel::IntelReport;

    fn report_with(pilots: &[&str]) -> IntelReport {
        IntelReport { pilots: pilots.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    /// Test intel from the alert rules takes the path of a logged line, with no chat log folder.
    #[test]
    fn injected_line_is_read_as_intel() {
        let mut info = HashMap::new();
        info.insert(
            "rancer".to_owned(),
            crate::geo::SystemInfo {
                id: 30_002_510,
                name: "Rancer".into(),
                security: 0.4,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        );
        let systems = Systems::new(info, HashMap::new());
        let state: Mutex<IntelState> = Default::default();
        let inject: SharedInject = Default::default();
        let msg = |text: &str| crate::chatlog::ChatMessage {
            timestamp: chrono::Utc::now().format("%Y.%m.%d %H:%M:%S").to_string(),
            author: "Test Scout".into(),
            text: text.into(),
        };
        inject.lock().unwrap().push(("Test.Intel".into(), msg("Rancer 3 reds")));
        inject.lock().unwrap().push(("Other.Channel".into(), msg("Rancer 5 reds")));
        let rescue: RescueHandle = Default::default();
        scan(
            &std::path::PathBuf::from("/nonexistent/eve-spai-test-chatlogs"),
            &["test.intel".to_owned()],
            &systems,
            &HashMap::new(),
            &Default::default(),
            &state,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &rescue,
            "",
            &HashMap::new(),
            &inject,
            &egui::Context::default(),
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut HashMap::new(),
            None,
            &Default::default(),
            &mut HashMap::new(),
        );
        assert!(inject.lock().unwrap().is_empty(), "the queue is drained");
        let st = state.lock().unwrap();
        assert_eq!(st.reports.len(), 1, "only the followed channel's line is read");
        let r = &st.reports[0];
        assert_eq!((r.channel.as_str(), r.reporter.as_str(), r.count), ("Test.Intel", "Test Scout", Some(3)));
        assert_eq!(r.systems.first().map(|s| s.id), Some(30_002_510));
    }

    #[test]
    fn feed_pilot_names_dedups_lowercased() {
        let reports = vec![
            report_with(&["Amryu", "Bob Smith"]),
            report_with(&["amryu", "Carol"]),
        ];
        let names = feed_pilot_names(&reports);
        assert_eq!(names.len(), 3);
        assert!(names.contains("amryu"));
        assert!(names.contains("bob smith"));
        assert!(names.contains("carol"));
    }

    #[test]
    fn feed_pilot_names_only_evaluates_feed_present() {
        let reports = vec![report_with(&["Amryu"])];
        let names = feed_pilot_names(&reports);
        assert!(names.contains("amryu"));
        assert!(!names.contains("ghost pilot"));
    }

    #[test]
    fn revival_refresh_sets_and_slides_the_30d_window() {
        let day = 86400;
        let t0 = 1_000_000_000;

        let until0 = revival_refresh(None, true, t0).expect("first roam revives");
        assert_eq!(until0, t0 + REVIVAL_TTL_SECS);

        let t1 = t0 + 10 * day;
        let until1 = revival_refresh(Some(until0), false, t1).expect("mention within window revives");
        assert_eq!(until1, t1 + REVIVAL_TTL_SECS);
        assert!(until1 > until0);

        let t2 = until1 + day;
        assert_eq!(revival_refresh(Some(until1), false, t2), None);
    }
}
