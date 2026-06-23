//! Persisted application settings (M0 subset — docs/DESIGN.md §7.1 E10).

use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub nav_expanded: bool,
    /// Show times in EVE time (UTC) rather than local.
    pub use_eve_time: bool,
    /// EVE chat-log directory (empty = unset / auto-detect later).
    pub eve_logs_dir: String,
    /// EVE character-settings directory (empty = unset).
    pub eve_settings_dir: String,
    /// Intel chat channels to watch.
    pub intel_channels: Vec<String>,
    /// EVE SSO application client ID (PKCE public client).
    #[serde(default = "default_client_id")]
    pub sso_client_id: String,
    /// OAuth loopback callback URL (must match the registered application).
    #[serde(default = "default_callback")]
    pub sso_callback: String,
    /// Name of the last-applied configuration pack (empty = none).
    #[serde(default)]
    pub configuration_pack: String,
    /// Configured jump bridges (system name pairs) — used for distance & battles.
    #[serde(default)]
    pub jump_bridges: Vec<JumpBridge>,
    /// Raise alerts on hostiles near the active character.
    #[serde(default = "default_true")]
    pub alert_enabled: bool,
    /// Alert when hostiles are within this many jumps of you (0 = off).
    #[serde(default = "default_alert_jumps")]
    pub alert_within_jumps: u32,
    /// Desktop alerts on combat events (under attack / scrambled) from game logs.
    #[serde(default = "default_true")]
    pub alert_combat: bool,
    /// Seconds until an intel report is considered outdated (and pruned).
    #[serde(default = "default_intel_ttl")]
    pub intel_ttl_secs: i64,
    /// Preferred online fitting site for "open fit" ("" = ask on first use).
    #[serde(default)]
    pub fit_site: String,
    /// Configured sovereignty upgrades per system (pasted from a coalition site).
    #[serde(default)]
    pub sov_upgrades: Vec<SovUpgrade>,
    /// Coalitions (member alliance names). Unlisted alliances are independent.
    #[serde(default = "default_coalitions")]
    pub coalitions: Vec<Coalition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coalition {
    pub name: String,
    /// Member alliance names (matched against the sov holder name).
    pub alliances: Vec<String>,
}

fn default_coalitions() -> Vec<Coalition> {
    // A starter Imperium roster; editable in Settings.
    vec![Coalition {
        name: "Imperium".to_owned(),
        alliances: [
            "Goonswarm Federation",
            "Tactical Narcotics Team",
            "The Bastion",
            "Get Off My Lawn",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SovUpgrade {
    pub system: String,
    pub upgrade: String,
}

fn default_intel_ttl() -> i64 {
    300
}

fn default_true() -> bool {
    true
}
fn default_alert_jumps() -> u32 {
    5
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JumpBridge {
    pub from: String,
    pub to: String,
}

fn default_client_id() -> String {
    crate::auth::DEFAULT_CLIENT_ID.to_owned()
}

fn default_callback() -> String {
    crate::auth::DEFAULT_CALLBACK.to_owned()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            nav_expanded: false,
            use_eve_time: true,
            eve_logs_dir: String::new(),
            eve_settings_dir: String::new(),
            intel_channels: Vec::new(),
            sso_client_id: default_client_id(),
            sso_callback: default_callback(),
            configuration_pack: String::new(),
            jump_bridges: Vec::new(),
            alert_enabled: true,
            alert_within_jumps: 5,
            alert_combat: true,
            intel_ttl_secs: 300,
            fit_site: String::new(),
            sov_upgrades: Vec::new(),
            coalitions: default_coalitions(),
        }
    }
}
