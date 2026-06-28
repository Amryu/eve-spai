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
    /// One-time migration marker: the fleet ping window was force-enabled once for existing
    /// users (it's now on by default). After that the user's own choice is respected.
    #[serde(default)]
    pub fleet_window_forced: bool,
    /// Auto-write the planned route into EVE (set destination hop-by-hop) while Live Mode is on.
    #[serde(default = "default_true")]
    pub travel_auto_dest: bool,
    /// Learned op-channel comms links: normalised op key ("op4") → gnf.lt mumble link, cached
    /// from well-formed fleet pings so malformed ones (that only name the channel) can still
    /// offer "Join Mumble".
    #[serde(default)]
    pub op_channel_links: std::collections::HashMap<String, String>,
    /// User-saved Travel routes.
    #[serde(default)]
    pub saved_routes: Vec<SavedRoute>,
    /// Folder names for organising saved routes (also covers empty folders).
    #[serde(default)]
    pub route_folders: Vec<String>,
    /// Configured sovereignty upgrades per system (pasted from a coalition site).
    #[serde(default)]
    pub sov_upgrades: Vec<SovUpgrade>,
    /// Favourited systems for the jump planner (preferred mid-points).
    #[serde(default)]
    pub jump_favourites: Vec<i64>,
    /// Saved capital jump routes.
    #[serde(default)]
    pub saved_jump_routes: Vec<SavedJumpRoute>,
    /// User-marked capital docking permits per system. Dockable systems are *preferred* when
    /// routing (never required), and flagged on the map / as a destination warning.
    #[serde(default)]
    pub jump_dock: Vec<DockPermit>,
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
    /// Custom battle-report inclusion rules (empty = default intel-area behaviour).
    #[serde(default)]
    pub battles: BattleFilter,
    /// Minimum cumulative ISK destroyed for a battle to be listed (0 = no minimum). Persisted.
    #[serde(default)]
    pub min_battle_isk: f64,
    /// How hard the app may work on heavy background tasks (battle feed + clustering).
    #[serde(default)]
    pub work_throttle: WorkThrottle,
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
    /// Automatically upload a detected d-scan (skip the share prompt).
    #[serde(default)]
    pub dscan_autoupload: bool,
    /// Where d-scans go. Auto picks adashboard.info/intel for the Imperium (when an
    /// `*.imperium` intel channel is configured), otherwise dscan.info.
    #[serde(default)]
    pub dscan_service: DscanService,
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
    /// Override the matched event's severity for this alert (None = leave unchanged).
    /// Lets a rule show an event quietly (e.g. force Info, whose default sound is silent)
    /// or louder without changing the severity rules. Applied after matching, so the
    /// rule's `min_severity` condition still tests the event's natural severity.
    #[serde(default)]
    pub severity_override: Option<Severity>,
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
            severity_override: None,
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

/// Which d-scan service to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DscanService {
    /// adashboard.info/intel for the Imperium, dscan.info for everyone else.
    #[default]
    Auto,
    DscanInfo,
    Adashboard,
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
    #[serde(default = "danger")]
    pub dropper: Severity,
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
            dropper: Danger,
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

fn danger() -> Severity {
    Severity::Danger
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

/// A saved capital jump route (endpoints, waypoints, hull + skills).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedJumpRoute {
    pub name: String,
    #[serde(default)]
    pub folder: String,
    pub from: i64,
    #[serde(default)]
    pub waypoints: Vec<i64>,
    pub to: i64,
    #[serde(default)]
    pub ship: usize,
    #[serde(default)]
    pub jdc: u32,
    #[serde(default)]
    pub jfc: u32,
    #[serde(default)]
    pub jumps: usize,
}

