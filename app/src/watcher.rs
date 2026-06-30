//! Chat-log watcher (docs/DESIGN.md §7.1 E3).
//!
//! A lightweight polling watcher: every interval it scans the Chatlogs directory,
//! parses each `.txt` file, and feeds newly-appended messages from the configured
//! intel channels into the shared intel state. Polling (vs. inotify) keeps it
//! simple and robust across the platforms EVE writes logs on.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::geo::Systems;
use crate::intel::{self, IntelState, Movement};

const POLL: Duration = Duration::from_millis(1500);
/// On first sight of a file, show at most this many trailing messages as backlog.
const FIRST_SIGHT_BACKLOG: usize = 20;
/// Cap movement-distance search (a hostile won't have "moved" further sensibly).
const MAX_MOVE_JUMPS: u32 = 15;
/// Grace window (seconds) for amending a reporter's previous intel post.
const AMEND_GRACE: i64 = 60;

pub fn spawn(
    chat_dir: PathBuf,
    channels: Vec<String>,
    systems: Arc<Systems>,
    ships: Arc<HashMap<String, (i64, String)>>,
    pilots: crate::pilot::SharedPilots,
    state: Arc<Mutex<IntelState>>,
    sightings: crate::intel::SharedSightings,
    activity: crate::activity::SharedActivity,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let channels: Vec<String> = channels.iter().map(|c| c.to_lowercase()).collect();
        let mut processed: HashMap<PathBuf, usize> = HashMap::new();
        // Per-file (size, mtime) so an unchanged log isn't re-read+decoded every poll.
        let mut file_sigs: HashMap<PathBuf, (u64, i64)> = HashMap::new();
        // Last sighting per channel: (system id, system name, pilot names lower-cased).
        let mut last_system: HashMap<String, (i64, String, Vec<String>)> = HashMap::new();
        // One SQLite connection for the watcher's lifetime — opening per message ran the
        // full schema migration under the intel lock and could stall the UI thread.
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
                &ctx,
                &mut processed,
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
fn scan(
    chat_dir: &PathBuf,
    channels: &[String],
    systems: &Systems,
    ships: &HashMap<String, (i64, String)>,
    pilots: &crate::pilot::SharedPilots,
    state: &Mutex<IntelState>,
    sightings: &crate::intel::SharedSightings,
    activity: &crate::activity::SharedActivity,
    ctx: &egui::Context,
    processed: &mut HashMap<PathBuf, usize>,
    file_sigs: &mut HashMap<PathBuf, (u64, i64)>,
    last_system: &mut HashMap<String, (i64, String, Vec<String>)>,
    db: Option<&crate::store::Store>,
    known_regions: &std::collections::HashSet<String>,
    channel_regions: &mut HashMap<String, Vec<String>>,
) {
    let Ok(entries) = std::fs::read_dir(chat_dir) else {
        return;
    };
    let mut any_new = false;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        // Skip a log that hasn't changed (same size + mtime) since we last processed it —
        // avoids re-reading and UTF-16-decoding every file every poll.
        let sig = entry.metadata().ok().map(|md| {
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            (md.len(), mtime)
        });
        if let Some(sig) = sig {
            if processed.contains_key(&path) && file_sigs.get(&path) == Some(&sig) {
                continue;
            }
        }
        let Some((meta, messages)) = crate::chatlog::read(&path) else {
            continue;
        };
        // Empty channel list = watch everything (useful before channels are set).
        if !channels.is_empty() && !channels.contains(&meta.channel.to_lowercase()) {
            continue;
        }
        // The channel's covered regions (from its MOTD) are a hint for disambiguating
        // null-sec abbreviations; learn them once per channel.
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

        let start = processed
            .get(&path)
            .copied()
            .unwrap_or_else(|| messages.len().saturating_sub(FIRST_SIGHT_BACKLOG));
        if messages.len() > start {
            let now = chrono::Utc::now().timestamp();
            // Snapshot the parser's pilot inputs together: `known` excludes demoted-for-inactivity
            // names (Phase 2), and `denied` is exactly that demoted set so their tokens stay free.
            let (known, denied) = {
                let c = pilots.lock().unwrap();
                (c.confirmed(), c.denied())
            };
            let mut st = state.lock().unwrap();
            for m in &messages[start..] {
                // Never parse the channel MOTD / system notices (posted by EVE System).
                if m.author.eq_ignore_ascii_case("EVE System") {
                    continue;
                }
                let received = intel::parse_eve_time(&m.timestamp).unwrap_or(now);
                // The channel's last-known system lets a bare "C-J gate" resolve.
                let context = last_system.get(&meta.channel).map(|(id, _, _)| *id);
                let mut report = intel::analyze_ctx(
                    &m.text, systems, ships, &known, received, &meta.channel, &m.author, context,
                    &regions, &denied,
                );

                // Queue every candidate name for the ESI resolver. A name already CONFIRMED in
                // the cache needs no permutation windowing — a double-space paste of a known
                // pilot resolves straight from cache with no extra ESI calls; only an unconfirmed
                // candidate is windowed into 1–3 word sub-spans so the cover can split an
                // over-glued run ("Wwallddo Lulu Uanid" → Wwallddo + Lulu Uanid).
                if !report.pilots.is_empty() {
                    let mut cache = pilots.lock().unwrap();
                    for name in &report.pilots {
                        let confirmed = matches!(cache.get(name), Some(Some(_)));
                        cache.queue(name);
                        if !confirmed {
                            for w in crate::pilot::name_windows(name) {
                                cache.queue(&w);
                            }
                        }
                    }
                    if cfg!(debug_assertions) {
                        eprintln!("[pilot] parsed '{}': pilots={:?}", m.author, report.pilots);
                    }
                }

                // Record pilot→system sightings (Phase 1 data layer; consumed in Phase 2).
                // Each named pilot × each detected system at the report's time.
                if !report.pilots.is_empty() && !report.systems.is_empty() {
                    let mut sight = sightings.lock().unwrap();
                    for name in &report.pilots {
                        for sys in &report.systems {
                            sight.record(name, sys.id, report.received);
                        }
                    }
                }

                // Successive messages from the same reporter (same/no system, ≤1 min)
                // amend their previous report rather than adding a new one.
                if st.try_amend(&report, AMEND_GRACE, systems) {
                    continue;
                }

                // Ignore non-placeable chatter: nothing to anchor without a system/gate. A held
                // location (a system token still inside an unresolved name blob) is the exception
                // — park it so the reconcile can derive the location once ESI frees the token.
                if report.systems.is_empty()
                    && report.gates.is_empty()
                    && !intel::has_held_system(&report, systems)
                {
                    continue;
                }

                // Movement: only inferred when the new sighting shares a named pilot
                // with the channel's previous sighting (the only reliable identifier;
                // consecutive reports otherwise needn't be the same group).
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

                // Wormhole sighting → record it. The named code's catalogue facts
                // (destination class, size, drifter) win over intel-text guesses; the
                // text only fills what the type leaves open (e.g. K162's destination).
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
                        };
                        if let Some(store) = db {
                            store.upsert_wormhole(&wh);
                        }
                    }
                }

                st.push(report);
            }
            // Keep reports up to an hour so outdated ones still show (greyed) past
            // the user-configurable outdated threshold; the UI marks staleness.
            st.prune(3600, now);
            drop(st);
            // Drop sightings outside the 4h window so the index doesn't grow unbounded.
            sightings.lock().unwrap().prune(now);
            any_new = true;
        }
        if let Some(sig) = sig {
            file_sigs.insert(path.clone(), sig);
        }
        processed.insert(path, messages.len());
    }

    // Phase 2: demote confirmed pilots whose character has no recent zKill activity (with a
    // young-account exemption + multi-system revival). Runs every poll so the set re-derives and
    // names auto-revive; only an actual flip re-parses the affected reports.
    demote_pass(pilots, activity, sightings, state, systems, ships, last_system, channel_regions, ctx);

    if any_new {
        ctx.request_repaint();
    }
}

