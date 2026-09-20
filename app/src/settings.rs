use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub nav_expanded: bool,
    /// Pilot picked in the top bar. Persisted so a restart resumes on the same character;
    /// cleared on load if that character is no longer authed.
    #[serde(default)]
    pub active_character: String,
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
    /// Systems the route planner always goes around, kept separately for the two kinds of route: a
    /// system you will not gate through is often perfectly fine to jump over, and the other way
    /// round. Added with `serde(default)`, never retyped: a changed field type fails the whole parse
    /// and resets every setting there is.
    #[serde(default)]
    pub route_avoid_gate: Vec<i64>,
    #[serde(default)]
    pub route_avoid_jump: Vec<i64>,
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
    pub saved_map_routes: Vec<SavedMapRoute>,
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
    #[serde(default = "default_web")]
    pub web: WebSettings,
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
    /// Folder uuid quick note and tag edits go to. Empty or stale falls back to the Default folder.
    #[serde(default)]
    pub notes_folder: String,
    /// The user's colours for built-in tags, by tag id.
    #[serde(default)]
    pub tag_colors: std::collections::BTreeMap<String, [u8; 3]>,
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
    /// Rooms the user deliberately left. We never rejoin these ourselves; a server-side force-join
    /// still wins and clears the entry.
    #[serde(default)]
    pub jabber_left_rooms: Vec<String>,
    /// Rooms and private chats the user removed from the sidebar. Their stored messages are kept,
    /// so rejoining or a new message restores the backlog; only the listing is suppressed.
    #[serde(default)]
    pub jabber_forgotten: Vec<String>,
    /// The main chat window's open tabs and selected tab, so a restart restores the tab bar the
    /// user left rather than rebuilding it from every room and DM the app knows about. Pop-outs
    /// keep theirs in `jabber_popout_windows`. Empty `active` means the Fleet pings pseudo-tab.
    #[serde(default)]
    pub jabber_main_tabs: Vec<String>,
    #[serde(default)]
    pub jabber_main_active: String,
    /// Last-known room MOTD (MUC subject) per room JID, so history-only channels still show it.
    #[serde(default)]
    pub jabber_room_subjects: std::collections::BTreeMap<String, String>,
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

    // --- Fleet dashboard (off by default; Imperium-specific, behind the `fleet` build feature) ---
    // Not cfg-gated, like the rescue fields below: settings are rewritten whole on save, so a build
    // without the feature still has to round-trip a config written by one with it.
    #[serde(default)]
    pub fleet_enabled: bool,
    /// Labelled fleet presets, the app's own copy of what the site keeps in localStorage.
    #[serde(default)]
    pub fleet_presets: Vec<FleetPreset>,
    /// Character the dashboard acts as.
    #[serde(default)]
    pub fleet_character: String,
    /// Which boosts each doctrine wants, and how badly.
    #[serde(default)]
    pub fleet_boost_requirements: Vec<FleetBoostRequirement>,
    /// Formup systems chosen other than the staging one, newest first, capped at three.
    #[serde(default)]
    pub fleet_recent_formup: Vec<String>,
    /// Doctrines the user added by hand, for boosts the dashboard's setup list does not cover.
    /// `(setup id, name)`, with negative ids so they cannot collide with the dashboard's.
    #[serde(default)]
    pub fleet_custom_doctrines: Vec<(i32, String)>,
    /// Which hulls belong to which doctrine, and which are welcome in any fleet.
    #[serde(default)]
    pub fleet_hulls: Vec<FleetHull>,
    /// How each doctrine tanks: `(setup id, "shield" | "armor")`. A fleet reps one way, so this
    /// belongs to the doctrine rather than to each of its hulls.
    #[serde(default)]
    pub fleet_doctrine_tanks: Vec<(i32, String)>,
    /// Where each doctrine is written up: `(setup id, url)`, for pasting into a ping.
    #[serde(default)]
    pub fleet_doctrine_urls: Vec<(i32, String)>,
    /// Setups that take their own hulls and nothing else. Some fleets are restricted enough that
    /// even a cyno or a bridging titan is out of place.
    #[serde(default)]
    pub fleet_doctrine_strict: Vec<i32>,

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

    #[serde(default = "default_rescue_op_channel")]
    pub rescue_op_channel: u8,
    #[serde(default = "default_rescue_template")]
    pub rescue_ping_template: String,
    /// Label of the fleet preset the rescue ping is built from. Empty until one is picked.
    #[serde(default)]
    pub rescue_preset: String,
    /// The cap-save template has been turned into a fleet preset, so it is not done twice.
    #[serde(default)]
    pub rescue_preset_seeded: bool,
    /// skirmish_commanders room JID: where the FC posts `!bping <group>` ping requests and watches
    /// the responses. coord/fc/all are directorbot ping groups, not separate rooms.
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
    /// Pilot tag ids, any of which on any reported pilot. `notes::ANY_TAG` is any tag. A deleted tag's
    /// id stays and matches nothing, so the rule fails closed.
    #[serde(default)]
    pub pilot_tags: Vec<String>,
    #[serde(default)]
    pub system_tags: Vec<String>,
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
            pilot_tags: Vec::new(),
            system_tags: Vec::new(),
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

