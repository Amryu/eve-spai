use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub nav_expanded: bool,
    pub use_eve_time: bool,
    pub eve_logs_dir: String,
    pub eve_settings_dir: String,
    pub intel_channels: Vec<String>,
    #[serde(default)]
    pub intel_disabled_chars: Vec<String>,
    #[serde(default = "default_client_id")]
    pub sso_client_id: String,
    #[serde(default = "default_callback")]
    pub sso_callback: String,
    #[serde(default)]
    pub configuration_pack: String,
    #[serde(default)]
    pub jump_bridges: Vec<JumpBridge>,
    #[serde(default = "default_true")]
    pub alert_enabled: bool,
    #[serde(default = "default_alert_jumps")]
    pub alert_within_jumps: u32,
    #[serde(default = "default_true")]
    pub alert_combat: bool,
    #[serde(default)]
    pub alert_only_undocked: bool,
    #[serde(default = "default_true")]
    pub kill_intel: bool,
    #[serde(default = "default_kill_jumps")]
    pub kill_intel_jumps: u32,
    #[serde(default = "default_intel_ttl")]
    pub intel_ttl_secs: i64,
    /// Whether the intel feed's jump distances count your own jump bridges. Off, as for alert
    /// rules, is what a hostile who cannot use them actually has to travel.
    #[serde(default)]
    pub intel_count_bridges: bool,
    #[serde(default)]
    pub verdict_explained: bool,
    #[serde(default)]
    pub fit_site: String,
    #[serde(default)]
    pub doctrine_url: String,
    #[serde(default)]
    pub fleet_ping_window: bool,
    #[serde(default)]
    pub fleet_ping_on_top: OnTop,
    /// One-time migration marker: the fleet ping window was force-enabled once for existing
    /// users (it's now on by default). After that the user's own choice is respected.
    #[serde(default)]
    pub fleet_window_forced: bool,
    #[serde(default = "default_true")]
    pub travel_auto_dest: bool,
    #[serde(default)]
    pub op_channel_links: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub saved_routes: Vec<SavedRoute>,
    #[serde(default)]
    pub route_folders: Vec<String>,
    #[serde(default)]
    pub sov_upgrades: Vec<SovUpgrade>,
    #[serde(default)]
    pub jump_favourites: Vec<i64>,
    #[serde(default)]
    pub saved_jump_routes: Vec<SavedJumpRoute>,
    #[serde(default)]
    pub jump_dock: Vec<DockPermit>,
    #[serde(default = "default_coalitions")]
    pub coalitions: Vec<Coalition>,
    #[serde(default)]
    pub view_options: String,
    #[serde(default)]
    pub alliances: Vec<AllianceConfig>,
    #[serde(default = "default_severity")]
    pub severity: SeverityRules,
    #[serde(default = "default_alerts")]
    pub alerts: AlertSettings,
    #[serde(default = "default_true")]
    pub battles_enabled: bool,
    #[serde(default)]
    pub battles: BattleFilter,
    #[serde(default)]
    pub min_battle_isk: f64,
    #[serde(default = "default_battle_break")]
    pub battle_break_secs: i64,
    #[serde(default)]
    pub bookmarks: Vec<i64>,
    #[serde(default)]
    pub work_throttle: WorkThrottle,
    #[serde(default = "default_overlay_opacity")]
    pub map_overlay_opacity: f32,
    #[serde(default)]
    pub map_overlay_smart: bool,
    #[serde(default)]
    pub jabber_enabled: bool,
    #[serde(default)]
    pub jabber_jid: String,
    #[serde(default = "default_jabber_server")]
    pub jabber_server: String,
    #[serde(default)]
    pub jabber_rooms: Vec<String>,
    #[serde(default)]
    pub jabber_muc_domain: String,
    /// Muted conversations/feeds: key (bare JID, room JID, or "pings") → unmute unix
    /// time (i64::MAX = muted until manually unmuted). Muted = no sound, no badge.
    #[serde(default)]
    pub jabber_muted: std::collections::BTreeMap<String, i64>,
    #[serde(default = "default_msg_sound")]
    pub jabber_msg_sound: String,
    #[serde(default = "default_ping_sound")]
    pub jabber_ping_sound: String,
    #[serde(default = "default_mention_sound")]
    pub jabber_mention_sound: String,
    #[serde(default = "default_volume")]
    pub jabber_msg_volume: f32,
    #[serde(default = "default_volume")]
    pub jabber_ping_volume: f32,
    #[serde(default = "default_volume")]
    pub jabber_mention_volume: f32,
    /// Extra words that count as a mention. The Jabber username always counts.
    #[serde(default)]
    pub jabber_mention_keywords: Vec<String>,
    #[serde(default = "default_true")]
    pub jabber_mention_ignores_mute: bool,
    #[serde(default = "default_true")]
    pub jabber_sound_enabled: bool,
    #[serde(default)]
    pub jabber_contacts: Vec<String>,
    #[serde(default)]
    pub jabber_closed_dms: Vec<String>,
    #[serde(default)]
    pub jabber_closed_rooms: Vec<String>,
    /// Rooms left/kicked while online, kept struck-through in the channel list across restarts.
    #[serde(default)]
    pub jabber_inaccessible_rooms: Vec<String>,
    /// Last-known room MOTD (MUC subject) per room JID, so history-only channels still show it.
    #[serde(default)]
    pub jabber_room_subjects: std::collections::BTreeMap<String, String>,
    // None = ask on first room-tab close; Some(true) = leave; Some(false) = keep joined, hide tab.
    #[serde(default)]
    pub jabber_close_room_leaves: Option<bool>,
    #[serde(default)]
    pub jabber_ping_bot: String,
    #[serde(default)]
    pub jabber_ping_groups: Vec<String>,
    #[serde(default)]
    pub jabber_ping_rules: Vec<PingRule>,
    #[serde(default)]
    pub jabber_ping_rules_seeded: bool,
    #[serde(default, deserialize_with = "de_chat_windows")]
    pub jabber_popout_windows: Vec<ChatWindowCfg>,
    #[serde(default)]
    pub update_skip_version: String,
    #[serde(default)]
    pub wizard_done: bool,
    #[serde(default = "default_true")]
    pub dscan_autoprompt: bool,
    #[serde(default)]
    pub dscan_autoupload: bool,
    #[serde(default)]
    pub dscan_service: DscanService,
    #[serde(default)]
    pub route_via_wormholes: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub main_window_pos: Option<(f32, f32)>,
    #[serde(default)]
    pub main_window_size: Option<(f32, f32)>,
    #[serde(default)]
    pub main_window_maximized: bool,
    #[serde(default)]
    pub fleet_ping_window_pos: Option<(f32, f32)>,
    #[serde(default)]
    pub fleet_ping_window_size: Option<(f32, f32)>,

    // --- FC / delve911 Rescue Mode (off by default; FC-only feature) ---
    #[serde(default)]
    pub fc_rescue_enabled: bool,
    #[serde(default = "default_rescue_channel")]
    pub rescue_channel: String,
    #[serde(default = "default_rescue_staging")]
    pub rescue_staging_system: String,
    /// System ids that host a friendly cyno generator (ESI can't enumerate these).
    #[serde(default)]
    pub cyno_generators: Vec<i64>,
    #[serde(default = "default_rescue_doctrines", deserialize_with = "de_rescue_doctrines")]
    pub rescue_doctrines: Vec<RescueDoctrine>,
    #[serde(default = "default_rescue_op_channel")]
    pub rescue_op_channel: u8,
    #[serde(default)]
    pub rescue_doctrine: String,
    #[serde(default = "default_rescue_template")]
    pub rescue_ping_template: String,
    /// skirmish_commanders room JID: where the FC posts `!bping <group>` ping requests and watches
    /// the responses. coord/fc/all are directorbot ping GROUPS, not separate rooms.
    #[serde(default)]
    pub rescue_skirmish_jid: String,
    /// XMPP room JID for the delve911 conference, so the FC can respond from the rescue window.
    #[serde(default)]
    pub rescue_delve911_jid: String,
    #[serde(default)]
    pub rescue_window_pos: Option<(f32, f32)>,
    #[serde(default)]
    pub rescue_window_size: Option<(f32, f32)>,
    #[serde(default = "default_rescue_col_ops")]
    pub rescue_col_ops_w: f32,
    #[serde(default = "default_rescue_col_mid")]
    pub rescue_col_mid_w: f32,
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
    pub alliances: Vec<String>,
    #[serde(default)]
    pub color: Option<(u8, u8, u8)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PingRule {
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub fc: String,
    #[serde(default)]
    pub pap: String,
    #[serde(default)]
    pub doctrine: String,
    #[serde(default)]
    pub formup: String,
    #[serde(default)]
    pub keyword: String,
    #[serde(default = "default_ping_sound")]
    pub sound: String,
    /// Per-rule sound volume; `None` uses the global fleet-ping volume.
    #[serde(default)]
    pub volume: Option<f32>,
    #[serde(default = "default_true")]
    pub notify: bool,
    #[serde(default)]
    pub suppress: bool,
    #[serde(default)]
    pub push: bool,
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
            volume: None,
            notify: true,
            suppress: false,
            push: false,
            expanded: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub min_severity: Severity,
    pub systems: Vec<String>,
    #[serde(default)]
    pub constellations: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    pub max_jumps: Option<u32>,
    #[serde(default)]
    pub count_bridges: bool,
    pub min_count: Option<u32>,
    pub require: Vec<String>,
    #[serde(default)]
    pub characters: Vec<String>,
    #[serde(default)]
    pub ships: Vec<String>,
    pub suppress: bool,
    #[serde(default)]
    pub severity_override: Option<Severity>,
    pub system_notification: bool,
    pub custom_window: bool,
    pub push: bool,
    pub sound: String,
    /// Per-rule sound volume; `None` uses the global intel-alert volume.
    #[serde(default)]
    pub volume: Option<f32>,
    pub cooldown_secs: i64,
    #[serde(skip)]
    pub expanded: bool,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            id: 0,
            name: "New rule".to_owned(),
            enabled: true,
            min_severity: Severity::Warning,
            systems: Vec::new(),
            constellations: Vec::new(),
            regions: Vec::new(),
            channels: Vec::new(),
            max_jumps: None,
            count_bridges: false,
            min_count: None,
            require: Vec::new(),
            characters: Vec::new(),
            ships: Vec::new(),
            suppress: false,
            severity_override: None,
            system_notification: true,
            custom_window: true,
            push: false,
            sound: String::new(),
            volume: None,
            cooldown_secs: 60,
            expanded: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OnTop {
    Always,
    #[default]
    Smart,
    Never,
}

/// Assign a stable nonzero `id` to any rule missing one (id == 0). Ids key the per-rule match
/// feed, which must survive reorder and delete. Idempotent.
pub fn ensure_rule_ids(rules: &mut [AlertRule]) {
    let mut next = rules.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    for r in rules.iter_mut() {
        if r.id == 0 {
            r.id = next;
            next += 1;
        }
    }
}

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

pub fn default_ping_rules() -> Vec<PingRule> {
    vec![
        PingRule {
            name: "Strategic fleet".to_owned(),
            pap: "strategic".to_owned(),
            sound: "horn".to_owned(),
            expanded: false,
            ..PingRule::default()
        },
        PingRule {
            name: "Peacetime fleet".to_owned(),
            pap: "peacetime".to_owned(),
            sound: "chime".to_owned(),
            expanded: false,
            ..PingRule::default()
        },
    ]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertSettings {
    pub sounds: Vec<String>,
    #[serde(default = "default_volume")]
    pub alert_volume: f32,
    pub window_pos: Option<(f32, f32)>,
    pub window_size: Option<(f32, f32)>,
    pub window_timeout: f32,
    pub on_top: OnTop,
    pub push_enabled: bool,
    pub pushover_token: String,
    pub pushover_user: String,
    pub rules: Vec<AlertRule>,
    pub seeded: bool,
    pub compact_mode: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            sounds: vec![
                "off".to_owned(),
                "warning".to_owned(),
                "danger".to_owned(),
                "critical".to_owned(),
            ],
            alert_volume: 1.0,
            window_pos: None,
            window_size: None,
            window_timeout: 30.0,
            on_top: OnTop::Always,
            push_enabled: false,
            pushover_token: String::new(),
            pushover_user: String::new(),
            rules: vec![default_rule()],
            seeded: true,
            compact_mode: false,
        }
    }
}

