//! The remote web view: an opt-in local server mirroring the intel feed, alerts, fleet pings and the
//! map to a browser on the same network.
//!
//! Everything web-facing lives under this module. `app.rs` is 27k lines and is the path of least
//! resistance for all of it, so the whole feature is allowed exactly four edits there: the `UiFacts`
//! field and its one call, the `if !headless` start hook, one arm in the existing `OverlayToMain`
//! drain, and the settings pane. See `ui-tickets/README.md`.

pub mod assets;
pub mod auth;
pub mod css;
pub mod detail;
#[cfg(test)]
pub mod demo;
pub mod icons;
pub mod map;
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
    pub count_bridges: bool,
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