/// The remote web view: an opt-in local server that mirrors the intel feed, alerts, fleet pings and
/// the map to a browser on the same network.
///
/// Everything web lives in this one sub-struct on purpose. `Store::load_settings` fails the whole
/// parse on a single bad field and returns None, so a key added or retyped at the `Settings` top
/// level is how every setting a user has gets reset. Growing inside here keeps that blast radius at
/// zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSettings {
    pub enabled: bool,
    pub port: u16,
    /// Bind 0.0.0.0 so a phone on the same network can reach it; false binds loopback only.
    pub bind_lan: bool,
    /// Whether the page may write back (pilot verdicts, alert acknowledgements) or is read-only.
    pub allow_writeback: bool,
    /// Seeds a browser's first visit only. The real layout is per device, in the browser.
    pub default_layout: WebLayout,
    /// Pairing secret, generated on first enable. Deliberately not in the OS keyring: that holds ESI
    /// refresh tokens, which reach a player's EVE account, while this reaches a LAN page on a machine
    /// an attacker is already on. Keeping it here also means the settings export carries it.
    pub token: String,
    /// An address to bind instead of the one `bind_lan` picks. Empty means the normal choice, which
    /// is what every user gets unless they go looking.
    #[serde(default)]
    pub bind_addr: String,
    /// Serve without pairing. Off, and it stays off unless someone reads the warning and says yes:
    /// this is the switch that puts alliance intel and private conversations on an open socket.
    #[serde(default)]
    pub no_pairing: bool,
    /// Whether the warning behind the advanced options has been shown and accepted.
    #[serde(default)]
    pub advanced_ack: bool,
}

impl Default for WebSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 6767,
            bind_lan: true,
            allow_writeback: true,
            default_layout: WebLayout::Auto,
            token: String::new(),
            bind_addr: String::new(),
            no_pairing: false,
            advanced_ack: false,
        }
    }
}

#[cfg(test)]
mod saved_routes {
    /// A route through a scanned wormhole goes stale, and the reader is what drops it.
    ///
    /// The chain it was planned on is hours old at best; a day later the route is not a route, and
    /// silently serving it is worse than losing it. An ordinary route is kept forever.
    #[test]
    fn only_wormhole_routes_expire() {
        let now = 1_700_000_000_i64;
        let keep = |r: &super::SavedMapRoute| {
            !r.via_wormholes || now - r.saved_at < super::WORMHOLE_ROUTE_TTL_SECS
        };
        let base = super::SavedMapRoute {
            name: "x".to_owned(),
            kind: "gate".to_owned(),
            anchors: vec![1, 2],
            avoid: Vec::new(),
            titans: Vec::new(),
            titan_at_start: true,
            titan_self_jump: false,
            hull: 0,
            jdc: 5,
            jfc: 5,
            saved_at: now - 10 * super::WORMHOLE_ROUTE_TTL_SECS,
            via_wormholes: false,
        };
        assert!(keep(&base), "a gate route from last week is still a gate route");
        let stale = super::SavedMapRoute { via_wormholes: true, ..base.clone() };
        assert!(!keep(&stale), "a day-old wormhole route is gone");
        let fresh = super::SavedMapRoute {
            via_wormholes: true,
            saved_at: now - 60,
            ..base
        };
        assert!(keep(&fresh), "a minute-old one is not");
    }
}

