//! Built-in pilot lookup (docs/DESIGN.md §9b).
//!
//! Resolves a character name to its id (ESI), pulls recent **losses** from
//! zKillboard, and fetches each killmail (ship + fitted items) from ESI. The UI
//! aggregates which hulls the pilot flies and reconstructs their fits.

use std::sync::{Arc, Mutex};

const ESI: &str = "https://esi.evetech.net/latest";
const ZKILL: &str = "https://zkillboard.com/api";
/// Cap killmails resolved per category per lookup (keep zKill/ESI load gentle, but more
/// than the last 50 the basic API returns).
const MAX_KILLS: usize = 75;
/// At most this many zKill pages (200 each) per category.
const MAX_PAGES: i64 = 2;

/// A fitted/cargo item from a killmail.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Loss {
    pub killmail_id: i64,
    pub hash: String,
    pub time: i64,
    pub ship_type_id: i64,
    pub system_id: i64,
    pub value: f64,
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PilotReport {
    pub name: String,
    pub character_id: i64,
    /// Recent losses, newest first.
    pub losses: Vec<Loss>,
    /// Recent kills (the pilot was an attacker), newest first.
    pub kills: Vec<Loss>,
    /// Recent solo kills, newest first.
    pub solo: Vec<Loss>,
    /// Still fetching categories in the background.
    pub loading: bool,
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

/// On-disk cache path for a character's lookup result.
fn cache_path(character_id: i64) -> Option<std::path::PathBuf> {
    let dir = crate::store::data_dir().ok()?.join("lookup");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("{character_id}.json")))
}

fn load_cache(character_id: i64) -> Option<PilotReport> {
    let path = cache_path(character_id)?;
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn save_cache(report: &PilotReport) {
    if let Some(path) = cache_path(report.character_id) {
        if let Ok(json) = serde_json::to_string(report) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// Newest `new` entries followed by the `cached` ones, capped so the cache stays bounded.
fn combine(new: &[Loss], cached: &[Loss]) -> Vec<Loss> {
    let mut v = new.to_vec();
    v.extend_from_slice(cached);
    v.truncate(200);
    v
}

/// Look up `name`: show the cached result instantly, then fetch only newer killmails (per
/// category, in parallel) and merge, persisting the result for next time.
pub fn spawn_lookup(name: String, state: SharedLookup, ctx: egui::Context) {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return;
    }
    *state.lock().unwrap() = LookupState::Loading(name.clone());
    ctx.request_repaint();
    std::thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool; pilot lookup)"))
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                *state.lock().unwrap() = LookupState::Failed(e.to_string());
                ctx.request_repaint();
                return;
            }
        };
        let (character_id, resolved) = match resolve_name(&client, &name) {
            Ok(v) => v,
            Err(e) => {
                *state.lock().unwrap() = LookupState::Failed(e);
                ctx.request_repaint();
                return;
            }
        };
        // Show the cached report immediately (faster re-lookup), then refresh.
        let base = load_cache(character_id).unwrap_or_else(|| PilotReport {
            name: resolved.clone(),
            character_id,
            losses: Vec::new(),
            kills: Vec::new(),
            solo: Vec::new(),
            loading: true,
        });
        *state.lock().unwrap() = LookupState::Done(PilotReport { name: resolved, loading: true, ..base });
        ctx.request_repaint();

        let pending = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(3));
        for (idx, category) in ["losses", "kills", "solo"].into_iter().enumerate() {
            let cached_list: Vec<Loss> = {
                let g = state.lock().unwrap();
                if let LookupState::Done(r) = &*g {
                    match idx {
                        0 => r.losses.clone(),
                        1 => r.kills.clone(),
                        _ => r.solo.clone(),
                    }
                } else {
                    Vec::new()
                }
            };
            let known: std::collections::HashSet<i64> = cached_list.iter().map(|l| l.killmail_id).collect();
            let client = client.clone();
            let state = state.clone();
            let ctx = ctx.clone();
            let pending = pending.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(idx as u64 * 500));
                fetch_category(&client, character_id, category, &known, &cached_list, |combined| {
                    if let LookupState::Done(r) = &mut *state.lock().unwrap() {
                        match idx {
                            0 => r.losses = combined.to_vec(),
                            1 => r.kills = combined.to_vec(),
                            _ => r.solo = combined.to_vec(),
                        }
                    }
                    ctx.request_repaint();
                });
                if pending.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                    if let LookupState::Done(r) = &mut *state.lock().unwrap() {
                        r.loading = false;
                        save_cache(r);
                    }
                    ctx.request_repaint();
                }
            });
        }
    });
}

