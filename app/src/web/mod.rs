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

/// What the dialog endpoints read. Pushed down from the UI thread with the rest of the facts, for
/// the same reason: the server thread has no store handle and no graph of its own.
#[derive(Default)]
pub struct DetailState {
    pub graph: Option<Arc<crate::geo::Systems>>,
    pub store: Option<crate::store::Store>,
    pub status: HashMap<i64, crate::systemstatus::SysFlags>,
    pub player_sys: Option<i64>,
    pub active_character: String,
    pub staging: Option<String>,
    /// The folder tree, for exports.
    pub notes: Arc<crate::notes::NoteBook>,
    /// Wakes the UI when the page posts an action. Otherwise an idle app window drains the inbox only
    /// on its next repaint, which made every edit from the page feel stuck.
    pub wake: Option<egui::Context>,
    pub count_bridges: bool,
    /// Type names the app has resolved, shared so the ship dialog can name the skill a hull bonus
    /// belongs to. Shared rather than copied because the request thread fills it in on a miss.
    pub type_names: Option<Arc<Mutex<std::collections::HashMap<i64, String>>>>,
    /// Systems the planner always goes around, one list per kind of route. Pushed from settings so
    /// the persistent half of avoidance stays in one place and the page only ever sends the
    /// this-route-only half.
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    /// Scanned wormhole connections as extra edges, and whether routes are allowed to use them.
    /// Both come from the app, because the app is where the chain and the setting live.
    pub holes: HashMap<i64, Vec<i64>>,
    pub via_wormholes: bool,
    /// Saved routes, already pruned of expired wormhole ones.
    pub saved_routes: Vec<crate::settings::SavedMapRoute>,
    /// Every system's real coordinates, for the light-year maths the jump routes are made of.
    pub coords: Option<Arc<Vec<crate::store::MapSystem>>>,
    /// Scanned wormholes, sov upgrades and bookmarks, for the system dialog. Copied: each is a
    /// handful of entries the user maintains by hand, and the request thread must not hold a lock
    /// the UI writes to.
    pub wh_cache: Vec<crate::wormholes::Wormhole>,
    pub sov_upgrades: Vec<crate::settings::SovUpgrade>,
    pub bookmarks: Vec<i64>,
    /// Gate camps, shared: the detection state is rebuilt from killmails on its own schedule and
    /// asking it about one system is cheap.
    pub camps: Option<Arc<Mutex<crate::camp::CampState>>>,
    /// The live jabber session, shared rather than copied: one room's backlog is larger than every
    /// other pane put together, and the pane reads one conversation at a time.
    pub jabber: Option<Arc<Mutex<crate::jabber::JabberState>>>,
}

pub type Detail = Arc<Mutex<DetailState>>;

/// Actions posted by the page, drained on the UI thread.
///
/// Deliberately the same `OverlayToMain` the overlay subprocess sends, landing in the same drain and
/// the same match arms. A second queue would be a second place for the two to disagree about what a
/// verdict means.
pub type Inbox = Arc<Mutex<Vec<crate::ipc::OverlayToMain>>>;

pub fn detail() -> Detail {
    Arc::new(Mutex::new(DetailState::default()))
}

pub fn inbox() -> Inbox {
    Arc::new(Mutex::new(Vec::new()))
}
