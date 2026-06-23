//! Built-in pilot lookup (docs/DESIGN.md §9b).
//!
//! Resolves a character name to its id (ESI), pulls recent **losses** from
//! zKillboard, and fetches each killmail (ship + fitted items) from ESI. The UI
//! aggregates which hulls the pilot flies and reconstructs their fits.

use std::sync::{Arc, Mutex};

const ESI: &str = "https://esi.evetech.net/latest";
const ZKILL: &str = "https://zkillboard.com/api";
/// Cap how many killmails we resolve per lookup (keep zKill/ESI load gentle).
const MAX_LOSSES: usize = 50;

/// A fitted/cargo item from a killmail.
#[derive(Clone, Debug)]
pub struct Item {
    pub type_id: i64,
    pub flag: i64,
    pub qty: i64,
}

/// Slot group an item flag belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    High,
    Mid,
    Low,
    Rig,
    Subsystem,
    Cargo,
    Other,
}

pub fn slot_of(flag: i64) -> Slot {
    match flag {
        27..=34 => Slot::High,
        19..=26 => Slot::Mid,
        11..=18 => Slot::Low,
        92..=94 => Slot::Rig,
        125..=132 => Slot::Subsystem,
        5 => Slot::Cargo,
        87 => Slot::Cargo, // drone bay
        _ => Slot::Other,
    }
}

/// One lost ship (a killmail) with its fit.
#[derive(Clone, Debug)]
pub struct Loss {
    pub killmail_id: i64,
    pub hash: String,
    pub time: i64,
    pub ship_type_id: i64,
    pub items: Vec<Item>,
}

impl Loss {
    /// Type ids fitted in high/mid/low/rig/subsystem slots, sorted — the fit's
    /// identity for grouping (cargo ignored).
    pub fn signature(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self
            .items
            .iter()
            .filter(|i| !matches!(slot_of(i.flag), Slot::Cargo | Slot::Other))
            .flat_map(|i| std::iter::repeat_n(i.type_id, i.qty.max(1) as usize))
            .collect();
        v.sort_unstable();
        v
    }
}

#[derive(Clone, Debug)]
pub struct PilotReport {
    pub name: String,
    pub character_id: i64,
    /// Recent losses, newest first.
    pub losses: Vec<Loss>,
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

    let zk: serde_json::Value = client
        .get(format!("{ZKILL}/losses/characterID/{character_id}/"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("zKillboard: {e}"))?;
    let entries = zk.as_array().cloned().unwrap_or_default();

    let mut losses = Vec::new();
    for km in entries.iter().take(MAX_LOSSES) {
        let Some(id) = km.get("killmail_id").and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(hash) = km.get("zkb").and_then(|z| z.get("hash")).and_then(|h| h.as_str()) else {
            continue;
        };
        if let Some(loss) = killmail(&client, id, hash) {
            losses.push(loss);
        }
    }

    Ok(PilotReport {
        name: resolved,
        character_id,
        losses,
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

/// Fetch a killmail and reconstruct the lost ship + its fit.
fn killmail(client: &reqwest::blocking::Client, id: i64, hash: &str) -> Option<Loss> {
    let km: serde_json::Value = client
        .get(format!("{ESI}/killmails/{id}/{hash}/"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    let victim = km.get("victim")?;
    let ship_type_id = victim.get("ship_type_id")?.as_i64()?;
    let time = km
        .get("killmail_time")
        .and_then(|t| t.as_str())
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp())
        .unwrap_or(0);

    let mut items = Vec::new();
    if let Some(arr) = victim.get("items").and_then(|i| i.as_array()) {
        for it in arr {
            let Some(type_id) = it.get("item_type_id").and_then(|v| v.as_i64()) else {
                continue;
            };
            let flag = it.get("flag").and_then(|v| v.as_i64()).unwrap_or(0);
            let qty = it.get("quantity_destroyed").and_then(|v| v.as_i64()).unwrap_or(0)
                + it.get("quantity_dropped").and_then(|v| v.as_i64()).unwrap_or(0);
            items.push(Item { type_id, flag, qty: qty.max(1) });
        }
    }
    Some(Loss { killmail_id: id, hash: hash.to_owned(), time, ship_type_id, items })
}

/// Bulk-resolve type ids to names via ESI `/universe/names` (≤1000 per call).
pub fn resolve_type_names(ids: &[i64]) -> std::collections::HashMap<i64, String> {
    let mut out = std::collections::HashMap::new();
    // /universe/names rejects duplicate ids (HTTP 400) — dedup first.
    let mut ids: Vec<i64> = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return out;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .user_agent("eve-spai/0.1 (EVE intel tool)")
        .timeout(std::time::Duration::from_secs(20))
        .build()
    else {
        return out;
    };
    for chunk in ids.chunks(1000) {
        let resp: Option<serde_json::Value> = client
            .post(format!("{ESI}/universe/names/"))
            .json(chunk)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .ok();
        if let Some(arr) = resp.as_ref().and_then(|v| v.as_array()) {
            for e in arr {
                if let (Some(id), Some(name)) =
                    (e.get("id").and_then(|i| i.as_i64()), e.get("name").and_then(|n| n.as_str()))
                {
                    out.insert(id, name.to_owned());
                }
            }
        }
    }
    out
}