/// A user-set capital docking permit for a system (there's no reliable public list of
/// friendly cap-dockable structures, so the user marks systems themselves). `supers` implies
/// `capitals` (a Keepstar docks both).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DockPermit {
    pub system: String,
    /// Regular capitals (dread / carrier / FAX / rorqual / jump freighter) can dock here.
    pub capitals: bool,
    /// Supercarriers / titans can dock here (a Keepstar).
    pub supers: bool,
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
    0
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
            fleet_ping_window: true,
            fleet_window_forced: false,
            travel_auto_dest: true,
            op_channel_links: std::collections::HashMap::new(),
            saved_routes: Vec::new(),
            route_folders: Vec::new(),
            sov_upgrades: Vec::new(),
            jump_favourites: Vec::new(),
            saved_jump_routes: Vec::new(),
            jump_dock: Vec::new(),
            coalitions: default_coalitions(),
            view_options: String::new(),
            alliances: Vec::new(),
            severity: SeverityRules::default(),
            alerts: AlertSettings::default(),
            battles: BattleFilter::default(),
            min_battle_isk: 0.0,
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
            dscan_service: DscanService::Auto,
            route_via_wormholes: false,
            minimize_to_tray: true,
            autostart: false,
            work_throttle: WorkThrottle::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Custom battle-report inclusion rules.
// ---------------------------------------------------------------------------

/// How hard the app works on heavy background tasks (the battle feed + clustering). Higher
/// throttle = lower CPU/network, at the cost of battle reports updating more slowly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkThrottle {
    /// No pacing; recluster almost immediately.
    Full,
    /// Light pacing — the default.
    #[default]
    Balanced,
    Light,
    /// Strongest throttle; lowest load, slowest updates.
    Minimal,
}

impl WorkThrottle {
    pub fn from_u8(n: u8) -> Self {
        match n {
            0 => WorkThrottle::Full,
            1 => WorkThrottle::Balanced,
            2 => WorkThrottle::Light,
            _ => WorkThrottle::Minimal,
        }
    }
    pub fn as_u8(self) -> u8 {
        match self {
            WorkThrottle::Full => 0,
            WorkThrottle::Balanced => 1,
            WorkThrottle::Light => 2,
            WorkThrottle::Minimal => 3,
        }
    }
    /// Pause after processing each feed killmail — caps CPU and request rate during catch-up.
    pub fn feed_delay_ms(self) -> u64 {
        match self {
            WorkThrottle::Full => 0,
            WorkThrottle::Balanced => 15,
            WorkThrottle::Light => 60,
            WorkThrottle::Minimal => 200,
        }
    }
    /// Minimum interval between battle re-clusters — coalesces the O(n²) clustering work.
    pub fn cluster_interval_ms(self) -> u64 {
        match self {
            WorkThrottle::Full => 800,
            WorkThrottle::Balanced => 3_000,
            WorkThrottle::Light => 8_000,
            WorkThrottle::Minimal => 20_000,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            WorkThrottle::Full => "Full",
            WorkThrottle::Balanced => "Balanced",
            WorkThrottle::Light => "Light",
            WorkThrottle::Minimal => "Minimal",
        }
    }
    pub const CHOICES: [WorkThrottle; 4] =
        [WorkThrottle::Full, WorkThrottle::Balanced, WorkThrottle::Light, WorkThrottle::Minimal];
}

/// Ordered battle-filter rules. The first rule that matches a battle decides whether it is shown;
/// if none match, the default intel-area behaviour applies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BattleFilter {
    #[serde(default)]
    pub rules: Vec<BattleRule>,
}

impl Default for BattleFilter {
    fn default() -> Self {
        Self { rules: BattleFilter::default_rules() }
    }
}

impl BattleFilter {
    /// Any Include rule that can pull in kills *beyond* the intel area (has a locally-checkable
    /// condition other than IntelArea). When false, a non-tracked kill can be dropped immediately
    /// without building (expensive) match data — the common case with only the default rule.
    pub fn widens_beyond_intel(&self) -> bool {
        self.rules.iter().any(|r| {
            r.action == RuleAction::Include
                && r.conditions
                    .iter()
                    .any(|c| c.local_at_ingest() && !matches!(c, BattleCond::IntelArea))
        })
    }

