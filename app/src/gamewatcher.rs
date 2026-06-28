//! Game-log watcher: polls the Gamelogs directory and fires desktop alerts on
//! combat events (under attack / warp scrambled), with per-kind cooldowns.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::gamelog::{self, CombatKind};

const POLL: Duration = Duration::from_millis(1500);

/// Shared log of fired alerts (unix seconds, text), shared with the Alerts view.
pub type AlertLog = Arc<Mutex<Vec<(i64, String)>>>;

pub fn spawn(
    game_dir: PathBuf,
    alerts: AlertLog,
    notify_on: Arc<std::sync::atomic::AtomicBool>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let mut processed: HashMap<PathBuf, usize> = HashMap::new();
        // Per-file (size, mtime) so an unchanged game log isn't re-read+parsed every poll.
        let mut file_sigs: HashMap<PathBuf, (u64, i64)> = HashMap::new();
        let mut cooldown: HashMap<CombatKind, i64> = HashMap::new();
        loop {
            scan(&game_dir, &alerts, &notify_on, &ctx, &mut processed, &mut file_sigs, &mut cooldown);
            std::thread::sleep(POLL);
        }
    });
}

fn cooldown_secs(kind: CombatKind) -> i64 {
    match kind {
        CombatKind::Scrambled => 30,
        CombatKind::UnderAttack => 60,
    }
}

fn scan(
    game_dir: &PathBuf,
    alerts: &AlertLog,
    notify_on: &std::sync::atomic::AtomicBool,
    ctx: &egui::Context,
    processed: &mut HashMap<PathBuf, usize>,
    file_sigs: &mut HashMap<PathBuf, (u64, i64)>,
    cooldown: &mut HashMap<CombatKind, i64>,
) {
    let Ok(entries) = std::fs::read_dir(game_dir) else {
        return;
    };
    let mut fired = false;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        // Skip a game log unchanged (same size + mtime) since we last processed it.
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
        let lines = gamelog::read(&path);
        // First sight: skip the backlog — only alert on live events.
        let start = processed
            .get(&path)
            .copied()
            .unwrap_or(lines.len())
            .min(lines.len());

        let now = chrono::Utc::now().timestamp();
        for line in &lines[start..] {
            let Some(kind) = line.kind else { continue };
            let last = cooldown.get(&kind).copied().unwrap_or(0);
            if now - last < cooldown_secs(kind) {
                continue;
            }
            cooldown.insert(kind, now);
            let text = kind.message().to_owned();
            if notify_on.load(std::sync::atomic::Ordering::Relaxed) {
                notify(&text);
            }
            alerts.lock().unwrap().push((now, text));
            fired = true;
        }
        if let Some(sig) = sig {
            file_sigs.insert(path.clone(), sig);
        }
        processed.insert(path, lines.len());
    }

    if fired {
        // Trim the shared log.
        let mut log = alerts.lock().unwrap();
        let len = log.len();
        if len > 50 {
            log.drain(0..len - 50);
        }
        ctx.request_repaint();
    }
}

fn notify(text: &str) {
    let text = text.to_owned();
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .summary("EVE Spai")
            .body(&text)
            .show();
    });
}