fn default_alerts() -> AlertSettings {
    AlertSettings::default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Danger,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DscanService {
    #[default]
    Auto,
    DscanInfo,
    Adashboard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeverityRules {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AllianceConfig {
    pub name: String,
    #[serde(default)]
    pub color: Option<(u8, u8, u8)>,
}

pub fn default_coalitions() -> Vec<Coalition> {
    // Imperium only. Add other coalitions in Settings; membership shifts often, so edit/reset
    // there to keep it current. Alliance names must match the sov holder name exactly (some end
    // with a period).
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
    ]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SovUpgrade {
    pub system: String,
    pub upgrade: String,
}

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DockPermit {
    pub system: String,
    pub capitals: bool,
    pub supers: bool,
}

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
    #[serde(default)]
    pub constraints: Option<RouteConstraints>,
}

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

fn default_battle_break() -> i64 {
    crate::battle::BATTLE_BREAK_SECS
}

fn default_true() -> bool {
    true
}
fn default_volume() -> f32 {
    1.0
}
fn default_msg_sound() -> String {
    "chime".to_owned()
}
fn default_ping_sound() -> String {
    "horn".to_owned()
}
fn default_mention_sound() -> String {
    "warning".to_owned()
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

/// One persisted pop-out chat window: which conversations it holds and where it sat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatWindowCfg {
    pub id: u64,
    pub tabs: Vec<String>,
    /// Empty means "no explicit active tab", i.e. fall back to the first one.
    pub active: String,
    pub pos: Option<(f32, f32)>,
    pub size: Option<(f32, f32)>,
}

/// Drop unparseable entries rather than failing the whole Settings parse, since one bad entry would
/// otherwise reset every setting the user has.
fn de_chat_windows<'de, D>(d: D) -> Result<Vec<ChatWindowCfg>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MaybeWindow {
        Good(ChatWindowCfg),
        Junk(serde::de::IgnoredAny),
    }
    let items = Vec::<MaybeWindow>::deserialize(d)?;
    Ok(items
        .into_iter()
        .filter_map(|w| match w {
            MaybeWindow::Good(c) => Some(c),
            MaybeWindow::Junk(_) => None,
        })
        .collect())
}

