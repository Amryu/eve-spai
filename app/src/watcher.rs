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

use crate::intel::{self, IntelState, SystemIndex};

const POLL: Duration = Duration::from_millis(1500);
/// On first sight of a file, show at most this many trailing messages as backlog.
const FIRST_SIGHT_BACKLOG: usize = 20;

pub fn spawn(
    chat_dir: PathBuf,
    channels: Vec<String>,
    index: Arc<SystemIndex>,
    state: Arc<Mutex<IntelState>>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let channels: Vec<String> = channels.iter().map(|c| c.to_lowercase()).collect();
        let mut processed: HashMap<PathBuf, usize> = HashMap::new();
        loop {
            scan(&chat_dir, &channels, &index, &state, &ctx, &mut processed);
            std::thread::sleep(POLL);
        }
    });
}

fn scan(
    chat_dir: &PathBuf,
    channels: &[String],
    index: &SystemIndex,
    state: &Mutex<IntelState>,
    ctx: &egui::Context,
    processed: &mut HashMap<PathBuf, usize>,
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
                st.push(intel::analyze(&m.text, index, received, &meta.channel, &m.author));
            }
            st.prune(intel::DEFAULT_TTL_SECS, now);
            any_new = true;
        }
        processed.insert(path, messages.len());
    }

    if any_new {
        ctx.request_repaint();
    }
}
