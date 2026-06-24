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
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let channels: Vec<String> = channels.iter().map(|c| c.to_lowercase()).collect();
        let mut processed: HashMap<PathBuf, usize> = HashMap::new();
        // Last sighting per channel: (system id, system name, pilot names lower-cased).
        let mut last_system: HashMap<String, (i64, String, Vec<String>)> = HashMap::new();
        loop {
            scan(
                &chat_dir,
                &channels,
                &systems,
                &ships,
                &pilots,
                &state,
                &ctx,
                &mut processed,
                &mut last_system,
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
    ctx: &egui::Context,
    processed: &mut HashMap<PathBuf, usize>,
    last_system: &mut HashMap<String, (i64, String, Vec<String>)>,
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
        let Some((meta, messages)) = crate::chatlog::read(&path) else {
            continue;
        };
        // Empty channel list = watch everything (useful before channels are set).
        if !channels.is_empty() && !channels.contains(&meta.channel.to_lowercase()) {
            continue;
        }

        let start = processed
            .get(&path)
            .copied()
            .unwrap_or_else(|| messages.len().saturating_sub(FIRST_SIGHT_BACKLOG));
        if messages.len() > start {
            let now = chrono::Utc::now().timestamp();
            let known = pilots.lock().unwrap().confirmed();
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
                );

                // Characters from in-game showinfo links already carry their id —
                // confirm them at once (and persist), then queue the rest for ESI.
                if !report.pilots.is_empty() || !report.char_ids.is_empty() {
                    let mut cache = pilots.lock().unwrap();
                    if !report.char_ids.is_empty() {
                        let store = crate::store::Store::open().ok();
                        for (name, id) in &report.char_ids {
                            cache.confirm(name, *id);
                            if let Some(s) = &store {
                                let _ = s.add_known_pilot(name, *id);
                            }
                        }
                    }
                    // Queue 1–3 word sub-spans so the resolver can confirm the real names
                    // inside an over-glued run ("Wwallddo Lulu Uanid" → Wwallddo + Lulu Uanid).
                    for name in &report.pilots {
                        for w in crate::pilot::name_windows(name) {
                            cache.queue(&w);
                        }
                    }
                    eprintln!(
                        "[pilot] parsed '{}': pilots={:?} char-linked={:?}",
                        m.author,
                        report.pilots,
                        report.char_ids.iter().map(|(n, _)| n).collect::<Vec<_>>()
                    );
                }

                // Successive messages from the same reporter (same/no system, ≤1 min)
                // amend their previous report rather than adding a new one.
                if st.try_amend(&report, AMEND_GRACE) {
                    continue;
                }

                // Ignore non-placeable chatter: nothing to anchor without a system/gate.
                if report.systems.is_empty() && report.gates.is_empty() {
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
                        if let Ok(store) = crate::store::Store::open() {
                            store.upsert_wormhole(&wh);
                        }
                    }
                }

                st.push(report);
            }
            // Keep reports up to an hour so outdated ones still show (greyed) past
            // the user-configurable outdated threshold; the UI marks staleness.
            st.prune(3600, now);
            any_new = true;
        }
        processed.insert(path, messages.len());
    }

    if any_new {
        ctx.request_repaint();
    }
}
