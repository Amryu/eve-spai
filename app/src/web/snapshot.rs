//! What the page is sent.
//!
//! One `Option` per pane, and `None` is the dirty flag: a pane is present only when it changed after
//! the client's last `seq`. Each pane carries the `rev` it last changed at, so a reconnecting client
//! asks once and gets exactly what it missed. No patch format, because a patch format is a second
//! thing to get subtly wrong.

use std::collections::HashMap;

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
    pub resolved_pilots: HashMap<String, i64>,
    pub uncertain: crate::pilot::UncertainPilots,
    pub last_ship: HashMap<String, (i64, String, i64)>,
    pub kills: HashMap<i64, crate::kills::KillInfo>,
    pub affil: HashMap<i64, crate::affiliation::Affil>,
    pub status: HashMap<i64, crate::systemstatus::SysFlags>,
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
    pub meta: Option<Meta>,
}