#[cfg(test)]
mod web_defaults {
    /// The three settings that can put the page on the open internet are off, and a blob written
    /// before they existed still parses to off.
    ///
    /// Worth a test of its own rather than trusting `Default`: each one is a way to hand alliance
    /// intel and private conversations to anyone who can reach the port.
    #[test]
    fn exposure_is_never_the_default() {
        let d = super::WebSettings::default();
        assert!(!d.no_pairing, "pairing is required unless the user turns it off");
        assert!(d.bind_addr.is_empty(), "no hand-picked bind address");
        assert!(!d.advanced_ack, "the warning has not been accepted for anyone");
        assert!(!d.enabled, "the whole feature is off");

        // A blob from before these existed, which is what every upgrading user has.
        let old = r#"{"enabled":true,"port":6767,"bind_lan":true,"allow_writeback":true,
                      "default_layout":"Auto","token":"abc"}"#;
        let parsed: super::WebSettings = serde_json::from_str(old).expect("old blobs still parse");
        assert!(!parsed.no_pairing);
        assert!(parsed.bind_addr.is_empty());
        assert!(!parsed.advanced_ack);
        assert_eq!(parsed.token, "abc", "and the rest of it survives");
    }
}

fn default_web() -> WebSettings {
    WebSettings::default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebLayout {
    /// Columns on a wide screen, swipeable tabs on a narrow one.
    #[default]
    Auto,
    Tabs,
    Columns,
    Grid,
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

/// A route built on the map, saved whole.
///
/// Everything that went into it, not just the endpoints: a route is the anchors *and* what you told
/// the planner about them, and reloading one that lost its avoid list or its titans would be a
/// different route with the same name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedMapRoute {
    pub name: String,
    pub kind: String,
    pub anchors: Vec<i64>,
    #[serde(default)]
    pub avoid: Vec<i64>,
    #[serde(default)]
    pub titans: Vec<i64>,
    #[serde(default)]
    pub titan_at_start: bool,
    #[serde(default)]
    pub titan_self_jump: bool,
    #[serde(default)]
    pub hull: usize,
    #[serde(default)]
    pub jdc: u32,
    #[serde(default)]
    pub jfc: u32,
    /// When it was saved, for the wormhole expiry.
    #[serde(default)]
    pub saved_at: i64,
    /// Whether it was planned through scanned wormholes. Those move: a chain is hours old at best, so
    /// a route that depended on one is deleted a day later rather than quietly becoming wrong.
    #[serde(default)]
    pub via_wormholes: bool,
}

/// How long a route through a scanned wormhole is worth keeping.
pub const WORMHOLE_ROUTE_TTL_SECS: i64 = 24 * 60 * 60;

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
    br_core::battle::BATTLE_BREAK_SECS
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

/// A labelled fleet preset: one click fills the whole start-fleet form.
///
/// Plain scalars rather than the `fleets::` id newtypes, and it lives here rather than in `fleets`,
/// because a build without the `fleet` feature still has to parse and rewrite this.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FleetPreset {
    /// What the chip says. The only part the user names.
    pub label: String,
    /// Which folder it sits in, empty for the top level. One level only: a tree of fleet presets
    /// is a thing to navigate, and the point of a preset is not navigating.
    pub folder: String,
    pub name: String,
    pub description: String,
    pub setup_id: i32,
    pub group_id: Option<i32>,
    pub mumble_channel_id: Option<i32>,
    pub logi_channel_id: Option<i32>,
    pub boost_channel_id: Option<i32>,
    /// Let the free-channel rule choose, keeping the ids above only while they are still free.
    pub auto_channels: bool,
    /// 0 start, 1 FC left.
    pub auto_close_type: i32,
    pub auto_close_time: i32,
    pub is_corporation_fleet: bool,
    pub ignore_participation_requirements: bool,
    pub set_motd: bool,
    pub doctrine_notes: String,
    pub tag_ids: Vec<i32>,
    pub use_backup: bool,
    /// (solar system id, name).
    pub formup_location: Option<(i64, String)>,
    /// (character id, name, snowflake type).
    pub snowflakes: Vec<(i64, String, u8)>,
}

