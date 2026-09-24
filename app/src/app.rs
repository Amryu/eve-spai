pub fn app_icon() -> std::sync::Arc<egui::IconData> {
    use std::sync::{Arc, OnceLock};
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| {
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/eve-spai.png"))
            .map(Arc::new)
            .unwrap_or_else(|_| Arc::new(egui::IconData { rgba: vec![0; 4], width: 1, height: 1 }))
    })
    .clone()
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum IntelTypeFilter {
    All,
    Hostile,
    Clear,
    Kill,
    Threat,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum SovMode {
    Off,
    Alliance,
    Coalition,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum ActivityMode {
    Off,
    ShipKills,
    PodKills,
    NpcKills,
    Jumps,
}

impl ActivityMode {
    fn to_u8(self) -> u8 {
        match self {
            ActivityMode::Off => 0,
            ActivityMode::ShipKills => 1,
            ActivityMode::PodKills => 2,
            ActivityMode::NpcKills => 3,
            ActivityMode::Jumps => 4,
        }
    }
    fn from_u8(n: u8) -> Self {
        match n {
            1 => ActivityMode::ShipKills,
            2 => ActivityMode::PodKills,
            3 => ActivityMode::NpcKills,
            4 => ActivityMode::Jumps,
            _ => ActivityMode::Off,
        }
    }
    fn value(self, f: &crate::systemstatus::SysFlags) -> u32 {
        match self {
            ActivityMode::Off => 0,
            ActivityMode::ShipKills => f.ship_kills,
            ActivityMode::PodKills => f.pod_kills,
            ActivityMode::NpcKills => f.npc_kills,
            ActivityMode::Jumps => f.jumps,
        }
    }
    fn scale(self) -> f32 {
        match self {
            ActivityMode::Jumps => 400.0,
            ActivityMode::NpcKills => 200.0,
            _ => 30.0,
        }
    }
    fn label(self) -> &'static str {
        match self {
            ActivityMode::Off => "off",
            ActivityMode::ShipKills => "ship kills",
            ActivityMode::PodKills => "pod kills",
            ActivityMode::NpcKills => "NPC kills",
            ActivityMode::Jumps => "jumps",
        }
    }
}

type SysHit = (i64, String, f64, String, String);

const JOVE_COLOR: egui::Color32 = egui::Color32::from_rgb(0xB8, 0x8C, 0xF0);

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct MapOverlays {
    sov: SovMode,
    bridges: bool,
    activity: ActivityMode,
    adm: bool,
    upgrades: bool,
    jump_range: bool,
    wormholes: bool,
    thera: bool,
    turnur: bool,
    camps: bool,
    #[serde(default)]
    cyno_gen: bool,
    #[serde(default)]
    jove: bool,
    #[serde(default = "overlay_on")]
    notes: bool,
    /// Ansiblex zones around the capital. Exclusive with `jump_range`: both colour the dots.
    #[serde(default)]
    ansiblex_zones: bool,
}

const MAP_TAG_MARKS: usize = 5;

fn overlay_on() -> bool {
    true
}

/// delve911 covers a titan bridge out of staging. Past this the fleet can't be dropped on the
/// target at all, which changes the answer from "form up" to "sorry".
/// Titan base jump range is 3.0 ly (live SDE), doubled by Jump Drive Calibration V.
#[cfg(feature = "fleet")]
const DELVE911_RANGE_LY: f64 = 6.0;

/// The jump-off system a titan can bridge to from `stage_pos` that leaves the fleet the fewest
/// jumps from `target`. Ties at the same jump count go to whichever is closest in lightyears.
/// Null-sec gate topology does not follow the map, so the geometrically nearest in-range system can
/// be a dozen gates out while a neighbour of the target sits in range.
///
/// Falls back to the lightyear-nearest candidate when nothing in range can reach the target within
/// `max_jumps`, so the out-of-range warning still names a system and reports no route rather than
/// disappearing.
#[cfg(feature = "fleet")]
fn best_jump_off<'a>(
    systems: &crate::geo::Systems,
    coords: &'a [crate::store::MapSystem],
    stage_pos: &crate::store::MapSystem,
    target: i64,
    target_pos: &crate::store::MapSystem,
    max_jumps: u32,
) -> Option<&'a crate::store::MapSystem> {
    let in_range: std::collections::HashSet<i64> = coords
        .iter()
        .filter(|s| s.id != target)
        .filter(|s| crate::map::ly_distance(stage_pos, s) <= DELVE911_RANGE_LY)
        .map(|s| s.id)
        .collect();
    let closest_ly = |ids: &dyn Fn(&crate::store::MapSystem) -> bool| {
        coords
            .iter()
            .filter(|s| ids(s))
            .min_by(|a, b| {
                crate::map::ly_distance(a, target_pos)
                    .total_cmp(&crate::map::ly_distance(b, target_pos))
            })
    };
    match systems.nearest_matching(target, max_jumps, |id| in_range.contains(&id)) {
        Some((_, hits)) => closest_ly(&|s| hits.contains(&s.id)),
        None => closest_ly(&|s| in_range.contains(&s.id)),
    }
}

/// Target sits outside titan range of staging: how far out, and the best jump-off system.
#[cfg(feature = "fleet")]
struct RangeWarning {
    ly_from_staging: f64,
    closest_name: String,
    /// Route from the jump-off system to the target. `None` = no route within the search cap.
    ansi_jumps: Option<u32>,
    gate_jumps: Option<u32>,
    ly_to_target: f64,
}

