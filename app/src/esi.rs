//! ESI access for the active character (docs/DESIGN.md §7.1 E7).
//!
//! Background poller: keeps the active character's solar-system location current
//! (refreshing the access token from the keychain when expired). Drives
//! "N jumps from you" distances in the intel and battle views.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use crate::auth;
use crate::store::Store;
use crate::tokens;

const LOCATION_URL: &str = "https://esi.evetech.net/latest/characters";
const POLL: Duration = Duration::from_secs(20);

/// Shared active character name (written by the UI) and resolved system id.
#[derive(Default)]
pub struct Player {
    pub active_name: String,
    pub system_id: Option<i64>,
    /// True when docked in a station/structure (ESI location has a station id).
    pub docked: bool,
    /// All linked characters' locations: name → (system id, docked).
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
            // Poll every linked character so rules can alert on any of them.
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

/// Set the in-game autopilot destination (`clear` = true) or add a waypoint
/// (`clear` = false) for the active character, via ESI. Requires the
/// `esi-ui.write_waypoint.v1` scope. Runs on a background thread.
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

/// Set an ordered list of waypoints (clears existing, then appends each in turn). Used
/// for wormhole-aware routing: a waypoint at each hole entrance, then the destination.
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
            let clear = i == 0; // first clears any existing route, rest append in order
            let url = format!(
                "https://esi.evetech.net/latest/ui/autopilot/waypoint/?add_to_beginning=false&clear_other_waypoints={clear}&destination_id={sys}"
            );
            let _ = client.post(url).bearer_auth(&token).send();
        }
    });
}

/// Shared slot for a fetched (Jump Drive Calibration, Jump Fuel Conservation) skill pair.
pub type SharedJumpSkills = std::sync::Arc<std::sync::Mutex<Option<(u32, u32)>>>;

/// Fetch the character's Jump Drive Calibration (21611) and Jump Fuel Conservation (21610)
/// trained levels via ESI (`esi-skills.read_skills.v1`) and write them into `out`. Does nothing
/// on failure / missing scope — the planner keeps the manually-entered (assume-V) values.
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

/// Save a fitting to the active character's in-game fitting list, via ESI.
/// Requires `esi-fittings.write_fittings.v1`. `items` = (type_id, flag, quantity).
/// Runs on a background thread.
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

/// Return a valid access token, refreshing via the keychain refresh token if the
/// stored one is within a minute of expiry.
fn current_access_token(
    store: &Store,
    client_id: &str,
    id: i64,
    expires_at: i64,
) -> Option<String> {
    let refresh = tokens::load_refresh(id)?;
    let now = chrono::Utc::now().timestamp();
    // Use the cached access token while it's still valid.
    if expires_at - 60 > now {
        if let Some(access) = store.kv_get(&format!("access:{id}")).filter(|a| !a.is_empty()) {
            return Some(access);
        }
    }

    let fresh = auth::refresh_access_token(client_id, &refresh).ok()?;
    // The refresh token may rotate — persist the new one.
    let _ = tokens::save_refresh(id, &fresh.refresh_token);
    store.kv_set(&format!("access:{id}"), &fresh.access_token);
    let _ = store.update_token_expiry(id, now + fresh.expires_in);
    Some(fresh.access_token)
}
