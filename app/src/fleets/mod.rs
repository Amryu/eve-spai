//! The GSF fleet dashboard mirror: tracking a fleet, requesting pings, and the composition of one
//! that is up.
//!
//! Pure logic, the same contract `rescue.rs` keeps: no egui, no reqwest, no locks held inside. The
//! UI owns the shared state and the threads; this module only answers questions.
//!
//! Version 1 is a dry run. Every write is turned into a `CallRecord` describing the request that
//! would go out, and the spoof stops there. Nothing in this module opens a socket.

// The wire model and the call builders land ahead of the UI that calls them, so most of this module
// is unreferenced until the tab is wired up. Drop this once it is.
#![allow(dead_code)]

pub mod backend;
pub mod model;
pub mod state;

pub use state::{FleetState, Page};

/// Where the reference data is read from, for the one line settings shows about it.
pub fn seed_path_hint() -> String {
    match std::env::var("EVE_SPAI_FLEET_SEED") {
        Ok(p) if !p.trim().is_empty() => p,
        _ => crate::store::data_dir()
            .map(|d| d.join("fleet-seed.json").display().to_string())
            .unwrap_or_else(|_| "fleet-seed.json in the profile directory".to_owned()),
    }
}
