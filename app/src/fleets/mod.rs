//! Fleet command: tracking a fleet, requesting pings, and the composition of one that is up.
//!
//! No egui: the UI owns the shared state and the threads; this module only answers questions.
//! `esi_backend.rs` is the one place a request leaves the machine, and it only ever goes to ESI.
//!
//! Every write is turned into a `CallRecord` describing the request, which the journal shows.

// The wire model and the call builders land ahead of the UI that calls them, so most of this module
// is unreferenced until the tab is wired up. Drop this once it is.
#![allow(dead_code)]

pub mod backend;
pub mod boosts;
pub mod checks;
pub mod comms;
pub mod config;
pub mod logi;
pub mod doctrine;
pub mod esi_backend;
pub mod model;
pub mod motd;
pub mod movement;
pub mod br;
pub mod tracker;
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

/// Which backend the tab talks to.
///
/// The headless arm is first and unconditional: no test and no screenshot may ever get the ESI
/// backend, because it reads the characters and writes the records of a real profile.
pub fn choose_backend(
    headless: bool,
    settings: &crate::settings::Settings,
) -> std::sync::Arc<dyn backend::FleetBackend> {
    if headless {
        return std::sync::Arc::new(spoof::SpoofBackend::instant());
    }
    live_backend(settings)
}

/// The backend the app runs: ESI through the stored characters' own tokens, records on disk.
pub fn live_backend(settings: &crate::settings::Settings) -> std::sync::Arc<dyn backend::FleetBackend> {
    std::sync::Arc::new(esi_backend::EsiBackend::new(settings.active_character.clone(), settings.sso_client_id.clone()))
}