/// A rescue doctrine: the short `name` shown in the selector, and the full `description` line that
/// goes into the ping's "Doctrine:" field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RescueDoctrine {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Accept both the old form (a list of plain name strings) and the new `{name, description}` form,
/// so a config saved before descriptions existed still loads instead of resetting all settings.
fn de_rescue_doctrines<'de, D>(d: D) -> Result<Vec<RescueDoctrine>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneDoctrine {
        Name(String),
        Full(RescueDoctrine),
    }
    let items = Vec::<OneDoctrine>::deserialize(d)?;
    Ok(items
        .into_iter()
        .map(|o| match o {
            OneDoctrine::Name(name) => RescueDoctrine { name, description: String::new() },
            OneDoctrine::Full(r) => r,
        })
        .collect())
}

fn default_rescue_channel() -> String {
    "delve911".to_owned()
}
fn default_rescue_staging() -> String {
    "C-J6MT".to_owned()
}
fn default_rescue_doctrines() -> Vec<RescueDoctrine> {
    let d = |name: &str, description: &str| RescueDoctrine {
        name: name.to_owned(),
        description: description.to_owned(),
    };
    vec![
        d("FNIs", "Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)"),
        d("Svipuls", "Svipul (Boosters > Kirin/Scalpel > Svipul > Else)"),
        d("Harpy", "Harpy Fleet (Boosters > Kirin/Scalpel > Harpy > Else)"),
        d("Flycatchers", "Flycatchers (Boosters > Kirin/Scalpel > Flycatcher > Else)"),
    ]
}
fn default_rescue_op_channel() -> u8 {
    1
}
fn default_rescue_template() -> String {
    "CAP Save - Get In!\n\
     Give me a titan on standby in {staging}\n\
     \n\
     FC Name: {fc}\n\
     Formup Location: {staging}\n\
     PAP Type: Strategic\n\
     Comms: Op {op} {mumble}\n\
     Doctrine: {doctrine}"
        .to_owned()
}
// Siege / Triage / Industrial Core cycle = 300s (5 min): the capital can't move or be
// remote-repped until it ends. PANIC = the Rorqual Pulse Activated Nexus Invulnerability Core,
// which holds invuln 4 min at base up to 6 min with Invulnerability Core Operation; default to a
// 5-min middle and let the FC dial it after asking the pilot.
fn default_rescue_col_ops() -> f32 {
    300.0
}
fn default_rescue_col_mid() -> f32 {
    320.0
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
            intel_count_bridges: false,
            verdict_explained: false,
            fit_site: String::new(),
            doctrine_url: String::new(),
            fleet_ping_window: true,
            fleet_ping_on_top: OnTop::Smart,
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
            battles_enabled: true,
            battles: BattleFilter::default(),
            min_battle_isk: 0.0,
            battle_break_secs: default_battle_break(),
            bookmarks: Vec::new(),
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
            jabber_mention_sound: default_mention_sound(),
            jabber_msg_volume: 1.0,
            jabber_ping_volume: 1.0,
            jabber_mention_volume: 1.0,
            jabber_mention_keywords: Vec::new(),
            jabber_mention_ignores_mute: true,
            jabber_sound_enabled: true,
            jabber_contacts: Vec::new(),
            jabber_closed_dms: Vec::new(),
            jabber_closed_rooms: Vec::new(),
            jabber_inaccessible_rooms: Vec::new(),
            jabber_room_subjects: std::collections::BTreeMap::new(),
            jabber_close_room_leaves: None,
            jabber_ping_bot: String::new(),
            jabber_ping_groups: Vec::new(),
            jabber_ping_rules: default_ping_rules(),
            jabber_ping_rules_seeded: true,
            jabber_popout_windows: Vec::new(),
            update_skip_version: String::new(),
            wizard_done: false,
            dscan_autoprompt: true,
            dscan_autoupload: false,
            dscan_service: DscanService::Auto,
            route_via_wormholes: false,
            minimize_to_tray: true,
            autostart: false,
            main_window_pos: None,
            main_window_size: None,
            main_window_maximized: false,
            fleet_ping_window_pos: None,
            fleet_ping_window_size: None,
            work_throttle: WorkThrottle::default(),
            fc_rescue_enabled: false,
            rescue_channel: default_rescue_channel(),
            rescue_staging_system: default_rescue_staging(),
            cyno_generators: Vec::new(),
            rescue_doctrines: default_rescue_doctrines(),
            rescue_op_channel: default_rescue_op_channel(),
            rescue_doctrine: "FNIs".to_owned(),
            rescue_ping_template: default_rescue_template(),
            rescue_skirmish_jid: String::new(),
            rescue_delve911_jid: String::new(),
            rescue_window_pos: None,
            rescue_window_size: None,
            rescue_col_ops_w: default_rescue_col_ops(),
            rescue_col_mid_w: default_rescue_col_mid(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkThrottle {
    Full,
    #[default]
    Balanced,
    Light,
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
    pub fn feed_delay_ms(self) -> u64 {
        match self {
            WorkThrottle::Full => 0,
            WorkThrottle::Balanced => 15,
            WorkThrottle::Light => 60,
            WorkThrottle::Minimal => 200,
        }
    }
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
    pub fn widens_beyond_intel(&self) -> bool {
        self.rules.iter().any(|r| {
            r.action == RuleAction::Include
                && r.conditions
                    .iter()
                    .any(|c| c.local_at_ingest() && !matches!(c, BattleCond::IntelArea))
        })
    }

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

    pub fn is_default_only(&self) -> bool {
        self.rules.iter().all(|r| r.conditions.iter().all(|c| matches!(c, BattleCond::IntelArea)))
    }

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BattleCond {
    IntelArea,
    Coalition(String),
    Alliance(String),
    Corporation(String),
    Player(String),
    Region(String),
    Constellation(String),
    System(String),
    JumpsFromMe(u32),
    HullSizeAtLeast(ShipSize),
    ShipType(String),
    IskAtLeast(f64),
    IskAtMost(f64),
}

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
    pub in_intel_area: bool,
    pub min_jumps_from_me: Option<u32>,
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

    pub fn is_broad(&self) -> bool {
        self.action == RuleAction::Include
            && !self.conditions.iter().any(|c| c.is_spatial() || c.is_participant())
    }
}

pub fn battle_decision(rules: &[BattleRule], d: &MatchData) -> Option<RuleAction> {
    rules.iter().find(|r| r.matches(d)).map(|r| r.action)
}

#[cfg(test)]
mod window_geometry_tests {
    use super::*;

    #[test]
    fn geometry_fields_roundtrip() {
        let mut s = Settings::default();
        s.main_window_pos = Some((100.0, 200.0));
        s.main_window_size = Some((1280.0, 800.0));
        s.main_window_maximized = true;
        s.fleet_ping_window_pos = Some((300.0, 50.0));
        s.fleet_ping_window_size = Some((600.0, 400.0));
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.main_window_pos, Some((100.0, 200.0)));
        assert_eq!(back.main_window_size, Some((1280.0, 800.0)));
        assert!(back.main_window_maximized);
        assert_eq!(back.fleet_ping_window_pos, Some((300.0, 50.0)));
        assert_eq!(back.fleet_ping_window_size, Some((600.0, 400.0)));
    }

    #[test]
    fn intel_count_bridges_roundtrips_and_defaults_to_gate_only() {
        let s = Settings { intel_count_bridges: true, ..Default::default() };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert!(back.intel_count_bridges);
        // A config written before the field existed keeps the alert rules' gate-only reading.
        let legacy: Settings = serde_json::from_str(r#"{"jabber_jid":"a@b"}"#).unwrap();
        assert_eq!(legacy.jabber_jid, "a@b");
        assert!(!legacy.intel_count_bridges);
    }

    #[test]
    fn legacy_settings_without_geometry_default() {
        // Settings JSON predating the geometry fields deserializes them to their defaults.
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.main_window_pos, None);
        assert_eq!(s.main_window_size, None);
        assert!(!s.main_window_maximized);
        assert_eq!(s.fleet_ping_window_pos, None);
        assert_eq!(s.fleet_ping_window_size, None);
    }

    #[test]
    fn legacy_string_doctrines_still_parse() {
        // A config saved before doctrine descriptions existed must NOT fail the whole Settings
        // parse (which would reset every setting). Old form = list of plain name strings.
        let json = r#"{"rescue_doctrines":["Harpy","FNI","Flycatcher"],"jabber_jid":"a@b"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.jabber_jid, "a@b");
        assert_eq!(s.rescue_doctrines.len(), 3);
        assert_eq!(s.rescue_doctrines[0].name, "Harpy");
        assert_eq!(s.rescue_doctrines[0].description, "");
        // New object form still parses too.
        let json2 = r#"{"rescue_doctrines":[{"name":"FNIs","description":"Hammer Fleet"}]}"#;
        let s2: Settings = serde_json::from_str(json2).unwrap();
        assert_eq!(s2.rescue_doctrines[0].description, "Hammer Fleet");
    }

    #[test]
    fn chat_window_cfgs_roundtrip() {
        let s = Settings {
            jabber_popout_windows: vec![
                ChatWindowCfg {
                    id: 7,
                    tabs: vec!["a@conf.x".to_owned(), "b@x".to_owned()],
                    active: "b@x".to_owned(),
                    pos: Some((-1920.0, 40.0)),
                    size: Some((700.0, 500.0)),
                },
                ChatWindowCfg { id: 8, ..Default::default() },
            ],
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.jabber_popout_windows, s.jabber_popout_windows);
    }

    #[test]
    fn legacy_settings_without_popout_windows_default() {
        let s: Settings = serde_json::from_str(r#"{"jabber_jid":"a@b"}"#).unwrap();
        assert_eq!(s.jabber_jid, "a@b");
        assert!(s.jabber_popout_windows.is_empty());
    }

    #[test]
    fn one_malformed_popout_window_keeps_the_rest() {
        // A single bad entry must drop only itself: failing the field would fail the whole
        // Settings parse and reset every setting the user has.
        let json = r#"{"jabber_jid":"a@b","jabber_popout_windows":[
            {"id":1,"tabs":["x@y"],"active":"x@y"},
            {"id":"not-a-number"},
            42,
            {"id":2,"tabs":[],"active":"","pos":null,"size":[600.0,400.0]}
        ]}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.jabber_jid, "a@b");
        let ids: Vec<u64> = s.jabber_popout_windows.iter().map(|w| w.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(s.jabber_popout_windows[0].tabs, vec!["x@y".to_owned()]);
        assert_eq!(s.jabber_popout_windows[1].size, Some((600.0, 400.0)));
    }

    #[test]
    fn geometry_update_positive_and_negative() {
        use crate::app::geometry_update;
        // Positive: a first value, and a move/resize past the dead-zone, are stored.
        assert_eq!(geometry_update(None, (10.0, 20.0), 2.0), Some((10.0, 20.0)));
        assert_eq!(geometry_update(Some((100.0, 100.0)), (140.0, 100.0), 2.0), Some((140.0, 100.0)));
        // Negative coords persist: a monitor left of / above the primary has negative
        // virtual-desktop coords, needed to reopen the window on that monitor.
        assert_eq!(geometry_update(None, (-1920.0, 10.0), 0.0), Some((-1920.0, 10.0)));
        assert_eq!(geometry_update(None, (200.0, -1080.0), 0.0), Some((200.0, -1080.0)));
        // Rejected: sub-dead-zone jitter, an unchanged value, and winit's minimized sentinel.
        assert_eq!(geometry_update(Some((100.0, 100.0)), (101.0, 100.5), 2.0), None);
        assert_eq!(geometry_update(Some((100.0, 100.0)), (100.0, 100.0), 0.0), None);
        assert_eq!(geometry_update(Some((100.0, 100.0)), (-32001.0, -32001.0), 0.0), None);
    }
}

