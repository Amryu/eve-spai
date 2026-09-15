//! UI-thread state the publisher cannot derive (characters, the active one, the theme), pushed down
//! like `AlertEngine::config` but kept separate so an optional feature stays off the alert path.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct UiFacts {
    /// The publisher thread is spawned unconditionally, so this keeps it idle when the feature is off.
    pub web_enabled: bool,
    pub systems: Option<Arc<crate::geo::Systems>>,
    /// Name and id per authenticated character. The page needs ids for portraits.
    pub chars: Vec<(String, i64)>,
    pub active_character: String,
    pub disabled: Vec<String>,
    pub only_undocked: bool,
    pub count_bridges: bool,
    pub staging: Option<String>,
    pub notes: Arc<crate::notes::NoteBook>,
    pub notes_view: Arc<crate::notes::NotesView>,
    pub intel_max_jumps: u32,
    pub intel_ttl_secs: i64,
    pub severity: crate::settings::SeverityRules,
    pub ping_rules: Vec<crate::settings::PingRule>,
    pub compact: bool,
    pub theme: crate::theme::Theme,
    pub allow_writeback: bool,
    /// Per-severity sound names, indexed like `AlertSettings::sounds`.
    pub sounds: Vec<String>,
    /// Pushed down because the publisher has no camp state, wormhole table or settings.
    pub camps: Vec<i64>,
    pub holes: Vec<(i64, i64)>,
    pub cyno: Vec<i64>,
    pub route: Vec<i64>,
    /// (kind, level, ore type id) per system.
    pub upgrades: Vec<(i64, Vec<(u8, u8, Option<i64>)>)>,
    /// Alliance name to colour, and to its coalition's colour, as the app resolves them.
    pub sov_colors: std::collections::HashMap<String, String>,
    pub coal_colors: std::collections::HashMap<String, String>,
    /// Built on the UI thread because the ordering rules read settings the publisher cannot reach.
    pub jabber: crate::web::jabber::JabberSide,
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    /// Filled only by an `fc-rescue` build with the mode switched on.
    pub rescue: Option<crate::web::rescue::RescueSide>,
}

pub type SharedFacts = Arc<Mutex<UiFacts>>;

pub fn shared() -> SharedFacts {
    Arc::new(Mutex::new(UiFacts::default()))
}
