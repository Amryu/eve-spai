use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use crate::auth;
use crate::store::Store;
use crate::tokens;

const LOCATION_URL: &str = "https://esi.evetech.net/latest/characters";
const POLL: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct Player {
    pub active_name: String,
    pub system_id: Option<i64>,
    pub docked: bool,
    pub locations: std::collections::HashMap<String, (i64, bool)>,
}

pub type SharedPlayer = Arc<Mutex<Player>>;

pub fn spawn_location_poller(client_id: String, player: SharedPlayer, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        loop {
            std::thread::sleep(POLL);
            let active = player.lock().unwrap().active_name.clone();
            let Ok(store) = Store::open() else { continue };
            let mut fresh: std::collections::HashMap<String, (i64, bool)> =
                std::collections::HashMap::new();
            for ch in store.list_characters() {
                if let Some((sys, docked)) = location_for(&client, &store, &client_id, &ch.name) {
                    fresh.insert(ch.name, (sys, docked));
                }
            }
            let mut p = player.lock().unwrap();
            let active_loc = fresh.get(&active).copied();
            let changed = p.locations != fresh
                || p.system_id != active_loc.map(|(s, _)| s)
                || p.docked != active_loc.map(|(_, d)| d).unwrap_or(false);
            p.locations = fresh;
            p.system_id = active_loc.map(|(s, _)| s);
            p.docked = active_loc.map(|(_, d)| d).unwrap_or(false);
            if changed {
                ctx.request_repaint();
            }
        }
    });
}

fn location_for(
    client: &reqwest::blocking::Client,
    store: &Store,
    client_id: &str,
    name: &str,
) -> Option<(i64, bool)> {
    let character = store.character_by_name(name)?;
    let token = current_access_token(store, client_id, character.id, character.expires_at)?;

    // Skip offline characters: ESI still returns their last-known location, but it must not
    // drive alert distances (an offline alt elsewhere was triggering far-away alerts).
    #[derive(Deserialize)]
    struct Online {
        online: bool,
    }
    let online: Online = client
        .get(format!("{LOCATION_URL}/{}/online/", character.id))
        .bearer_auth(&token)
        .send()
        .ok()?
        .json()
        .ok()?;
    if !online.online {
        return None;
    }

    #[derive(Deserialize)]
    struct Location {
        solar_system_id: i64,
        station_id: Option<i64>,
        structure_id: Option<i64>,
    }
    let url = format!("{LOCATION_URL}/{}/location/", character.id);
    let loc: Location = client.get(url).bearer_auth(token).send().ok()?.json().ok()?;
    let docked = loc.station_id.is_some() || loc.structure_id.is_some();
    Some((loc.solar_system_id, docked))
}

pub fn set_waypoint(
    client_id: String,
    char_name: String,
    system_id: i64,
    clear: bool,
) {
    std::thread::spawn(move || {
        let Ok(store) = Store::open() else { return };
        let Some(character) = store.character_by_name(&char_name) else { return };
        let Some(token) =
            current_access_token(&store, &client_id, character.id, character.expires_at)
        else {
            return;
        };
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        let url = format!(
            "https://esi.evetech.net/latest/ui/autopilot/waypoint/?add_to_beginning=false&clear_other_waypoints={clear}&destination_id={system_id}"
        );
        let _ = client.post(url).bearer_auth(token).send();
    });
}

pub fn set_route(client_id: String, char_name: String, waypoints: Vec<i64>) {
    std::thread::spawn(move || {
        let Ok(store) = Store::open() else { return };
        let Some(character) = store.character_by_name(&char_name) else { return };
        let Some(token) =
            current_access_token(&store, &client_id, character.id, character.expires_at)
        else {
            return;
        };
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        for (i, sys) in waypoints.iter().enumerate() {
            let clear = i == 0;
            let url = format!(
                "https://esi.evetech.net/latest/ui/autopilot/waypoint/?add_to_beginning=false&clear_other_waypoints={clear}&destination_id={sys}"
            );
            let _ = client.post(url).bearer_auth(&token).send();
        }
    });
}

pub type SharedJumpSkills = std::sync::Arc<std::sync::Mutex<Option<(u32, u32)>>>;

/// Fetch the character's Jump Drive Calibration (21611) and Jump Fuel Conservation (21610)
pub fn fetch_jump_skills(
    client_id: String,
    char_name: String,
    out: SharedJumpSkills,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let Ok(store) = Store::open() else { return };
        let Some(character) = store.character_by_name(&char_name) else { return };
        let Some(token) =
            current_access_token(&store, &client_id, character.id, character.expires_at)
        else {
            return;
        };
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        #[derive(serde::Deserialize)]
        struct Skill {
            skill_id: i64,
            active_skill_level: u32,
        }
        #[derive(serde::Deserialize)]
        struct Skills {
            skills: Vec<Skill>,
        }
        let url = format!(
            "https://esi.evetech.net/latest/characters/{}/skills/?datasource=tranquility",
            character.id
        );
        let Ok(resp) = client.get(url).bearer_auth(&token).send() else { return };
        let Ok(skills) = resp.error_for_status().and_then(|r| r.json::<Skills>()) else { return };
        let level = |id: i64| skills.skills.iter().find(|s| s.skill_id == id).map(|s| s.active_skill_level);
        if let (Some(jdc), Some(jfc)) = (level(21611), level(21610)) {
            *out.lock().unwrap() = Some((jdc, jfc));
            ctx.request_repaint();
        }
    });
}