#[cfg(test)]
mod ping_seed_tests {
    use super::*;

    #[test]
    fn default_ping_rules_cover_strategic_and_peacetime() {
        let rules = default_ping_rules();
        assert_eq!(rules.len(), 2);
        let strat = rules.iter().find(|r| r.pap == "strategic").expect("strategic rule");
        assert!(strat.enabled && strat.notify && strat.sound == "horn");
        let peace = rules.iter().find(|r| r.pap == "peacetime").expect("peacetime rule");
        assert!(peace.enabled && peace.notify && peace.sound == "chime");
    }

    #[test]
    fn fresh_settings_are_seeded_with_ping_rules() {
        let s = Settings::default();
        assert!(s.jabber_ping_rules_seeded);
        assert_eq!(s.jabber_ping_rules.len(), 2);
    }

    #[test]
    fn ensure_rule_ids_assigns_unique_nonzero_and_is_idempotent() {
        let mut rules = vec![
            AlertRule { id: 0, ..AlertRule::default() },
            AlertRule { id: 5, ..AlertRule::default() },
            AlertRule { id: 0, ..AlertRule::default() },
        ];
        ensure_rule_ids(&mut rules);
        assert!(rules.iter().all(|r| r.id != 0));
        assert_eq!(rules[1].id, 5);
        let ids: std::collections::HashSet<u64> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 3);

