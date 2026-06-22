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

pub fn spawn(game_dir: PathBuf, alerts: AlertLog, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut processed: HashMap<PathBuf, usize> = HashMap::new();
        let mut cooldown: HashMap<CombatKind, i64> = HashMap::new();
        loop {
            scan(&game_dir, &alerts, &ctx, &mut processed, &mut cooldown);
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
    ctx: &egui::Context,
    processed: &mut HashMap<PathBuf, usize>,
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
            notify(&text);
            alerts.lock().unwrap().push((now, text));
            fired = true;
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