    /// Largest "jumps from me ≤ N" used by any rule, to bound the distance search (None = no rule
    /// uses distance, so it needn't be computed at all).
    pub fn max_jumps_condition(&self) -> Option<u32> {
        self.rules
            .iter()
            .flat_map(|r| &r.conditions)
            .filter_map(|c| match c {
                BattleCond::JumpsFromMe(n) => Some(*n),
                _ => None,
            })
            .max()
    }

    /// Whether every condition is just IntelArea (or there are none) — the shipped default. Then
    /// display visibility is exactly the tracked-area test, with no per-battle match data.
    pub fn is_default_only(&self) -> bool {
        self.rules.iter().all(|r| r.conditions.iter().all(|c| matches!(c, BattleCond::IntelArea)))
    }

    /// The shipped baseline: include battles in the intel-tracked area (today's behaviour),
    /// as one editable rule.
    pub fn default_rules() -> Vec<BattleRule> {
        vec![BattleRule {
            action: RuleAction::Include,
            match_all: true,
            conditions: vec![BattleCond::IntelArea],
            expanded: true,
        }]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleAction {
    Include,
    Exclude,
}

/// Hull size ladder. `Other` (industrials, freighters, pods, …) sorts lowest so "Battleship and
/// up" never matches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShipSize {
    Other,
    Frigate,
    Destroyer,
    Cruiser,
    Battlecruiser,
    Battleship,
    Capital,
    Supercapital,
}

impl ShipSize {
    /// Classify an SDE `group_name` into a size tier.
    pub fn from_group(group: &str) -> ShipSize {
        let g = group.to_lowercase();
        if g.contains("titan") || g.contains("supercarrier") {
            ShipSize::Supercapital
        } else if g.contains("dreadnought")
            || g.contains("carrier")
            || g.contains("force auxiliary")
            || g.contains("capital industrial")
            || g == "rorqual"
        {
            ShipSize::Capital
        } else if g.contains("battlecruiser") || g.contains("command ship") {
            ShipSize::Battlecruiser
        } else if g.contains("battleship") || g.contains("marauder") || g.contains("black ops") {
            ShipSize::Battleship
        } else if g.contains("frigate")
            || g.contains("interceptor")
            || g.contains("covert ops")
            || g.contains("stealth bomber")
            || g.contains("electronic attack")
            || g.contains("corvette")
        {
            // Checked before "cruiser"/"logistics" so a Logistics Frigate stays a frigate.
            ShipSize::Frigate
        } else if g.contains("cruiser") || g.contains("logistics") || g.contains("recon") {
            ShipSize::Cruiser
        } else if g.contains("destroyer") || g.contains("interdictor") {
            ShipSize::Destroyer
        } else {
            ShipSize::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShipSize::Other => "Other",
            ShipSize::Frigate => "Frigate",
            ShipSize::Destroyer => "Destroyer",
            ShipSize::Cruiser => "Cruiser",
            ShipSize::Battlecruiser => "Battlecruiser",
            ShipSize::Battleship => "Battleship",
            ShipSize::Capital => "Capital",
            ShipSize::Supercapital => "Supercapital",
        }
    }

    /// The tiers offered in the dialog (Frigate … Supercapital — "Other" isn't a useful floor).
    pub const CHOICES: [ShipSize; 7] = [
        ShipSize::Frigate,
        ShipSize::Destroyer,
        ShipSize::Cruiser,
        ShipSize::Battlecruiser,
        ShipSize::Battleship,
        ShipSize::Capital,
        ShipSize::Supercapital,
    ];
}

/// One condition in a battle rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BattleCond {
    /// Within the default intel-tracked area (near a system in the intel feed). This is the
    /// app's baseline behaviour, expressed as an editable condition.
    IntelArea,
    Coalition(String),
    Alliance(String),
    Corporation(String),
    Player(String),
    Region(String),
    Constellation(String),
    System(String),
    /// Battle within N jumps of the active character.
    JumpsFromMe(u32),
    HullSizeAtLeast(ShipSize),
    ShipType(String),
    /// Total ISK destroyed (both sides) ≥ this value.
    IskAtLeast(f64),
    /// Total ISK destroyed (both sides) ≤ this value.
    IskAtMost(f64),
}