/// The secondary tag that marks a preset as one a capital rescue runs on. A rescue picks from
/// these and nothing else, so an FC under pressure is not scrolling past every roam they saved.
#[cfg(feature = "fc-rescue")]
pub const CAPITAL_SAVE_TAG: i32 = 28;

/// Turns the old free-text cap-save template into a fleet preset, once.
///
/// The template already carried everything a preset does: a name, a formup location, a comms
/// channel and a doctrine line. It only lacked somewhere to live.
#[cfg(feature = "fc-rescue")]
pub fn seed_rescue_preset(s: &mut Settings) -> bool {
    if s.rescue_preset_seeded
        || s.fleet_presets.iter().any(|p| p.tag_ids.contains(&CAPITAL_SAVE_TAG))
    {
        s.rescue_preset_seeded = true;
        return false;
    }
    let label = "Capital Save".to_owned();
    s.fleet_presets.push(FleetPreset {
        label: label.clone(),
        folder: "Rescue".to_owned(),
        name: "CAP Save".to_owned(),
        description: "Give me a titan on standby".to_owned(),
        mumble_channel_id: Some(i32::from(s.rescue_op_channel)),
        auto_close_type: 1,
        auto_close_time: 30,
        set_motd: true,
        // Strategic, because a capital is on the field, and the tag that makes it a rescue preset.
        tag_ids: vec![1, CAPITAL_SAVE_TAG],
        ..FleetPreset::default()
    });
    s.rescue_preset = label;
    s.rescue_preset_seeded = true;
    true
}

/// A hull that belongs somewhere: in one doctrine, or in any fleet at all.
///
/// The dashboard's setup list carries no hulls, so without this everything a fleet flies reads as
/// out of doctrine. By name rather than by type id because the name is what the composition
/// carries and what the user types.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FleetHull {
    /// The setup this belongs to, or 0 for a hull welcome in any fleet.
    pub setup_id: i32,
    /// The hull's type id, which is what a composition is matched on. 0 for a row written before
    /// ids were stored, which still matches by name.
    #[serde(default)]
    pub type_id: i64,
    pub name: String,
}

/// One boost a doctrine wants, and how badly, so the tracking view can say what to put on next.
///
/// `priority` is a string rather than an enum for the same reason `FleetPreset` uses plain scalars:
/// a build without the `fleet` feature rewrites this file whole, and an unknown value written by a
/// later version must not fail the parse and reset every other setting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FleetBoostRequirement {
    pub setup_id: i32,
    /// The charge's name as the fitting window gives it, without the trailing "Charge".
    pub charge: String,
    /// "high", "medium" or "low".
    pub priority: String,
}


/// Accept both the old form (a list of plain name strings) and the new `{name, description}` form,
/// so a config saved before descriptions existed still loads instead of resetting all settings.