impl Default for MapOverlays {
    fn default() -> Self {
        Self {
            sov: SovMode::Off,
            bridges: true,
            activity: ActivityMode::Off,
            adm: false,
            upgrades: true,
            jump_range: true,
            wormholes: true,
            thera: false,
            turnur: true,
            camps: true,
            cyno_gen: false,
            jove: false,
            notes: true,
            ansiblex_zones: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MapMode {
    #[default]
    Standard,
    Travel,
    Hunting,
    Safety,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasteKind {
    Dscan,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum RouteView {
    #[default]
    ByFolder,
    ByName,
    BySystem,
}

#[derive(Clone)]
pub(crate) struct RouteItem {
    name: String,
    folder: String,
    from: i64,
    to: i64,
    jumps: usize,
    wp: usize,
}

impl MapMode {
    fn label(self) -> &'static str {
        match self {
            MapMode::Standard => "Standard",
            MapMode::Travel => "Travel",
            MapMode::Hunting => "Hunting",
            MapMode::Safety => "Safety",
        }
    }
    fn overlay_preset(self) -> MapOverlays {
        MapOverlays {
            sov: SovMode::Off,
            adm: false,
            upgrades: false,
            jump_range: false,
            wormholes: false,
            thera: false,
            turnur: false,
            camps: !matches!(self, MapMode::Standard),
            bridges: matches!(self, MapMode::Travel | MapMode::Hunting),
            activity: match self {
                MapMode::Standard => ActivityMode::Off,
                _ => ActivityMode::ShipKills,
            },
            cyno_gen: false,
            jove: false,
            // The user's own marks, which no mode has a reason to hide.
            notes: true,
            ansiblex_zones: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
struct PersistedView {
    overlays: MapOverlays,
    #[serde(default)]
    map_layout: crate::map::MapLayout,
    #[serde(default = "default_threat_jumps")]
    map_threat_jumps: u32,
    intel_max_jumps: u32,
    intel_type: IntelTypeFilter,
}

fn default_threat_jumps() -> u32 {
    5
}

mod notes_ui;
mod dscan_update;
mod wormholes_ui;
mod views;
mod lookup_ui;
mod web_glue;
mod settings_ui;
mod info_windows;
mod map_panels;
mod travel_ui;
mod map_ui;
mod map_route;
mod battles_ui;
pub(crate) mod fleet_ui;
#[cfg(feature = "fleet")]
pub(crate) mod fleet_map;
mod rescue_ui;
#[cfg(all(test, feature = "fleet"))]
pub(crate) use rescue_ui::ping_timer_row;
pub(crate) use settings_ui::TestIntel;
mod jabber_ui;
mod note_widgets;
pub(crate) use note_widgets::*;
mod intel_card;
pub(crate) use intel_card::*;
mod alert_window;
pub(crate) use alert_window::*;
mod chat_tabs;
pub(crate) use chat_tabs::*;
mod char_rings;
pub(crate) use char_rings::*;
mod alert_engine;
pub(crate) use alert_engine::*;
#[cfg(test)]
mod tests;

/// Fired alerts, newest last, as (time, text), for the dashboard.
type AlertLog = std::sync::Arc<std::sync::Mutex<Vec<(i64, String)>>>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum IntelClick {
    System(i64),
    Ship(i64),
    Pilot(String),
    Dscan(String),
    LocalScan(String),
    PilotVerdict(String),
    /// Open the note and tag editor for a system or pilot.
    Annotate(crate::notes::Subject),
    /// A quick edit from a chip's menu, applied as is.
    Notes(crate::notes::NotesOp),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RightDockTab {
    Mode,
    System,
    /// The route being planned. Its own tab, since the map has no jump-plan mode to host it.
    Route,
}

/// Which list the Jabber left sidebar shows.
///
/// `Convos` is everything you are actually talking in, DMs above rooms, newest first, so a direct
/// message is never behind a tab you are not looking at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JabberPane {
    Convos,
    Directory,
}

#[derive(Default)]
pub(crate) struct SystemInfoOut {
    nav: Option<i64>,
    show_on_map: bool,
    intel_click: Option<IntelClick>,
    open_const: Option<i64>,
    open_region: Option<i64>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PilotSort {
    MostLost,
    Recent,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PilotPane {
    #[default]
    Info,
    Ships,
    Kills,
    Solo,
    Losses,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FitMode {
    Recent,
    MostUsed,
}

impl IntelTypeFilter {
    fn matches(self, r: &crate::intel::IntelReport) -> bool {
        match self {
            IntelTypeFilter::All => true,
            IntelTypeFilter::Hostile => {
                !r.clear && !r.killmail && (r.count.is_some() || !r.systems.is_empty())
            }
            IntelTypeFilter::Clear => r.clear,
            IntelTypeFilter::Kill => r.killmail,
            IntelTypeFilter::Threat => r.spike || r.camp || r.bubble || r.cyno || r.dropper || r.help || r.tackled || r.cap_tackled,
        }
    }
}

use crate::auth::{self, AuthStatus, SharedAuth};
use crate::brview::RosterSort;
use crate::nav::{self, View};
use crate::sde::{self, SdeStatus, SharedStatus};
use crate::settings::Settings;
use crate::store::{CharacterRow, Store};
use crate::theme::{Rgb, Theme};

pub struct SpaiApp {
    store: Option<Store>,
    pub(crate) settings: Settings,
    pub(crate) view: View,
    pub(crate) intel_channels_open: bool,
    pub(crate) jump_bridges_open: bool,
    jb_paste: String,
    sov_upgrades_open: bool,
    sov_paste: String,
    pub(crate) coalitions_open: bool,
    pub(crate) severity_open: bool,
    pub(crate) coal_edit: Vec<(String, String)>,
    alliance_add: String,
    active_character: String,
    needs_save: bool,
    sde_status: SharedStatus,
    auth_status: SharedAuth,
    pub(crate) characters: Vec<CharacterRow>,
    copy_settings: crate::copysettings::CopyState,
    eve_clients: std::sync::Arc<std::sync::Mutex<crate::eveproc::Clients>>,
    eve_settings_path: std::sync::Arc<std::sync::Mutex<String>>,
    pub(crate) intel_state: std::sync::Arc<std::sync::Mutex<crate::intel::IntelState>>,
    watcher_started: bool,
    pub(crate) chat_dir: Option<std::path::PathBuf>,
    intel_query: String,
    intel_max_jumps: u32,
    intel_type: IntelTypeFilter,
    pub(crate) battles: crate::zkill::SharedBattles,
    battle_history: crate::zkill::SharedBattles,
    battle_history_loading: std::sync::Arc<std::sync::atomic::AtomicBool>,
    show_history: bool,
    pub(crate) battle_selected: Option<i64>,
    pub(crate) battle_detail_cache: Option<std::sync::Arc<crate::brview::BattleDetail>>,
    loaded_report: Option<LoadedReport>,
    report_msg: Option<String>,
    build_from_kill: crate::zkill::SharedBuildFromKill,
    build_kill_input: String,
    build_kill_error: Option<String>,
    battle_ship_ids: Option<std::sync::Arc<std::collections::HashSet<i64>>>,
    br_share: crate::brshare::SharedShare,
    /// The battle (by kid) the current share status belongs to, so the "Shared:" banner shows only
    /// on that report, not on whatever BR you navigate to next.
    br_share_kid: Option<i64>,
    br_mine: crate::brshare::SharedMine,
    br_mine_open: bool,
    br_unlisted: bool,
    br_character: Option<i64>,
    battle_search: String,
    battle_hover: Option<BattleHover>,
    battle_condensed: bool,
    battle_roster_sort: RosterSort,
    battle_filter: crate::zkill::SharedBattleFilter,
    ship_sizes: crate::zkill::ShipSizes,
    player_sys_shared: std::sync::Arc<std::sync::atomic::AtomicI64>,
    recent_wh: crate::zkill::RecentWh,
    work_throttle_shared: std::sync::Arc<std::sync::atomic::AtomicU8>,
    battles_enabled_shared: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) battle_filter_open: bool,
    pub(crate) filter_picker: Option<crate::pickers::FilterPicker>,
    verdict_popup: Option<String>,
    pub(crate) verdict_explainer_open: bool,
    filter_add_result: std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>,
    battle_filter_confirm_reset: bool,
    battle_overrides: crate::zkill::SharedOverrides,
    battle_break_shared: std::sync::Arc<std::sync::atomic::AtomicI64>,
    battle_overrides_gen_shared: std::sync::Arc<std::sync::atomic::AtomicU64>,
    battle_add_queue: std::sync::Arc<std::sync::Mutex<Vec<i64>>>,
    battle_excluded_count: usize,
    battle_scrub_count: usize,
    battle_edit_mode: bool,
    battle_kill_sel: std::collections::HashSet<i64>,
    battle_split_preview:
        Option<(std::collections::HashSet<i64>, br_core::battle::Battle, br_core::battle::Battle)>,
    battle_merge_sel: std::collections::HashSet<i64>,
    battle_add_open: bool,
    battle_add_link: String,
    battle_excluded_open: bool,
    battle_scrubs_open: bool,
    br_inputs: std::sync::Arc<std::sync::Mutex<crate::brview::BrInputs>>,
    br_outputs: std::sync::Arc<std::sync::Mutex<crate::brview::BrOutputs>>,
    /// The slowest frame of the last 120, in milliseconds: what the status bar reports as fps.
    frame_ms: f32,
    frame_worst: Vec<f32>,
    /// When the battles view last drew, so the worker can idle while nobody is looking.
    br_demand: std::sync::Arc<std::sync::atomic::AtomicU64>,
    br_wake: crate::brview::Wake,
    br_last_sent_sig: u64,
    battle_filter_gen_shared: std::sync::Arc<std::sync::atomic::AtomicU64>,
    // UI-side snapshots of the worker output, re-cloned only when its signature changes (never
    // per frame), so scrolling/rendering never clones the battle list or the open battle.
    battle_cards: Vec<(i64, Option<u32>, br_core::battle::Battle)>,
    battle_cards_total: usize,
    battle_cards_filtered: usize,
    battle_cards_ready: bool,
    battle_cards_out_sig: u64,
    battle_wait_since: Option<std::time::Instant>,
    battle_detail_out_sig: u64,
    camps: crate::camp::SharedCamps,
    camped_cache: Vec<(i64, crate::camp::CampLevel)>,
    camped_cache_at: i64,
    killfeed: crate::zkill::SharedKillFeed,
    ship_by_id: std::collections::HashMap<i64, String>,
    kills_loaded: bool,
    pub(crate) player: crate::esi::SharedPlayer,
    /// UI-thread state the web publisher cannot work out for itself, pushed down once a
    /// frame. Same arrangement as `AlertEngine::config`.
    web_facts: crate::web::facts::SharedFacts,
    web: crate::web::state::SharedWeb,
    web_detail: crate::web::Detail,
    web_inbox: crate::web::Inbox,
    /// The pairing QR and the link it encodes, so a changed token cannot leave a stale code.
    web_qr: Option<(String, egui::TextureHandle)>,
    /// Whether the token is on screen. Never persisted: a revealed secret should not survive a
    /// restart into the next time someone shares this pane.
    pub(crate) web_reveal: bool,
    web_server: Option<crate::web::server::Handle>,
    /// What the listener was started for. Restarting on a change is cheaper and clearer than
    /// reaching into a running server to re-read its settings.
    web_started_for: Option<u64>,
    /// A bind that failed, kept so settings can say why the page is unreachable rather than leaving
    /// the user to find it in a terminal they never opened.
    pub(crate) web_error: Option<String>,
    /// The UI context, for waking a frame when a worker finishes something the window is waiting on.
    ui_ctx: egui::Context,
    /// A listener being bound on a worker thread, and the settings hash it is being bound for.
    web_starting: Option<(u64, std::sync::mpsc::Receiver<Result<crate::web::server::Handle, String>>)>,
    /// A bind address being typed, and when it was last touched. The socket waits for the typing to
    /// stop: a half-typed address is a different address, and binding one per keystroke stalls the
    /// UI thread on the resolver.
    pub(crate) web_bind_draft: Option<(String, std::time::Instant)>,
    /// Off in the UI harness, which builds an app without any of the side effects.
    web_allowed: bool,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    bridges_applied: crate::ansiblex::BridgeKey,
    system_status: crate::systemstatus::SharedStatus,
    alerts_engine: std::sync::Arc<AlertEngine>,
    recent_alerts: AlertLog,
    alert_feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub(crate) alert_rules_open: bool,
    /// Lines the watcher reads as if EVE had logged them, for testing alert rules.
    pub(crate) intel_inject: crate::watcher::SharedInject,
    pub(crate) test_intel: Option<TestIntel>,
    alert_selected_rule: Option<u64>,
    rule_feeds:
        std::collections::HashMap<u64, Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)>>,
    alert_shared: SharedAlertWindow,
    alert_viewport_cb: std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    proc_monitor: crate::procstat::Monitor,
    pub(crate) jabber: crate::jabber::SharedJabber,
    jabber_tx: Option<crate::jabber::CmdSender>,
    jabber_chat: Option<String>,
    jabber_tabs: Vec<String>,
    /// Extra floating chat windows. `jabber_tabs`/`jabber_chat` stay the main window's storage.
    pub(crate) jabber_popouts: Vec<ChatWindow>,
    jabber_tab_drag: Option<TabDrag>,
    /// Main window's cached (outer, inner) screen rects, for cross-window drop hit-testing.
    jabber_main_rect: Option<(egui::Rect, egui::Rect)>,
    jabber_join_open: bool,
    /// Which half of the join dialog the user asked for, so it opens on the one they pressed.
    jabber_join_rooms: bool,
    pub(crate) jabber_drafts: std::collections::HashMap<String, String>,
    jabber_room_input: String,
    jabber_contact_search: String,
    jabber_dm_input: String,
    jabber_dm_error: String,
    jabber_pane: JabberPane,
    /// The count currently painted on the window icon, so it is only redrawn when it moves.
    taskbar_badge: Option<u32>,
    /// Conversations pinned into the Convos list for as long as the tab stays open, because they
    /// went unread while it was. Reading one must not make it disappear mid-click.
    jabber_sticky: std::collections::BTreeSet<String>,
    /// The room whose MOTD is open in its own window, if any.
    jabber_motd_window: Option<String>,
    jabber_collapsed: std::collections::HashSet<String>,
    jabber_my_presence: crate::jabber::Presence,
    jabber_my_status: String,
    jabber_pw_input: String,
    /// Fleet-ping history is paginated: render the newest N, load 50 more on scroll to bottom.
    jabber_pings_visible: usize,
    ping_rules_open: bool,
    /// Index of the fleet-ping rule whose config dialog is open, or `None`. Only one opens at a time.
    ping_rule_editing: Option<usize>,
    /// Comma-separated edit buffer for `jabber_mention_keywords`, so a half-typed "a," survives the
    /// round trip through the Vec.
    mention_input: String,
    pub(crate) session_start: i64,
    eve_focused: std::sync::Arc<std::sync::atomic::AtomicBool>,
    eve_focus_checked: Option<std::time::Instant>,
    ship_index: Option<std::sync::Arc<std::collections::HashMap<String, (i64, String)>>>,
    update: crate::update::SharedUpdate,
    update_checked_at: Option<std::time::Instant>,
    update_dismissed: bool,
    /// Set when the database can't be opened or written; drives a one-time warning.
    store_error: Option<String>,
    store_warn_dismissed: bool,
    /// Backoff after a failed settings save, so a full disk is not retried every frame.
    persist_retry_at: Option<std::time::Instant>,
    /// Dismissed at this level; cleared when the level worsens. Never persisted: a restart should
    /// re-warn, and on a full disk the settings write is itself likely to be what is failing.
    disk_banner_dismissed: Option<crate::disk::Level>,
    /// Snapshotted once per frame from `disk`'s process-global state, so rendering is a pure
    /// function of the app and a scene can set what it wants to draw.
    pub(crate) disk_level: crate::disk::Level,
    pub(crate) disk_free: Option<u64>,
    pub(crate) disk_saw_failure: bool,
    kill_cache: crate::kills::KillCache,
    kill_tx: Option<crate::kills::KillSender>,
    lookup_table: crate::localscan::SharedTable,
    /// The pilots on show, in paste order.
    lookup_current: Vec<String>,
    /// `None` sorts by name.
    lookup_sort: Option<lookup_ui::Col>,
    lookup_sort_desc: bool,
    lookup_note: Option<String>,
    /// A local scan link's pilots, fetched off the UI thread.
    lookup_incoming: std::sync::Arc<std::sync::Mutex<Option<Result<Vec<String>, String>>>>,
    /// Contact standings by character, corporation or alliance id, from ESI.
    standings: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, f32>>>,
    /// Which character the standings were last fetched for, and when.
    standings_for: Option<(String, std::time::Instant)>,
    intel_heights: std::collections::HashMap<u64, f32>,
    intel_heights_notes_rev: u64,
    /// Rendered height per chat row, so the history can skip over off-screen ones.
    jabber_msg_heights: std::collections::HashMap<u64, f32>,
    wizard_open: bool,
    wizard_step: u8,
    wizard_checked: bool,
    /// Result of the wizard's create-shortcut action: None = not tried, Some(Ok) = done.
    wizard_shortcut: Option<Result<(), String>>,
    tray: Option<crate::tray::TrayCmd>,
    really_exit: bool,
    raise_reset_top: bool,
    raise_main: bool,
    overlay: Option<crate::ipc::OverlayLink>,
    config_sent_hash: Option<u64>,
    dscan_clip: Option<arboard::Clipboard>,
    dscan_checked: Option<std::time::Instant>,
    dscan_seen_hash: u64,
    dscan_dismissed_hash: u64,
    dscan_prompt: Option<(String, usize, PasteKind)>,
    dscan_pos: Option<(f32, f32)>,
    dscan_link_used: bool,
    dscan_unfocused_at: Option<std::time::Instant>,
    dscan_share: std::sync::Arc<std::sync::Mutex<DscanShare>>,
    dscan_view: Option<DscanView>,
    pub(crate) wh_cache: Vec<crate::wormholes::Wormhole>,
    wh_reloaded: Option<std::time::Instant>,
    wh_overlay: WhOverlay,
    wh_filter_dest: Option<crate::wormholes::DestClass>,
    wh_filter_source: Option<crate::wormholes::Source>,
    wh_filter_expiring: bool,
    map_overlays: MapOverlays,
    map_mode: MapMode,
    standard_overlays: MapOverlays,
    travel_start: Option<i64>,
    travel_end: Option<i64>,
    travel_start_q: String,
    travel_end_q: String,
    travel_regional_gates: bool,
    travel_jump_bridges: bool,
    travel_avoid_camps: bool,
    travel_max_ship_kills: u32,
    travel_sec: [bool; 3],
    travel_start_sel: usize,
    travel_end_sel: usize,
    travel_sugg_key: (String, Option<i64>, String, Option<i64>),
    travel_sugg: (Vec<SysHit>, Vec<SysHit>),
    travel_wp_q: String,
    travel_wp_sel: usize,
    travel_wp_sugg_key: String,
    travel_wp_sugg: Vec<SysHit>,
    travel_metric: ActivityMode,
    travel_planned_hash: u64,
    travel_pending_hash: u64,
    travel_dirty_at: Option<f64>,
    travel_direct_route: Option<Vec<i64>>,
    travel_live: bool,
    travel_live_base: Option<Vec<i64>>,
    travel_changed: Vec<i64>,
    travel_changed_at: Option<i64>,
    travel_live_next: f64,
    /// The single in-game destination we last wrote (the next hop on the route), so we only
    /// re-write it when it changes. EVE rejects duplicate waypoints, so we advance one hop at a
    /// time instead of writing the whole (possibly self-revisiting) route at once.
    travel_ingame_dest: Option<i64>,
    travel_waypoints: Vec<i64>,
    pub(crate) routes_dialog_open: bool,
    route_save_name: String,
    route_save_folder: String,
    route_search: String,
    route_new_folder: String,
    route_view: RouteView,
    route_edit: Option<(String, String)>,
    route_edit_name: String,
    route_edit_folder: String,
    travel_avoid: Vec<i64>,
    travel_avoid_sov: std::collections::HashSet<String>,
    travel_sov_dialog_open: bool,
    travel_route: Option<Vec<i64>>,
    ctx_menu_system: Option<i64>,
    jump_ship: usize,
    jump_jdc: u32,
    jump_jfc: u32,
    jump_skills: crate::esi::SharedJumpSkills,
    jump_systems: Option<std::sync::Arc<Vec<crate::store::MapSystem>>>,
    map_view: crate::map::MapView,
    map_initialized: bool,
    map_history: Vec<crate::map::MapView>,
    map_forward: Vec<crate::map::MapView>,
    map_regions: Vec<(i64, String)>,
    map_systems: Vec<crate::store::MapSystem>,
    map_loaded: Option<crate::map::MapView>,
    map_pan: egui::Vec2,
    map_last_rect: Option<egui::Rect>,
    map_zoom: f32,
    map_follow: bool,
    map_follow_region: Option<(i64, i64)>,
    map_popped: bool,
    map_in_popout: bool,
    map_char_popouts: Vec<String>,
    map_char_view: std::collections::HashMap<
        String,
        (crate::map::MapView, egui::Vec2, f32, bool, Option<egui::Rect>),
    >,
    map_window_on_top: bool,
    map_controls_hidden: bool,
    map_overlay_mode: bool,
    map_overlay_locked: bool,
    map_vp_props: Option<(bool, bool)>,
    map_overlay_drag: bool,
    /// A route drag in flight: the system it started on.
    map_link: Option<i64>,
    /// Where the last frame put each system, so a drag can be hit-tested against where the user
    /// actually pressed rather than against positions the same frame's pan has already moved.
    map_pos_prev: std::collections::HashMap<i64, egui::Pos2>,
    /// The radial menu a finished route drag left behind: (from, to) and where to draw it.
    map_link_menu: Option<(i64, i64, egui::Pos2)>,
    /// The route the user picked, its alternatives, and which one is showing.
    map_route_opts: Vec<crate::web::route::RouteOption>,
    map_route_at: usize,
    map_route_kind: &'static str,
    /// The systems the drags named, in order: start, waypoints, destination.
    map_route_anchors: Vec<i64>,
    /// Whether the titan is in the system the route starts from. Off, it is waiting at the far end.
    map_titan_at_start: bool,
    /// Whether the titan may move itself first and have the fleet gate out to meet it.
    map_titan_self_jump: bool,
    /// This route's Ansiblex zone limit in place of the setting, until the app closes.
    map_route_zone: Option<u8>,
    /// The graph for `map_route_zone`, keyed by the base graph it was built from and the zone.
    map_route_graph: Option<(usize, u8, std::sync::Arc<crate::geo::Systems>)>,
    /// The ways of flying each leg, and which one is picked.
    map_route_legs: Vec<crate::web::route::LegChoice>,
    map_leg_pick: Vec<usize>,
    /// Which way to leave a system the route forks at, chosen in the route list.
    map_forks: crate::web::route::Picks,
    /// Systems avoided for this route only, kept apart from the two persistent lists.
    map_avoid_once: std::collections::HashSet<i64>,
    /// The system whose intel is being read from a route warning.
    map_intel_for: Option<i64>,
    /// Systems a titan is sitting in, for this route. Not a setting: which ships are where is a fact
    /// about the operation you are planning, not about the installation.
    map_titans: Vec<i64>,
    /// Systems offered as a stop between two hops, while that picker is open.
    map_alts: Option<Vec<i64>>,
    map_save_open: bool,
    map_save_name: String,
    map_load_open: bool,
    map_layout: crate::map::MapLayout,
    map_threat_jumps: u32,
    map_threat_center: Option<i64>,
    threat_include_bridges: bool,
    safety_prev: Option<std::collections::HashSet<i64>>,
    safety_last_scan: f64,
    sov_discover_last: f64,
    safety_prev_layout: Option<crate::map::MapLayout>,
    flash_until: f64,
    map_draw: Vec<crate::store::MapSystem>,
    map_draw_spaced: bool,
    map_draw_key: Option<(crate::map::MapView, bool)>,
    map_systems_cache: std::collections::HashMap<crate::map::MapView, Vec<crate::store::MapSystem>>,
    map_draw_cache:
        std::collections::HashMap<(crate::map::MapView, bool), Vec<crate::store::MapSystem>>,
    map_focus: Option<i64>,
    map_selected: Option<i64>,
    /// Which system the pointer has been resting on, and since when: the tooltip waits this out.
    map_hover_since: Option<(i64, std::time::Instant)>,
    /// Mean colour of a sov logo, by image URL, so the map dot can take the holder's colour.
    logo_avg: std::collections::HashMap<String, egui::Color32>,
    route_destination: Option<i64>,
    /// Whether this app set the character's in-game route, so clearing here can clear it there too.
    ingame_route: bool,
    map_search: String,
    map_search_sel: usize,
    map_search_key: String,
    map_search_sys: Vec<(i64, String, f64)>,
    map_search_const: Vec<(String, i64)>,
    map_search_reg: Vec<(i64, String)>,
    map_search_upgrades: Vec<String>,
    left_dock_open: bool,
    right_dock_open: bool,
    map_docked_system: Option<i64>,
    right_dock_tab: RightDockTab,
    upgrade_kinds: [bool; 4],
    map_highlight_upgrade: Option<String>,
    system_window: Option<i64>,
    system_kills_tab: bool,
    system_kills_cache: std::collections::HashMap<i64, crate::lookup::SharedLookup>,
    constellation_window: Option<i64>,
    region_window: Option<i64>,
    focus_window: Option<egui::ViewportId>,
    /// Overlay→main clicks that OPEN a dialog window, deferred to the next frame. Opening an
    /// immediate viewport in the same frame the IPC message was drained (the frame the overlay's
    /// reader thread woke via `request_repaint`) panics egui with "the user callback was never
    /// called"; processing them at the top of a normally-scheduled frame avoids that.
    pending_overlay_clicks: Vec<IntelClick>,
    ship_window: Option<i64>,
    pilot_query: String,
    pilot_lookup: crate::lookup::SharedLookup,
    pilot_window_open: bool,
    pilot_sort: PilotSort,
    pilot_pane: PilotPane,
    fit_view: Option<(i64, FitMode)>,
    fit_loss: Option<crate::lookup::Loss>,
    ping_shared: SharedPingWindow,
    ping_viewport_cb: std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    pilots: crate::pilot::SharedPilots,
    affiliations: crate::affiliation::SharedAffil,
    activity: crate::activity::SharedActivity,
    sightings: crate::intel::SharedSightings,
    revivals: crate::watcher::SharedRevivals,
    #[cfg(feature = "fleet")]
    fleet: std::sync::Arc<std::sync::Mutex<crate::fleets::FleetState>>,
    /// Answers the tab's questions. A dry run today, an HTTP client later, same trait.
    #[cfg(feature = "fleet")]
    fleet_backend: std::sync::Arc<dyn crate::fleets::backend::FleetBackend>,
    #[cfg(feature = "fleet")]
    fleet_login: crate::fleets::login::SharedLogin,
    /// The server's full text behind a failed fleet-boss check, while its dialog is open.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_boss_detail: Option<String>,
    #[cfg(feature = "fleet")]
    pub(crate) fleet_snowflakes_open: Option<crate::app::fleet_ui::SnowflakeTarget>,
    #[cfg(feature = "fleet")]
    pub(crate) fleet_migrate_open: bool,
    /// The fleet whose push stream is open, and the flag that stops its thread.
    #[cfg(feature = "fleet")]
    fleet_hub: Option<(crate::fleets::model::FleetId, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
    /// Set once a backend has said it has no hub, so the app stops trying and keeps polling.
    #[cfg(feature = "fleet")]
    fleet_hub_unavailable_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// A fleet to re-read, when to try next and how many tries are left.
    #[cfg(feature = "fleet")]
    fleet_reopen: Option<(crate::fleets::model::FleetId, std::time::Instant, u8)>,
    /// When the open fleet's Fleet Finder advert was last asked about.
    #[cfg(feature = "fleet")]
    fleet_advert_at: Option<(crate::fleets::model::FleetId, std::time::Instant)>,
    /// A character search waiting for the typing to pause, and when it is due.
    #[cfg(feature = "fleet")]
    fleet_search_pending: Option<(String, std::time::Instant)>,
    /// Short link to the `mumble://` link its page points at, filled in the background, with when
    /// it was last asked. `None` is one in flight or one that failed; a failure is retried after
    /// `COMMS_RETRY`, because gnf.lt answers some requests with an empty 400 and the next one fine.
    #[cfg(feature = "fleet")]
    pub(crate) comms_resolved: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, (Option<String>, std::time::Instant)>>,
    >,
    /// Which boost's breakdown is open, by charge name.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_boost_detail: Option<String>,
    /// `preset/op` the rescue last asked the dashboard to render, so it asks once per change.
    #[cfg(feature = "fleet")]
    pub(crate) rescue_preview_key: Option<String>,
    #[cfg(feature = "fleet")]
    pub(crate) rescue_preview_at: Option<std::time::Instant>,
    /// `(index, label, folder)` of the preset being renamed.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_preset_rename: Option<(usize, String, String)>,
    /// Whether the docked chat beside the fleet form is expanded, and which room it shows.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_chat_open: bool,
    #[cfg(feature = "fleet")]
    pub(crate) fleet_chat_tab: u8,
    /// What the FC has typed into the docked chat, per room.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_chat_draft: [String; 2],
    /// The last ping posted to Jabber: (group, body, when). Guards against a double click
    /// putting the same ping into skirmish_commanders twice.
    #[cfg(feature = "fleet")]
    fleet_last_ping: Option<(String, String, std::time::Instant)>,
    /// When the fleet-boss check was last asked for, so the refresh button cannot be held down.
    #[cfg(feature = "fleet")]
    fleet_boss_asked: Option<std::time::Instant>,
    #[cfg(feature = "fleet")]
    fleet_channels_at: Option<std::time::Instant>,
    /// One channel for the whole tab: several commands are in flight at once, so a single slot the
    /// way the web server's start does it would not do.
    #[cfg(feature = "fleet")]
    fleet_tx: std::sync::mpsc::Sender<(crate::fleets::state::Gen, crate::fleets::state::Outcome)>,
    #[cfg(feature = "fleet")]
    fleet_rx: std::sync::mpsc::Receiver<(crate::fleets::state::Gen, crate::fleets::state::Outcome)>,
    /// Bumped on every navigation, so a result for a page the user has left is dropped.
    #[cfg(feature = "fleet")]
    fleet_gen: crate::fleets::state::Gen,
    #[cfg(feature = "fleet")]
    pub(crate) fleet_booted: bool,
    /// When the form's next preview is due, so typing does not spawn a worker per keystroke.
    #[cfg(feature = "fleet")]
    fleet_preview_at: Option<std::time::Instant>,
    #[cfg(feature = "fleet")]
    pub(crate) fleet_journal_open: bool,
    /// When the tracked fleet's boost channel was last read off disk.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_boosts_read: Option<std::time::Instant>,
    /// Built for a screenshot or an assertion, so nothing may reach the real profile or its logs.
    #[cfg(feature = "fleet")]
    pub(crate) headless: bool,
    /// The per-doctrine boost editor window is open.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_boost_editor: bool,
    /// The tracked fleet's settings sidebar is open.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_sidebar_open: bool,
    /// The Quick Fleet preset picker is open.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_quick_open: bool,
    /// Where Mumble last said it was, as a `mumble://` URL.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_mumble_at: Option<String>,
    #[cfg(feature = "fleet")]
    fleet_mumble_asked: Option<std::time::Instant>,
    #[cfg(feature = "fleet")]
    fleet_mumble_tx: std::sync::mpsc::Sender<Option<String>>,
    #[cfg(feature = "fleet")]
    fleet_mumble_rx: std::sync::mpsc::Receiver<Option<String>>,
    /// Which half of a fleet's page is showing: who is in it, or what they are flying.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_detail_tab: crate::app::fleet_ui::DetailTab,
    #[cfg(feature = "fleet")]
    pub(crate) fleet_map: fleet_map::FleetMapView,
    /// Fleets whose movement is being recorded, each on its own thread.
    #[cfg(feature = "fleet")]
    fleet_trackers: std::collections::HashMap<
        crate::fleets::model::FleetId,
        (std::sync::Arc<std::sync::atomic::AtomicBool>, std::thread::JoinHandle<()>),
    >,
    #[cfg(feature = "fleet")]
    fleet_tracks_resumed: bool,
    /// A destructive action waiting to be confirmed: which fleet, what, and how much of the fleet
    /// it takes with it.
    #[cfg(feature = "fleet")]
    pub(crate) fleet_confirm: Option<(
        crate::fleets::model::FleetId,
        crate::fleets::backend::Action,
        &'static str,
        usize,
    )>,
    #[cfg(feature = "fleet")]
    rescue: std::sync::Arc<std::sync::Mutex<crate::rescue::RescueState>>,
    /// Highest rescue-event seq already surfaced into the ping feed (drained in `ui`).
    #[cfg(feature = "fleet")]
    rescue_feed_cursor: u64,
    /// SDE ship name (lowercased) -> group, shared with the chat-log watcher so the jabber ingest
    /// can resolve a hull named in a ping without rebuilding the map.
    #[cfg(feature = "fleet")]
    ship_groups: Option<std::sync::Arc<std::collections::HashMap<String, String>>>,
    /// Timestamp of the newest delve911 jabber message already parsed into `rescue`.
    #[cfg(feature = "fleet")]
    delve911_cursor: i64,
    notes: std::sync::Arc<crate::notes::NoteBook>,
    /// What `notes` means right now, rebuilt on every edit and shared with the alert engine.
    notes_view: std::sync::Arc<crate::notes::NotesView>,
    /// Why the last note edit was refused, shown in the editor and manager.
    notes_error: Option<String>,
    note_editor: Option<NoteDraft>,
    notes_manager: Option<notes_ui::NotesManager>,
    /// Set where `self` is still borrowed; opened on the next frame.
    note_editor_pending: Option<crate::notes::Subject>,
    /// Every system's 3D position, for the titan-range check. Loaded once with the SDE.
    #[cfg(feature = "fleet")]
    map_coords: Option<std::sync::Arc<Vec<crate::store::MapSystem>>>,
    /// (staging, target) the cached range check was computed for.
    #[cfg(feature = "fleet")]
    rescue_range_for: Option<(i64, i64)>,
    /// Set when the target sits outside titan range of staging.
    #[cfg(feature = "fleet")]
    rescue_range: Option<RangeWarning>,
    /// Cyno-generator list editing. Not rescue-gated: the generator map overlay is useful on its own.
    rescue_cyno_input: String,
    cyno_generators_open: bool,
    /// Set true to arm rescue mode (a delve911 ping arrived); the banner offers 1-click entry.
    #[cfg(feature = "fleet")]
    rescue_armed: bool,
    ship_cache: std::cell::RefCell<std::collections::HashMap<i64, Option<crate::store::ShipDetails>>>,
    ship_roles_cache: std::cell::RefCell<std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>>,
    type_names: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, String>>>,
    type_names_loading: std::sync::Arc<std::sync::Mutex<bool>>,
}

/// Which pilot should be active, given the pick restored from settings and the authed list.
fn resolve_active_character(current: &str, characters: &[CharacterRow]) -> String {
    // An empty list means nothing is authed *yet*, not that the remembered pilot is gone. Keeping
    // the name here is what stops a restored pick being discarded before the store has loaded.
    if characters.is_empty() {
        return current.to_owned();
    }
    // A pilot that has since been removed must not stay selected: the name would sit in the top
    // bar while every ESI call keyed off it failed.
    if current != "No character" && characters.iter().any(|c| c.name.eq_ignore_ascii_case(current))
    {
        return current.to_owned();
    }
    characters[0].name.clone()
}

impl SpaiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::build(&cc.egui_ctx, false)
    }

    /// `headless` skips every side effect this constructor normally performs: the image loaders,
    /// the single-instance control socket, opening the store, and all background threads
    /// (including the tray and the overlay subprocess). Everything that only shapes in-memory
    /// state still runs, so the resulting app renders the same as a live one.
    pub(crate) fn build(ctx: &egui::Context, headless: bool) -> Self {
        crate::theme::install_fonts(ctx);
        #[cfg(feature = "fleet")]
        let (fleet_tx, fleet_rx) = std::sync::mpsc::channel();
        #[cfg(feature = "fleet")]
        let fleet_mumble = std::sync::mpsc::channel();

        if !headless {
            crate::image_cache::install_image_loaders_cached(ctx);
            crate::instance::start_control_listener(ctx.clone());
        }

        // A headless build without the override would open the user's real profile, so it gets
        // no store at all rather than one pointed at live data.
        let (store, store_error) = if headless && std::env::var_os("EVE_SPAI_DATA_DIR").is_none() {
            (None, None)
        } else {
            match Store::open() {
                Ok(s) => match s.write_probe() {
                    Ok(()) => (Some(s), None),
                    // Opened read-only (file perms), so reads work but nothing persists.
                    Err(e) => (Some(s), Some(format!("the database is not writable ({e})"))),
                },
                Err(e) => {
                    eprintln!("store: {e:#}");
                    (None, Some(format!("{e:#}")))
                }
            }
        };
        let mut settings = store
            .as_ref()
            .and_then(|s| s.load_settings())
            .unwrap_or_default();
        let notes = std::sync::Arc::new(store.as_ref().map(|s| s.load_notes()).unwrap_or_default());
        let notes_view = std::sync::Arc::new(notes.view_with(&settings.notes_folder, &settings.tag_colors));

        settings.theme.apply(ctx);

        if !settings.alerts.seeded {
            settings.alerts.rules.insert(0, crate::settings::default_rule());
            settings.alerts.seeded = true;
        }
        crate::settings::ensure_rule_ids(&mut settings.alerts.rules);
        if !settings.jabber_ping_rules_seeded {
            if settings.jabber_ping_rules.is_empty() {
                settings.jabber_ping_rules = crate::settings::default_ping_rules();
            }
            settings.jabber_ping_rules_seeded = true;
        }
        if !settings.fleet_window_forced {
            settings.fleet_ping_window = true;
            settings.fleet_window_forced = true;
            if let Some(s) = &store {
                let _ = s.save_settings(&settings);
            }
        }
        let pv: PersistedView = serde_json::from_str(&settings.view_options).unwrap_or(PersistedView {
            overlays: MapOverlays::default(),
            map_layout: crate::map::MapLayout::Spaced,
            map_threat_jumps: 5,
            intel_max_jumps: 0,
            intel_type: IntelTypeFilter::All,
        });

        let initial = if store.as_ref().map(|s| s.sde_ready()).unwrap_or(false) {
            SdeStatus::Ready
        } else {
            SdeStatus::default()
        };
        let sde_status: SharedStatus = std::sync::Arc::new(std::sync::Mutex::new(initial));
        if !headless {
            crate::wormholes::spawn_scout(ctx.clone());
        }
        if let Some(store) = &store {
            if !headless && matches!(*sde_status.lock().unwrap(), SdeStatus::NotReady) {
                sde::spawn_download(store.path().to_path_buf(), sde_status.clone(), ctx.clone());
            }
        }

        let characters = store
            .as_ref()
            .map(|s| s.list_characters())
            .unwrap_or_default();

        let active_character = if settings.active_character.is_empty() {
            "No character".to_owned()
        } else {
            settings.active_character.clone()
        };

        let player: crate::esi::SharedPlayer =
            std::sync::Arc::new(std::sync::Mutex::new(crate::esi::Player::default()));
        if let Some(store) = &store {
            let _ = store;
            let cid = non_empty_or(&settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
            if !headless {
                crate::esi::spawn_location_poller(cid, player.clone(), ctx.clone());
                // Its own thread rather than a frame-driven poll: the window minimises to tray
                // while the kill firehose keeps writing, which is exactly when the disk fills.
                crate::disk::spawn_monitor(ctx.clone());
            }
        }

        let eve_clients: std::sync::Arc<std::sync::Mutex<crate::eveproc::Clients>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let eve_settings_path: std::sync::Arc<std::sync::Mutex<String>> =
            std::sync::Arc::new(std::sync::Mutex::new(settings.eve_settings_dir.clone()));
        if !headless {
            crate::eveproc::spawn_poller(eve_clients.clone(), eve_settings_path.clone(), ctx.clone());
        }

        let loaded_pings: Vec<crate::pings::Ping> = store
            .as_ref()
            .map(|s| {
                s.load_pings(2000).into_iter().filter_map(|j| serde_json::from_str(&j).ok()).collect()
            })
            .unwrap_or_default();
        for p in &loaded_pings {
            if let crate::pings::Ping::Fleet {
                comms: Some(crate::pings::Comms::Mumble { channel, link }),
                ..
            } = p
            {
                if let Some(k) = op_key(channel) {
                    settings.op_channel_links.entry(k).or_insert_with(|| link.clone());
                }
            }
        }
        let mut loaded_chats: std::collections::BTreeMap<String, Vec<crate::jabber::ChatMsg>> =
            std::collections::BTreeMap::new();
        if let Some(s) = &store {
            let mut purge: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (jid, sender, body, time, outgoing) in s.load_chats(5000) {
                if !valid_bare_jid(&jid) {
                    purge.insert(jid);
                    continue;
                }
                loaded_chats.entry(jid).or_default().push(crate::jabber::ChatMsg {
                    from: sender,
                    body,
                    time,
                    outgoing,
                });
            }
            for j in purge {
                s.delete_chat_jid(&j);
            }
        }
        let jabber = std::sync::Arc::new(std::sync::Mutex::new(crate::jabber::JabberState {
            pings: loaded_pings,
            chats: loaded_chats,
            rooms_inaccessible: settings.jabber_inaccessible_rooms.iter().cloned().collect(),
            rooms_left: settings.jabber_left_rooms.iter().cloned().collect(),
            room_subjects: settings.jabber_room_subjects.clone(),
            ..Default::default()
        }));

        let kill_cache: crate::kills::KillCache =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let kill_tx = (!headless).then(|| crate::kills::spawn_fetcher(kill_cache.clone(), ctx.clone()));

        let activity: crate::activity::SharedActivity = {
            let mut c = crate::activity::ActivityCache::default();
            if let Some(s) = &store {
                c.preload(s.pilot_activity());
            }
            std::sync::Arc::new(std::sync::Mutex::new(c))
        };
        if !headless {
            crate::activity::spawn(activity.clone(), ctx.clone());
        }
        let sightings: crate::intel::SharedSightings = Default::default();
        let revivals: crate::watcher::SharedRevivals = {
            let now = chrono::Utc::now().timestamp();
            let mut map = std::collections::HashMap::new();
            if let Some(s) = &store {
                for (name, until) in s.load_revivals() {
                    if until > now {
                        map.insert(name, until);
                    }
                }
            }
            std::sync::Arc::new(std::sync::Mutex::new(map))
        };

        let intel_state =
            std::sync::Arc::new(std::sync::Mutex::new(crate::intel::IntelState::default()));
        let pilots: crate::pilot::SharedPilots =
            std::sync::Arc::new(std::sync::Mutex::new(crate::pilot::PilotCache::default()));
        if !headless {
            crate::pilot::spawn_resolver(pilots.clone(), ctx.clone());
        }
        let killfeed: crate::zkill::SharedKillFeed =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recent_alerts: AlertLog =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let alert_shared: SharedAlertWindow =
            std::sync::Arc::new(std::sync::Mutex::new(AlertWindowState::default()));
        let overlay_stdin: std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let alerts_engine = std::sync::Arc::new(AlertEngine::new(
            recent_alerts.clone(),
            chrono::Utc::now().timestamp(),
            alert_shared.clone(),
            ctx.clone(),
            overlay_stdin.clone(),
        ));
        let system_status: crate::systemstatus::SharedStatus =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        if !headless {
            crate::systemstatus::spawn(system_status.clone(), ctx.clone());
        }
        let affiliations = std::sync::Arc::new(std::sync::Mutex::new(
            crate::affiliation::AffilCache::default(),
        ));
        if !headless {
            crate::affiliation::spawn(affiliations.clone(), ctx.clone());
        }
        let ping_shared: SharedPingWindow = std::sync::Arc::new(std::sync::Mutex::new(
            PingWindowState { enabled: settings.fleet_ping_window, ..Default::default() },
        ));
        if !headless {
            spawn_alert_daemon(
                alerts_engine.clone(),
                intel_state.clone(),
                pilots.clone(),
                player.clone(),
                killfeed.clone(),
                kill_cache.clone(),
                system_status.clone(),
                affiliations.clone(),
                ping_shared.clone(),
                ctx.clone(),
            );
        }

        let web_facts = crate::web::facts::shared();
        let web = crate::web::state::shared();
        let web_detail = crate::web::detail();
        let web_inbox = crate::web::inbox();
        if !headless {
            let engine = alerts_engine.clone();
            let (a_intel, a_pilots, a_player, a_status, a_affil, a_kills) = (
                intel_state.clone(),
                pilots.clone(),
                player.clone(),
                system_status.clone(),
                affiliations.clone(),
                kill_cache.clone(),
            );
            crate::web::publish::spawn(
                crate::web::publish::Deps {
                    facts: web_facts.clone(),
                    web: web.clone(),
                    intel_state: intel_state.clone(),
                    pilots: pilots.clone(),
                    player: player.clone(),
                    system_status: system_status.clone(),
                    jabber: jabber.clone(),
                },
                move || {
                    let gate = engine.alerts_enabled();
                    engine.build_alert_msg(
                        &a_intel, &a_pilots, &a_player, &a_status, &a_affil, &a_kills, gate,
                    )
                },
            );
        }

        let eve_focused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let ping_viewport_cb = build_ping_viewport_cb(ping_shared.clone());
        let alert_viewport_cb = build_alert_viewport_cb(alert_shared.clone());

        if !headless {
            let ctx = ctx.clone();
            let ping_shared = ping_shared.clone();
            let alert_shared = alert_shared.clone();
            let _ = std::thread::Builder::new().name("overlay-ticker".into()).spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let ping_active = !ping_shared.lock().unwrap().windows.is_empty();
                if ping_active {
                    ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
                }
                let alert_active = {
                    let st = alert_shared.lock().unwrap();
                    !st.feed.is_empty() || st.secs > 0.0
                };
                if alert_active {
                    ctx.request_repaint_of(egui::ViewportId::from_hash_of("alert_window"));
                }
            });
        }

        let popouts = popouts_from_cfg(&settings.jabber_popout_windows);
        // Read out before `settings` moves into the struct below.
        let (main_tabs, main_active) = restored_main_tabs(&settings);
        #[cfg(feature = "fleet")]
        let fleet_backend_at_start = crate::fleets::choose_backend(headless, &settings);
        let mut app = Self {
            web_facts,
            web,
            web_detail,
            web_inbox,
            web_qr: None,
            web_reveal: false,
            web_server: None,
            web_started_for: None,
            web_error: None,
            ui_ctx: ctx.clone(),
            web_starting: None,
            web_bind_draft: None,
            web_allowed: !headless,
            store,
            settings,
            view: View::Dashboard,
            intel_channels_open: false,
            jump_bridges_open: false,
            jb_paste: String::new(),
            sov_upgrades_open: false,
            sov_paste: String::new(),
            coalitions_open: false,
            severity_open: false,
            coal_edit: Vec::new(),
            alliance_add: String::new(),
            active_character,
            needs_save: false,
            sde_status,
            auth_status: std::sync::Arc::new(std::sync::Mutex::new(AuthStatus::Idle)),
            characters,
            copy_settings: Default::default(),
            eve_clients,
            eve_settings_path,
            intel_state,
            watcher_started: false,
            chat_dir: None,
            intel_query: String::new(),
            intel_max_jumps: pv.intel_max_jumps,
            intel_type: pv.intel_type,
            battles: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            battle_history: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            battle_history_loading: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            show_history: false,
            battle_selected: None,
            loaded_report: None,
            report_msg: None,
            build_from_kill: std::sync::Arc::new(std::sync::Mutex::new(
                crate::zkill::BuildFromKill::Idle,
            )),
            build_kill_input: String::new(),
            build_kill_error: None,
            battle_ship_ids: None,
            br_share: std::sync::Arc::new(std::sync::Mutex::new(crate::brshare::ShareStatus::Idle)),
            br_share_kid: None,
            br_mine: std::sync::Arc::new(std::sync::Mutex::new(crate::brshare::MineState::default())),
            br_mine_open: false,
            br_unlisted: false,
            br_character: None,
            battle_condensed: false,
            battle_roster_sort: RosterSort::default(),
            battle_search: String::new(),
            battle_hover: None,
            battle_filter: std::sync::Arc::new(std::sync::Mutex::new(crate::settings::BattleFilter::default())),
            ship_sizes: std::sync::Arc::new(std::collections::HashMap::new()),
            player_sys_shared: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
            recent_wh: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            work_throttle_shared: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            battles_enabled_shared: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            battle_filter_open: false,
            filter_picker: None,
            verdict_popup: None,
            verdict_explainer_open: false,
            filter_add_result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            battle_filter_confirm_reset: false,
            battle_overrides: std::sync::Arc::new(std::sync::Mutex::new(br_core::battle::Overrides::default())),
            battle_break_shared: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(br_core::battle::BATTLE_BREAK_SECS)),
            battle_overrides_gen_shared: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            battle_add_queue: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            battle_excluded_count: 0,
            battle_scrub_count: 0,
            battle_edit_mode: false,
            battle_kill_sel: std::collections::HashSet::new(),
            battle_split_preview: None,
            battle_merge_sel: std::collections::HashSet::new(),
            battle_add_open: false,
            battle_add_link: String::new(),
            battle_excluded_open: false,
            battle_scrubs_open: false,
            battle_detail_cache: None,
            br_inputs: std::sync::Arc::new(std::sync::Mutex::new(crate::brview::BrInputs::default())),
            br_outputs: std::sync::Arc::new(std::sync::Mutex::new(crate::brview::BrOutputs::default())),
            br_demand: Default::default(),
            frame_ms: 0.0,
            frame_worst: Vec::new(),
            br_wake: std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new())),
            br_last_sent_sig: 0,
            battle_filter_gen_shared: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            battle_cards: Vec::new(),
            battle_cards_total: 0,
            battle_cards_filtered: 0,
            battle_cards_ready: false,
            battle_cards_out_sig: u64::MAX,
            battle_wait_since: None,
            battle_detail_out_sig: u64::MAX,
            camps: std::sync::Arc::new(std::sync::Mutex::new(crate::camp::CampState::default())),
            camped_cache: Vec::new(),
            camped_cache_at: 0,
            killfeed,
            ship_by_id: std::collections::HashMap::new(),
            kills_loaded: false,
            player,
            systems: None,
            bridges_applied: Default::default(),
            system_status,
            alerts_engine,
            recent_alerts,
            alert_feed: Vec::new(),
            alert_rules_open: false,
            intel_inject: Default::default(),
            test_intel: None,
            alert_selected_rule: None,
            rule_feeds: std::collections::HashMap::new(),
            alert_shared,
            alert_viewport_cb,
            proc_monitor: crate::procstat::Monitor::new(),
            jabber,
            jabber_tx: None,
            jabber_chat: main_active,
            jabber_tabs: main_tabs,
            jabber_popouts: popouts,
            jabber_tab_drag: None,
            jabber_main_rect: None,
            jabber_join_open: false,
            jabber_join_rooms: false,
            jabber_drafts: std::collections::HashMap::new(),
            jabber_room_input: String::new(),
            jabber_contact_search: String::new(),
            jabber_dm_input: String::new(),
            jabber_dm_error: String::new(),
            jabber_pane: JabberPane::Convos,
            taskbar_badge: None,
            jabber_sticky: Default::default(),
            jabber_motd_window: None,
            jabber_collapsed: std::collections::HashSet::new(),
            jabber_my_presence: crate::jabber::Presence::Online,
            jabber_my_status: String::new(),
            jabber_pw_input: String::new(),
            jabber_pings_visible: 50,
            ping_rules_open: false,
            ping_rule_editing: None,
            mention_input: String::new(),
            session_start: chrono::Utc::now().timestamp(),
            eve_focused,
            eve_focus_checked: None,
            ship_index: None,
            update: std::sync::Arc::new(std::sync::Mutex::new(crate::update::UpdateState::default())),
            update_checked_at: None,
            update_dismissed: false,
            store_error,
            store_warn_dismissed: false,
            persist_retry_at: None,
            disk_banner_dismissed: None,
            disk_level: crate::disk::Level::Normal,
            disk_free: None,
            disk_saw_failure: false,
            kill_cache,
            kill_tx,
            lookup_table: Default::default(),
            lookup_current: Vec::new(),
            lookup_sort: Some(lookup_ui::Col::Danger),
            lookup_sort_desc: true,
            lookup_note: None,
            lookup_incoming: Default::default(),
            standings: Default::default(),
            standings_for: None,
            intel_heights: std::collections::HashMap::new(),
            intel_heights_notes_rev: 0,
            jabber_msg_heights: std::collections::HashMap::new(),
            wizard_open: false,
            wizard_step: 0,
            wizard_shortcut: None,
            wizard_checked: false,
            tray: if headless { None } else { crate::tray::spawn(ctx.clone()) },
            really_exit: false,
            raise_reset_top: false,
            raise_main: false,
            overlay: if headless {
                None
            } else {
                match crate::ipc::OverlayLink::start(ctx.clone(), overlay_stdin) {
                    Ok(link) => Some(link),
                    Err(e) => {
                        eprintln!("[main] overlay failed to start (in-process fallback): {e}");
                        None
                    }
                }
            },
            config_sent_hash: None,
            dscan_clip: None,
            dscan_checked: None,
            dscan_seen_hash: 0,
            dscan_dismissed_hash: 0,
            dscan_prompt: None,
            dscan_pos: None,
            dscan_link_used: false,
            dscan_unfocused_at: None,
            dscan_share: std::sync::Arc::new(std::sync::Mutex::new(DscanShare::default())),
            dscan_view: None,
            wh_cache: Vec::new(),
            wh_reloaded: None,
            wh_overlay: WhOverlay::default(),
            wh_filter_dest: None,
            wh_filter_source: None,
            wh_filter_expiring: false,
            map_overlays: pv.overlays,
            map_mode: MapMode::Standard,
            standard_overlays: pv.overlays,
            travel_start: None,
            travel_end: None,
            travel_start_q: String::new(),
            travel_end_q: String::new(),
            travel_regional_gates: true,
            travel_jump_bridges: true,
            travel_avoid_camps: true,
            travel_max_ship_kills: 0,
            travel_sec: [true, true, true],
            travel_start_sel: 0,
            travel_end_sel: 0,
            travel_sugg_key: (String::new(), None, String::new(), None),
            travel_sugg: (Vec::new(), Vec::new()),
            travel_wp_q: String::new(),
            travel_wp_sel: 0,
            travel_wp_sugg_key: String::new(),
            travel_wp_sugg: Vec::new(),
            travel_metric: ActivityMode::ShipKills,
            travel_planned_hash: 0,
            travel_pending_hash: 0,
            travel_dirty_at: None,
            travel_direct_route: None,
            travel_live: false,
            travel_live_base: None,
            travel_changed: Vec::new(),
            travel_changed_at: None,
            travel_live_next: 0.0,
            travel_ingame_dest: None,
            travel_waypoints: Vec::new(),
            routes_dialog_open: false,
            route_save_name: String::new(),
            route_save_folder: String::new(),
            route_search: String::new(),
            route_new_folder: String::new(),
            route_view: RouteView::ByFolder,
            route_edit: None,
            route_edit_name: String::new(),
            route_edit_folder: String::new(),
            travel_avoid: Vec::new(),
            travel_avoid_sov: std::collections::HashSet::new(),
            travel_sov_dialog_open: false,
            travel_route: None,
            ctx_menu_system: None,
            jump_ship: 0,
            jump_jdc: 5,
            jump_jfc: 5,
            jump_skills: std::sync::Arc::new(std::sync::Mutex::new(None)),
            jump_systems: None,
            map_view: crate::map::MapView::Universe,
            map_initialized: false,
            map_history: Vec::new(),
            map_forward: Vec::new(),
            map_regions: Vec::new(),
            map_systems: Vec::new(),
            map_loaded: None,
            map_pan: egui::Vec2::ZERO,
            map_last_rect: None,
            map_zoom: 1.0,
            map_follow: false,
            map_follow_region: None,
            map_popped: false,
            map_in_popout: false,
            map_char_popouts: Vec::new(),
            map_char_view: std::collections::HashMap::new(),
            map_window_on_top: false,
            map_controls_hidden: false,
            map_overlay_mode: false,
            map_vp_props: None,
            map_overlay_locked: false,
            map_overlay_drag: false,
            map_link: None,
            map_pos_prev: std::collections::HashMap::new(),
            map_link_menu: None,
            map_route_opts: Vec::new(),
            map_route_at: 0,
            map_route_kind: "gate",
            map_route_anchors: Vec::new(),
            map_titan_at_start: true,
            map_titan_self_jump: false,
            map_route_zone: None,
            map_route_graph: None,
            map_route_legs: Vec::new(),
            map_leg_pick: Vec::new(),
            map_forks: Default::default(),
            map_avoid_once: std::collections::HashSet::new(),
            map_intel_for: None,
            map_titans: Vec::new(),
            map_alts: None,
            map_save_open: false,
            map_save_name: String::new(),
            map_load_open: false,
            map_layout: pv.map_layout,
            map_threat_jumps: pv.map_threat_jumps,
            map_threat_center: None,
            threat_include_bridges: true,
            safety_prev: None,
            safety_last_scan: 0.0,
            sov_discover_last: 0.0,
            safety_prev_layout: None,
            flash_until: 0.0,
            map_draw: Vec::new(),
            map_draw_spaced: false,
            map_draw_key: None,
            map_systems_cache: std::collections::HashMap::new(),
            map_draw_cache: std::collections::HashMap::new(),
            map_focus: None,
            map_selected: None,
            map_hover_since: None,
            logo_avg: std::collections::HashMap::new(),
            route_destination: None,
            ingame_route: false,
            map_search: String::new(),
            map_search_sel: 0,
            map_search_key: String::new(),
            map_search_sys: Vec::new(),
            map_search_const: Vec::new(),
            map_search_reg: Vec::new(),
            map_search_upgrades: Vec::new(),
            left_dock_open: true,
            right_dock_open: true,
            map_docked_system: None,
            right_dock_tab: RightDockTab::System,
            upgrade_kinds: [true; 4],
            map_highlight_upgrade: None,
            system_window: None,
            system_kills_tab: false,
            system_kills_cache: std::collections::HashMap::new(),
            constellation_window: None,
            region_window: None,
            focus_window: None,
            ship_window: None,
            pending_overlay_clicks: Vec::new(),
            pilot_query: String::new(),
            pilot_lookup: std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle)),
            pilot_window_open: false,
            pilot_sort: PilotSort::MostLost,
            pilot_pane: PilotPane::default(),
            fit_view: None,
            fit_loss: None,
            ping_shared,
            ping_viewport_cb,
            ship_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            ship_roles_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            type_names: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            type_names_loading: std::sync::Arc::new(std::sync::Mutex::new(false)),
            pilots,
            affiliations,
            activity,
            sightings,
            revivals,
            #[cfg(feature = "fleet")]
            fleet: std::sync::Arc::new(std::sync::Mutex::new(crate::fleets::FleetState::default())),
            #[cfg(feature = "fleet")]
            fleet_backend: fleet_backend_at_start,
            #[cfg(feature = "fleet")]
            fleet_login: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_boss_detail: None,
            #[cfg(feature = "fleet")]
            fleet_snowflakes_open: None,
            #[cfg(feature = "fleet")]
            fleet_migrate_open: false,
            #[cfg(feature = "fleet")]
            fleet_hub: None,
            #[cfg(feature = "fleet")]
            fleet_hub_unavailable_flag: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_reopen: None,
            #[cfg(feature = "fleet")]
            fleet_advert_at: None,
            #[cfg(feature = "fleet")]
            fleet_search_pending: None,
            #[cfg(feature = "fleet")]
            comms_resolved: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_boost_detail: None,
            #[cfg(feature = "fleet")]
            rescue_preview_key: None,
            #[cfg(feature = "fleet")]
            rescue_preview_at: None,
            #[cfg(feature = "fleet")]
            fleet_preset_rename: None,
            #[cfg(feature = "fleet")]
            fleet_chat_open: true,
            #[cfg(feature = "fleet")]
            fleet_chat_tab: 0,
            #[cfg(feature = "fleet")]
            fleet_chat_draft: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_last_ping: None,
            #[cfg(feature = "fleet")]
            fleet_boss_asked: None,
            #[cfg(feature = "fleet")]
            fleet_channels_at: None,
            #[cfg(feature = "fleet")]
            fleet_tx,
            #[cfg(feature = "fleet")]
            fleet_rx,
            #[cfg(feature = "fleet")]
            fleet_gen: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_booted: false,
            #[cfg(feature = "fleet")]
            fleet_preview_at: None,
            #[cfg(feature = "fleet")]
            fleet_journal_open: false,
            #[cfg(feature = "fleet")]
            fleet_boosts_read: None,
            #[cfg(feature = "fleet")]
            headless,
            #[cfg(feature = "fleet")]
            fleet_boost_editor: false,
            // Open by default: what a fleet flies and where it talks is the first thing checked
            // on a fleet that is already up.
            #[cfg(feature = "fleet")]
            fleet_sidebar_open: true,
            #[cfg(feature = "fleet")]
            fleet_quick_open: false,
            #[cfg(feature = "fleet")]
            fleet_mumble_at: None,
            #[cfg(feature = "fleet")]
            fleet_mumble_asked: None,
            #[cfg(feature = "fleet")]
            fleet_mumble_tx: fleet_mumble.0,
            #[cfg(feature = "fleet")]
            fleet_mumble_rx: fleet_mumble.1,
            #[cfg(feature = "fleet")]
            fleet_detail_tab: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_map: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_trackers: Default::default(),
            #[cfg(feature = "fleet")]
            fleet_tracks_resumed: false,
            #[cfg(feature = "fleet")]
            fleet_confirm: None,
            #[cfg(feature = "fleet")]
            rescue: std::sync::Arc::new(std::sync::Mutex::new(crate::rescue::RescueState::default())),
            #[cfg(feature = "fleet")]
            rescue_feed_cursor: 0,
            #[cfg(feature = "fleet")]
            ship_groups: None,
            #[cfg(feature = "fleet")]
            delve911_cursor: 0,
            notes_view,
            notes,
            notes_error: None,
            note_editor: None,
            notes_manager: None,
            note_editor_pending: None,
            #[cfg(feature = "fleet")]
            map_coords: None,
            #[cfg(feature = "fleet")]
            rescue_range_for: None,
            #[cfg(feature = "fleet")]
            rescue_range: None,
            rescue_cyno_input: String::new(),
            cyno_generators_open: false,
            #[cfg(feature = "fleet")]
            rescue_armed: false,
        };
        app.tab_set().normalize();
        app
    }

    pub(crate) fn open_system(&mut self, system_id: i64) {
        self.system_window = Some(system_id);
        self.focus_window = Some(egui::ViewportId::from_hash_of("system_window"));
    }

    fn dock_system(&mut self, system_id: i64) {
        self.map_docked_system = Some(system_id);
        self.right_dock_open = true;
        self.right_dock_tab = RightDockTab::System;
    }

    fn open_ship(&mut self, ship_id: i64) {
        self.ship_window = Some(ship_id);
        self.focus_window = Some(egui::ViewportId::from_hash_of("ship_window"));
    }

    fn drain_alerts(&mut self) {
        self.migrate_dock_permits();
        {
            let mut cfg = self.alerts_engine.config.lock().unwrap();
            cfg.enabled = self.settings.alert_enabled;
            cfg.alerts = self.settings.alerts.clone();
            cfg.severity = self.settings.severity.clone();
            cfg.only_undocked = self.settings.alert_only_undocked;
            cfg.disabled = self.settings.intel_disabled_chars.clone();
            cfg.systems = self.systems.clone();
            cfg.ship_index = self.ship_index.clone();
            cfg.active_character = self.active_character.clone();
            cfg.chars = self.characters.iter().map(|c| (c.name.clone(), c.id)).collect();
            cfg.kill_intel = self.settings.kill_intel;
            cfg.kill_intel_jumps = self.settings.kill_intel_jumps;
            cfg.intel_max_jumps = self.intel_max_jumps;
            cfg.intel_count_bridges = self.settings.intel_count_bridges;
            cfg.staging = self.staging_system().map(str::to_owned);
            cfg.notes = self.notes_view.clone();
        }
        self.publish_ui_facts();
        self.sync_web_server();
        let (fired, matched) = {
            let mut rt = self.alerts_engine.runtime.lock().unwrap();
            (std::mem::take(&mut rt.fired_ui), std::mem::take(&mut rt.matched_ui))
        };
        for (report, sev, rule_id, suppressed) in matched {
            let feed = self.rule_feeds.entry(rule_id).or_default();
            feed.push((report, sev, suppressed));
            let n = feed.len();
            if n > 50 {
                feed.drain(0..n - 50);
            }
        }
        if fired.is_empty() {
            return;
        }
        for (report, sev, _win) in fired {
            self.alert_feed.push((report, sev));
        }
        let n = self.alert_feed.len();
        if n > 100 {
            self.alert_feed.drain(0..n - 100);
        }
    }

    fn kill_intel_range(ui: &mut egui::Ui, jumps: &mut u32) -> egui::Response {
        ui.add(
            egui::DragValue::new(jumps)
                .range(0..=20)
                .prefix(format!("{}  ", egui_phosphor::regular::CARET_UP_DOWN))
                .custom_formatter(|n, _| match n as u32 {
                    0 => "within the feed's range".to_owned(),
                    1 => "within 1 jump".to_owned(),
                    n => format!("within {n} jumps"),
                })
                // The formatter writes words, which the default numeric parser cannot read back.
                .custom_parser(|s| {
                    s.chars().filter(char::is_ascii_digit).collect::<String>().parse().ok()
                }),
        )
        .on_hover_text(
            "How far from you a kill counts as intel. The lowest setting follows the intel feed's own jumps filter.",
        )
    }

    fn alerts_view(&mut self, ui: &mut egui::Ui) {
        if self.alert_rules_open {
            self.alert_rules_editor(ui);
            return;
        }
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut self.settings.alert_enabled, "Enable intel alerts")
                .on_hover_text("Master switch for all intel alerts")
                .changed()
            {
                self.needs_save = true;
            }
            {
                let mut snooze = self.alert_shared.lock().unwrap().snooze;
                if ui
                    .checkbox(
                        &mut snooze,
                        format!(
                            "{}  Snooze alert window until I undock",
                            egui_phosphor::regular::ALARM
                        ),
                    )
                    .on_hover_text(
                        "Suppress the alert window from opening. Intel is still collected. Clears when any character undocks.",
                    )
                    .changed()
                {
                    self.alert_shared.lock().unwrap().snooze = snooze;
                }
            }
            if ui
                .checkbox(&mut self.settings.kill_intel, "zKill intel")
                .on_hover_text("Within range, killmails appear as intel cards (and respect the alert rules)")
                .changed()
            {
                self.needs_save = true;
            }
            if self.settings.kill_intel
                && Self::kill_intel_range(ui, &mut self.settings.kill_intel_jumps).changed()
            {
                self.needs_save = true;
            }
        });
        if !self.settings.alert_enabled {
            ui.colored_label(
                crate::theme::standing::WARNING,
                "Intel alerts are off. No rule will fire until this is enabled.",
            );
        } else if !self.settings.alerts.rules.iter().any(|r| r.enabled) {
            ui.colored_label(
                crate::theme::standing::WARNING,
                "No alert rule is enabled. Nothing will fire. Enable or add a rule below.",
            );
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let n = self.settings.alerts.rules.iter().filter(|r| r.enabled).count();
            if ui
                .button(format!(
                    "{}  Alert rules ({n} on)",
                    egui_phosphor::regular::SLIDERS_HORIZONTAL
                ))
                .on_hover_text("Configure alert rules")
                .clicked()
            {
                self.alert_rules_open = true;
            }
            if ui
                .button(format!("{}  Pilot notes and tags", egui_phosphor::regular::TAG))
                .on_hover_text("Manage pilot tags, notes and their folders")
                .clicked()
            {
                self.open_notes_manager(crate::notes::NoteKind::Pilot);
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(egui::RichText::new("Recent alerts").strong());
            self.alert_history_ui(ui);
        });
    }

    fn alert_history_ui(&mut self, ui: &mut egui::Ui) {
        let feed = self.alert_feed.iter().rev().take(60).map(|(r, sev)| (r.clone(), *sev, false)).collect();
        self.alert_cards_ui(ui, feed);
    }

    /// One rule's "Recent matches", each card marked when the rule suppressed it.
    fn rule_feed_ui(&mut self, ui: &mut egui::Ui, rule_id: u64) {
        let feed = self
            .rule_feeds
            .get(&rule_id)
            .map(|f| f.iter().rev().take(60).cloned().collect())
            .unwrap_or_default();
        self.alert_cards_ui(ui, feed);
    }

    /// Alert cards, newest first, as (report, severity, suppressed).
    fn alert_cards_ui(&mut self, ui: &mut egui::Ui, mut feed: Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)>) {
        if feed.is_empty() {
            ui.label(egui::RichText::new("None yet.").weak());
            return;
        }
        {
            let mut cache = self.pilots.lock().unwrap_or_else(|e| e.into_inner());
            for (r, _, _) in feed.iter_mut() {
                r.pilots.retain(|p| {
                    if crate::intel::is_pilot_stopword(p) {
                        return false;
                    }
                    match cache.get(p) {
                        Some(Some(_)) => !cache.is_hidden(p),
                        Some(None) => false,
                        None => {
                            cache.queue(p);
                            true
                        }
                    }
                });
            }
        }
        let ship_ids: std::collections::HashSet<i64> =
            feed.iter().flat_map(|(r, _, _)| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
            ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp = cache
                .display_ids(feed.iter().flat_map(|(r, _, _)| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let status = self.system_status.lock().unwrap().clone();
        let last_ship = build_last_ship(&self.intel_state.lock().unwrap().reports);
        let systems = self.systems.clone();
        let player_sys = self.player_system();
        let rings = self.char_rings();
        let bridges = self.settings.intel_count_bridges;
        let now = chrono::Utc::now().timestamp();
        let kc = self.kill_cache.clone();
        let affil = self.affiliations.clone();
        let mut click: Option<IntelClick> = None;
        for (r, sev, suppressed) in &feed {
            if *suppressed {
                ui.label(
                    egui::RichText::new(format!("{}  suppressed", egui_phosphor::regular::BELL_SLASH))
                        .color(crate::theme::standing::NEUTRAL),
                );
            }
            let target = r.primary_system().map(|s| s.id);
            let from_you = jumps_from_you(&systems, player_sys, target, bridges);
            let via = jump_via(&systems, player_sys, target, bridges, from_you);
            let cchars = rings.card_for(r);
            if let Some(c) = intel_row(
                ui, r, now, false, from_you, via, &cchars, &systems, &status, &ship_details, &ship_roles,
                &resolved_pilots, &uncertain, &last_ship, &kc, *sev, true, &affil,
                &self.notes_view, false, &mut None,
            ) {
                click = Some(c);
            }
        }
        if let Some(c) = click {
            self.act_on_intel_click(c, ui.ctx());
        }
    }

    fn maybe_start_watcher(&mut self, ctx: &egui::Context) {
        if self.watcher_started {
            return;
        }
        let ready = matches!(*self.sde_status.lock().unwrap(), SdeStatus::Ready { .. });
        if !ready {
            return;
        }
        let Some(store) = &self.store else { return };

        self.chat_dir = crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir);
        self.watcher_started = true;

        let mut systems = store.load_systems();
        if apply_baked_defaults(&mut self.settings, &systems, BAKED_REGION, BAKED_BRIDGES, BAKED_UPGRADES) {
            self.needs_save = true;
        }
        self.bridges_applied = crate::ansiblex::feed(&self.settings, &mut systems);
        let systems = std::sync::Arc::new(systems);
        self.systems = Some(systems.clone());

        if let Some(store) = &self.store {
            if !store.traits_baked() {
                sde::spawn_traits_bake(store.path().to_path_buf(), ctx.clone());
            }
            // Pre-load remembered pilot names so they're recognised immediately. Negatives are
            // not preloaded: they live in-memory with a TTL (see NEG_TTL), so a name ESI once
            // missed is re-checked rather than cached as "not a name" across restarts.
            {
                let mut c = self.pilots.lock().unwrap();
                c.preload(&store.known_pilots());
                c.preload_verdicts(store.load_pilot_verdicts());
            }
        }

        let camp_types = self.store.as_ref().map(|s| s.load_camp_types()).unwrap_or_default();
        let ship_ids = std::sync::Arc::new(
            store.ship_index().values().map(|(id, _)| *id).collect::<std::collections::HashSet<i64>>(),
        );
        self.battle_ship_ids = Some(ship_ids.clone());
        self.ship_sizes = std::sync::Arc::new(store.ship_sizes());
        *self.battle_filter.lock().unwrap() = self.settings.battles.clone();
        *self.battle_overrides.lock().unwrap() = store.load_battle_overrides();
        self.battle_excluded_count = store.count_excluded();
        self.battle_scrub_count = store.count_scrubs();
        self.battle_break_shared
            .store(self.settings.battle_break_secs, std::sync::atomic::Ordering::Relaxed);
        self.work_throttle_shared
            .store(self.settings.work_throttle.as_u8(), std::sync::atomic::Ordering::Relaxed);
        self.battles_enabled_shared
            .store(self.settings.battles_enabled, std::sync::atomic::Ordering::Relaxed);
        crate::zkill::spawn(
            systems.clone(),
            self.intel_state.clone(),
            self.battles.clone(),
            self.camps.clone(),
            self.killfeed.clone(),
            camp_types,
            ship_ids,
            self.battle_filter.clone(),
            self.ship_sizes.clone(),
            self.player_sys_shared.clone(),
            self.recent_wh.clone(),
            self.work_throttle_shared.clone(),
            self.battle_break_shared.clone(),
            self.battle_overrides.clone(),
            self.battle_overrides_gen_shared.clone(),
            self.battle_add_queue.clone(),
            self.battles_enabled_shared.clone(),
            ctx.clone(),
        );
        crate::brview::spawn(
            Some(systems.clone()),
            self.intel_state.clone(),
            self.battles.clone(),
            self.battle_history.clone(),
            self.battle_filter.clone(),
            self.ship_sizes.clone(),
            self.type_names.clone(),
            self.battle_overrides_gen_shared.clone(),
            self.battle_filter_gen_shared.clone(),
            self.br_inputs.clone(),
            self.br_outputs.clone(),
            self.br_wake.clone(),
            self.battles_enabled_shared.clone(),
            self.br_demand.clone(),
            ctx.clone(),
        );

        // Seed the rescue selectors from persisted settings before the window reads them, and turn
        // the old cap-save template into a fleet preset the first time this build runs.
        #[cfg(feature = "fleet")]
        {
            // Straight off disk rather than out of `FleetState`, whose seed is still the
            // invented one this early: the migration matches old doctrine names against the real
            // setup list.
            let setups: Vec<(i32, String)> = crate::fleets::seed::load()
                .setups
                .iter()
                .map(|s| (s.id.0, s.name.trim().to_owned()))
                .collect();
            if crate::settings::seed_rescue_preset(&mut self.settings, &setups) {
                self.needs_save = true;
            }
            let mut r = self.rescue.lock().unwrap();
            r.op_channel = self.settings.rescue_op_channel.clamp(1, 12);
            if r.doctrine.is_empty() {
                r.doctrine = self.settings.rescue_preset.clone();
            }
        }

        // SDE ship name (lowercased) -> group, so a ping naming a specific hull resolves to a
        // capital class (e.g. "Phoenix Navy Issue" -> Dreadnought). Built outside the chat-log
        // branch: the jabber delve911 ingest needs it even with no EVE log directory configured.
        let ship_groups = std::sync::Arc::new(
            store
                .all_ships()
                .into_iter()
                .map(|(_, name, group)| (name.to_lowercase(), group))
                .collect::<std::collections::HashMap<String, String>>(),
        );
        #[cfg(feature = "fleet")]
        {
            self.ship_groups = Some(ship_groups.clone());
            if self.map_coords.is_none() {
                self.map_coords = Some(std::sync::Arc::new(store.all_map_systems()));
            }
        }

        if let Some(dir) = self.chat_dir.clone() {
            let ships = std::sync::Arc::new(store.ship_index());
            self.ship_index = Some(ships.clone());
            #[cfg(feature = "fleet")]
            let rescue_channel = if self.settings.fc_rescue_enabled {
                self.settings.rescue_channel.clone()
            } else {
                String::new()
            };
            #[cfg(not(feature = "fleet"))]
            let rescue_channel = String::new();
            // Built as a local because `#[cfg]` can't be applied to a call argument.
            #[cfg(feature = "fleet")]
            let rescue_handle = self.rescue.clone();
            #[cfg(not(feature = "fleet"))]
            let rescue_handle = ();
            crate::watcher::spawn(
                dir,
                self.settings.intel_channels.clone(),
                systems,
                ships,
                self.pilots.clone(),
                self.intel_state.clone(),
                self.sightings.clone(),
                self.activity.clone(),
                self.revivals.clone(),
                rescue_handle,
                rescue_channel,
                ship_groups,
                self.intel_inject.clone(),
                ctx.clone(),
            );
        }

    }

    /// The intel toolbar's search field. Its own hint text is the floor: a field too narrow to
    /// show its placeholder tells the user nothing about what it filters.
    pub(crate) const INTEL_FILTER_HINT: &'static str = "Filter by system, text, channel, or tag";
    /// The hint lays out at ~187px, so a crowded row wraps the field onto its own line rather than
    /// shrinking it past what it can say.
    const INTEL_FILTER_MIN_W: f32 = 220.0;

    fn fleet_ping_window_ui(&mut self, ctx: &egui::Context) {
        if self.settings.fleet_ping_on_top == crate::settings::OnTop::Smart {
            let due = self.eve_focus_checked.map(|t| t.elapsed().as_millis() > 800).unwrap_or(true);
            if due {
                self.eve_focused.store(eve_is_focused(), std::sync::atomic::Ordering::Relaxed);
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        let (moved, moved_size) = {
            let mut st = self.ping_shared.lock().unwrap();
            st.on_top = self.settings.fleet_ping_on_top;
            st.enabled = self.settings.fleet_ping_window;
            st.systems = self.systems.clone();
            st.doctrine_url = self.settings.doctrine_url.clone();
            st.op_links = self.settings.op_channel_links.clone();
            st.eve_focused = self.eve_focused.load(std::sync::atomic::Ordering::Relaxed);
            st.win_pos = self.settings.fleet_ping_window_pos;
            st.win_size = self.settings.fleet_ping_window_size;
            (st.moved.take(), st.moved_size.take())
        };
        // In-process only: the subprocess overlay reports its own move over IPC (PingMoved).
        self.persist_ping_geometry(moved, moved_size);

        if self.overlay.is_some() {
            self.send_ping_to_overlay();
            return;
        }

        let on_top = self.settings.fleet_ping_on_top != crate::settings::OnTop::Never
            && (self.settings.fleet_ping_on_top == crate::settings::OnTop::Always
                || self.eve_focused.load(std::sync::atomic::Ordering::Relaxed));
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("fleet_ping_window"),
            ping_viewport_builder(
                on_top,
                self.settings.fleet_ping_window_pos,
                self.settings.fleet_ping_window_size,
            ),
            {
                let cb = self.ping_viewport_cb.clone();
                move |ui: &mut egui::Ui, class: egui::ViewportClass| cb(ui, class)
            },
        );
    }

    fn alert_window_feature(&self) -> bool {
        self.settings.alert_enabled
            && self.settings.alerts.rules.iter().any(|r| r.enabled && r.custom_window)
    }

    fn overlay_config(&self) -> crate::ipc::OverlayConfig {
        let (ping_enabled, ping_on_top) = {
            let st = self.ping_shared.lock().unwrap();
            (st.enabled, st.on_top)
        };
        crate::ipc::OverlayConfig {
            ping_enabled,
            ping_on_top,
            alert_enabled: self.alert_window_feature(),
            alert_on_top: self.settings.alerts.on_top,
            window_timeout: self.settings.alerts.window_timeout,
            win_pos: self.settings.alerts.window_pos,
            win_size: self.settings.alerts.window_size,
            ping_win_pos: self.settings.fleet_ping_window_pos,
            ping_win_size: self.settings.fleet_ping_window_size,
            compact: self.settings.alerts.compact_mode,
        }
    }

    fn send_ping_to_overlay(&mut self) {
        use std::hash::{Hash, Hasher};
        let Some(link) = self.overlay.as_ref() else { return };
        if link.take_reconnected() {
            self.config_sent_hash = None;
            *self.alerts_engine.alert_sent_hash.lock().unwrap() = None;
            *self.alerts_engine.ping_sent_hash.lock().unwrap() = None;
        }

        // The engine thread forwards the ping list (so pings raise the overlay while minimized);
        // the UI only owns the overlay Config, which doesn't change while minimized.
        let cfg = self.overlay_config();
        let config_hash = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            cfg.ping_enabled.hash(&mut h);
            (cfg.ping_on_top as u8).hash(&mut h);
            cfg.alert_enabled.hash(&mut h);
            (cfg.alert_on_top as u8).hash(&mut h);
            cfg.window_timeout.to_bits().hash(&mut h);
            cfg.win_pos.map(|(x, y)| (x.to_bits(), y.to_bits())).hash(&mut h);
            cfg.win_size.map(|(x, y)| (x.to_bits(), y.to_bits())).hash(&mut h);
            cfg.ping_win_pos.map(|(x, y)| (x.to_bits(), y.to_bits())).hash(&mut h);
            cfg.ping_win_size.map(|(x, y)| (x.to_bits(), y.to_bits())).hash(&mut h);
            cfg.compact.hash(&mut h);
            h.finish()
        };
        if Some(config_hash) != self.config_sent_hash {
            link.send(&crate::ipc::MainToOverlay::Config(cfg));
            self.config_sent_hash = Some(config_hash);
        }
    }

    fn start_sde(&self, ctx: &egui::Context) {
        if let Some(store) = &self.store {
            sde::spawn_download(store.path().to_path_buf(), self.sde_status.clone(), ctx.clone());
        }
    }

    fn alert_window(&mut self, ctx: &egui::Context) {
        let feature = self.alert_window_feature();
        if self.settings.alerts.on_top == crate::settings::OnTop::Smart {
            let due = self
                .eve_focus_checked
                .map(|t| t.elapsed().as_millis() > 800)
                .unwrap_or(true);
            if due {
                self.eve_focused.store(eve_is_focused(), std::sync::atomic::Ordering::Relaxed);
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }

        // Overlay subprocess: the engine thread pushes the enriched update (works while minimized);
        // only the in-process fallback renders here.
        if self.overlay.is_some() {
            return;
        }

        let feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
            if !feature || self.alert_feed.is_empty() {
                Vec::new()
            } else {
                let live = self.intel_state.lock().unwrap();
                let start = self.alert_feed.len().saturating_sub(50);
                self.alert_feed[start..]
                    .iter()
                    .filter_map(|(r, sev)| {
                        let id = r.id;
                        live.reports.iter().find(|lr| lr.id == id).cloned().map(|lr| (lr, *sev))
                    })
                    .collect()
            };
        let resolved_pilots: std::collections::HashMap<String, i64> = if feed.is_empty() {
            Default::default()
        } else {
            let mut cache = self.pilots.lock().unwrap();
            cache.display_ids(feed.iter().flat_map(|(r, _)| r.pilots.iter()).map(|s| s.as_str()))
        };
        let uncertain = if feed.is_empty() {
            Default::default()
        } else {
            uncertain_set(&self.pilots.lock().unwrap(), &resolved_pilots)
        };
        let status = if feed.is_empty() {
            Default::default()
        } else {
            self.system_status.lock().unwrap().clone()
        };
        let last_ship = if feed.is_empty() {
            Default::default()
        } else {
            build_last_ship(&self.intel_state.lock().unwrap().reports)
        };

        {
            let on_top = self.settings.alerts.on_top != crate::settings::OnTop::Never
                && (self.settings.alerts.on_top == crate::settings::OnTop::Always
                    || self.eve_focused.load(std::sync::atomic::Ordering::Relaxed));
            let ship_ids: std::collections::HashSet<i64> =
                feed.iter().flat_map(|(r, _)| r.ships.iter().map(|s| s.id)).collect();
            let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
                ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
            let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
                ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
            let systems = self.systems.clone();
            let player_sys = self.player_system();
            let rings = self.char_rings();
            let card_chars: Vec<CardChars> = feed
                .iter()
                .map(|(r, _)| rings.card_for(r))
                .collect();

            let (_active, just_opened, clicks, verdicts, moved, moved_size, compact_toggle) = {
                let mut st = self.alert_shared.lock().unwrap();
                st.enabled = feature;
                if st.verdict_explained && !self.settings.verdict_explained {
                    self.settings.verdict_explained = true;
                    self.needs_save = true;
                }
                st.verdict_explained = self.settings.verdict_explained;
                st.on_top_level = on_top;
                st.compact = self.settings.alerts.compact_mode;
                st.count_bridges = self.settings.intel_count_bridges;
                st.win_pos = self.settings.alerts.window_pos;
                st.win_size = self.settings.alerts.window_size;
                st.feed = feed;
                st.chars = card_chars;
                st.notes = self.notes_view.clone();
                st.status = status;
                st.ship_details = ship_details;
                st.ship_roles = ship_roles;
                st.resolved_pilots = resolved_pilots;
                st.uncertain = uncertain;
                st.last_ship = last_ship;
                st.systems = systems;
                st.player_sys = player_sys;
                st.kills = Some(self.kill_cache.clone());
                st.affil = Some(self.affiliations.clone());
                if !feature {
                    st.secs = 0.0;
                    st.pinned = false;
                    st.feed.clear();
                }
                let active = st.enabled && (st.secs > 0.0 || st.pinned);
                let just_opened = active && !st.open;
                let clicks = std::mem::take(&mut st.clicks);
                let verdicts = std::mem::take(&mut st.verdict_out);
                let moved = st.moved.take();
                let moved_size = st.moved_size.take();
                let compact_toggle = st.compact_toggle.take();
                (active, just_opened, clicks, verdicts, moved, moved_size, compact_toggle)
            };

            if let Some(v) = compact_toggle {
                self.settings.alerts.compact_mode = v;
                self.needs_save = true;
            }

            for click in clicks {
                self.act_on_intel_click(click, ctx);
            }
            for (name, hidden) in verdicts {
                self.apply_pilot_verdict(&name, hidden);
            }
            // Not on the open frame, where the window briefly reports its builder default before
            // the saved geometry is re-applied.
            if !just_opened {
                self.persist_alert_geometry(moved, moved_size);
            }

            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("alert_window"),
                alert_viewport_builder(
                    on_top,
                    self.settings.alerts.window_pos,
                    self.settings.alerts.window_size,
                ),
                {
                    let cb = self.alert_viewport_cb.clone();
                    move |ui: &mut egui::Ui, class: egui::ViewportClass| cb(ui, class)
                },
            );
        }
    }

    /// The single place a note edit is applied, whichever window or device it came from.
    pub(crate) fn apply_notes_op(
        &mut self,
        op: crate::notes::NotesOp,
    ) -> Result<crate::notes::Applied, String> {
        let remember = matches!(op, crate::notes::NotesOp::SetEntry { .. });
        let systems = self.systems.clone();
        let name = move |id: i64| systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone());
        let now = chrono::Utc::now().timestamp();
        let mut next = (*self.notes).clone();
        let applied = match next.apply(op, now, &name) {
            Ok(a) => a,
            Err(e) => {
                self.notes_error = Some(e.to_owned());
                return Err(e.to_owned());
            }
        };
        if let Some(store) = &self.store {
            if let Err(e) = store.save_notes(&next) {
                let msg = format!("could not save notes: {e:#}");
                self.notes_error = Some(msg.clone());
                return Err(msg);
            }
        }
        self.notes = std::sync::Arc::new(next);
        self.notes_error = None;
        if remember {
            if let Some(f) = &applied.folder {
                if *f != self.settings.notes_folder {
                    self.settings.notes_folder = f.clone();
                    self.needs_save = true;
                }
            }
        }
        self.rebuild_notes_view();
        Ok(applied)
    }

    pub(crate) fn open_note_editor(&mut self, subject: crate::notes::Subject) {
        let mut d = NoteDraft {
            subject,
            folder: self.notes_view.target.clone(),
            note: String::new(),
            tags: Vec::new(),
            new_tag: String::new(),
            new_color: crate::notes::default_color(self.notes.all().iter().map(|f| f.tags.len()).sum()),
        };
        d.load(&self.notes);
        self.note_editor = Some(d);
        self.notes_error = None;
        self.focus_window = Some(egui::ViewportId::from_hash_of("note_editor"));
    }

    fn notes_click(&mut self, c: IntelClick) {
        match c {
            IntelClick::Notes(op) => {
                let _ = self.apply_notes_op(op);
            }
            IntelClick::Annotate(subject) => self.open_note_editor(subject),
            _ => {}
        }
    }

    pub(crate) fn set_default_tag_color(&mut self, id: String, color: Option<[u8; 3]>) {
        if !crate::notes::default_tags().iter().any(|t| t.id == id) {
            return;
        }
        let before = self.settings.tag_colors.get(&id).copied();
        match color {
            Some(c) => self.settings.tag_colors.insert(id, c),
            None => self.settings.tag_colors.remove(&id),
        };
        if before != color {
            self.needs_save = true;
            self.rebuild_notes_view();
        }
    }

    pub(crate) fn set_notes_target(&mut self, folder: String) {
        if self.notes.find(&folder).is_some() && folder != self.settings.notes_folder {
            self.settings.notes_folder = folder;
            self.needs_save = true;
            self.rebuild_notes_view();
        }
    }

    fn rebuild_notes_view(&mut self) {
        self.notes_view = std::sync::Arc::new(self.notes.view_with(&self.settings.notes_folder, &self.settings.tag_colors));
    }

    /// One handler for everything that arrives from outside the UI thread.
    ///
    /// The overlay subprocess and the web page send the same enum into the same arms, so there is a
    /// single answer to what a verdict or an acknowledgement does, rather than two that can drift.
    fn apply_overlay_message(&mut self, m: crate::ipc::OverlayToMain, ctx: &egui::Context) {
        match m {
            crate::ipc::OverlayToMain::Click(c) => {
                self.pending_overlay_clicks.push(c);
                ctx.request_repaint();
            }
            crate::ipc::OverlayToMain::Verdict { name, hidden } => {
                self.apply_pilot_verdict(&name, hidden)
            }
            crate::ipc::OverlayToMain::Notes(op) => {
                let _ = self.apply_notes_op(op);
            }
            crate::ipc::OverlayToMain::NotesTarget { folder } => self.set_notes_target(folder),
            crate::ipc::OverlayToMain::DefaultTagColor { id, color } => self.set_default_tag_color(id, color),
            crate::ipc::OverlayToMain::AlertMoved { pos, size } => {
                self.persist_alert_geometry(pos, size)
            }
            crate::ipc::OverlayToMain::PingMoved { pos, size } => {
                self.persist_ping_geometry(pos, size)
            }
            crate::ipc::OverlayToMain::CompactToggle(v) => {
                self.settings.alerts.compact_mode = v;
                self.needs_save = true;
            }
            crate::ipc::OverlayToMain::AlertAck { id } => self.ack_alert(id),
            crate::ipc::OverlayToMain::JoinComms { ts } => self.join_comms(ts),
            crate::ipc::OverlayToMain::SelectSystem { id } => {
                self.map_selected = Some(id);
                self.map_focus = Some(id);
            }
            crate::ipc::OverlayToMain::SetDestination { id } => self.web_set_destination(id),
            crate::ipc::OverlayToMain::SetIngameRoute { waypoints } => self.set_ingame_route(waypoints),
            crate::ipc::OverlayToMain::ClearIngameRoute => self.clear_route(),
            crate::ipc::OverlayToMain::AvoidSystem { id, jump, on } => {
                let list = if jump {
                    &mut self.settings.route_avoid_jump
                } else {
                    &mut self.settings.route_avoid_gate
                };
                list.retain(|&s| s != id);
                if on {
                    list.push(id);
                }
                self.needs_save = true;
            }
            crate::ipc::OverlayToMain::JabberRead { jid } => self.jabber_mark_read(&jid),
            crate::ipc::OverlayToMain::JabberClose { jid } => {
                // Which list it belongs in is the app's to decide: the page knows a conversation is
                // a room, but the rescue-room guard and the closed lists live here.
                let is_room = {
                    let st = self.jabber.lock().unwrap_or_else(|e| e.into_inner());
                    st.rooms.contains(&jid) || st.rooms_left.contains(&jid)
                };
                self.close_jabber_tab(&jid, is_room);
            }
            crate::ipc::OverlayToMain::Bookmark { id, on } => {
                self.settings.bookmarks.retain(|&b| b != id);
                if on {
                    self.settings.bookmarks.push(id);
                }
                self.needs_save = true;
            }
            crate::ipc::OverlayToMain::SaveRoute { mut route } => {
                route.saved_at = chrono::Utc::now().timestamp();
                self.settings.saved_map_routes.retain(|r| r.name != route.name);
                self.settings.saved_map_routes.push(route);
                self.needs_save = true;
            }
            crate::ipc::OverlayToMain::DeleteRoute { name } => {
                self.settings.saved_map_routes.retain(|r| r.name != name);
                self.needs_save = true;
            }
            crate::ipc::OverlayToMain::RouteViaWormholes { on } => {
                self.settings.route_via_wormholes = on;
                self.needs_save = true;
            }
            crate::ipc::OverlayToMain::JabberOpen { name, room } => self.web_open_convo(&name, room),
            crate::ipc::OverlayToMain::JabberSend { jid, body } => self.web_send_convo(&jid, &body),
            crate::ipc::OverlayToMain::Hello => {}
        }
    }

    /// Route the selected character to a system, from the map on the phone.
    ///
    /// The same two things the map's own context menu does: the in-game destination, and the app's
    /// route overlay, so whoever is sitting at the machine sees where the phone just sent them.
    /// A planned route as in-game waypoints. Bounded because each one is its own ESI call, and the
    /// page can ask for anything.
    fn set_ingame_route(&mut self, waypoints: Vec<i64>) {
        const MAX: usize = 100;
        if self.active_character == "No character" || waypoints.is_empty() || waypoints.len() > MAX {
            return;
        }
        let known = |id: &i64| self.systems.as_ref().is_some_and(|g| g.info_of(*id).is_some());
        if !waypoints.iter().all(known) {
            return;
        }
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        crate::esi::set_route(cid, self.active_character.clone(), waypoints);
        self.ingame_route = true;
    }

    /// Drop the planned route, the drawn one and, when this app set it, the character's in-game route.
    ///
    /// ESI has no "clear the route" call. A waypoint on the system the character is already in, set
    /// with the clear flag, is how the client is told to forget the rest.
    pub(crate) fn clear_route(&mut self) {
        self.map_route_clear();
        self.route_destination = None;
        if self.ingame_route {
            self.ingame_route = false;
            if let Some(here) = self.player_system() {
                if self.active_character != "No character" {
                    let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                    crate::esi::set_waypoint(cid, self.active_character.clone(), here, true);
                }
            }
        }
    }

    fn web_set_destination(&mut self, id: i64) {
        if self.active_character == "No character" {
            return;
        }
        let cid = crate::auth::DEFAULT_CLIENT_ID.to_owned();
        let cname = self.active_character.clone();
        self.set_destination_esi(cid, cname, id);
        self.route_destination = Some(id);
        self.ingame_route = true;
    }

    /// Open a conversation on behalf of the page, by the same route the app's own start dialog uses.
    ///
    /// The page sends a name and a kind rather than a JID: resolving one needs the configured
    /// domain, joining a room is a command to the session, and a socket on the LAN should not be
    /// able to name an arbitrary JID for this machine to join.
    fn web_open_convo(&mut self, name: &str, room: bool) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let jid = if room {
            let jid = self.full_room_jid(name);
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::JoinRoom { room: jid.clone() });
            }
            if !self.settings.jabber_rooms.contains(&jid) {
                self.settings.jabber_rooms.push(jid.clone());
            }
            self.settings.jabber_closed_rooms.retain(|r| r != &jid);
            self.jabber_unleave(&jid);
            jid
        } else {
            // An exact name already in the roster or in the history wins over a guess at the
            // domain, which is the order the desktop dialog resolves in.
            let known = {
                let st = self.jabber.lock().unwrap_or_else(|e| e.into_inner());
                st.roster
                    .iter()
                    .find(|(jid, c)| {
                        c.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))
                            || jid.split('@').next().is_some_and(|l| l.eq_ignore_ascii_case(name))
                    })
                    .map(|(jid, _)| jid.clone())
            };
            let jid = known.unwrap_or_else(|| self.full_user_jid(name));
            self.settings.jabber_closed_dms.retain(|j| j != &jid);
            jid
        };
        self.jabber_unforget(&jid);
        self.jabber_mark_read(&jid);
        self.needs_save = true;
        self.jabber_open(&jid, ChatWinKey::Main);
    }

    /// Send one message from the page. Behind `allow_writeback` like every other write, and only to
    /// a conversation that already exists, so the page cannot start a thread with a stranger.
    fn web_send_convo(&mut self, jid: &str, body: &str) {
        let body = body.trim();
        if body.is_empty() {
            return;
        }
        let Some(tx) = &self.jabber_tx else { return };
        let room = {
            let st = self.jabber.lock().unwrap_or_else(|e| e.into_inner());
            if !st.chats.contains_key(jid) && !st.rooms.contains(jid) {
                return;
            }
            st.rooms.contains(jid)
        };
        let _ = tx.send(if room {
            crate::jabber::Cmd::SendRoom { room: jid.to_owned(), body: body.to_owned() }
        } else {
            crate::jabber::Cmd::Send { to: jid.to_owned(), body: body.to_owned() }
        });
    }

    /// Open the comms link of the ping sent at `ts`, on this machine.
    ///
    /// The phone cannot follow a `mumble://` link usefully; the client is here. So the page asks the
    /// app to join, and the app looks the link up in the pings it already holds rather than being
    /// handed a URL by something on the network.
    ///
    /// Joins through `open_mumble`, which resolves a redirect page to the real `mumble://` URL, so
    /// this lands in the Mumble client rather than in a browser tab.
    fn join_comms(&mut self, ts: i64) {
        let link = {
            let j = self.jabber.lock().unwrap_or_else(|e| e.into_inner());
            j.pings.iter().find_map(|p| match p {
                crate::pings::Ping::Fleet { timestamp, comms, .. } if *timestamp == ts => {
                    match comms {
                        Some(crate::pings::Comms::Mumble { link, .. }) => Some(link.clone()),
                        _ => None,
                    }
                }
                _ => None,
            })
        };
        match link {
            // `open_mumble`, not `open::that`. A ping's comms link is usually a gnf.lt page that
            // redirects; opening it directly opens a browser, which is what the app avoids by
            // resolving the page to its real `mumble://` URL first.
            Some(l) => open_mumble(l),
            None => eprintln!("[web] join comms: no ping at {ts} with a mumble link"),
        }
    }

    /// Clear one report from everywhere an alert is remembered.
    ///
    /// All three, not just the feed: the alert window reads `alert_shared`, the Alerts tab reads
    /// `alert_feed`, and a rule's own history reads `rule_feeds`. Clearing one would leave the same
    /// alert acknowledged in one place and outstanding in another.
    fn ack_alert(&mut self, id: u64) {
        self.alert_feed.retain(|(r, _)| r.id != id);
        for feed in self.rule_feeds.values_mut() {
            feed.retain(|(r, _, _)| r.id != id);
        }
        let mut st = self.alert_shared.lock().unwrap_or_else(|e| e.into_inner());
        st.feed.retain(|(r, _)| r.id != id);
    }

    /// The pilot window, looking the name up.
    fn open_pilot(&mut self, name: String, ctx: &egui::Context) {
        self.pilot_query = name;
        crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ctx.clone());
        self.pilot_window_open = true;
        self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
    }

    /// Where every click out of an intel card lands, whichever view or window drew the card.
    fn act_on_intel_click(&mut self, click: IntelClick, ctx: &egui::Context) {
        match click {
            IntelClick::System(id) => self.open_system(id),
            IntelClick::Ship(id) => self.open_ship(id),
            IntelClick::Pilot(name) => self.open_pilot(name, ctx),
            IntelClick::Dscan(url) => self.open_dscan(url, ctx),
            IntelClick::LocalScan(url) => self.open_local_scan(url, ctx),
            IntelClick::PilotVerdict(name) => self.open_pilot_verdict(name),
            c @ (IntelClick::Annotate(_) | IntelClick::Notes(_)) => self.notes_click(c),
        }
    }

    fn persist_alert_geometry(&mut self, moved: Option<(f32, f32)>, moved_size: Option<(f32, f32)>) {
        if let Some(p) = moved.and_then(|p| geometry_update(self.settings.alerts.window_pos, p, 0.0)) {
            self.settings.alerts.window_pos = Some(p);
            self.needs_save = true;
        }
        if let Some(s) = moved_size.and_then(|s| geometry_update(self.settings.alerts.window_size, s, 2.0)) {
            self.settings.alerts.window_size = Some(s);
            self.needs_save = true;
        }
    }

    fn persist_ping_geometry(&mut self, moved: Option<(f32, f32)>, moved_size: Option<(f32, f32)>) {
        if let Some(p) = moved.and_then(|p| geometry_update(self.settings.fleet_ping_window_pos, p, 0.0)) {
            self.settings.fleet_ping_window_pos = Some(p);
            self.needs_save = true;
        }
        if let Some(s) =
            moved_size.and_then(|s| geometry_update(self.settings.fleet_ping_window_size, s, 2.0))
        {
            self.settings.fleet_ping_window_size = Some(s);
            self.needs_save = true;
        }
    }

    /// Persist the main window's geometry. While maximized we keep the last floating pos/size (so
    /// un-maximizing returns there) and only record the maximized flag.
    fn persist_main_geometry(
        &mut self,
        pos: Option<(f32, f32)>,
        size: Option<(f32, f32)>,
        maximized: bool,
    ) {
        if self.settings.main_window_maximized != maximized {
            self.settings.main_window_maximized = maximized;
            self.needs_save = true;
        }
        if maximized {
            return;
        }
        if let Some(p) = pos.and_then(|p| geometry_update(self.settings.main_window_pos, p, 0.0)) {
            self.settings.main_window_pos = Some(p);
            self.needs_save = true;
        }
        if let Some(s) = size.and_then(|s| geometry_update(self.settings.main_window_size, s, 2.0)) {
            self.settings.main_window_size = Some(s);
            self.needs_save = true;
        }
    }

    fn persist_view_options(&mut self) {
        let pv = PersistedView {
            overlays: if self.map_mode == MapMode::Standard {
                self.map_overlays
            } else {
                self.standard_overlays
            },
            map_layout: self.map_layout,
            map_threat_jumps: self.map_threat_jumps,
            intel_max_jumps: self.intel_max_jumps,
            intel_type: self.intel_type,
        };
        if let Ok(s) = serde_json::to_string(&pv) {
            if s != self.settings.view_options {
                self.settings.view_options = s;
                self.needs_save = true;
            }
        }
    }

    fn player_system(&self) -> Option<i64> {
        let p = self.player.lock().unwrap();
        p.locations.get(&self.active_character).map(|(s, _)| *s).or(p.system_id)
    }

    /// Who this frame's cards may attribute a number to. Built once per view: a character costs a
    /// graph walk, a card costs a lookup.
    pub(crate) fn char_rings(&self) -> CharRings {
        let (locations, fallback) = {
            let p = self.player.lock().unwrap();
            (p.locations.clone(), p.system_id)
        };
        let chars: Vec<(String, i64)> =
            self.characters.iter().map(|c| (c.name.clone(), c.id)).collect();
        build_char_rings(
            &self.systems,
            &chars,
            &locations,
            &self.active_character,
            fallback,
            &self.settings.intel_disabled_chars,
            self.settings.alert_only_undocked,
            self.settings.intel_count_bridges,
        )
        .with_staging(self.staging_system())
    }

    /// The only staging system the app knows is rescue mode's, which a build without it cannot
    /// configure, so its default must not surface there.
    fn staging_system(&self) -> Option<&str> {
        (cfg!(feature = "fleet") && self.settings.fc_rescue_enabled)
            .then_some(self.settings.rescue_staging_system.as_str())
    }

    /// Systems tagged for docking in an online folder. A supercarrier or titan needs Super Docking; any
    /// other capital fits either.
    fn jump_dockable_ids(&self) -> std::collections::HashSet<i64> {
        let supers = self.jump_ship == 1;
        let wanted: &[&str] =
            if supers { &[crate::notes::SUPER_DOCKING] } else { &[crate::notes::SUPER_DOCKING, crate::notes::CAPITAL_DOCKING] };
        self.notes_view
            .systems
            .iter()
            .filter(|(_, m)| m.tags.iter().any(|t| wanted.contains(&t.as_str())))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Dock permits from before the docking tags existed, turned into those tags in the quick-edit
    /// folder once the graph can name the systems. The old list is emptied only when every permit moved.
    fn migrate_dock_permits(&mut self) {
        if self.settings.jump_dock.is_empty() || self.systems.is_none() {
            return;
        }
        let permits = std::mem::take(&mut self.settings.jump_dock);
        let mut left = Vec::new();
        for p in permits {
            let Some(id) = self.systems.as_ref().and_then(|g| g.lookup(&p.system)).map(|s| s.id) else {
                left.push(p);
                continue;
            };
            let folder = self.notes_view.target.clone();
            let mut ok = true;
            for (on, tag) in [(p.capitals, crate::notes::CAPITAL_DOCKING), (p.supers, crate::notes::SUPER_DOCKING)] {
                if on {
                    let op = crate::notes::NotesOp::SetTag {
                        folder: folder.clone(),
                        subject: crate::notes::Subject::System(id),
                        tag: tag.to_owned(),
                        on: true,
                    };
                    ok &= self.apply_notes_op(op).is_ok();
                }
            }
            if !ok {
                left.push(p);
            }
        }
        self.settings.jump_dock = left;
        self.needs_save = true;
    }

    fn ensure_jump_systems(&mut self) {
        if self.jump_systems.is_none() {
            if let Some(store) = &self.store {
                self.jump_systems = Some(std::sync::Arc::new(store.all_map_systems()));
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn jump_plan_ui(&mut self, ui: &mut egui::Ui) {
        self.jump_plan_content(ui);
    }

    /// The Layers panel only renders inside a map that needs the SDE, which headless does not have.
    #[cfg(test)]
    pub(crate) fn map_layers_ui(&mut self, ui: &mut egui::Ui) {
        self.map_layers_content(ui);
    }

    #[cfg(test)]
    pub(crate) fn docked_system_ui(&mut self, ui: &mut egui::Ui, id: i64) {
        self.system_info_body(ui, id, true);
    }

    #[cfg(test)]
    pub(crate) fn seed_lookup(&mut self, rows: Vec<(String, crate::localscan::Row)>, orgs: Vec<(i64, crate::localscan::Org)>) {
        let mut t = self.lookup_table.lock().unwrap();
        self.lookup_current = rows.iter().map(|(n, _)| n.clone()).collect();
        for (n, r) in rows {
            t.rows.insert(n.to_lowercase(), r);
        }
        t.orgs.extend(orgs);
        drop(t);
        self.standings.lock().unwrap().extend([(99_000_002, -5.0), (90_000_003, 10.0), (90_000_004, 0.0), (98_000_003, -10.0)]);
    }

    #[cfg(test)]
    pub(crate) fn seed_pilot_report(&mut self, report: crate::lookup::PilotReport) {
        self.pilot_query = report.name.clone();
        *self.pilot_lookup.lock().unwrap() = crate::lookup::LookupState::Done(report);
        self.pilot_window_open = true;
    }

    /// Swaps the backend after a sign-in or a sign-out.
    ///
    /// The generation bump orphans every read still in flight from the old backend, and the state
    /// is rebuilt so the journal cannot carry a dry-run record into a live session.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_set_backend(
        &mut self,
        backend: std::sync::Arc<dyn crate::fleets::backend::FleetBackend>,
    ) {
        self.fleet_backend = backend;
        self.fleet_gen.page += 1;
        *self.fleet.lock().unwrap_or_else(|e| e.into_inner()) = crate::fleets::FleetState {
            seed: crate::fleets::seed::load(),
            ..Default::default()
        };
        // `fleet_ui` boots on the next frame, so there is one boot path rather than two.
        self.fleet_booted = false;
    }

    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_mode_for_test(&self) -> crate::fleets::backend::Mode {
        self.fleet_backend.mode()
    }

    /// The backend the app itself would call, so a test can drive the real path instead of
    /// standing up a second one beside it.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_backend_for_test(
        &self,
    ) -> &std::sync::Arc<dyn crate::fleets::backend::FleetBackend> {
        &self.fleet_backend
    }

    /// Sends one action against whatever fleet the page is on, the way a button would.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_act_for_test(&mut self, action: crate::fleets::backend::Action) {
        let page = self.fleet.lock().unwrap().page.clone();
        let id = page.fleet().cloned().expect("no fleet on this page");
        self.fleet_dispatch(crate::fleets::state::Cmd::Act(id, action));
    }

    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_collect_for_test(&mut self) {
        self.fleet_collect();
    }

    /// The re-read a freshly closed fleet needs, which the fleet tab normally drives per frame.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_reopen_poll_for_test(&mut self) {
        let ctx = self.ui_ctx.clone();
        self.fleet_reopen_poll(&ctx);
    }

    /// Applies a start-form action the way a click would, for a test of what it does.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_apply_form_for_test(&mut self, act: crate::app::fleet_ui::FormAct) {
        self.fleet_apply_form(act);
    }

    /// The character search waiting for the typing to pause, if any.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_search_pending_for_test(&self) -> Option<String> {
        self.fleet_search_pending.as_ref().map(|(v, _)| v.clone())
    }

    /// The main window's Jabber tabs, for a test that checks what a message opened.
    #[cfg(test)]
    pub(crate) fn jabber_tabs_for_test(&self) -> Vec<String> {
        self.jabber_tabs.clone()
    }

    /// The fleet tab's shared state, so a scene can fill it the way the workers would.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn fleet_state_for_test(
        &self,
    ) -> &std::sync::Arc<std::sync::Mutex<crate::fleets::FleetState>> {
        &self.fleet
    }

    /// A map graph and its coordinates, as the SDE load would leave them.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn seed_map_world(&mut self, systems: crate::geo::Systems, coords: Vec<crate::store::MapSystem>) {
        self.systems = Some(std::sync::Arc::new(systems));
        self.map_coords = Some(std::sync::Arc::new(coords));
    }

    /// The rescue panel's shared state, so a scene can put a ping in it the way the watcher would.
    #[cfg(all(test, feature = "fleet"))]
    pub(crate) fn rescue_state_for_test(
        &self,
    ) -> &std::sync::Arc<std::sync::Mutex<crate::rescue::RescueState>> {
        &self.rescue
    }

    #[cfg(test)]
    pub(crate) fn seed_notes(&mut self, book: crate::notes::NoteBook) {
        self.systems = Some(crate::uitest::fixtures::systems());
        self.notes = std::sync::Arc::new(book);
        self.rebuild_notes_view();
    }

    /// A planned gate route between two fixture systems, so the route panel has something to draw.
    #[cfg(test)]
    pub(crate) fn seed_map_route(&mut self, from: i64, to: i64) {
        self.systems = Some(crate::uitest::fixtures::systems());
        let graph = self.systems.clone().expect("seeded");
        let opt = crate::web::route::gate(&graph, from, to, false, &Default::default(), &Default::default())
            .expect("the fixture systems are connected");
        self.map_route_kind = "gate";
        self.map_route_anchors = vec![from, to];
        self.map_route_opts = vec![opt];
        self.map_route_at = 0;
    }

    /// Turns a seeded route into the busiest row shape there is: a jump hop with its costs and a
    /// warning, which is the layout the panel has to fit.
    #[cfg(test)]
    pub(crate) fn seed_route_detail(&mut self) {
        let Some(opt) = self.map_route_opts.get_mut(self.map_route_at) else { return };
        for (i, h) in opt.hops.iter_mut().enumerate().skip(1) {
            h.kind = 2;
            h.ly = Some(4.5);
            h.fuel = Some(4200.0);
            h.fatigue_min = Some(74.0);
            h.reactivation_min = Some(12.0);
            if i % 2 == 1 {
                h.warn = Some(crate::web::route::HopWarning {
                    sev: 3,
                    at: chrono::Utc::now().timestamp() - 240,
                    kills: 6,
                    pods: 2,
                });
            }
        }
    }

    /// A fork on the first hop, which the three-system fixture graph is too small to grow.
    #[cfg(test)]
    pub(crate) fn seed_route_fork(&mut self) {
        let Some(opt) = self.map_route_opts.get_mut(self.map_route_at) else { return };
        let Some(next) = opt.path.get(1).copied() else { return };
        if let Some(h) = opt.hops.first_mut() {
            h.fork = vec![
                crate::web::route::Branch { id: next, name: "319-3D".into() },
                crate::web::route::Branch { id: 30_000_142, name: "Jita".into() },
            ];
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_jump_ship(&mut self, ship: usize) {
        self.jump_ship = ship;
    }

    #[allow(deprecated)]
    fn dialog_viewport(
        parent: &egui::Context,
        id: &str,
        title: &str,
        size: [f32; 2],
        content: impl FnOnce(&mut egui::Ui),
    ) -> bool {
        dialog_viewport_ext(parent, id, title, size, false, content)
    }

    fn persist(&mut self) {
        // Back off rather than retry every frame: ~90 sites set `needs_save`, `persist_main_geometry`
        // among them, so a failing save would otherwise be a 60Hz loop of failing writes.
        if self.persist_retry_at.is_some_and(|t| std::time::Instant::now() < t) {
            return;
        }
        let Some(store) = &self.store else {
            self.needs_save = false;
            return;
        };
        match store.save_settings(&self.settings) {
            Ok(()) => {
                self.persist_retry_at = None;
                self.needs_save = false;
            }
            Err(e) => {
                // Keep `needs_save` set, or settings silently stop saving for the rest of the
                // session, reported only to a console a release build does not have.
                eprintln!("save settings: {e:#}");
                self.store_error = Some(format!("settings could not be saved ({e:#})"));
                self.persist_retry_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar")
            .exact_size(40.0)
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Character").weak());

                    // egui's defaults are a 100px button and a 200px popup, which truncate long
                    // pilot names and cap the list at ~7 rows. Size the button to the widest name
                    // and let the popup grow into the window.
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let widest = std::iter::once("No character")
                        .chain(self.characters.iter().map(|c| c.name.as_str()))
                        .map(|name| {
                            ui.painter()
                                .layout_no_wrap(
                                    name.to_owned(),
                                    font.clone(),
                                    egui::Color32::PLACEHOLDER,
                                )
                                .size()
                                .x
                        })
                        .fold(0.0_f32, f32::max);
                    // Room for the dropdown arrow and the button's own padding.
                    let combo_w = (widest + 44.0).clamp(180.0, 360.0);
                    // Budget for the top bar the popup hangs off plus the status bar below it.
                    let popup_h =
                        (ui.ctx().content_rect().height() - 120.0).clamp(200.0, 720.0);

                    let before = self.active_character.clone();
                    egui::ComboBox::from_id_salt("active_character")
                        .selected_text(&self.active_character)
                        .width(combo_w)
                        .height(popup_h)
                        .show_ui(ui, |ui| {
                            ui.menu_value(
                                &mut self.active_character,
                                "No character".to_owned(),
                                "No character",
                            );
                            for c in &self.characters {
                                ui.menu_value(
                                    &mut self.active_character,
                                    c.name.clone(),
                                    &c.name,
                                );
                            }
                        });
                    if self.active_character != before {
                        self.remember_active_character();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        let clock = if self.settings.use_eve_time {
                            format!("{} EVE", chrono::Utc::now().format("%H:%M"))
                        } else {
                            format!("{} Local", chrono::Local::now().format("%H:%M"))
                        };
                        ui.label(egui::RichText::new(clock).monospace());
                        ui.separator();
                        let esi_ok = !self.system_status.lock().unwrap().is_empty();
                        let (icon, text, col) = if esi_ok {
                            (
                                egui_phosphor::regular::PLUGS_CONNECTED,
                                "ESI online",
                                egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
                            )
                        } else {
                            (egui_phosphor::regular::PLUGS, "ESI offline", ui.visuals().weak_text_color())
                        };
                        ui.label(egui::RichText::new(format!("{icon}  {text}")).color(col));
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(30.0)
            .show_inside(ui, |ui| {
                self.proc_monitor.tick();
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    let intel = self.intel_state.lock().unwrap().reports.len();
                    ui.label(format!("Intel: {intel}"));
                    ui.separator();
                    ui.label(egui::RichText::new(&self.active_character).weak());
                    ui.separator();
                    ui.label(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
                    if let Some(av) = self.update.lock().unwrap().available.clone() {
                        if av.version != self.settings.update_skip_version {
                            ui.label(
                                egui::RichText::new(format!("● v{} available", av.version))
                                    .color(egui::Color32::from_rgb(0x5a, 0xc8, 0x7a)),
                            )
                            .on_hover_text("A newer version is available. See the update prompt.");
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "{:.0} fps   CPU {:.0}%   RAM {}",
                                self.frame_ms.max(0.1).recip() * 1000.0,
                                self.proc_monitor.cpu_percent,
                                self.proc_monitor.rss_human(),
                            ))
                            .weak(),
                        )
                        .on_hover_text(format!(
                            "{:.1} ms per frame, the slowest of the last 120 · CPU (share of one \
                             core) · resident memory",
                            self.frame_ms
                        ));
                        if self.disk_level != crate::disk::Level::Normal {
                            if let Some(free) = self.disk_free {
                                ui.label(
                                    egui::RichText::new(format!("Disk {}", fmt_bytes(free)))
                                        .color(crate::theme::standing::WARNING),
                                )
                                .on_hover_text("Free space where EVE Spai stores its data");
                            }
                        }
                    });
                });
            });
    }

    fn nav_rail(&mut self, ui: &mut egui::Ui) {
        let width = if self.settings.nav_expanded {
            nav::WIDTH_EXPANDED
        } else {
            nav::WIDTH_COLLAPSED
        };
        let badge = self.jabber_has_unread();
        // Only once a session has worked: before that the Jabber page shows its own login state.
        let jabber_down = {
            let s = self.jabber.lock().unwrap();
            s.ever_online && !s.connected
        };
        egui::Panel::left("nav_rail")
            .resizable(false)
            .exact_size(width)
            .show_inside(ui, |ui| {
                let mut expanded = self.settings.nav_expanded;
                let badged: &[nav::View] = if badge { &[nav::View::Jabber] } else { &[] };
                let warned: &[nav::View] = if jabber_down { &[nav::View::Jabber] } else { &[] };
                let has_fleet = cfg!(feature = "fleet") && self.settings.fleet_enabled;
                // A rescue runs on a fleet preset and hands over to the fleet tab, so it is part
                // of fleet command rather than a feature beside it. It keeps its own switch only
                // because it also needs the delve911 rooms joined.
                let has_rescue = has_fleet && self.settings.fc_rescue_enabled;
                let rows: Vec<nav::View> = nav::View::primary()
                    .iter()
                    .copied()
                    .filter(|v| *v != nav::View::Rescue || has_rescue)
                    .filter(|v| *v != nav::View::Fleet || has_fleet)
                    .collect();
                let selected = nav::rail(ui, self.view, &mut expanded, badged, warned, &rows);
                if selected != self.view {
                    self.view = selected;
                }
                if expanded != self.settings.nav_expanded {
                    self.settings.nav_expanded = expanded;
                    self.needs_save = true;
                }
            });
    }

    /// The window chrome, split out of `App::ui` so the UI harness can render a whole window
    /// without the poll/side-effect prologue that surrounds it there.
    pub(crate) fn root_chrome(&mut self, ui: &mut egui::Ui) {
        self.top_bar(ui);
        // Between the two so it spans the full width above the nav rail, and so every view gets
        // it without any of them knowing about it.
        self.disk_banner(ui);
        self.auth_banner(ui);
        self.status_bar(ui);
        self.nav_rail(ui);
    }

    /// Low disk space, and whether the archive has already stopped. Rendered from `root_chrome`
    /// rather than per view, because the news is the same everywhere.
    pub(crate) fn disk_banner(&mut self, ui: &mut egui::Ui) {
        let level = self.disk_level;
        if level == crate::disk::Level::Normal {
            self.disk_banner_dismissed = None;
            return;
        }
        // A worse level is new news, so a dismissal of the milder one does not cover it.
        if self.disk_banner_dismissed == Some(level) {
            return;
        }
        let free = self.disk_free.map(fmt_bytes);
        let warn = crate::theme::standing::WARNING;
        // No `exact_size`: the wording has to wrap on a narrow window rather than clip.
        egui::Panel::top("disk_banner").show_inside(ui, |ui| {
            ui.add_space(4.0);
            // Headline and reading on one row, the explanation on its own: wrapping a long label
            // inline beside short ones makes its box span the whole row and cover them.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(egui_phosphor::regular::WARNING).color(warn).strong());
                let headline = match level {
                    crate::disk::Level::Critical => "Disk almost full.",
                    _ => "Low disk space.",
                };
                ui.label(egui::RichText::new(headline).color(warn).strong());
                // A number from a poll that has not fired yet would be a worse answer than saying
                // what actually happened.
                if self.disk_saw_failure {
                    ui.label("A save just failed because the disk is full.");
                } else if let Some(free) = &free {
                    ui.label(format!("{free} free where EVE Spai stores its data."));
                }
            });
            ui.label(match level {
                crate::disk::Level::Critical => {
                    "EVE Spai has stopped recording history (battles, kill details, chat, cached \
                     images) so it can keep running. Intel and alerts still work normally. \
                     Recording resumes on its own when space is free."
                }
                _ => "Old battle history is being trimmed. Free some space to keep recording.",
            });
            ui.horizontal(|ui| {
                if ui.button("Open data folder").clicked() {
                    if let Ok(dir) = crate::store::data_dir() {
                        let _ = open::that(dir);
                    }
                }
                // Critical has no dismiss: it reports a live degradation, and it clears itself.
                if level == crate::disk::Level::Low && ui.button("Dismiss").clicked() {
                    self.disk_banner_dismissed = Some(level);
                }
            })
            .response
            .on_hover_text(match (crate::store::data_dir(), crate::disk::measured_at()) {
                (Ok(d), Some(at)) => format!(
                    "Measured at {}, {}s ago",
                    d.display(),
                    (chrono::Utc::now().timestamp() - at).max(0)
                ),
                (Ok(d), None) => format!("Measured at {}", d.display()),
                _ => "Could not resolve the data folder".to_owned(),
            });
            ui.add_space(4.0);
        });
    }

    /// A character whose EVE login has stopped working, said out loud.
    ///
    /// An expired refresh token makes every ESI call quietly return nothing, and the only symptom
    /// is the map no longer showing where they are. No dismiss, because it does not clear itself:
    /// logging in again clears it.
    pub(crate) fn auth_banner(&mut self, ui: &mut egui::Ui) {
        let hurt: Vec<(i64, String, crate::esi::AuthProblem)> = self
            .characters
            .iter()
            .filter_map(|c| crate::esi::auth_problem(c.id).map(|p| (c.id, c.name.clone(), p)))
            .collect();
        if hurt.is_empty() {
            return;
        }
        let warn = crate::theme::standing::WARNING;
        let keyring = hurt.iter().any(|(_, _, p)| *p == crate::esi::AuthProblem::NoKeychain);
        let mut login = false;
        egui::Panel::top("auth_banner").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(egui_phosphor::regular::WARNING).color(warn).strong());
                let head = if hurt.len() == 1 {
                    hurt[0].2.message(&hurt[0].1)
                } else {
                    format!("{} characters have lost their EVE login.", hurt.len())
                };
                ui.label(egui::RichText::new(head).color(warn).strong());
            });
            if keyring {
                ui.label(
                    "Neither the system keychain nor the encrypted fallback could give the saved \
                     login back. If the profile directory was copied here from another machine or \
                     another user account, the fallback cannot open it by design; logging in again \
                     replaces it.",
                );
            } else {
                ui.label(
                    "Location, fleet membership and in-game routes stay blank until the character \
                     is logged in again. Intel, alerts and jabber are unaffected.",
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Log in again").clicked() {
                    login = true;
                }
                if hurt.len() > 1 {
                    ui.label(
                        egui::RichText::new(
                            hurt.iter().map(|(_, n, _)| n.as_str()).collect::<Vec<_>>().join(", "),
                        )
                        .weak(),
                    );
                }
            });
            ui.add_space(4.0);
        });
        if login {
            self.start_login(&ui.ctx().clone());
        }
    }

    /// Every dialog and secondary window, split out of `App::ui` for the same reason as
    /// [`Self::root_chrome`]: the harness can only reach them without the poll/side-effect
    /// prologue.
    pub(crate) fn root_dialogs(&mut self, ctx: &egui::Context, jframe: Option<&JabberFrame>) {
        #[cfg(feature = "fleet")]
        self.fleet_boss_detail_window(ctx);
        #[cfg(feature = "fleet")]
        self.fleet_snowflakes_window(ctx);
        #[cfg(feature = "fleet")]
        self.fleet_migrate_window(ctx);
        #[cfg(feature = "fleet")]
        self.fleet_boost_detail_window(ctx);
        #[cfg(feature = "fleet")]
        self.fleet_preset_rename_window(ctx);
        self.intel_channels_window(ctx);
        self.jump_bridges_window(ctx);
        self.sov_upgrades_window(ctx);
        self.coalitions_window(ctx);
        self.travel_sov_dialog(ctx);
        self.severity_window(ctx);
        self.test_intel_dialog(ctx);
        self.note_editor_window(ctx);
        self.notes_manager_window(ctx);
        self.alert_window(ctx);
        self.system_window(ctx);
        self.constellation_window(ctx);
        self.region_window(ctx);
        self.ship_window(ctx);
        self.route_intel_window(ctx);
        self.map_alts_window(ctx);
        self.map_route_store_windows(ctx);
        self.pilot_window(ctx);
        self.fit_window(ctx);
        self.battle_filter_dialog(ctx);
        self.filter_picker_dialog(ctx);
        self.verdict_dialog(ctx);
        self.dscan_view_dialog(ctx);
        self.fleet_ping_window_ui(ctx);
        self.routes_dialog(ctx);
        self.safety_watch(ctx);
        self.screen_flash(ctx);
        if let Some(vp) = self.focus_window.take() {
            ctx.send_viewport_cmd_to(vp, egui::ViewportCommand::Focus);
        }
        if self.map_popped {
            self.show_map_viewport(ctx);
        }
        self.char_popout_windows(ctx);
        if let Some(f) = jframe {
            self.jabber_popout_windows(ctx, f);
        }
        self.cyno_generators_window(ctx);
        #[cfg(feature = "fleet")]
        if self.settings.fc_rescue_enabled {
            // The feature being on is the mode. `active` still gates the pollers, and nothing else
            // sets it, so the fleet poller would otherwise wait forever.
            {
                let mut r = self.rescue.lock().unwrap_or_else(|e| e.into_inner());
                if !r.active {
                    r.active = true;
                    r.select_newest();
                }
            }
            self.update_rescue_range();
        }
    }

    /// The central panel and its dispatch on [`Self::view`]. `jframe` stays a parameter because
    /// `App::ui` builds it once per frame and reuses it for the popout windows afterwards.
    pub(crate) fn root_central(&mut self, ui: &mut egui::Ui, jframe: Option<&JabberFrame>) {
        egui::CentralPanel::default().show_inside(ui, |ui| match self.view {
            View::Dashboard => self.dashboard_view(ui),
            View::Map => self.map_view(ui),
            View::Characters => self.characters_view(ui),
            View::Intel => self.intel_view(ui),
            View::Battles => self.battles_view(ui),
            View::Wormholes => self.wormholes_view(ui),
            View::Lookup => self.lookup_view(ui),
            View::Alerts => self.alerts_view(ui),
            View::Jabber => {
                if let Some(f) = jframe {
                    self.jabber_view(ui, f);
                }
            }
            // In the main window, not a viewport of its own: a separate always-on-top window is a
            // second place to look and a second thing to lose behind the game client.
            View::Fleet => self.fleet_view(ui),
            View::Rescue => self.rescue_view(ui),
            View::Settings => self.settings_view(ui),
        });
        // Its own window, opened from settings and from a tracked fleet, so it cannot live inside
        // either view's body.
        if self.fleet_boost_editor(ui.ctx()) {
            self.needs_save = true;
        }
    }
}

