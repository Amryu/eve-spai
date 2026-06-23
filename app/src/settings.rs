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
    /// Characters whose chat logs are NOT used for intel (by listener name).
    #[serde(default)]
    pub intel_disabled_chars: Vec<String>,
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
    /// Only raise intel alerts while the active character is undocked.
    #[serde(default)]
    pub alert_only_undocked: bool,
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
    /// Persisted map overlay + intel-filter options (opaque JSON blob owned by the
    /// UI layer). Empty = use defaults.
    #[serde(default)]
    pub view_options: String,
    /// Sov-holding alliances seen (auto-discovered + manual), with colour overrides.
    /// Never auto-pruned when an alliance stops holding sov.
    #[serde(default)]
    pub alliances: Vec<AllianceConfig>,
    /// Intel severity rules (condition → level → card colour).
    #[serde(default = "default_severity")]
    pub severity: SeverityRules,
    /// Alerting configuration (rules, sounds, custom window, push).
    #[serde(default = "default_alerts")]
    pub alerts: AlertSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coalition {
    pub name: String,
    /// Member alliance names (matched against the sov holder name).
    pub alliances: Vec<String>,
    /// Map colour override; None = auto-generated from the name.
    #[serde(default)]
    pub color: Option<(u8, u8, u8)>,
}

/// One alert rule (condition chain → actions). Rules are evaluated top-first; the
/// first enabled rule whose conditions all match decides the actions (or suppresses
/// the alert). If no rule matches, the default actions apply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    pub enabled: bool,
    // --- conditions (a set/empty field means "don't care") ---
    pub min_severity: Severity,
    /// Specific systems by name (empty = any).
    pub systems: Vec<String>,
    /// Within this many jumps of an alerting character (None = any distance).
    pub max_jumps: Option<u32>,
    /// At least this many hostiles (None = any).
    pub min_count: Option<u32>,
    /// Required condition tags (any of): bubble/camp/cyno/kill/ess/wormhole/spike/threat.
    pub require: Vec<String>,
    /// Only apply for these characters (by name); empty = any enabled character.
    #[serde(default)]
    pub characters: Vec<String>,
    // --- actions ---
    /// Suppress the alert entirely (takes precedence over the action toggles).
    pub suppress: bool,
    pub system_notification: bool,
    pub custom_window: bool,
    pub push: bool,
    /// Sound override: "" = the severity default, "off" = silent, else preset/path.
    pub sound: String,
    pub cooldown_secs: i64,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            name: "New rule".to_owned(),
            enabled: true,
            min_severity: Severity::Warning,
            systems: Vec::new(),
            max_jumps: None,
            min_count: None,
            require: Vec::new(),
            characters: Vec::new(),
            suppress: false,
            system_notification: true,
            custom_window: true,
            push: false,
            sound: String::new(),
            cooldown_secs: 60,
        }
    }
}

/// Custom alert window "always on top" behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnTop {
    /// Always above other windows.
    Always,
    /// Above only while the EVE client is the focused window.
    Smart,
    /// Never forced on top.
    Never,
}

fn default_on_top() -> OnTop {
    OnTop::Always
}

/// Intel alerting configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertSettings {
    /// Severities at or above this alert by default (default: Warning).
    pub min_severity: Severity,
    /// Per-severity default sound preset/path: [Info, Warning, Danger, Critical].
    pub sounds: Vec<String>,
    pub system_notifications: bool,
    pub use_custom_window: bool,
    /// Custom-window top-left position (screen pixels); None = auto.
    pub window_pos: Option<(f32, f32)>,
    /// Seconds the custom window stays after the last alert.
    pub window_timeout: f32,
    /// Always-on-top behaviour for the custom window.
    #[serde(default = "default_on_top")]
    pub on_top: OnTop,
    pub push_enabled: bool,
    pub pushover_token: String,
    pub pushover_user: String,
    /// Ordered rules (top = highest precedence).
    pub rules: Vec<AlertRule>,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            min_severity: Severity::Warning,
            sounds: vec![
                "off".to_owned(),      // Info
                "warning".to_owned(),  // Warning
                "danger".to_owned(),   // Danger
                "critical".to_owned(), // Critical
            ],
            system_notifications: true,
            use_custom_window: false,
            window_pos: None,
            window_timeout: 30.0,
            on_top: OnTop::Always,
            push_enabled: false,
            pushover_token: String::new(),
            pushover_user: String::new(),
            rules: Vec::new(),
        }
    }
}

fn default_alerts() -> AlertSettings {
    AlertSettings::default()
}

/// Intel severity levels (drive the card colour, lowest → highest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Danger,
    Critical,
}

/// Configurable mapping of intel conditions → severity level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeverityRules {
    /// A "gang" this size or larger is treated as `big_gang`.
    pub big_gang_threshold: u32,
    pub small_gang: Severity,
    pub big_gang: Severity,
    pub bubble: Severity,
    pub gate_camp: Severity,
    pub spike: Severity,
    pub cyno: Severity,
    pub kill: Severity,
    pub no_visual: Severity,
    pub wormhole: Severity,
    pub ess: Severity,
    /// High-threat hull names (matched against reported ships).
    pub threat_ships: Vec<String>,
    pub threat_ship: Severity,
}

impl Default for SeverityRules {
    fn default() -> Self {
        use Severity::*;
        Self {
            big_gang_threshold: 5,
            small_gang: Warning,
            big_gang: Danger,
            bubble: Danger,
            gate_camp: Danger,
            spike: Danger,
            cyno: Critical,
            kill: Danger,
            no_visual: Warning,
            wormhole: Warning,
            ess: Warning,
            threat_ships: ["Kikimora", "Cenotaph"].iter().map(|s| s.to_string()).collect(),
            threat_ship: Danger,
        }
    }
}

fn default_severity() -> SeverityRules {
    SeverityRules::default()
}

/// A sov-holding alliance the app has seen (auto-discovered from ESI or added by
/// hand), with an optional map-colour override.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AllianceConfig {
    pub name: String,
    #[serde(default)]
    pub color: Option<(u8, u8, u8)>,
}

pub fn default_coalitions() -> Vec<Coalition> {
    // Best-effort snapshot of the major null-sec coalitions; the political map
    // shifts often, so edit/reset in Settings to keep it current. Alliance names
    // must match the sov holder name exactly (some end with a period).
    let coal = |name: &str, members: &[&str]| Coalition {
        name: name.to_owned(),
        alliances: members.iter().map(|s| s.to_string()).collect(),
        color: None,
    };
    vec![
        coal(
            "The Imperium",
            &["Goonswarm Federation", "Tactical Narcotics Team", "The Bastion", "Get Off My Lawn"],
        ),
        coal(
            "Winter Coalition",
            &["Fraternity.", "Northern Coalition.", "Solyaris Chtonium"],
        ),
        coal("The Initiative", &["The Initiative.", "Initiative Mercenaries"]),
        coal("PanFam", &["Pandemic Legion", "Pandemic Horde"]),
    ]
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
            intel_disabled_chars: Vec::new(),
            sso_client_id: default_client_id(),
            sso_callback: default_callback(),
            configuration_pack: String::new(),
            jump_bridges: Vec::new(),
            alert_enabled: true,
            alert_within_jumps: 5,
            alert_combat: true,
            alert_only_undocked: false,
            intel_ttl_secs: 300,
            fit_site: String::new(),
            sov_upgrades: Vec::new(),
            coalitions: default_coalitions(),
            view_options: String::new(),
            alliances: Vec::new(),
            severity: SeverityRules::default(),
            alerts: AlertSettings::default(),
        }
    }
}
