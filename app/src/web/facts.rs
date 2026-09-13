//! What the publisher thread cannot work out for itself.
//!
//! Most of a rendered card is derivable off the UI thread, which is why the alert daemon can already
//! build one while the main window is minimized. The rest is UI-thread state: which characters are
//! authenticated, which are disabled, the active one, the current theme. `AlertEngine::config` solves
//! the same problem for the alert daemon by having the UI push a snapshot down; this is that, for the
//! web view, kept separate so an optional feature never sits on the alert path.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct UiFacts {
    /// Whether the feature is on at all. The publisher thread is spawned unconditionally, so this is
    /// what stops it doing a feed's worth of work twice a second for the large majority of users who
    /// never turn the web view on.
    pub web_enabled: bool,
    pub systems: Option<Arc<crate::geo::Systems>>,
    /// Name and id per authenticated character. The publisher has no store handle, and the page
    /// needs the ids to draw portraits.
    pub chars: Vec<(String, i64)>,
    pub active_character: String,
    pub disabled: Vec<String>,
    pub only_undocked: bool,
    pub count_bridges: bool,
    pub intel_max_jumps: u32,
    pub intel_ttl_secs: i64,
    pub severity: crate::settings::SeverityRules,
    pub ping_rules: Vec<crate::settings::PingRule>,
    pub alert_enabled: bool,
    pub compact: bool,
    pub theme: crate::theme::Theme,
    pub allow_writeback: bool,
    /// Per-severity sound names, indexed the way `AlertSettings::sounds` is. WEB-002's ticket listed
    /// this and it landed here instead, with the ticket that needed it.
    pub sounds: Vec<String>,
    /// Map overlays the UI thread already has to hand. Pushed down rather than re-derived, because
    /// the publisher has neither the camp state, the wormhole table nor the settings.
    pub camps: Vec<i64>,
    pub holes: Vec<(i64, i64)>,
    pub cyno: Vec<i64>,
    pub route: Vec<i64>,
    /// Per system, its sov upgrades as (kind, level, ore type id).
    pub upgrades: Vec<(i64, Vec<(u8, u8, Option<i64>)>)>,
    /// Alliance name to colour, and to its coalition's colour, as the app resolves them.
    pub sov_colors: std::collections::HashMap<String, String>,
    pub coal_colors: std::collections::HashMap<String, String>,
    /// The Convos list, ordered by the app's own rules. Built on the UI thread because the rules
    /// read settings the publisher has no handle on: contacts, closed conversations, forgotten ones.
    pub jabber: crate::web::jabber::JabberSide,
    /// Permanently avoided systems, one list per kind of route.
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    /// Filled only by an `fc-rescue` build with the mode switched on.
    pub rescue: Option<crate::web::rescue::RescueSide>,
}

pub type SharedFacts = Arc<Mutex<UiFacts>>;

pub fn shared() -> SharedFacts {
    Arc::new(Mutex::new(UiFacts::default()))
}