pub fn save_fitting(
    client_id: String,
    char_name: String,
    name: String,
    ship_type_id: i64,
    items: Vec<(i64, i64, i64)>,
) {
    std::thread::spawn(move || {
        let Ok(store) = Store::open() else { return };
        let Some(character) = store.character_by_name(&char_name) else { return };
        let Some(token) =
            current_access_token(&store, &client_id, character.id, character.expires_at)
        else {
            return;
        };
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        let body = serde_json::json!({
            "name": name,
            "description": "Saved by EVE Spai",
            "ship_type_id": ship_type_id,
            "items": items.iter().map(|(t, f, q)| serde_json::json!({
                "type_id": t, "flag": f, "quantity": q
            })).collect::<Vec<_>>(),
        });
        let url = format!("https://esi.evetech.net/latest/characters/{}/fittings/", character.id);
        let _ = client.post(url).bearer_auth(token).json(&body).send();
    });
}

const FLEET_POLL: Duration = Duration::from_secs(7);

/// id -> (ship name, SDE group name), preloaded once so the poller does no SQLite per member.
pub type ShipTypeMap = Arc<std::collections::HashMap<i64, (String, String)>>;

fn esi_client() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
        .timeout(Duration::from_secs(20))
        .build()
        .ok()
}

fn fleet_id_for(
    client: &reqwest::blocking::Client,
    store: &Store,
    client_id: &str,
    name: &str,
) -> Option<i64> {
    let character = store.character_by_name(name)?;
    let token = current_access_token(store, client_id, character.id, character.expires_at)?;
    #[derive(Deserialize)]
    struct Fleet {
        fleet_id: i64,
    }
    // 404 = not in a fleet; error_for_status turns it into None instead of a JSON parse error.
    let fleet: Fleet = client
        .get(format!("{LOCATION_URL}/{}/fleet/", character.id))
        .bearer_auth(&token)
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    Some(fleet.fleet_id)
}

#[derive(Deserialize)]
struct RawMember {
    character_id: i64,
    #[serde(default)]
    ship_type_id: i64,
    #[serde(default)]
    solar_system_id: i64,
}

fn fleet_members_raw(
    client: &reqwest::blocking::Client,
    store: &Store,
    client_id: &str,
    boss_id: i64,
    boss_expires: i64,
    fleet_id: i64,
) -> Option<Vec<RawMember>> {
    let token = current_access_token(store, client_id, boss_id, boss_expires)?;
    client
        .get(format!("https://esi.evetech.net/latest/fleets/{fleet_id}/members/"))
        .bearer_auth(&token)
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()
}

/// GET /fleets/{id}/ -> is_registered (fleet advertised). Defaults false on any failure.
fn fleet_is_registered(
    client: &reqwest::blocking::Client,
    store: &Store,
    client_id: &str,
    boss_id: i64,
    boss_expires: i64,
    fleet_id: i64,
) -> bool {
    #[derive(Deserialize)]
    struct FleetInfo {
        #[serde(default)]
        is_registered: bool,
    }
    let Some(token) = current_access_token(store, client_id, boss_id, boss_expires) else {
        return false;
    };
    client
        .get(format!("https://esi.evetech.net/latest/fleets/{fleet_id}/"))
        .bearer_auth(&token)
        .send()
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .and_then(|r| r.json::<FleetInfo>().ok())
        .map(|f| f.is_registered)
        .unwrap_or(false)
}

fn resolve_names(client: &reqwest::blocking::Client, ids: &[i64]) -> std::collections::HashMap<i64, String> {
    #[derive(Deserialize)]
    struct Named {
        id: i64,
        name: String,
    }
    if ids.is_empty() {
        return std::collections::HashMap::new();
    }
    client
        .post("https://esi.evetech.net/latest/universe/names/?datasource=tranquility")
        .json(ids)
        .send()
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .and_then(|r| r.json::<Vec<Named>>().ok())
        .map(|v| v.into_iter().map(|n| (n.id, n.name)).collect())
        .unwrap_or_default()
}

