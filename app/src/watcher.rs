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
        // Last system a sighting was reported in, per channel (id, name).
        let mut last_system: HashMap<String, (i64, String)> = HashMap::new();
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
    last_system: &mut HashMap<String, (i64, String)>,
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
            let mut st = state.lock().unwrap();
            for m in &messages[start..] {
                let received = intel::parse_eve_time(&m.timestamp).unwrap_or(now);
                let mut report =
                    intel::analyze(&m.text, systems, ships, received, &meta.channel, &m.author);

                // Ignore non-placeable chatter: nothing to anchor without a system/gate.
                if report.systems.is_empty() && report.gate.is_none() {
                    continue;
                }

                // Queue candidate pilot names for background ESI confirmation.
                if !report.pilots.is_empty() {
                    let mut cache = pilots.lock().unwrap();
                    for name in &report.pilots {
                        cache.queue(name);
                    }
                }

                // Movement: link to the channel's previous sighting in a different
                // system, recording direction + jump distance.
                if !report.clear {
                    let primary = report.primary_system().map(|s| (s.id, s.name.clone()));
                    if let Some((pid, pname)) = primary {
                        if let Some((prev_id, prev_name)) = last_system.get(&meta.channel) {
                            if *prev_id != pid {
                                report.movement = Some(Movement {
                                    from: prev_name.clone(),
                                    jumps: systems.jumps(*prev_id, pid, MAX_MOVE_JUMPS),
                                });
                            }
                        }
                        last_system.insert(meta.channel.clone(), (pid, pname));
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
