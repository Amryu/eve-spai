//! The remote web view: an opt-in local server mirroring the intel feed, alerts, fleet pings and the
//! map to a browser on the same network.
//!
//! Web-facing code lives under this module. The app hands it state through `UiFacts` and
//! `DetailState`, and takes the page's writes back through the shared `OverlayToMain` drain.

pub mod assets;
pub mod auth;
pub mod css;
pub mod detail;
#[cfg(test)]
pub mod demo;
pub mod icons;
pub mod jabber;
pub mod map;
pub mod rescue;
pub mod route;
pub mod routes;
pub mod server;
pub mod sse;
pub mod facts;
pub mod publish;
pub mod snapshot;
pub mod state;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// What the dialog endpoints read, pushed down from the UI thread like the facts.
#[derive(Default)]
pub struct DetailState {
    pub graph: Option<Arc<crate::geo::Systems>>,
    pub store: Option<crate::store::Store>,
    pub status: HashMap<i64, crate::systemstatus::SysFlags>,
    pub player_sys: Option<i64>,
    pub active_character: String,
    pub staging: Option<String>,
    /// For exports.
    pub notes: Arc<crate::notes::NoteBook>,
    /// Wakes the UI when the page posts an action, or an idle window drains the inbox only on its
    /// next repaint.
    pub wake: Option<egui::Context>,
    pub count_bridges: bool,
    /// Shared rather than copied because the request thread fills it in on a miss.
    pub type_names: Option<Arc<Mutex<std::collections::HashMap<i64, String>>>>,
    /// Persistent avoidance from settings. The page only sends the per-route half.
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    /// Scanned wormhole connections as extra edges.
    pub holes: HashMap<i64, Vec<i64>>,
    pub via_wormholes: bool,
    /// Saved routes, already pruned of expired wormhole ones.
    pub saved_routes: Vec<crate::settings::SavedMapRoute>,
    /// Real coordinates, for jump-route light-year maths.
    pub coords: Option<Arc<Vec<crate::store::MapSystem>>>,
    /// Copied: each is a handful of entries, and the request thread must not hold a lock the UI
    /// writes to.
    pub wh_cache: Vec<crate::wormholes::Wormhole>,
    pub sov_upgrades: Vec<crate::settings::SovUpgrade>,
    pub bookmarks: Vec<i64>,
    /// Shared: rebuilt on its own schedule, and a one-system query is cheap.
    pub camps: Option<Arc<Mutex<crate::camp::CampState>>>,
    /// Shared rather than copied: one room's backlog outweighs every other pane.
    pub jabber: Option<Arc<Mutex<crate::jabber::JabberState>>>,
}

pub type Detail = Arc<Mutex<DetailState>>;

/// Actions posted by the page, drained on the UI thread through the same `OverlayToMain` match arms
/// as the overlay subprocess, so the two cannot disagree.
pub type Inbox = Arc<Mutex<Vec<crate::ipc::OverlayToMain>>>;

pub fn detail() -> Detail {
    Arc::new(Mutex::new(DetailState::default()))
}

pub fn inbox() -> Inbox {
    Arc::new(Mutex::new(Vec::new()))
}