impl eframe::App for SpaiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(feature = "fleet")]
        self.fleet_boot_once();
        #[cfg(feature = "fleet")]
        self.fleet_track_poll();

        // Cached here rather than in the `cumulative_pass_nr() > 30` block below, which would
        // starve cross-window drop hit-testing for the first 30 frames.
        self.jabber_main_rect = ctx.input(|i| {
            let vp = i.viewport();
            vp.outer_rect.zip(vp.inner_rect)
        });
        // Safety net for a drag whose source never saw the release (focus stolen, or the tab
        // overflowed off the bar mid-drag): a stale drag would pin a highlight on forever. The
        // source re-arms `alive` each frame it is still dragging, so a drag goes stale one frame
        // after its window stops reporting it.
        if let Some(d) = &mut self.jabber_tab_drag {
            let jid = d.jid.clone();
            let alive = std::mem::take(&mut d.alive);
            if !alive || self.tab_set().owner(&jid).is_none() {
                self.jabber_tab_drag = None;
            }
        }

        for c in std::mem::take(&mut self.pending_overlay_clicks) {
            self.act_on_intel_click(c, &ctx);
        }

        // The page's actions are the overlay's actions: same enum, same handlers. Drained here so
        // there is one place that decides what a verdict or an acknowledgement does.
        if self.settings.web.enabled {
            let mut d = self.web_detail.lock().unwrap_or_else(|e| e.into_inner());
            if d.wake.is_none() {
                d.wake = Some(ctx.clone());
            }
        }
        let from_web =
            std::mem::take(&mut *self.web_inbox.lock().unwrap_or_else(|e| e.into_inner()));
        for m in from_web {
            self.apply_overlay_message(m, &ctx);
        }

        // `|`, not `||`: short-circuiting would leave `raise_main` set whenever a second-instance
        // request fired first, raising the window again on a later frame.
        if crate::instance::take_raise_request() | std::mem::take(&mut self.raise_main) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
            self.raise_reset_top = true;
            ctx.request_repaint();
        } else if self.raise_reset_top {
            self.raise_reset_top = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        }

        if let Some(link) = self.overlay.as_mut() {
            link.poll();
        }
        {
            let msgs = self.overlay.as_ref().map(|l| l.drain_inbox()).unwrap_or_default();
            for m in msgs {
                self.apply_overlay_message(m, &ctx);
            }
        }

        #[cfg(feature = "fleet")]
        if self.settings.fc_rescue_enabled {
            self.ingest_delve911_jabber();
            self.drain_rescue_feed(&ctx);
        }

        let cur_sys = self.player_system().unwrap_or(0);
        self.player_sys_shared.store(cur_sys, std::sync::atomic::Ordering::Relaxed);
        if crate::geo::is_wormhole_system(cur_sys) {
            let now = chrono::Utc::now().timestamp();
            let mut wh = self.recent_wh.lock().unwrap();
            wh.insert(cur_sys, now);
            wh.retain(|_, t| now - *t <= 600);
        }

        if let Some(tray) = self.tray.clone() {
            if tray.take_show() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if tray.exit_requested() {
                self.really_exit = true;
            }
            tray.set_unread(self.jabber_unread_total());
        }
        // The taskbar carries the same number as the tray, for the same reason: the window is very
        // often behind something else when a message lands.
        let unread = self.jabber_unread_total();
        self.sync_taskbar_badge(&ctx, unread);
        if ctx.input(|i| i.viewport().close_requested())
            && !self.really_exit
            && self.settings.minimize_to_tray
            && self.tray.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if self.really_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Worst of the window, not the average: a feed that hitches every few frames still reads
        // as smooth on a mean.
        self.frame_worst.push(ctx.input(|i| i.unstable_dt) * 1000.0);
        if self.frame_worst.len() >= 120 {
            self.frame_ms = self.frame_worst.iter().copied().fold(0.0, f32::max);
            self.frame_worst.clear();
        }

        self.settings.theme.apply(&ctx);

        self.refresh_characters();
        self.disk_level = crate::disk::level();
        self.disk_free = crate::disk::available();
        self.disk_saw_failure = crate::disk::saw_failure();
        self.player.lock().unwrap().active_name = self.active_character.clone();
        self.maybe_start_watcher(&ctx);
        self.maybe_start_jabber(&ctx);
        self.load_persisted_kills();
        self.reload_wormholes();
        self.poll_update_check(&ctx);
        self.update_dialog(&ctx);
        self.update_check_dialog(&ctx);
        self.store_warning_dialog(&ctx);
        if !self.wizard_checked {
            self.wizard_checked = true;
            self.wizard_open = !self.settings.wizard_done;
        }
        self.setup_wizard(&ctx);
        self.poll_dscan_clipboard(&ctx);
        self.poll_local_scan(&ctx);
        self.maybe_refresh_standings(&ctx);
        self.poll_jabber_notify(&ctx);
        self.poll_kill_fetches();
        self.dscan_dialog(&ctx);
        self.ping_rules_dialog(&ctx);
        self.maybe_rebuild_graph(&ctx);
        self.persist_view_options();
        self.discover_sov_alliances(&ctx);
        self.drain_alerts();
        self.root_chrome(ui);

        // Reconciliation runs once per frame, before any chat window renders: per-window it would
        // let one window re-add a tab another one owns.
        let jframe = (self.view == View::Jabber || !self.jabber_popouts.is_empty())
            .then(|| self.jabber_frame(ctx.input(|i| i.focused)));
        if let Some(f) = &jframe {
            self.jabber_reconcile(f);
        }
        self.sync_popout_settings();

        self.root_central(ui, jframe.as_ref());

        self.root_dialogs(&ctx, jframe.as_ref());

        // Remember the main window's location + size across restarts. Skip the first passes, where
        // the window can briefly report a pre-restore rect.
        if ctx.cumulative_pass_nr() > 30 {
            let (pos, maximized, minimized) = ctx.input(|i| {
                let vp = i.viewport();
                (
                    vp.outer_rect.map(|r| (r.min.x, r.min.y)),
                    vp.maximized.unwrap_or(false),
                    vp.minimized.unwrap_or(false),
                )
            });
            if !minimized {
                let s = ctx.content_rect().size();
                let size = (s.x > 0.0 && s.y > 0.0).then_some((s.x, s.y));
                self.persist_main_geometry(pos, size, maximized);
            }
        }

        if self.needs_save {
            self.persist();
        }
    }

    fn on_exit(&mut self) {
        if let Some(link) = self.overlay.as_mut() {
            link.shutdown();
        }
        self.persist();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // A transparent backbuffer (the default) lets the idle alert window and the map
        // overlay be genuinely see-through; the main window and popped-out map cover their
        // backbuffer with opaque panels, so they still look solid. A semi-opaque clear leaks
        // through as a dark idle alert window. EVE_SPAI_OPAQUE forces a solid clear if a driver
        // mis-presents transparency.
        if crate::transparency_enabled() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            crate::theme::chip::KILL_CARD_BG.to_normalized_gamma_f32()
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn active_window() -> Option<(String, String)> {
    None
}

#[cfg(target_os = "linux")]
fn active_window() -> Option<(String, String)> {
    use std::process::Command;
    let id = Command::new("xdotool").arg("getactivewindow").output().ok()?;
    if !id.status.success() {
        return None;
    }
    let id = String::from_utf8_lossy(&id.stdout).trim().to_owned();
    if id.is_empty() {
        return None;
    }
    let name = Command::new("xdotool").args(["getwindowname", &id]).output().ok()?;
    Some((id, String::from_utf8_lossy(&name.stdout).trim().to_owned()))
}

pub(crate) fn eve_is_focused() -> bool {
    match active_window() {
        Some((_, name)) if !name.is_empty() => {
            let n = name.to_lowercase();
            n.contains("eve") && !n.contains("eve spai")
        }
        _ => true,
    }
}

#[cfg(not(target_os = "linux"))]
fn eve_window_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

#[cfg(target_os = "linux")]
fn eve_window_rect() -> Option<(i32, i32, i32, i32)> {
    use std::process::Command;
    let out = Command::new("xdotool").args(["search", "--name", "EVE"]).output().ok()?;
    for id in String::from_utf8_lossy(&out.stdout).split_whitespace() {
        let name = Command::new("xdotool").args(["getwindowname", id]).output().ok()?;
        let n = String::from_utf8_lossy(&name.stdout).to_lowercase();
        if !n.contains("eve") || n.contains("eve spai") {
            continue;
        }
        let geo =
            Command::new("xdotool").args(["getwindowgeometry", "--shell", id]).output().ok()?;
        let g = String::from_utf8_lossy(&geo.stdout);
        let val = |k: &str| g.lines().find_map(|l| l.strip_prefix(k)?.trim().parse::<i32>().ok());
        if let (Some(x), Some(y), Some(w), Some(h)) =
            (val("X="), val("Y="), val("WIDTH="), val("HEIGHT="))
        {
            return Some((x, y, w, h));
        }
    }
    None
}

fn notify(summary: String, body: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .summary(&summary)
            .body(&body)
            .timeout(notify_rust::Timeout::Milliseconds(8000))
            .show();
    });
}