/// Fetch a zKill category's **newer** killmails (stopping at the first already-cached id),
/// merging onto `cached`. `on_batch` gets the merged list as it grows.
fn fetch_category(
    client: &reqwest::blocking::Client,
    character_id: i64,
    category: &str,
    known: &std::collections::HashSet<i64>,
    cached: &[Loss],
    mut on_batch: impl FnMut(&[Loss]),
) {
    let mut new: Vec<Loss> = Vec::new();
    let mut page = 1;
    'pages: while new.len() < MAX_KILLS && page <= MAX_PAGES {
        let url = format!("{ZKILL}/{category}/characterID/{character_id}/page/{page}/");
        let zk: serde_json::Value = match client
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
        {
            Ok(v) => v,
            Err(_) => break,
        };
        let entries = zk.as_array().cloned().unwrap_or_default();
        if entries.is_empty() {
            break;
        }
        for km in &entries {
            let Some(id) = km.get("killmail_id").and_then(|v| v.as_i64()) else { continue };
            if known.contains(&id) {
                break 'pages; // reached already-cached killmails
            }
            if new.len() >= MAX_KILLS {
                break 'pages;
            }
            let Some(hash) = km.get("zkb").and_then(|z| z.get("hash")).and_then(|h| h.as_str())
            else {
                continue;
            };
            let value = km
                .get("zkb")
                .and_then(|z| z.get("totalValue"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if let Some(loss) = killmail(client, id, hash, value) {
                new.push(loss);
                if new.len() % 6 == 0 {
                    on_batch(&combine(&new, cached));
                }
            }
        }
        page += 1;
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    on_batch(&combine(&new, cached));
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

/// Fetch a solar system's recent kills (newest first) for the system-info window. Reuses the
/// `PilotReport.kills` list so the existing `km_list` renderer (badges, skips, click-to-fit)
/// can show them.
pub fn spawn_system_kills(system_id: i64, state: SharedLookup, ctx: egui::Context) {
    *state.lock().unwrap() = LookupState::Loading(format!("system {system_id}"));
    ctx.request_repaint();
    std::thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool; system kills)"))
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                *state.lock().unwrap() = LookupState::Failed(e.to_string());
                ctx.request_repaint();
                return;
            }
        };
        let url = format!("{ZKILL}/solarSystemID/{system_id}/");
        let zk: serde_json::Value =
            match client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.json()) {
                Ok(v) => v,
                Err(e) => {
                    *state.lock().unwrap() = LookupState::Failed(format!("zKill: {e}"));
                    ctx.request_repaint();
                    return;
                }
            };
        *state.lock().unwrap() = LookupState::Done(PilotReport {
            name: format!("System {system_id}"),
            character_id: 0,
            losses: Vec::new(),
            kills: Vec::new(),
            solo: Vec::new(),
            loading: true,
        });
        ctx.request_repaint();
        let mut kills: Vec<Loss> = Vec::new();
        for km in zk.as_array().cloned().unwrap_or_default().iter().take(MAX_KILLS) {
            let Some(id) = km.get("killmail_id").and_then(|v| v.as_i64()) else { continue };
            let Some(hash) = km.get("zkb").and_then(|z| z.get("hash")).and_then(|h| h.as_str()) else {
                continue;
            };
            let value =
                km.get("zkb").and_then(|z| z.get("totalValue")).and_then(|v| v.as_f64()).unwrap_or(0.0);
            if let Some(loss) = killmail(&client, id, hash, value) {
                kills.push(loss);
                if kills.len() % 6 == 0 {
                    if let LookupState::Done(r) = &mut *state.lock().unwrap() {
                        r.kills = kills.clone();
                    }
                    ctx.request_repaint();
                }
            }
        }
        if let LookupState::Done(r) = &mut *state.lock().unwrap() {
            r.kills = kills;
            r.loading = false;
        }
        ctx.request_repaint();
    });
}

/// Fetch a killmail and reconstruct the (victim) ship + its fit.
fn killmail(client: &reqwest::blocking::Client, id: i64, hash: &str, value: f64) -> Option<Loss> {
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
    let system_id = km.get("solar_system_id").and_then(|v| v.as_i64()).unwrap_or(0);
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
    Some(Loss { killmail_id: id, hash: hash.to_owned(), time, ship_type_id, system_id, value, items })
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
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
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
