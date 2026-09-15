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
        let Ok(client) = crate::http::client(20)
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
    // drive alert distances, or an offline alt elsewhere triggers far-away alerts.
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
        let Ok(client) = crate::http::client(20)
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
        let Ok(client) = crate::http::client(20)
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

/// Fetch the character's Jump Drive Calibration (21611) and Jump Fuel Conservation (21610) levels.
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
        let Ok(client) = crate::http::client(20)
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
        let Ok(client) = crate::http::client(20)
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

#[cfg(feature = "fc-rescue")]
const FLEET_POLL: Duration = Duration::from_secs(7);

/// id -> (ship name, SDE group name), preloaded once so the poller does no SQLite per member.
#[cfg(feature = "fc-rescue")]
pub type ShipTypeMap = Arc<std::collections::HashMap<i64, (String, String)>>;

#[cfg(feature = "fc-rescue")]
fn esi_client() -> Option<reqwest::blocking::Client> {
    crate::http::client(20)
        .ok()
}

#[cfg(feature = "fc-rescue")]
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

#[cfg(feature = "fc-rescue")]
#[derive(Deserialize)]
struct RawMember {
    character_id: i64,
    #[serde(default)]
    ship_type_id: i64,
}

#[cfg(feature = "fc-rescue")]
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
#[cfg(feature = "fc-rescue")]
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