/// `dashed_flow` along a polyline, so an arc crawls the same way a straight leg does.
///
/// Per segment with the phase carried forward, rather than per segment from zero: restarting the
/// pattern at every sample of a fourteen-point arc turns a crawl into a shimmer.
fn polyline_flow(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    color: egui::Color32,
    phase: f32,
) {
    polyline_flow_gradient(painter, pts, color, color, phase);
}

/// [`polyline_flow`] shading from `from` to `to`, for a bridge coloured by the zones at its ends.
pub(crate) fn polyline_flow_gradient(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    from: egui::Color32,
    to: egui::Color32,
    phase: f32,
) {
    let total: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let len = (w[1] - w[0]).length();
        let t = if total > 0.0 { (walked + len * 0.5) / total } else { 0.0 };
        dashed_flow(painter, w[0], w[1], lerp_color(from, to, t), phase - walked);
        walked += len;
    }
}

fn dashed_flow(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32, phase: f32) {
    let dir = p2 - p1;
    let len = dir.length();
    if len < 1.0 {
        return;
    }
    let unit = dir / len;
    let (dash, period) = (6.0f32, 12.0f32);
    let mut d = (phase % period) - period;
    let stroke = egui::Stroke::new(2.0, color);
    while d < len {
        let s = d.max(0.0);
        let e = (d + dash).min(len);
        if e > s {
            painter.line_segment([p1 + unit * s, p1 + unit * e], stroke);
        }
        d += period;
    }
}

