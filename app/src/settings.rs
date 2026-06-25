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
    /// Show zKill killmails within `kill_intel_jumps` jumps of you as intel cards.
    #[serde(default = "default_true")]
    pub kill_intel: bool,
    #[serde(default = "default_kill_jumps")]
    pub kill_intel_jumps: u32,
    /// Seconds until an intel report is considered outdated (and pruned).
    #[serde(default = "default_intel_ttl")]
    pub intel_ttl_secs: i64,
    /// Preferred online fitting site for "open fit" ("" = ask on first use).
    #[serde(default)]
    pub fit_site: String,
    /// URL/path opened by the "Doctrines" link on a fleet ping ("" = hide it). May be a
    /// file:/// link to an offline doctrine page.
    #[serde(default)]
    pub doctrine_url: String,
    /// Pop a foreground window (grabbing focus) on a fleet ping, in addition to the desktop
    /// notification.
    #[serde(default)]
    pub fleet_ping_window: bool,
    /// Auto-write the planned route into EVE (set destination hop-by-hop) while Live Mode is on.
    #[serde(default = "default_true")]
    pub travel_auto_dest: bool,
    /// User-saved Travel routes.
    #[serde(default)]
    pub saved_routes: Vec<SavedRoute>,
    /// Folder names for organising saved routes (also covers empty folders).
    #[serde(default)]
    pub route_folders: Vec<String>,
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
    /// Map overlay-mode window opacity (0.3–1.0).
    #[serde(default = "default_overlay_opacity")]
    pub map_overlay_opacity: f32,
    /// Overlay mode follows "smart" on-top (above only while EVE is focused).
    #[serde(default)]
    pub map_overlay_smart: bool,
    /// Connect to Jabber (XMPP) for fleet pings.
    #[serde(default)]
    pub jabber_enabled: bool,
    /// Jabber bare JID, e.g. "MyCharacter@goonfleet.com" (password in the keychain).
    #[serde(default)]
    pub jabber_jid: String,
    /// XMPP server host to connect to directly (the JID domain often doesn't have an
    /// SRV record). Empty = resolve from the JID domain.
    #[serde(default = "default_jabber_server")]
    pub jabber_server: String,
    /// Persisted MUC rooms to auto-(re)join on connect (bare room JIDs).
    #[serde(default)]
    pub jabber_rooms: Vec<String>,
    /// MUC conference host, so a room can be joined by local part only ("scouts").
    /// Empty = derive `conference.<jid domain>`.
    #[serde(default)]
    pub jabber_muc_domain: String,
    /// Muted conversations/feeds: key (bare JID, room JID, or "pings") → unmute unix
    /// time (i64::MAX = muted until manually unmuted). Muted = no sound, no badge.
    #[serde(default)]
    pub jabber_muted: std::collections::BTreeMap<String, i64>,
    /// Sound preset/path for a normal incoming jabber message.
    #[serde(default = "default_msg_sound")]
    pub jabber_msg_sound: String,
    /// Sound preset/path for a fleet ping.
    #[serde(default = "default_ping_sound")]
    pub jabber_ping_sound: String,
    /// Master switch for jabber notification sounds.
    #[serde(default = "default_true")]
    pub jabber_sound_enabled: bool,
    /// The user's private contact list (bare JIDs) — shown via the directory/contacts
    /// toggle, independent of the server roster.
    #[serde(default)]
    pub jabber_contacts: Vec<String>,
    /// DMs the user closed (hidden from the open-DM chips; history is kept).
    #[serde(default)]
    pub jabber_closed_dms: Vec<String>,
    /// Bot JID that broadcast pings are sent to (empty = derive directorbot@<domain>).
    #[serde(default)]
    pub jabber_ping_bot: String,
    /// Editable list of ping groups offered by the quick-ping UI (coord, recon, …).
    #[serde(default)]
    pub jabber_ping_groups: Vec<String>,
    /// Fleet-ping alert rules: a match plays a sound + highlights the ping.
    #[serde(default)]
    pub jabber_ping_rules: Vec<PingRule>,
    /// A version the user chose not to be reminded about ("No" on the update prompt).
    #[serde(default)]
    pub update_skip_version: String,
    /// First-run setup wizard has been completed or dismissed.
    #[serde(default)]
    pub wizard_done: bool,
    /// Watch the clipboard for d-scans and offer to share them.
    #[serde(default = "default_true")]
    pub dscan_autoprompt: bool,
    /// Automatically upload a detected d-scan to dscan.info (skip the share prompt).
    #[serde(default)]
    pub dscan_autoupload: bool,
    /// When setting a destination, route through known wormholes (waypoints at each
    /// hole entrance) if that's shorter than the gate route.
    #[serde(default)]
    pub route_via_wormholes: bool,
    /// Hide to the system tray instead of quitting when the window is closed.
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    /// Launch EVE Spai automatically on login.
    #[serde(default)]
    pub autostart: bool,
}

