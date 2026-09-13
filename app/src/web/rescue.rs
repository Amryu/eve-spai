//! The rescue pane: what the FC-only capital rescue mode is looking at.
//!
//! Read-only. The desktop's rescue window sends pings and invites people into comms; none of that is
//! offered here, because a socket on the LAN should not be able to broadcast to an alliance.
//!
//! The types are unconditional and the filling is not: a build without `fc-rescue` simply never
//! publishes the pane, which keeps `cfg` out of the snapshot and out of the page.

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct RescuePing {
    pub seq: u64,
    pub at: i64,
    pub author: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pilot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cyno: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anomaly: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Whether this is the ping the FC is working.
    pub selected: bool,
}

/// How far the staging is from the capital, which is the whole question the mode exists to answer.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct RescueRange {
    pub ly: f64,
    pub closest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansiblex_jumps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_jumps: Option<u32>,
    pub ly_to_target: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct RescueSide {
    pub active: bool,
    pub test_mode: bool,
    pub doctrine: String,
    pub op_channel: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capital_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capital_pilot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<RescueRange>,
    pub pings: Vec<RescuePing>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RescuePane {
    pub rev: u64,
    #[serde(flatten)]
    pub side: RescueSide,
}