fn resolve_system(graph: &crate::geo::Systems, raw: &str) -> Option<String> {
    let tok = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'');
    if tok.len() < 2 {
        return None;
    }
    graph.lookup(tok).map(|i| i.name.clone())
}

fn parse_bridges(text: &str, graph: &crate::geo::Systems) -> Vec<crate::settings::JumpBridge> {
    crate::ansiblex::dotlan_pairs(text)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(a, b)| {
            let (from, to) = (resolve_system(graph, &a)?, resolve_system(graph, &b)?);
            (from != to).then_some(crate::settings::JumpBridge { from, to })
        })
        .collect()
}

const BAKED_DEFAULTS: u32 = 1;
const BAKED_REGION: &str = "Insmother";
const BAKED_BRIDGES: &str = include_str!("../assets/default_ansiblex.txt");
const BAKED_UPGRADES: &str = include_str!("../assets/default_sov_upgrades.txt");

/// Applies the bundled bridge network and sov upgrades once per [`BAKED_DEFAULTS`] bump. Staging
/// in `region` overwrites both lists; anyone else only gets them where their list is empty.
/// Returns whether settings changed.
fn apply_baked_defaults(
    settings: &mut crate::settings::Settings,
    graph: &crate::geo::Systems,
    region: &str,
    bridges: &str,
    upgrades: &str,
) -> bool {
    if settings.baked_defaults >= BAKED_DEFAULTS {
        return false;
    }
    let home = graph.lookup(settings.rescue_staging_system.trim()).is_some_and(|i| i.region == region);
    if home || settings.jump_bridges.is_empty() {
        settings.jump_bridges = bridges
            .lines()
            .filter_map(|l| {
                let (a, b) = l.split_once("::")?;
                let (from, to) = (resolve_system(graph, a)?, resolve_system(graph, b)?);
                Some(crate::settings::JumpBridge { from, to })
            })
            .collect();
    }
    if home || settings.sov_upgrades.is_empty() {
        settings.sov_upgrades = parse_sov_upgrades(upgrades, graph);
    }
    settings.baked_defaults = BAKED_DEFAULTS;
    true
}

pub(crate) fn split_upgrade_label(label: &str) -> Vec<&str> {
    label
        .split(',')
        .map(|u| u.trim_start_matches("<-").trim())
        .filter(|u| !u.is_empty())
        .collect()
}

fn parse_sov_upgrades(text: &str, graph: &crate::geo::Systems) -> Vec<crate::settings::SovUpgrade> {
    let lines: Vec<&str> = text.lines().collect();
    if let Some(rest) = lines.first().and_then(|l| l.trim().strip_prefix("Sovereignty Hub ")) {
        if let Some(name) = resolve_system(graph, rest.trim()) {
            let mut out = Vec::new();
            for l in &lines[1..] {
                let parts: Vec<&str> = l.split('\t').collect();
                if parts.len() >= 2 && parts[0].trim().chars().all(|c| c.is_ascii_digit()) {
                    let upgrade = parts[1].trim();
                    if !upgrade.is_empty() {
                        out.push(crate::settings::SovUpgrade {
                            system: name.clone(),
                            upgrade: upgrade.to_owned(),
                        });
                    }
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    let mut out = Vec::new();
    for line in lines {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((idx, name)) =
            words.iter().enumerate().find_map(|(i, w)| resolve_system(graph, w).map(|n| (i, n)))
        else {
            continue;
        };
        let upgrade: String = words
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != idx)
            .map(|(_, w)| *w)
            .collect::<Vec<_>>()
            .join(" ");
        out.push(crate::settings::SovUpgrade { system: name, upgrade: upgrade.trim().to_owned() });
    }
    out
}

fn nearest_system(
    p: egui::Pos2,
    pos: &std::collections::HashMap<i64, egui::Pos2>,
    threshold: f32,
) -> Option<i64> {
    let mut best: Option<(i64, f32)> = None;
    for (id, sp) in pos {
        let d = sp.distance(p);
        if d <= threshold && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((*id, d));
        }
    }
    best.map(|(id, _)| id)
}

#[allow(clippy::type_complexity)]
fn bfs_tree(
    graph: &crate::geo::Systems,
    center: i64,
    depth: u32,
    use_bridges: bool,
) -> (
    std::collections::HashMap<i64, u32>,
    std::collections::HashMap<i64, Vec<i64>>,
    Vec<i64>,
) {
    use std::collections::{HashMap, VecDeque};
    let mut dist: HashMap<i64, u32> = HashMap::from([(center, 0)]);
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut order = vec![center];
    let mut queue = VecDeque::from([center]);
    while let Some(s) = queue.pop_front() {
        let d = dist[&s];
        if d >= depth {
            continue;
        }
        let mut ns: Vec<i64> = if use_bridges {
            graph.neighbors(s).to_vec()
        } else {
            graph.neighbors_gates_only(s).to_vec()
        };
        ns.sort_unstable();
        for n in ns {
            if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(n) {
                e.insert(d + 1);
                children.entry(s).or_default().push(n);
                order.push(n);
                queue.push_back(n);
            }
        }
    }
    (dist, children, order)
}

fn assign_fracs(
    node: i64,
    children: &std::collections::HashMap<i64, Vec<i64>>,
    total_leaves: f32,
    next_leaf: &mut u32,
    out: &mut std::collections::HashMap<i64, f32>,
) -> f32 {
    match children.get(&node) {
        Some(kids) if !kids.is_empty() => {
            let mut sum = 0.0;
            for &k in kids {
                sum += assign_fracs(k, children, total_leaves, next_leaf, out);
            }
            let f = sum / kids.len() as f32;
            out.insert(node, f);
            f
        }
        _ => {
            let f = (*next_leaf as f32 + 0.5) / total_leaves;
            *next_leaf += 1;
            out.insert(node, f);
            f
        }
    }
}

fn is_kspace(id: i64) -> bool {
    (30_000_000..31_000_000).contains(&id)
}
fn is_jspace(id: i64) -> bool {
    (31_000_000..32_000_000).contains(&id)
}

#[derive(Default, Clone)]
struct WhOverlay {
    direct: Vec<(i64, i64)>,
    chains: Vec<(i64, i64, usize)>,
    jspace_holes: std::collections::HashSet<i64>,
    thera_conns: Vec<i64>,
}

impl WhOverlay {
    fn build(whs: &[crate::wormholes::Wormhole]) -> WhOverlay {
        use std::collections::{HashMap, HashSet, VecDeque};
        const MAX_J_HOPS: usize = 4;
        const MAX_CHAINS: usize = 60;
        const MAX_HUB_DEGREE: usize = 6;

        use crate::wormholes::DestClass;
        // Turnur is itself K-space, so a hole to it is a K→K edge the is_jspace test below misses.
        let notable_dest =
            |d: DestClass| matches!(d, DestClass::Wspace | DestClass::Thera | DestClass::Turnur);
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut jspace_holes: HashSet<i64> = HashSet::new();
        for w in whs {
            let a = w.system_id;
            if is_kspace(a) && notable_dest(w.dest) {
                jspace_holes.insert(a);
            }
            if let Some(b) = w.dest_system_id {
                adj.entry(a).or_default().push(b);
                adj.entry(b).or_default().push(a);
                if is_kspace(a) && is_jspace(b) {
                    jspace_holes.insert(a);
                }
                if is_kspace(b) && is_jspace(a) {
                    jspace_holes.insert(b);
                }
            }
        }
        let degree: HashMap<i64, usize> =
            adj.iter().map(|(k, v)| (*k, v.len())).collect();

        let mut direct: Vec<(i64, i64)> = Vec::new();
        let mut chains: Vec<(i64, i64, usize)> = Vec::new();
        let mut seen: HashSet<(i64, i64)> = HashSet::new();
        let mut starts: Vec<i64> = adj.keys().copied().filter(|id| is_kspace(*id)).collect();
        starts.sort_unstable();
        for &start in &starts {
            let mut visited: HashSet<i64> = HashSet::from([start]);
            let mut q: VecDeque<(i64, usize)> = VecDeque::from([(start, 0usize)]);
            while let Some((node, jhops)) = q.pop_front() {
                for &nb in adj.get(&node).into_iter().flatten() {
                    if is_kspace(nb) {
                        if nb == start {
                            continue;
                        }
                        let key = (start.min(nb), start.max(nb));
                        if seen.insert(key) {
                            if jhops == 0 {
                                direct.push(key);
                            } else {
                                chains.push((key.0, key.1, jhops));
                            }
                        }
                    } else if is_jspace(nb)
                        && !visited.contains(&nb)
                        && jhops < MAX_J_HOPS
                        && degree.get(&nb).copied().unwrap_or(0) <= MAX_HUB_DEGREE
                    {
                        visited.insert(nb);
                        q.push_back((nb, jhops + 1));
                    }
                }
            }
        }
        chains.sort_by_key(|c| c.2);
        chains.truncate(MAX_CHAINS);
        const THERA: i64 = 31_000_005;
        let thera_conns: Vec<i64> = adj
            .get(&THERA)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| is_kspace(*id))
            .collect();
        WhOverlay { direct, chains, jspace_holes, thera_conns }
    }
}

pub(crate) fn eve_time_label(ts: i64, now: i64) -> String {
    use chrono::{Datelike, TimeZone, Utc};
    let Some(t) = Utc.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    let n = Utc.timestamp_opt(now, 0).single().unwrap_or(t);
    // Seconds, not minutes: intel and rescue traffic is ordered and read back at that resolution,
    // and two reports half a minute apart must not stamp identically.
    if t.year() == n.year() && t.ordinal() == n.ordinal() {
        format!("EVE {}", t.format("%H:%M:%S"))
    } else {
        format!("EVE {}", t.format("%Y/%m/%d %H:%M:%S"))
    }
}

fn render_message_body(ui: &mut egui::Ui, body: &str) {
    if body.is_empty() {
        // Nothing is emitted for an empty body, and a tight row is then literally 0.0px, so the
        // message collapses into the one above it.
        ui.allocate_space(egui::vec2(0.0, ui.text_style_height(&egui::TextStyle::Body)));
        return;
    }
    render_linked_text(ui, body, false);
}

/// Trailing sentence punctuation is not part of the URL, and copying it produces a link that fails
/// when pasted. Brackets and quotes match what `intel::extract_links` already strips.
fn trim_url_tail(tok: &str) -> &str {
    tok.trim_end_matches(|c: char| ".,;:!?)]}>\"'".contains(c))
}

/// Right-click "Copy" for a link. Copying matters because the game client has no browser: a link is
/// often something to paste into chat or a fleet broadcast, not to open.
fn link_copy_menu(resp: &egui::Response, url: &str) {
    resp.context_menu(|ui| {
        if ui.button(format!("{}  Copy", egui_phosphor::regular::COPY)).clicked() {
            ui.ctx().copy_text(url.to_owned());
            ui.close();
        }
    });
}

fn render_link(ui: &mut egui::Ui, url: &str) {
    let resp = ui.hyperlink_to(url, url);
    link_copy_menu(&resp, url);
}

/// Body text with every http(s) URL turned into a link. Emits inline widgets, so the caller must
/// already be in a `horizontal_wrapped` (or another horizontal layout).
fn render_linked_text(ui: &mut egui::Ui, body: &str, weak: bool) {
    let styled = |s: &str| {
        let t = egui::RichText::new(s);
        if weak {
            t.weak()
        } else {
            t
        }
    };
    let mut rest = body;
    while let Some(rel) = rest.find("http") {
        let after = &rest[rel..];
        if after.starts_with("http://") || after.starts_with("https://") {
            if rel > 0 {
                ui.label(styled(&rest[..rel]));
            }
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            let tok = &after[..end];
            let url = trim_url_tail(tok);
            render_link(ui, url);
            if url.len() < tok.len() {
                ui.label(styled(&tok[url.len()..]));
            }
            rest = &after[end..];
        } else {
            ui.label(styled(&rest[..rel + 4]));
            rest = &rest[rel + 4..];
        }
    }
    if !rest.is_empty() {
        ui.label(styled(rest));
    }
}

/// A ping's free text. Laid out a line at a time: ping bodies are multi-line, and a single wrapped
/// row would put everything after an embedded newline beside the tall label instead of under it.
fn render_ping_body(ui: &mut egui::Ui, body: &str, weak: bool) {
    // Not `horizontal_wrapped`: that floors the row at `interact_size.y`, which is 11px of dead air
    // per line on a body that only ever holds text and links.
    let row = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
    for line in body.lines() {
        if line.trim().is_empty() {
            // A tight row allocates nothing, so the author's paragraph break needs its own space.
            ui.add_space(ui.text_style_height(&egui::TextStyle::Body));
            continue;
        }
        let size = egui::vec2(ui.available_size_before_wrap().x, 0.0);
        ui.allocate_ui_with_layout(size, row, |ui| {
            render_linked_text(ui, line, weak);
        });
    }
}

/// Shortest gate+wormhole path, reported as the waypoints the player must set: the near side of every
/// hole they have to jump, then the destination. Gates between two waypoints the game routes itself.
fn wh_route_waypoints(
    geo: &crate::geo::Systems,
    wh_adj: &std::collections::HashMap<i64, Vec<i64>>,
    from: i64,
    dest: i64,
) -> Option<Vec<i64>> {
    use std::collections::{HashMap, HashSet, VecDeque};
    let mut prev: HashMap<i64, (i64, bool)> = HashMap::new();
    let mut visited: HashSet<i64> = HashSet::from([from]);
    let mut q: VecDeque<i64> = VecDeque::from([from]);
    let mut found = from == dest;
    while let Some(u) = q.pop_front() {
        if u == dest {
            found = true;
            break;
        }
        let gates = geo.neighbors(u).iter().map(|v| (*v, false));
        let holes = wh_adj.get(&u).into_iter().flatten().map(|v| (*v, true));
        for (v, via_wh) in gates.chain(holes) {
            if v != dest && crate::geo::is_no_transit(v) {
                continue;
            }
            if visited.insert(v) {
                prev.insert(v, (u, via_wh));
                q.push_back(v);
            }
        }
    }
    if !found {
        return None;
    }
    // Walk the path back out. `prev[cur] = (p, via_wh)` describes the step INTO `cur`, so the flag
    // belongs to `cur`, not to `p`.
    let mut path: Vec<(i64, bool)> = Vec::new();
    let mut cur = dest;
    while let Some(&(p, via_wh)) = prev.get(&cur) {
        path.push((cur, via_wh));
        cur = p;
    }
    path.push((cur, false)); // `from`, reached by nothing
    path.reverse();

    // The client cannot route through a hole, so every hole jump needs a waypoint on BOTH sides:
    // one to fly to, and one to pick the route up again from after jumping. Between two waypoints
    // the game routes by gates, which is exactly right for the gate legs.
    let mut waypoints: Vec<i64> = Vec::new();
    for w in path.windows(2) {
        let (a, (b, via_hole)) = (w[0].0, w[1]);
        if via_hole {
            waypoints.push(a);
            waypoints.push(b);
        }
    }
    waypoints.push(dest);
    // J-space cannot hold a waypoint (the client will not route to it), and there is no point
    // waypointing the system we are already sitting in.
    waypoints.retain(|&s| !crate::geo::is_wormhole_system(s));
    waypoints.dedup();
    if waypoints.first() == Some(&from) {
        waypoints.remove(0);
    }
    Some(waypoints)
}

fn valid_bare_jid(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return false;
    }
    let mut it = s.split('@');
    match (it.next(), it.next(), it.next()) {
        (Some(l), Some(d), None) => !l.is_empty() && d.contains('.'),
        _ => false,
    }
}

fn truncate_to(s: &str, max: usize) -> String {
    if max > 1 && s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_owned()
    }
}

fn short_chip(s: &str) -> String {
    truncate_to(s, 20)
}

/// Whether a conversation is a direct message rather than a room.
///
/// The frame's `convos` is built from everything with history, and a room has history, so this is
/// the only thing separating the two lists. `dm_keys` already excludes rooms, joined or left; a
/// contact is a person by definition.
fn is_direct_message(
    jid: &String,
    dm_keys: &std::collections::HashSet<&String>,
    contacts: &std::collections::HashSet<&String>,
) -> bool {
    dm_keys.contains(jid) || contacts.contains(jid)
}

/// Whether a conversation belongs in the Direct messages list.
///
/// Being a DM is the gate, and being sticky only overrides having been closed. Otherwise a room that
/// went unread would be listed twice, and the duplicate row is dead: two rows with the same jid ask
/// egui to interact with one id twice, and only one of them can win the hit test.
fn shows_in_dm_list(
    jid: &String,
    dm_keys: &std::collections::HashSet<&String>,
    contacts: &std::collections::HashSet<&String>,
    closed: &std::collections::HashSet<&String>,
    sticky: &std::collections::BTreeSet<String>,
) -> bool {
    is_direct_message(jid, dm_keys, contacts) && (sticky.contains(jid) || !closed.contains(jid))
}

/// A room's MOTD as one line, for a title bar that has one line to give it.
///
/// A MOTD is written as a notice board: several lines, blank lines between them, sometimes a rule
/// made of dashes. Collapsed, that is a paragraph; the separator keeps the parts from running into
/// each other so the first line still reads as the headline.
fn motd_one_line(motd: &str) -> String {
    let parts: Vec<&str> = motd
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        // A row of dashes or equals is a rule: it separates the lines above from the ones below,
        // and on one line it separates nothing while taking a third of the bar.
        .filter(|l| l.chars().any(|c| c.is_alphanumeric()))
        .collect();
    parts.join("  ·  ")
}

/// The first `max` non-empty lines, with a marker when there is more behind them.
///
/// A tooltip carrying a whole MOTD covers the window it is explaining. The cap is what makes it a
/// preview; the marker is what says a preview is what you are looking at.
fn motd_preview(motd: &str, max: usize) -> String {
    let lines: Vec<&str> = motd.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let mut out = lines.iter().take(max).copied().collect::<Vec<_>>().join("\n");
    if lines.len() > max {
        out.push_str("\n…");
    }
    out
}

fn fit_chars(width: f32) -> usize {
    (width / 7.5).floor().max(3.0) as usize
}

/// A filled presence/status dot. `size` is the font size of the phosphor CIRCLE glyph it replaces;
/// the painted diameter matches that glyph's ~0.72em footprint so inline spacing is unchanged.
fn status_dot(ui: &mut egui::Ui, color: egui::Color32, size: f32) {
    let d = size * 0.72;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), d / 2.0, color);
}

const PINNED_ROOM_TIP: &str = "Rescue Mode needs this channel. Turn Rescue Mode off to remove it. Closing its tab is safe: the room stays joined.";

/// The sidebar's "remove from the list" affordance, sized and framed to match the contacts star it
/// sits beside, since an icon control is judged against its neighbours rather than a pixel floor.
fn forget_button(ui: &mut egui::Ui, name: &str, blocked: Option<&str>) -> bool {
    let btn = egui::Button::new(
        egui::RichText::new(egui_phosphor::regular::X_CIRCLE).color(ui.visuals().weak_text_color()),
    )
    .frame(false)
    // The glyph alone allocates a 13px-wide target against the app's ~27px norm.
    .min_size(egui::vec2(24.0, 24.0));
    let resp = ui.add_enabled(blocked.is_none(), btn);
    match blocked {
        Some(why) => {
            resp.on_disabled_hover_text(why);
            false
        }
        None => {
            resp.on_hover_text(format!("Remove {name} from the list. Chat history is kept."))
                .clicked()
        }
    }
}

/// A selectable chip whose border is drawn in every state, so hovering doesn't pop a border in and
/// nudge the row. egui's selectable_label hides the frame when inactive+unselected (Button::selectable
/// sets frame_when_inactive(selected)); a plain Button keeps frame_when_inactive on by default, giving
/// a stable box across idle/hover/selected.
/// `selectable_label` and `selectable_value`, minus the border egui adds under the cursor.
///
/// An unframed button gets a stroke when it is hovered, and the stroke counts towards its size, so
/// every widget after it on the row steps sideways as the pointer passes. Dropping the stroke
/// keeps the geometry identical in both states and looks the same: the hover fill is what reads as
/// hover, not a one-pixel outline.
pub(crate) trait SteadySelect {
    fn menu_label<'a>(&mut self, selected: bool, text: impl egui::IntoAtoms<'a>)
        -> egui::Response;

    fn menu_value<'a, V: PartialEq>(
        &mut self,
        current: &mut V,
        value: V,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response;

    /// The same, at a fixed size, for a row of tabs that must not move as the pointer crosses it.
    /// Only the fleet tabs use it, and the hover test that pins it runs without the feature.
    #[cfg(any(feature = "fleet", test))]
    fn menu_label_sized<'a>(
        &mut self,
        size: impl Into<egui::Vec2>,
        selected: bool,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response;
}

impl SteadySelect for egui::Ui {
    fn menu_label<'a>(
        &mut self,
        selected: bool,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        self.add(
            egui::Button::new(text)
                .selected(selected)
                .frame_when_inactive(selected)
                .stroke(egui::Stroke::NONE),
        )
    }

    #[cfg(any(feature = "fleet", test))]
    fn menu_label_sized<'a>(
        &mut self,
        size: impl Into<egui::Vec2>,
        selected: bool,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        self.add_sized(
            size,
            egui::Button::new(text)
                .selected(selected)
                .frame_when_inactive(selected)
                .stroke(egui::Stroke::NONE),
        )
    }

    fn menu_value<'a, V: PartialEq>(
        &mut self,
        current: &mut V,
        value: V,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        let mut resp = self.menu_label(*current == value, text);
        if resp.clicked() && *current != value {
            *current = value;
            resp.mark_changed();
        }
        resp
    }
}

fn selectable_chip<'a>(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl egui::IntoAtoms<'a>,
) -> egui::Response {
    ui.add(egui::Button::new(text).selected(selected))
}

/// Immediate child viewports repaint in lockstep with the parent, so the count is capped rather
/// than letting a busy main window drag an unbounded number of chat windows along at 60 fps.
const MAX_POPOUTS: usize = 6;

const TAB_H: f32 = 24.0;
/// Separates the pin from the tab strip, which otherwise runs flush at zero item spacing.
const PIN_GAP: f32 = 6.0;
const TAB_PAD_X: f32 = 8.0;
const TAB_GAP: f32 = 6.0;
const TAB_LEAD_W: f32 = 16.0;
const TAB_CLOSE_W: f32 = 16.0;
/// Width of the trailing "open in new window" slot on a closable tab.
const TAB_POP_W: f32 = 16.0;
/// The eight offsets that make a 1px outline around map text.
const OUTLINE: [egui::Vec2; 8] = [
    egui::vec2(-1.0, -1.0),
    egui::vec2(0.0, -1.0),
    egui::vec2(1.0, -1.0),
    egui::vec2(-1.0, 0.0),
    egui::vec2(1.0, 0.0),
    egui::vec2(-1.0, 1.0),
    egui::vec2(0.0, 1.0),
    egui::vec2(1.0, 1.0),
];

/// How long the pointer must rest on a system before the map tooltip appears.
const MAP_TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

/// How long the battles view waits on a started worker before calling it stuck rather than slow.
const BATTLE_STALL: std::time::Duration = std::time::Duration::from_secs(20);

const UNREAD_RED: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x4C, 0x4C);
/// Backdrop for a chat line that named us. Alpha-blended so it reads on both themes.
const MENTION_BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(0x38, 0x14, 0x14, 0x50);

#[derive(Clone, Copy, Default)]
pub(crate) struct MsgActions {
    copy: bool,
    mention: bool,
    dm: bool,
}

const HISTORY_MIN_H: f32 = 60.0;
const COMPOSER_MIN_ROWS: f32 = 2.0;
const COMPOSER_MAX_ROWS: f32 = 10.0;
/// `TextEdit`'s own default margin, which the composer draws itself.
const COMPOSER_MARGIN: egui::Margin = egui::Margin::symmetric(4, 2);

/// A text-edit border box for the composer. `TextEdit` folds its stroke into the same margin, so
/// the inset is subtracted here too and the whole box still measures `COMPOSER_MARGIN` tall.
fn composer_frame(ui: &egui::Ui) -> egui::Frame {
    let v = ui.visuals().widgets.inactive;
    egui::Frame::new()
        .fill(ui.visuals().text_edit_bg_color())
        .stroke(v.bg_stroke)
        .corner_radius(v.corner_radius)
        .inner_margin(COMPOSER_MARGIN - v.bg_stroke.width.round() as i8)
}