/// Poll the FC's fleet composition while Rescue Mode is active and write it into `RescueState`.
/// Every network path degrades to keeping the previous snapshot (marked stale); it never panics.
pub fn spawn_fleet_poller(
    client_id: String,
    player: SharedPlayer,
    rescue: Arc<Mutex<crate::rescue::RescueState>>,
    ship_types: ShipTypeMap,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let Some(client) = esi_client() else { return };
        loop {
            std::thread::sleep(FLEET_POLL);
            // Poll whenever rescue is active, INCLUDING test mode (test only disables sending, so
            // the FC still sees their real fleet composition). Skip only when inactive.
            if !rescue.lock().unwrap().active {
                continue;
            }
            let Ok(store) = Store::open() else { continue };
            let want = player.lock().unwrap().active_name.clone();
            // Try the active character first, then any other, so the FC's boss character is found.
            let mut chars = store.list_characters();
            chars.sort_by_key(|c| c.name != want);

            let mut built: Option<crate::rescue::FleetSnapshot> = None;
            for ch in &chars {
                let Some(fleet_id) = fleet_id_for(&client, &store, &client_id, &ch.name) else {
                    continue;
                };
                let Some(raw) =
                    fleet_members_raw(&client, &store, &client_id, ch.id, ch.expires_at, fleet_id)
                else {
                    // In a fleet but not the boss (members endpoint 403) — try another character.
                    continue;
                };
                let ids: Vec<i64> = raw.iter().map(|m| m.character_id).collect();
                let names = resolve_names(&client, &ids);
                let now = chrono::Utc::now().timestamp();
                let members = raw
                    .into_iter()
                    .map(|m| {
                        let (ship, group) = ship_types
                            .get(&m.ship_type_id)
                            .cloned()
                            .unwrap_or_else(|| (String::new(), String::new()));
                        let role = crate::rescue::classify(&group);
                        crate::rescue::FleetMember {
                            character_id: m.character_id,
                            name: names.get(&m.character_id).cloned().unwrap_or_default(),
                            ship_type_id: m.ship_type_id,
                            ship,
                            group,
                            role,
                            system_id: m.solar_system_id,
                        }
                    })
                    .collect();
                let mut snap = crate::rescue::FleetSnapshot::build(Some(fleet_id), members, now);
                snap.is_registered =
                    fleet_is_registered(&client, &store, &client_id, ch.id, ch.expires_at, fleet_id);
                built = Some(snap);
                break;
            }

            {
                let mut r = rescue.lock().unwrap();
                match built {
                    Some(snap) => {
                        // Update sticky snowflakes from the FRESH ship types (handles re-ships), but
                        // never remove an existing one — a podded titan pilot stays flagged.
                        let cap = r.capital_pilot.as_deref().map(|s| s.to_lowercase());
                        let cyno = r.cyno_pilot.as_deref().map(|s| s.to_lowercase());
                        for m in &snap.members {
                            if let Some(tag) =
                                crate::rescue::snowflake_tag(m, cap.as_deref(), cyno.as_deref())
                            {
                                r.snowflakes.entry(m.character_id).or_insert_with(|| tag.to_string());
                            }
                        }
                        r.fleet = snap;
                    }
                    // Keep the last good composition on screen, flag it as stale.
                    None => r.fleet.stale = true,
                }
            }
            ctx.request_repaint();
        }
    });
}

fn refresh_lock(id: i64) -> std::sync::Arc<std::sync::Mutex<()>> {
    static LOCKS: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<i64, std::sync::Arc<std::sync::Mutex<()>>>>,
    > = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    LOCKS.lock().unwrap().entry(id).or_default().clone()
}

fn current_access_token(
    store: &Store,
    client_id: &str,
    id: i64,
    expires_at: i64,
) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    // 60s margin so a token doesn't expire mid-request.
    if expires_at - 60 > now {
        if let Some(access) = store.kv_get(&format!("access:{id}")).filter(|a| !a.is_empty()) {
            return Some(access);
        }
    }

    // EVE SSO rotates the refresh token on each use, so two threads refreshing the same
    // character concurrently would invalidate each other and log it out. Serialise per
    // character, then re-check: another thread may have just refreshed while we waited.
    let lock = refresh_lock(id);
    let _guard = lock.lock().unwrap();
    let now = chrono::Utc::now().timestamp();
    if store.token_expiry(id).is_some_and(|exp| exp - 60 > now) {
        if let Some(access) = store.kv_get(&format!("access:{id}")).filter(|a| !a.is_empty()) {
            return Some(access);
        }
    }

    // Load the refresh token inside the lock so we pick up a rotation from another thread.
    let refresh = tokens::load_refresh(id)?;
    let fresh = auth::refresh_access_token(client_id, &refresh).ok()?;
    // The refresh token may rotate — persist the new one.
    let _ = tokens::save_refresh(id, &fresh.refresh_token);
    store.kv_set(&format!("access:{id}"), &fresh.access_token);
    let _ = store.update_token_expiry(id, now + fresh.expires_in);
    Some(fresh.access_token)
}
