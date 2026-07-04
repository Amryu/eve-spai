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
