//! What the page is sent.
//!
//! One `Option` per pane, and `None` is the dirty flag: a pane is present only when it changed after
//! the client's last `seq`. Each pane carries the `rev` it last changed at, so a reconnecting client
//! asks once and gets exactly what it missed. No patch format, because a patch format is a second
//! thing to get subtly wrong.

use std::collections::BTreeMap;

/// Ordered, not hashed, throughout this module.
///
/// The publisher rebuilds these every tick and hashes the result to decide whether anything
/// changed. A `HashMap` serializes in iteration order and every instance gets its own seed, so two
/// identical maps hashed differently and every pane republished every tick. See
/// `state::a_hashmap_would_not_have_hashed_stably`.

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

/// The lookup tables a pane's cards index into. Shared rather than inlined per card, because a
/// pilot, a hull or an alliance appears in many cards and a phone pays for every duplicated byte.
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

/// One system's ESI-sourced state, in the shortest form that still says everything the map draws.
///
/// `SysFlags` serialized whole was 190 bytes a system across 5382 systems, once inside the intel
/// pane and again inside the alert pane: over 2 MB of a 2.86 MB snapshot, re-sent whenever either
/// changed. Everything optional is skipped, and sovereignty arrives as a resolved colour so the page
/// never has to know an alliance exists.
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
    /// Sovereignty colour by alliance, then by coalition. Both resolved the way the app resolves
    /// them, so the two maps agree without the page knowing the rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sov: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coal: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inc: bool,
}

/// Shared reference data: every system ESI has anything to say about.
///
/// Its own pane because it moves on ESI's cadence, minutes apart, while intel moves constantly.
/// Inside the intel pane it was re-sent on every report.
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
    /// The overlay's own DTO, verbatim. It already carries the feed and every lookup a card needs,
    /// it is pinned by compatibility tests, and reusing it means the page and the overlay window
    /// cannot end up showing different things.
    pub msg: crate::ipc::AlertMsg,
}

#[derive(Clone, Debug, Serialize)]
pub struct PingCard {
    pub ping: crate::pings::Ping,
    /// The rule that matched, if any. It is the only thing separating a ping that concerns you from
    /// one that does not, so it travels with the ping rather than being re-derived in the browser.
    pub rule: Option<String>,
    pub suppressed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PingPane {
    pub rev: u64,
    pub pings: Vec<PingCard>,
    /// Names for the system ids a formup points at. `Formup::System` carries an id and nothing else,
    /// and the page has no SDE, so without this a formup reads "30004759" instead of "1DQ1-A".
    pub systems: BTreeMap<i64, String>,
}

/// The map's live layer. Geometry never travels here: it comes from `/api/map/geometry`, cached hard
/// against the SDE version, because re-sending 8000 nodes twice a second would be absurd.
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
    /// Configured cyno generators.
    pub cyno: Vec<i64>,
    /// The current travel route, in order. The page draws a leg solid when the two systems are gate
    /// neighbours and dashed otherwise, which it can tell from the geometry it already has.
    pub route: Vec<i64>,
    /// Sov upgrades per system, classified the way the app classifies them.
    pub upgrades: Vec<(i64, Vec<UpgradeMark>)>,
}

/// One sov upgrade, reduced to what a map draws: what kind it is, what level, and for a mining
/// upgrade which ore, so the page can show the same icon the app does.
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
    /// Severity name to sound name, so the page asks for the sound the user configured rather than
    /// guessing one from the severity.
    pub sounds: BTreeMap<String, String>,
    /// Bumped when the synthesis changes, so a browser cannot keep an immutable WAV past a tone
    /// change.
    pub sound_rev: u32,
    /// The permanent route-avoidance lists, so the page can say "stop avoiding" only where there is
    /// something to stop, and mark them on the map.
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    /// Whether this build has the rescue mode and the user has it switched on. The pane only exists
    /// when both are true, and there is no point offering it otherwise.
    pub rescue: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub seq: u64,
    /// Changes every time the app restarts. A client that sees a new one throws away what it has,
    /// which is what stops a stale `seq` from a previous run silencing a whole pane.
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
}
