//! The GSF fleet dashboard mirror: tracking a fleet, requesting pings, and the composition of one
//! that is up.
//!
//! No egui: the UI owns the shared state and the threads; this module only answers questions.
//! `http.rs` and `creds.rs` are the deliberate exceptions to the rest of that rule, being the one
//! place a request leaves the machine and the one place the session is stored.
//!
//! Every write is turned into a `CallRecord` describing the request that would go out. The spoof
//! stops there; `HttpBackend` sends it once the user has turned writes on.

// The wire model and the call builders land ahead of the UI that calls them, so most of this module
// is unreferenced until the tab is wired up. Drop this once it is.
#![allow(dead_code)]

pub mod backend;
pub mod boosts;
pub mod checks;
pub mod comms;
pub mod config;
pub mod creds;
pub mod logi;
pub mod login;
pub mod doctrine;
pub mod hub;
pub mod http;
pub mod model;
pub mod ping;
pub mod seed;
pub mod spoof;
pub mod state;

pub use state::FleetState;

/// Where the reference data is read from, for the one line settings shows about it.
pub fn seed_path_hint() -> String {
    match std::env::var("EVE_SPAI_FLEET_SEED") {
        Ok(p) if !p.trim().is_empty() => p,
        _ => crate::store::data_dir()
            .map(|d| d.join("fleet-seed.json").display().to_string())
            .unwrap_or_else(|_| "fleet-seed.json in the profile directory".to_owned()),
    }
}

/// A pasted `name=value; name=value` session, for driving the real service before the embedded
/// browser exists. Read once at startup; the keychain is the normal source.
pub const COOKIE_ENV: &str = "EVE_SPAI_FLEET_COOKIES";

/// Which backend the tab talks to.
///
/// The headless arm is first and unconditional: no test and no screenshot may ever get an
/// `HttpBackend`, because the keychain is outside the `EVE_SPAI_DATA_DIR` redirect the uitest
/// guard applies.
pub fn choose_backend(
    headless: bool,
    settings: &crate::settings::Settings,
) -> std::sync::Arc<dyn backend::FleetBackend> {
    if headless {
        return std::sync::Arc::new(spoof::SpoofBackend::instant());
    }
    match live_backend(settings) {
        Some(b) => b,
        None => std::sync::Arc::new(spoof::SpoofBackend::seeded()),
    }
}

/// The signed-in backend, when there is a session to sign in with and the user has turned it on.
pub fn live_backend(
    settings: &crate::settings::Settings,
) -> Option<std::sync::Arc<dyn backend::FleetBackend>> {
    if !settings.fleet_live {
        return None;
    }
    let text = std::env::var(COOKIE_ENV)
        .ok()
        .filter(|t| !t.trim().is_empty())
        .or_else(creds::load)?;
    let mode = if settings.fleet_send_writes {
        backend::Mode::Live
    } else {
        backend::Mode::ReadOnly
    };
    let b = http::HttpBackend::new(
        http::Cookies::restore(&text),
        // The character picked in the top bar, as the session's default FC. The start form's FC
        // picker overrides it per fleet, which is why there is no separate setting for this.
        settings.active_character.clone(),
        mode,
        seed::load(),
        settings.sso_client_id.clone(),
    )
    .ok()?;
    Some(std::sync::Arc::new(b))
}