/// Re-derive the demoted-for-inactivity pilot set and, on a flip, re-parse the reports that
/// mention a flipped name so a newly-demoted name frees its tokens (keywords/ships/other pilots)
/// and a revived name is re-anchored.
///
/// Lock discipline (no ABBA with the fetcher/watcher/reconcile threads): `pilots`, `activity`,
/// and `sightings` are taken only as brief LEAF locks (lock → read/clone → drop) and never held
/// while another is acquired. The re-parse holds ONLY `intel_state` (the parser inputs are
/// snapshotted from `pilots` first), so it never nests `pilots` under `intel_state`.
#[allow(clippy::too_many_arguments)]
fn demote_pass(
    pilots: &crate::pilot::SharedPilots,
    activity: &crate::activity::SharedActivity,
    sightings: &crate::intel::SharedSightings,
    state: &Mutex<IntelState>,
    systems: &Systems,
    ships: &HashMap<String, (i64, String)>,
    last_system: &HashMap<String, (i64, String, Vec<String>)>,
    channel_regions: &HashMap<String, Vec<String>>,
    ctx: &egui::Context,
) {
    let now = chrono::Utc::now().timestamp();
    // 1. Snapshot every confirmed character (incl. currently demoted) + the old demoted set.
    let (candidates, old_demoted) = {
        let c = pilots.lock().unwrap();
        (c.all_confirmed(), c.denied())
    };
    if candidates.is_empty() {
        return;
    }
    // 2. Queue/refresh + read each character's activity (leaf lock, dropped before step 3).
    let acts: HashMap<i64, Option<crate::activity::Activity>> = {
        let mut a = activity.lock().unwrap();
        candidates
            .values()
            .map(|&id| {
                a.want(id);
                (id, a.get(id))
            })
            .collect()
    };
    // 3. Derive the demoted set. `None` activity (not fetched yet) KEEPs. The revival check is a
    //    brief sightings leaf lock.
    let new_demoted: std::collections::HashSet<String> = {
        let s = sightings.lock().unwrap();
        candidates
            .iter()
            .filter_map(|(name, id)| {
                let a = acts.get(id).copied().flatten()?; // not fetched yet → KEEP
                let revived = s.revived(name, now);
                crate::activity::demote_decision(a.active_recent, a.birthday, now, revived)
                    .then(|| name.clone())
            })
            .collect()
    };
    // 4. Replace the demotion set (pilots leaf lock) and detect the flip vs. the previous set.
    let flipped: std::collections::HashSet<String> =
        old_demoted.symmetric_difference(&new_demoted).cloned().collect();
    pilots.lock().unwrap().set_demoted(new_demoted);
    if flipped.is_empty() {
        return; // re-deriving the same set is cheap; only a flip re-parses
    }
    // 5. A flip: snapshot the parser inputs (known excludes demoted; denied = demoted) under a
    //    brief pilots leaf lock, then hold ONLY intel_state while re-parsing.
    let (known, denied) = {
        let c = pilots.lock().unwrap();
        (c.confirmed(), c.denied())
    };
    let mut st = state.lock().unwrap();
    let mut changed = false;
    for r in &mut st.reports {
        // A report is affected if its text mentions a name that just flipped (in either
        // direction): a demoted name still in `pilots`, or a revived name whose tokens were free.
        let toks: Vec<String> =
            intel::tokenize(&r.text).iter().map(|t| t.to_lowercase()).collect();
        let mentions = flipped.iter().any(|f| {
            let fw: Vec<&str> = f.split_whitespace().collect();
            !fw.is_empty() && toks.windows(fw.len()).any(|w| w.iter().zip(&fw).all(|(a, b)| a == b))
        });
        if !mentions {
            continue;
        }
        let context = last_system.get(&r.channel).map(|(id, _, _)| *id);
        let regions = channel_regions.get(&r.channel).cloned().unwrap_or_default();
        let mut fresh = intel::analyze_ctx(
            &r.text, systems, ships, &known, r.received, &r.channel, &r.reporter, context,
            &regions, &denied,
        );
        fresh.id = r.id; // preserve the stable report id across the in-place replace
        fresh.movement = r.movement.take(); // movement is set by the watcher, not the parser
        *r = fresh;
        changed = true;
    }
    drop(st);
    if changed {
        ctx.request_repaint();
    }
}