fn default_rescue_channel() -> String {
    "delve911".to_owned()
}
fn default_rescue_staging() -> String {
    "C-J6MT".to_owned()
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
            active_character: String::new(),
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
            alert_only_undocked: false,
            kill_intel: true,
            kill_intel_jumps: default_kill_jumps(),
            intel_ttl_secs: 300,
            intel_count_bridges: false,
            route_avoid_gate: Vec::new(),
            route_avoid_jump: Vec::new(),
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
            saved_map_routes: Vec::new(),
            jump_dock: Vec::new(),
            coalitions: default_coalitions(),
            view_options: String::new(),
            alliances: Vec::new(),
            severity: SeverityRules::default(),
            alerts: AlertSettings::default(),
            web: WebSettings::default(),
            battles_enabled: true,
            battles: BattleFilter::default(),
            min_battle_isk: 0.0,
            battle_break_secs: default_battle_break(),
            bookmarks: Vec::new(),
            notes_folder: String::new(),
            tag_colors: Default::default(),
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
            jabber_left_rooms: Vec::new(),
            jabber_forgotten: Vec::new(),
            jabber_main_tabs: Vec::new(),
            jabber_main_active: String::new(),
            jabber_inaccessible_rooms: Vec::new(),
            jabber_room_subjects: std::collections::BTreeMap::new(),
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
            fleet_enabled: false,
            fleet_presets: Vec::new(),
            fleet_character: String::new(),
            fleet_boost_requirements: Vec::new(),
            fleet_recent_formup: Vec::new(),
            fleet_custom_doctrines: Vec::new(),
            fleet_hulls: Vec::new(),
            fleet_doctrine_tanks: Vec::new(),
            fleet_doctrine_urls: Vec::new(),
            fleet_doctrine_strict: Vec::new(),
            fc_rescue_enabled: false,
            rescue_channel: default_rescue_channel(),
            rescue_staging_system: default_rescue_staging(),
            cyno_generators: Vec::new(),
            rescue_op_channel: default_rescue_op_channel(),
            rescue_ping_template: default_rescue_template(),
            rescue_preset: String::new(),
            rescue_preset_seeded: false,
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
    fn active_character_roundtrips_and_legacy_configs_still_parse() {
        let s = Settings { active_character: "Amryu".to_owned(), ..Default::default() };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active_character, "Amryu");
        // A config written before the field existed must still deserialize whole: one field that
        // fails to parse aborts all of Settings and silently resets every other setting.
        let legacy: Settings =
            serde_json::from_str(r#"{"jabber_jid":"a@b","nav_expanded":true}"#).unwrap();
        assert_eq!(legacy.jabber_jid, "a@b");
        assert!(legacy.nav_expanded);
        assert_eq!(legacy.active_character, "");
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

    /// The fleet fields are not feature-gated, so a config written by a `fleet` build has to load
    /// and save unchanged in a build without it. Settings are rewritten whole, so losing them here
    /// would silently drop every preset the moment the user ran a stock binary.
    #[test]
    fn a_fleet_config_round_trips_in_any_build() {
        let mut s = Settings::default();
        s.fleet_enabled = true;
        s.fleet_character = "Amryu".to_owned();
        s.fleet_recent_formup = vec!["1DQ1-A".to_owned(), "319-3D".to_owned()];
        s.fleet_custom_doctrines = vec![(-1, "Shield Cruisers".to_owned())];
        s.fleet_doctrine_tanks = vec![(46, "shield".to_owned())];
        s.fleet_doctrine_urls = vec![(46, "https://example.invalid/doctrine".to_owned())];
        s.fleet_doctrine_strict = vec![19];
        s.fleet_hulls = vec![
            FleetHull { setup_id: 46, type_id: 11_381, name: "Harpy".to_owned() },
            FleetHull { setup_id: 0, type_id: 11_957, name: "Falcon".to_owned() },
        ];
        s.fleet_boost_requirements = vec![FleetBoostRequirement {
            setup_id: 46,
            charge: "Shield Extension".to_owned(),
            priority: "high".to_owned(),
        }];
        s.fleet_presets = vec![FleetPreset {
            label: "Home Defence".to_owned(),
            folder: "Home".to_owned(),
            name: "Home Defense".to_owned(),
            setup_id: 46,
            auto_channels: true,
            auto_close_type: 1,
            auto_close_time: 30,
            tag_ids: vec![1, 12],
            snowflakes: vec![(90_000_001, "Scout Alt".to_owned(), 4)],
            formup_location: Some((30_000_772, "C-J6MT".to_owned())),
            ..FleetPreset::default()
        }];
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        // Field by field, not the whole struct: `BattleRule.expanded` is a UI flag that is not
        // serialised, so a whole-Settings equality can never hold.
        assert_eq!(back.fleet_enabled, s.fleet_enabled);
        assert_eq!(back.fleet_character, s.fleet_character);
        assert_eq!(back.fleet_presets, s.fleet_presets);
        assert_eq!(back.fleet_boost_requirements, s.fleet_boost_requirements);
        assert_eq!(back.fleet_recent_formup, s.fleet_recent_formup);
        assert_eq!(back.fleet_custom_doctrines, s.fleet_custom_doctrines);
        assert_eq!(back.fleet_hulls, s.fleet_hulls);
        assert_eq!(back.fleet_doctrine_tanks, s.fleet_doctrine_tanks);
        assert_eq!(back.fleet_doctrine_urls, s.fleet_doctrine_urls);
        assert_eq!(back.fleet_doctrine_strict, s.fleet_doctrine_strict);
    }

    /// The cap-save template becomes a preset once, and never overwrites one that exists.
    #[cfg(feature = "fc-rescue")]
    #[test]
    fn the_rescue_template_becomes_a_preset_once() {
        let mut s = Settings::default();
        s.rescue_op_channel = 4;
        assert!(seed_rescue_preset(&mut s));
        assert_eq!(s.fleet_presets.len(), 1);
        let p = &s.fleet_presets[0];
        assert!(p.tag_ids.contains(&CAPITAL_SAVE_TAG));
        assert_eq!(p.mumble_channel_id, Some(4), "the op channel it was set to came across");
        assert_eq!(s.rescue_preset, p.label);
        assert!(s.rescue_preset_seeded);

        // Twice does nothing.
        assert!(!seed_rescue_preset(&mut s));
        assert_eq!(s.fleet_presets.len(), 1);

        // And an existing capital-save preset is left alone even on a fresh flag.
        let mut other = Settings::default();
        other.fleet_presets = vec![FleetPreset {
            label: "Mine".to_owned(),
            tag_ids: vec![CAPITAL_SAVE_TAG],
            ..FleetPreset::default()
        }];
        assert!(!seed_rescue_preset(&mut other));
        assert_eq!(other.fleet_presets.len(), 1);
        assert_eq!(other.fleet_presets[0].label, "Mine");
        assert!(other.rescue_preset_seeded);
    }

    /// A config written before the tab existed must not fail the parse, which would reset every
    /// other setting there is.
    #[test]
    fn a_config_without_the_fleet_fields_still_parses() {
        let s: Settings = serde_json::from_str(r#"{"jabber_jid":"a@b"}"#).unwrap();
        assert_eq!(s.jabber_jid, "a@b");
        assert!(!s.fleet_enabled);
        assert!(s.fleet_presets.is_empty());
        assert!(s.fleet_boost_requirements.is_empty());
        assert!(s.fleet_recent_formup.is_empty());
        assert!(s.fleet_custom_doctrines.is_empty());
        assert!(s.fleet_hulls.is_empty());
        assert!(s.fleet_doctrine_tanks.is_empty());
        assert!(s.fleet_doctrine_urls.is_empty());
        assert!(s.fleet_doctrine_strict.is_empty());
        assert!(s.rescue_preset.is_empty());
        assert!(!s.rescue_preset_seeded);
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

/// The tab bar is persisted through `Store::save_settings` / `load_settings`, which are a plain
/// `serde_json` round-trip. These cover that link without pointing a test at a profile on disk.
#[cfg(test)]
mod tab_persistence_tests {
    use super::*;

    const ROOM: &str = "delve@conference.goonfleet.com";
    const DM: &str = "someguy@goonfleet.com";

    fn round_trip(s: &Settings) -> Settings {
        serde_json::from_str(&serde_json::to_string(s).expect("serialize")).expect("deserialize")
    }

    #[test]
    fn the_open_tabs_survive_a_save_and_load() {
        let mut s = Settings::default();
        s.jabber_main_tabs = vec![ROOM.to_owned(), DM.to_owned()];
        s.jabber_main_active = DM.to_owned();
        let back = round_trip(&s);
        assert_eq!(back.jabber_main_tabs, vec![ROOM.to_owned(), DM.to_owned()]);
        assert_eq!(back.jabber_main_active, DM);
    }

    #[test]
    fn a_config_written_before_the_feature_still_loads() {
        let s: Settings = serde_json::from_str(r#"{"jabber_rooms":["a@conference.x"]}"#)
            .expect("an older config must not fail the parse");
        assert!(s.jabber_main_tabs.is_empty());
        assert!(s.jabber_main_active.is_empty());
        assert_eq!(s.jabber_rooms, vec!["a@conference.x".to_owned()]);
    }

    /// An unknown key such as the removed `jabber_close_room_leaves` must be ignored, not fail the
    /// whole parse: `load_settings` falls back to defaults on a parse error, which would reset
    /// every setting the user has.
    #[test]
    fn a_config_carrying_the_removed_key_still_loads() {
        let s: Settings = serde_json::from_str(
            r#"{"jabber_close_room_leaves":true,"jabber_rooms":["a@conference.x"]}"#,
        )
        .expect("a dropped field must not fail the parse");
        assert_eq!(s.jabber_rooms, vec!["a@conference.x".to_owned()]);
    }

    /// The persisted Jabber lists, together, so none of them is the one that silently resets.
    /// Field by field, not whole-struct: some UI-transient flags are deliberately not persisted
    /// (`BattleRule::expanded` among them) and would fail an equality check for the wrong reason.
    #[test]
    fn the_jabber_lists_all_round_trip() {
        let mut s = Settings::default();
        s.jabber_closed_rooms = vec!["a".to_owned()];
        s.jabber_closed_dms = vec!["b".to_owned()];
        s.jabber_left_rooms = vec!["c".to_owned()];
        s.jabber_forgotten = vec!["d".to_owned()];
        s.jabber_main_tabs = vec!["e".to_owned()];
        s.jabber_main_active = "e".to_owned();
        let b = round_trip(&s);
        assert_eq!(b.jabber_closed_rooms, s.jabber_closed_rooms);
        assert_eq!(b.jabber_closed_dms, s.jabber_closed_dms);
        assert_eq!(b.jabber_left_rooms, s.jabber_left_rooms);
        assert_eq!(b.jabber_forgotten, s.jabber_forgotten);
        assert_eq!(b.jabber_main_tabs, s.jabber_main_tabs);
        assert_eq!(b.jabber_main_active, s.jabber_main_active);
    }
}

#[cfg(test)]
mod web_settings_tests {
    use super::*;

    #[test]
    fn web_settings_roundtrip_and_default_to_off_on_6767() {
        let d = Settings::default();
        assert!(!d.web.enabled, "the server must never start unasked");
        assert_eq!(d.web.port, 6767);
        assert!(d.web.bind_lan);
        assert!(d.web.allow_writeback);
        assert_eq!(d.web.default_layout, WebLayout::Auto);
        assert!(d.web.token.is_empty(), "the token is minted on first enable, not at rest");

        let s = Settings {
            web: WebSettings {
                bind_addr: String::new(),
                no_pairing: false,
                advanced_ack: false,
                enabled: true,
                port: 9000,
                bind_lan: false,
                allow_writeback: false,
                default_layout: WebLayout::Grid,
                token: "tok".to_owned(),
            },
            ..Default::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.web, s.web);
    }

    /// The whole reason the web keys live in one sub-struct: `Store::load_settings` fails the entire
    /// parse on one bad field, so a blob written before this feature existed has to keep loading, and
    /// every other setting in it has to survive.
    #[test]
    fn a_settings_blob_written_before_the_web_feature_still_loads() {
        let legacy = r#"{"jabber_jid":"pilot@example.com","intel_ttl_secs":900,"nav_expanded":true}"#;
        let back: Settings = serde_json::from_str(legacy).unwrap();
        assert_eq!(back.jabber_jid, "pilot@example.com");
        assert_eq!(back.intel_ttl_secs, 900);
        assert!(back.nav_expanded);
        assert_eq!(back.web, WebSettings::default());
    }
}