/// Resolved facts about a kill (at ingest) or a whole battle (at display) that conditions test.
/// All name sets are lower-cased. At ingest only the locally-derivable sets are filled
/// (coalitions, locations, hulls, jumps); alliance/corp/pilot names need ESI resolution and are
/// only filled for already-clustered battles at display.
#[derive(Default)]
pub struct MatchData {
    pub systems: std::collections::HashSet<String>,
    pub regions: std::collections::HashSet<String>,
    pub constellations: std::collections::HashSet<String>,
    pub coalitions: std::collections::HashSet<String>,
    pub alliances: std::collections::HashSet<String>,
    pub corporations: std::collections::HashSet<String>,
    pub pilots: std::collections::HashSet<String>,
    pub max_size: ShipSize,
    pub ship_names: std::collections::HashSet<String>,
    /// Within the default intel-tracked area.
    pub in_intel_area: bool,
    /// Distance from the active character (None = unknown / unreachable).
    pub min_jumps_from_me: Option<u32>,
    /// Total ISK destroyed (None at ingest — not yet known; ISK conditions then can't disprove).
    pub total_isk: Option<f64>,
}

impl Default for ShipSize {
    fn default() -> Self {
        ShipSize::Other
    }
}

impl BattleCond {
    pub fn matches(&self, d: &MatchData) -> bool {
        let has = |set: &std::collections::HashSet<String>, v: &str| set.contains(&v.trim().to_lowercase());
        match self {
            BattleCond::IntelArea => d.in_intel_area,
            BattleCond::Coalition(v) => has(&d.coalitions, v),
            BattleCond::Alliance(v) => has(&d.alliances, v),
            BattleCond::Corporation(v) => has(&d.corporations, v),
            BattleCond::Player(v) => has(&d.pilots, v),
            BattleCond::Region(v) => has(&d.regions, v),
            BattleCond::Constellation(v) => has(&d.constellations, v),
            BattleCond::System(v) => has(&d.systems, v),
            BattleCond::JumpsFromMe(n) => d.min_jumps_from_me.is_some_and(|j| j <= *n),
            BattleCond::HullSizeAtLeast(s) => d.max_size >= *s,
            BattleCond::ShipType(v) => has(&d.ship_names, v),
            BattleCond::IskAtLeast(v) => d.total_isk.map_or(true, |t| t >= *v),
            BattleCond::IskAtMost(v) => d.total_isk.map_or(true, |t| t <= *v),
        }
    }

    /// Spatial bound — limits *where* a rule reaches.
    pub fn is_spatial(&self) -> bool {
        matches!(
            self,
            BattleCond::IntelArea
                | BattleCond::Region(_)
                | BattleCond::Constellation(_)
                | BattleCond::System(_)
                | BattleCond::JumpsFromMe(_)
        )
    }

    pub fn is_participant(&self) -> bool {
        matches!(
            self,
            BattleCond::Coalition(_)
                | BattleCond::Alliance(_)
                | BattleCond::Corporation(_)
                | BattleCond::Player(_)
        )
    }