/// Height the composer wants for `draft`, clamped to 2..=10 rows. Measured off the laid-out
/// galley because `TextEdit::desired_rows` counts logical rows, so one long wrapped line would
/// reserve a single row and clip the rest.
fn composer_height(ui: &egui::Ui, draft: &str, avail_w: f32) -> f32 {
    let row_h = ui.text_style_height(&egui::TextStyle::Body);
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let wrap_w = (avail_w - COMPOSER_MARGIN.sum().x).max(24.0);
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout(draft.to_owned(), font_id, ui.visuals().text_color(), wrap_w));
    galley.size().y.clamp(row_h * COMPOSER_MIN_ROWS, row_h * COMPOSER_MAX_ROWS)
        + COMPOSER_MARGIN.sum().y
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum MsgRowAction {
    None,
    Copy,
    Mention,
    GoToDm,
}

/// How far past the visible viewport a chat row is still built. Matches the intel feed's margin,
/// and covers a wheel step so scrolling does not land on unmeasured rows.
const MSG_OVERDRAW: f32 = 400.0;

/// Identifies a chat row for the height cache. Position is no use as a key: the 1000-message cap
/// drains from the front, which shifts every index. Width is in the key because two windows can
/// show one conversation at different widths, and a row's height is its wrap count.
fn msg_row_key(m: &crate::jabber::ChatMsg, grouped: bool, width: f32) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    m.from.hash(&mut h);
    m.body.hash(&mut h);
    m.time.hash(&mut h);
    m.outgoing.hash(&mut h);
    grouped.hash(&mut h);
    (width as i32).hash(&mut h);
    h.finish()
}

/// A skipped row leaves no widget and no AccessKit node behind, so the harness cannot tell a
/// virtualized history from a fully built one. Counts what the pass actually built.
#[cfg(test)]
fn record_msg_row_built(ctx: &egui::Context) {
    let id = egui::Id::new("jabber_msg_rows_built");
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let seen: &mut (u64, usize) = d.get_temp_mut_or_default(id);
        if seen.0 != pass {
            *seen = (pass, 0);
        }
        seen.1 += 1;
    });
}

#[cfg(test)]
pub(crate) fn built_msg_rows(ctx: &egui::Context) -> usize {
    let id = egui::Id::new("jabber_msg_rows_built");
    ctx.data(|d| d.get_temp::<(u64, usize)>(id).map_or(0, |s| s.1))
}

/// A chat row that brightens on hover and reveals its actions at the top right. `id` must be
/// stable and unique per message.
pub(crate) fn message_row(
    ui: &mut egui::Ui,
    id: egui::Id,
    mention_bg: bool,
    show: MsgActions,
    content: impl FnOnce(&mut egui::Ui),
) -> MsgRowAction {
    let mut frame = egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 1));
    if mention_bg {
        frame = frame.fill(MENTION_BG);
    }
    let rect = frame
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        })
        .response
        .rect;
    let resp = ui.interact(rect, id, egui::Sense::hover());
    // Not `hovered()`: the action buttons live inside this rect, so reaching one makes the row stop
    // being the hovered widget, which drops the buttons, which re-hovers the row, a flicker every
    // other frame. `contains_pointer` is geometric and stays true underneath them.
    let hot = resp.contains_pointer();
    if !hot {
        return MsgRowAction::None;
    }
    let txt = ui.visuals().text_color();
    let tint = egui::Color32::from_rgba_unmultiplied(txt.r(), txt.g(), txt.b(), 26);
    ui.painter().rect_filled(rect, 3.0, tint);

    let mut acts: Vec<(&str, &str, MsgRowAction)> = Vec::new();
    if show.copy {
        acts.push((egui_phosphor::regular::COPY, "Copy message", MsgRowAction::Copy));
    }
    if show.mention {
        acts.push((egui_phosphor::regular::AT, "Mention", MsgRowAction::Mention));
    }
    if show.dm {
        acts.push((egui_phosphor::regular::CHAT_TEARDROP_TEXT, "Go to DM", MsgRowAction::GoToDm));
    }
    if acts.is_empty() {
        return MsgRowAction::None;
    }
    // Painted, not `ui.put`: a widget here allocates space, which grows the row the instant it is
    // hovered, moves the pointer off it, and jitters. `interact` registers a hit area only. Same
    // reason the tab close X is painted (see `jabber_tab_box`).
    const SLOT: f32 = 21.0;
    const PAD: f32 = 3.0;
    let n = acts.len() as f32;
    let h = (SLOT).min(rect.height());
    let strip = egui::Rect::from_min_size(
        egui::pos2(rect.right() - SLOT * n - PAD * 2.0, rect.top()),
        egui::vec2(SLOT * n + PAD * 2.0, h),
    );
    let painter = ui.painter().clone();
    // Opaque, or the icons sit unreadably on top of a long message body.
    painter.rect_filled(strip, 4.0, ui.visuals().panel_fill);
    let mut out = MsgRowAction::None;
    for (i, (icon, tip, act)) in acts.iter().enumerate() {
        let slot = egui::Rect::from_min_size(
            egui::pos2(strip.left() + PAD + SLOT * i as f32, strip.top()),
            egui::vec2(SLOT, h),
        );
        let r = ui.interact(slot, id.with(("act", i)), egui::Sense::click()).on_hover_text(*tip);
        let col = if r.hovered() {
            painter.rect_filled(slot.shrink(1.0), 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        painter.text(
            slot.center(),
            egui::Align2::CENTER_CENTER,
            *icon,
            egui::FontId::proportional(14.0),
            col,
        );
        if r.clicked() {
            out = *act;
        }
    }
    out
}

fn muc_domain_of(setting: &str, jid: &str) -> String {
    if !setting.trim().is_empty() {
        return setting.trim().to_owned();
    }
    match jid.split('@').nth(1).unwrap_or("").trim() {
        "" => String::new(),
        d => format!("conference.{d}"),
    }
}

#[derive(Clone, Copy)]
enum TabLead {
    Dot(egui::Color32),
    Icon(&'static str),
}

/// Exact rendered width of a tab, so the overflow split matches what `jabber_tab_box` draws and the
/// dropdown is never pushed off the edge. `closable` reserves the trailing close-X slot.
fn jabber_tab_width(ui: &egui::Ui, closable: bool, unread: bool, label: &str) -> f32 {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let label_w =
        ui.painter().layout_no_wrap(label.to_owned(), body, egui::Color32::WHITE).size().x;
    let lead_w = TAB_LEAD_W + TAB_GAP;
    let trail_w = if closable {
        TAB_GAP + TAB_POP_W + TAB_GAP + TAB_CLOSE_W
    } else if unread {
        TAB_GAP + 10.0
    } else {
        0.0
    };
    2.0 * TAB_PAD_X + lead_w + label_w + trail_w
}

/// Shorten `label` with a trailing ellipsis so the whole tab fits `max_tab_w`. Returns the original
/// when it already fits, or "…" when there is not even room for one character.
fn ellipsize_tab_label(
    ui: &egui::Ui,
    closable: bool,
    unread: bool,
    label: &str,
    max_tab_w: f32,
) -> String {
    if jabber_tab_width(ui, closable, unread, label) <= max_tab_w {
        return label.to_owned();
    }
    let fixed = jabber_tab_width(ui, closable, unread, "");
    let label_budget = (max_tab_w - fixed).max(0.0);
    let body = egui::TextStyle::Body.resolve(ui.style());
    let width_of = |s: &str| {
        ui.painter().layout_no_wrap(s.to_owned(), body.clone(), egui::Color32::WHITE).size().x
    };
    let chars: Vec<char> = label.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        let cand: String = chars[..n].iter().collect::<String>() + "…";
        if width_of(&cand) <= label_budget {
            return cand;
        }
        n -= 1;
    }
    "…".to_owned()
}

/// The slot a dragged tab came from: veiled and outlined, so the origin stays obvious while the
/// ghost is off under the pointer.
fn jabber_tab_lifted(ui: &egui::Ui, rect: egui::Rect) {
    let v = ui.visuals();
    let p = ui.painter();
    p.rect_filled(rect, 0.0, v.panel_fill.gamma_multiply(0.7));
    p.rect_stroke(
        rect.shrink(1.0),
        0.0,
        egui::Stroke::new(1.0, v.selection.stroke.color),
        egui::StrokeKind::Inside,
    );
}

/// The chip that follows the pointer while a tab is being dragged, so the gesture is visible where
/// the user is looking. Painted straight into a foreground layer rather than an `Area`, which would
/// put a hit target on top of whatever it floats over.
fn jabber_drag_ghost(ui: &egui::Ui, win: ChatWinKey, label: &str, at: egui::Pos2) {
    const OFFSET: f32 = 14.0;
    const PAD_Y: f32 = 4.0;
    let v = ui.visuals();
    let text_col = v.strong_text_color();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::TextStyle::Body.resolve(ui.style()),
        text_col,
    );
    let size = galley.size() + egui::vec2(2.0 * TAB_PAD_X, 2.0 * PAD_Y);
    let bounds = ui.ctx().content_rect();
    let min = egui::pos2(
        (at.x + OFFSET).min(bounds.right() - size.x - 2.0),
        (at.y + OFFSET).min(bounds.bottom() - size.y - 2.0),
    );
    let rect = egui::Rect::from_min_size(min, size);
    let p = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new(("jabber_tab_ghost", win)),
    ));
    p.rect_filled(rect, 4.0, v.window_fill);
    p.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, v.selection.stroke.color),
        egui::StrokeKind::Inside,
    );
    p.galley(egui::pos2(rect.left() + TAB_PAD_X, rect.top() + PAD_Y), galley, text_col);
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// What a tab reported this frame. `resp` carries the drag state and hosts the context menu.
struct TabHit {
    select: bool,
    close: bool,
    popout: bool,
    resp: egui::Response,
}

/// One compact, flush Jabber tab. Fixed height, optional leading dot/icon, the name, and, on hover
/// or when active, trailing "open in new window" and close icons (the close slot otherwise carries
/// the unread marker).
#[allow(clippy::too_many_arguments)]
fn jabber_tab_box(
    ui: &mut egui::Ui,
    id: egui::Id,
    selected: bool,
    unread: bool,
    mention: bool,
    lead: TabLead,
    closable: bool,
    // The pop-out slot is reserved whether or not the icon is offered, so reaching the pop-out
    // cap does not reflow the whole bar.
    can_popout: bool,
    label: &str,
) -> TabHit {
    let w = jabber_tab_width(ui, closable, unread, label);
    // Explicit id, not the auto one: the overflow split reshuffles tab order, and an auto id would
    // re-bind a tab's interactions (and its close X) to whatever now sits in that slot.
    let (_, rect) = ui.allocate_space(egui::vec2(w, TAB_H));
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    // Not `hovered()`: the close X is a second widget inside this rect, so once the pointer reaches
    // it the tab stops being the hovered widget. On an unselected tab that would drop the X, which
    // un-hovers it, which draws it again, a flicker every other frame. `contains_pointer` is
    // geometric and stays true underneath the X.
    let hovered = resp.contains_pointer();
    let body = egui::TextStyle::Body.resolve(ui.style());
    let (fill, text_color, accent, sep_col, weak_col, strong_col) = {
        let v = ui.visuals();
        (
            if selected {
                v.selection.bg_fill
            } else if hovered {
                v.widgets.hovered.weak_bg_fill
            } else {
                egui::Color32::TRANSPARENT
            },
            if selected || unread { v.strong_text_color() } else { v.text_color() },
            v.selection.stroke.color,
            v.widgets.noninteractive.bg_stroke.color,
            v.weak_text_color(),
            v.strong_text_color(),
        )
    };

    let painter = ui.painter().clone();
    if fill != egui::Color32::TRANSPARENT {
        painter.rect_filled(rect, 0.0, fill);
    }
    if selected {
        painter.hline(rect.x_range(), rect.top() + 1.0, egui::Stroke::new(2.0, accent));
    }
    painter.vline(rect.right(), rect.y_range(), egui::Stroke::new(1.0, sep_col));

    let cy = rect.center().y;
    let mut x = rect.left() + TAB_PAD_X;
    match lead {
        TabLead::Dot(c) => {
            painter.circle_filled(egui::pos2(x + TAB_LEAD_W / 2.0, cy), 4.0, c);
            x += TAB_LEAD_W + TAB_GAP;
        }
        TabLead::Icon(ic) => {
            painter.text(
                egui::pos2(x, cy),
                egui::Align2::LEFT_CENTER,
                ic,
                egui::FontId::proportional(15.0),
                text_color,
            );
            x += TAB_LEAD_W + TAB_GAP;
        }
    }
    let galley = painter.layout_no_wrap(label.to_owned(), body, text_color);
    painter.galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, text_color);

    // An unread mention gets an "@" where an ordinary unread tab gets a dot.
    let mark = |at: egui::Pos2| {
        if mention {
            painter.text(
                at,
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::AT,
                egui::FontId::proportional(14.0),
                UNREAD_RED,
            );
        } else {
            painter.circle_filled(at, 4.0, UNREAD_RED);
        }
    };

    let mut select = resp.clicked();
    let mut close = false;
    let mut popout = false;
    let slot_cx = rect.right() - TAB_PAD_X - TAB_CLOSE_W / 2.0;
    if closable {
        let close_rect =
            egui::Rect::from_center_size(egui::pos2(slot_cx, cy), egui::vec2(TAB_CLOSE_W, TAB_CLOSE_W));
        let pop_cx = rect.right() - TAB_PAD_X - TAB_CLOSE_W - TAB_GAP - TAB_POP_W / 2.0;
        let pop_rect =
            egui::Rect::from_center_size(egui::pos2(pop_cx, cy), egui::vec2(TAB_POP_W, TAB_POP_W));
        if hovered || selected {
            if can_popout {
                let presp = ui.interact(pop_rect, resp.id.with("popout"), egui::Sense::click());
                let pcol = if presp.hovered() { strong_col } else { weak_col };
                painter.text(
                    pop_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    egui_phosphor::regular::ARROW_SQUARE_OUT,
                    egui::FontId::proportional(13.0),
                    pcol,
                );
                if presp.on_hover_text("Open in new window").clicked() {
                    popout = true;
                    select = false;
                }
            }
            let cresp = ui.interact(close_rect, resp.id.with("close"), egui::Sense::click());
            let col = if cresp.hovered() { strong_col } else { weak_col };
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::X,
                egui::FontId::proportional(13.0),
                col,
            );
            if cresp.on_hover_text("Close").clicked() {
                close = true;
                select = false;
            }
        } else if unread {
            mark(close_rect.center());
        }
    } else if unread {
        mark(egui::pos2(slot_cx, cy));
    }
    TabHit { select, close, popout, resp }
}

#[derive(Default)]
struct DscanShare {
    uploading: bool,
    link: Option<String>,
    error: Option<String>,
}

pub(crate) struct DscanView {
    url: String,
    fetch: std::sync::Arc<std::sync::Mutex<DscanFetch>>,
}

pub(crate) enum DscanFetch {
    Loading,
    Ready(Vec<(i64, String, u32)>),
    /// The link held a local scan, not a d-scan: these pilots go to the Lookup view.
    Local(Vec<String>),
    Failed,
}

impl DscanFetch {
    fn snapshot(&self) -> DscanFetch {
        match self {
            DscanFetch::Loading => DscanFetch::Loading,
            DscanFetch::Failed => DscanFetch::Failed,
            DscanFetch::Ready(v) => DscanFetch::Ready(v.clone()),
            DscanFetch::Local(v) => DscanFetch::Local(v.clone()),
        }
    }
}

pub(crate) fn fetch_dscan_ships(
    url: &str,
    ship_index: Option<&std::collections::HashMap<String, (i64, String)>>,
) -> Option<Vec<(i64, String, u32)>> {
    let idx = ship_index?;
    let client = crate::http::client(20)
        .ok()?;
    let mut candidates = vec![url.to_string()];
    if !url.contains("/v/") {
        if let Some(pos) = url.rfind('/') {
            candidates.push(format!("{}/v/{}", &url[..pos], &url[pos + 1..]));
        }
    }
    for u in candidates {
        let Ok(resp) = client.get(&u).send() else { continue };
        let Ok(body) = resp.error_for_status().and_then(|r| r.text()) else { continue };
        let mut counts: std::collections::HashMap<i64, (String, u32)> = std::collections::HashMap::new();
        for (name, n) in crate::dscan::parse_dscan_ships_html(&body) {
            if let Some((id, canon)) = idx.get(&name.to_lowercase()) {
                let e = counts.entry(*id).or_insert_with(|| (canon.clone(), 0));
                e.1 += n;
            }
        }
        if !counts.is_empty() {
            let mut out: Vec<(i64, String, u32)> =
                counts.into_iter().map(|(id, (name, n))| (id, name, n)).collect();
            out.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
            return Some(out);
        }
    }
    None
}

pub(crate) fn open_dscan_view(
    url: String,
    ship_index: Option<std::sync::Arc<std::collections::HashMap<String, (i64, String)>>>,
    ctx: &egui::Context,
) -> Option<DscanView> {
    if !url.contains("dscan.info") {
        let _ = open::that(&url);
        return None;
    }
    let fetch = std::sync::Arc::new(std::sync::Mutex::new(DscanFetch::Loading));
    let view = DscanView { url: url.clone(), fetch: fetch.clone() };
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = fetch_dscan_ships(&url, ship_index.as_deref());
        *fetch.lock().unwrap() = match result {
            Some(v) if !v.is_empty() => DscanFetch::Ready(v),
            _ => {
                let pilots = crate::localscan::fetch_page_pilots(&url);
                if pilots.is_empty() { DscanFetch::Failed } else { DscanFetch::Local(pilots) }
            }
        };
        ctx.request_repaint();
    });
    Some(view)
}

pub(crate) fn dscan_view_dialog_ui(
    ctx: &egui::Context,
    dscan_view: &mut Option<DscanView>,
    taskbar_off: bool,
    on_open_ship: &mut Option<i64>,
) {
    use egui_phosphor::regular as icon;
    let Some(view) = dscan_view.as_ref() else { return };
    let url = view.url.clone();
    let state = view.fetch.lock().unwrap().snapshot();
    let mut open_ship: Option<i64> = None;
    let keep = dialog_viewport_ext(
        ctx,
        "dscan_view",
        "EVE Spai - D-scan",
        [340.0, 520.0],
        taskbar_off,
        |ui| {
            ui.horizontal(|ui| {
                if ui.button(format!("{}  Open on dscan.info", icon::ARROW_SQUARE_OUT)).clicked() {
                    let _ = open::that(&url);
                }
            });
            ui.separator();
            match &state {
                DscanFetch::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Fetching scan…");
                    });
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                DscanFetch::Failed => {
                    ui.label(egui::RichText::new("Couldn't read this scan. Open it on the site.").weak());
                }
                DscanFetch::Local(names) => {
                    ui.label(format!("A local scan of {} pilots, opening in Lookup.", names.len()));
                }
                DscanFetch::Ready(ships) => {
                    let total: u32 = ships.iter().map(|(_, _, n)| n).sum();
                    ui.label(egui::RichText::new(format!("{} ships · {} types", total, ships.len())).weak());
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for (id, name, n) in ships {
                            ui.horizontal(|ui| {
                                hull_badge(ui, *id, 24.0);
                                if ui
                                    .add(egui::Label::new(
                                        egui::RichText::new(name).color(ui.visuals().hyperlink_color),
                                    )
                                    .sense(egui::Sense::click()))
                                    .on_hover_text("Ship info")
                                    .clicked()
                                {
                                    open_ship = Some(*id);
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(format!("×{n}")).strong());
                                });
                            });
                        }
                    });
                }
            }
        },
    );
    if let Some(id) = open_ship {
        *on_open_ship = Some(id);
    }
    if !keep {
        *dscan_view = None;
    }
}

/// A dialog as its own normal window. Dialogs carry no always-on-top pin: the strip it needs pushes
/// every dialog's content down, and a dialog is opened from the app, which focuses it.
#[allow(deprecated)]
pub(crate) fn dialog_viewport_ext(
    parent: &egui::Context,
    id: &str,
    title: &str,
    size: [f32; 2],
    taskbar_off: bool,
    content: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut keep = true;
    let mut content = Some(content);
    let mut builder = egui::ViewportBuilder::default()
        .with_icon(app_icon())
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size([size[0].min(380.0), size[1].min(320.0)]);
    if taskbar_off {
        builder = builder.with_taskbar(false);
        #[cfg(target_os = "linux")]
        {
            builder = builder.with_window_type(egui::X11WindowType::Utility);
        }
    }
    parent.show_viewport_immediate(
        egui::ViewportId::from_hash_of(id),
        builder,
        |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(c) = content.take() {
                    c(ui);
                }
            });
            if ctx.input(|i| i.viewport().close_requested()) {
                keep = false;
            }
        },
    );
    keep
}

fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// A saved battle report's file name: its first system and start date.
fn battle_file_name(battle: &br_core::battle::Battle) -> String {
    let system = battle
        .systems
        .first()
        .map(|(_, name, _)| name.as_str())
        .unwrap_or("battle");
    let safe: String = system
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let date = chrono::DateTime::from_timestamp(battle.start, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("{safe}-{date}.evespai-br.json")
}

/// "45s", "12m", "3h", "9d": one unit, for ages that sit in a narrow column.
fn human_ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86_400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86_400)
    }
}

fn system_chips(
    ui: &mut egui::Ui,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    system_id: i64,
) {
    system_chips_ex(ui, systems, status, system_id, true, true);
}

fn system_chips_ex(
    ui: &mut egui::Ui,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    system_id: i64,
    show_location: bool,
    show_sov: bool,
) {
    use crate::theme::standing;
    if let Some(info) = systems.as_ref().and_then(|s| s.info_of(system_id)) {
        let loc = match (info.constellation.as_str(), info.region.as_str()) {
            ("", "") => String::new(),
            ("", r) => format!("< {r}"),
            (c, "") => format!("< {c}"),
            (c, r) => format!("< {c} < {r}"),
        };
        if show_location && !loc.is_empty() {
            ui.label(egui::RichText::new(loc).weak());
        }
        if !info.faction.is_empty() && info.security < 0.5 {
            ui.label(egui::RichText::new(&info.faction).color(standing::NEUTRAL));
        }
    }
    if crate::jove::has(system_id) {
        ui.label(
            egui::RichText::new(format!("{}  Jove Observatory", egui_phosphor::regular::CELL_TOWER))
                .color(JOVE_COLOR),
        );
    }
    if let Some(f) = status.get(&system_id) {
        if f.incursion {
            ui.label(egui::RichText::new("INCURSION").color(standing::ALLIANCE));
        }
        if let Some(fw) = &f.fw {
            ui.label(egui::RichText::new(format!("FW {fw}")).color(standing::WARNING));
        }
        if show_sov {
            if let Some(sov) = &f.sov {
                ui.label(egui::RichText::new(format!("Sov: {sov}")).color(standing::CORP));
            }
        }
    }
}

fn from_you_chip(ui: &mut egui::Ui, from_you: Option<u32>) {
    if let Some(j) = from_you {
        let txt = if j == 0 { "here".to_owned() } else { format!("{j}j") };
        ui.label(egui::RichText::new(format!("{txt:>4}")).monospace().weak());
    }
}

/// Free space, in the same shape as `procstat::rss_human` so the two read alike in the status bar.
pub(crate) fn fmt_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// "3y 2m", "5m 12d", "9d": the two largest units, which is all a glance needs.
fn span_text(secs: i64) -> String {
    let days = secs.max(0) / 86_400;
    let (y, m, d) = (days / 365, (days % 365) / 30, (days % 365) % 30);
    match (y, m) {
        (0, 0) => format!("{d}d"),
        (0, m) => format!("{m}m {d}d"),
        (y, m) => format!("{y}y {m}m"),
    }
}

fn day_text(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

fn fmt_isk(isk: f64) -> String {
    crate::intel::format_isk(isk.max(0.0) as u64)
}

fn side_color(i: usize) -> egui::Color32 {
    match i {
        0 => egui::Color32::from_rgb(0x4f, 0xc3, 0xf7),
        1 => egui::Color32::from_rgb(0xe0, 0x4c, 0x4c),
        2 => egui::Color32::from_rgb(0x9c, 0xcc, 0x65),
        _ => egui::Color32::from_rgb(0xb0, 0xb0, 0xb0),
    }
}

fn eve_img_size(px: f32) -> u32 {
    let want = px.ceil().max(1.0) as u32;
    [32u32, 64, 128, 256, 512].into_iter().find(|&s| s >= want).unwrap_or(512)
}

fn eve_portrait_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/characters/{id}/portrait?size={}", eve_img_size(px))
}

fn eve_corp_logo_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/corporations/{id}/logo?size={}", eve_img_size(px))
}

fn eve_alliance_logo_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/alliances/{id}/logo?size={}", eve_img_size(px))
}

/// How a route gets from one system to the next. Each kind draws in its own colour, so a glance at
/// the line says whether you are taking a gate, a bridge, or a hole.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Leg {
    Gate,
    Bridge,
    Hole,
}

impl Leg {
    fn color(self) -> egui::Color32 {
        match self {
            Leg::Gate => egui::Color32::from_rgb(0x4F, 0xC3, 0xF7),
            Leg::Bridge => egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
            Leg::Hole => egui::Color32::from_rgb(0xB0, 0x7C, 0xE8),
        }
    }
}

/// Where the pieces of a system's label row go, decided in one pass so the parts that draw later
/// agree with the parts that drew earlier.
struct LabelRow {
    lead_x: f32,
    name_x: f32,
    icons_x: f32,
    /// Vertical centre; row parts draw `*_CENTER` on this y so differing-height name and icons align.
    mid_y: f32,
    name_shown: bool,
    rect: egui::Rect,
}

/// What the system dot borrows from its sov holder.
pub(crate) struct SovArt {
    icon: String,
    /// Only player sov recolours the dot; NPC systems keep their security colour.
    dot: Option<egui::Color32>,
}

/// Mean of a logo's opaque pixels, pushed to stay legible and distinguishable against the map's dark
/// background. Averaging a multi-hued logo pulls it toward grey, so the mean's own hue is kept but
/// its saturation is pushed back up; without that, every alliance ends up the same murky slate.
fn mean_logo_color(img: &egui::ColorImage) -> Option<egui::Color32> {
    const SATURATION_BOOST: f32 = 2.4;
    const MIN_VALUE: f32 = 130.0;

    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for px in img.pixels.iter() {
        if px.a() < 128 {
            continue;
        }
        r += px.r() as u64;
        g += px.g() as u64;
        b += px.b() as u64;
        n += 1;
    }
    if n == 0 {
        return None;
    }
    let (r, g, b) = ((r / n) as f32, (g / n) as f32, (b / n) as f32);

    // Saturation is the gap between each channel and the darkest one; widening that gap saturates
    // the colour while leaving its hue and brightest channel alone. A true grey has no gap and so
    // stays grey, which is right: there is no hue in it to recover.
    let lo = r.min(g).min(b);
    let sat = |v: f32| (lo + (v - lo) * SATURATION_BOOST).clamp(0.0, 255.0);
    let (r, g, b) = (sat(r), sat(g), sat(b));

    // Then lift a dark logo into view, keeping the ratios (and so the hue) intact.
    let lift = (MIN_VALUE / r.max(g).max(b).max(1.0)).max(1.0);
    let c = |v: f32| (v * lift).min(255.0) as u8;
    Some(egui::Color32::from_rgb(c(r), c(g), c(b)))
}

pub(crate) fn eve_type_icon_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/types/{id}/icon?size={}", eve_img_size(px))
}

fn eve_type_render_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/types/{id}/render?size={}", eve_img_size(px))
}

pub(crate) fn party_badge(ui: &mut egui::Ui, p: &br_core::battle::Party, size: f32, clickable: bool) {
    use br_core::battle::PartyKind;
    let urls = match p.kind {
        PartyKind::Alliance => Some((
            eve_alliance_logo_url(p.id, size),
            format!("https://zkillboard.com/alliance/{}/", p.id),
        )),
        PartyKind::Corporation => Some((
            eve_corp_logo_url(p.id, size),
            format!("https://zkillboard.com/corporation/{}/", p.id),
        )),
        PartyKind::Character => Some((
            eve_portrait_url(p.id, size),
            format!("https://zkillboard.com/character/{}/", p.id),
        )),
        _ => None,
    };
    let Some((img_url, zkill)) = urls else {
        ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(&p.name);
        return;
    };
    let img = egui::Image::new(img_url).fit_to_exact_size(egui::Vec2::splat(size));
    if clickable {
        if ui.add(egui::Button::image(img)).on_hover_text(&p.name).clicked() {
            let _ = open::that(zkill);
        }
    } else {
        ui.add(img).on_hover_text(&p.name);
    }
}

pub(crate) fn hull_badge(ui: &mut egui::Ui, type_id: i64, size: f32) {
    if type_id == 0 {
        return;
    }
    let url = if crate::intel::structure_name_by_type(type_id).is_some() {
        eve_type_render_url(type_id, size)
    } else {
        eve_type_icon_url(type_id, size)
    };
    ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(size)));
}

fn side_title(side: &br_core::battle::Side) -> String {
    side.coalition
        .clone()
        .or_else(|| side.parties.first().map(|p| p.name.clone()))
        .unwrap_or_else(|| "?".to_owned())
}

const TOOLBAR_SEP_W: f32 = 10.0;

#[derive(Clone, Default)]
struct ToolbarSeps {
    /// Dividers placed but not yet painted, as (rect, row top, reserved shape).
    pending: Vec<(egui::Rect, f32, egui::layers::ShapeIdx)>,
    /// Where the content before each divider, and before the flush, ended.
    ends: Vec<egui::Pos2>,
}

fn toolbar_seps_id(ui: &egui::Ui) -> egui::Id {
    ui.id().with("toolbar_seps")
}

/// A wrapping toolbar row, and the only place the dividers [`toolbar_sep`] deferred get painted.
fn toolbar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        let r = add(ui);
        let id = toolbar_seps_id(ui);
        let mut st: ToolbarSeps = ui.data_mut(|d| d.remove_temp(id).unwrap_or_default());
        let cursor = ui.cursor();
        st.ends.push(egui::pos2(cursor.left(), cursor.top()));
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let gap = ui.spacing().item_spacing.x + 0.5;
        let same_row = ui.spacing().interact_size.y * 0.5;
        for (rect, top, idx) in &st.pending {
            let followed = st
                .ends
                .iter()
                .any(|e| (e.y - top).abs() < same_row && e.x > rect.right() + gap);
            if followed {
                ui.painter()
                    .set(*idx, egui::Shape::vline(rect.center().x, rect.y_range(), stroke));
                #[cfg(test)]
                record_toolbar_sep(ui, *rect);
            }
        }
        r
    })
    .inner
}

/// Dividers are pure decoration and emit no AccessKit node, so the uitest harness cannot see one
/// at all. Records what was painted this pass so it can check them anyway.
#[cfg(test)]
fn record_toolbar_sep(ui: &egui::Ui, rect: egui::Rect) {
    let id = egui::Id::new("toolbar_seps_painted");
    let pass = ui.ctx().cumulative_pass_nr();
    ui.data_mut(|d| {
        let seen: &mut (u64, Vec<egui::Rect>) = d.get_temp_mut_or_default(id);
        if seen.0 != pass {
            *seen = (pass, Vec::new());
        }
        seen.1.push(rect);
    });
}

#[cfg(test)]
pub(crate) fn painted_toolbar_seps(ctx: &egui::Context) -> Vec<egui::Rect> {
    let id = egui::Id::new("toolbar_seps_painted");
    ctx.data(|d| d.get_temp::<(u64, Vec<egui::Rect>)>(id).map(|s| s.1).unwrap_or_default())
}

/// Group boundary inside a [`toolbar`]. A divider at the start or the end of a row separates
/// nothing, and the wrap point moves with the window width, so one that would land at a row start
/// is dropped outright and the rest are painted only once the row is known to continue past them.
fn toolbar_sep(ui: &mut egui::Ui) {
    let id = toolbar_seps_id(ui);
    let mut st: ToolbarSeps = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
    let cursor = ui.cursor();
    st.ends.push(egui::pos2(cursor.left(), cursor.top()));
    let at_row_start = cursor.left() <= ui.max_rect().left() + 0.5;
    let would_wrap = ui.available_rect_before_wrap().width() < TOOLBAR_SEP_W;
    if !at_row_start && !would_wrap {
        let h = ui.spacing().interact_size.y;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(TOOLBAR_SEP_W, h), egui::Sense::hover());
        let idx = ui.painter().add(egui::Shape::Noop);
        st.pending.push((rect, cursor.top(), idx));
    }
    ui.data_mut(|d| d.insert_temp(id, st));
}

/// A `ComboBox` in a [`toolbar`].
///
/// `ComboBox::show_ui` opens with a plain `ui.horizontal`, whose desired size is whatever is left
/// of the row, so the wrapping layout is never told a width that could not fit and never breaks
/// before one. The box then lays its selected text out unwrapped and paints past the row's right
/// edge. Reserving the same width egui is about to paint puts the wrap decision back on the real
/// size, at any window width.
fn toolbar_combo<R>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    selected: String,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
    let (icon_spacing, icon_width, pad_x, combo_width, row_h) = {
        let sp = ui.spacing();
        (sp.icon_spacing, sp.icon_width, sp.button_padding.x, sp.combo_width, sp.interact_size.y)
    };
    let galley = egui::WidgetText::from(selected.clone()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let w = (galley.size().x + icon_spacing + icon_width + 2.0 * pad_x).max(combo_width);
    ui.allocate_ui_with_layout(
        egui::vec2(w, row_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            egui::ComboBox::from_id_salt(id_salt)
                .selected_text(selected)
                .show_ui(ui, contents)
                .response
        },
    )
    .inner
}

