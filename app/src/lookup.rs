//! Built-in pilot lookup (docs/DESIGN.md §9b).
//!
//! Resolves a character name to its id (ESI), pulls recent **losses** from
//! zKillboard, fetches each killmail from ESI, and aggregates which ship hulls the
//! pilot tends to lose — i.e. what they fly. Module-level fit analysis (weapons,
//! active-vs-buffer tank, fitted EHP) needs the full module/dogma SDE and is a
//! follow-up; this first cut works at the hull level from the ship data we bake.

use std::sync::{Arc, Mutex};

const ESI: &str = "https://esi.evetech.net/latest";
const ZKILL: &str = "https://zkillboard.com/api";
/// Cap how many killmails we resolve per lookup (keep zKill/ESI load gentle).
const MAX_LOSSES: usize = 30;

/// One hull the pilot has lost, with how many times (in the sampled window).
#[derive(Clone, Debug)]
pub struct PilotShip {
    pub ship_type_id: i64,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct PilotReport {
    pub name: String,
    pub character_id: i64,
    pub losses_analyzed: usize,
    /// Hulls lost, most-flown first.
    pub ships: Vec<PilotShip>,
}

#[derive(Clone, Debug, Default)]
pub enum LookupState {
    #[default]
    Idle,
    Loading(String),
    Done(PilotReport),
    Failed(String),
}

pub type SharedLookup = Arc<Mutex<LookupState>>;

/// Look up `name` in the background; updates `state` and wakes the UI.
pub fn spawn_lookup(name: String, state: SharedLookup, ctx: egui::Context) {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return;
    }
    *state.lock().unwrap() = LookupState::Loading(name.clone());
    ctx.request_repaint();

    std::thread::spawn(move || {
        let result = run(&name);
        *state.lock().unwrap() = match result {
            Ok(report) => LookupState::Done(report),
            Err(e) => LookupState::Failed(e),
        };
        ctx.request_repaint();
    });
}

fn run(name: &str) -> Result<PilotReport, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("eve-spai/0.1 (EVE intel tool; pilot lookup)")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let (character_id, resolved) = resolve_name(&client, name)?;

    // Recent losses from zKillboard (newest first).
    let losses: serde_json::Value = client
        .get(format!("{ZKILL}/losses/characterID/{character_id}/"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("zKillboard: {e}"))?;
    let entries = losses.as_array().cloned().unwrap_or_default();

    let mut counts: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
    let mut analyzed = 0usize;
    for km in entries.iter().take(MAX_LOSSES) {
        let Some(id) = km.get("killmail_id").and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(hash) = km.get("zkb").and_then(|z| z.get("hash")).and_then(|h| h.as_str()) else {
            continue;
        };
        if let Some(ship) = killmail_ship(&client, id, hash) {
            *counts.entry(ship).or_default() += 1;
            analyzed += 1;
        }
    }

    let mut ships: Vec<PilotShip> = counts
        .into_iter()
        .map(|(ship_type_id, count)| PilotShip { ship_type_id, count })
        .collect();
    ships.sort_by(|a, b| b.count.cmp(&a.count).then(a.ship_type_id.cmp(&b.ship_type_id)));

    Ok(PilotReport {
        name: resolved,
        character_id,
        losses_analyzed: analyzed,
        ships,
    })
}

/// Resolve a character name to (id, canonical name) via ESI.
fn resolve_name(client: &reqwest::blocking::Client, name: &str) -> Result<(i64, String), String> {
    let body: serde_json::Value = client
        .post(format!("{ESI}/universe/ids/"))
        .json(&[name])
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("name lookup: {e}"))?;
    body.get("characters")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| {
            let id = c.get("id")?.as_i64()?;
            let nm = c.get("name")?.as_str()?.to_owned();
            Some((id, nm))
        })
        .ok_or_else(|| format!("No character named \"{name}\""))
}

/// Fetch a killmail and return the victim's ship type id.
fn killmail_ship(client: &reqwest::blocking::Client, id: i64, hash: &str) -> Option<i64> {
    let km: serde_json::Value = client
        .get(format!("{ESI}/killmails/{id}/{hash}/"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    km.get("victim")?.get("ship_type_id")?.as_i64()
}