        let snapshot: Vec<u64> = rules.iter().map(|r| r.id).collect();
        ensure_rule_ids(&mut rules);
        assert_eq!(snapshot, rules.iter().map(|r| r.id).collect::<Vec<_>>());
    }

    #[test]
    fn ensure_rule_ids_default_rule_gets_id_one() {
        let mut rules = vec![default_rule()];
        ensure_rule_ids(&mut rules);
        assert_eq!(rules[0].id, 1);
    }
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
        assert_eq!(battle_decision(&rules, &data()), Some(RuleAction::Include));
        let mut small = data();
        small.total_isk = Some(100_000_000.0);
        assert_eq!(battle_decision(&rules, &small), Some(RuleAction::Exclude));
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
        assert!(hull_only.is_broad());
        assert!(hull_only.admits_ingest(&MatchData { max_size: ShipSize::Battleship, ..Default::default() }));
        let isk_only = BattleRule {
            action: RuleAction::Include,
            match_all: true,
            conditions: vec![BattleCond::IskAtLeast(1.0)],
            expanded: false,
        };
        assert!(!isk_only.admits_ingest(&MatchData::default()));
        let located = BattleRule {
            action: RuleAction::Include,
            match_all: true,
            conditions: vec![BattleCond::Region("Delve".into())],
            expanded: false,
        };
        assert!(!located.is_broad());
    }
}
