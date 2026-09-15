//! What the page is sent.
//!
//! One `Option` per pane: a pane is present only when it changed after the client's last `seq`, and
//! carries the `rev` it last changed at, so a reconnecting client gets exactly what it missed.

// `BTreeMap`, never `HashMap`: the publisher hashes serialized panes to detect changes, and a
// `HashMap` serializes in a per-instance random order.
// See `state::a_hashmap_would_not_have_hashed_stably`.
use std::collections::BTreeMap;

use serde::Serialize;

use crate::intel::IntelReport;
use crate::settings::Severity;

/// One rendered intel card: the report plus everything `intel_row` reads that is not on the report.
#[derive(Clone, Debug, Serialize)]
pub struct IntelCard {
    pub report: IntelReport,
    pub severity: Severity,
    pub from_you: Option<u32>,
    pub via: crate::app::JumpVia,
    pub chars: crate::app::CardChars,
}

/// Lookup tables the cards index into, shared because a pilot, hull or alliance appears in many
/// cards.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Lookups {
    pub resolved_pilots: BTreeMap<String, i64>,
    pub uncertain: crate::pilot::UncertainPilots,
    pub last_ship: BTreeMap<String, (i64, String, i64)>,
    pub kills: BTreeMap<i64, crate::kills::KillInfo>,
    pub affil: BTreeMap<i64, crate::affiliation::Affil>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// One system's ESI-sourced state, compact because there are thousands of systems. `SysFlags`
/// whole is about 190 bytes each. Sovereignty arrives as a resolved colour.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SysInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adm: Option<f64>,
    /// Ship kills, pod kills, NPC kills and jumps in the last hour.
    #[serde(skip_serializing_if = "is_zero")]
    pub k: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub p: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub n: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub j: u32,
    /// Sovereignty colour by alliance, then by coalition, resolved by the app's rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sov: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coal: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inc: bool,
}

/// Every system ESI reports on. Its own pane because it changes on ESI's cadence, minutes apart,
/// while intel changes constantly.
#[derive(Clone, Debug, Serialize)]
pub struct StatusPane {
    pub rev: u64,
    pub systems: BTreeMap<i64, SysInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IntelPane {
    pub rev: u64,
    pub cards: Vec<IntelCard>,
    pub lookups: Lookups,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlertPane {
    pub rev: u64,
    /// The overlay's own DTO, reused so the page and the overlay window cannot diverge.
    pub msg: crate::ipc::AlertMsg,
}

#[derive(Clone, Debug, Serialize)]
pub struct PingCard {
    pub ping: crate::pings::Ping,
    /// The matched rule, sent so the browser does not re-derive it.
    pub rule: Option<String>,
    pub suppressed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PingPane {
    pub rev: u64,
    pub pings: Vec<PingCard>,
    /// Names for formup system ids, since the page has no SDE.
    pub systems: BTreeMap<i64, String>,
}

/// The map's live layer. Geometry comes from `/api/map/geometry` instead.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MapLive {
    pub rev: u64,
    pub you: Option<i64>,
    /// System id and how many of your characters are in it.
    pub chars: Vec<(i64, u32)>,
    /// System id, worst severity seen, newest report timestamp. Bounded by `intel_ttl_secs`.
    pub intel: Vec<(i64, u8, i64)>,
    pub camps: Vec<i64>,
    /// Scanned wormhole connections, as system pairs. Both ends known only.
    pub holes: Vec<(i64, i64)>,
    pub cyno: Vec<i64>,
    /// The current travel route, in order.
    pub route: Vec<i64>,
    pub upgrades: Vec<(i64, Vec<UpgradeMark>)>,
}

/// One sov upgrade: kind, level, and for mining the ore, so the page shows the app's icon.
#[derive(Clone, Debug, Serialize)]
pub struct UpgradeMark {
    /// 0 ratting, 1 exploration, 2 mining, 3 other. Matches `app::UpgradeKind`.
    pub k: u8,
    pub l: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ore: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Meta {
    pub rev: u64,
    pub version: &'static str,
    pub theme: crate::theme::Theme,
    pub compact: bool,
    pub intel_ttl_secs: i64,
    pub intel_max_jumps: u32,
    pub count_bridges: bool,
    pub allow_writeback: bool,
    pub active_character: String,
    pub chars: Vec<(String, i64)>,
    pub player_system: Option<i64>,
    /// Severity name to configured sound name.
    pub sounds: BTreeMap<String, String>,
    /// Bumped when the synthesis changes, so a browser does not keep an immutable WAV past a tone
    /// change.
    pub sound_rev: u32,
    /// Permanent route-avoidance lists, for the map marks and "stop avoiding".
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    /// The build has rescue mode and the user switched it on.
    pub rescue: bool,
}

/// Notes and tags. The view is what cards and the map draw; the book is for the manager, cheap to
/// send because it changes only on edits.
#[derive(Clone, Debug, Serialize)]
pub struct NotesPane {
    pub rev: u64,
    pub view: crate::notes::NotesView,
    pub book: crate::notes::NoteBook,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub seq: u64,
    /// Changes on every app restart. A client that sees a new one discards its state, so a stale
    /// `seq` from a previous run cannot silence a pane.
    pub gen: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intel: Option<IntelPane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alerts: Option<AlertPane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pings: Option<PingPane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapLive>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusPane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jabber: Option<crate::web::jabber::JabberPane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rescue: Option<crate::web::rescue::RescuePane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<NotesPane>,
}