fn default_jabber_server() -> String {
    "jabber-server.goonfleet.com".to_owned()
}

fn default_overlay_opacity() -> f32 {
    0.9
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
/// A fleet-ping alert rule: a match plays a sound + highlights the ping. Empty fields
/// mean "don't care"; all set fields must match (case-insensitive substring).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PingRule {
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub fc: String,
    /// "strategic" | "peacetime" | "" (any).
    #[serde(default)]
    pub pap: String,
    #[serde(default)]
    pub doctrine: String,
    #[serde(default)]
    pub formup: String,
    /// Matches anywhere in the ping text.
    #[serde(default)]
    pub keyword: String,
    #[serde(default = "default_ping_sound")]
    pub sound: String,
    // --- actions (suppress overrides the others) ---
    #[serde(default = "default_true")]
    pub notify: bool,
    #[serde(default)]
    pub suppress: bool,
    #[serde(default)]
    pub push: bool,
    /// UI: rule card expanded.
    #[serde(default)]
    pub expanded: bool,
}

impl Default for PingRule {
    fn default() -> Self {
        Self {
            name: "New rule".to_owned(),
            enabled: true,
            fc: String::new(),
            pap: String::new(),
            doctrine: String::new(),
            formup: String::new(),
            keyword: String::new(),
            sound: default_ping_sound(),
            notify: true,
            suppress: false,
            push: false,
            expanded: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    pub enabled: bool,
    // --- conditions (a set/empty field means "don't care") ---
    pub min_severity: Severity,
    /// Specific systems by name (empty = any).
    pub systems: Vec<String>,
    /// Constellations by name — a report in any of them matches.
    #[serde(default)]
    pub constellations: Vec<String>,
    /// Regions by name — a report in any of them matches.
    #[serde(default)]
    pub regions: Vec<String>,
    /// Intel channels — a report from any matching channel passes (each entry is a
    /// case-insensitive regex/substring; empty = any channel).
    #[serde(default)]
    pub channels: Vec<String>,
    /// Within this many jumps of an alerting character (None = any distance).
    pub max_jumps: Option<u32>,
    /// Count jump bridges in the jump-range distance. Off = gate-only (the distance
    /// a hostile, who can't use your bridges, would actually have to travel).
    #[serde(default = "default_true")]
    pub count_bridges: bool,
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
    /// UI-only: whether the rule is expanded in the editor (not persisted).
    #[serde(skip)]
    pub expanded: bool,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            name: "New rule".to_owned(),
            enabled: true,
            min_severity: Severity::Warning,
            systems: Vec::new(),
            constellations: Vec::new(),
            regions: Vec::new(),
            channels: Vec::new(),
            max_jumps: None,
            count_bridges: true,
            min_count: None,
            require: Vec::new(),
            characters: Vec::new(),
            suppress: false,
            system_notification: true,
            custom_window: true,
            push: false,
            sound: String::new(),
            cooldown_secs: 60,
            expanded: false,
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

/// The seeded default rule: any intel ≥ Warning within 10 jumps of an enabled
/// character → system notification + sound + custom window.
pub fn default_rule() -> AlertRule {
    AlertRule {
        name: "Nearby intel".to_owned(),
        enabled: true,
        min_severity: Severity::Warning,
        max_jumps: Some(10),
        custom_window: true,
        ..AlertRule::default()
    }
}

/// Intel alerting configuration. Fully rule-based: a report alerts only if a rule
/// matches it (top rule wins). The seeded default rule covers the common case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertSettings {
    /// Per-severity default sound preset/path: [Info, Warning, Danger, Critical].
    pub sounds: Vec<String>,
    /// Custom-window top-left position (screen pixels); None = auto.
    pub window_pos: Option<(f32, f32)>,
    /// Custom-window size; None = default.
    pub window_size: Option<(f32, f32)>,
    /// Seconds the custom window stays after the last alert.
    pub window_timeout: f32,
    /// Always-on-top behaviour for the custom window.
    pub on_top: OnTop,
    pub push_enabled: bool,
    pub pushover_token: String,
    pub pushover_user: String,
    /// Ordered rules (top = highest precedence).
    pub rules: Vec<AlertRule>,
    /// Whether the default rule has been seeded (so we only do it once).
    pub seeded: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            sounds: vec![
                "off".to_owned(),      // Info
                "warning".to_owned(),  // Warning
                "danger".to_owned(),   // Danger
                "critical".to_owned(), // Critical
            ],
            window_pos: None,
            window_size: None,
            window_timeout: 30.0,
            on_top: OnTop::Always,
            push_enabled: false,
            pushover_token: String::new(),
            pushover_user: String::new(),
            rules: vec![default_rule()],
            seeded: true,
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
    #[serde(default = "crit")]
    pub cap_tackled: Severity,
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
            cap_tackled: Critical,
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

fn crit() -> Severity {
    Severity::Critical
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

/// A named Travel route saved by the user (optionally filed under a folder).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedRoute {
    pub name: String,
    #[serde(default)]
    pub folder: String,
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub waypoints: Vec<i64>,
    #[serde(default)]
    pub jumps: usize,
    /// Routing constraints captured at save time (None = older route without them).
    #[serde(default)]
    pub constraints: Option<RouteConstraints>,
}

/// Travel routing constraints saved alongside a route.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteConstraints {
    pub sec: [bool; 3],
    pub metric: u8,
    pub regional_gates: bool,
    pub jump_bridges: bool,
    pub avoid_camps: bool,
    #[serde(default)]
    pub avoid: Vec<i64>,
    #[serde(default)]
    pub avoid_sov: Vec<String>,
}

fn default_intel_ttl() -> i64 {
    300
}

fn default_true() -> bool {
    true
}
fn default_msg_sound() -> String {
    "chime".to_owned()
}
fn default_ping_sound() -> String {
    "horn".to_owned()
}
fn default_alert_jumps() -> u32 {
    5
}

fn default_kill_jumps() -> u32 {
    6
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
            kill_intel: true,
            kill_intel_jumps: default_kill_jumps(),
            intel_ttl_secs: 300,
            fit_site: String::new(),
            doctrine_url: String::new(),
            fleet_ping_window: false,
            travel_auto_dest: true,
            saved_routes: Vec::new(),
            route_folders: Vec::new(),
            sov_upgrades: Vec::new(),
            coalitions: default_coalitions(),
            view_options: String::new(),
            alliances: Vec::new(),
            severity: SeverityRules::default(),
            alerts: AlertSettings::default(),
            map_overlay_opacity: 0.9,
            map_overlay_smart: false,
            jabber_enabled: false,
            jabber_jid: String::new(),
            jabber_server: default_jabber_server(),
            jabber_rooms: Vec::new(),
            jabber_muc_domain: String::new(),
            jabber_muted: std::collections::BTreeMap::new(),
            jabber_msg_sound: default_msg_sound(),
            jabber_ping_sound: default_ping_sound(),
            jabber_sound_enabled: true,
            jabber_contacts: Vec::new(),
            jabber_closed_dms: Vec::new(),
            jabber_ping_bot: String::new(),
            jabber_ping_groups: Vec::new(),
            jabber_ping_rules: Vec::new(),
            update_skip_version: String::new(),
            wizard_done: false,
            dscan_autoprompt: true,
            dscan_autoupload: false,
            route_via_wormholes: false,
            minimize_to_tray: true,
            autostart: false,
        }
    }
}