fn battle_preview_summary(ui: &mut egui::Ui, label: &str, b: &br_core::battle::Battle) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.label(format!("{} kills", b.kills));
        ui.label(egui::RichText::new(format!("{} ISK", fmt_isk(b.isk))).weak());
        for (i, side) in b.sides.iter().take(2).enumerate() {
            if i > 0 {
                ui.label(egui::RichText::new("vs").weak());
            }
            let name = side.parties.first().map(|p| p.name.as_str()).unwrap_or("?");
            ui.label(egui::RichText::new(name).color(side_color(i)).strong());
            ui.label(egui::RichText::new(format!("{}k/{}l", side.kills, side.losses)).weak());
        }
        if b.sides.is_empty() {
            ui.label(egui::RichText::new("no clear sides").weak());
        }
    });
}

pub(crate) fn battle_row(
    ui: &mut egui::Ui,
    b: &br_core::battle::Battle,
    now: i64,
    from_you: Option<u32>,
) -> bool {
    let span_min = ((b.end - b.start) / 60).max(0);
    let resp = egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{:>7}", fmt_age(now - b.end))).monospace().weak());
            from_you_chip(ui, from_you);
            for (_id, name, sec) in &b.systems {
                ui.label(security_badge(*sec));
                ui.label(egui::RichText::new(name).strong());
            }
            ui.separator();
            ui.label(format!("{} kills", b.kills));
            if b.ambiguous {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::WARNING)
                        .color(crate::theme::standing::WARNING)
                        .strong(),
                )
                .on_hover_text("This battle may be two fights. Open to review.");
            }
            ui.label(egui::RichText::new(format!("{} ISK", fmt_isk(b.isk))).weak());
            if span_min > 0 {
                ui.label(egui::RichText::new(format!("over {span_min}m")).weak());
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (i, side) in b.sides.iter().take(2).enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new("vs").strong());
                }
                let col = side_color(i);
                if let Some(lead) = side.parties.first() {
                    party_badge(ui, lead, 18.0, false);
                }
                ui.label(egui::RichText::new(side_title(side)).color(col).strong());
                ui.label(egui::RichText::new(format!("{}k/{}l", side.kills, side.losses)).weak());
            }
        });
    })
    .response;
    let resp = resp.interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ShipHighlight {
    None,
    Hovered,
    Assist,
}

pub(crate) fn ship_row(
    ui: &mut egui::Ui,
    width: f32,
    party: &br_core::battle::Party,
    ship: i64,
    pilot: &str,
    name_of: &dyn Fn(i64) -> String,
    lost: Option<&br_core::battle::Lost>,
    red: egui::Color32,
    highlight: ShipHighlight,
    border: bool,
) -> egui::Response {
    use egui_phosphor::regular as icon;
    let fill = match highlight {
        ShipHighlight::Assist => egui::Color32::from_rgb(0xE0, 0xB0, 0x4C).gamma_multiply(0.26),
        ShipHighlight::Hovered if lost.is_some() => red.gamma_multiply(0.28),
        ShipHighlight::Hovered => egui::Color32::from_rgba_unmultiplied(255, 255, 255, 24),
        ShipHighlight::None if lost.is_some() => red.gamma_multiply(0.16),
        ShipHighlight::None => egui::Color32::TRANSPARENT,
    };
    let stroke = egui::Stroke::new(1.5, if border { red } else { egui::Color32::TRANSPARENT });
    let resp = egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .corner_radius(4.0)
        .stroke(stroke)
        .show(ui, |ui| {
            ui.set_width(width);
            ui.horizontal_wrapped(|ui| {
                hull_badge(ui, ship, 28.0);
                ui.label(egui::RichText::new(name_of(ship)).strong());
                if let Some(l) = lost {
                    ui.label(egui::RichText::new(fmt_isk(l.value)).color(red).strong());
                    if l.pod_value > 0.0 {
                        ui.label(egui::RichText::new("+").weak());
                        // The actual capsule variant (regular / Genolution), 670 as a fallback.
                        let pod = if l.pod_ship != 0 { l.pod_ship } else { 670 };
                        hull_badge(ui, pod, 16.0);
                        if l.pod_value >= 1_000_000.0 {
                            ui.label(egui::RichText::new(fmt_isk(l.pod_value)).color(red).weak())
                                .on_hover_text("pod value");
                        }
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                party_badge(ui, party, 14.0, true);
                ui.label(egui::RichText::new(pilot).weak());
                if let Some(l) = lost {
                    if ui
                        .button(format!("{} zKill", icon::LINK))
                        .on_hover_text("Open on zKillboard")
                        .clicked()
                    {
                        let _ = open::that(format!("https://zkillboard.com/kill/{}/", l.kill_id));
                    }
                }
            });
        })
        .response;
    ui.add_space(3.0);
    resp
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct BattleHover {
    char_id: i64,
    kill_id: Option<i64>,
}

struct LoadedReport {
    title: String,
    battle: br_core::battle::Battle,
    inv: br_core::battle::Involvement,
    rosters: Vec<Vec<br_core::battle::Participant>>,
    sorted: Vec<Vec<br_core::battle::Participant>>,
    condensed_rows: Vec<Vec<crate::brview::CondensedRow>>,
    sorted_for: Option<(RosterSort, bool)>,
    hover: Option<BattleHover>,
}

pub(crate) fn battle_detail(
    ui: &mut egui::Ui,
    b: &br_core::battle::Battle,
    type_names: &std::collections::HashMap<i64, String>,
    inv: &br_core::battle::Involvement,
    rosters: &[Vec<br_core::battle::Participant>],
    condensed_rows: &[Vec<crate::brview::CondensedRow>],
    condensed: bool,
    prev_hover: Option<BattleHover>,
) -> (Option<i64>, Option<BattleHover>) {
    use egui_phosphor::regular as icon;
    use std::collections::HashSet;
    let mut open_system: Option<i64> = None;
    // Borrow (never clone) the hover-highlight sets: this runs every frame while a row is hovered.
    let killed: Option<&HashSet<i64>> = prev_hover.and_then(|h| inv.killed.get(&h.char_id));
    let border_set: Option<&HashSet<i64>> = prev_hover
        .and_then(|h| h.kill_id)
        .and_then(|kid| inv.attackers.get(&kid));
    let new_hover = std::cell::Cell::new(None);
    let span_min = ((b.end - b.start) / 60).max(0);
    ui.horizontal_wrapped(|ui| {
        for (id, name, sec) in &b.systems {
            ui.label(security_badge(*sec));
            if ui.link(egui::RichText::new(name).strong()).on_hover_text("Open system info").clicked() {
                open_system = Some(*id);
            }
        }
        ui.separator();
        ui.label(format!("{} kills", b.kills));
        ui.label(egui::RichText::new(format!("{} ISK", fmt_isk(b.isk))).weak());
        if span_min > 0 {
            ui.label(egui::RichText::new(format!("over {span_min}m")).weak());
        }
        let now = chrono::Utc::now().timestamp();
        let remaining = br_core::battle::BATTLE_WINDOW_SECS - (now - b.end);
        if remaining > 0 {
            let green = egui::Color32::from_rgb(0x6f, 0xcf, 0x7f);
            ui.label(egui::RichText::new(format!("{} Live", icon::BROADCAST)).color(green).strong())
                .on_hover_text(format!(
                    "Still accepting new kills for ~{}m. The view updates live.",
                    remaining / 60 + 1
                ));
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
    });
    ui.add_space(6.0);

    let green = egui::Color32::from_rgb(0x6f, 0xcf, 0x7f);
    let red = crate::theme::standing::HOSTILE;
    let name_of = |id: i64| -> String {
        if id == 0 {
            return "?".to_owned();
        }
        crate::intel::structure_name_by_type(id)
            .map(|s| s.to_owned())
            .or_else(|| type_names.get(&id).cloned())
            .unwrap_or_else(|| format!("Type {id}"))
    };

    const SIDE_W: f32 = 360.0;
    const MAX_ROWS: usize = 200;
    let col_h = (ui.available_height() - 12.0).max(180.0);
    let list_h = (col_h - 60.0).max(120.0);
    egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for (i, side) in b.sides.iter().enumerate() {
                let col = side_color(i);
                let roster = &rosters[i];
                egui::Frame::group(ui.style()).fill(col.gamma_multiply(0.05)).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(SIDE_W);
                        ui.set_min_width(SIDE_W);
                        ui.set_min_height(col_h);
                        ui.horizontal_wrapped(|ui| {
                            if let Some(lead) = side.parties.first() {
                                party_badge(ui, lead, 22.0, true);
                            }
                            ui.label(egui::RichText::new(side_title(side)).color(col).strong().size(15.0));
                            if side.parties.len() > 1 {
                                ui.label(egui::RichText::new(format!("+{}", side.parties.len() - 1)).weak())
                                    .on_hover_text(side.parties.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{} {}  {} {}", icon::SWORD, side.kills, icon::SKULL, side.losses)).weak(),
                            );
                            if let Some(eff) = side.isk_efficiency() {
                                let tint = if eff >= 50.0 { green } else { red };
                                ui.label(egui::RichText::new(format!("{eff:.0}% eff")).color(tint).strong())
                                    .on_hover_text(format!(
                                        "{} destroyed / {} lost",
                                        fmt_isk(side.isk_destroyed),
                                        fmt_isk(side.isk_lost)
                                    ));
                            }
                            ui.label(egui::RichText::new(format!("{} lost", fmt_isk(side.isk_lost))).weak());
                        });
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .id_salt(("battle_side", b.start, i))
                            .max_height(list_h)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(SIDE_W - 16.0);
                                let row_w = SIDE_W - 16.0;
                                if condensed {
                                    for r in &condensed_rows[i] {
                                        let resp = condensed_row(
                                            ui, row_w, r.ship, r.total, r.lost, r.ship_isk,
                                            r.pod_isk, &name_of, red,
                                        );
                                        if resp.hovered() {
                                            let hl = egui::Color32::from_rgba_unmultiplied(
                                                col.r(), col.g(), col.b(), 32,
                                            );
                                            ui.painter().rect_filled(resp.rect, 4.0, hl);
                                        }
                                    }
                                    if roster.is_empty() {
                                        ui.label(egui::RichText::new("No ships").weak());
                                    }
                                    return;
                                }
                                // `roster` is already sorted for the active sort by the worker.
                                for p in roster.iter().take(MAX_ROWS) {
                                    let row_kill = p.lost.as_ref().map(|l| l.kill_id);
                                    let is_hovered = p.char_id != 0
                                        && prev_hover.map_or(false, |h| h.char_id == p.char_id && h.kill_id == row_kill);
                                    let highlight = if is_hovered {
                                        ShipHighlight::Hovered
                                    } else if p.char_id != 0 && killed.is_some_and(|s| s.contains(&p.char_id)) {
                                        ShipHighlight::Assist
                                    } else {
                                        ShipHighlight::None
                                    };
                                    let border =
                                        p.char_id != 0 && border_set.is_some_and(|s| s.contains(&p.char_id));
                                    let resp = ship_row(
                                        ui, row_w, &p.party, p.ship, &p.pilot, &name_of,
                                        p.lost.as_ref(), red, highlight, border,
                                    );
                                    if p.char_id != 0 && ui.rect_contains_pointer(resp.rect) {
                                        new_hover.set(Some(BattleHover {
                                            char_id: p.char_id,
                                            kill_id: p.lost.as_ref().map(|l| l.kill_id),
                                        }));
                                    }
                                }
                                if roster.len() > MAX_ROWS {
                                    ui.label(egui::RichText::new(format!("+{} more", roster.len() - MAX_ROWS)).weak());
                                }
                                if roster.is_empty() {
                                    ui.label(egui::RichText::new("No ships").weak());
                                }
                            });
                    });
                });
                ui.add_space(6.0);
            }
        });
    });
    (open_system, new_hover.get())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn condensed_row(
    ui: &mut egui::Ui,
    row_w: f32,
    ship: i64,
    total: u32,
    lost: u32,
    ship_isk: f64,
    pod_isk: f64,
    name_of: &dyn Fn(i64) -> String,
    red: egui::Color32,
) -> egui::Response {
    let resp = ui
        .horizontal(|ui| {
            ui.set_min_width(row_w);
            hull_badge(ui, ship, 26.0);
            ui.label(egui::RichText::new(name_of(ship)).strong());
            ui.label(egui::RichText::new(format!("\u{00d7}{total}")).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if pod_isk > 0.0 {
                    ui.label(egui::RichText::new(format!("+{} pods", fmt_isk(pod_isk))).weak())
                        .on_hover_text("Cumulative pod ISK lost");
                }
                if ship_isk > 0.0 {
                    ui.label(egui::RichText::new(fmt_isk(ship_isk)).color(red))
                        .on_hover_text("Cumulative ship ISK lost");
                }
                if lost > 0 {
                    ui.label(egui::RichText::new(format!("{lost} lost")).color(red).strong());
                }
            });
        })
        .response;
    resp.interact(egui::Sense::hover())
}

fn rule_matches(
    ru: &crate::settings::AlertRule,
    r: &crate::intel::IntelReport,
    sev: crate::settings::Severity,
    jumps: Option<u32>,
    geo: &Option<std::sync::Arc<crate::geo::Systems>>,
    notes: &crate::notes::NotesView,
) -> bool {
    if sev < ru.min_severity {
        return false;
    }
    // By name: the engine fires on a report's first fresh tick, usually before ESI has resolved
    // who anyone is.
    if !ru.pilot_tags.is_empty()
        && !r.pilots.iter().any(|p| notes.pilot_by_name(p).is_some_and(|m| crate::notes::has_any(m, &ru.pilot_tags)))
    {
        return false;
    }
    if !ru.system_tags.is_empty()
        && !r.systems.iter().any(|s| notes.system(s.id).is_some_and(|m| crate::notes::has_any(m, &ru.system_tags)))
    {
        return false;
    }
    if !ru.channels.is_empty() && !r.killmail {
        static RE_CACHE: std::sync::LazyLock<
            std::sync::Mutex<std::collections::HashMap<String, Option<regex::Regex>>>,
        > = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
        let ch = r.channel.to_lowercase();
        let matched = ru.channels.iter().any(|pat| {
            let mut cache = RE_CACHE.lock().unwrap();
            let re = cache
                .entry(pat.clone())
                .or_insert_with(|| regex::Regex::new(&format!("(?i){pat}")).ok());
            match re {
                Some(re) => re.is_match(&r.channel),
                None => ch.contains(&pat.to_lowercase()),
            }
        });
        if !matched {
            return false;
        }
    }
    if let Some(mj) = ru.max_jumps {
        // Distance-limited rule: fire only when the report is provably within range. An
        // unmeasurable distance (no known character location, or an unreachable target while in a
        // wormhole) counts as out of range, or k-space intel floods alerts while in w-space.
        if !jumps.is_some_and(|j| j <= mj) {
            return false;
        }
    }
    let loc_filter =
        !ru.systems.is_empty() || !ru.constellations.is_empty() || !ru.regions.is_empty();
    if loc_filter {
        let matched = r.systems.iter().any(|ds| {
            if ru.systems.iter().any(|n| n.eq_ignore_ascii_case(&ds.name)) {
                return true;
            }
            if let Some(info) = geo.as_ref().and_then(|g| g.info_of(ds.id)) {
                ru.constellations.iter().any(|n| n.eq_ignore_ascii_case(&info.constellation))
                    || ru.regions.iter().any(|n| n.eq_ignore_ascii_case(&info.region))
            } else {
                false
            }
        });
        if !matched {
            return false;
        }
    }
    if let Some(mc) = ru.min_count {
        if !r.count.is_some_and(|c| c >= mc) {
            return false;
        }
    }
    if !ru.ships.is_empty() {
        let matched = r
            .ships
            .iter()
            .any(|s| ru.ships.iter().any(|n| n.eq_ignore_ascii_case(&s.name)));
        if !matched {
            return false;
        }
    }
    for tag in &ru.require {
        let ok = match tag.to_lowercase().as_str() {
            "bubble" => r.bubble,
            "camp" => r.camp,
            "cyno" => r.cyno,
            "dropper" | "hotdrop" | "hotdropper" | "blops" => r.dropper,
            "captackled" | "cap" => r.cap_tackled,
            "tackled" | "point" | "scram" => r.tackled,
            "kill" | "killmail" => r.killmail,
            "ess" => r.ess,
            "wormhole" | "wh" => r.wormhole,
            "spike" => r.spike,
            "skyhook" => r.skyhook,
            "filament" | "needlejack" | "trace" => r.filament,
            "nv" | "novisual" => r.no_visual,
            "help" | "sos" | "backup" => r.help,
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// One alert rule condition on tags: a menu of the active tags, "any tag", and whatever ids the rule
/// holds that no longer resolve, which can only be removed.
fn rule_tag_row(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash,
    label: &str,
    notes: &crate::notes::NotesView,
    kind: crate::notes::NoteKind,
    list: &mut Vec<String>,
) -> bool {
    use crate::notes::ANY_TAG;
    let mut changed = false;
    let name_of = |id: &str| -> String {
        if id == ANY_TAG {
            "any tag".to_owned()
        } else {
            notes.tag(id).map(|t| t.name.clone()).unwrap_or_else(|| "deleted tag".to_owned())
        }
    };
    ui.push_id(salt, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.menu_button("Edit", |ui| {
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    let mut toggle = |ui: &mut egui::Ui, id: &str, text: egui::RichText| {
                        let mut on = list.iter().any(|x| x == id);
                        if ui.checkbox(&mut on, text).changed() {
                            list.retain(|x| x != id);
                            if on {
                                list.push(id.to_owned());
                            }
                            changed = true;
                        }
                    };
                    toggle(ui, ANY_TAG, egui::RichText::new("Any tag"));
                    ui.separator();
                    for t in notes.tags(kind) {
                        toggle(ui, &t.id, egui::RichText::new(&t.name).color(crate::notes::color32(t.color)));
                    }
                    let dangling: Vec<String> =
                        list.iter().filter(|id| *id != ANY_TAG && notes.tag(id).is_none()).cloned().collect();
                    if !dangling.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Deleted or offline, matching nothing:").weak());
                        for id in dangling {
                            if ui.button(format!("{}  Remove", egui_phosphor::regular::X)).clicked() {
                                list.retain(|x| *x != id);
                                changed = true;
                            }
                        }
                    }
                });
            });
            let s = if list.is_empty() {
                "any".to_owned()
            } else if list.len() <= 3 {
                list.iter().map(|id| name_of(id)).collect::<Vec<_>>().join(", ")
            } else {
                format!("{} selected", list.len())
            };
            ui.label(egui::RichText::new(s).weak());
        });
    });
    changed
}

/// The intel search: report text, channel and system names, plus the tags and notes on its systems
/// and pilots. `query` is already lowercased.
pub(crate) fn intel_query_matches(r: &crate::intel::IntelReport, query: &str, notes: &crate::notes::NotesView) -> bool {
    r.text.to_lowercase().contains(query)
        || r.channel.to_lowercase().contains(query)
        || r.systems.iter().any(|s| {
            s.name.to_lowercase().contains(query) || notes.system(s.id).is_some_and(|m| notes.matches(m, query))
        })
        || r.pilots.iter().any(|p| notes.pilot_by_name(p).is_some_and(|m| notes.matches(m, query)))
}

fn op_key(text: &str) -> Option<String> {
    find_op_channel(text).map(|c| c.to_lowercase().replace(' ', ""))
}

fn find_op_channel(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let bytes = lower.as_bytes();
    for (idx, _) in lower.match_indices("op") {
        if idx > 0 && bytes[idx - 1].is_ascii_alphabetic() {
            continue;
        }
        let num: String =
            lower[idx + 2..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num.is_empty() {
            return Some(format!("Op {num}"));
        }
    }
    None
}

enum RowAction {
    None,
    Load,
    Delete,
    Edit,
    Commit,
    Cancel,
}

#[allow(clippy::too_many_arguments)]
fn route_item_row(
    ui: &mut egui::Ui,
    it: &RouteItem,
    from_name: &str,
    to_name: &str,
    is_editing: bool,
    edit_name: &mut String,
    edit_folder: &mut String,
    folders: &[String],
) -> RowAction {
    let mut act = RowAction::None;
    if is_editing {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(edit_name).desired_width(120.0).hint_text("Name"));
            egui::ComboBox::from_id_salt(("route_edit_folder", it.name.as_str()))
                .selected_text(if edit_folder.is_empty() {
                    "(root)".to_owned()
                } else {
                    edit_folder.clone()
                })
                .show_ui(ui, |ui| {
                    ui.menu_value(edit_folder, String::new(), "(root)");
                    for f in folders {
                        ui.menu_value(edit_folder, f.clone(), f);
                    }
                });
            if ui.button("Save").clicked() {
                act = RowAction::Commit;
            }
            if ui.button("Cancel").clicked() {
                act = RowAction::Cancel;
            }
        });
    } else {
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                act = RowAction::Load;
            }
            ui.label(egui::RichText::new(&it.name).strong());
            ui.label(egui::RichText::new(format!("{from_name} \u{2192} {to_name}")).weak());
            ui.label(egui::RichText::new(format!("{}j", it.jumps)).weak());
            if it.wp > 0 {
                ui.label(egui::RichText::new(format!("{} wp", it.wp)).weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::TRASH).on_hover_text("Delete").clicked() {
                    act = RowAction::Delete;
                }
                if ui
                    .button(egui_phosphor::regular::PENCIL_SIMPLE)
                    .on_hover_text("Rename / move to folder")
                    .clicked()
                {
                    act = RowAction::Edit;
                }
            });
        });
    }
    act
}

/// Direct Mumble deep-link for a command-comms channel (op 1-12). Channels 9-12 carry a "- SC"
/// suffix. Opened locally by the rescue window's Command Comms button; never posted, because a
/// `mumble://` URL is dead on clients with no protocol handler.
#[cfg(feature = "fleet")]
fn command_mumble_url(ch: u8) -> String {
    let ch = ch.clamp(1, 12);
    let chan = if ch >= 9 {
        format!("Command%20{ch}%20-%20SC")
    } else {
        format!("Command%20{ch}")
    };
    format!(
        "mumble://mumble.goonfleet.com/Ops/Command%20Sector%20Alpha/{chan}?title=Goonfleet&version=1.2.0"
    )
}

/// Goonfleet gnf.lt short link for an op channel's regular comms. Verified to redirect to
/// `mumble://.../Ops/Op Channels/OP <n> - ...`, not Command Sector Alpha.
#[cfg(feature = "fleet")]
fn op_comms_short_link(ch: u8) -> Option<&'static str> {
    crate::fleets::comms::builtin_op_link(ch)
}

/// Comms link for anything we POST. Always the gnf.lt short link, never a raw `mumble://` URL:
/// those silently fail on clients and OSes that have no handler registered. Empty when the op has
/// no short link (only op 8, which the op picker doesn't offer).
#[cfg(feature = "fleet")]
fn op_comms_url(ch: u8) -> String {
    op_comms_short_link(ch).unwrap_or_default().to_owned()
}

/// Cosmetic: the ping bot replies "… requests the attention of: a, b, c, …" with huge name lists.
/// Collapse the trailing comma-separated list to "[n users]" for DISPLAY only. Mention detection
/// runs on the raw body elsewhere, so a mention still highlights/counts.
fn condense_attention_list(body: &str) -> std::borrow::Cow<'_, str> {
    const MARKER: &str = "requests the attention of:";
    let lower = body.to_ascii_lowercase();
    if let Some(pos) = lower.find(MARKER) {
        let after = pos + MARKER.len();
        let list = body[after..].trim();
        if !list.is_empty() {
            let n = list.split(',').filter(|s| !s.trim().is_empty()).count();
            let unit = if n == 1 { "user" } else { "users" };
            return std::borrow::Cow::Owned(format!("{} [{n} {unit}]", body[..after].trim_end()));
        }
    }
    std::borrow::Cow::Borrowed(body)
}

/// One chat line, jabber-style: the sender's name in its per-name colour, then the message body.
#[cfg(feature = "fleet")]
fn rescue_chat_line(
    ui: &mut egui::Ui,
    id: egui::Id,
    who: &str,
    body: &str,
    grouped: bool,
    show: MsgActions,
) -> MsgRowAction {
    let body = condense_attention_list(body);
    message_row(ui, id, false, show, |ui| {
        // Not `horizontal_wrapped`: that floors the row at `interact_size.y`, 11px of dead air per
        // line on a row that only holds a nick, text and the occasional link.
        let row = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
        let size = egui::vec2(ui.available_size_before_wrap().x, 0.0);
        ui.allocate_ui_with_layout(size, row, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if !grouped {
                ui.label(egui::RichText::new(format!("{who}:")).color(name_color(who)).strong());
            }
            // Same body renderer as the main chat: URLs become clickable links, text stays selectable.
            render_message_body(ui, body.as_ref());
        });
    })
}

/// Consecutive lines from one sender inside five minutes share a header, as on the Jabber page.
#[cfg(feature = "fleet")]
fn rescue_grouped(sender: &str, time: i64, prev_sender: Option<&str>, prev_time: i64) -> bool {
    prev_sender == Some(sender) && time >= prev_time && time - prev_time < 300
}

/// The rescue window's chat feed. Returns the action the user clicked plus the nick and the raw
/// body of the message it came from.
#[cfg(feature = "fleet")]
pub(crate) fn rescue_chat_feed(
    ui: &mut egui::Ui,
    msgs: &[(String, String, bool, i64)],
    salt: &str,
) -> Option<(MsgRowAction, String, String)> {
    if msgs.is_empty() {
        ui.label(egui::RichText::new("(no messages)").weak());
        return None;
    }
    let now = chrono::Utc::now().timestamp();
    let mut out = None;
    let mut prev_sender: Option<String> = None;
    let mut prev_time = 0i64;
    for (i, (who, body, outgoing, time)) in msgs.iter().enumerate() {
        let sender = if *outgoing { "\u{0}me".to_owned() } else { who.clone() };
        let grouped = rescue_grouped(&sender, *time, prev_sender.as_deref(), prev_time);
        if !grouped {
            ui.add_space(5.0);
            ui.label(egui::RichText::new(eve_time_label(*time, now)).weak().size(9.5));
        }
        let show = MsgActions { copy: true, mention: !*outgoing, dm: !*outgoing };
        let act = rescue_chat_line(ui, ui.id().with((salt, i)), who, body, grouped, show);
        if act != MsgRowAction::None {
            out = Some((act, who.clone(), body.clone()));
        }
        prev_sender = Some(sender);
        prev_time = *time;
    }
    out
}

/// The directorbot posts under a per-room nick ("DelveBot" in delve911) and echoes every ping back.
/// Matched exactly, not by a "bot" suffix, so a pilot named like a bot still gets rescued.
#[cfg(feature = "fleet")]
fn is_ping_bot(nick: &str) -> bool {
    ["delvebot", "directorbot"].contains(&nick.to_ascii_lowercase().as_str())
}

/// Soft amber pulse marking an action the FC hasn't taken yet. `None` once done, so the button
/// falls back to its normal styling.
#[cfg(feature = "fleet")]
fn pulse_fill(ui: &egui::Ui, pending: bool) -> Option<egui::Color32> {
    if !pending {
        return None;
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    let k = 0.5 + 0.5 * (ui.input(|i| i.time) * 2.2).sin();
    Some(egui::Color32::from_rgba_unmultiplied(0xE6, 0xA5, 0x1E, (36.0 + 76.0 * k) as u8))
}

/// "<nick>: Join OP <n> Comms <link>" for the delve911 channel. Deliberately the op's regular
/// comms, never Command Sector Alpha, so the rescued pilot lands where the fleet is.
///
/// The link ends the message with nothing after it: a trailing `)` (or any punctuation) gets
/// swallowed into the URL by the receiving client's auto-linker and breaks the link.
#[cfg(feature = "fleet")]
fn rescue_comms_invite(author: Option<&str>, op: u8) -> Option<String> {
    let author = author.map(str::trim).filter(|a| !a.is_empty())?;
    let link = op_comms_short_link(op)?;
    Some(format!("{author}: Join OP {op} Comms {link}"))
}

/// Empty setting -> the goonfleet default room the app already joins.
#[cfg(feature = "fleet")]
pub(crate) fn goon_jid(cfg: &str, default: &str) -> String {
    if cfg.trim().is_empty() { default.to_string() } else { cfg.trim().to_string() }
}

/// Floating pin, for viewports whose content is a bare central panel with no row to host it.
fn ontop_pin(ctx: &egui::Context, id: &str) {
    egui::Area::new(egui::Id::new(("ontop_area", id)))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-6.0, 6.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| ontop_pin_ui(ui, id));
}

/// Width `ontop_pin_ui` takes, so a row can reserve it before laying out its own content.
fn ontop_pin_w(ui: &egui::Ui) -> f32 {
    ontop_pin_size(ui).x
}

fn ontop_pin_size(ui: &egui::Ui) -> egui::Vec2 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text = ui
        .painter()
        .layout_no_wrap(
            egui_phosphor::regular::PUSH_PIN.to_owned(),
            font,
            egui::Color32::WHITE,
        )
        .size();
    let size = text + 2.0 * ui.spacing().button_padding;
    egui::vec2(size.x, size.y.max(ui.spacing().interact_size.y))
}

/// The pin itself, laid out where it is called. `id` is the viewport's, since the toggle state and
/// the window command both belong to that window.
fn ontop_pin_ui(ui: &mut egui::Ui, id: &str) {
    let ctx = ui.ctx().clone();
    let key = egui::Id::new(("ontop", id));
    let mut on = ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(true));
    if ui
        .menu_label(on, egui_phosphor::regular::PUSH_PIN)
        .on_hover_text(if on { "Always on top (on)" } else { "Always on top (off)" })
        .clicked()
    {
        on = !on;
        ctx.data_mut(|d| d.insert_temp(key, on));
    }
    // Only send the window-level command when it changes. Sending it every frame leaves a pending
    // viewport command that forces a repaint each frame, spinning the dialog at 100% CPU.
    let applied_key = egui::Id::new(("ontop_applied", id));
    let applied = ctx.data(|d| d.get_temp::<bool>(applied_key));
    if applied != Some(on) {
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
        ctx.data_mut(|d| d.insert_temp(applied_key, on));
    }
}

pub(crate) fn notify_os(summary: &str, body: &str) {
    let (summary, body) = (summary.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().summary(&summary).body(&body).show();
    });
}

/// An arch between two points, sampled as a polyline.
///
/// Jump bridges are drawn as arches rather than straight lines: a bridge and a gate between the same
/// pair of systems are otherwise the same stroke in a different colour, and on a busy map colour
/// alone is not enough to tell a route you can fly from one you need a bridge for.
///
/// The apex rises straight up the screen rather than perpendicular to the segment. The map is a
/// top-down projection of a plane, so "above the plane" is up, whatever direction the bridge runs;
/// a perpendicular bow makes a north-south bridge bulge sideways, which reads as a detour rather
/// than as height.
pub(crate) fn arc_polyline(a: egui::Pos2, b: egui::Pos2, bow: f32) -> Vec<egui::Pos2> {
    let d = b - a;
    let len = d.length();
    if len < 0.5 {
        return vec![a, b];
    }
    let mid = a + d * 0.5;
    let ctrl = mid - egui::vec2(0.0, len * bow * 2.0);
    (0..=14)
        .map(|i| {
            let t = i as f32 / 14.0;
            let u = 1.0 - t;
            egui::pos2(
                u * u * a.x + 2.0 * u * t * ctrl.x + t * t * b.x,
                u * u * a.y + 2.0 * u * t * ctrl.y + t * t * b.y,
            )
        })
        .collect()
}

/// Marks the only direction a route may take a bridge, `inset` back from the end of `arc` so the
/// destination's dot does not cover it.
pub(crate) fn bridge_arrowhead(painter: &egui::Painter, arc: &[egui::Pos2], color: egui::Color32, inset: f32) {
    let mut left = inset;
    let mut at = None;
    for w in arc.windows(2).rev() {
        let (a, b) = (w[0], w[1]);
        let len = (b - a).length();
        if len >= left && len > 0.0 {
            let dir = (b - a) / len;
            at = Some((b - dir * left, dir));
            break;
        }
        left -= len;
    }
    let Some((tip, dir)) = at else { return };
    let back = tip - dir * 10.0;
    let side = egui::vec2(-dir.y, dir.x) * 5.5;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, back + side, back - side],
        color,
        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(160)),
    ));
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// `pts` as a solid line shading from `from` to `to` along its length.
pub(crate) fn gradient_polyline(painter: &egui::Painter, pts: &[egui::Pos2], from: egui::Color32, to: egui::Color32, width: f32) {
    let total: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let len = (w[1] - w[0]).length();
        let t = if total > 0.0 { (walked + len * 0.5) / total } else { 0.0 };
        painter.line_segment([w[0], w[1]], egui::Stroke::new(width, lerp_color(from, to, t)));
        walked += len;
    }
}

/// How high a bridge arch rises, as a fraction of its own length.
pub(crate) const BRIDGE_BOW: f32 = 0.12;

fn open_mumble(link: String) {
    std::thread::spawn(move || {
        let resolved = crate::http::client(10)
            .ok()
            .and_then(|client| {
                for attempt in 1..=5 {
                    let got = client
                        .get(&link)
                        .send()
                        .and_then(|r| r.error_for_status())
                        .and_then(|r| r.text())
                        .ok()
                        .and_then(|body| crate::pings::extract_mumble_url(&body));
                    if got.is_some() {
                        return got;
                    }
                    if attempt < 5 {
                        std::thread::sleep(std::time::Duration::from_millis(400));
                    }
                }
                None
            });
        match &resolved {
            Some(url) => match open::that(url) {
                Ok(_) => return,
                Err(e) => eprintln!("[mumble] opening {url} failed ({e}); falling back to browser"),
            },
            None => eprintln!("[mumble] could not resolve {link} after 5 tries; opening in browser"),
        }
        let _ = open::that(&link);
    });
}