    /// Whether this condition can be evaluated for a single kill at ingest without ESI name
    /// resolution. Alliance/Corp/Player/ShipType (need id→name) and ISK (need the clustered
    /// battle) cannot, and are deferred to the display check.
    fn local_at_ingest(&self) -> bool {
        matches!(
            self,
            BattleCond::IntelArea
                | BattleCond::Coalition(_)
                | BattleCond::Region(_)
                | BattleCond::Constellation(_)
                | BattleCond::System(_)
                | BattleCond::JumpsFromMe(_)
                | BattleCond::HullSizeAtLeast(_)
        )
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            BattleCond::IntelArea => "Intel area",
            BattleCond::Coalition(_) => "Coalition",
            BattleCond::Alliance(_) => "Alliance",
            BattleCond::Corporation(_) => "Corporation",
            BattleCond::Player(_) => "Player",
            BattleCond::Region(_) => "Region",
            BattleCond::Constellation(_) => "Constellation",
            BattleCond::System(_) => "System",
            BattleCond::JumpsFromMe(_) => "Jumps from me ≤",
            BattleCond::HullSizeAtLeast(_) => "Hull size ≥",
            BattleCond::ShipType(_) => "Ship type",
            BattleCond::IskAtLeast(_) => "ISK total ≥",
            BattleCond::IskAtMost(_) => "ISK total ≤",
        }
    }

    /// Default instance of each condition kind, for the dialog's type picker.
    pub fn kinds() -> Vec<BattleCond> {
        vec![
            BattleCond::IntelArea,
            BattleCond::Coalition(String::new()),
            BattleCond::Alliance(String::new()),
            BattleCond::Corporation(String::new()),
            BattleCond::Player(String::new()),
            BattleCond::Region(String::new()),
            BattleCond::Constellation(String::new()),
            BattleCond::System(String::new()),
            BattleCond::JumpsFromMe(5),
            BattleCond::HullSizeAtLeast(ShipSize::Battleship),
            BattleCond::ShipType(String::new()),
            BattleCond::IskAtLeast(1_000_000_000.0),
            BattleCond::IskAtMost(1_000_000_000.0),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BattleRule {
    pub action: RuleAction,
    /// All conditions must match (AND) vs any (OR).
    pub match_all: bool,
    pub conditions: Vec<BattleCond>,
    #[serde(skip)]
    pub expanded: bool,
}

impl Default for BattleRule {
    fn default() -> Self {
        Self { action: RuleAction::Include, match_all: true, conditions: Vec::new(), expanded: true }
    }
}

impl BattleRule {
    pub fn matches(&self, d: &MatchData) -> bool {
        if self.match_all {
            self.conditions.iter().all(|c| c.matches(d))
        } else {
            self.conditions.iter().any(|c| c.matches(d))
        }
    }

    /// Whether an Include rule admits a single kill at ingest, using only conditions evaluable
    /// without ESI. Conditions needing resolution (alliance/corp/player/ISK) are deferred to the
    /// display check, so a rule with *no* locally-evaluable condition never widens ingestion.
    pub fn admits_ingest(&self, d: &MatchData) -> bool {
        if self.action != RuleAction::Include {
            return false;
        }
        let local: Vec<&BattleCond> = self.conditions.iter().filter(|c| c.local_at_ingest()).collect();
        if local.is_empty() {
            return false;
        }
        if self.match_all {
            local.iter().all(|c| c.matches(d))
        } else {
            local.iter().any(|c| c.matches(d))
        }
    }

    /// An Include rule with no spatial or participant bound matches across all of EVE — flag it so
    /// the user knows it can store a lot of history.
    pub fn is_broad(&self) -> bool {
        self.action == RuleAction::Include
            && !self.conditions.iter().any(|c| c.is_spatial() || c.is_participant())
    }
}

/// First matching rule's action, or `None` if no rule matches (caller falls back to default).
pub fn battle_decision(rules: &[BattleRule], d: &MatchData) -> Option<RuleAction> {
    rules.iter().find(|r| r.matches(d)).map(|r| r.action)
}

#[cfg(test)]
mod battle_filter_tests {
    use super::*;

    fn data() -> MatchData {
        let s = |v: &str| std::iter::once(v.to_lowercase()).collect::<std::collections::HashSet<_>>();
        MatchData {
            regions: s("delve"),
            alliances: ["goonswarm federation".to_owned()].into_iter().collect(),
            max_size: ShipSize::Battleship,
            total_isk: Some(10_000_000_000.0),
            min_jumps_from_me: Some(3),
            systems: s("1dq1-a"),
            ..Default::default()
        }
    }

    #[test]
    fn ship_size_from_group() {
        assert_eq!(ShipSize::from_group("Battleship"), ShipSize::Battleship);
        assert_eq!(ShipSize::from_group("Marauder"), ShipSize::Battleship);
        assert_eq!(ShipSize::from_group("Heavy Assault Cruiser"), ShipSize::Cruiser);
        assert_eq!(ShipSize::from_group("Logistics Frigate"), ShipSize::Frigate);
        assert_eq!(ShipSize::from_group("Interdictor"), ShipSize::Destroyer);
        assert_eq!(ShipSize::from_group("Titan"), ShipSize::Supercapital);
        assert_eq!(ShipSize::from_group("Capsule"), ShipSize::Other);
        assert!(ShipSize::Battleship >= ShipSize::Battleship);
        assert!(ShipSize::Capital >= ShipSize::Battleship);
        assert!(ShipSize::Other < ShipSize::Battleship);
    }

    #[test]
    fn condition_matching() {
        let d = data();
        assert!(BattleCond::Region("Delve".into()).matches(&d));
        assert!(!BattleCond::Region("Fountain".into()).matches(&d));
        assert!(BattleCond::Alliance("Goonswarm Federation".into()).matches(&d));
        assert!(BattleCond::HullSizeAtLeast(ShipSize::Battleship).matches(&d));
        assert!(!BattleCond::HullSizeAtLeast(ShipSize::Capital).matches(&d));
        assert!(BattleCond::JumpsFromMe(5).matches(&d));
        assert!(!BattleCond::JumpsFromMe(2).matches(&d));
        assert!(BattleCond::IskAtLeast(1_000_000_000.0).matches(&d));
        assert!(BattleCond::IskAtMost(1_000_000_000.0).matches(&d) == false);
        // ISK can't be disproved before clustering.
        let mut ingest = data();
        ingest.total_isk = None;
        assert!(BattleCond::IskAtMost(1.0).matches(&ingest));
    }

    #[test]
    fn decision_first_match_wins() {
        let rules = vec![
            BattleRule {
                action: RuleAction::Exclude,
                match_all: true,
                conditions: vec![BattleCond::IskAtMost(500_000_000.0)],
                expanded: false,
            },
            BattleRule {
                action: RuleAction::Include,
                match_all: true,
                conditions: vec![
                    BattleCond::Region("Delve".into()),
                    BattleCond::HullSizeAtLeast(ShipSize::Battleship),
                ],
                expanded: false,
            },
        ];
        // Big Delve BS battle → second rule includes (first doesn't match: ISK not ≤ 500M).
        assert_eq!(battle_decision(&rules, &data()), Some(RuleAction::Include));
        // A tiny battle → first rule excludes.
        let mut small = data();
        small.total_isk = Some(100_000_000.0);
        assert_eq!(battle_decision(&rules, &small), Some(RuleAction::Exclude));
        // Unrelated battle (known ISK above the exclude floor, wrong region) → no rule matches.
        let mut other = MatchData { total_isk: Some(2_000_000_000.0), ..MatchData::default() };
        other.regions = ["fountain".to_owned()].into_iter().collect();
        assert_eq!(battle_decision(&rules, &other), None);
    }

    #[test]
    fn broad_and_widening_flags() {
        let hull_only = BattleRule {
            action: RuleAction::Include,
            match_all: true,
            conditions: vec![BattleCond::HullSizeAtLeast(ShipSize::Battleship)],
            expanded: false,
        };
        assert!(hull_only.is_broad()); // no spatial/participant bound
        assert!(hull_only.admits_ingest(&MatchData { max_size: ShipSize::Battleship, ..Default::default() }));
        let isk_only = BattleRule {
            action: RuleAction::Include,
            match_all: true,
            conditions: vec![BattleCond::IskAtLeast(1.0)],
            expanded: false,
        };
        assert!(!isk_only.admits_ingest(&MatchData::default())); // ISK-only can't widen ingest
        let located = BattleRule {
            action: RuleAction::Include,
            match_all: true,
            conditions: vec![BattleCond::Region("Delve".into())],
            expanded: false,
        };
        assert!(!located.is_broad());
    }
}