/// Poll the FC's fleet composition while Rescue Mode is active and write it into `RescueState`.
/// Every network path degrades to keeping the previous snapshot (marked stale); it never panics.
#[cfg(feature = "fc-rescue")]
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
            // Poll whenever rescue is active, including test mode (test only disables sending, so
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
                    // In a fleet but not the boss (members endpoint 403), so try another character.
                    continue;
                };
                let ids: Vec<i64> = raw.iter().map(|m| m.character_id).collect();
                let names = crate::universe::names(&client, &ids);
                let members = raw
                    .into_iter()
                    .map(|m| {
                        let group = ship_types.get(&m.ship_type_id).map(|(_, g)| g.as_str()).unwrap_or("");
                        crate::rescue::FleetMember {
                            character_id: m.character_id,
                            name: names.get(&m.character_id).cloned().unwrap_or_default(),
                            role: crate::rescue::classify(group),
                        }
                    })
                    .collect();
                let mut snap = crate::rescue::FleetSnapshot::build(Some(fleet_id), members);
                snap.is_registered =
                    fleet_is_registered(&client, &store, &client_id, ch.id, ch.expires_at, fleet_id);
                built = Some(snap);
                break;
            }

            {
                let mut r = rescue.lock().unwrap();
                match built {
                    Some(snap) => {
                        // Update sticky snowflakes from the fresh ship types (handles re-ships), but
                        // never remove an existing one, so a podded titan pilot stays flagged.
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

/// Why a character's ESI calls stopped working, when it is not something that fixes itself.
///
/// Kept beside the one place that can tell: call sites only see `Option<String>` for the access
/// token and cannot tell "not now" from "not ever again".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthProblem {
    /// EVE SSO rejected the saved login. Only logging in again fixes it.
    LoggedOut,
    /// Neither the OS keychain nor the encrypted fallback could give the token back.
    NoKeychain,
}

impl AuthProblem {
    pub fn message(self, name: &str) -> String {
        match self {
            Self::LoggedOut => format!("{name}'s EVE login has expired. Log in again to restore location, fleet and route features."),
            Self::NoKeychain => format!("{name}'s saved login could not be read back."),
        }
    }
}

static AUTH_PROBLEMS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<i64, AuthProblem>>,
> = std::sync::LazyLock::new(Default::default);

fn note_problem(id: i64, p: AuthProblem) {
    AUTH_PROBLEMS.lock().unwrap_or_else(|e| e.into_inner()).insert(id, p);
}

fn clear_problem(id: i64) {
    AUTH_PROBLEMS.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
}

pub fn auth_problem(id: i64) -> Option<AuthProblem> {
    AUTH_PROBLEMS.lock().unwrap_or_else(|e| e.into_inner()).get(&id).copied()
}

/// Called when a character logs in again, so a fixed problem stops being reported.
pub fn forget_auth_problem(id: i64) {
    clear_problem(id);
}

/// Put a character into a failed state without an EVE login to fail. For the scene that renders the
/// banner, which is otherwise unreachable from a test.
#[cfg(test)]
pub(crate) fn set_auth_problem_for_test(id: i64, p: AuthProblem) {
    note_problem(id, p);
}

fn refresh_lock(id: i64) -> std::sync::Arc<std::sync::Mutex<()>> {
    static LOCKS: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<i64, std::sync::Arc<std::sync::Mutex<()>>>>,
    > = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    LOCKS.lock().unwrap().entry(id).or_default().clone()
}

/// Where the short-lived access token and its expiry are cached. The app's `Store` and the
/// battle-report share's own connection read the same rows and must share one refresh path: two
/// independent refreshers race the rotating refresh token and log the character out.
pub(crate) trait AccessCache {
    fn access(&self, id: i64) -> Option<String>;
    fn expiry(&self, id: i64) -> Option<i64>;
    fn put(&self, id: i64, access: &str, expires_at: i64);
}

impl AccessCache for Store {
    fn access(&self, id: i64) -> Option<String> {
        self.kv_get(&format!("access:{id}"))
    }
    fn expiry(&self, id: i64) -> Option<i64> {
        self.token_expiry(id)
    }
    fn put(&self, id: i64, access: &str, expires_at: i64) {
        self.kv_set(&format!("access:{id}"), access);
        let _ = self.update_token_expiry(id, expires_at);
    }
}

impl AccessCache for rusqlite::Connection {
    fn access(&self, id: i64) -> Option<String> {
        self.query_row("SELECT value FROM kv WHERE key = ?1", [format!("access:{id}")], |r| r.get(0))
            .ok()
    }
    fn expiry(&self, id: i64) -> Option<i64> {
        self.query_row("SELECT expires_at FROM characters WHERE id = ?1", [id], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .ok()
        .flatten()
    }
    fn put(&self, id: i64, access: &str, expires_at: i64) {
        let _ = self.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![format!("access:{id}"), access],
        );
        let _ = self.execute(
            "UPDATE characters SET expires_at = ?1 WHERE id = ?2",
            rusqlite::params![expires_at, id],
        );
    }
}

fn current_access_token(
    store: &Store,
    client_id: &str,
    id: i64,
    expires_at: i64,
) -> Option<String> {
    access_token(store, client_id, id, Some(expires_at))
}

/// `expires_hint` is the caller's possibly stale copy of the expiry, which skips a query on the
/// common path. `None` reads it from the cache.
pub(crate) fn access_token(
    store: &impl AccessCache,
    client_id: &str,
    id: i64,
    expires_hint: Option<i64>,
) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    let expires_at = expires_hint.or_else(|| store.expiry(id)).unwrap_or(0);
    // 60s margin so a token doesn't expire mid-request.
    if expires_at - 60 > now {
        if let Some(access) = store.access(id).filter(|a| !a.is_empty()) {
            return Some(access);
        }
    }

    // EVE SSO rotates the refresh token on each use, so two threads refreshing the same
    // character concurrently would invalidate each other and log it out. Serialise per
    // character, then re-check: another thread may have just refreshed while we waited.
    let lock = refresh_lock(id);
    let _guard = lock.lock().unwrap();
    let now = chrono::Utc::now().timestamp();
    if store.expiry(id).is_some_and(|exp| exp - 60 > now) {
        if let Some(access) = store.access(id).filter(|a| !a.is_empty()) {
            return Some(access);
        }
    }

    // Load the refresh token inside the lock so we pick up a rotation from another thread.
    let refresh = match tokens::try_load_refresh(id) {
        Ok(Some(r)) => r,
        // No entry at all: this character was never logged in, or its token was deleted. Same
        // remedy as a rejection, and the same thing worth saying out loud.
        Ok(None) => {
            note_problem(id, AuthProblem::LoggedOut);
            return None;
        }
        Err(e) => {
            eprintln!("keychain unavailable for character {id}: {e:#}");
            note_problem(id, AuthProblem::NoKeychain);
            return None;
        }
    };
    let fresh = match auth::refresh_access_token(client_id, &refresh) {
        Ok(f) => f,
        Err(auth::RefreshError::Rejected(msg)) => {
            eprintln!("EVE SSO rejected the saved login for character {id}: {msg}");
            note_problem(id, AuthProblem::LoggedOut);
            return None;
        }
        // Transient: say nothing and let the next call try again. A warning that appears every time
        // the network hiccups is a warning nobody reads.
        Err(_) => return None,
    };
    clear_problem(id);
    // The refresh token may rotate, so persist the new one.
    let _ = tokens::save_refresh(id, &fresh.refresh_token);
    store.put(id, &fresh.access_token, now + fresh.expires_in);
    Some(fresh.access_token)
}

#[cfg(test)]
mod tests {
    use super::{access_token, AccessCache};

    fn scratch() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE characters (id INTEGER PRIMARY KEY, name TEXT, expires_at INTEGER, scopes TEXT);
             INSERT INTO characters (id, name, expires_at, scopes) VALUES (7, 'Scratch', NULL, '');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn connection_cache_round_trips() {
        let conn = scratch();
        assert_eq!(conn.access(7), None);
        assert_eq!(conn.expiry(7), None);
        conn.put(7, "tok", 1234);
        assert_eq!(conn.access(7).as_deref(), Some("tok"));
        assert_eq!(conn.expiry(7), Some(1234));
    }

    /// Reuse needs a token that outlives the 60 s margin, read from the cache when the caller has
    /// no expiry of its own. The refresh side reaches the keychain and SSO, so it is not run here.
    #[test]
    fn fresh_cached_token_is_reused_without_refresh() {
        let conn = scratch();
        let now = chrono::Utc::now().timestamp();
        conn.put(7, "cached", now + 3600);
        assert_eq!(access_token(&conn, "client", 7, None).as_deref(), Some("cached"));
        assert_eq!(access_token(&conn, "client", 7, Some(0)).as_deref(), Some("cached"));
    }
}
