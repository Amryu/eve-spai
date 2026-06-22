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
}

pub type SharedPlayer = Arc<Mutex<Player>>;

pub fn spawn_location_poller(client_id: String, player: SharedPlayer, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent("eve-spai/0.1 (EVE intel tool)")
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        loop {
            std::thread::sleep(POLL);
            let name = player.lock().unwrap().active_name.clone();
            if name.is_empty() || name == "No character" {
                continue;
            }
            let Ok(store) = Store::open() else { continue };
            if let Some(sys) = location_for(&client, &store, &client_id, &name) {
                let mut p = player.lock().unwrap();
                if p.system_id != Some(sys) {
                    p.system_id = Some(sys);
                    ctx.request_repaint();
                }
            }
        }
    });
}

fn location_for(
    client: &reqwest::blocking::Client,
    store: &Store,
    client_id: &str,
    name: &str,
) -> Option<i64> {
    let character = store.character_by_name(name)?;
    let token = current_access_token(store, client_id, character.id, character.expires_at)?;

    #[derive(Deserialize)]
    struct Location {
        solar_system_id: i64,
    }
    let url = format!("{LOCATION_URL}/{}/location/", character.id);
    let loc: Location = client.get(url).bearer_auth(token).send().ok()?.json().ok()?;
    Some(loc.solar_system_id)
}

/// Return a valid access token, refreshing via the keychain refresh token if the
/// stored one is within a minute of expiry.
fn current_access_token(
    store: &Store,
    client_id: &str,
    id: i64,
    expires_at: i64,
) -> Option<String> {
    let stored = tokens::load(id)?;
    let now = chrono::Utc::now().timestamp();
    if expires_at - 60 > now {
        return Some(stored.access_token);
    }

    let fresh = auth::refresh_access_token(client_id, &stored.refresh_token).ok()?;
    let _ = tokens::save(
        id,
        &tokens::Tokens {
            refresh_token: fresh.refresh_token.clone(),
            access_token: fresh.access_token.clone(),
        },
    );
    let _ = store.update_token_expiry(id, now + fresh.expires_in);
    Some(fresh.access_token)
}