fn alert_text(r: &crate::intel::IntelReport) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(s) = r.primary_system() {
        parts.push(s.name.clone());
    }
    if let Some(n) = r.count {
        parts.push(format!("{n} hostiles"));
    }
    if r.bubble {
        parts.push("bubble".into());
    }
    if r.nullified {
        parts.push("nullified".into());
    }
    if r.camp {
        parts.push("gate camp".into());
    }
    if r.cyno {
        parts.push("CYNO".into());
    }
    if r.filament {
        parts.push("FILAMENT".into());
    }
    if r.diamond_rats {
        parts.push("\u{25C6} Rats \u{25C6}".into());
    }
    for (kind, code) in &r.anom_sigs {
        let word = match kind {
            crate::intel::AnomKind::Anomaly => "Anom",
            crate::intel::AnomKind::Signature => "Sig",
        };
        parts.push(if code.is_empty() { word.to_string() } else { format!("{word} {code}") });
    }
    if r.dropper {
        parts.push("DROPPER".into());
    }
    if r.cap_tackled {
        parts.push("CAP TACKLED".into());
    }
    for sh in r.ships.iter().take(4) {
        parts.push(sh.name.clone());
    }
    if r.clear {
        parts.push("clear".into());
    }
    if parts.is_empty() {
        parts.push(r.text.clone());
    }
    parts.join(" · ")
}

pub(crate) fn render_ping(
    ui: &mut egui::Ui,
    p: &crate::pings::Ping,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    highlight: bool,
    doctrine_url: &str,
    op_links: &std::collections::HashMap<String, String>,
) {
    use crate::pings::{Comms, Formup, PapType, Ping};
    use egui_phosphor::regular as icon;
    let mumble_row = |ui: &mut egui::Ui, label: String, link: &str| {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            if ui
                .button(format!("{}  Join Mumble", icon::HEADSET))
                .on_hover_text("Open the Mumble client on this channel")
                .clicked()
            {
                open_mumble(link.to_owned());
            }
            let resp = ui.hyperlink_to(icon::LINK, link).on_hover_text(link);
            link_copy_menu(&resp, link);
        });
    };
    let sys_name = |id: i64| -> String {
        systems
            .as_ref()
            .and_then(|g| g.info_of(id))
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "?".to_owned())
    };
    let formup_str = |fs: &[Formup]| {
        fs.iter()
            .map(|f| match f {
                Formup::System(id) => sys_name(*id),
                Formup::Text(t) => t.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let now = chrono::Utc::now().timestamp();
    let ago = human_ago(now - p.timestamp());
    let frame = if highlight {
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(2.0, ui.visuals().hyperlink_color))
            .fill(ui.visuals().hyperlink_color.gamma_multiply(0.08))
    } else {
        egui::Frame::group(ui.style())
    };
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match p {
            Ping::Fleet { fc, fleet, formup, pap, comms, doctrine, description, source, target, .. } => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("{}  Fleet ping", icon::MEGAPHONE)).strong());
                    if let Some(f) = fleet {
                        ui.label(egui::RichText::new(f).strong());
                    }
                    if let Some(p) = pap {
                        let (t, c) = match p {
                            PapType::Strategic => ("STRAT", crate::theme::standing::HOSTILE),
                            PapType::Peacetime => ("PEACE", crate::theme::standing::WARNING),
                            PapType::Text(s) => (s.as_str(), ui.visuals().weak_text_color()),
                        };
                        ui.label(egui::RichText::new(t).color(c).strong());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(format!("{}  Copy", icon::COPY))
                            .on_hover_text("Copy the ping text")
                            .clicked()
                        {
                            ui.ctx().copy_text(p.raw().to_owned());
                        }
                        ui.label(egui::RichText::new(format!("{ago} ago")).weak());
                    });
                });
                ui.label(format!("FC: {fc}"));
                if !formup.is_empty() {
                    ui.label(format!("Formup: {}", formup_str(formup)));
                }
                if let Some(c) = comms {
                    match c {
                        Comms::Mumble { channel, link } => {
                            mumble_row(ui, format!("Comms: {channel}"), link);
                        }
                        Comms::Text(t) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Comms:");
                                render_linked_text(ui, t, false);
                            });
                        }
                    }
                } else if let Some(op) = find_op_channel(description) {
                    match op_key(&op).and_then(|k| op_links.get(&k)) {
                        Some(link) => mumble_row(ui, format!("Comms: {op}"), link),
                        None => {
                            ui.label(egui::RichText::new(format!("Comms: {op}?")).weak());
                        }
                    }
                }
                if doctrine.is_some() || !doctrine_url.is_empty() {
                    // `horizontal_wrapped` floors the row at `interact_size.y` on the assumption
                    // it holds a button. This one only ever holds text and links, so that floor is
                    // 11px of dead air breaking the rhythm of the metadata rows above it.
                    let row = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
                    let size = egui::vec2(ui.available_size_before_wrap().x, 0.0);
                    ui.allocate_ui_with_layout(size, row, |ui| {
                        if let Some(d) = doctrine {
                            if let Some(url) = crate::doctrines::link_for(d) {
                                if ui
                                    .link(format!("Doctrine: {d} \u{2197}"))
                                    .on_hover_text(url)
                                    .clicked()
                                {
                                    let _ = open::that(url);
                                }
                            } else {
                                ui.label(format!("Doctrine: {d}"));
                            }
                        }
                        if !doctrine_url.is_empty()
                            && ui.link("Doctrines \u{2197}").on_hover_text(doctrine_url).clicked()
                        {
                            let _ = open::that(doctrine_url);
                        }
                    });
                }
                if !description.is_empty() {
                    render_ping_body(ui, description, true);
                }
                let from = source.as_deref().unwrap_or("?");
                let to = target.as_deref().unwrap_or("?");
                ui.label(egui::RichText::new(format!("{from} {} {to}", icon::ARROW_RIGHT)).weak());
            }
            Ping::Plain { text, sender, target, .. } => {
                ui.horizontal_wrapped(|ui| {
                    let from = sender.as_deref().unwrap_or("ping");
                    let to = target.as_deref().map(|t| format!(" {} {t}", icon::ARROW_RIGHT)).unwrap_or_default();
                    ui.label(egui::RichText::new(format!("{from}{to}")).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(format!("{}  Copy", icon::COPY))
                            .on_hover_text("Copy the ping text")
                            .clicked()
                        {
                            ui.ctx().copy_text(p.raw().to_owned());
                        }
                        ui.label(egui::RichText::new(format!("{ago} ago")).weak());
                    });
                });
                render_ping_body(ui, text, false);
                // Offer the op channel's cached Mumble link (from earlier well-formed pings).
                if let Some(chan) = find_op_channel(text) {
                    let key = chan.to_lowercase().replace(' ', "");
                    if let Some(link) = op_links.get(&key) {
                        mumble_row(ui, chan, link.as_str());
                    }
                }
            }
        }
    });
}

/// The warning line as a button, so the intel behind it can be read.
///
/// Returns whether it was clicked. Only clickable when there is intel: a line that only says "3 kills
/// this hour" has nothing to open.
fn warn_button(ui: &mut egui::Ui, w: &crate::web::route::HopWarning) -> bool {
    let Some((text, col)) = warn_text(w) else { return false };
    if w.sev < crate::web::route::WARN_SEVERITY {
        ui.label(egui::RichText::new(text).color(col).size(11.0));
        return false;
    }
    ui.add(egui::Button::new(egui::RichText::new(text).color(col).size(11.0)).frame(false))
        .on_hover_text("Show the intel for this system")
        .clicked()
}

/// One line of "why not to fly through here", under a hop, or nothing when there is nothing to warn
/// about.
///
/// Intel below Danger is deliberately absent: a nullsec route passes through dozens of systems
/// someone has said something about, and a warning on all of them is a warning on none.
fn warn_text(w: &crate::web::route::HopWarning) -> Option<(String, egui::Color32)> {
    let mut bits: Vec<String> = Vec::new();
    if w.sev >= crate::web::route::WARN_SEVERITY {
        let age = fmt_age((chrono::Utc::now().timestamp() - w.at).max(0));
        bits.push(format!("{} intel {age}", if w.sev >= 3 { "Critical" } else { "Danger" }));
    }
    // No "this hour": the figures are hourly and every row saying so is three words of the same
    // thing on every row.
    if w.kills > 0 || w.pods > 0 {
        let mut k = format!("{} kills", w.kills);
        if w.pods > 0 {
            k.push_str(&format!(" · {} pods", w.pods));
        }
        bits.push(k);
    }
    if bits.is_empty() {
        return None;
    }
    let col = if w.sev >= 3 {
        crate::theme::standing::HOSTILE
    } else {
        crate::theme::standing::WARNING
    };
    Some((format!("{}  {}", egui_phosphor::regular::WARNING, bits.join(" · ")), col))
}

pub(crate) fn severity_of(
    r: &crate::intel::IntelReport,
    rules: &crate::settings::SeverityRules,
) -> crate::settings::Severity {
    use crate::settings::Severity::*;
    let mut s = if r.killmail && r.channel.eq_ignore_ascii_case("zkill") { Warning } else { Info };
    if let Some(n) = r.count {
        s = s.max(if n >= rules.big_gang_threshold { rules.big_gang } else { rules.small_gang });
    } else if !r.systems.is_empty() && !r.clear && !r.killmail && !r.status {
        s = s.max(rules.small_gang);
    }
    if r.bubble {
        s = s.max(rules.bubble);
    }
    if r.camp {
        s = s.max(rules.gate_camp);
    }
    if r.spike {
        s = s.max(rules.spike);
    }
    if r.cyno {
        s = s.max(rules.cyno);
    }
    if r.dropper {
        s = s.max(rules.dropper);
    }
    if r.cap_tackled {
        s = s.max(rules.cap_tackled);
    }
    if r.killmail {
        s = s.max(rules.kill);
    }
    if r.no_visual {
        s = s.max(rules.no_visual);
    }
    if r.wormhole {
        s = s.max(rules.wormhole);
    }
    if r.ess {
        s = s.max(rules.ess);
    }
    if r.ships.iter().any(|sh| rules.threat_ships.iter().any(|t| t.eq_ignore_ascii_case(&sh.name))) {
        s = s.max(rules.threat_ship);
    }
    s
}

pub(crate) fn severity_color(s: crate::settings::Severity) -> egui::Color32 {
    use crate::settings::Severity::*;
    match s {
        Info => egui::Color32::from_rgb(0x6E, 0x7A, 0x86),
        Warning => crate::theme::standing::WARNING,
        Danger => egui::Color32::from_rgb(0xE6, 0x6A, 0x2A),
        Critical => crate::theme::standing::HOSTILE,
    }
}

pub(crate) fn build_last_ship(
    reports: &[crate::intel::IntelReport],
) -> std::collections::HashMap<String, (i64, String, i64)> {
    let mut out: std::collections::HashMap<String, (i64, String, i64)> =
        std::collections::HashMap::new();
    for r in reports {
        if r.pilots.len() == 1 && r.ships.len() == 1 {
            let sh = &r.ships[0];
            let e = out
                .entry(r.pilots[0].to_lowercase())
                .or_insert((sh.id, sh.name.clone(), r.received));
            if r.received >= e.2 {
                *e = (sh.id, sh.name.clone(), r.received);
            }
        }
    }
    out
}

fn fmt_age(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    }
}

/// Narrow age for compact mode: seconds under a minute, then minutes-only, then hours-only.
fn fmt_age_compact(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else {
        format!("{}h", s / 3600)
    }
}

fn resize_grip(ui: &mut egui::Ui) {
    const SZ: f32 = 18.0;
    let corner = ui.max_rect().right_bottom();
    let rect = egui::Rect::from_min_max(corner - egui::vec2(SZ, SZ), corner);
    let resp = ui.interact(rect, ui.id().with("resize_grip"), egui::Sense::drag());
    let hot = resp.hovered() || resp.dragged();
    let col = if hot {
        ui.visuals().strong_text_color()
    } else {
        // Brighter than weak text so the grip reads on the dark fill even in a tiny compact window.
        egui::Color32::from_rgb(0x8a, 0x92, 0x9c)
    };
    // Paint on the foreground layer so scroll content / scrollbars can't cover the grip.
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        ui.id().with("resize_grip_paint"),
    ));
    let br = corner - egui::vec2(3.0, 3.0);
    for i in 0..3 {
        let o = 4.0 * (i as f32 + 1.0);
        painter.line_segment(
            [egui::pos2(br.x - o, br.y), egui::pos2(br.x, br.y - o)],
            egui::Stroke::new(1.5, col),
        );
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(
            egui::ResizeDirection::SouthEast,
        ));
    }
}

fn report_key(r: &crate::intel::IntelReport) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    r.received.hash(&mut h);
    r.reporter.hash(&mut h);
    r.text.len().hash(&mut h);
    h.finish()
}

pub(crate) fn uncertain_set(
    cache: &crate::pilot::PilotCache,
    resolved: &std::collections::HashMap<String, i64>,
) -> crate::pilot::UncertainPilots {
    resolved.keys().filter(|n| cache.is_uncertain(n)).collect()
}

/// A 0..=100% volume slider. Returns true when the value changed.
fn volume_slider(ui: &mut egui::Ui, value: &mut f32) -> bool {
    ui.add(
        egui::Slider::new(value, 0.0..=1.0)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
            .custom_parser(|s| {
                s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|p| p / 100.0)
            }),
    )
    .changed()
}

fn sound_picker(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash,
    allow_default: bool,
    value: &mut String,
    volume: f32,
) -> bool {
    use egui_phosphor::regular as icon;
    let mut changed = false;
    ui.horizontal(|ui| {
        let is_default = allow_default && value.is_empty();
        let is_off = value.eq_ignore_ascii_case("off") || (!allow_default && value.is_empty());
        let is_file = std::path::Path::new(value.as_str()).is_file();
        let label = if is_default {
            "Default".to_owned()
        } else if is_off {
            "Off".to_owned()
        } else if is_file {
            let name = std::path::Path::new(value.as_str())
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| value.clone());
            format!("{} {name}", icon::FILE_AUDIO)
        } else {
            value.clone()
        };
        egui::ComboBox::from_id_salt(("sound_picker", salt)).selected_text(label).show_ui(ui, |ui| {
            if allow_default && ui.menu_label(is_default, "Default").clicked() {
                value.clear();
                changed = true;
            }
            if ui.menu_label(is_off, "Off").clicked() {
                *value = "off".to_owned();
                changed = true;
            }
            for &p in crate::sound::PRESETS {
                ui.horizontal(|ui| {
                    if ui.menu_label(value.eq_ignore_ascii_case(p), p).clicked() {
                        *value = p.to_owned();
                        changed = true;
                    }
                    if ui.small_button(icon::PLAY).on_hover_text("Preview").clicked() {
                        crate::sound::play(p, volume);
                    }
                });
            }
            if ui.menu_label(is_file, format!("{} Custom file…", icon::FOLDER_OPEN)).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("audio", &["wav", "mp3", "ogg", "flac"])
                    .pick_file()
                {
                    *value = path.to_string_lossy().into_owned();
                    changed = true;
                }
            }
        });
        if ui.button(icon::PLAY).on_hover_text("Test").clicked() {
            crate::sound::play(value, volume);
        }
    });
    changed
}

fn pick_loss(
    report: &crate::lookup::PilotReport,
    ship_id: i64,
    mode: FitMode,
) -> Option<crate::lookup::Loss> {
    let losses: Vec<&crate::lookup::Loss> =
        report.losses.iter().filter(|l| l.ship_type_id == ship_id).collect();
    match mode {
        FitMode::Recent => losses.iter().max_by_key(|l| l.time).map(|l| (*l).clone()),
        FitMode::MostUsed => {
            let mut groups: std::collections::HashMap<Vec<i64>, (u32, &crate::lookup::Loss)> =
                std::collections::HashMap::new();
            for l in &losses {
                let e = groups.entry(l.signature()).or_insert((0, l));
                e.0 += 1;
                if l.time > e.1.time {
                    e.1 = l;
                }
            }
            groups.into_values().max_by_key(|(c, _)| *c).map(|(_, l)| l.clone())
        }
    }
}

fn fit_cargo(loss: &crate::lookup::Loss) -> std::collections::BTreeMap<i64, i64> {
    use crate::lookup::Slot;
    let mut cargo: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for it in &loss.items {
        match crate::lookup::slot_of(it.flag) {
            Slot::Cargo | Slot::Other => *cargo.entry(it.type_id).or_insert(0) += it.qty.max(1),
            _ if it.qty > 1 => *cargo.entry(it.type_id).or_insert(0) += it.qty,
            _ => {}
        }
    }
    cargo
}

fn eft_string(
    ship: &str,
    loss: &crate::lookup::Loss,
    names: &std::collections::HashMap<i64, String>,
) -> String {
    use crate::lookup::Slot;
    let name = |id: i64| names.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"));
    let mut sections: Vec<Vec<String>> = vec![Vec::new(); 5];
    let idx = |s: Slot| match s {
        Slot::Low => 0,
        Slot::Mid => 1,
        Slot::High => 2,
        Slot::Rig => 3,
        _ => 4,
    };
    for it in &loss.items {
        let s = crate::lookup::slot_of(it.flag);
        if !matches!(s, Slot::Cargo | Slot::Other) && it.qty == 1 {
            sections[idx(s)].push(name(it.type_id));
        }
    }
    let mut out = format!("[{ship}, EVE Spai]\n");
    for sec in &sections {
        for line in sec {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    for (tid, q) in fit_cargo(loss) {
        if q > 1 {
            out.push_str(&format!("{} x{}\n", name(tid), q));
        } else {
            out.push_str(&format!("{}\n", name(tid)));
        }
    }
    out
}

const FIT_SITES: &[(&str, &str)] =
    &[("eveship", "EVEShip.fit"), ("workbench", "EVE Workbench"), ("zkillboard", "zKillboard")];

fn site_label(site: &str) -> &str {
    FIT_SITES.iter().find(|(id, _)| *id == site).map(|(_, l)| *l).unwrap_or(site)
}

fn fit_url(site: &str, loss: &crate::lookup::Loss) -> String {
    match site {
        "eveship" => format!("https://eveship.fit/?fit=killmail:{}/{}", loss.killmail_id, loss.hash),
        "workbench" => "https://eveworkbench.com/fitting".to_owned(),
        _ => format!("https://zkillboard.com/kill/{}/", loss.killmail_id),
    }
}

enum UpgradeIcon {
    Mineral(i64),
    Glyph(&'static str),
}

fn upgrade_info(name: &str) -> (UpgradeIcon, u8) {
    use egui_phosphor::regular as icon;
    let lower = name.to_lowercase();
    let level = name.chars().rev().find(|c| c.is_ascii_digit()).and_then(|c| c.to_digit(10)).unwrap_or(0)
        as u8;
    const MINERALS: &[(&str, i64)] = &[
        ("tritanium", 34),
        ("pyerite", 35),
        ("mexallon", 36),
        ("isogen", 37),
        ("nocxium", 38),
        ("zydrine", 39),
        ("megacyte", 40),
        ("morphite", 11399),
    ];
    for (m, id) in MINERALS {
        if lower.contains(m) {
            return (UpgradeIcon::Mineral(*id), level);
        }
    }
    let glyph = if lower.contains("pirate")
        || lower.contains("detection")
        || lower.contains("reconnaissance")
        || lower.contains("insurgenc")
        || lower.contains("ratting")
    {
        icon::SKULL
    } else if lower.contains("scan")
        || lower.contains("survey")
        || lower.contains("explor")
        || lower.contains("relic")
        || lower.contains("data")
    {
        icon::BROADCAST
    } else if lower.contains("cyno") {
        icon::RADIOACTIVE
    } else {
        icon::GEAR
    };
    (UpgradeIcon::Glyph(glyph), level)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpgradeKind {
    Ratting = 0,
    Exploration = 1,
    Mining = 2,
    Other = 3,
}

fn upgrade_kind(name: &str) -> UpgradeKind {
    let lower = name.to_lowercase();
    const MINERALS: &[&str] = &[
        "tritanium", "pyerite", "mexallon", "isogen", "nocxium", "zydrine", "megacyte", "morphite",
    ];
    if MINERALS.iter().any(|m| lower.contains(m)) {
        UpgradeKind::Mining
    } else if lower.contains("pirate")
        || lower.contains("detection")
        || lower.contains("reconnaissance")
        || lower.contains("insurgenc")
        || lower.contains("ratting")
    {
        UpgradeKind::Ratting
    } else if lower.contains("scan")
        || lower.contains("survey")
        || lower.contains("explor")
        || lower.contains("relic")
        || lower.contains("data")
    {
        UpgradeKind::Exploration
    } else {
        UpgradeKind::Other
    }
}

fn level_color(l: u8) -> egui::Color32 {
    match l {
        2 => egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
        3..=5 => egui::Color32::from_rgb(0xE5, 0x4B, 0x4B),
        _ => egui::Color32::WHITE,
    }
}

/// Discord's rule: a real number up to 99, then `99+`. Past a hundred the exact figure has stopped
/// meaning anything and the width starts costing more than the precision is worth.
pub(crate) fn badge_count(n: u32) -> String {
    if n > 99 {
        "99+".to_owned()
    } else {
        n.to_string()
    }
}

fn is_hidden_region(region: &str) -> bool {
    region.chars().any(|c| c.is_ascii_digit())
}

fn hull_size(g: &str) -> &'static str {
    if g.contains("Capsule") {
        "Capsule"
    } else if g.contains("Titan")
        || g.contains("Carrier")
        || g.contains("Dreadnought")
        || g.contains("Force Auxiliary")
        || g.contains("Capital")
    {
        "Capital"
    } else if g.contains("Freighter")
        || g.contains("Industrial")
        || g.contains("Hauler")
        || g.contains("Transport")
        || g.contains("Barge")
        || g.contains("Exhumer")
    {
        "Industrial"
    } else if g.contains("Battleship") || g.contains("Marauder") || g.contains("Black Ops") {
        "Battleship"
    } else if g.contains("Battlecruiser") || g.contains("Command Ship") {
        "Battlecruiser"
    } else if g.contains("Cruiser") || g.contains("Recon") {
        "Cruiser"
    } else if g.contains("Destroyer") || g.contains("Interdictor") {
        "Destroyer"
    } else if g.contains("Frigate")
        || g.contains("Interceptor")
        || g.contains("Covert Ops")
        || g.contains("Bomber")
        || g.contains("Electronic Attack")
    {
        "Frigate"
    } else if g.contains("Shuttle") || g.contains("Corvette") {
        "Rookie"
    } else {
        ""
    }
}

pub(crate) fn ship_details_cached(
    store: &crate::store::Store,
    cache: &std::cell::RefCell<std::collections::HashMap<i64, Option<crate::store::ShipDetails>>>,
    id: i64,
) -> Option<crate::store::ShipDetails> {
    if let Some(d) = cache.borrow().get(&id) {
        return d.clone();
    }
    let d = store.ship_details(id);
    cache.borrow_mut().insert(id, d.clone());
    d
}

pub(crate) fn ship_roles_cached(
    store: &crate::store::Store,
    cache: &std::cell::RefCell<std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>>,
    id: i64,
) -> Vec<(&'static str, &'static str)> {
    if let Some(r) = cache.borrow().get(&id) {
        return r.clone();
    }
    let roles = derive_roles(&store.ship_traits(id));
    cache.borrow_mut().insert(id, roles.clone());
    roles
}

pub(crate) struct ShipLookup {
    store: crate::store::Store,
    details: std::cell::RefCell<std::collections::HashMap<i64, Option<crate::store::ShipDetails>>>,
    roles: std::cell::RefCell<std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>>,
}

impl ShipLookup {
    pub(crate) fn new(store: crate::store::Store) -> Self {
        Self {
            store,
            details: std::cell::RefCell::new(std::collections::HashMap::new()),
            roles: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    pub(crate) fn details(&self, id: i64) -> Option<crate::store::ShipDetails> {
        ship_details_cached(&self.store, &self.details, id)
    }

    pub(crate) fn roles(&self, id: i64) -> Vec<(&'static str, &'static str)> {
        ship_roles_cached(&self.store, &self.roles, id)
    }
}

pub(crate) fn derive_roles(traits: &[(i64, f64, String)]) -> Vec<(&'static str, &'static str)> {
    use egui_phosphor::regular as i;
    let t: String = traits.iter().map(|x| x.2.to_lowercase()).collect::<Vec<_>>().join(" | ");
    let has = |k: &str| t.contains(k);
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    if has("shield") {
        out.push((i::SHIELD, "Shield"));
    }
    if has("armor") {
        out.push((i::HARD_HAT, "Armor"));
    }
    if has("hybrid") || has("railgun") || has("blaster") {
        out.push((i::CROSSHAIR_SIMPLE, "Hybrid turrets"));
    }
    if has("laser") || has("energy turret") || has("beam") || has("pulse") {
        out.push((i::SUN, "Energy turrets"));
    }
    if has("projectile") || has("autocannon") || has("artillery") {
        out.push((i::CROSSHAIR, "Projectile turrets"));
    }
    if has("missile") || has("rocket") || has("torpedo") {
        out.push((i::ROCKET, "Missiles"));
    }
    if has("drone") {
        out.push((i::DRONE, "Drones"));
    }
    if has("neutralizer") || has("nosferatu") || has("energy vampire") || has("nos ") {
        out.push((i::LIGHTNING, "Energy neut / nos"));
    }
    if has("remote ") || has("logistics") {
        out.push((i::FIRST_AID, "Remote reps"));
    }
    if has("disrupt") || has("scrambl") || has("web") || has("stasis") || has("target paint")
        || has("dampen") || has("ecm") || has("jam") || has("tracking")
    {
        out.push((i::EYE_SLASH, "EWAR"));
    }
    out
}

fn role_badges(ui: &mut egui::Ui, roles: &[(&'static str, &'static str)]) {
    if roles.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (icon, label) in roles {
            ui.label(egui::RichText::new(*icon).size(18.0).color(ui.visuals().hyperlink_color))
                .on_hover_text(*label);
        }
    });
}

pub(crate) fn layer_ehp(hp: f64, r: [u32; 4]) -> f64 {
    if hp <= 0.0 {
        return 0.0;
    }
    let avg_resist = (r[0] + r[1] + r[2] + r[3]) as f64 / 4.0 / 100.0;
    hp / (1.0 - avg_resist).max(0.01)
}

fn ship_stats(ui: &mut egui::Ui, d: &crate::store::ShipDetails) {
    let dmg_col = [
        egui::Color32::from_rgb(0x5A, 0xA9, 0xE0),
        egui::Color32::from_rgb(0xD6, 0x45, 0x45),
        egui::Color32::from_rgb(0x9A, 0xA3, 0xA8),
        egui::Color32::from_rgb(0xD6, 0xA6, 0x45),
    ];
    let dmg_lbl = ["EM", "Th", "Kin", "Exp"];
    let layers = [
        ("Shield", d.shield_hp, d.shield_resist),
        ("Armor", d.armor_hp, d.armor_resist),
        ("Hull", d.hull_hp, d.hull_resist),
    ];

    egui::Grid::new("ship_resists").num_columns(7).spacing([10.0, 2.0]).show(ui, |ui| {
        ui.label("");
        ui.label(egui::RichText::new("HP").weak());
        for (i, lbl) in dmg_lbl.iter().enumerate() {
            ui.label(egui::RichText::new(*lbl).color(dmg_col[i]).strong());
        }
        ui.label(egui::RichText::new("EHP").strong());
        ui.end_row();
        for (name, hp, r) in layers {
            if hp <= 0.0 {
                continue;
            }
            ui.label(egui::RichText::new(name).strong());
            ui.label(format!("{hp:.0}"));
            for i in 0..4 {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(42.0, 18.0), egui::Sense::hover());
                let frac = (r[i] as f32 / 100.0).clamp(0.0, 1.0);
                let painter = ui.painter();
                painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
                let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, rect.height()));
                painter.rect_filled(bar, 2.0, dmg_col[i].gamma_multiply(0.6));
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}%", r[i]),
                    egui::FontId::proportional(12.5),
                    egui::Color32::WHITE,
                );
            }
            ui.label(format!("{:.0}", layer_ehp(hp, r)));
            ui.end_row();
        }
    });
    let total = layer_ehp(d.shield_hp, d.shield_resist)
        + layer_ehp(d.armor_hp, d.armor_resist)
        + layer_ehp(d.hull_hp, d.hull_resist);
    ui.label(egui::RichText::new(format!("Total EHP {total:.0}")).strong());

    ui.separator();
    let mut hp = Vec::new();
    if d.turret_hardpoints > 0 {
        hp.push(format!("{} turret", d.turret_hardpoints));
    }
    if d.launcher_hardpoints > 0 {
        hp.push(format!("{} launcher", d.launcher_hardpoints));
    }
    if !hp.is_empty() {
        ui.label(format!("Hardpoints: {}", hp.join(" · ")));
    }
    ui.label(format!(
        "Slots: {} high · {} mid · {} low",
        d.high_slots, d.mid_slots, d.low_slots
    ));
    if d.drone_cap > 0.0 {
        ui.label(format!("Drones: {:.0} m³ / {:.0} Mbit", d.drone_cap, d.drone_bw));
    }
    ui.label(format!("Max velocity: {:.0} m/s", d.max_velocity));
    if d.warp_speed > 0.0 {
        ui.label(format!("Warp speed: {:.2} AU/s", d.warp_speed));
    }
}

fn system_hover(
    ui: &mut egui::Ui,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    s: &crate::intel::DetectedSystem,
    ly: &CardLy,
    notes: Option<&NoteTip>,
) {
    ui.horizontal(|ui| {
        ui.label(security_badge(s.security));
        ui.label(egui::RichText::new(&s.name).strong());
    });
    system_chips(ui, systems, status, s.id);
    if let Some((staging, you)) = ly.of(s.id) {
        if let Some(c) = staging {
            ui.label(ly_line(c, &format!("staging {}", ly.staging)));
        }
        if let Some(c) = you {
            ui.label(ly_line(c, &ly.you));
        }
    }
    if let Some(n) = notes {
        note_tip_ui(ui, n);
    }
}

fn ly_line(centi: u32, from: &str) -> String {
    format!("{}.{:02} ly from {from}", centi / 100, centi % 100)
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        fallback.to_owned()
    } else {
        v.to_owned()
    }
}

fn coalition_hash(name: &str) -> i64 {
    let mut h: u64 = 1469598103934665603;
    for b in name.to_lowercase().bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    h as i64
}

fn alliance_color(id: i64) -> egui::Color32 {
    let h = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    egui::Color32::from_rgb(
        0x50 | ((h >> 16) as u8 >> 1),
        0x50 | ((h >> 8) as u8 >> 1),
        0x50 | ((h) as u8 >> 1),
    )
}

pub(crate) fn name_color(name: &str) -> egui::Color32 {
    alliance_color(coalition_hash(name))
}

/// Activity counts (NPC kills especially) run into the thousands, and a four-digit number under a
/// map dot is unreadable, so anything three digits or longer is abbreviated.
fn compact_count(v: u32) -> String {
    if v < 100 {
        v.to_string()
    } else {
        format!("{:.1}k", v as f32 / 1000.0)
    }
}

fn camp_color(level: crate::camp::CampLevel) -> egui::Color32 {
    match level {
        crate::camp::CampLevel::Likely => egui::Color32::from_rgb(0xEF, 0x44, 0x44),
        crate::camp::CampLevel::Possible => egui::Color32::from_rgb(0xFF, 0xA7, 0x26),
        crate::camp::CampLevel::Flag => egui::Color32::from_rgb(0xFF, 0xD5, 0x4F),
    }
}

fn activity_color(v: u32, scale: f32) -> egui::Color32 {
    let heat = (v as f32 / scale).min(1.0);
    egui::Color32::from_rgb(0xFF, (0xC0 as f32 * (1.0 - heat)) as u8, 0x30)
}

pub(crate) fn security_color(security: f64) -> egui::Color32 {
    const COLORS: [(u8, u8, u8); 11] = [
        (0xB0, 0x3A, 0x9A),
        (0xD7, 0x30, 0x00),
        (0xF0, 0x48, 0x00),
        (0xF0, 0x60, 0x00),
        (0xD7, 0x77, 0x00),
        (0xEF, 0xEF, 0x00),
        (0x8F, 0xEF, 0x2F),
        (0x00, 0xF0, 0x00),
        (0x00, 0xEF, 0x47),
        (0x48, 0xF0, 0xC0),
        (0x2F, 0xEF, 0xEF),
    ];
    let idx = (security * 10.0).round().clamp(0.0, 10.0) as usize;
    let (r, g, b) = COLORS[idx];
    egui::Color32::from_rgb(r, g, b)
}

fn security_badge(security: f64) -> egui::RichText {
    let sec = (security * 10.0).round() / 10.0;
    egui::RichText::new(format!("{sec:.1}"))
        .color(security_color(security))
        .monospace()
}

fn dir_picker_row(ui: &mut egui::Ui, hint: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        // Right-to-left so the button reserves its width first and the field claims whatever
        // is left, instead of a fixed width that clips long EVE paths.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(format!("{}  Browse…", egui_phosphor::regular::FOLDER_OPEN))
                .clicked()
            {
                let mut dialog = rfd::FileDialog::new();
                let start = if value.is_empty() { hint } else { value.as_str() };
                if std::path::Path::new(start).is_dir() {
                    dialog = dialog.set_directory(start);
                }
                if let Some(path) = dialog.pick_folder() {
                    *value = path.to_string_lossy().into_owned();
                    changed = true;
                }
            }
            let width = ui.available_width();
            changed |= ui
                .add(
                    egui::TextEdit::singleline(value)
                        .desired_width(width)
                        .hint_text(hint),
                )
                .changed();
        });
    });
    changed
}

fn color_row(ui: &mut egui::Ui, label: &str, rgb: &mut Rgb) -> bool {
    let mut arr = rgb.array();
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.color_edit_button_srgb(&mut arr).changed() {
            *rgb = Rgb::from_array(arr);
            changed = true;
        }
        ui.label(label);
    });
    changed
}
