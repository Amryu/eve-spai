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
enum IntelTypeFilter {
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
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum MapMode {
    #[default]
    Standard,
    Travel,
    Hunting,
    Safety,
    JumpPlan,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PasteKind {
    Dscan,
    Local,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum RouteKind {
    #[default]
    Travel,
    Jump,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum RouteView {
    #[default]
    ByType,
    ByName,
    BySystem,
}

#[derive(Clone)]
struct RouteItem {
    kind: RouteKind,
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
            MapMode::JumpPlan => "Jump Plan",
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
                MapMode::Standard | MapMode::JumpPlan => ActivityMode::Off,
                _ => ActivityMode::ShipKills,
            },
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum IntelClick {
    System(i64),
    Ship(i64),
    Pilot(String),
    Dscan(String),
    PilotVerdict(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RightDockTab {
    Mode,
    System,
}

#[derive(Default)]
struct SystemInfoOut {
    nav: Option<i64>,
    show_on_map: bool,
    intel_click: Option<IntelClick>,
    open_const: Option<i64>,
    open_region: Option<i64>,
}

#[derive(Clone, Copy, PartialEq)]
enum PilotSort {
    MostLost,
    Recent,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum PilotTab {
    #[default]
    Overview,
    Kills,
    Solo,
    Losses,
}

#[derive(Clone, Copy, PartialEq)]
enum FitMode {
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
    settings: Settings,
    view: View,
    intel_channels_open: bool,
    jump_bridges_open: bool,
    jb_paste: String,
    sov_upgrades_open: bool,
    sov_paste: String,
    coalitions_open: bool,
    severity_open: bool,
    coal_edit: Vec<(String, String)>,
    alliance_add: String,
    active_character: String,
    needs_save: bool,
    sde_status: SharedStatus,
    auth_status: SharedAuth,
    characters: Vec<CharacterRow>,
    intel_state: std::sync::Arc<std::sync::Mutex<crate::intel::IntelState>>,
    watcher_started: bool,
    chat_dir: Option<std::path::PathBuf>,
    intel_query: String,
    intel_max_jumps: u32,
    intel_type: IntelTypeFilter,
    battles: crate::zkill::SharedBattles,
    battle_history: crate::zkill::SharedBattles,
    battle_history_loading: std::sync::Arc<std::sync::atomic::AtomicBool>,
    show_history: bool,
    battle_selected: Option<i64>,
    battle_detail_cache: Option<std::sync::Arc<crate::brview::BattleDetail>>,
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
    battle_filter_open: bool,
    filter_picker: Option<crate::pickers::FilterPicker>,
    verdict_popup: Option<String>,
    verdict_explainer_open: bool,
    filter_add_result: std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>,
    battle_filter_confirm_reset: bool,
    battle_filter_gen: u64,
    battle_overrides: crate::zkill::SharedOverrides,
    battle_break_shared: std::sync::Arc<std::sync::atomic::AtomicI64>,
    battle_overrides_gen: u64,
    battle_overrides_gen_shared: std::sync::Arc<std::sync::atomic::AtomicU64>,
    battle_add_queue: std::sync::Arc<std::sync::Mutex<Vec<i64>>>,
    battle_excluded_count: usize,
    battle_scrub_count: usize,
    battle_edit_mode: bool,
    battle_kill_sel: std::collections::HashSet<i64>,
    battle_split_preview:
        Option<(std::collections::HashSet<i64>, crate::battle::Battle, crate::battle::Battle)>,
    battle_merge_sel: std::collections::HashSet<i64>,
    battle_add_open: bool,
    battle_add_link: String,
    battle_excluded_open: bool,
    battle_scrubs_open: bool,
    br_inputs: std::sync::Arc<std::sync::Mutex<crate::brview::BrInputs>>,
    br_outputs: std::sync::Arc<std::sync::Mutex<crate::brview::BrOutputs>>,
    br_wake: crate::brview::Wake,
    br_last_sent_sig: u64,
    battle_filter_gen_shared: std::sync::Arc<std::sync::atomic::AtomicU64>,
    // UI-side snapshots of the worker output, re-cloned only when its signature changes (never
    // per frame), so scrolling/rendering never clones the battle list or the open battle.
    battle_cards: Vec<(i64, Option<u32>, crate::battle::Battle)>,
    battle_cards_total: usize,
    battle_cards_filtered: usize,
    battle_cards_ready: bool,
    battle_cards_out_sig: u64,
    battle_detail_out_sig: u64,
    camps: crate::camp::SharedCamps,
    camped_cache: Vec<(i64, crate::camp::CampLevel)>,
    camped_cache_at: i64,
    killfeed: crate::zkill::SharedKillFeed,
    ship_by_id: std::collections::HashMap<i64, String>,
    kills_loaded: bool,
    player: crate::esi::SharedPlayer,
    systems: Option<std::sync::Arc<crate::geo::Systems>>,
    bridges_applied: Vec<crate::settings::JumpBridge>,
    system_status: crate::systemstatus::SharedStatus,
    alerts_engine: std::sync::Arc<AlertEngine>,
    recent_alerts: crate::gamewatcher::AlertLog,
    alert_feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    alert_rules_open: bool,
    alert_selected_rule: Option<u64>,
    rule_feeds:
        std::collections::HashMap<u64, Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)>>,
    alert_shared: SharedAlertWindow,
    alert_viewport_cb: std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    os_notify: std::sync::Arc<std::sync::atomic::AtomicBool>,
    proc_monitor: crate::procstat::Monitor,
    jabber: crate::jabber::SharedJabber,
    jabber_tx: Option<crate::jabber::CmdSender>,
    jabber_popped: bool,
    jabber_chat: Option<String>,
    jabber_tabs: Vec<String>,
    jabber_join_open: bool,
    jabber_close_room_prompt: Option<String>,
    jabber_drafts: std::collections::HashMap<String, String>,
    jabber_room_input: String,
    jabber_contact_search: String,
    jabber_dm_input: String,
    jabber_dm_error: String,
    jabber_show_directory: bool,
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
    session_start: i64,
    eve_focused: std::sync::Arc<std::sync::atomic::AtomicBool>,
    eve_focus_checked: Option<std::time::Instant>,
    ship_index: Option<std::sync::Arc<std::collections::HashMap<String, (i64, String)>>>,
    update: crate::update::SharedUpdate,
    update_checked_at: Option<std::time::Instant>,
    update_dismissed: bool,
    /// Set when the database can't be opened or written; drives a one-time warning.
    store_error: Option<String>,
    store_warn_dismissed: bool,
    kill_cache: crate::kills::KillCache,
    kill_tx: Option<crate::kills::KillSender>,
    lookup_input: String,
    lookup_tabs: Vec<String>,
    lookup_active: usize,
    lookup_cache: crate::charlookup::LookupCache,
    lookup_tx: Option<crate::charlookup::LookupSender>,
    intel_heights: std::collections::HashMap<u64, f32>,
    wizard_open: bool,
    wizard_step: u8,
    wizard_checked: bool,
    /// Result of the wizard's create-shortcut action: None = not tried, Some(Ok) = done.
    wizard_shortcut: Option<Result<(), String>>,
    tray: Option<crate::tray::TrayCmd>,
    really_exit: bool,
    raise_reset_top: bool,
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
    wh_cache: Vec<crate::wormholes::Wormhole>,
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
    routes_dialog_open: bool,
    route_save_name: String,
    route_save_folder: String,
    route_search: String,
    route_new_folder: String,
    route_kind: RouteKind,
    route_view: RouteView,
    route_edit: Option<(RouteKind, String, String)>,
    route_edit_name: String,
    route_edit_folder: String,
    travel_avoid: Vec<i64>,
    travel_avoid_sov: std::collections::HashSet<String>,
    travel_sov_dialog_open: bool,
    travel_route: Option<Vec<i64>>,
    ctx_menu_system: Option<i64>,
    jump_plan_from: Option<i64>,
    jump_plan_to: Option<i64>,
    jump_ship: usize,
    jump_jdc: u32,
    jump_jfc: u32,
    jump_skills: crate::esi::SharedJumpSkills,
    jump_waypoints: Vec<i64>,
    jump_favourites: std::collections::HashSet<i64>,
    jump_systems: Option<std::sync::Arc<Vec<crate::store::MapSystem>>>,
    jump_legs: Vec<crate::jumproute::Leg>,
    jump_route: Vec<i64>,
    jump_alt: Vec<i64>,
    jump_route_err: Option<String>,
    jump_route_key: Option<u64>,
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
    feed_cache: std::collections::HashMap<String, crate::lookup::SharedLookup>,
    pilot_window_open: bool,
    pilot_sort: PilotSort,
    pilot_tab: PilotTab,
    fit_view: Option<(i64, FitMode)>,
    fit_loss: Option<crate::lookup::Loss>,
    ping_shared: SharedPingWindow,
    ping_viewport_cb: std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    pilots: crate::pilot::SharedPilots,
    affiliations: crate::affiliation::SharedAffil,
    #[allow(dead_code)]
    activity: crate::activity::SharedActivity,
    sightings: crate::intel::SharedSightings,
    revivals: crate::watcher::SharedRevivals,
    ship_cache: std::cell::RefCell<std::collections::HashMap<i64, Option<crate::store::ShipDetails>>>,
    ship_roles_cache: std::cell::RefCell<std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>>,
    type_names: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, String>>>,
    type_names_loading: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl SpaiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);

        crate::image_cache::install_image_loaders_cached(&cc.egui_ctx);

        crate::instance::start_control_listener(cc.egui_ctx.clone());

        let (store, store_error) = match Store::open() {
            Ok(s) => match s.write_probe() {
                Ok(()) => (Some(s), None),
                // Opened read-only (file perms), so reads work but nothing persists.
                Err(e) => (Some(s), Some(format!("the database is not writable ({e})"))),
            },
            Err(e) => {
                eprintln!("store: {e:#}");
                (None, Some(format!("{e:#}")))
            }
        };
        let mut settings = store
            .as_ref()
            .and_then(|s| s.load_settings())
            .unwrap_or_default();

        settings.theme.apply(&cc.egui_ctx);

        let combat_on = settings.alert_combat;
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
        crate::wormholes::spawn_scout(cc.egui_ctx.clone());
        if let Some(store) = &store {
            if matches!(*sde_status.lock().unwrap(), SdeStatus::NotReady) {
                sde::spawn_download(store.path().to_path_buf(), sde_status.clone(), cc.egui_ctx.clone());
            }
        }

        let characters = store
            .as_ref()
            .map(|s| s.list_characters())
            .unwrap_or_default();

        let player: crate::esi::SharedPlayer =
            std::sync::Arc::new(std::sync::Mutex::new(crate::esi::Player::default()));
        if let Some(store) = &store {
            let _ = store;
            let cid = non_empty_or(&settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
            crate::esi::spawn_location_poller(cid, player.clone(), cc.egui_ctx.clone());
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
            ..Default::default()
        }));

        let kill_cache: crate::kills::KillCache =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let kill_tx = Some(crate::kills::spawn_fetcher(kill_cache.clone(), cc.egui_ctx.clone()));
        let lookup_cache: crate::charlookup::LookupCache = Default::default();
        let lookup_tx =
            Some(crate::charlookup::spawn_fetcher(lookup_cache.clone(), cc.egui_ctx.clone()));

        let activity: crate::activity::SharedActivity = {
            let mut c = crate::activity::ActivityCache::default();
            if let Some(s) = &store {
                c.preload(s.pilot_activity());
            }
            std::sync::Arc::new(std::sync::Mutex::new(c))
        };
        crate::activity::spawn(activity.clone(), cc.egui_ctx.clone());
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
        let jump_favourites: std::collections::HashSet<i64> =
            settings.jump_favourites.iter().copied().collect();

        let intel_state =
            std::sync::Arc::new(std::sync::Mutex::new(crate::intel::IntelState::default()));
        let pilots: crate::pilot::SharedPilots =
            std::sync::Arc::new(std::sync::Mutex::new(crate::pilot::PilotCache::default()));
        crate::pilot::spawn_resolver(pilots.clone(), cc.egui_ctx.clone());
        let killfeed: crate::zkill::SharedKillFeed =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recent_alerts: crate::gamewatcher::AlertLog =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let alert_shared: SharedAlertWindow =
            std::sync::Arc::new(std::sync::Mutex::new(AlertWindowState::default()));
        let overlay_stdin: std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let alerts_engine = std::sync::Arc::new(AlertEngine::new(
            recent_alerts.clone(),
            chrono::Utc::now().timestamp(),
            alert_shared.clone(),
            cc.egui_ctx.clone(),
            overlay_stdin.clone(),
        ));
        let system_status: crate::systemstatus::SharedStatus =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        crate::systemstatus::spawn(system_status.clone(), cc.egui_ctx.clone());
        let affiliations = std::sync::Arc::new(std::sync::Mutex::new(
            crate::affiliation::AffilCache::default(),
        ));
        crate::affiliation::spawn(affiliations.clone(), cc.egui_ctx.clone());
        let ping_shared: SharedPingWindow = std::sync::Arc::new(std::sync::Mutex::new(
            PingWindowState { enabled: settings.fleet_ping_window, ..Default::default() },
        ));
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
            cc.egui_ctx.clone(),
        );

        let eve_focused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let ping_viewport_cb = build_ping_viewport_cb(ping_shared.clone());
        let alert_viewport_cb = build_alert_viewport_cb(alert_shared.clone());

        {
            let ctx = cc.egui_ctx.clone();
            let ping_shared = ping_shared.clone();
            let alert_shared = alert_shared.clone();
            std::thread::spawn(move || loop {
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

        Self {
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
            active_character: "No character".to_owned(),
            needs_save: false,
            sde_status,
            auth_status: std::sync::Arc::new(std::sync::Mutex::new(AuthStatus::Idle)),
            characters,
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
            battle_filter_gen: 0,
            battle_overrides: std::sync::Arc::new(std::sync::Mutex::new(crate::battle::Overrides::default())),
            battle_break_shared: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(crate::battle::BATTLE_BREAK_SECS)),
            battle_overrides_gen: 0,
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
            br_wake: std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new())),
            br_last_sent_sig: 0,
            battle_filter_gen_shared: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            battle_cards: Vec::new(),
            battle_cards_total: 0,
            battle_cards_filtered: 0,
            battle_cards_ready: false,
            battle_cards_out_sig: u64::MAX,
            battle_detail_out_sig: u64::MAX,
            camps: std::sync::Arc::new(std::sync::Mutex::new(crate::camp::CampState::default())),
            camped_cache: Vec::new(),
            camped_cache_at: 0,
            killfeed,
            ship_by_id: std::collections::HashMap::new(),
            kills_loaded: false,
            player,
            systems: None,
            bridges_applied: Vec::new(),
            system_status,
            alerts_engine,
            recent_alerts,
            alert_feed: Vec::new(),
            alert_rules_open: false,
            alert_selected_rule: None,
            rule_feeds: std::collections::HashMap::new(),
            alert_shared,
            alert_viewport_cb,
            os_notify: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(combat_on)),
            proc_monitor: crate::procstat::Monitor::new(),
            jabber,
            jabber_tx: None,
            jabber_popped: false,
            jabber_chat: None,
            jabber_tabs: Vec::new(),
            jabber_join_open: false,
            jabber_close_room_prompt: None,
            jabber_drafts: std::collections::HashMap::new(),
            jabber_room_input: String::new(),
            jabber_contact_search: String::new(),
            jabber_dm_input: String::new(),
            jabber_dm_error: String::new(),
            jabber_show_directory: true,
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
            kill_cache,
            kill_tx,
            lookup_input: String::new(),
            lookup_tabs: Vec::new(),
            lookup_active: 0,
            lookup_cache,
            lookup_tx,
            intel_heights: std::collections::HashMap::new(),
            wizard_open: false,
            wizard_step: 0,
            wizard_shortcut: None,
            wizard_checked: false,
            tray: crate::tray::spawn(cc.egui_ctx.clone()),
            really_exit: false,
            raise_reset_top: false,
            overlay: match crate::ipc::OverlayLink::start(cc.egui_ctx.clone(), overlay_stdin) {
                Ok(link) => Some(link),
                Err(e) => {
                    eprintln!("[main] overlay failed to start (in-process fallback): {e}");
                    None
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
            route_kind: RouteKind::Travel,
            route_view: RouteView::ByType,
            route_edit: None,
            route_edit_name: String::new(),
            route_edit_folder: String::new(),
            travel_avoid: Vec::new(),
            travel_avoid_sov: std::collections::HashSet::new(),
            travel_sov_dialog_open: false,
            travel_route: None,
            ctx_menu_system: None,
            jump_plan_from: None,
            jump_plan_to: None,
            jump_ship: 0,
            jump_jdc: 5,
            jump_jfc: 5,
            jump_skills: std::sync::Arc::new(std::sync::Mutex::new(None)),
            jump_waypoints: Vec::new(),
            jump_favourites,
            jump_systems: None,
            jump_legs: Vec::new(),
            jump_route: Vec::new(),
            jump_alt: Vec::new(),
            jump_route_err: None,
            jump_route_key: None,
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
            feed_cache: std::collections::HashMap::new(),
            pilot_window_open: false,
            pilot_sort: PilotSort::MostLost,
            pilot_tab: PilotTab::default(),
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
        }
    }

    fn open_system(&mut self, system_id: i64) {
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
            cfg.kill_intel = self.settings.kill_intel;
            cfg.kill_intel_jumps = self.settings.kill_intel_jumps;
            cfg.intel_max_jumps = self.intel_max_jumps;
        }
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
            if self.settings.kill_intel {
                ui.label("within");
                if ui
                    .add(
                        egui::DragValue::new(&mut self.settings.kill_intel_jumps)
                            .range(0..=20)
                            .custom_formatter(|n, _| if n == 0.0 { "feed".to_owned() } else { format!("{n}j") }),
                    )
                    .changed()
                {
                    self.needs_save = true;
                }
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
        if self.alert_feed.is_empty() {
            ui.label(egui::RichText::new("None yet.").weak());
            return;
        }
        let mut feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
            self.alert_feed.iter().rev().take(60).cloned().collect();
        {
            let mut cache = self.pilots.lock().unwrap_or_else(|e| e.into_inner());
            for (r, _) in feed.iter_mut() {
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
            feed.iter().flat_map(|(r, _)| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
            ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp = cache
                .display_ids(feed.iter().flat_map(|(r, _)| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let status = self.system_status.lock().unwrap().clone();
        let last_ship = build_last_ship(&self.intel_state.lock().unwrap().reports);
        let systems = self.systems.clone();
        let player_sys = self.player_system();
        let now = chrono::Utc::now().timestamp();
        let mut click: Option<IntelClick> = None;
        for (r, sev) in &feed {
            let from_you = jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id));
            let kc = self.kill_cache.clone();
            let affil = self.affiliations.clone();
            if let Some(c) = intel_row(
                ui, r, now, false, from_you, &systems, &status, &ship_details, &ship_roles,
                &resolved_pilots, &uncertain, &last_ship, &kc, *sev, true,
            &affil, false, &mut None,
            ) {
                click = Some(c);
            }
        }
        self.apply_intel_click(click, ui);
    }

    fn apply_intel_click(&mut self, click: Option<IntelClick>, ui: &mut egui::Ui) {
        match click {
            Some(IntelClick::System(id)) => self.open_system(id),
            Some(IntelClick::Ship(id)) => self.open_ship(id),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(
                    self.pilot_query.clone(),
                    self.pilot_lookup.clone(),
                    ui.ctx().clone(),
                );
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            Some(IntelClick::Dscan(url)) => self.open_dscan(url, ui.ctx()),
            Some(IntelClick::PilotVerdict(name)) => self.open_pilot_verdict(name),
            None => {}
        }
    }

    /// Render one rule's "Recent matches" feed. Mirrors `alert_history_ui` but sources from
    /// `rule_feeds` and tags each card as allowed or suppressed.
    fn rule_feed_ui(&mut self, ui: &mut egui::Ui, rule_id: u64) {
        let entries: Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)> = self
            .rule_feeds
            .get(&rule_id)
            .map(|f| f.iter().rev().take(60).cloned().collect())
            .unwrap_or_default();
        if entries.is_empty() {
            ui.label(egui::RichText::new("None yet.").weak());
            return;
        }
        let mut feed: Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)> = entries;
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
        let now = chrono::Utc::now().timestamp();
        let mut click: Option<IntelClick> = None;
        for (r, sev, suppressed) in &feed {
            if *suppressed {
                ui.label(
                    egui::RichText::new(format!("{}  suppressed", egui_phosphor::regular::BELL_SLASH))
                        .color(crate::theme::standing::NEUTRAL),
                );
            }
            let from_you = jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id));
            let kc = self.kill_cache.clone();
            let affil = self.affiliations.clone();
            if let Some(c) = intel_row(
                ui, r, now, false, from_you, &systems, &status, &ship_details, &ship_roles,
                &resolved_pilots, &uncertain, &last_ship, &kc, *sev, true, &affil,
            false, &mut None,
            ) {
                click = Some(c);
            }
        }
        self.apply_intel_click(click, ui);
    }

    fn maybe_start_jabber(&mut self, ctx: &egui::Context) {
        // A terminal failure (bad credentials, unreachable) drops back to the login form and stops
        // auto-restarting; the reason stays in state until the user retries.
        if self.jabber.lock().unwrap().fatal.is_some() && self.settings.jabber_enabled {
            self.settings.jabber_enabled = false;
            self.needs_save = true;
        }
        let enabled = self.settings.jabber_enabled && !self.settings.jabber_jid.trim().is_empty();
        {
            let mut s = self.jabber.lock().unwrap();
            s.enabled = enabled;
            if s.running || !enabled {
                return;
            }
        }
        let Some(systems) = self.systems.clone() else { return };
        let jid = self.settings.jabber_jid.trim().to_owned();
        let Some(pw) = crate::jabber::load_password(&jid) else { return };
        let resolve: crate::jabber::Resolver = std::sync::Arc::new(move |t: &str| {
            systems.lookup(t).or_else(|| systems.lookup_prefix(t)).map(|i| i.id)
        });
        let server = self.settings.jabber_server.clone();
        let rooms = self.settings.jabber_rooms.clone();
        self.jabber_tx = Some(crate::jabber::spawn(
            jid,
            pw,
            server,
            rooms,
            resolve,
            self.jabber.clone(),
            self.ping_shared.clone(),
            ctx.clone(),
        ));
    }

    fn full_room_jid(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains('@') {
            return input.to_owned();
        }
        let domain = if !self.settings.jabber_muc_domain.trim().is_empty() {
            self.settings.jabber_muc_domain.trim().to_owned()
        } else {
            let jid_domain = self.settings.jabber_jid.split('@').nth(1).unwrap_or("");
            format!("conference.{jid_domain}")
        };
        format!("{input}@{domain}")
    }

    /// What counts as being named in a room: the Jabber username, plus whatever the user added.
    fn mention_names(&self) -> Vec<String> {
        let node = self.settings.jabber_jid.split('@').next().unwrap_or_default().trim();
        std::iter::once(node.to_owned())
            .chain(self.settings.jabber_mention_keywords.iter().cloned())
            .filter(|n| !n.trim().is_empty())
            .collect()
    }

    fn jabber_mark_read(&self, jid: &str) {
        let mut st = self.jabber.lock().unwrap();
        st.unread.remove(jid);
        st.mentions.remove(jid);
    }

    fn jabber_is_muted(&self, key: &str) -> bool {
        self.settings
            .jabber_muted
            .get(key)
            .is_some_and(|&until| until == i64::MAX || chrono::Utc::now().timestamp() < until)
    }

    fn jabber_has_unread(&self) -> bool {
        let st = self.jabber.lock().unwrap();
        if st.pings_unread && !self.jabber_is_muted(crate::jabber::PING_FEED_KEY) {
            return true;
        }
        st.unread.iter().any(|k| !self.jabber_is_muted(k))
    }

    fn cache_op_links(&mut self, pings: &[crate::pings::Ping]) {
        use crate::pings::{Comms, Ping};
        let mut changed = false;
        for p in pings {
            if let Ping::Fleet { comms: Some(Comms::Mumble { channel, link }), .. } = p {
                if let Some(k) = op_key(channel) {
                    if self.settings.op_channel_links.get(&k) != Some(link) {
                        self.settings.op_channel_links.insert(k, link.clone());
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.needs_save = true;
        }
    }

    fn poll_jabber_notify(&mut self, ctx: &egui::Context) {
        let events: Vec<(String, bool)> = {
            let mut st = self.jabber.lock().unwrap();
            st.notify_cfg.sound_enabled = self.settings.jabber_sound_enabled;
            st.notify_cfg.ping_sound = self.settings.jabber_ping_sound.clone();
            st.notify_cfg.msg_sound = self.settings.jabber_msg_sound.clone();
            st.notify_cfg.mention_sound = self.settings.jabber_mention_sound.clone();
            st.notify_cfg.ping_volume = self.settings.jabber_ping_volume;
            st.notify_cfg.msg_volume = self.settings.jabber_msg_volume;
            st.notify_cfg.mention_volume = self.settings.jabber_mention_volume;
            st.notify_cfg.mention_names = self.mention_names();
            st.notify_cfg.mention_ignores_mute = self.settings.jabber_mention_ignores_mute;
            st.notify_cfg.ping_rules = self.settings.jabber_ping_rules.clone();
            st.notify_cfg.muted = self.settings.jabber_muted.clone();
            std::mem::take(&mut st.notify)
        };
        if events.is_empty() {
            return;
        }
        let recent: Vec<crate::pings::Ping> =
            { self.jabber.lock().unwrap().pings.iter().rev().take(10).cloned().collect() };
        self.cache_op_links(&recent);
        let mut any = false;
        for (key, is_ping) in events {
            if self.jabber_is_muted(&key) {
                continue;
            }
            let suppress = if is_ping {
                let latest = self.jabber.lock().unwrap().pings.last().cloned();
                match latest.as_ref().and_then(|p| self.matching_ping_rule(p)) {
                    Some(r) => r.suppress,
                    None => false,
                }
            } else {
                false
            };
            if suppress {
                continue;
            }
            any = true;
        }
        if any && !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }

    fn matching_ping_rule(&self, p: &crate::pings::Ping) -> Option<&crate::settings::PingRule> {
        crate::pings::match_ping_rule(&self.settings.jabber_ping_rules, p)
    }

    fn full_user_jid(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains('@') {
            return input.to_owned();
        }
        let domain = self.settings.jabber_jid.split('@').nth(1).unwrap_or("");
        format!("{input}@{domain}")
    }

    fn ping_rules_dialog(&mut self, ctx: &egui::Context) {
        if !self.ping_rules_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "jabber_alerts_window",
            "EVE Spai - Jabber alerts",
            [540.0, 620.0],
            |ui| {
              egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("snd").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    changed |= ui
                        .checkbox(&mut self.settings.jabber_sound_enabled, "Notification sounds")
                        .changed();
                    ui.end_row();
                    let msg_vol = self.settings.jabber_msg_volume;
                    let ping_vol = self.settings.jabber_ping_volume;
                    let mention_vol = self.settings.jabber_mention_volume;
                    ui.label("Message sound");
                    changed |= sound_picker(ui, "jabber_msg", false, &mut self.settings.jabber_msg_sound, msg_vol);
                    ui.end_row();
                    ui.label("Message volume");
                    changed |= volume_slider(ui, &mut self.settings.jabber_msg_volume);
                    ui.end_row();
                    ui.label("Default ping sound");
                    changed |= sound_picker(ui, "jabber_ping", false, &mut self.settings.jabber_ping_sound, ping_vol);
                    ui.end_row();
                    ui.label("Fleet ping volume");
                    changed |= volume_slider(ui, &mut self.settings.jabber_ping_volume);
                    ui.end_row();
                    ui.label("Mention sound");
                    changed |= sound_picker(ui, "jabber_mention", false, &mut self.settings.jabber_mention_sound, mention_vol);
                    ui.end_row();
                    ui.label("Mention volume");
                    changed |= volume_slider(ui, &mut self.settings.jabber_mention_volume);
                    ui.end_row();
                    ui.label("");
                    ui.label(egui::RichText::new("presets: horn · chime · beep · sweep · info · warning · danger · critical · off, or a file path").weak().small());
                    ui.end_row();
                    ui.label("Mention words");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.mention_input)
                                .hint_text("extra words, comma separated"),
                        )
                        .changed()
                    {
                        self.settings.jabber_mention_keywords = self
                            .mention_input
                            .split(',')
                            .map(str::trim)
                            .filter(|w| !w.is_empty())
                            .map(str::to_owned)
                            .collect();
                        changed = true;
                    }
                    ui.end_row();
                    ui.label("");
                    let me = self.settings.jabber_jid.split('@').next().unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "your name \"{me}\" always counts as a mention"
                        ))
                        .weak(),
                    );
                    ui.end_row();
                    ui.label("");
                    changed |= ui
                        .checkbox(
                            &mut self.settings.jabber_mention_ignores_mute,
                            "Mentions notify even in muted chats",
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Doctrine link");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.doctrine_url)
                                .hint_text("URL or file:/// path shown on fleet pings"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Fleet ping window");
                    changed |= ui
                        .checkbox(
                            &mut self.settings.fleet_ping_window,
                            "Pop a focused window on fleet pings",
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Keep on top");
                    ui.horizontal(|ui| {
                        use crate::settings::OnTop;
                        changed |= ui
                            .selectable_value(&mut self.settings.fleet_ping_on_top, OnTop::Always, "Always")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.settings.fleet_ping_on_top, OnTop::Smart, "When EVE focused")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.settings.fleet_ping_on_top, OnTop::Never, "Never")
                            .changed();
                    });
                    ui.end_row();
                    ui.label("Ping bot JID");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.jabber_ping_bot)
                                .hint_text("directorbot@…"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Closing a room tab");
                    ui.horizontal(|ui| {
                        let v = &mut self.settings.jabber_close_room_leaves;
                        changed |= ui.selectable_value(v, None, "Ask").changed();
                        changed |= ui.selectable_value(v, Some(true), "Leave room").changed();
                        changed |= ui.selectable_value(v, Some(false), "Keep joined").changed();
                    });
                    ui.end_row();
                });
                ui.separator();
                ui.label(
                    egui::RichText::new("Fleet-ping rules. A match plays its sound and highlights the ping.")
                        .weak(),
                );
                let mut remove: Option<usize> = None;
                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;
                let mut edit: Option<usize> = None;
                let n = self.settings.jabber_ping_rules.len();
                use egui_phosphor::regular as ic;
                for (i, r) in self.settings.jabber_ping_rules.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut r.enabled, "").changed();
                            let nm = if r.name.is_empty() { "(unnamed rule)" } else { &r.name };
                            let txt = if r.enabled {
                                egui::RichText::new(nm).strong()
                            } else {
                                egui::RichText::new(nm).weak().strikethrough()
                            };
                            if ui
                                .add(egui::Label::new(txt).sense(egui::Sense::click()))
                                .on_hover_text("Edit rule")
                                .clicked()
                            {
                                edit = Some(i);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(ic::X).on_hover_text("Delete").clicked() {
                                    remove = Some(i);
                                }
                                if i + 1 < n && ui.button(ic::ARROW_DOWN).on_hover_text("Move down").clicked() {
                                    move_down = Some(i);
                                }
                                if i > 0 && ui.button(ic::ARROW_UP).on_hover_text("Move up").clicked() {
                                    move_up = Some(i);
                                }
                                if ui.button(ic::PENCIL_SIMPLE).on_hover_text("Edit rule").clicked() {
                                    edit = Some(i);
                                }
                            });
                        });
                    });
                }
                // A structural change invalidates the editor's index; close it rather than risk
                // editing the wrong rule.
                if let Some(i) = remove {
                    self.settings.jabber_ping_rules.remove(i);
                    self.ping_rule_editing = None;
                    changed = true;
                }
                if let Some(i) = move_up {
                    self.settings.jabber_ping_rules.swap(i, i - 1);
                    self.ping_rule_editing = None;
                    changed = true;
                }
                if let Some(i) = move_down {
                    self.settings.jabber_ping_rules.swap(i, i + 1);
                    self.ping_rule_editing = None;
                    changed = true;
                }
                if let Some(i) = edit {
                    self.ping_rule_editing = Some(i);
                }
                ui.separator();
                if ui.button("+ Add rule").clicked() {
                    self.settings.jabber_ping_rules.push(crate::settings::PingRule::default());
                    self.ping_rule_editing = Some(self.settings.jabber_ping_rules.len() - 1);
                    changed = true;
                }
              });
            },
        );
        if !keep {
            self.ping_rules_open = false;
            self.ping_rule_editing = None;
        }
        if changed {
            self.needs_save = true;
        }
        self.ping_rule_editor(ctx);
    }

    /// Config dialog for a single fleet-ping rule, opened on top of the Jabber alerts window. Only
    /// one is open at a time (`ping_rule_editing`).
    fn ping_rule_editor(&mut self, ctx: &egui::Context) {
        let Some(i) = self.ping_rule_editing else { return };
        if i >= self.settings.jabber_ping_rules.len() {
            self.ping_rule_editing = None;
            return;
        }
        let mut changed = false;
        let global_ping_vol = self.settings.jabber_ping_volume;
        let keep = Self::dialog_viewport(
            ctx,
            "ping_rule_editor",
            "EVE Spai - Fleet ping rule",
            [420.0, 460.0],
            |ui| {
              // Scope the rule borrow so the "Done" button below can touch `self.ping_rule_editing`.
              {
                let r = &mut self.settings.jabber_ping_rules[i];
                ui.horizontal(|ui| {
                    ui.label("Name");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut r.name).desired_width(240.0))
                        .changed();
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Match on (blank = any). A ping must match every filled field.")
                        .weak(),
                );
                egui::Grid::new("rule").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    let wide = 250.0;
                    ui.label("FC");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.fc).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("PAP type");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.pap).hint_text("any  (strategic / peacetime)").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("Doctrine");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.doctrine).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("Form-up");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.formup).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("Keyword");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.keyword).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    changed |= ui
                        .checkbox(&mut r.suppress, "Suppress")
                        .on_hover_text("Ignore matching pings: no sound, no highlight, no push")
                        .changed();
                    if r.suppress {
                        r.notify = false;
                        r.push = false;
                    }
                    ui.add_enabled_ui(!r.suppress, |ui| {
                        changed |= ui.checkbox(&mut r.notify, "Notify").changed();
                        changed |= ui.checkbox(&mut r.push, "Push").changed();
                    });
                });
                ui.add_enabled_ui(!r.suppress && r.notify, |ui| {
                    let eff_vol = r.volume.unwrap_or(global_ping_vol);
                    ui.horizontal(|ui| {
                        ui.label("Sound");
                        changed |= sound_picker(ui, ("ping_rule", i), true, &mut r.sound, eff_vol);
                    });
                    ui.horizontal(|ui| {
                        let mut custom = r.volume.is_some();
                        if ui
                            .checkbox(&mut custom, "Custom volume")
                            .on_hover_text("Override the global fleet-ping volume for this rule")
                            .changed()
                        {
                            r.volume = if custom { Some(global_ping_vol) } else { None };
                            changed = true;
                        }
                        if let Some(v) = r.volume.as_mut() {
                            changed |= volume_slider(ui, v);
                        }
                    });
                });
              }
                ui.add_space(8.0);
                ui.separator();
                if ui.button("Done").clicked() {
                    self.ping_rule_editing = None;
                }
            },
        );
        if !keep {
            self.ping_rule_editing = None;
        }
        if changed {
            self.needs_save = true;
        }
    }

    fn poll_kill_fetches(&self) {
        let Some(tx) = &self.kill_tx else { return };
        let mut to_fetch: Vec<i64> = Vec::new();
        {
            let cache = self.kill_cache.lock().unwrap();
            let st = self.intel_state.lock().unwrap();
            for r in &st.reports {
                for l in &r.links {
                    if l.kind == crate::intel::LinkKind::Killmail {
                        if let Some(id) = l.kill_id {
                            if !cache.contains_key(&id) {
                                to_fetch.push(id);
                            }
                        }
                    }
                }
            }
        }
        for id in to_fetch {
            self.kill_cache.lock().unwrap().entry(id).or_insert(None);
            let _ = tx.send(id);
        }
    }

    fn remove_jabber_tab(&mut self, jid: &str) {
        if let Some(idx) = self.jabber_tabs.iter().position(|t| t == jid) {
            self.jabber_tabs.remove(idx);
            if self.jabber_chat.as_deref() == Some(jid) {
                // Focus the left neighbour, falling back to the Fleet pings tab.
                self.jabber_chat = if idx > 0 { self.jabber_tabs.get(idx - 1).cloned() } else { None };
            }
        }
    }

    fn close_jabber_tab(&mut self, jid: &str, is_room: bool) {
        if is_room {
            match self.settings.jabber_close_room_leaves {
                None => {
                    // First time: ask, then re-run with the saved choice.
                    self.jabber_close_room_prompt = Some(jid.to_owned());
                    return;
                }
                Some(true) => {
                    if let Some(tx) = &self.jabber_tx {
                        let _ = tx.send(crate::jabber::Cmd::LeaveRoom { room: jid.to_owned() });
                    }
                    self.settings.jabber_rooms.retain(|r| r != jid);
                }
                Some(false) => {
                    if !self.settings.jabber_closed_rooms.iter().any(|r| r == jid) {
                        self.settings.jabber_closed_rooms.push(jid.to_owned());
                    }
                }
            }
        } else if !self.settings.jabber_closed_dms.iter().any(|d| d == jid) {
            self.settings.jabber_closed_dms.push(jid.to_owned());
        }
        self.needs_save = true;
        self.remove_jabber_tab(jid);
    }

    fn jabber_join_dialog(&mut self, ctx: &egui::Context, convos: &[Convo]) {
        if !self.jabber_join_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new(format!(
            "{}  Join conversation",
            egui_phosphor::regular::CHAT_CIRCLE_DOTS
        ))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            // Size fields to a constant, NOT ui.available_width(): this window auto-fits its content
            // (resizable=false), so a field derived from available_width feeds the window width back
            // into itself and the dialog creeps wider every frame.
            const DIALOG_W: f32 = 320.0;
            const FIELD_W: f32 = DIALOG_W - 70.0;
            ui.set_min_width(DIALOG_W);
            ui.label(egui::RichText::new("Join room").strong());
            let room_go = ui
                .horizontal(|ui| {
                    let resp = ui.add_sized(
                        [FIELD_W, 22.0],
                        egui::TextEdit::singleline(&mut self.jabber_room_input)
                            .hint_text("room@conference.…"),
                    );
                    let enter =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.button("Join").clicked() || enter
                })
                .inner;
            if room_go && !self.jabber_room_input.trim().is_empty() {
                let room = self.full_room_jid(&self.jabber_room_input);
                self.jabber_room_input.clear();
                if let Some(tx) = &self.jabber_tx {
                    let _ = tx.send(crate::jabber::Cmd::JoinRoom { room: room.clone() });
                }
                if !self.settings.jabber_rooms.contains(&room) {
                    self.settings.jabber_rooms.push(room.clone());
                }
                self.settings.jabber_closed_rooms.retain(|r| r != &room);
                self.needs_save = true;
                if !self.jabber_tabs.iter().any(|t| t == &room) {
                    self.jabber_tabs.push(room.clone());
                }
                self.jabber_chat = Some(room);
                close = true;
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Message someone").strong());
            let dm_go = ui
                .horizontal(|ui| {
                    let resp = ui.add_sized(
                        [FIELD_W, 22.0],
                        egui::TextEdit::singleline(&mut self.jabber_dm_input)
                            .hint_text("Message someone…"),
                    );
                    let enter =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.button("Open").clicked() || enter
                })
                .inner;
            if dm_go && !self.jabber_dm_input.trim().is_empty() {
                let input = self.jabber_dm_input.trim().to_owned();
                let resolved = if input.contains('@') {
                    Some(input.clone())
                } else if let Some(c) = convos.iter().find(|c| {
                    c.name.eq_ignore_ascii_case(&input)
                        || c.jid
                            .split('@')
                            .next()
                            .is_some_and(|l| l.eq_ignore_ascii_case(&input))
                }) {
                    Some(c.jid.clone())
                } else if !input.contains(' ') {
                    Some(self.full_user_jid(&input))
                } else {
                    None
                };
                match resolved {
                    Some(jid) => {
                        self.jabber_dm_input.clear();
                        self.jabber_dm_error.clear();
                        self.settings.jabber_closed_dms.retain(|j| j != &jid);
                        self.needs_save = true;
                        self.jabber_mark_read(&jid);
                        if !self.jabber_tabs.iter().any(|t| t == &jid) {
                            self.jabber_tabs.push(jid.clone());
                        }
                        self.jabber_chat = Some(jid);
                        close = true;
                    }
                    None => {
                        self.jabber_dm_error = format!("No contact matching \"{input}\"");
                    }
                }
            }
            if !self.jabber_dm_error.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&self.jabber_dm_error)
                        .color(crate::theme::standing::WARNING),
                );
            }
        });
        if close || !open {
            self.jabber_join_open = false;
            self.jabber_dm_error.clear();
        }
    }

    fn jabber_close_room_dialog(&mut self, ctx: &egui::Context) {
        let Some(jid) = self.jabber_close_room_prompt.clone() else {
            return;
        };
        let name = jid.split('@').next().unwrap_or(&jid).to_owned();
        let mut open = true;
        let mut dismiss = false;
        egui::Window::new("Close room tab")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.label(format!("Closing the tab for \"{name}\"."));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Leave the room, or keep it joined and just hide the tab? You can change this later in the Jabber alerts window.",
                    )
                    .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Leave room").clicked() {
                        self.settings.jabber_close_room_leaves = Some(true);
                        self.close_jabber_tab(&jid, true);
                        dismiss = true;
                    }
                    if ui.button("Just hide tab").clicked() {
                        self.settings.jabber_close_room_leaves = Some(false);
                        self.close_jabber_tab(&jid, true);
                        dismiss = true;
                    }
                });
            });
        if dismiss || !open {
            self.jabber_close_room_prompt = None;
        }
    }

    fn jabber_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        if self.jabber_popped {
            ui.label(egui::RichText::new("Jabber is open in a separate window.").weak());
            return;
        }
        self.jabber_ui(ui);
    }

    fn jabber_ui(&mut self, ui: &mut egui::Ui) {
        let (fatal_set, ever_online) = {
            let s = self.jabber.lock().unwrap();
            (s.fatal.is_some(), s.ever_online)
        };
        let configured = self.settings.jabber_enabled
            && !self.settings.jabber_jid.trim().is_empty()
            && crate::jabber::has_password(self.settings.jabber_jid.trim())
            && !fatal_set;
        if !configured {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Connect to your alliance Jabber (XMPP) for chat and fleet pings.",
                )
                .weak(),
            );
            ui.label(egui::RichText::new("Imperium: jabber-server.goonfleet.com").weak());
            ui.add_space(6.0);
            egui::Grid::new("jabber_login").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("JID");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.jabber_jid)
                        .hint_text("MyCharacter@goonfleet.com")
                        .desired_width(260.0),
                );
                ui.end_row();
                ui.label("Server");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.jabber_server)
                        .hint_text("jabber-server.goonfleet.com")
                        .desired_width(260.0),
                )
                .on_hover_text("XMPP server host (the JID domain usually has no SRV record)");
                ui.end_row();
                ui.label("Password");
                let pw_hint = if crate::jabber::has_password(self.settings.jabber_jid.trim()) {
                    "<saved password>"
                } else {
                    ""
                };
                ui.add(
                    egui::TextEdit::singleline(&mut self.jabber_pw_input)
                        .password(true)
                        .hint_text(pw_hint)
                        .desired_width(260.0),
                );
                ui.end_row();
            });
            if ui.button("Connect").clicked() {
                let jid = self.settings.jabber_jid.trim().to_owned();
                if let Some(err) = crate::jabber::jid_format_error(&jid) {
                    let mut s = self.jabber.lock().unwrap();
                    s.fatal = Some(err.clone());
                    s.status = err;
                } else {
                    // Save a freshly typed password, otherwise reuse the stored one (e.g. retrying
                    // after a network error without re-typing it).
                    let ready = if !self.jabber_pw_input.is_empty() {
                        match crate::jabber::save_password(&jid, &self.jabber_pw_input) {
                            Ok(()) => {
                                self.jabber_pw_input.clear();
                                true
                            }
                            Err(e) => {
                                self.jabber.lock().unwrap().status = format!("Keychain error: {e}");
                                false
                            }
                        }
                    } else {
                        crate::jabber::has_password(&jid)
                    };
                    if ready {
                        let mut s = self.jabber.lock().unwrap();
                        s.fatal = None;
                        s.status = "Connecting…".to_owned();
                        drop(s);
                        self.settings.jabber_enabled = true;
                        self.needs_save = true;
                    } else {
                        self.jabber.lock().unwrap().status = "Enter your password".to_owned();
                    }
                }
            }
            let (status, fatal) = {
                let s = self.jabber.lock().unwrap();
                (s.status.clone(), s.fatal.is_some())
            };
            if !status.is_empty() {
                ui.add_space(4.0);
                let txt = egui::RichText::new(status);
                ui.label(if fatal { txt.color(crate::theme::standing::HOSTILE) } else { txt.weak() });
            }
            return;
        }

        if !ever_online {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(28.0));
                ui.add_space(10.0);
                let status = self.jabber.lock().unwrap().status.clone();
                let txt = if status.is_empty() { "Connecting…".to_owned() } else { status };
                ui.label(egui::RichText::new(txt).weak());
                ui.add_space(10.0);
                if ui.button("Cancel").clicked() {
                    self.settings.jabber_enabled = false;
                    self.needs_save = true;
                    self.jabber.lock().unwrap().status.clear();
                }
            });
            return;
        }

        let (connected, status, convos, sel_msgs, pings, rooms, dm_keys, unread, mentions, pings_unread) = {
            let st = self.jabber.lock().unwrap();
            let mut set: std::collections::BTreeMap<String, Convo> =
                std::collections::BTreeMap::new();
            for (jid, c) in &st.roster {
                set.entry(jid.clone()).or_insert_with(|| Convo {
                    jid: jid.clone(),
                    name: c.name.clone().unwrap_or_else(|| jid.split('@').next().unwrap_or(jid).to_owned()),
                    unread: false,
                    group: c.groups.first().cloned().unwrap_or_else(|| "Other".to_owned()),
                    presence: c.presence,
                    status_text: c.status_text.clone(),
                });
            }
            for jid in st.chats.keys() {
                let pres = st.presences.get(jid).map(|(p, _)| *p).unwrap_or_default();
                set.entry(jid.clone()).or_insert_with(|| Convo {
                    jid: jid.clone(),
                    name: jid.split('@').next().unwrap_or(jid).to_owned(),
                    unread: false,
                    group: "Other".to_owned(),
                    presence: pres,
                    status_text: String::new(),
                });
            }
            for jid in &st.unread {
                if let Some(e) = set.get_mut(jid) {
                    e.unread = true;
                }
            }
            let convos: Vec<Convo> = set.into_values().collect();
            let sel_msgs = self
                .jabber_chat
                .as_ref()
                .and_then(|j| st.chats.get(j))
                .cloned()
                .unwrap_or_default();
            let rooms: Vec<(String, bool)> =
                st.rooms.iter().map(|r| (r.clone(), st.unread.contains(r))).collect();
            let dm_keys: Vec<String> = st
                .chats
                .keys()
                .filter(|k| !st.rooms.contains(*k) && k.as_str() != crate::jabber::PING_FEED_KEY && valid_bare_jid(k))
                .cloned()
                .collect();
            let unread = st.unread.clone();
            let mentions = st.mentions.clone();
            (st.connected, st.status.clone(), convos, sel_msgs, st.pings.clone(), rooms, dm_keys, unread, mentions, st.pings_unread)
        };

        // Reconcile the open-conversation tabs from joined rooms + DM history. An incoming
        // message (present in `unread`) reopens a conversation whose tab was closed.
        {
            let mut save = false;
            for k in &unread {
                if let Some(p) = self.settings.jabber_closed_dms.iter().position(|j| j == k) {
                    self.settings.jabber_closed_dms.remove(p);
                    save = true;
                }
                if let Some(p) = self.settings.jabber_closed_rooms.iter().position(|j| j == k) {
                    self.settings.jabber_closed_rooms.remove(p);
                    save = true;
                }
            }
            let closed_dms: std::collections::HashSet<String> =
                self.settings.jabber_closed_dms.iter().cloned().collect();
            let closed_rooms: std::collections::HashSet<String> =
                self.settings.jabber_closed_rooms.iter().cloned().collect();
            let room_set: std::collections::HashSet<String> =
                rooms.iter().map(|(r, _)| r.clone()).collect();
            for (rjid, _) in &rooms {
                // A room we were put into by the server (bookmark, invite, force-join) is only known
                // to this session; persist it so we rejoin it ourselves next time.
                if !self.settings.jabber_rooms.iter().any(|r| r == rjid) {
                    self.settings.jabber_rooms.push(rjid.clone());
                    save = true;
                }
                if !closed_rooms.contains(rjid) && !self.jabber_tabs.iter().any(|t| t == rjid) {
                    self.jabber_tabs.push(rjid.clone());
                }
            }
            for k in &dm_keys {
                if !closed_dms.contains(k) && !self.jabber_tabs.iter().any(|t| t == k) {
                    self.jabber_tabs.push(k.clone());
                }
            }
            self.jabber_tabs.retain(|t| {
                if room_set.contains(t) {
                    !closed_rooms.contains(t)
                } else {
                    !closed_dms.contains(t)
                }
            });
            if let Some(jid) = self.jabber_chat.clone() {
                if !room_set.contains(&jid) && !self.jabber_tabs.iter().any(|t| t == &jid) {
                    self.jabber_chat = None;
                }
            }
            if save {
                self.needs_save = true;
            }
        }

        let mut presence_changed = false;
        ui.horizontal(|ui| {
            if connected {
                use crate::jabber::Presence;
                let (r, g, b) = self.jabber_my_presence.color();
                status_dot(ui, egui::Color32::from_rgb(r, g, b), 10.0);
                ui.label(egui::RichText::new(&self.settings.jabber_jid).weak());
                egui::ComboBox::from_id_salt("my_presence")
                    .selected_text(self.jabber_my_presence.label())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for p in [Presence::Online, Presence::Away, Presence::Xa, Presence::Dnd] {
                            if ui
                                .selectable_value(&mut self.jabber_my_presence, p, p.label())
                                .clicked()
                            {
                                presence_changed = true;
                            }
                        }
                    });
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.jabber_my_status)
                        .hint_text("status message")
                        .desired_width(150.0),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    presence_changed = true;
                }
            } else {
                status_dot(ui, crate::theme::standing::WARNING, 10.0);
                ui.label(egui::RichText::new(status.as_str()).weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Disconnect").clicked() {
                    self.settings.jabber_enabled = false;
                    self.needs_save = true;
                }
                if !self.jabber_popped && ui.button("Pop out").clicked() {
                    self.jabber_popped = true;
                }
                if ui
                    .button(egui_phosphor::regular::BELL_RINGING)
                    .on_hover_text("Ping alert rules")
                    .clicked()
                {
                    self.mention_input = self.settings.jabber_mention_keywords.join(", ");
                    self.ping_rules_open = true;
                }
            });
        });
        if presence_changed {
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::SetPresence {
                    show: self.jabber_my_presence,
                    status: self.jabber_my_status.clone(),
                });
            }
        }
        ui.separator();

        let systems = self.systems.clone();
        egui::Panel::left("jabber_split")
            .resizable(true)
            .default_size(210.0)
            .size_range(150.0..=460.0)
            .show_inside(ui, |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 24.0],
                        egui::Button::new(format!(
                            "{}  Join conversation",
                            egui_phosphor::regular::CHAT_CIRCLE_DOTS
                        )),
                    )
                    .on_hover_text("Join a room or start a direct message")
                    .clicked()
                {
                    self.jabber_dm_error.clear();
                    self.jabber_join_open = true;
                }
                ui.separator();
                let contacts: std::collections::HashSet<String> =
                    self.settings.jabber_contacts.iter().cloned().collect();
                let dir_unread = convos.iter().any(|c| c.unread);
                let con_unread = convos.iter().any(|c| c.unread && contacts.contains(&c.jid));
                ui.horizontal(|ui| {
                    let dir = selectable_chip(ui, self.jabber_show_directory, "Directory");
                    if dir_unread {
                        ui.scope(|ui| {
                            ui.label(egui::RichText::new(egui_phosphor::regular::CIRCLE).color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C)).size(8.0));
                        });
                    }
                    if dir.clicked() {
                        self.jabber_show_directory = true;
                    }
                    let con = selectable_chip(ui, !self.jabber_show_directory, "Contacts");
                    if con_unread {
                        ui.label(egui::RichText::new(egui_phosphor::regular::CIRCLE).color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C)).size(8.0));
                    }
                    if con.clicked() {
                        self.jabber_show_directory = false;
                    }
                });
                ui.add_sized(
                    [ui.available_width(), 20.0],
                    egui::TextEdit::singleline(&mut self.jabber_contact_search).hint_text("Search"),
                );
                let search = self.jabber_contact_search.to_lowercase();
                let show_dir = self.jabber_show_directory;
                let shown: Vec<&Convo> = convos
                    .iter()
                    .filter(|c| show_dir || contacts.contains(&c.jid))
                    .filter(|c| {
                        search.is_empty()
                            || c.name.to_lowercase().contains(&search)
                            || c.jid.to_lowercase().contains(&search)
                    })
                    .collect();
                let mut groups: std::collections::BTreeMap<&str, Vec<&Convo>> =
                    std::collections::BTreeMap::new();
                for c in shown {
                    let g = if c.group.trim().is_empty() { "Other" } else { c.group.as_str() };
                    groups.entry(g).or_default().push(c);
                }
                let accent = ui.visuals().hyperlink_color;
                let mut toggle_contact: Option<(String, bool)> = None;
                egui::ScrollArea::vertical().id_salt("convos").auto_shrink([false, false]).show(ui, |ui| {
                    // Roster rows are list items, not chips: a border here is too heavy and would pop
                    // in on hover. Keep the fill highlight, drop the stroke, so nothing shifts.
                    let w = &mut ui.visuals_mut().widgets;
                    w.inactive.bg_stroke = egui::Stroke::NONE;
                    w.hovered.bg_stroke = egui::Stroke::NONE;
                    w.active.bg_stroke = egui::Stroke::NONE;
                    if groups.is_empty() && !show_dir {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("No contacts yet. Add people from the Directory.").weak());
                    }
                    for (group, mut members) in groups {
                        members.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        let online = members.iter().filter(|c| c.presence.online()).count();
                        ui.add_space(7.0);
                        let collapsed = self.jabber_collapsed.contains(group);
                        let grp_unread = members.iter().any(|c| c.unread);
                        let hdr = ui
                            .horizontal(|ui| {
                                let caret = if collapsed {
                                    egui_phosphor::regular::CARET_RIGHT
                                } else {
                                    egui_phosphor::regular::CARET_DOWN
                                };
                                let gname = truncate_to(group, fit_chars(ui.available_width() - 40.0));
                                let r = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("{caret}  {gname}"))
                                            .strong()
                                            .size(15.0)
                                            .color(accent),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{online}/{}", members.len())).weak(),
                                );
                                if collapsed && grp_unread {
                                    ui.label(
                                        egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                            .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                            .size(8.0),
                                    );
                                }
                                r
                            })
                            .inner;
                        if hdr.clicked() {
                            if collapsed {
                                self.jabber_collapsed.remove(group);
                            } else {
                                self.jabber_collapsed.insert(group.to_owned());
                            }
                        }
                        if collapsed {
                            continue;
                        }
                        for c in members {
                            let sel = self.jabber_chat.as_deref() == Some(c.jid.as_str());
                            let (r, g, b) = c.presence.color();
                            let disp = truncate_to(
                                &c.name,
                                fit_chars(ui.available_width() - 34.0 - if c.unread { 16.0 } else { 0.0 }),
                            );
                            let name = if c.unread {
                                egui::RichText::new(disp).strong()
                            } else if c.presence.online() {
                                egui::RichText::new(disp)
                            } else {
                                egui::RichText::new(disp).weak()
                            };
                            let is_contact = contacts.contains(&c.jid);
                            let resp = ui.horizontal(|ui| {
                                status_dot(ui, egui::Color32::from_rgb(r, g, b), 9.0);
                                let clicked = ui.selectable_label(sel, name)
                                    .on_hover_text(&c.name)
                                    .clicked();
                                if c.unread {
                                    ui.label(
                                        egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                            .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                            .size(8.0),
                                    );
                                }
                                let star_col = if is_contact {
                                    ui.visuals().hyperlink_color
                                } else {
                                    ui.visuals().weak_text_color()
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(egui_phosphor::regular::STAR)
                                                .small()
                                                .color(star_col),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text(if is_contact { "Remove from contacts" } else { "Add to contacts" })
                                    .clicked()
                                {
                                    toggle_contact = Some((c.jid.clone(), !is_contact));
                                }
                                clicked
                            });
                            let tip = if c.status_text.is_empty() {
                                c.presence.label().to_owned()
                            } else {
                                format!("{} — {}", c.presence.label(), c.status_text)
                            };
                            resp.response.on_hover_text(tip);
                            if resp.inner {
                                self.settings.jabber_closed_dms.retain(|j| j != &c.jid);
                                if !self.jabber_tabs.iter().any(|t| t == &c.jid) {
                                    self.jabber_tabs.push(c.jid.clone());
                                }
                                self.jabber_chat = Some(c.jid.clone());
                                self.jabber_mark_read(&c.jid);
                            }
                        }
                    }
                });
                if let Some((jid, add)) = toggle_contact {
                    if add {
                        if !self.settings.jabber_contacts.contains(&jid) {
                            self.settings.jabber_contacts.push(jid);
                        }
                    } else {
                        self.settings.jabber_contacts.retain(|j| j != &jid);
                    }
                    self.needs_save = true;
                }
            });
        egui::Panel::top("jabber_tab_bar")
            .frame(egui::Frame::new().fill(ui.visuals().panel_fill))
            .show_inside(ui, |ui| {
                // Tab strip: Fleet pings (static, left-most) then one tab per open conversation.
                let mut focus: Option<Option<String>> = None;
                let mut close_tab: Option<(String, bool)> = None;
                // Set when a not-currently-visible tab is picked from the dropdown, so it is moved to
                // the front of the bar (right after Fleet pings) and the rightmost tab overflows.
                let mut promote: Option<String> = None;
                // One non-scrolling row: Fleet pings (pinned) + as many chat tabs as fit, the rest in
                // a right-side overflow dropdown. Widths are estimated from the label galley so we can
                // decide inclusion without a horizontal scroll area.
                struct TabInfo {
                    jid: String,
                    is_room: bool,
                    is_unread: bool,
                    is_mention: bool,
                    lead: TabLead,
                    label: String,
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // The overflow dropdown is a fixed-width pseudo-tab pinned to the right edge;
                    // tabs fill the space to its left. A boundary tab is ellipsized (rather than
                    // dropped) when it only partly fits, so the row is packed and the dropdown
                    // never moves.
                    const MIN_TAB_W: f32 = 76.0;
                    let full = ui.available_width();
                    // Reserve the dropdown's exact width for its widest state (caret + the largest
                    // possible overflow count) so the badge never grows the button past the edge.
                    let dd_w = {
                        let body = egui::TextStyle::Body.resolve(ui.style());
                        let widest = format!(
                            "{}  {}",
                            egui_phosphor::regular::CARET_DOWN,
                            self.jabber_tabs.len().max(1)
                        );
                        let text_w = ui
                            .painter()
                            .layout_no_wrap(widest, body, egui::Color32::WHITE)
                            .size()
                            .x;
                        text_w + 2.0 * ui.spacing().button_padding.x + 4.0
                    };
                    let tab_area = (full - dd_w).max(0.0);

                    let pings_label = format!("Fleet pings ({})", pings.len());
                    let (sel, _) = jabber_tab_box(
                        ui,
                        self.jabber_chat.is_none(),
                        pings_unread,
                        false,
                        TabLead::Icon(egui_phosphor::regular::MEGAPHONE),
                        false,
                        &pings_label,
                    );
                    if sel {
                        focus = Some(None);
                    }
                    let mut used = jabber_tab_width(ui, false, pings_unread, &pings_label);

                    let infos: Vec<TabInfo> = self
                        .jabber_tabs
                        .iter()
                        .map(|jid| {
                            let is_room = rooms.iter().any(|(r, _)| r == jid);
                            let is_unread = unread.contains(jid);
                            let label = short_chip(jid.split('@').next().unwrap_or(jid));
                            let lead = if is_room {
                                TabLead::Icon(egui_phosphor::regular::USERS_THREE)
                            } else {
                                let pres = convos
                                    .iter()
                                    .find(|c| &c.jid == jid)
                                    .map(|c| c.presence)
                                    .unwrap_or_default();
                                let (pr, pg, pb) = pres.color();
                                TabLead::Dot(egui::Color32::from_rgb(pr, pg, pb))
                            };
                            let is_mention = mentions.contains(jid);
                            TabInfo { jid: jid.clone(), is_room, is_unread, is_mention, lead, label }
                        })
                        .collect();

                    // Plan the visible tabs (with the label actually rendered, possibly ellipsized)
                    // and collect the rest into the dropdown.
                    let mut plan: Vec<(&TabInfo, String)> = Vec::new();
                    let mut overflow: Vec<&TabInfo> = Vec::new();
                    let mut full_bar = false;
                    for t in &infos {
                        if full_bar {
                            overflow.push(t);
                            continue;
                        }
                        let w = jabber_tab_width(ui, true, t.is_unread, &t.label);
                        let remaining = tab_area - used;
                        if w <= remaining {
                            used += w;
                            plan.push((t, t.label.clone()));
                        } else if remaining >= MIN_TAB_W {
                            let lbl = ellipsize_tab_label(ui, true, t.is_unread, &t.label, remaining);
                            used += jabber_tab_width(ui, true, t.is_unread, &lbl);
                            plan.push((t, lbl));
                            full_bar = true;
                        } else {
                            overflow.push(t);
                            full_bar = true;
                        }
                    }
                    // Guarantee the open conversation stays on the bar: evict trailing tabs until it
                    // fits, then place it (ellipsized if needed).
                    if let Some(sel_jid) = self.jabber_chat.clone() {
                        if !plan.iter().any(|(t, _)| t.jid == sel_jid) {
                            if let Some(pos) = overflow.iter().position(|t| t.jid == sel_jid) {
                                while tab_area - used < MIN_TAB_W && !plan.is_empty() {
                                    let (t, lbl) = plan.pop().unwrap();
                                    used -= jabber_tab_width(ui, true, t.is_unread, &lbl);
                                    overflow.insert(0, t);
                                }
                                let t = overflow.remove(pos.min(overflow.len().saturating_sub(1)));
                                let remaining = tab_area - used;
                                let lbl =
                                    ellipsize_tab_label(ui, true, t.is_unread, &t.label, remaining);
                                used += jabber_tab_width(ui, true, t.is_unread, &lbl);
                                plan.push((t, lbl));
                            }
                        }
                    }

                    for (t, lbl) in &plan {
                        let (sel, close) = jabber_tab_box(
                            ui,
                            self.jabber_chat.as_deref() == Some(t.jid.as_str()),
                            t.is_unread,
                            t.is_mention,
                            t.lead,
                            true,
                            lbl,
                        );
                        if sel {
                            focus = Some(Some(t.jid.clone()));
                        }
                        if close {
                            close_tab = Some((t.jid.clone(), t.is_room));
                        }
                    }

                    // Pin the dropdown pseudo-tab to the right edge (always shown, static position).
                    let pad = (full - used - dd_w).max(0.0);
                    if pad > 0.0 {
                        ui.add_space(pad);
                    }
                    let any_unread = overflow.iter().any(|t| t.is_unread);
                    let caret = if overflow.is_empty() {
                        egui_phosphor::regular::CARET_DOWN.to_owned()
                    } else {
                        format!("{} {}", egui_phosphor::regular::CARET_DOWN, overflow.len())
                    };
                    let caret = if any_unread {
                        egui::RichText::new(caret).strong()
                    } else {
                        egui::RichText::new(caret)
                    };
                    let dd_btn = egui::Button::new(caret)
                        .min_size(egui::vec2(dd_w, TAB_H))
                        .corner_radius(0.0);
                    let menu_list: Vec<&TabInfo> =
                        if overflow.is_empty() { infos.iter().collect() } else { overflow };
                    egui::containers::menu::MenuButton::from_button(dd_btn).ui(ui, |ui| {
                        for t in &menu_list {
                            ui.horizontal(|ui| {
                                match t.lead {
                                    TabLead::Dot(c) => status_dot(ui, c, 9.0),
                                    TabLead::Icon(ic) => {
                                        ui.label(ic);
                                    }
                                }
                                if ui.selectable_label(false, t.label.as_str()).clicked() {
                                    focus = Some(Some(t.jid.clone()));
                                    if !plan.iter().any(|(pt, _)| pt.jid == t.jid) {
                                        promote = Some(t.jid.clone());
                                    }
                                    ui.close();
                                }
                                if t.is_unread {
                                    ui.label(
                                        egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                            .color(UNREAD_RED)
                                            .size(8.0),
                                    );
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(egui_phosphor::regular::X).small(),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text("Close")
                                    .clicked()
                                {
                                    close_tab = Some((t.jid.clone(), t.is_room));
                                    ui.close();
                                }
                            });
                        }
                    });
                });
                if let Some(target) = focus {
                    match &target {
                        None => self.jabber.lock().unwrap().pings_unread = false,
                        Some(jid) => {
                            self.jabber_mark_read(jid);
                        }
                    }
                    self.jabber_chat = target;
                }
                if let Some((jid, is_room)) = close_tab {
                    self.close_jabber_tab(&jid, is_room);
                }
                if let Some(jid) = promote {
                    if let Some(pos) = self.jabber_tabs.iter().position(|t| t == &jid) {
                        let t = self.jabber_tabs.remove(pos);
                        self.jabber_tabs.insert(0, t);
                    }
                }
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
                match self.jabber_chat.clone() {
                    None => {
                        let hl: Vec<bool> =
                            pings.iter().map(|p| self.matching_ping_rule(p).is_some_and(|r| !r.suppress)).collect();
                        let doctrine_url = self.settings.doctrine_url.clone();
                        let op_links = self.settings.op_channel_links.clone();
                        let visible = self.jabber_pings_visible.min(pings.len());
                        let out = egui::ScrollArea::vertical().id_salt("pings").auto_shrink([false, false]).show(ui, |ui| {
                            if pings.is_empty() {
                                ui.label(egui::RichText::new("No pings yet.").weak());
                            }
                            for (i, p) in pings.iter().enumerate().rev().take(visible) {
                                render_ping(ui, p, &systems, hl[i], &doctrine_url, &op_links);
                            }
                            if visible < pings.len() {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(format!("+{} older", pings.len() - visible))
                                        .weak(),
                                );
                            }
                        });
                        // Load the next page once the user scrolls near the bottom of the shown set.
                        if visible < pings.len()
                            && out.state.offset.y + out.inner_rect.height()
                                >= out.content_size.y - 200.0
                        {
                            self.jabber_pings_visible = (visible + 50).min(pings.len());
                            ui.ctx().request_repaint();
                        }
                    }
                    Some(jid) => {
                        self.jabber_pings_visible = 50;
                        use egui_phosphor::regular as icon;
                        let is_room = rooms.iter().any(|(r, _)| r == &jid);
                        let muted = self.jabber_is_muted(&jid);
                        ui.horizontal(|ui| {
                            let name = jid.split('@').next().unwrap_or(&jid);
                            let glyph = if is_room { icon::USERS_THREE } else { icon::USER };
                            ui.label(egui::RichText::new(format!("{glyph}  {name}")).strong());
                            if muted {
                                ui.label(egui::RichText::new(icon::BELL_SLASH).weak())
                                    .on_hover_text("Muted");
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !is_room {
                                        let is_contact =
                                            self.settings.jabber_contacts.contains(&jid);
                                        let col = if is_contact {
                                            ui.visuals().hyperlink_color
                                        } else {
                                            ui.visuals().weak_text_color()
                                        };
                                        if ui
                                            .button(egui::RichText::new(icon::STAR).color(col))
                                            .on_hover_text(if is_contact {
                                                "Remove from contacts"
                                            } else {
                                                "Add as contact"
                                            })
                                            .clicked()
                                        {
                                            if is_contact {
                                                self.settings.jabber_contacts.retain(|j| j != &jid);
                                            } else {
                                                self.settings.jabber_contacts.push(jid.clone());
                                            }
                                            self.needs_save = true;
                                        }
                                    }
                                    let bell = if muted { icon::BELL_SLASH } else { icon::BELL };
                                    ui.menu_button(bell, |ui| {
                                        let now = chrono::Utc::now().timestamp();
                                        let set = |app: &mut Self, until: i64| {
                                            app.settings.jabber_muted.insert(jid.clone(), until);
                                            app.needs_save = true;
                                        };
                                        if ui.button("Mute 1 hour").clicked() {
                                            set(self, now + 3600);
                                            ui.close();
                                        }
                                        if ui.button("Mute 8 hours").clicked() {
                                            set(self, now + 8 * 3600);
                                            ui.close();
                                        }
                                        if ui.button("Mute until I unmute").clicked() {
                                            set(self, i64::MAX);
                                            ui.close();
                                        }
                                        if muted && ui.button("Unmute").clicked() {
                                            self.settings.jabber_muted.remove(&jid);
                                            self.needs_save = true;
                                            ui.close();
                                        }
                                    })
                                    .response
                                    .on_hover_text("Mute notifications");
                                },
                            );
                        });
                        ui.separator();
                        let composer_h = 32.0;
                        let session_start = self.session_start;
                        let mut dm_click: Option<String> = None;
                        let body_h = ui.available_height();
                        // Don't snap to the bottom while the pointer is held: that snap on
                        // every incoming message was wiping out any text selection mid-drag
                        // (the chat felt unselectable in a busy channel). It resumes on release.
                        let selecting = ui.input(|i| i.pointer.any_down());
                        egui::ScrollArea::vertical()
                            .id_salt("msgs")
                            .auto_shrink([false, false])
                            .max_height((body_h - composer_h - 8.0).max(60.0))
                            .stick_to_bottom(!selecting)
                            .show(ui, |ui| {
                                let accent = ui.visuals().hyperlink_color;
                                let me_col = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
                                let now = chrono::Utc::now().timestamp();
                                let names = self.mention_names();
                                ui.spacing_mut().item_spacing.y = 1.0;
                                let mut hist_drawn = false;
                                let mut prev_sender: Option<String> = None;
                                let mut prev_time: i64 = 0;
                                for m in &sel_msgs {
                                    if !hist_drawn && m.time >= session_start && m.time > 0 {
                                        hist_drawn = true;
                                        prev_sender = None;
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("— new —").weak().small());
                                            ui.separator();
                                        });
                                    }
                                    let sender =
                                        if m.outgoing { "\u{0}me".to_owned() } else { m.from.clone() };
                                    let grouped = prev_sender.as_deref() == Some(sender.as_str())
                                        && m.time >= prev_time
                                        && m.time - prev_time < 300;
                                    if !grouped {
                                        ui.add_space(5.0);
                                        ui.label(
                                            egui::RichText::new(eve_time_label(m.time, now))
                                                .weak()
                                                .size(9.5),
                                        );
                                    }
                                    let mut row = |ui: &mut egui::Ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            if !grouped {
                                                if m.outgoing {
                                                    ui.label(
                                                        egui::RichText::new("me:")
                                                            .color(me_col)
                                                            .strong(),
                                                    );
                                                } else {
                                                    let n =
                                                        m.from.split('@').next().unwrap_or(&m.from);
                                                    let lbl = egui::Label::new(
                                                        egui::RichText::new(format!("{n}:"))
                                                            .strong()
                                                            .color(accent),
                                                    );
                                                    let resp = if is_room {
                                                        ui.add(lbl.sense(egui::Sense::click()))
                                                            .on_hover_text("Message")
                                                    } else {
                                                        ui.add(lbl)
                                                    };
                                                    if resp.clicked() {
                                                        dm_click = Some(n.to_owned());
                                                    }
                                                }
                                            }
                                            render_message_body(ui, &m.body);
                                        });
                                    };
                                    let mentioned = !m.outgoing
                                        && crate::jabber::mention_hit(&m.body, &names);
                                    if mentioned {
                                        egui::Frame::new()
                                            .fill(MENTION_BG)
                                            .inner_margin(egui::Margin::symmetric(4, 2))
                                            .show(ui, &mut row);
                                    } else {
                                        row(ui);
                                    }
                                    prev_sender = Some(sender);
                                    prev_time = m.time;
                                }
                            });
                        ui.horizontal_top(|ui| {
                            let shift_enter = egui::KeyboardShortcut::new(
                                egui::Modifiers::SHIFT,
                                egui::Key::Enter,
                            );
                            let row_h = ui.text_style_height(&egui::TextStyle::Body);
                            let resp = egui::ScrollArea::vertical()
                                .id_salt("composer")
                                .max_height(row_h * 8.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(
                                            self.jabber_drafts.entry(jid.clone()).or_default(),
                                        )
                                        .hint_text("Message (Shift+Enter for a new line)")
                                        .return_key(shift_enter)
                                        .desired_rows(2)
                                        .desired_width(ui.available_width() - 60.0),
                                    )
                                })
                                .inner;
                            let send = resp.has_focus()
                                && ui.input(|i| {
                                    i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                                });
                            let draft_empty = self
                                .jabber_drafts
                                .get(&jid)
                                .map_or(true, |d| d.trim().is_empty());
                            if (ui.button("Send").clicked() || send) && !draft_empty {
                                let body = self
                                    .jabber_drafts
                                    .get_mut(&jid)
                                    .map(std::mem::take)
                                    .unwrap_or_default();
                                if let Some(tx) = &self.jabber_tx {
                                    let cmd = if is_room {
                                        crate::jabber::Cmd::SendRoom { room: jid.clone(), body }
                                    } else {
                                        crate::jabber::Cmd::Send { to: jid.clone(), body }
                                    };
                                    let _ = tx.send(cmd);
                                }
                            }
                        });
                        if let Some(nick) = dm_click {
                            let dm = self.full_user_jid(&nick);
                            self.jabber_mark_read(&dm);
                            self.settings.jabber_closed_dms.retain(|j| j != &dm);
                            if !self.jabber_tabs.iter().any(|t| t == &dm) {
                                self.jabber_tabs.push(dm.clone());
                            }
                            self.jabber_chat = Some(dm);
                        }
                    }
                }
            });
        self.jabber_join_dialog(ui.ctx(), &convos);
        self.jabber_close_room_dialog(ui.ctx());
    }

    #[allow(deprecated)]
    fn show_jabber_viewport(&mut self, ctx: &egui::Context) {
        let mut keep = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("jabber_window"),
            egui::ViewportBuilder::default().with_icon(app_icon()).with_title("EVE Spai - Jabber").with_inner_size([720.0, 560.0]),
            |ctx, _| {
                egui::CentralPanel::default().show(ctx, |ui| self.jabber_ui(ui));
                ontop_pin(ctx, "jabber_window");
                if ctx.input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            },
        );
        if !keep {
            self.jabber_popped = false;
        }
    }

    fn ensure_ship_by_id(&mut self) {
        if self.ship_by_id.is_empty() {
            if let Some(idx) = &self.ship_index {
                for (id, name) in idx.values() {
                    self.ship_by_id.insert(*id, name.clone());
                }
            }
        }
    }

    fn load_persisted_kills(&mut self) {
        if self.kills_loaded {
            return;
        }
        let Some(geo) = self.systems.clone() else { return };
        self.ensure_ship_by_id();
        if self.ship_by_id.is_empty() {
            return;
        }
        self.kills_loaded = true;
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - 3600;
        let (saved, details) = {
            let Some(store) = &self.store else { return };
            let rows = store.load_kill_intel(cutoff);
            store.prune_kill_intel(cutoff);
            let details = store.load_kill_details();
            (rows, details)
        };
        if !details.is_empty() {
            let mut c = self.kill_cache.lock().unwrap();
            for k in details {
                let id = k.kill_id;
                c.entry(id).or_insert(Some(k));
            }
        }
        if saved.is_empty() {
            return;
        }
        let mut reports = Vec::new();
        for (killmail_id, system_id, ship_type_id, time, value) in saved {
            let near_celestial = self
                .kill_cache
                .lock()
                .unwrap()
                .get(&killmail_id)
                .and_then(|o| o.as_ref())
                .and_then(|k| k.near_celestial.clone());
            let ev = crate::zkill::KillEvent {
                system_id,
                ship_type_id,
                time,
                value,
                killmail_id,
                info: crate::kills::KillInfo { near_celestial, ..Default::default() },
            };
            if let Some(report) = kill_report(&ev, &geo, &self.ship_by_id) {
                reports.push(report);
            }
        }
        let ids: Vec<u64> = {
            let mut st = self.intel_state.lock().unwrap();
            reports.into_iter().map(|report| st.push(report)).collect()
        };
        // These are historical kills, not live events — pre-mark them alerted so the recency
        // gate in the alert daemon doesn't pop them into the alert window at startup.
        let now = chrono::Utc::now().timestamp();
        {
            let mut rt = self.alerts_engine.runtime.lock().unwrap();
            for id in ids {
                rt.alerted.insert(id, now);
            }
        }
    }

    fn reload_wormholes(&mut self) {
        let due = self.wh_reloaded.map(|t| t.elapsed().as_millis() > 2000).unwrap_or(true);
        if !due {
            return;
        }
        self.wh_reloaded = Some(std::time::Instant::now());
        if let Some(store) = &self.store {
            let now = chrono::Utc::now().timestamp();
            store.prune_wormholes(now);
            let mut whs = store.wormholes();
            whs.retain(|w| !w.is_expired(now));
            whs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            self.wh_overlay = WhOverlay::build(&whs);
            self.wh_cache = whs;
        }
    }

    /// The live hole graph. Built from `wh_cache`, not `wh_overlay`: the overlay drops high-degree
    /// hubs to keep the map readable, which is exactly what would remove Thera from a route.
    fn wh_adjacency(&self) -> std::collections::HashMap<i64, Vec<i64>> {
        let mut adj: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        for w in &self.wh_cache {
            if let Some(b) = w.dest_system_id {
                adj.entry(w.system_id).or_default().push(b);
                adj.entry(b).or_default().push(w.system_id);
            }
        }
        adj
    }

    fn wh_route_waypoints(&self, from: i64, dest: i64) -> Option<Vec<i64>> {
        let geo = self.systems.as_ref()?;
        wh_route_waypoints(geo, &self.wh_adjacency(), from, dest)
    }

    /// The hole collapsed. Drop it, then re-route: any plan that was going through it is now wrong.
    fn kill_wormhole(&mut self, id: i64) {
        if let Some(store) = self.store.as_ref() {
            store.kill_wormhole(id);
        }
        self.wh_reloaded = None; // bypass the reload debounce, the map must not show it again
        self.reload_wormholes();
        self.replan_routes();
    }

    /// `crossed_jspace` covers the case where the step passed through systems the k-space map cannot
    /// place, which can only have happened through a hole.
    fn leg_kind(&self, a: i64, b: i64, crossed_jspace: bool) -> Leg {
        let Some(g) = self.systems.as_ref() else { return Leg::Gate };
        if crossed_jspace || g.is_hole_step(a, b) {
            Leg::Hole
        } else if g.is_bridge(a, b) {
            Leg::Bridge
        } else {
            Leg::Gate
        }
    }

    /// Re-run every route that could depend on the hole graph: the planned map route, and the
    /// destination we last pushed to the client.
    fn replan_routes(&mut self) {
        self.plan_route();
        if let Some(dest) = self.route_destination {
            if self.active_character != "No character" {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                self.set_destination_esi(cid, self.active_character.clone(), dest);
            }
        }
    }

    fn set_destination_esi(&self, cid: String, cname: String, dest: i64) {
        if self.settings.route_via_wormholes {
            let from = self.player_system();
            if let Some(from) = from {
                if let Some(wp) = self.wh_route_waypoints(from, dest) {
                    if wp.len() > 1 {
                        crate::esi::set_route(cid, cname, wp);
                        return;
                    }
                }
            }
        }
        crate::esi::set_waypoint(cid, cname, dest, true);
    }

    fn wormholes_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(format!("{}  Wormholes", egui_phosphor::regular::SPIRAL));
            ui.label(egui::RichText::new(format!("{} known", self.wh_cache.len())).weak());
        });
        ui.horizontal(|ui| {
            use crate::wormholes::{DestClass, Source};
            ui.label("Dest:");
            egui::ComboBox::from_id_salt("wh_dest_filter")
                .selected_text(self.wh_filter_dest.map_or("Any", |d| d.label()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.wh_filter_dest, None, "Any");
                    for d in [
                        DestClass::Highsec,
                        DestClass::Lowsec,
                        DestClass::Nullsec,
                        DestClass::Wspace,
                        DestClass::Thera,
                        DestClass::Turnur,
                        DestClass::Unknown,
                    ] {
                        ui.selectable_value(&mut self.wh_filter_dest, Some(d), d.label());
                    }
                });
            ui.label("Source:");
            egui::ComboBox::from_id_salt("wh_src_filter")
                .selected_text(self.wh_filter_source.map_or("Any", |s| s.label()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.wh_filter_source, None, "Any");
                    for s in [Source::EveScout, Source::Intel, Source::Manual] {
                        ui.selectable_value(&mut self.wh_filter_source, Some(s), s.label());
                    }
                });
            ui.checkbox(&mut self.wh_filter_expiring, "Expiring <4h");
        });
        ui.separator();
        if self.wh_cache.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No wormholes known yet.").weak());
                ui.label(
                    egui::RichText::new("Seeded from EVE-Scout (Thera/Turnur) and intel channels.")
                        .weak()
                        .small(),
                );
            });
            return;
        }

        let now = chrono::Utc::now().timestamp();
        struct Row {
            sys_id: i64,
            sys: String,
            wh_type: String,
            drifter: bool,
            dest: String,
            dest_click: Option<i64>,
            dest_const: String,
            dest_region: String,
            size: String,
            life: String,
            source: String,
        }
        let info_of = |id: i64| self.systems.as_ref().and_then(|s| s.info_of(id)).cloned();
        let (fd, fs, fe) = (self.wh_filter_dest, self.wh_filter_source, self.wh_filter_expiring);
        let rows: Vec<Row> = self
            .wh_cache
            .iter()
            .filter(|w| {
                fd.map_or(true, |d| w.dest == d)
                    && fs.map_or(true, |s| w.source == s)
                    && (!fe || w.hours_left(now).is_some_and(|h| h <= 4))
            })
            .map(|w| {
                let mut sys = info_of(w.system_id)
                    .map(|i| i.name)
                    .unwrap_or_else(|| format!("#{}", w.system_id));
                if let Some(sig) = &w.signature {
                    sys = format!("{sys}  [{sig}]");
                }
                let (dest, dest_const, dest_region) = match w.dest_system_id.and_then(info_of) {
                    Some(i) => (i.name, i.constellation, i.region),
                    None => (w.dest.label().to_string(), String::new(), String::new()),
                };
                let life = if w.explicit_expiry.is_some() {
                    match w.hours_left(now) {
                        Some(h) => format!("< {h}h left"),
                        None => "expired".into(),
                    }
                } else {
                    format!("reported {} ago", human_ago(now - w.reported_at))
                };
                Row {
                    sys_id: w.system_id,
                    sys,
                    wh_type: w.wh_type.clone().unwrap_or_else(|| "—".into()),
                    drifter: w.is_drifter,
                    dest,
                    dest_click: w.dest_system_id,
                    dest_const,
                    dest_region,
                    size: w.effective_size().map(|s| s.label().to_string()).unwrap_or_else(|| "—".into()),
                    life,
                    source: w.source.label().to_string(),
                }
            })
            .collect();

        use egui_phosphor::regular as icon;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("wh_grid").striped(true).num_columns(8).spacing([16.0, 6.0]).show(
                ui,
                |ui| {
                    for h in
                        ["System", "Type", "Destination", "Constellation", "Region", "Size", "Life", "Source"]
                    {
                        ui.label(egui::RichText::new(h).strong().small());
                    }
                    ui.end_row();
                    for r in &rows {
                        if ui.link(&r.sys).clicked() {
                            self.open_system(r.sys_id);
                        }
                        ui.horizontal(|ui| {
                            ui.label(&r.wh_type);
                            if r.drifter {
                                ui.label(
                                    egui::RichText::new(format!("{} drifter", icon::WARNING))
                                        .color(crate::theme::standing::WARNING)
                                        .small(),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon::ARROW_RIGHT).weak());
                            if let Some(id) = r.dest_click {
                                if ui.link(&r.dest).clicked() {
                                    self.open_system(id);
                                }
                            } else {
                                ui.label(&r.dest);
                            }
                        });
                        ui.label(&r.dest_const);
                        ui.label(&r.dest_region);
                        ui.label(&r.size);
                        ui.label(&r.life);
                        ui.label(&r.source);
                        ui.end_row();
                    }
                },
            );
        });
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
        let bridges: Vec<(i64, i64)> = self
            .settings
            .jump_bridges
            .iter()
            .filter_map(|b| {
                let from = systems.lookup(&b.from)?.id;
                let to = systems.lookup(&b.to)?.id;
                Some((from, to))
            })
            .collect();
        systems.add_bridges(&bridges);
        let systems = std::sync::Arc::new(systems);
        self.systems = Some(systems.clone());
        self.bridges_applied = self.settings.jump_bridges.clone();

        if let Some(store) = &self.store {
            if !store.traits_baked() {
                sde::spawn_traits_bake(store.path().to_path_buf(), ctx.clone());
            }
            // Pre-load remembered pilot names so they're recognised immediately. Negatives are
            // NOT preloaded — they live in-memory with a TTL (see NEG_TTL), so a name ESI once
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
            ctx.clone(),
        );

        if let Some(dir) = self.chat_dir.clone() {
            let ships = std::sync::Arc::new(store.ship_index());
            self.ship_index = Some(ships.clone());
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
                ctx.clone(),
            );
        }

        // Combat-event alerts (warp scrambled / under attack) are disabled for now: the game-log
        // watcher is not spawned, so no combat events reach the alert log or OS notifications.
    }

    fn intel_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        if self.chat_dir.is_none() {
            ui.colored_label(
                crate::theme::standing::WARNING,
                "EVE chat logs not found. Set the logs directory in Settings.",
            );
            return;
        }

        let player_sys = self.player_system();
        let systems = self.systems.clone();

        ui.horizontal(|ui| {
            use IntelTypeFilter::*;
            for (lbl, v) in [
                ("All", All),
                ("Hostile", Hostile),
                ("Clear", Clear),
                ("Kill", Kill),
                ("Threat", Threat),
            ] {
                if selectable_chip(ui, self.intel_type == v, lbl).clicked() {
                    self.intel_type = v;
                }
            }
            ui.separator();
            ui.label("\u{2264} jumps");
            ui.add(
                egui::DragValue::new(&mut self.intel_max_jumps)
                    .range(0..=50)
                    .custom_formatter(|n, _| if n == 0.0 { "any".to_owned() } else { format!("{n}") }),
            );
            ui.separator();
            ui.label("outdated after").on_hover_text("How long until intel is outdated");
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.intel_ttl_secs)
                        .range(30..=3600)
                        .suffix("s"),
                )
                .changed()
            {
                self.needs_save = true;
            }
            ui.separator();
            if ui.button("Severity…").on_hover_text("Configure intel severity colours").clicked() {
                self.severity_open = true;
            }
            ui.separator();
            if ui
                .checkbox(&mut self.settings.kill_intel, "zKill intel")
                .on_hover_text("Show zKill killmails within range as intel cards")
                .changed()
            {
                self.needs_save = true;
            }
            if self.settings.kill_intel
                && ui
                    .add(
                        egui::DragValue::new(&mut self.settings.kill_intel_jumps)
                            .range(0..=20)
                            .custom_formatter(|n, _| if n == 0.0 { "feed".to_owned() } else { format!("{n}j") }),
                    )
                    .on_hover_text("Kill-intel range")
                    .changed()
            {
                self.needs_save = true;
            }
            ui.separator();
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                egui::TextEdit::singleline(&mut self.intel_query)
                    .hint_text("Filter by system, text, or channel"),
            );
        });
        ui.add_space(6.0);

        let now = chrono::Utc::now().timestamp();
        let query = self.intel_query.trim().to_lowercase();
        let type_filter = self.intel_type;
        let max_jumps = self.intel_max_jumps;
        let sev_rules = self.settings.severity.clone();
        let state = self.intel_state.lock().unwrap();

        let mut matches: Vec<&crate::intel::IntelReport> = state
            .reports
            .iter()
            .filter(|r| r.primary_system().is_some() || !r.gates.is_empty())
            .filter(|r| type_filter.matches(r))
            .filter(|r| {
                max_jumps == 0
                    || jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id))
                        .is_some_and(|j| j <= max_jumps)
            })
            .filter(|r| {
                query.is_empty()
                    || r.text.to_lowercase().contains(&query)
                    || r.channel.to_lowercase().contains(&query)
                    || r.systems.iter().any(|s| s.name.to_lowercase().contains(&query))
            })
            .collect();
        matches.sort_by(|a, b| b.received.cmp(&a.received));
        let last_ship = build_last_ship(&state.reports);

        ui.label(egui::RichText::new(format!("{} reports", matches.len())).weak());
        ui.add_space(4.0);
        let filters_active = !query.is_empty()
            || type_filter != IntelTypeFilter::All
            || max_jumps != 0;
        if matches.is_empty() && filters_active {
            let mut clear = false;
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("No reports match the current filters.").weak(),
                );
                ui.add_space(8.0);
                clear = ui.button("Clear Filters").clicked();
            });
            if clear {
                self.intel_query.clear();
                self.intel_type = IntelTypeFilter::All;
                self.intel_max_jumps = 0;
            }
            return;
        }
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> = matches
            .iter()
            .flat_map(|r| r.ships.iter().map(|s| s.id))
            .collect::<std::collections::HashSet<i64>>()
            .into_iter()
            .filter_map(|id| self.ship_details_cached(id).map(|d| (id, d)))
            .collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> = matches
            .iter()
            .flat_map(|r| r.ships.iter().map(|s| s.id))
            .collect::<std::collections::HashSet<i64>>()
            .into_iter()
            .map(|id| (id, self.ship_roles_cached(id)))
            .collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp =
                cache.display_ids(matches.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let mut action: Option<IntelClick> = None;
        let ttl = self.settings.intel_ttl_secs;
        {
            let status = self.system_status.lock().unwrap();
            const CARD_CAP: usize = 250;
            if self.intel_heights.len() > 2000 {
                self.intel_heights.clear();
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(
                ui,
                |ui, viewport| {
                    let origin = ui.cursor().top();
                    for r in matches.iter().take(CARD_CAP) {
                        let y = ui.cursor().top() - origin;
                        let key = report_key(r);
                        if let Some(h) = self.intel_heights.get(&key).copied() {
                            if y + h < viewport.min.y - 400.0 || y > viewport.max.y + 400.0 {
                                ui.add_space(h);
                                continue;
                            }
                        }
                        let stale = state.is_stale(r) || (now - r.received) > ttl;
                        let from_you = jumps_from_you(
                            &systems,
                            player_sys,
                            r.primary_system().map(|s| s.id),
                        );
                        let sev = severity_of(r, &sev_rules);
                        let kc = self.kill_cache.clone();
                        let affil = self.affiliations.clone();
                        let inner = ui.scope(|ui| {
                            intel_row(
                                ui, r, now, stale, from_you, &systems, &status, &ship_details,
                                &ship_roles, &resolved_pilots, &uncertain, &last_ship, &kc, sev, true,
                            &affil, false, &mut None,
                            )
                        });
                        if let Some(a) = inner.inner {
                            action = Some(a);
                        }
                        let h = inner.response.rect.height();
                        if h > 0.0 {
                            self.intel_heights.insert(key, h);
                        }
                    }
                    if matches.len() > CARD_CAP {
                        ui.label(
                            egui::RichText::new(format!("+{} older", matches.len() - CARD_CAP))
                                .weak(),
                        );
                    }
                },
            );
        }
        drop(state);
        self.handle_intel_click(action, ui.ctx());
    }

    fn handle_intel_click(&mut self, action: Option<IntelClick>, ctx: &egui::Context) {
        match action {
            Some(IntelClick::System(id)) => self.open_system(id),
            Some(IntelClick::Ship(id)) => self.open_ship(id),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ctx.clone());
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            Some(IntelClick::Dscan(url)) => self.open_dscan(url, ctx),
            Some(IntelClick::PilotVerdict(name)) => self.open_pilot_verdict(name),
            None => {}
        }
    }

    fn open_pilot_verdict(&mut self, name: String) {
        if !self.settings.verdict_explained {
            self.verdict_explainer_open = true;
        }
        self.verdict_popup = Some(name);
    }

    fn apply_pilot_verdict(&mut self, name: &str, hidden: bool) {
        self.pilots.lock().unwrap_or_else(|e| e.into_inner()).set_verdict(name, hidden);
        if let Some(store) = &self.store {
            store.set_pilot_verdict(name, hidden);
        }
    }

    fn verdict_dialog(&mut self, ctx: &egui::Context) {
        if self.verdict_explainer_open {
            let mut ack = false;
            let resp = egui::Modal::new(egui::Id::new("verdict_explainer")).show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.heading("Uncertain pilot (?)");
                ui.add_space(4.0);
                ui.label(
                    "A \"?\" means this name matched a real EVE character, but that character looks \
                     inactive (no recent kills, corp move, or wide roaming). It may be a real but \
                     rarely-used pilot, or a chat word that happens to match a character name.",
                );
                ui.add_space(6.0);
                ui.label(
                    "Mark it \"Real pilot\" to keep it (the ? clears), or \"Not a pilot\" to hide it. \
                     Your choice is remembered.",
                );
                ui.add_space(8.0);
                if ui.button("Got it").clicked() {
                    ack = true;
                }
            });
            if ack || resp.should_close() {
                self.verdict_explainer_open = false;
                self.settings.verdict_explained = true;
                self.needs_save = true;
            }
            return;
        }
        let Some(name) = self.verdict_popup.clone() else {
            return;
        };
        let mut verdict: Option<bool> = None;
        let resp = egui::Modal::new(egui::Id::new("verdict_popup")).show(ctx, |ui| {
            ui.heading(format!("Is \"{name}\" a pilot?"));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("\"{name}\" matched a character that looks inactive."))
                    .weak(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Real pilot").clicked() {
                    verdict = Some(false);
                }
                if ui.button("Not a pilot (hide)").clicked() {
                    verdict = Some(true);
                }
            });
        });
        if let Some(hidden) = verdict {
            self.apply_pilot_verdict(&name, hidden);
            self.verdict_popup = None;
            ctx.request_repaint();
        } else if resp.should_close() {
            self.verdict_popup = None;
        }
    }

    fn render_intel_cards(
        &mut self,
        ui: &mut egui::Ui,
        reports: &[crate::intel::IntelReport],
    ) -> Option<IntelClick> {
        if reports.is_empty() {
            ui.label(egui::RichText::new("No intel in range.").weak());
            return None;
        }
        let now = chrono::Utc::now().timestamp();
        let player_sys = self.player_system();
        let systems = self.systems.clone();
        let sev_rules = self.settings.severity.clone();
        let ttl = self.settings.intel_ttl_secs;
        let ids: std::collections::HashSet<i64> =
            reports.iter().flat_map(|r| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> = ids
            .iter()
            .filter_map(|id| self.ship_details_cached(*id).map(|d| (*id, d)))
            .collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ids.iter().map(|id| (*id, self.ship_roles_cached(*id))).collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp =
                cache.display_ids(reports.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let last_ship = build_last_ship(reports);
        let kc = self.kill_cache.clone();
        let affil = self.affiliations.clone();
        let mut action = None;
        let state = self.intel_state.lock().unwrap();
        let status = self.system_status.lock().unwrap();
        for r in reports {
            let stale = state.is_stale(r) || (now - r.received) > ttl;
            let from_you = jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id));
            let sev = severity_of(r, &sev_rules);
            let inner = ui.scope(|ui| {
                intel_row(
                    ui, r, now, stale, from_you, &systems, &status, &ship_details, &ship_roles,
                    &resolved_pilots, &uncertain, &last_ship, &kc, sev, true,
                &affil, false, &mut None,
                )
            });
            if let Some(a) = inner.inner {
                action = Some(a);
            }
        }
        action
    }

    fn dashboard_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let now = chrono::Utc::now().timestamp();
        let player_sys = self.player_system();
        let systems = self.systems.clone();

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&self.active_character).strong());
                match player_sys.and_then(|s| systems.as_ref().and_then(|sy| sy.info_of(s))) {
                    Some(info) => {
                        ui.label("in");
                        ui.label(security_badge(info.security));
                        ui.label(egui::RichText::new(&info.name).strong());
                        system_chips(ui, &systems, &self.system_status.lock().unwrap(), info.id);
                    }
                    None => {
                        ui.label(egui::RichText::new("location unknown").weak());
                    }
                }
            });
        });
        ui.add_space(6.0);

        let (intel_count, nearest) = {
            let state = self.intel_state.lock().unwrap();
            let live: Vec<&crate::intel::IntelReport> =
                state.reports.iter().filter(|r| !r.clear && !state.is_stale(r)).collect();
            let nearest = live
                .iter()
                .filter_map(|r| {
                    let id = r.primary_system()?.id;
                    let j = jumps_from_you(&systems, player_sys, Some(id))?;
                    Some((j, r.primary_system().unwrap().name.clone()))
                })
                .min_by_key(|(j, _)| *j);
            (live.len(), nearest)
        };
        let battle_count = self.battles.lock().unwrap().iter().filter(|b| b.kills >= 2).count();

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Live intel: {intel_count}"));
            ui.separator();
            if let Some((j, name)) = &nearest {
                ui.label("Nearest hostile:");
                ui.label(egui::RichText::new(name).strong());
                ui.label(egui::RichText::new(format!("({j}j)")).weak());
            } else {
                ui.label(egui::RichText::new("no nearby hostiles").weak());
            }
            ui.separator();
            if battle_count > 0 {
                if ui.link(format!("Battles: {battle_count}")).clicked() {
                    self.view = View::Battles;
                }
            } else {
                ui.label(format!("Battles: {battle_count}"));
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        ui.label(egui::RichText::new("Recent alerts").strong());
        let log = self.recent_alerts.lock().unwrap();
        if log.is_empty() {
            ui.label(egui::RichText::new("None.").weak());
        } else {
            for (t, text) in log.iter().rev().take(5) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{:>7}", fmt_age(now - t))).monospace().weak());
                    ui.label(text);
                });
            }
        }
    }

    fn add_lookup_names(&mut self, text: &str) {
        for line in text.lines() {
            let name = line.split('\t').next().unwrap_or(line).trim();
            if name.len() < 3 || name.len() > 37 {
                continue;
            }
            if self.lookup_tabs.iter().any(|t| t.eq_ignore_ascii_case(name)) {
                continue;
            }
            self.lookup_tabs.push(name.to_owned());
            if let Some(tx) = &self.lookup_tx {
                let _ = tx.send(name.to_owned());
            }
        }
        if !self.lookup_tabs.is_empty() {
            self.lookup_active = self.lookup_tabs.len() - 1;
        }
    }

    fn lookup_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let text = f
                .bytes
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_else(|| f.name.clone());
            self.add_lookup_names(&text);
        }

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Paste pilot names (one per line, e.g. the local member list) or drop them here.",
            )
            .weak(),
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.lookup_input)
                .hint_text("Pilot names, one per line…")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Look up", icon::MAGNIFYING_GLASS)).clicked()
                && !self.lookup_input.trim().is_empty()
            {
                let text = std::mem::take(&mut self.lookup_input);
                self.add_lookup_names(&text);
            }
            if !self.lookup_tabs.is_empty() && ui.button("Close all").clicked() {
                self.lookup_tabs.clear();
                self.lookup_active = 0;
            }
        });
        ui.separator();
        if self.lookup_tabs.is_empty() {
            ui.label(egui::RichText::new("No lookups yet.").weak());
            return;
        }

        let tabs = self.lookup_tabs.clone();
        let mut close: Option<usize> = None;
        egui::ScrollArea::horizontal().id_salt("lookup_tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, name) in tabs.iter().enumerate() {
                    let label = self
                        .lookup_cache
                        .lock()
                        .unwrap()
                        .get(&name.to_lowercase())
                        .and_then(|o| o.as_ref())
                        .filter(|inf| inf.found)
                        .map(|inf| inf.name.clone())
                        .unwrap_or_else(|| name.clone());
                    if ui.selectable_label(self.lookup_active == i, label).clicked() {
                        self.lookup_active = i;
                    }
                    if ui
                        .add(egui::Button::new(egui::RichText::new(icon::X).small()).frame(false))
                        .on_hover_text("Close tab")
                        .clicked()
                    {
                        close = Some(i);
                    }
                    ui.separator();
                }
            });
        });
        if let Some(i) = close {
            self.lookup_tabs.remove(i);
            if self.lookup_active >= self.lookup_tabs.len() {
                self.lookup_active = self.lookup_tabs.len().saturating_sub(1);
            }
        }
        ui.separator();

        let Some(name) = self.lookup_tabs.get(self.lookup_active).cloned() else { return };
        let info = self.lookup_cache.lock().unwrap().get(&name.to_lowercase()).cloned();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Overview, "Overview");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Kills, "Kills");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Solo, "Solo");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Losses, "Losses");
        });
        ui.separator();
        let feed = if self.pilot_tab != PilotTab::Overview {
            Some(
                self.feed_cache
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let s = std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle));
                        crate::lookup::spawn_lookup(name.clone(), s.clone(), ui.ctx().clone());
                        s
                    })
                    .clone(),
            )
        } else {
            None
        };
        egui::ScrollArea::vertical().id_salt("lookup_body").show(ui, |ui| {
            if self.pilot_tab == PilotTab::Overview {
                match info {
                    None | Some(None) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("Looking up {name}..."));
                        });
                    }
                    Some(Some(inf)) if !inf.found => {
                        ui.label(format!("No character named \"{name}\" was found."));
                    }
                    Some(Some(inf)) => Self::lookup_profile(ui, &inf),
                }
                return;
            }
            match feed.map(|f| f.lock().unwrap().clone()) {
                Some(crate::lookup::LookupState::Done(report)) => {
                    let list = match self.pilot_tab {
                        PilotTab::Kills => &report.kills,
                        PilotTab::Solo => &report.solo,
                        _ => &report.losses,
                    };
                    self.km_list(ui, list, report.loading, true);
                }
                Some(crate::lookup::LookupState::Failed(e)) => {
                    ui.label(egui::RichText::new(e).weak());
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading killmails\u{2026}");
                    });
                }
            }
        });
    }

    fn lookup_profile(ui: &mut egui::Ui, info: &crate::charlookup::LookupInfo) {
        use egui_phosphor::regular as icon;
        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(eve_portrait_url(info.char_id, 72.0))
                    .fit_to_exact_size(egui::Vec2::splat(72.0)),
            );
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&info.name).strong().size(18.0));
                ui.horizontal(|ui| {
                    if let Some(aid) = info.alliance_id {
                        ui.add(
                            egui::Image::new(eve_alliance_logo_url(aid, 40.0))
                                .fit_to_exact_size(egui::Vec2::splat(40.0)),
                        )
                        .on_hover_text(if info.alliance.is_empty() {
                            "Alliance"
                        } else {
                            info.alliance.as_str()
                        });
                    }
                    if let Some(cid) = info.corp_id {
                        ui.add(
                            egui::Image::new(eve_corp_logo_url(cid, 40.0))
                                .fit_to_exact_size(egui::Vec2::splat(40.0)),
                        )
                        .on_hover_text(if info.corp.is_empty() {
                            "Corporation"
                        } else {
                            info.corp.as_str()
                        });
                    }
                });
            });
        });
        ui.separator();
        egui::Grid::new("lookup_stats").spacing([24.0, 4.0]).show(ui, |ui| {
            ui.label("Kills");
            ui.label(egui::RichText::new(info.ships_destroyed.to_string()).strong());
            ui.label("Losses");
            ui.label(info.ships_lost.to_string());
            ui.end_row();
            ui.label("ISK destroyed");
            ui.label(fmt_isk(info.isk_destroyed));
            ui.label("ISK lost");
            ui.label(fmt_isk(info.isk_lost));
            ui.end_row();
            ui.label("Danger");
            ui.label(format!("{}%", info.danger_ratio));
            ui.label("Gang");
            ui.label(format!("{}%", info.gang_ratio));
            ui.end_row();
        });
        if !info.top_ships.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Most-used ships").strong());
            ui.horizontal_wrapped(|ui| {
                for (id, name, kills) in &info.top_ships {
                    ui.add(
                        egui::Image::new(eve_type_icon_url(id, 28.0))
                            .fit_to_exact_size(egui::Vec2::splat(28.0)),
                    )
                    .on_hover_text(format!("{name}: {kills} kills"));
                }
            });
        }
        if !info.top_systems.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Most active systems").strong());
            ui.horizontal_wrapped(|ui| {
                for (sys, kills) in &info.top_systems {
                    ui.label(egui::RichText::new(format!("{sys} ({kills})")).weak());
                }
            });
        }
        ui.separator();
        if ui.button(format!("{}  Open on zKillboard", icon::ARROW_SQUARE_OUT)).clicked() {
            let _ = open::that(format!("https://zkillboard.com/character/{}/", info.char_id));
        }
    }

    fn battle_filter_dialog(&mut self, ctx: &egui::Context) {
        use crate::settings::{BattleCond, RuleAction, ShipSize};
        use egui_phosphor::regular as icon;
        if !self.battle_filter_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(ctx, "battle_filter", "EVE Spai - Battle rules", [580.0, 620.0], |ui| {
            ui.label(
                egui::RichText::new(
                    "Battles near your intel are shown by default. Add rules to include or exclude \
                     others. The first rule that matches a battle wins.",
                )
                .weak(),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let rules = &mut self.settings.battles.rules;
                let n = rules.len();
                let mut delete: Option<usize> = None;
                let mut swap: Option<(usize, usize)> = None;
                for (i, rule) in rules.iter_mut().enumerate() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut act = rule.action;
                            egui::ComboBox::from_id_salt(("br_act", i))
                                .selected_text(if act == RuleAction::Include { "Include" } else { "Exclude" })
                                .width(84.0)
                                .show_ui(ui, |ui| {
                                    changed |= ui.selectable_value(&mut act, RuleAction::Include, "Include").changed();
                                    changed |= ui.selectable_value(&mut act, RuleAction::Exclude, "Exclude").changed();
                                });
                            rule.action = act;
                            let mut all = rule.match_all;
                            egui::ComboBox::from_id_salt(("br_all", i))
                                .selected_text(if all { "All of" } else { "Any of" })
                                .width(72.0)
                                .show_ui(ui, |ui| {
                                    changed |= ui.selectable_value(&mut all, true, "All of").changed();
                                    changed |= ui.selectable_value(&mut all, false, "Any of").changed();
                                });
                            rule.match_all = all;
                            if rule.is_broad() {
                                ui.label(egui::RichText::new(icon::WARNING).color(egui::Color32::from_rgb(0xE0, 0xB0, 0x4C)))
                                    .on_hover_text("Matches anywhere in EVE and can store a lot of battle history. Add a region/constellation/system/jumps or participant condition to bound it.");
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(icon::TRASH).clicked() {
                                    delete = Some(i);
                                }
                                if i + 1 < n && ui.button(icon::ARROW_DOWN).clicked() {
                                    swap = Some((i, i + 1));
                                }
                                if i > 0 && ui.button(icon::ARROW_UP).clicked() {
                                    swap = Some((i, i - 1));
                                }
                            });
                        });
                        let mut del_cond: Option<usize> = None;
                        for (j, cond) in rule.conditions.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("br_ck", i, j))
                                    .selected_text(cond.kind_label())
                                    .width(140.0)
                                    .show_ui(ui, |ui| {
                                        for k in BattleCond::kinds() {
                                            if ui.selectable_label(cond.kind_label() == k.kind_label(), k.kind_label()).clicked()
                                                && cond.kind_label() != k.kind_label()
                                            {
                                                *cond = k;
                                                changed = true;
                                            }
                                        }
                                    });
                                match cond {
                                    BattleCond::IntelArea => {
                                        ui.label(egui::RichText::new("(default tracked area)").weak());
                                    }
                                    BattleCond::Coalition(s)
                                    | BattleCond::Alliance(s)
                                    | BattleCond::Corporation(s)
                                    | BattleCond::Player(s)
                                    | BattleCond::Region(s)
                                    | BattleCond::Constellation(s)
                                    | BattleCond::System(s)
                                    | BattleCond::ShipType(s) => {
                                        changed |= ui
                                            .add(egui::TextEdit::singleline(s).desired_width(200.0).hint_text("name"))
                                            .changed();
                                    }
                                    BattleCond::JumpsFromMe(nn) => {
                                        changed |= ui.add(egui::DragValue::new(nn).range(0..=100).suffix(" jumps")).changed();
                                    }
                                    BattleCond::HullSizeAtLeast(sz) => {
                                        egui::ComboBox::from_id_salt(("br_sz", i, j))
                                            .selected_text(sz.label())
                                            .show_ui(ui, |ui| {
                                                for opt in ShipSize::CHOICES {
                                                    changed |= ui.selectable_value(sz, opt, opt.label()).changed();
                                                }
                                            });
                                    }
                                    BattleCond::IskAtLeast(v) | BattleCond::IskAtMost(v) => {
                                        let mut m = *v / 1e6;
                                        if ui
                                            .add(egui::DragValue::new(&mut m).speed(50.0).range(0.0..=1e9).suffix("M ISK"))
                                            .changed()
                                        {
                                            *v = m * 1e6;
                                            changed = true;
                                        }
                                    }
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(icon::X).on_hover_text("Remove condition").clicked() {
                                        del_cond = Some(j);
                                    }
                                });
                            });
                            if matches!(
                                cond,
                                BattleCond::Alliance(_)
                                    | BattleCond::Corporation(_)
                                    | BattleCond::Player(_)
                                    | BattleCond::ShipType(_)
                            ) {
                                ui.label(
                                    egui::RichText::new(
                                        "   pairs with a region/coalition/hull condition to pull in new battles",
                                    )
                                    .weak()
                                    .size(11.0),
                                );
                            }
                        }
                        if let Some(j) = del_cond {
                            rule.conditions.remove(j);
                            changed = true;
                        }
                        if ui.button(format!("{}  condition", icon::PLUS)).clicked() {
                            rule.conditions.push(BattleCond::Region(String::new()));
                            changed = true;
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some((a, b)) = swap {
                    rules.swap(a, b);
                    changed = true;
                }
                if let Some(i) = delete {
                    rules.remove(i);
                    changed = true;
                }
                if ui.button(format!("{}  Add rule", icon::PLUS)).clicked() {
                    rules.push(crate::settings::BattleRule::default());
                    changed = true;
                }
                ui.add_space(8.0);
                ui.separator();
                if self.battle_filter_confirm_reset {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Replace all rules with the default?").strong());
                        if ui.button("Restore").clicked() {
                            *rules = crate::settings::BattleFilter::default_rules();
                            changed = true;
                            self.battle_filter_confirm_reset = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.battle_filter_confirm_reset = false;
                        }
                    });
                } else if ui.button("Restore defaults").clicked() {
                    self.battle_filter_confirm_reset = true;
                }
            });
        });
        if !keep {
            self.battle_filter_open = false;
            self.battle_filter_confirm_reset = false;
        }
        if changed {
            self.needs_save = true;
            self.battle_filter_gen = self.battle_filter_gen.wrapping_add(1);
            self.battle_filter_gen_shared.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *self.battle_filter.lock().unwrap() = self.settings.battles.clone();
        }
    }


    fn open_dscan(&mut self, url: String, ctx: &egui::Context) {
        if let Some(view) = open_dscan_view(url, self.ship_index.clone(), ctx) {
            self.dscan_view = Some(view);
        }
    }

    fn dscan_view_dialog(&mut self, ctx: &egui::Context) {
        let mut open_ship: Option<i64> = None;
        dscan_view_dialog_ui(ctx, &mut self.dscan_view, false, &mut open_ship);
        if let Some(id) = open_ship {
            self.open_ship(id);
        }
    }

    fn load_battle_history(&self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        if self.battle_history_loading.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(systems) = self.systems.clone() else {
            self.battle_history_loading.store(false, Ordering::SeqCst);
            return;
        };
        let out = self.battle_history.clone();
        let loading = self.battle_history_loading.clone();
        let break_gap = self.settings.battle_break_secs;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let battles = crate::store::Store::open()
                .ok()
                .map(|s| {
                    let engs = s.load_engagements(0);
                    let overrides = s.load_battle_overrides();
                    crate::battle::cluster(
                        &engs,
                        crate::battle::BATTLE_WINDOW_SECS,
                        crate::battle::BATTLE_MAX_JUMPS,
                        break_gap,
                        &overrides,
                        |a, b| systems.jumps(a, b, crate::battle::BATTLE_MAX_JUMPS),
                    )
                    .into_iter()
                    .filter(|b| b.is_anchored() && b.is_two_sided())
                    .collect()
                })
                .unwrap_or_default();
            *out.lock().unwrap() = battles;
            loading.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    fn apply_battle_edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&crate::store::Store)) {
        {
            let Some(store) = &self.store else { return };
            f(store);
            let fresh = store.load_battle_overrides();
            *self.battle_overrides.lock().unwrap() = fresh;
            self.battle_excluded_count = store.count_excluded();
            self.battle_scrub_count = store.count_scrubs();
        }
        self.battle_overrides_gen = self.battle_overrides_gen.wrapping_add(1);
        self.battle_overrides_gen_shared
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.battle_detail_cache = None;
        if self.show_history {
            self.load_battle_history(ctx);
        }
    }

    fn battle_edit_view(&mut self, ui: &mut egui::Ui, now: i64) {
        use egui_phosphor::regular as icon;
        let ctx = ui.ctx().clone();
        let mut engs: Vec<crate::battle::Engagement> = self
            .battle_detail_cache
            .as_ref()
            .map(|c| c.battle.engagements.clone())
            .unwrap_or_default();
        engs.sort_by_key(|e| e.time);
        let splits = self
            .battle_detail_cache
            .as_ref()
            .map(|c| c.battle.suggested_splits.clone())
            .unwrap_or_default();
        if engs.is_empty() {
            ui.label(egui::RichText::new("No kills to edit.").weak());
            return;
        }
        let ship_ids: Vec<i64> = engs.iter().map(|e| e.victim_ship).filter(|&i| i != 0).collect();
        self.ensure_type_names(&ship_ids, &ctx);
        let names: std::collections::HashMap<i64, String> = {
            let g = self.type_names.lock().unwrap();
            ship_ids
                .iter()
                .map(|&id| (id, g.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"))))
                .collect()
        };
        let name_of = |id: i64| -> String {
            if id == 0 {
                return "?".to_owned();
            }
            crate::intel::structure_name_by_type(id)
                .map(|s| s.to_owned())
                .or_else(|| names.get(&id).cloned())
                .unwrap_or_else(|| format!("Type {id}"))
        };
        let break_gap = self.settings.battle_break_secs;

        let mut do_exclude: Option<i64> = None;
        let mut open_sys: Option<i64> = None;
        let mut purge_pilot: Option<i64> = None;
        let mut do_split = false;

        if !splits.is_empty() {
            let ship_count = |pred: &dyn Fn(&crate::battle::Engagement) -> bool| -> usize {
                let mut set: std::collections::HashSet<i64> = std::collections::HashSet::new();
                for e in engs.iter().filter(|e| pred(e)) {
                    if e.victim_char != 0 {
                        set.insert(e.victim_char);
                    }
                    for a in &e.attackers {
                        if a.char_id != 0 {
                            set.insert(a.char_id);
                        }
                    }
                }
                set.len()
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Suggested splits:").strong());
                for sug in &splits {
                    let boundary = sug.time;
                    let before_ships = ship_count(&|e| e.time < boundary);
                    let after_ships = ship_count(&|e| e.time >= boundary);
                    let before_kills = engs.iter().filter(|e| e.time < boundary).count();
                    let after_kills = engs.len() - before_kills;
                    let split_before = before_ships < after_ships
                        || (before_ships == after_ships && before_kills < after_kills);
                    let off = before_ships.min(after_ships);
                    let hhmm = chrono::DateTime::from_timestamp(boundary, 0)
                        .map(|t| t.format("%H:%M").to_string())
                        .unwrap_or_default();
                    let reason = sug.reason.label();
                    if ui
                        .button(format!("{} Split off {off} ships: {reason} ({hhmm})", icon::SCISSORS))
                        .on_hover_text(format!(
                            "Suggested because of a {reason} at {hhmm}. Selects the smaller group ({off} ships) to split off."
                        ))
                        .clicked()
                    {
                        self.battle_kill_sel = engs
                            .iter()
                            .filter(|e| if split_before { e.time < boundary } else { e.time >= boundary })
                            .map(|e| e.kill_id)
                            .collect();
                    }
                }
            });
            ui.add_space(6.0);
        }

        let sel_ids = self.battle_kill_sel.clone();
        if sel_ids.is_empty() {
            self.battle_split_preview = None;
        }
        if !sel_ids.is_empty() {
            let stale = self
                .battle_split_preview
                .as_ref()
                .map(|(s, _, _)| s != &sel_ids)
                .unwrap_or(true);
            if stale {
                let sel: Vec<crate::battle::Engagement> =
                    engs.iter().filter(|e| sel_ids.contains(&e.kill_id)).cloned().collect();
                let rest: Vec<crate::battle::Engagement> =
                    engs.iter().filter(|e| !sel_ids.contains(&e.kill_id)).cloned().collect();
                let pa = crate::battle::preview_battle(sel, break_gap);
                let pb = crate::battle::preview_battle(rest, break_gap);
                self.battle_split_preview = Some((sel_ids.clone(), pa, pb));
            }
            let (pa, pb) = self
                .battle_split_preview
                .as_ref()
                .map(|(_, a, b)| (a, b))
                .unwrap();
            let rest_empty = pb.engagements.is_empty();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(format!("Split preview: {} kills selected", sel_ids.len())).strong(),
                );
                ui.horizontal_wrapped(|ui| {
                    battle_preview_summary(ui, "Split off", pa);
                    ui.separator();
                    battle_preview_summary(ui, "Remaining", pb);
                });
                ui.horizontal(|ui| {
                    if rest_empty {
                        ui.label(
                            egui::RichText::new("Leave at least one kill behind.")
                                .color(crate::theme::standing::WARNING),
                        );
                    } else if ui
                        .button(format!("{} Split off {} kills", icon::SCISSORS, sel_ids.len()))
                        .on_hover_text("Tag both halves so they cluster as two separate battles")
                        .clicked()
                    {
                        do_split = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.battle_kill_sel.clear();
                    }
                });
            });
            ui.add_space(6.0);
        }

        let avail_h = (ui.available_height() - 8.0).max(160.0);
        egui::ScrollArea::vertical().id_salt("battle_edit_kills").max_height(avail_h).show(ui, |ui| {
            for e in &engs {
                let mut sel = self.battle_kill_sel.contains(&e.kill_id);
                egui::Frame::new()
                    .fill(if sel {
                        crate::theme::standing::WARNING.gamma_multiply(0.12)
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut sel, "").changed() {
                                if sel {
                                    self.battle_kill_sel.insert(e.kill_id);
                                } else {
                                    self.battle_kill_sel.remove(&e.kill_id);
                                }
                            }
                            ui.label(
                                egui::RichText::new(format!("{:>7}", fmt_age(now - e.time))).monospace().weak(),
                            );
                            hull_badge(ui, e.victim_ship, 22.0);
                            ui.label(egui::RichText::new(name_of(e.victim_ship)).strong());
                            ui.label(&e.victim_pilot);
                            if ui
                                .link(egui::RichText::new(&e.system_name).weak())
                                .on_hover_text("Open system info")
                                .clicked()
                            {
                                open_sys = Some(e.system_id);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .button(egui_phosphor::regular::TRASH)
                                    .on_hover_text("Remove this kill from battle reports")
                                    .clicked()
                                {
                                    do_exclude = Some(e.kill_id);
                                }
                                ui.label(egui::RichText::new(fmt_isk(e.isk)).weak());
                            });
                        });
                    });
                ui.add_space(2.0);
            }

            ui.add_space(6.0);
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{} Pilots", egui_phosphor::regular::USERS)).strong(),
            )
            .id_salt("battle_edit_pilots")
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Purge removes a pilot from this battle: their losses are excluded and their \
                         attacker entries scrubbed.",
                    )
                    .weak(),
                );
                let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
                let mut pilots: Vec<(i64, String)> = Vec::new();
                for e in &engs {
                    if e.victim_char != 0 && seen.insert(e.victim_char) {
                        pilots.push((e.victim_char, e.victim_pilot.clone()));
                    }
                    for a in &e.attackers {
                        if a.char_id != 0 && seen.insert(a.char_id) {
                            pilots.push((a.char_id, a.pilot.clone()));
                        }
                    }
                }
                pilots.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
                for (char_id, pilot) in &pilots {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(pilot).strong());
                        if ui
                            .button(format!("{} Remove pilot", egui_phosphor::regular::USER_MINUS))
                            .on_hover_text("Exclude their losses and scrub their attacker entries here")
                            .clicked()
                        {
                            purge_pilot = Some(*char_id);
                        }
                    });
                }
                if pilots.is_empty() {
                    ui.label(egui::RichText::new("No identified pilots.").weak());
                }
            });
        });

        if let Some(sid) = open_sys {
            self.open_system(sid);
        }
        if let Some(kid) = do_exclude {
            self.apply_battle_edit(&ctx, |s| s.set_battle_excluded(kid, true));
        }
        if let Some(p) = purge_pilot {
            let engs2 = engs.clone();
            self.apply_battle_edit(&ctx, move |s| {
                for e in &engs2 {
                    if e.victim_char == p {
                        s.set_battle_excluded(e.kill_id, true);
                    } else if e.attackers.iter().any(|a| a.char_id == p) {
                        s.set_scrub(e.kill_id, p, true);
                    }
                }
            });
        }
        if do_split {
            let selected: Vec<i64> =
                engs.iter().map(|e| e.kill_id).filter(|k| sel_ids.contains(k)).collect();
            let rest_ids: Vec<i64> =
                engs.iter().map(|e| e.kill_id).filter(|k| !sel_ids.contains(k)).collect();
            self.apply_battle_edit(&ctx, move |s| {
                let ta = s.next_battle_tag();
                for kid in &selected {
                    s.set_battle_tag(*kid, Some(ta));
                }
                let tb = ta + 1;
                for kid in &rest_ids {
                    s.set_battle_tag(*kid, Some(tb));
                }
            });
            self.battle_kill_sel.clear();
            self.battle_edit_mode = false;
            self.battle_selected = None;
            self.battle_detail_cache = None;
        }
    }

    fn battle_review_panels(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        let now = chrono::Utc::now().timestamp();

        if self.battle_excluded_open {
            let list = self.store.as_ref().map(|s| s.list_excluded_engagements()).unwrap_or_default();
            let ids: Vec<i64> = list.iter().map(|e| e.victim_ship).filter(|&i| i != 0).collect();
            self.ensure_type_names(&ids, ctx);
            let names: std::collections::HashMap<i64, String> = {
                let g = self.type_names.lock().unwrap();
                ids.iter()
                    .map(|&id| (id, g.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"))))
                    .collect()
            };
            let hull = |id: i64| -> String {
                crate::intel::structure_name_by_type(id)
                    .map(|s| s.to_owned())
                    .or_else(|| names.get(&id).cloned())
                    .unwrap_or_else(|| "?".to_owned())
            };
            let mut open = true;
            let mut restore: Option<i64> = None;
            egui::Window::new(format!("{} Excluded kills", icon::TRASH))
                .open(&mut open)
                .resizable(true)
                .default_width(460.0)
                .show(ctx, |ui| {
                    if list.is_empty() {
                        ui.label(egui::RichText::new("No excluded kills.").weak());
                    }
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for e in &list {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:>7}", fmt_age(now - e.time)))
                                        .monospace()
                                        .weak(),
                                );
                                hull_badge(ui, e.victim_ship, 20.0);
                                ui.label(egui::RichText::new(hull(e.victim_ship)).strong());
                                ui.label(&e.victim_pilot);
                                ui.label(egui::RichText::new(&e.system_name).weak());
                                ui.label(egui::RichText::new(fmt_isk(e.isk)).weak());
                                if ui
                                    .button(format!("{} Restore", icon::ARROW_COUNTER_CLOCKWISE))
                                    .clicked()
                                {
                                    restore = Some(e.kill_id);
                                }
                            });
                        }
                    });
                });
            self.battle_excluded_open = open;
            if let Some(kid) = restore {
                self.apply_battle_edit(ctx, |s| s.set_battle_excluded(kid, false));
            }
        }

        if self.battle_scrubs_open {
            let list = self.store.as_ref().map(|s| s.list_scrubs()).unwrap_or_default();
            let mut open = true;
            let mut restore: Option<(i64, i64)> = None;
            egui::Window::new(format!("{} Scrubbed pilots", icon::BROOM))
                .open(&mut open)
                .resizable(true)
                .default_width(360.0)
                .show(ctx, |ui| {
                    if list.is_empty() {
                        ui.label(egui::RichText::new("No scrubbed pilots.").weak());
                    }
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (kill_id, char_id) in &list {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("kill {kill_id}")).monospace().weak());
                                ui.label(format!("char {char_id}"));
                                if ui
                                    .button(format!("{} Restore", icon::ARROW_COUNTER_CLOCKWISE))
                                    .clicked()
                                {
                                    restore = Some((*kill_id, *char_id));
                                }
                            });
                        }
                    });
                });
            self.battle_scrubs_open = open;
            if let Some((kill_id, char_id)) = restore {
                self.apply_battle_edit(ctx, |s| s.set_scrub(kill_id, char_id, false));
            }
        }

        if self.battle_add_open {
            let target_kids: Vec<i64> = self
                .battle_detail_cache
                .as_ref()
                .map(|c| c.battle.engagements.iter().map(|e| e.kill_id).collect())
                .unwrap_or_default();
            let existing_tag = {
                let o = self.battle_overrides.lock().unwrap();
                target_kids.iter().find_map(|k| o.tag.get(k).copied())
            };
            let known: std::collections::HashSet<i64> = {
                let mut s = std::collections::HashSet::new();
                for src in [&self.battles, &self.battle_history] {
                    for b in src.lock().unwrap().iter() {
                        for e in &b.engagements {
                            s.insert(e.kill_id);
                        }
                    }
                }
                s
            };
            let mut rows = self.store.as_ref().map(|s| s.load_kill_intel(now - 86_400)).unwrap_or_default();
            rows.reverse();
            rows.truncate(300);
            let ship_ids: Vec<i64> = rows.iter().map(|r| r.2).filter(|&i| i != 0).collect();
            self.ensure_type_names(&ship_ids, ctx);
            let names: std::collections::HashMap<i64, String> = {
                let g = self.type_names.lock().unwrap();
                ship_ids
                    .iter()
                    .map(|&id| (id, g.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"))))
                    .collect()
            };
            let systems = self.systems.clone();
            let sys_name = |id: i64| -> String {
                systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| format!("Sys {id}"))
            };
            let hull = |id: i64| -> String {
                crate::intel::structure_name_by_type(id)
                    .map(|s| s.to_owned())
                    .or_else(|| names.get(&id).cloned())
                    .unwrap_or_else(|| "?".to_owned())
            };

            let mut open = true;
            let mut add_kid: Option<i64> = None;
            let mut link_input = std::mem::take(&mut self.battle_add_link);
            egui::Window::new(format!("{} Add kill to battle", icon::PLUS))
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("zKill link");
                        ui.add(
                            egui::TextEdit::singleline(&mut link_input)
                                .hint_text("https://zkillboard.com/kill/…")
                                .desired_width(300.0),
                        );
                        if ui.button("Add").clicked() {
                            if let Some(k) =
                                crate::intel::extract_links(&link_input).into_iter().find_map(|l| l.kill_id)
                            {
                                add_kid = Some(k);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Recent kills (last 24h). Pick one to attach to this battle.")
                            .weak(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "A kill not yet in a battle is tagged now and will appear when fetched.",
                        )
                        .weak(),
                    );
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (kill_id, system_id, ship_type_id, time, value) in &rows {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:>7}", fmt_age(now - time)))
                                        .monospace()
                                        .weak(),
                                );
                                hull_badge(ui, *ship_type_id, 20.0);
                                ui.label(egui::RichText::new(hull(*ship_type_id)).strong());
                                ui.label(egui::RichText::new(sys_name(*system_id)).weak());
                                ui.label(egui::RichText::new(fmt_isk(*value)).weak());
                                if known.contains(kill_id) {
                                    ui.label(
                                        egui::RichText::new("in a BR")
                                            .color(crate::theme::standing::WARNING),
                                    )
                                    .on_hover_text("Already part of a clustered battle");
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(format!("{} Add", icon::PLUS)).clicked() {
                                            add_kid = Some(*kill_id);
                                        }
                                    },
                                );
                            });
                        }
                    });
                });
            self.battle_add_link = link_input;
            self.battle_add_open = open;
            if let Some(nk) = add_kid {
                let tids = target_kids.clone();
                self.apply_battle_edit(ctx, move |s| {
                    let tag = existing_tag.unwrap_or_else(|| s.next_battle_tag());
                    if existing_tag.is_none() {
                        for k in &tids {
                            s.set_battle_tag(*k, Some(tag));
                        }
                    }
                    s.set_battle_tag(nk, Some(tag));
                });
                self.battle_add_queue.lock().unwrap().push(nk);
                self.battle_add_link.clear();
            }
        }
    }

    fn save_battle_report(
        &self,
        battle: &crate::battle::Battle,
    ) -> anyhow::Result<Option<std::path::PathBuf>> {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(crate::breport::default_file_name(battle))
            .add_filter("EVE Spai battle report", &["json"])
            .save_file()
        else {
            return Ok(None);
        };
        let overrides = self.battle_overrides.lock().unwrap().clone();
        let now = chrono::Utc::now().timestamp();
        let ship_names = self.battle_ship_names(battle);
        let affiliations = self.battle_affiliations(battle);
        let doc = crate::breport::BattleReportDoc::new(
            battle.clone(),
            battle.engagements.clone(),
            overrides,
            None,
            now,
            ship_names,
            affiliations,
        );
        std::fs::write(&path, doc.to_json()?)?;
        Ok(Some(path))
    }

    fn br_authed_chars(&self) -> Vec<(i64, String)> {
        self.characters
            .iter()
            .filter(|c| crate::tokens::load_refresh(c.id).is_some())
            .map(|c| (c.id, c.name.clone()))
            .collect()
    }

    fn share_identity(&self) -> Option<(i64, std::path::PathBuf)> {
        let path = self.store.as_ref()?.path().to_path_buf();
        let authed = |id: i64| crate::tokens::load_refresh(id).is_some();
        let id = self
            .br_character
            .filter(|id| authed(*id))
            .or_else(|| {
                self.characters
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(&self.active_character) && authed(c.id))
                    .map(|c| c.id)
            })
            .or_else(|| self.characters.iter().map(|c| c.id).find(|id| authed(*id)))?;
        Some((id, path))
    }

    fn battle_ship_names(
        &self,
        battle: &crate::battle::Battle,
    ) -> std::collections::BTreeMap<i64, String> {
        let mut ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for e in &battle.engagements {
            ids.insert(e.victim_ship);
            for a in &e.attackers {
                ids.insert(a.ship);
            }
        }
        for i in 0..battle.sides.len() {
            for p in battle.roster(i) {
                ids.insert(p.ship);
                if let Some(l) = &p.lost {
                    ids.insert(l.pod_ship);
                }
            }
        }
        ids.remove(&0);
        let type_names = self.type_names.lock().unwrap();
        ids.into_iter()
            .filter_map(|id| type_names.get(&id).map(|n| (id, n.clone())))
            .collect()
    }

    fn battle_affiliations(
        &self,
        battle: &crate::battle::Battle,
    ) -> std::collections::BTreeMap<i64, crate::battle::Affil> {
        let mut ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for e in &battle.engagements {
            ids.insert(e.victim_char);
            for a in &e.attackers {
                ids.insert(a.char_id);
            }
        }
        for i in 0..battle.sides.len() {
            for p in battle.roster(i) {
                ids.insert(p.char_id);
            }
        }
        ids.remove(&0);
        let cache = self.affiliations.lock().unwrap();
        ids.into_iter()
            .filter_map(|id| {
                let a = cache.get(id)?;
                let corp_id = a.corp?;
                Some((
                    id,
                    crate::battle::Affil {
                        corp_id,
                        corp_name: a.corp_name.unwrap_or_default(),
                        alliance_id: a.alliance.unwrap_or(0),
                        alliance_name: a.alliance_name.unwrap_or_default(),
                    },
                ))
            })
            .collect()
    }

    fn build_share_doc(&self, battle: &crate::battle::Battle) -> crate::breport::BattleReportDoc {
        let overrides = self.battle_overrides.lock().unwrap().clone();
        let now = chrono::Utc::now().timestamp();
        let ship_names = self.battle_ship_names(battle);
        let affiliations = self.battle_affiliations(battle);
        crate::breport::BattleReportDoc::new(
            battle.clone(),
            battle.engagements.clone(),
            overrides,
            None,
            now,
            ship_names,
            affiliations,
        )
    }

    fn start_share(&mut self, battle: &crate::battle::Battle, ctx: &egui::Context) {
        match self.share_identity() {
            Some((char_id, path)) => {
                let doc = self.build_share_doc(battle);
                crate::brshare::spawn_share(
                    doc,
                    path,
                    char_id,
                    self.br_unlisted,
                    self.br_share.clone(),
                    ctx.clone(),
                );
            }
            None => {
                *self.br_share.lock().unwrap() =
                    crate::brshare::ShareStatus::Error("Log in to share (opening EVE SSO…).".into());
                self.start_login(ctx);
            }
        }
    }

    fn open_my_shared(&mut self, ctx: &egui::Context) {
        self.br_mine_open = true;
        match self.share_identity() {
            Some((char_id, path)) => {
                crate::brshare::spawn_load_mine(path, char_id, self.br_mine.clone(), ctx.clone());
            }
            None => {
                self.br_mine.lock().unwrap().status =
                    crate::brshare::MineStatus::Error("Log in to see your shared reports.".into());
            }
        }
    }

    fn share_status_ui(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        enum Action {
            None,
            Dismiss,
            Delete(String),
        }
        let mut action = Action::None;
        {
            let state = self.br_share.lock().unwrap();
            match &*state {
                crate::brshare::ShareStatus::Idle => return,
                crate::brshare::ShareStatus::Uploading => {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Sharing to eve-spai.com…");
                    });
                }
                crate::brshare::ShareStatus::Done { id, url } => {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}  Shared:", icon::SHARE_NETWORK))
                                .color(egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)),
                        );
                        ui.hyperlink(url);
                        if ui.button(format!("{} Copy", icon::COPY)).clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                        if ui.button(format!("{} Open", icon::GLOBE)).clicked() {
                            let _ = open::that(url);
                        }
                        if ui.button(format!("{} Delete", icon::TRASH)).clicked() {
                            action = Action::Delete(id.clone());
                        }
                        if ui.button(icon::X).on_hover_text("Dismiss").clicked() {
                            action = Action::Dismiss;
                        }
                    });
                }
                crate::brshare::ShareStatus::Error(e) => {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(crate::theme::standing::WARNING, e);
                        if ui.button(icon::X).on_hover_text("Dismiss").clicked() {
                            action = Action::Dismiss;
                        }
                    });
                }
            }
        }
        match action {
            Action::None => {}
            Action::Dismiss => {
                *self.br_share.lock().unwrap() = crate::brshare::ShareStatus::Idle;
            }
            Action::Delete(id) => {
                if let Some((char_id, path)) = self.share_identity() {
                    crate::brshare::spawn_delete_share(
                        path,
                        char_id,
                        id,
                        self.br_share.clone(),
                        ui.ctx().clone(),
                    );
                }
            }
        }
    }

    fn my_shared_window(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        if !self.br_mine_open {
            return;
        }
        let mut open = true;
        let base = crate::brshare::api_base();
        let mut reload = false;
        let mut delete_id: Option<String> = None;
        egui::Window::new(format!("{}  My shared BRs", icon::SHARE_NETWORK))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Refresh", icon::ARROWS_CLOCKWISE)).clicked() {
                        reload = true;
                    }
                });
                ui.add_space(4.0);
                let state = self.br_mine.lock().unwrap();
                if let Some(msg) = &state.msg {
                    ui.colored_label(crate::theme::standing::WARNING, msg);
                    ui.add_space(4.0);
                }
                match &state.status {
                    crate::brshare::MineStatus::Idle => {}
                    crate::brshare::MineStatus::Loading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading your shared reports…");
                        });
                    }
                    crate::brshare::MineStatus::Error(e) => {
                        ui.colored_label(crate::theme::standing::WARNING, e);
                    }
                    crate::brshare::MineStatus::Loaded(rows) if rows.is_empty() => {
                        ui.label(egui::RichText::new("You haven't shared any battle reports yet.").weak());
                    }
                    crate::brshare::MineStatus::Loaded(rows) => {
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            for r in rows {
                                egui::Frame::new()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(egui::Margin::symmetric(8, 6))
                                    .corner_radius(4.0)
                                    .show(ui, |ui| {
                                        let title = r.title.clone().unwrap_or_else(|| {
                                            r.systems.first().cloned().unwrap_or_else(|| "Battle report".into())
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(title).strong());
                                            if r.unlisted == Some(true) {
                                                ui.label(
                                                    egui::RichText::new("unlisted")
                                                        .color(crate::theme::standing::WARNING),
                                                );
                                            }
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .button(format!("{} Delete", icon::TRASH))
                                                        .clicked()
                                                    {
                                                        delete_id = Some(r.id.clone());
                                                    }
                                                    if ui
                                                        .button(format!("{} Open", icon::GLOBE))
                                                        .clicked()
                                                    {
                                                        let _ = open::that(r.url(&base));
                                                    }
                                                },
                                            );
                                        });
                                        ui.horizontal_wrapped(|ui| {
                                            if !r.systems.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(r.systems.join(", ")).weak(),
                                                );
                                                ui.label(egui::RichText::new("·").weak());
                                            }
                                            if let Some(d) = r
                                                .started_at
                                                .as_deref()
                                                .and_then(|s| {
                                                    chrono::DateTime::parse_from_rfc3339(s).ok()
                                                })
                                            {
                                                ui.label(
                                                    egui::RichText::new(
                                                        d.format("%Y-%m-%d %H:%M").to_string(),
                                                    )
                                                    .weak(),
                                                );
                                                ui.label(egui::RichText::new("·").weak());
                                            }
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} kills · {:.1}B ISK · {} {} views",
                                                    r.kills,
                                                    r.total_isk / 1e9,
                                                    icon::EYE,
                                                    r.views
                                                ))
                                                .weak(),
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                        });
                    }
                }
            });
        if let Some(id) = delete_id {
            if let Some((char_id, path)) = self.share_identity() {
                crate::brshare::spawn_delete_mine(
                    path,
                    char_id,
                    id,
                    self.br_mine.clone(),
                    ctx.clone(),
                );
            }
        }
        if reload {
            self.open_my_shared(ctx);
        }
        if !open {
            self.br_mine_open = false;
        }
    }

    fn load_battle_report(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let parsed = std::fs::read_to_string(path)
            .map_err(anyhow::Error::from)
            .and_then(|s| crate::breport::BattleReportDoc::from_json(&s));
        match parsed {
            Ok(doc) => {
                let b = if doc.engagements.is_empty() {
                    doc.battle
                } else {
                    crate::battle::preview_battle(doc.engagements, self.settings.battle_break_secs)
                };
                let title = doc.title.clone().unwrap_or_else(|| {
                    b.systems.first().map(|(_, n, _)| n.clone()).unwrap_or_else(|| "Battle report".into())
                });
                self.show_imported_report(b, title, ctx);
            }
            Err(e) => self.report_msg = Some(format!("Could not open report: {e}")),
        }
    }

    fn show_imported_report(&mut self, b: crate::battle::Battle, title: String, ctx: &egui::Context) {
        let ids: Vec<i64> = b
            .engagements
            .iter()
            .flat_map(|e| {
                let mut v = vec![e.victim_ship];
                v.extend(e.attackers.iter().map(|a| a.ship));
                v
            })
            .filter(|&id| id != 0)
            .collect();
        self.ensure_type_names(&ids, ctx);
        let inv = b.involvement();
        let rosters: Vec<Vec<crate::battle::Participant>> =
            (0..b.sides.len()).map(|i| b.roster(i)).collect();
        self.loaded_report = Some(LoadedReport {
            title,
            battle: b,
            inv,
            rosters,
            sorted: Vec::new(),
            condensed_rows: Vec::new(),
            sorted_for: None,
            hover: None,
        });
        self.report_msg = None;
    }

    fn poll_build_from_kill(&mut self, ctx: &egui::Context) {
        let done = {
            let mut g = self.build_from_kill.lock().unwrap();
            match &*g {
                crate::zkill::BuildFromKill::Done(..) | crate::zkill::BuildFromKill::Failed(..) => {
                    Some(std::mem::replace(&mut *g, crate::zkill::BuildFromKill::Idle))
                }
                _ => None,
            }
        };
        match done {
            Some(crate::zkill::BuildFromKill::Done(engs, _seed)) => {
                let b = crate::battle::preview_battle(engs, self.settings.battle_break_secs);
                let title = b
                    .systems
                    .first()
                    .map(|(_, n, _)| n.clone())
                    .unwrap_or_else(|| "Battle report".into());
                self.show_imported_report(b, title, ctx);
                self.build_kill_input.clear();
                self.build_kill_error = None;
            }
            Some(crate::zkill::BuildFromKill::Failed(msg)) => self.build_kill_error = Some(msg),
            _ => {}
        }
    }

    fn loaded_report_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let Some(lr) = self.loaded_report.as_ref() else { return };
        let mut go_back = false;
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Back to battles", icon::ARROW_LEFT)).clicked() {
                go_back = true;
            }
            ui.separator();
            ui.label(egui::RichText::new(&lr.title).strong());
            ui.label(
                egui::RichText::new(format!("{}  Imported", icon::DOWNLOAD_SIMPLE))
                    .color(crate::theme::standing::WARNING),
            );
            ui.separator();
            ui.checkbox(&mut self.battle_condensed, "Condensed");
        });
        if go_back {
            self.loaded_report = None;
            return;
        }
        ui.add_space(6.0);
        let condensed = self.battle_condensed;
        let sort = self.battle_roster_sort;
        // A single static report: (re)sort only when the toggle changes, then render pre-sorted.
        if let Some(lr) = self.loaded_report.as_mut() {
            if lr.sorted_for != Some((sort, condensed)) {
                let type_names = self.type_names.lock().unwrap();
                let (sorted, cond) =
                    crate::brview::sorted_detail(&lr.rosters, sort, &self.ship_sizes, &type_names);
                lr.sorted = sorted;
                lr.condensed_rows = cond;
                lr.sorted_for = Some((sort, condensed));
            }
        }
        let prev_hover = self.loaded_report.as_ref().and_then(|lr| lr.hover);
        let (clicked_system, hover) = {
            let lr = self.loaded_report.as_ref().unwrap();
            let type_names = self.type_names.lock().unwrap();
            battle_detail(
                ui,
                &lr.battle,
                &type_names,
                &lr.inv,
                &lr.sorted,
                &lr.condensed_rows,
                condensed,
                prev_hover,
            )
        };
        if hover != prev_hover {
            if let Some(lr) = self.loaded_report.as_mut() {
                lr.hover = hover;
            }
            ui.ctx().request_repaint();
        }
        if let Some(sid) = clicked_system {
            self.open_system(sid);
        }
    }

    fn battles_view(&mut self, ui: &mut egui::Ui) {
        self.my_shared_window(&ui.ctx().clone());
        self.poll_build_from_kill(&ui.ctx().clone());
        if self.loaded_report.is_some() {
            self.loaded_report_view(ui);
            return;
        }
        if !self.settings.battles_enabled {
            ui.add_space(10.0);
            if ui
                .checkbox(&mut self.settings.battles_enabled, "Enable battle reports")
                .on_hover_text(
                    "Generate and compute battle reports from the zKill feed. \
                     While off, no battles are clustered or computed.",
                )
                .changed()
            {
                self.battles_enabled_shared
                    .store(self.settings.battles_enabled, std::sync::atomic::Ordering::Relaxed);
                self.needs_save = true;
                ui.ctx().request_repaint();
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Battle reports are off. No battles are generated or computed. \
                     Gate-camp warnings and the kill feed keep working.",
                )
                .weak(),
            );
            return;
        }
        ui.add_space(10.0);
        let now = chrono::Utc::now().timestamp();
        let source = if self.show_history { self.battle_history.clone() } else { self.battles.clone() };

        if let Some(kid) = self.battle_selected {
            let exists = source
                .lock()
                .unwrap()
                .iter()
                .any(|b| b.engagements.iter().any(|e| e.kill_id == kid));
            match exists {
                false => {
                    self.battle_selected = None;
                    self.battle_detail_cache = None;
                }
                true => {
                    // The brview worker builds the detail off-thread; mirror it into the cache only
                    // when the worker publishes new output or the selection changed — never a
                    // per-frame clone of the (heavy) battle.
                    let need = self.battle_detail_cache.as_ref().map(|c| c.kid) != Some(kid)
                        || self.battle_detail_out_sig != self.br_outputs.lock().unwrap().sig;
                    if need {
                        let out = self.br_outputs.lock().unwrap();
                        self.battle_detail_out_sig = out.sig;
                        let fresh = out.detail.as_ref().is_some_and(|d| d.kid == kid);
                        if fresh {
                            let d = out.detail.clone();
                            drop(out);
                            if let Some(d) = &d {
                                self.ensure_type_names(&d.ship_ids, ui.ctx());
                            }
                            self.battle_detail_cache = d;
                        } else if self.battle_detail_cache.as_ref().map(|c| c.kid) != Some(kid) {
                            // Worker hasn't produced this battle yet and we have nothing for it.
                            // Render nothing (no spinner) — the worker repaints when it's ready.
                            self.battle_detail_cache = None;
                        }
                    }
                    if self.battle_detail_cache.is_some() {
                        use egui_phosphor::regular as icon;
                        let ambiguous =
                            self.battle_detail_cache.as_ref().map(|c| c.battle.ambiguous).unwrap_or(false);
                        let excl_n = self.battle_excluded_count;
                        let scrub_n = self.battle_scrub_count;
                        let mut go_back = false;
                        let mut save_clicked = false;
                        let mut share_clicked = false;
                        let mut mine_clicked = false;
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(format!("{}  Back to battles", icon::ARROW_LEFT))
                                .clicked()
                            {
                                go_back = true;
                            }
                            toolbar_sep(ui);
                            ui.toggle_value(&mut self.battle_edit_mode, format!("{}  Edit", icon::PENCIL))
                                .on_hover_text("Split off kills, remove kills/pilots, add a kill");
                            toolbar_sep(ui);
                            ui.checkbox(&mut self.battle_condensed, "Condensed")
                                .on_hover_text("Stack each side's ships by hull (count + losses)");
                            toolbar_sep(ui);
                            ui.label("Sort");
                            egui::ComboBox::from_id_salt("battle_roster_sort")
                                .selected_text(match self.battle_roster_sort {
                                    RosterSort::Value => "ISK loss",
                                    RosterSort::Hull => "Hull size",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.battle_roster_sort,
                                        RosterSort::Value,
                                        "ISK loss",
                                    );
                                    ui.selectable_value(
                                        &mut self.battle_roster_sort,
                                        RosterSort::Hull,
                                        "Hull size",
                                    );
                                });
                            toolbar_sep(ui);
                            if ui.button(format!("{}  Add kill", icon::PLUS)).clicked() {
                                self.battle_add_open = true;
                            }
                            if ui.button(format!("{} Excluded ({excl_n})", icon::TRASH)).clicked() {
                                self.battle_excluded_open = true;
                            }
                            if ui.button(format!("{} Scrubbed ({scrub_n})", icon::BROOM)).clicked() {
                                self.battle_scrubs_open = true;
                            }
                            toolbar_sep(ui);
                            if ui
                                .button(format!("{}  Save JSON", icon::FLOPPY_DISK))
                                .on_hover_text("Save this battle report as a JSON file you can re-open or share")
                                .clicked()
                            {
                                save_clicked = true;
                            }
                            toolbar_sep(ui);
                            let authed = self.br_authed_chars();
                            if authed.len() > 1 {
                                let current = self.share_identity().map(|(id, _)| id);
                                let sel_name = current
                                    .and_then(|id| {
                                        authed.iter().find(|(a, _)| *a == id).map(|(_, n)| n.clone())
                                    })
                                    .unwrap_or_else(|| "Select character".to_owned());
                                ui.label("Manage as:");
                                egui::ComboBox::from_id_salt("br_manage_as")
                                    .selected_text(sel_name)
                                    .show_ui(ui, |ui| {
                                        for (id, name) in &authed {
                                            if ui
                                                .selectable_label(self.br_character == Some(*id), name)
                                                .clicked()
                                            {
                                                self.br_character = Some(*id);
                                            }
                                        }
                                    })
                                    .response
                                    .on_hover_text("Battle reports are owned per character; pick which one to upload + manage under");
                                toolbar_sep(ui);
                            }
                            let sharing = matches!(
                                *self.br_share.lock().unwrap(),
                                crate::brshare::ShareStatus::Uploading
                            );
                            if ui
                                .add_enabled(
                                    !sharing,
                                    egui::Button::new(format!("{}  Share to eve-spai.com", icon::SHARE_NETWORK)),
                                )
                                .on_hover_text("Upload this battle report to eve-spai.com and get a shareable link")
                                .clicked()
                            {
                                share_clicked = true;
                            }
                            ui.checkbox(&mut self.br_unlisted, "Unlisted")
                                .on_hover_text("Don't list it in the public directory (reachable only by link)");
                            if ui
                                .button(format!("{}  My shared BRs", icon::GLOBE))
                                .on_hover_text("List and manage the reports you've shared")
                                .clicked()
                            {
                                mine_clicked = true;
                            }
                        });
                        if share_clicked {
                            if let Some(b) = self.battle_detail_cache.as_ref().map(|c| c.battle.clone()) {
                                let ctx = ui.ctx().clone();
                                self.br_share_kid = self.battle_selected;
                                self.start_share(&b, &ctx);
                            }
                        }
                        if mine_clicked {
                            let ctx = ui.ctx().clone();
                            self.open_my_shared(&ctx);
                        }
                        if self.battle_selected == self.br_share_kid {
                            self.share_status_ui(ui);
                        }
                        if save_clicked {
                            if let Some(b) = self.battle_detail_cache.as_ref().map(|c| c.battle.clone()) {
                                self.report_msg = match self.save_battle_report(&b) {
                                    Ok(Some(path)) => Some(format!("Saved report to {}", path.display())),
                                    Ok(None) => None,
                                    Err(e) => Some(format!("Could not save report: {e}")),
                                };
                            }
                        }
                        if let Some(msg) = self.report_msg.clone() {
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new(msg).weak());
                        }
                        if go_back {
                            self.battle_selected = None;
                            self.battle_hover = None;
                            self.battle_detail_cache = None;
                            self.battle_edit_mode = false;
                            self.battle_kill_sel.clear();
                            return;
                        }
                        ui.add_space(6.0);
                        self.battle_review_panels(ui.ctx());
                        if ambiguous && !self.battle_edit_mode {
                            egui::Frame::new()
                                .fill(crate::theme::standing::WARNING.gamma_multiply(0.14))
                                .inner_margin(egui::Margin::symmetric(8, 5))
                                .corner_radius(4.0)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}  Possible separate engagements",
                                                icon::WARNING
                                            ))
                                            .color(crate::theme::standing::WARNING)
                                            .strong(),
                                        );
                                        if ui.button(format!("{} Review / split", icon::SCISSORS)).clicked() {
                                            self.battle_edit_mode = true;
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        }
                        if self.battle_edit_mode {
                            self.battle_edit_view(ui, now);
                            return;
                        }
                        let prev_hover = self.battle_hover;
                        let condensed = self.battle_condensed;
                        let cache = self.battle_detail_cache.as_ref().unwrap();
                        // Snapshot only the type names this battle needs, so the render loop
                        // (hundreds of rows) does not hold the shared type_names lock and stall
                        // the brview worker. 670 = the default capsule pod fallback.
                        let names: std::collections::HashMap<i64, String> = {
                            let t = self.type_names.lock().unwrap();
                            cache
                                .ship_ids
                                .iter()
                                .chain(std::iter::once(&670))
                                .filter_map(|id| t.get(id).map(|n| (*id, n.clone())))
                                .collect()
                        };
                        let (clicked_system, hover) = battle_detail(
                            ui,
                            &cache.battle,
                            &names,
                            &cache.inv,
                            &cache.rosters,
                            &cache.condensed,
                            condensed,
                            prev_hover,
                        );
                        if hover != prev_hover {
                            self.battle_hover = hover;
                            ui.ctx().request_repaint();
                        }
                        if let Some(sid) = clicked_system {
                            self.open_system(sid);
                        }
                        return;
                    }
                }
            }
        }

        if self.chat_dir.is_none() && self.settings.intel_channels.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Battle reports cluster killmails near systems seen in intel. \
                     Configure intel channels (Settings) so there's an area to watch.",
                )
                .weak(),
            );
        }

        let mut to_load: Option<std::path::PathBuf> = None;
        let mut open_my_shared = false;
        let mut do_build = false;
        let building =
            matches!(*self.build_from_kill.lock().unwrap(), crate::zkill::BuildFromKill::Loading);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add(
                egui::TextEdit::singleline(&mut self.battle_search)
                    .hint_text("Filter by system, alliance, pilot…")
                    .desired_width(240.0),
            );
            if !self.battle_search.is_empty() && ui.button("Clear").clicked() {
                self.battle_search.clear();
            }
            toolbar_sep(ui);
            ui.label("\u{2265} ISK").on_hover_text("Only list battles whose total ISK destroyed is at least this many billions");
            let mut bn = self.settings.min_battle_isk / 1e9;
            if ui
                .add(
                    egui::DragValue::new(&mut bn)
                        .range(0.0..=100_000.0)
                        .speed(0.5)
                        .custom_formatter(|n, _| if n == 0.0 { "off".to_owned() } else { format!("{n:.0}B") }),
                )
                .changed()
            {
                self.settings.min_battle_isk = (bn * 1e9).max(0.0);
                self.needs_save = true;
            }
            toolbar_sep(ui);
            ui.label("Split gap (min)")
                .on_hover_text("Auto-split a battle when there's a lull longer than this.");
            let mut mins = (self.settings.battle_break_secs / 60).clamp(1, 30);
            if ui
                .add(egui::DragValue::new(&mut mins).range(1..=30).speed(0.2))
                .on_hover_text("Auto-split a battle when there's a lull longer than this.")
                .changed()
            {
                let secs = mins.clamp(1, 30) * 60;
                self.settings.battle_break_secs = secs;
                self.needs_save = true;
                self.battle_break_shared.store(secs, std::sync::atomic::Ordering::Relaxed);
            }
            if ui.checkbox(&mut self.show_history, "Full history").changed() {
                self.battle_selected = None;
                if self.show_history {
                    self.load_battle_history(ui.ctx());
                }
            }
            if ui.button(format!("{}  Rules…", egui_phosphor::regular::FUNNEL)).clicked() {
                self.battle_filter_open = true;
            }
            toolbar_sep(ui);
            if ui
                .button(format!("{}  Open JSON", egui_phosphor::regular::FOLDER_OPEN))
                .on_hover_text("Open a saved battle-report JSON file")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("EVE Spai battle report", &["json"])
                    .pick_file()
                {
                    to_load = Some(path);
                }
            }
            if ui
                .button(format!("{}  My shared BRs", egui_phosphor::regular::GLOBE))
                .on_hover_text("List and manage the reports you've shared to eve-spai.com")
                .clicked()
            {
                open_my_shared = true;
            }
            toolbar_sep(ui);
            let input = ui.add_enabled(
                !building,
                egui::TextEdit::singleline(&mut self.build_kill_input)
                    .hint_text("Paste a zKill kill link or id")
                    .desired_width(220.0),
            );
            let submit =
                input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !building;
            if ui
                .add_enabled(
                    !building,
                    egui::Button::new(format!(
                        "{}  Build from kill",
                        egui_phosphor::regular::HAMMER
                    )),
                )
                .on_hover_text("Build a battle report from one zKillboard kill link or id")
                .clicked()
                || submit
            {
                do_build = true;
            }
            if building {
                ui.add(egui::Spinner::new());
            }
            let mut th = self.settings.work_throttle;
            egui::ComboBox::from_id_salt("work_throttle")
                .selected_text(format!("{}  {}", egui_phosphor::regular::GAUGE, th.label()))
                .show_ui(ui, |ui| {
                    for opt in crate::settings::WorkThrottle::CHOICES {
                        ui.selectable_value(&mut th, opt, opt.label());
                    }
                })
                .response
                .on_hover_text("Throttle background work (battle feed + clustering) to limit CPU.");
            if th != self.settings.work_throttle {
                self.settings.work_throttle = th;
                self.work_throttle_shared.store(th.as_u8(), std::sync::atomic::Ordering::Relaxed);
                self.needs_save = true;
            }
            toolbar_sep(ui);
            if ui
                .checkbox(&mut self.settings.battles_enabled, "Enabled")
                .on_hover_text(
                    "Generate and compute battle reports. Turn off to stop all \
                     battle-report computation.",
                )
                .changed()
            {
                self.battles_enabled_shared
                    .store(self.settings.battles_enabled, std::sync::atomic::Ordering::Relaxed);
                self.needs_save = true;
            }
        });
        if open_my_shared {
            self.open_my_shared(&ui.ctx().clone());
        }
        if do_build {
            match crate::zkill::parse_kill_id(&self.build_kill_input) {
                None => {
                    self.build_kill_error = Some("Not a valid zKill kill link or id".to_owned());
                }
                Some(id) => {
                    self.build_kill_error = None;
                    if let (Some(systems), Some(ship_ids)) =
                        (self.systems.clone(), self.battle_ship_ids.clone())
                    {
                        crate::zkill::spawn_build_from_kill(
                            id,
                            systems,
                            ship_ids,
                            self.build_from_kill.clone(),
                            ui.ctx().clone(),
                        );
                    } else {
                        self.build_kill_error =
                            Some("Ship data is still loading, try again in a moment".to_owned());
                    }
                }
            }
        }
        if let Some(err) = self.build_kill_error.clone() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(err).color(crate::theme::standing::WARNING));
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    self.build_kill_error = None;
                }
            });
        }
        if let Some(path) = to_load {
            self.load_battle_report(&path, ui.ctx());
            return;
        }
        if let Some(msg) = self.report_msg.clone() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(msg).weak());
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    self.report_msg = None;
                }
            });
        }
        ui.add_space(4.0);
        let query = self.battle_search.trim().to_lowercase();
        let loading = self.battle_history_loading.load(std::sync::atomic::Ordering::Relaxed);

        // All filtering/roster work runs on the brview worker; the UI only publishes inputs and
        // reads results, showing a spinner while the worker catches up.
        {
            let mut inp = self.br_inputs.lock().unwrap();
            inp.query = self.battle_search.trim().to_owned();
            inp.min_isk = self.settings.min_battle_isk;
            inp.show_history = self.show_history;
            inp.break_secs = self.settings.battle_break_secs;
            inp.player_sys = self.player_system().unwrap_or(0);
            inp.selected_kid = self.battle_selected;
            inp.sort = self.battle_roster_sort;
            inp.condensed = self.battle_condensed;
        }
        let want_sig = {
            let inp = self.br_inputs.lock().unwrap();
            crate::brview::ui_signature(
                &self.battles,
                &self.battle_history,
                &self.battle_filter_gen_shared,
                &self.battle_overrides_gen_shared,
                &self.intel_state,
                &inp,
            )
        };
        if want_sig != self.br_last_sent_sig {
            self.br_last_sent_sig = want_sig;
            crate::brview::poke(&self.br_wake);
        }
        {
            let out = self.br_outputs.lock().unwrap();
            if out.sig != self.battle_cards_out_sig {
                self.battle_cards_out_sig = out.sig;
                self.battle_cards = out.cards.clone();
                self.battle_cards_total = out.total;
                self.battle_cards_filtered = out.filtered;
                self.battle_cards_ready = out.ready;
            }
        }
        let total = self.battle_cards_total;
        let filtered = self.battle_cards_filtered;
        let ready = self.battle_cards_ready;
        let fresh = self.battle_cards_out_sig == want_sig;

        if self.battle_cards.is_empty() {
            if !ready || !fresh {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(egui::RichText::new("Loading battles…").weak());
                });
                ui.ctx().request_repaint();
                return;
            }
            let msg = if self.show_history && loading {
                "Loading full history…".to_owned()
            } else if filtered > 0 {
                format!(
                    "{} battle(s) below the {} ISK minimum.",
                    filtered,
                    fmt_isk(self.settings.min_battle_isk)
                )
            } else if self.show_history {
                "No recorded battles yet.".to_owned()
            } else if query.is_empty() {
                "No active battles near the tracked area.".to_owned()
            } else {
                "No battles match the filter.".to_owned()
            };
            ui.label(egui::RichText::new(msg).weak());
            return;
        }

        let shown_n = self.battle_cards.len();
        let count_txt = if filtered > 0 {
            format!("{total} battles ({filtered} filtered)")
        } else {
            format!("{total} battles")
        };
        let mut do_merge = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(count_txt).weak());
            ui.separator();
            ui.toggle_value(
                &mut self.battle_edit_mode,
                format!("{}  Merge", egui_phosphor::regular::ARROWS_MERGE),
            )
            .on_hover_text("Tick two or more battles to merge them into one");
            if self.battle_edit_mode {
                let n = self.battle_merge_sel.len();
                if n >= 2
                    && ui
                        .button(format!("{} Merge {n} battles", egui_phosphor::regular::ARROWS_MERGE))
                        .clicked()
                {
                    do_merge = true;
                }
                if n > 0 && ui.button("Clear").clicked() {
                    self.battle_merge_sel.clear();
                }
            }
        });
        ui.add_space(4.0);
        let mut open: Option<i64> = None;
        let edit = self.battle_edit_mode;
        let mut merge_sel = std::mem::take(&mut self.battle_merge_sel);
        let cards = &self.battle_cards;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (kid, from_you, b) in cards {
                if edit {
                    ui.horizontal(|ui| {
                        let mut on = merge_sel.contains(kid);
                        if ui.checkbox(&mut on, "").changed() {
                            if on {
                                merge_sel.insert(*kid);
                            } else {
                                merge_sel.remove(kid);
                            }
                        }
                        if battle_row(ui, b, now, *from_you) {
                            open = Some(*kid);
                        }
                    });
                } else if battle_row(ui, b, now, *from_you) {
                    open = Some(*kid);
                }
                ui.add_space(4.0);
            }
            if total > shown_n {
                ui.label(
                    egui::RichText::new(format!(
                        "Showing the newest {shown_n}. Narrow with search or rules to see the other {}.",
                        total - shown_n
                    ))
                    .weak(),
                );
            }
        });
        self.battle_merge_sel = merge_sel;
        if let Some(kid) = open {
            self.battle_selected = Some(kid);
            self.battle_edit_mode = false;
        }
        if do_merge {
            let sel = self.battle_merge_sel.clone();
            let mut kids: Vec<i64> = Vec::new();
            {
                let guard = source.lock().unwrap();
                for b in guard.iter() {
                    let rep = b.engagements.iter().map(|e| e.kill_id).max().unwrap_or(0);
                    if sel.contains(&rep) {
                        kids.extend(b.engagements.iter().map(|e| e.kill_id));
                    }
                }
            }
            let ctx = ui.ctx().clone();
            self.apply_battle_edit(&ctx, move |s| {
                let t = s.next_battle_tag();
                for kid in &kids {
                    s.set_battle_tag(*kid, Some(t));
                }
            });
            self.battle_merge_sel.clear();
            self.battle_edit_mode = false;
        }
    }

    fn refresh_characters(&mut self) {
        if let Some(store) = &self.store {
            self.characters = store.list_characters();
        }
        if self.active_character == "No character" {
            if let Some(first) = self.characters.first() {
                self.active_character = first.name.clone();
            }
        }
    }

    fn char_missing_scope(&self, name: &str, scope: &str) -> bool {
        self.characters
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| !c.scopes.split(' ').any(|s| s == scope))
            .unwrap_or(false)
    }

    fn start_login(&self, ctx: &egui::Context) {
        let client_id = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        let callback = non_empty_or(&self.settings.sso_callback, auth::DEFAULT_CALLBACK);
        let scopes = auth::DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect();
        if let Some(store) = &self.store {
            auth::spawn_login(
                client_id,
                callback,
                scopes,
                store.path().to_path_buf(),
                self.auth_status.clone(),
                ctx.clone(),
            );
        }
    }

    fn characters_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        match self.auth_status.lock().unwrap().clone() {
            AuthStatus::Waiting(msg) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(msg);
                });
            }
            AuthStatus::Success(name) => {
                ui.colored_label(
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
                    format!("Logged in as {name}"),
                );
            }
            AuthStatus::Failed(err) => {
                ui.colored_label(crate::theme::standing::WARNING, format!("Login failed: {err}"));
            }
            AuthStatus::Idle => {}
        }

        ui.add_space(6.0);
        if ui.button("Add character (EVE SSO)").clicked() {
            self.start_login(&ui.ctx().clone());
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);

        if self.characters.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No characters yet. Click \"Add character\" to log in with EVE SSO.",
                )
                .weak(),
            );
            return;
        }

        let now = chrono::Utc::now().timestamp();
        let mut remove: Option<i64> = None;
        let mut toggle: Option<(String, bool)> = None;
        let mut reauth = false;
        for c in &self.characters {
            let have: std::collections::HashSet<&str> = c.scopes.split(' ').collect();
            let scope_count = have.iter().filter(|s| !s.is_empty()).count();
            let missing: Vec<&str> =
                auth::DEFAULT_SCOPES.iter().copied().filter(|s| !have.contains(s)).collect();
            let token_ok = c.expires_at > now;
            let mut intel_on =
                !self.settings.intel_disabled_chars.iter().any(|d| d.eq_ignore_ascii_case(&c.name));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&c.name).strong());
                ui.label(egui::RichText::new(format!("· {scope_count} scopes")).weak());
                let (col, txt) = if token_ok {
                    (egui::Color32::from_rgb(0x5A, 0xC8, 0x6A), "token valid")
                } else {
                    (crate::theme::standing::WARNING, "token expired")
                };
                ui.label(egui::RichText::new("·").weak());
                ui.label(egui::RichText::new(txt).color(col));
                if !missing.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("{} missing scopes", egui_phosphor::regular::WARNING))
                            .color(crate::theme::standing::WARNING),
                    )
                    .on_hover_text(format!("Re-auth to grant: {}", missing.join(", ")));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(c.id);
                    }
                    if !missing.is_empty()
                        && ui
                            .small_button("Re-auth")
                            .on_hover_text("Log in again to grant the new scopes")
                            .clicked()
                    {
                        reauth = true;
                    }
                    if ui
                        .checkbox(&mut intel_on, "Alert")
                        .on_hover_text("Raise intel alerts while this character is active")
                        .changed()
                    {
                        toggle = Some((c.name.clone(), intel_on));
                    }
                });
            });
        }
        if reauth {
            self.start_login(&ui.ctx().clone());
        }
        if let Some((name, on)) = toggle {
            self.settings.intel_disabled_chars.retain(|d| !d.eq_ignore_ascii_case(&name));
            if !on {
                self.settings.intel_disabled_chars.push(name);
            }
            self.needs_save = true;
        }
        if let Some(id) = remove {
            if let Some(store) = &self.store {
                let _ = store.remove_character(id);
            }
            self.refresh_characters();
        }
    }

    fn ship_roles_cached(&self, id: i64) -> Vec<(&'static str, &'static str)> {
        match self.store.as_ref() {
            Some(s) => ship_roles_cached(s, &self.ship_roles_cache, id),
            None => Vec::new(),
        }
    }

    fn ship_details_cached(&self, id: i64) -> Option<crate::store::ShipDetails> {
        match self.store.as_ref() {
            Some(s) => ship_details_cached(s, &self.ship_cache, id),
            None => None,
        }
    }

    fn pilot_window(&mut self, ctx: &egui::Context) {
        use crate::lookup::LookupState;
        if !self.pilot_window_open {
            return;
        }
        let keep = Self::dialog_viewport(ctx, "pilot_window", "EVE Spai - Pilot", [420.0, 560.0], |ui| {
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.pilot_query)
                        .hint_text("Character name")
                        .desired_width(200.0),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Look up").clicked() || enter {
                    crate::lookup::spawn_lookup(
                        self.pilot_query.clone(),
                        self.pilot_lookup.clone(),
                        ui.ctx().clone(),
                    );
                }
            });
            ui.separator();

            let state = self.pilot_lookup.lock().unwrap().clone();
            match state {
                LookupState::Idle => {
                    ui.label(egui::RichText::new("Enter a pilot name.").weak());
                }
                LookupState::Loading(n) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Looking up {n}…"));
                    });
                }
                LookupState::Failed(e) => {
                    ui.colored_label(crate::theme::standing::WARNING, e);
                }
                LookupState::Done(report) => self.pilot_report_ui(ui, &report),
            }
        });
        if !keep {
            self.pilot_window_open = false;
        }
    }

    /// `show_system` is off for a list already scoped to one system, where naming it on every row is
    /// just noise.
    fn km_list(
        &mut self,
        ui: &mut egui::Ui,
        list: &[crate::lookup::Loss],
        loading: bool,
        show_system: bool,
    ) {
        if list.is_empty() {
            let msg = if loading { "Loading\u{2026}" } else { "Nothing in this category." };
            ui.label(egui::RichText::new(msg).weak());
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let mut clicked: Option<crate::lookup::Loss> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for l in list {
                let det = self.ship_details_cached(l.ship_type_id);
                let skip = det.as_ref().is_some_and(|d| {
                    d.group == "Capsule"
                        || d.group == "Corvette"
                        || matches!(
                            d.name.as_str(),
                            "Caldari Shuttle" | "Gallente Shuttle" | "Amarr Shuttle" | "Minmatar Shuttle"
                        )
                });
                if skip {
                    continue;
                }
                ui.horizontal(|ui| {
                    let url = eve_type_icon_url(l.ship_type_id, 26.0);
                    let img = ui.add(
                        egui::Image::new(url)
                            .fit_to_exact_size(egui::Vec2::splat(26.0))
                            .sense(egui::Sense::click()),
                    );
                    let ship = det.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "?".to_owned());
                    let age = now - l.time;
                    let age_s = if age < 3600 {
                        format!("{}m", age / 60)
                    } else if age < 86_400 {
                        format!("{}h", age / 3600)
                    } else {
                        format!("{}d", age / 86_400)
                    };
                    // The fixed-width tail is laid out from the right, so the ship name gets whatever
                    // is left and truncates. Left to itself, a long name sets the row's minimum width
                    // and drags the whole side panel wider as the list loads.
                    let mut hit = img.on_hover_text("Show fit").clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("\u{2197}").on_hover_text("Open on zKillboard").clicked() {
                            let _ =
                                open::that(format!("https://zkillboard.com/kill/{}/", l.killmail_id));
                        }
                        ui.label(egui::RichText::new(age_s).weak());
                        if l.value > 0.0 {
                            let isk = if l.value >= 1e9 {
                                format!("{:.1}B", l.value / 1e9)
                            } else {
                                format!("{:.0}M", l.value / 1e6)
                            };
                            ui.label(isk);
                        }
                        if show_system {
                            if let Some(sys) =
                                self.systems.as_ref().and_then(|g| g.info_of(l.system_id))
                            {
                                ui.label(egui::RichText::new(&sys.name).weak());
                            }
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let name = ui.add(
                                egui::Label::new(egui::RichText::new(&ship).strong())
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            );
                            hit |= name.on_hover_text(&ship).clicked();
                        });
                    });
                    if hit {
                        clicked = Some(l.clone());
                    }
                });
            }
        });
        if let Some(l) = clicked {
            self.fit_loss = Some(l);
        }
    }

    fn pilot_report_ui(&mut self, ui: &mut egui::Ui, report: &crate::lookup::PilotReport) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&report.name).strong());
            if ui.button("zKillboard").clicked() {
                let _ = open::that(format!("https://zkillboard.com/character/{}/", report.character_id));
            }
            if report.loading {
                ui.spinner();
                ui.label(egui::RichText::new("loading\u{2026}").weak());
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Overview, "Overview");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Kills, format!("Kills ({})", report.kills.len()));
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Solo, format!("Solo ({})", report.solo.len()));
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Losses, format!("Losses ({})", report.losses.len()));
        });
        ui.separator();
        match self.pilot_tab {
            PilotTab::Kills => return self.km_list(ui, &report.kills, report.loading, true),
            PilotTab::Solo => return self.km_list(ui, &report.solo, report.loading, true),
            PilotTab::Losses => return self.km_list(ui, &report.losses, report.loading, true),
            PilotTab::Overview => {}
        }
        ui.horizontal(|ui| {
            ui.label("Sort:");
            ui.selectable_value(&mut self.pilot_sort, PilotSort::MostLost, "Most lost");
            ui.selectable_value(&mut self.pilot_sort, PilotSort::Recent, "Recent");
        });

        let mut agg: std::collections::HashMap<i64, (u32, i64)> = std::collections::HashMap::new();
        for l in &report.losses {
            let skip = self
                .ship_details_cached(l.ship_type_id)
                .map(|d| matches!(d.group.as_str(), "Capsule" | "Corvette" | "Shuttle"))
                .unwrap_or(false);
            if skip {
                continue;
            }
            let e = agg.entry(l.ship_type_id).or_insert((0, 0));
            e.0 += 1;
            e.1 = e.1.max(l.time);
        }
        let mut ships: Vec<(i64, u32, i64)> = agg.into_iter().map(|(id, (c, t))| (id, c, t)).collect();
        match self.pilot_sort {
            PilotSort::MostLost => ships.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2))),
            PilotSort::Recent => ships.sort_by(|a, b| b.2.cmp(&a.2)),
        }

        ui.add_space(4.0);
        if ships.is_empty() {
            ui.label(egui::RichText::new("No relevant losses.").weak());
            return;
        }
        egui::ScrollArea::vertical().id_salt("pilot_ships").auto_shrink([false, false]).show(ui, |ui| {
            for (ship_id, count, _) in ships {
                let name = self
                    .ship_details_cached(ship_id)
                    .map(|d| d.name)
                    .unwrap_or_else(|| "Other".to_owned());
                ui.horizontal(|ui| {
                    let url = eve_type_icon_url(ship_id, 24.0);
                    ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(24.0)));
                    if ui
                        .add(egui::Button::new(format!("{name}  ×{count}")).frame(false))
                        .on_hover_text("View fits")
                        .clicked()
                    {
                        self.fit_view = Some((ship_id, FitMode::Recent));
                    }
                });
            }
        });
    }

    fn ensure_type_names(&self, ids: &[i64], ctx: &egui::Context) {
        let missing: Vec<i64> = {
            let names = self.type_names.lock().unwrap();
            ids.iter().copied().filter(|id| !names.contains_key(id)).collect()
        };
        if missing.is_empty() {
            return;
        }
        {
            let mut loading = self.type_names_loading.lock().unwrap();
            if *loading {
                return;
            }
            *loading = true;
        }
        let cache = self.type_names.clone();
        let loading = self.type_names_loading.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let resolved = crate::lookup::resolve_type_names(&missing);
            cache.lock().unwrap().extend(resolved);
            *loading.lock().unwrap() = false;
            ctx.request_repaint();
        });
    }

    fn fit_window(&mut self, ctx: &egui::Context) {
        let (loss, ship_id, mode, has_mode) = if let Some(l) = self.fit_loss.clone() {
            let sid = l.ship_type_id;
            (Some(l), sid, FitMode::Recent, false)
        } else if let Some((ship_id, mode)) = self.fit_view {
            let l = {
                let state = self.pilot_lookup.lock().unwrap();
                match &*state {
                    crate::lookup::LookupState::Done(report) => pick_loss(report, ship_id, mode),
                    _ => None,
                }
            };
            (l, ship_id, mode, true)
        } else {
            return;
        };
        if let Some(l) = &loss {
            let mut ids: Vec<i64> = l.items.iter().map(|i| i.type_id).collect();
            ids.push(ship_id);
            self.ensure_type_names(&ids, ctx);
        }
        let ship_name = self.ship_details_cached(ship_id).map(|d| d.name).unwrap_or_default();
        let names = self.type_names.lock().unwrap().clone();
        let mut new_mode = mode;

        let keep = Self::dialog_viewport(ctx, "fit_window", "EVE Spai - Fit", [460.0, 620.0], |ui| {
            ui.horizontal(|ui| {
                let url = eve_type_icon_url(ship_id, 28.0);
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(28.0)));
                ui.heading(&ship_name);
            });
            if has_mode {
                ui.horizontal(|ui| {
                    ui.label("Fit:");
                    ui.selectable_value(&mut new_mode, FitMode::Recent, "Most recent");
                    ui.selectable_value(&mut new_mode, FitMode::MostUsed, "Most used");
                });
            }
            ui.separator();
            let Some(loss) = &loss else {
                ui.label(egui::RichText::new("No fit found.").weak());
                return;
            };

            egui::ScrollArea::vertical().max_height(330.0).auto_shrink([false, false]).show(ui, |ui| {
                use crate::lookup::Slot;
                let cargo = fit_cargo(loss);
                let section = |ui: &mut egui::Ui, title: &str, slot: Slot| {
                    let mods: Vec<&crate::lookup::Item> = loss
                        .items
                        .iter()
                        .filter(|i| crate::lookup::slot_of(i.flag) == slot && i.qty == 1)
                        .collect();
                    if mods.is_empty() {
                        return;
                    }
                    ui.label(egui::RichText::new(title).strong().color(ui.visuals().hyperlink_color));
                    for it in mods {
                        ui.label(names.get(&it.type_id).cloned().unwrap_or_else(|| "…".to_owned()));
                    }
                    ui.add_space(4.0);
                };
                section(ui, "High", Slot::High);
                section(ui, "Mid", Slot::Mid);
                section(ui, "Low", Slot::Low);
                section(ui, "Rigs", Slot::Rig);
                section(ui, "Subsystems", Slot::Subsystem);
                if !cargo.is_empty() {
                    ui.label(
                        egui::RichText::new("Cargo & drones").strong().color(ui.visuals().hyperlink_color),
                    );
                    for (tid, q) in &cargo {
                        let n = names.get(tid).cloned().unwrap_or_else(|| "…".to_owned());
                        if *q > 1 {
                            ui.label(format!("{n}  ×{q}"));
                        } else {
                            ui.label(n);
                        }
                    }
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Copy EFT").clicked() {
                    ui.ctx().copy_text(eft_string(&ship_name, loss, &names));
                }
                let has_char = self.active_character != "No character";
                ui.add_enabled_ui(has_char, |ui| {
                    if ui.button("Save Fit").on_hover_text("Save to your in-game fittings").clicked() {
                        use crate::lookup::Slot;
                        let mut items: Vec<(i64, i64, i64)> = loss
                            .items
                            .iter()
                            .filter(|i| {
                                !matches!(crate::lookup::slot_of(i.flag), Slot::Cargo | Slot::Other)
                                    && i.qty == 1
                            })
                            .map(|i| (i.type_id, i.flag, 1))
                            .collect();
                        for (tid, q) in fit_cargo(loss) {
                            items.push((tid, 5, q)); // flag 5 = cargo
                        }
                        let cid =
                            non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID).to_owned();
                        crate::esi::save_fitting(
                            cid,
                            self.active_character.clone(),
                            format!("{}'s {ship_name} Fit", self.active_character),
                            ship_id,
                            items,
                        );
                    }
                });
                let site = self.settings.fit_site.clone();
                if site.is_empty() {
                    ui.label("Open in:");
                    for (id, label) in FIT_SITES {
                        if ui.button(*label).clicked() {
                            self.settings.fit_site = (*id).to_owned();
                            self.needs_save = true;
                        }
                    }
                } else if ui.button(format!("Open in {}", site_label(&site))).clicked() {
                    let _ = open::that(fit_url(&site, ship_id, loss));
                }
            });
        });

        if has_mode && new_mode != mode {
            self.fit_view = Some((ship_id, new_mode));
        } else if !keep {
            self.fit_view = None;
            self.fit_loss = None;
        }
    }

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
        let uncertain: std::collections::HashSet<String> = if feed.is_empty() {
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
                st.win_pos = self.settings.alerts.window_pos;
                st.win_size = self.settings.alerts.window_size;
                st.feed = feed;
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
            // Save a moved position / resized size — but NOT on the open frame, where the window
            // briefly reports its builder default before the saved geometry is re-applied.
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

    fn act_on_intel_click(&mut self, click: IntelClick, ctx: &egui::Context) {
        match click {
            IntelClick::System(id) => self.open_system(id),
            IntelClick::Ship(id) => self.open_ship(id),
            IntelClick::Pilot(name) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(
                    self.pilot_query.clone(),
                    self.pilot_lookup.clone(),
                    ctx.clone(),
                );
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            IntelClick::Dscan(url) => self.open_dscan(url, ctx),
            IntelClick::PilotVerdict(name) => self.open_pilot_verdict(name),
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

    fn maybe_rebuild_graph(&mut self, ctx: &egui::Context) {
        if self.systems.is_none() || self.settings.jump_bridges == self.bridges_applied {
            return;
        }
        let Some(store) = &self.store else { return };
        let mut systems = store.load_systems();
        let bridges: Vec<(i64, i64)> = self
            .settings
            .jump_bridges
            .iter()
            .filter_map(|b| Some((systems.lookup(&b.from)?.id, systems.lookup(&b.to)?.id)))
            .collect();
        systems.add_bridges(&bridges);
        self.systems = Some(std::sync::Arc::new(systems));
        self.bridges_applied = self.settings.jump_bridges.clone();
        self.map_loaded = None;
        self.map_draw_key = None;
        self.map_systems_cache.clear();
        self.map_draw_cache.clear();
        ctx.request_repaint();
    }

    fn map_view(&mut self, ui: &mut egui::Ui) {
        let status = self.sde_status.lock().unwrap().clone();
        match status {
            SdeStatus::Ready => {
                if self.map_popped {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Map is in its own window.").weak());
                    if ui.button("Dock map").clicked() {
                        self.map_popped = false;
                    }
                } else {
                    self.map_area(ui);
                }
            }
            SdeStatus::Downloading(msg) => {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(msg);
                });
            }
            SdeStatus::NotReady => {
                ui.add_space(10.0);
                ui.label("Static data has not been downloaded yet.");
                if ui.button("Download static data").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
            SdeStatus::Failed(err) => {
                ui.add_space(10.0);
                ui.colored_label(crate::theme::standing::WARNING, format!("SDE download failed: {err}"));
                if ui.button("Retry").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
        }
    }

    fn set_map_view(&mut self, v: crate::map::MapView) {
        self.map_view = v;
        self.map_pan = egui::Vec2::ZERO;
        self.map_zoom = 1.0;
        self.map_follow = false;
    }
    fn map_go(&mut self, v: crate::map::MapView) {
        if self.map_view == v {
            return;
        }
        self.map_history.push(self.map_view);
        self.map_forward.clear();
        self.set_map_view(v);
    }
    fn map_back(&mut self) {
        if let Some(v) = self.map_history.pop() {
            self.map_forward.push(self.map_view);
            self.set_map_view(v);
        }
    }
    fn map_forward_nav(&mut self) {
        if let Some(v) = self.map_forward.pop() {
            self.map_history.push(self.map_view);
            self.set_map_view(v);
        }
    }

    #[allow(deprecated)]
    fn draw_map(&mut self, ui: &mut egui::Ui) {
        use crate::map::MapView;
        if self.map_regions.is_empty() {
            if let Some(store) = &self.store {
                self.map_regions = store.regions();
            }
        }
        let active_char = self.active_character.clone();
        let (player_sys, char_here) = {
            let p = self.player.lock().unwrap();
            let mut here: std::collections::HashMap<i64, (u32, bool)> =
                std::collections::HashMap::new();
            for (name, (sys, _)) in &p.locations {
                let e = here.entry(*sys).or_insert((0, false));
                e.0 += 1;
                if name.eq_ignore_ascii_case(&active_char) {
                    e.1 = true;
                }
            }
            let sys = p.locations.get(&active_char).map(|(s, _)| *s).or(p.system_id);
            (sys, here)
        };
        if !self.map_initialized {
            self.map_view = MapView::Universe;
            self.map_initialized = true;
        }

        if self.map_follow {
            if let (MapView::Region(r), Some(psys)) = (self.map_view, player_sys) {
                let pr = match self.map_follow_region {
                    Some((s, reg)) if s == psys => Some(reg),
                    _ => {
                        let reg = self.store.as_ref().and_then(|s| s.region_of_system(psys));
                        if let Some(reg) = reg {
                            self.map_follow_region = Some((psys, reg));
                        }
                        reg
                    }
                };
                if let Some(pr) = pr {
                    if pr != r {
                        self.map_view = MapView::Region(pr);
                    }
                }
            }
        }

        if self.map_layout.is_threat() {
            let rect = ui.available_rect_before_wrap();
            self.map_last_rect = Some(rect);
            if self.map_overlay_mode {
                ui.set_opacity(self.settings.map_overlay_opacity.clamp(0.2, 1.0));
            }
            self.draw_threat_view(ui, rect, player_sys);
            self.map_chrome(ui, rect);
            return;
        }

        if self.map_loaded != Some(self.map_view) {
            if let Some(old) = self.map_loaded {
                self.map_systems_cache.insert(old, std::mem::take(&mut self.map_systems));
            }
            if let Some(cached) = self.map_systems_cache.remove(&self.map_view) {
                self.map_systems = cached;
            } else {
                let raw = match self.map_view {
                    MapView::Universe => self.store.as_ref().map(|s| s.all_map_systems()),
                    MapView::Region(id) => self.store.as_ref().map(|s| s.region_systems(id)),
                }
                .unwrap_or_default();
                self.map_systems = if let Some(g) = &self.systems {
                    raw.into_iter()
                        .filter(|s| !g.neighbors(s.id).is_empty())
                        .filter(|s| {
                            g.info_of(s.id).map(|i| !is_hidden_region(&i.region)).unwrap_or(true)
                        })
                        .collect()
                } else {
                    raw
                };
            }
            self.map_loaded = Some(self.map_view);
        }

        let spaced = self.map_layout == crate::map::MapLayout::Spaced;
        let want = (self.map_view, spaced);
        if self.map_draw_key != Some(want) {
            if let Some(old) = self.map_draw_key {
                self.map_draw_cache.insert(old, std::mem::take(&mut self.map_draw));
            }
            if let Some(cached) = self.map_draw_cache.remove(&want) {
                self.map_draw = cached;
            } else {
                self.map_draw = if spaced {
                    self.map_systems
                        .iter()
                        .map(|s| crate::store::MapSystem { x: s.x2d, z: s.z2d, ..s.clone() })
                        .collect()
                } else {
                    self.map_systems.clone()
                };
            }
            self.map_draw_spaced = spaced;
            self.map_draw_key = Some(want);
        }
        let schematic = self.map_draw_spaced;

        let Some(bounds) = crate::map::Bounds::of(&self.map_draw) else {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("No systems to show.").weak());
            return;
        };

        if self.map_overlay_mode {
            ui.set_opacity(self.settings.map_overlay_opacity.clamp(0.2, 1.0));
        }
        let rect = ui.available_rect_before_wrap();
        if let Some(prev) = self.map_last_rect {
            let d = prev.size() - rect.size();
            if d.x.abs() > 0.5 || d.y.abs() > 0.5 {
                let old_s = bounds.base_scale(prev, 30.0);
                let new_s = bounds.base_scale(rect, 30.0);
                if old_s > 0.0 {
                    self.map_pan *= new_s / old_s;
                }
            }
        }
        self.map_last_rect = Some(rect);
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1)) {
            self.map_back();
        }
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra2)) {
            self.map_forward_nav();
        }
        if resp.dragged() && !self.map_overlay_drag {
            self.map_pan += resp.drag_delta();
            self.map_follow = false;
        }
        if !resp.dragged() {
            self.map_overlay_drag = false;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
                    let old = self.map_zoom;
                    let new = (old * (scroll * 0.003).exp()).clamp(0.7, 60.0);
                    let q = cursor - (rect.center() + self.map_pan);
                    self.map_pan += q * (1.0 - new / old);
                    self.map_zoom = new;
                }
            }
        }
        if self.map_follow {
            if let Some(ps) = player_sys.and_then(|id| self.map_draw.iter().find(|s| s.id == id)) {
                let base = crate::map::project(ps.x, ps.z, &bounds, rect, self.map_zoom, egui::Vec2::ZERO);
                self.map_pan = rect.center() - base;
            }
        }

        let mut pos: std::collections::HashMap<i64, egui::Pos2> = std::collections::HashMap::new();
        for s in &self.map_draw {
            pos.insert(s.id, crate::map::project(s.x, s.z, &bounds, rect, self.map_zoom, self.map_pan));
        }

        if let Some(fid) = self.map_focus.take() {
            if let Some(s) = self.map_draw.iter().find(|s| s.id == fid) {
                let base = crate::map::project(s.x, s.z, &bounds, rect, self.map_zoom, egui::Vec2::ZERO);
                self.map_pan = rect.center() - base;
            }
        }

        if self.map_overlay_mode && !self.map_overlay_locked && resp.drag_started() {
            let on_obj = ui
                .input(|i| i.pointer.press_origin())
                .and_then(|p| nearest_system(p, &pos, 10.0))
                .is_some();
            if !on_obj {
                self.map_overlay_drag = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        }

        if resp.clicked() {
            if let Some(click) = ui.input(|i| i.pointer.interact_pos()) {
                match nearest_system(click, &pos, 10.0) {
                    Some(id) => {
                        self.map_selected = (self.map_selected != Some(id)).then_some(id);
                        if self.map_mode == MapMode::JumpPlan {
                            self.jump_click_edit(id);
                        } else {
                            self.dock_system(id);
                        }
                    }
                    None => self.map_selected = None,
                }
            }
        }

        if resp.secondary_clicked() {
            self.ctx_menu_system =
                ui.input(|i| i.pointer.interact_pos()).and_then(|p| nearest_system(p, &pos, 10.0));
        }
        let ctx_sys = self.ctx_menu_system;
        resp.context_menu(|ui| {
            ui.set_min_width(220.0);
            let Some(sid) = ctx_sys else {
                ui.close();
                return;
            };
            if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(sid)) {
                ui.label(egui::RichText::new(&info.name).strong());
            }
            // In travel mode the map edits the planned route, not the client: sending a waypoint to
            // the game from a planning view is not what the click looks like it does.
            if self.map_mode == MapMode::Travel {
                if ui.button("Set as Start").clicked() {
                    self.travel_set(TravelEnd::Start, sid);
                    ui.close();
                }
                if ui.button("Set as Destination").clicked() {
                    self.travel_set(TravelEnd::Dest, sid);
                    ui.close();
                }
                if ui.button("Add Waypoint").clicked() {
                    if !self.travel_waypoints.contains(&sid) {
                        self.travel_waypoints.push(sid);
                    }
                    self.travel_avoid.retain(|&a| a != sid);
                    self.plan_route();
                    ui.close();
                }
                let planned = self.travel_route.is_some() || self.travel_start.is_some();
                if planned && ui.button("Clear Route").clicked() {
                    self.clear_travel();
                    ui.close();
                }
            } else {
                let has_char = self.active_character != "No character";
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                let cname = self.active_character.clone();
                ui.add_enabled_ui(has_char, |ui| {
                    if ui.button("Set Destination").clicked() {
                        self.set_destination_esi(cid.clone(), cname.clone(), sid);
                        self.route_destination = Some(sid);
                        ui.close();
                    }
                    if ui.button("Add Waypoint").clicked() {
                        crate::esi::set_waypoint(cid.clone(), cname.clone(), sid, false);
                        ui.close();
                    }
                });
                if self.route_destination.is_some() && ui.button("Clear Route").clicked() {
                    self.route_destination = None;
                    ui.close();
                }
            }
            let holes: Vec<(i64, String)> = self
                .wh_cache
                .iter()
                .filter(|w| w.system_id == sid || w.dest_system_id == Some(sid))
                .map(|w| {
                    let far = if w.system_id == sid { w.dest_system_id } else { Some(w.system_id) };
                    let dest = far
                        .and_then(|d| self.systems.as_ref().and_then(|g| g.info_of(d)))
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| w.dest.label().to_owned());
                    let sig = w.signature.clone().unwrap_or_default();
                    let label =
                        if sig.is_empty() { dest } else { format!("{sig} \u{2192} {dest}") };
                    (w.id, label)
                })
                .collect();
            if !holes.is_empty() {
                ui.separator();
                if holes.len() == 1 {
                    if ui
                        .button(format!("Mark hole dead ({})", holes[0].1))
                        .on_hover_text("Drop this hole from the map and from routing")
                        .clicked()
                    {
                        self.kill_wormhole(holes[0].0);
                        ui.close();
                    }
                } else {
                    ui.menu_button("Mark hole dead", |ui| {
                        for (id, label) in &holes {
                            if ui.button(label).clicked() {
                                self.kill_wormhole(*id);
                                ui.close();
                            }
                        }
                    });
                }
            }
            ui.separator();
            if ui.button("Plan Jump Route From Here").clicked() {
                self.jump_plan_from = Some(sid);
                self.set_map_mode(MapMode::JumpPlan);
                ui.close();
            }
            if ui.button("Plan Jump Route To Here").clicked() {
                self.jump_plan_to = Some(sid);
                self.set_map_mode(MapMode::JumpPlan);
                ui.close();
            }
            if ui.button("Add as Jump Waypoint").clicked() {
                if Some(sid) != self.jump_plan_from
                    && Some(sid) != self.jump_plan_to
                    && !self.jump_waypoints.contains(&sid)
                {
                    self.jump_waypoints.push(sid);
                }
                self.set_map_mode(MapMode::JumpPlan);
                ui.close();
            }
            let fav = self.jump_favourites.contains(&sid);
            let fav_label = format!(
                "{} {}",
                egui_phosphor::regular::STAR,
                if fav { "Unfavourite" } else { "Favourite" }
            );
            if ui.button(fav_label).clicked() {
                if fav {
                    self.jump_favourites.remove(&sid);
                } else {
                    self.jump_favourites.insert(sid);
                }
                self.persist_jump_favourites();
                self.jump_route_key = None;
                ui.close();
            }
            if ui.button("Show Info").clicked() {
                self.dock_system(sid);
                ui.close();
            }
            let permit = self
                .systems
                .as_ref()
                .and_then(|g| g.info_of(sid).map(|s| s.name.clone()))
                .and_then(|n| self.settings.jump_dock.iter().find(|p| p.system.eq_ignore_ascii_case(&n)).cloned());
            let caps = permit.as_ref().map(|p| p.capitals).unwrap_or(false);
            let sups = permit.as_ref().map(|p| p.supers).unwrap_or(false);
            if ui.selectable_label(caps, "Capitals dock here").clicked() {
                self.toggle_dock_permit(sid, false);
                ui.close();
            }
            if ui.selectable_label(sups, "Supers/titans dock here").clicked() {
                self.toggle_dock_permit(sid, true);
                ui.close();
            }
            if self.map_mode == MapMode::Travel {
                ui.separator();
                if ui.button("Travel: set as start").clicked() {
                    self.travel_start = Some(sid);
                    self.travel_start_q.clear();
                    if Some(sid) != self.player_system() {
                        self.travel_live = false;
                        self.map_follow = false;
                    }
                    self.plan_route();
                    ui.close();
                }
                if ui.button("Travel: set as destination").clicked() {
                    self.travel_end = Some(sid);
                    self.travel_end_q.clear();
                    self.plan_route();
                    ui.close();
                }
                if ui.button("Travel: add waypoint").clicked() {
                    if !self.travel_waypoints.contains(&sid) {
                        self.travel_waypoints.push(sid);
                    }
                    self.travel_avoid.retain(|&a| a != sid);
                    self.plan_route();
                    ui.close();
                }
                if ui.button("Travel: avoid system").clicked() {
                    if !self.travel_avoid.contains(&sid) {
                        self.travel_avoid.push(sid);
                    }
                    self.travel_waypoints.retain(|&w| w != sid);
                    self.plan_route();
                    ui.close();
                }
            }
        });

        let painter = ui.painter_at(rect);
        // In overlay mode the transparent viewport frame already supplies the opacity-scaled
        // backdrop; an opaque canvas rect here would mask it and keep the overlay solid.
        if !self.map_overlay_mode {
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        }

        let dot = (0.5 * self.map_zoom).clamp(0.7, 12.0);
        let ov = self.map_overlays;
        let zoomed = matches!(self.map_view, MapView::Region(_)) || self.map_zoom >= 12.0;
        let show_sys_labels = zoomed;
        let cull = rect.expand(8.0);

        // The row above a dot reads: wormhole/camp icons, name, sov upgrade icons, all centred on the
        // dot as one block. It is laid out here, once, because the pieces draw in different passes:
        // the upgrade icons on the right need the width of everything to their left, and all of them
        // need to know whether the name survived culling.
        const NAME_FONT: f32 = 13.0;
        const NAME_GAP: f32 = 4.0;
        let name_font = egui::FontId::proportional(NAME_FONT);
        // Icons track the dot, so they neither float away from a tiny dot nor crowd a fat one.
        let icon_h = (dot * 1.6 + 8.0).clamp(11.0, 20.0);
        let icon_w = icon_h + 3.0;
        let icon_font = egui::FontId::proportional(icon_h);

        let mut lead_icons: std::collections::HashMap<i64, Vec<(&str, egui::Color32)>> =
            std::collections::HashMap::new();
        if ov.wormholes {
            let wh_col = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
            for sid in &self.wh_overlay.jspace_holes {
                lead_icons
                    .entry(*sid)
                    .or_default()
                    .push((egui_phosphor::regular::SPIRAL, wh_col));
            }
        }
        if ov.camps {
            let now = chrono::Utc::now().timestamp();
            if now - self.camped_cache_at >= 2 {
                self.camped_cache = self.camps.lock().unwrap().camped(now);
                self.camped_cache_at = now;
            }
            for (id, level) in &self.camped_cache {
                lead_icons
                    .entry(*id)
                    .or_default()
                    .push((egui_phosphor::regular::CAMPFIRE, camp_color(*level)));
            }
        }

        let mut upgrade_icons: std::collections::HashMap<i64, Vec<String>> =
            std::collections::HashMap::new();
        if ov.upgrades && zoomed {
            let mut by_name: std::collections::HashMap<String, Vec<&str>> =
                std::collections::HashMap::new();
            for u in &self.settings.sov_upgrades {
                by_name.entry(u.system.to_lowercase()).or_default().push(u.upgrade.as_str());
            }
            let kinds = self.upgrade_kinds;
            for s in &self.map_draw {
                if let Some(ups) = by_name.get(&s.name.to_lowercase()) {
                    let parts: Vec<String> = ups
                        .iter()
                        .flat_map(|u| split_upgrade_label(u))
                        .filter(|up| kinds[upgrade_kind(up) as usize])
                        .take(6)
                        .map(str::to_owned)
                        .collect();
                    if !parts.is_empty() {
                        upgrade_icons.insert(s.id, parts);
                    }
                }
            }
        }

        let mut label_at: std::collections::HashMap<i64, LabelRow> =
            std::collections::HashMap::new();
        {
            let mut placed: Vec<egui::Rect> = Vec::new();
            for s in &self.map_draw {
                let p = pos[&s.id];
                if !cull.contains(p) {
                    continue;
                }
                let lead = lead_icons.get(&s.id).map_or(0, Vec::len) as f32;
                let right = upgrade_icons.get(&s.id).map_or(0, Vec::len) as f32;
                if lead == 0.0 && right == 0.0 && !show_sys_labels {
                    continue;
                }
                // Extra lift without a name, to clear the halos that ring the bare dot.
                let mid_y = p.y - dot - if show_sys_labels { 2.0 } else { 8.0 } - icon_h / 2.0;
                let mut name_w = if show_sys_labels {
                    painter
                        .layout_no_wrap(s.name.clone(), name_font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                } else {
                    0.0
                };
                let lay = |name_w: f32| {
                    let name_span = if name_w > 0.0 { name_w + NAME_GAP } else { 0.0 };
                    let total = (lead + right) * icon_w + name_span;
                    let left = p.x - total / 2.0;
                    let name_x = left + lead * icon_w;
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(left, mid_y - icon_h / 2.0),
                        egui::pos2(left + total, mid_y + icon_h / 2.0),
                    );
                    LabelRow {
                        lead_x: left,
                        name_x,
                        icons_x: name_x + name_span,
                        mid_y,
                        name_shown: name_w > 0.0,
                        rect,
                    }
                };
                if name_w > 0.0 && placed.iter().any(|r| r.expand(2.0).intersects(lay(name_w).rect))
                {
                    // No room for the name. The icons stay, and re-centre on the dot as if the name
                    // had never been there.
                    name_w = 0.0;
                }
                let row = lay(name_w);
                if row.name_shown {
                    placed.push(row.rect);
                }
                label_at.insert(s.id, row);
            }
        }
        let mut activity_heat: std::collections::HashMap<i64, egui::Color32> =
            std::collections::HashMap::new();
        // Zoomed out, only the busy systems are worth a number; a quiet system shows nothing at all.
        let act_floor: u32 = match self.map_zoom {
            z if z >= 12.0 => 1,
            z if z >= 6.0 => 3,
            z if z >= 3.0 => 10,
            _ => 25,
        };

        if let Some(up) = &self.map_highlight_upgrade {
            let upl = up.to_lowercase();
            let hi: std::collections::HashSet<String> = self
                .settings
                .sov_upgrades
                .iter()
                .filter(|u| split_upgrade_label(&u.upgrade).iter().any(|p| p.to_lowercase() == upl))
                .map(|u| u.system.to_lowercase())
                .collect();
            let col = ui.visuals().hyperlink_color;
            for s in &self.map_draw {
                if hi.contains(&s.name.to_lowercase()) {
                    painter.circle_filled(pos[&s.id], dot + 9.0, col.gamma_multiply(0.28));
                }
            }
        }

        let bridges: std::collections::HashSet<(i64, i64)> = if let Some(g) = &self.systems {
            self.settings
                .jump_bridges
                .iter()
                .filter_map(|b| {
                    let a = g.lookup(&b.from)?.id;
                    let c = g.lookup(&b.to)?.id;
                    Some((a.min(c), a.max(c)))
                })
                .collect()
        } else {
            Default::default()
        };

        let cull = rect.expand(8.0);
        let seg_visible = |a: egui::Pos2, b: egui::Pos2| egui::Rect::from_two_pos(a, b).intersects(cull);

        let line_col = ui.visuals().weak_text_color().gamma_multiply(0.5);
        if let Some(graph) = &self.systems {
            for s in &self.map_draw {
                let p1 = pos[&s.id];
                for &n in graph.neighbors(s.id) {
                    if s.id < n && !bridges.contains(&(s.id, n)) {
                        if let Some(p2) = pos.get(&n) {
                            if seg_visible(p1, *p2) {
                                painter.line_segment([p1, *p2], egui::Stroke::new(1.0, line_col));
                            }
                        }
                    }
                }
            }
        }
        if self.map_overlays.bridges {
            let bridge_col = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);
            for &(a, c) in &bridges {
                if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&c)) {
                    if seg_visible(*p1, *p2) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, bridge_col));
                    }
                }
            }
        }

        if self.map_overlays.wormholes {
            let wh_col = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
            let chain_col = egui::Color32::from_rgb(0xB0, 0x7C, 0xE8);
            const TURNUR: i64 = 30_002_086;
            for &(a, b) in &self.wh_overlay.direct {
                if !self.map_overlays.turnur && (a == TURNUR || b == TURNUR) {
                    continue;
                }
                if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&b)) {
                    painter.line_segment([*p1, *p2], egui::Stroke::new(1.6, wh_col));
                }
            }
            for &(a, b, hops) in &self.wh_overlay.chains {
                if !self.map_overlays.turnur && (a == TURNUR || b == TURNUR) {
                    continue;
                }
                if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&b)) {
                    painter.extend(egui::Shape::dashed_line(
                        &[*p1, *p2],
                        egui::Stroke::new(1.8, chain_col),
                        6.0,
                        4.0,
                    ));
                    let mid = egui::pos2((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
                    let txt = format!("{hops}J");
                    let r = painter.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        &txt,
                        egui::FontId::proportional(11.0),
                        chain_col,
                    );
                    painter.rect_filled(r.expand(2.0), 3.0, ui.visuals().extreme_bg_color.gamma_multiply(0.7));
                    painter.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        &txt,
                        egui::FontId::proportional(11.0),
                        chain_col,
                    );
                }
            }
            if self.map_overlays.thera {
                let conns: Vec<&crate::store::MapSystem> = self
                    .wh_overlay
                    .thera_conns
                    .iter()
                    .filter_map(|id| self.map_draw.iter().find(|s| s.id == *id))
                    .collect();
                let conn_screen: Vec<egui::Pos2> =
                    conns.iter().filter_map(|s| pos.get(&s.id).copied()).collect();
                if !conns.is_empty() && !conn_screen.is_empty() {
                    let mut cx = conns.iter().map(|s| s.x).sum::<f64>() / conns.len() as f64;
                    let min_z = conns.iter().map(|s| s.z).fold(f64::INFINITY, f64::min);
                    let max_z = conns.iter().map(|s| s.z).fold(f64::NEG_INFINITY, f64::max);
                    let mut tz = min_z - (max_z - min_z).max(1.0) * 0.25;
                    if self.map_layout == crate::map::MapLayout::Spaced {
                        let rc = |rid: i64| -> Option<(f64, f64)> {
                            let sys: Vec<&crate::store::MapSystem> =
                                self.map_draw.iter().filter(|s| s.region_id == rid).collect();
                            if sys.is_empty() {
                                return None;
                            }
                            let n = sys.len() as f64;
                            Some((
                                sys.iter().map(|s| s.x).sum::<f64>() / n,
                                sys.iter().map(|s| s.z).sum::<f64>() / n,
                            ))
                        };
                        if let (Some(sl), Some(dm)) = (rc(10_000_053), rc(10_000_045)) {
                            cx = (sl.0 + dm.0) / 2.0;
                            tz = (sl.1 + dm.1) / 2.0;
                        }
                    }
                    let tp = crate::map::project(cx, tz, &bounds, rect, self.map_zoom, self.map_pan);
                    let line_col = egui::Color32::from_rgb(0x6E, 0xC8, 0xF0);
                    let tcol = egui::Color32::from_rgb(0xB0, 0x70, 0xE0);
                    for p in &conn_screen {
                        painter.line_segment([tp, *p], egui::Stroke::new(1.6, line_col));
                    }
                    painter.circle_filled(tp, dot + 3.0, tcol);
                    painter.circle_stroke(tp, dot + 6.0, egui::Stroke::new(2.0, tcol));
                    let lp = tp + egui::vec2(0.0, -dot - 11.0);
                    let r = painter.text(lp, egui::Align2::CENTER_CENTER, "Thera",
                        egui::FontId::proportional(12.0), tcol);
                    painter.rect_filled(r.expand(2.0), 3.0,
                        ui.visuals().extreme_bg_color.gamma_multiply(0.7));
                    painter.text(lp, egui::Align2::CENTER_CENTER, "Thera",
                        egui::FontId::proportional(12.0), tcol);
                }
            }
            if self.map_overlays.turnur {
                if let Some(tp) = pos.get(&TURNUR).copied() {
                    let col = egui::Color32::from_rgb(0xE0, 0xA8, 0x4C);
                    painter.circle_stroke(tp, dot + 6.0, egui::Stroke::new(2.0, col));
                    let lp = tp + egui::vec2(0.0, -dot - 11.0);
                    let r = painter.text(lp, egui::Align2::CENTER_CENTER, "Turnur",
                        egui::FontId::proportional(12.0), col);
                    painter.rect_filled(r.expand(2.0), 3.0,
                        ui.visuals().extreme_bg_color.gamma_multiply(0.7));
                    painter.text(lp, egui::Align2::CENTER_CENTER, "Turnur",
                        egui::FontId::proportional(12.0), col);
                }
            }
        }

        if ov.adm || ov.activity != ActivityMode::Off || ov.upgrades {
            let status = self.system_status.lock().unwrap();
            for s in &self.map_draw {
                let p = pos[&s.id];
                if let Some(f) = status.get(&s.id) {
                    if ov.adm {
                        if let Some(adm) = f.adm {
                            let c = if adm >= 5.0 {
                                egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
                            } else if adm >= 3.0 {
                                crate::theme::standing::WARNING
                            } else {
                                crate::theme::standing::HOSTILE
                            };
                            painter.circle_filled(p, dot + 7.0, c.gamma_multiply(0.30));
                        }
                    }
                    if ov.activity != ActivityMode::Off {
                        let v = ov.activity.value(f);
                        if v >= act_floor {
                            activity_heat.insert(s.id, activity_color(v, ov.activity.scale()));
                            // Zoomed out there is no room for a number under every dot; the dot
                            // itself carries the heat instead (see the dot loop).
                            if show_sys_labels {
                                painter.text(
                                    p + egui::vec2(0.0, dot + 3.0),
                                    egui::Align2::CENTER_TOP,
                                    compact_count(v),
                                    egui::FontId::proportional(12.0),
                                    activity_color(v, ov.activity.scale()),
                                );
                            }
                        }
                    }
                }
                if let (Some(ups), Some(row)) =
                    (upgrade_icons.get(&s.id), label_at.get(&s.id))
                {
                    for (k, up) in ups.iter().enumerate() {
                        let ip = egui::pos2(row.icons_x + k as f32 * icon_w, row.mid_y);
                        if ip.x + icon_w > rect.right()
                            || ip.y - icon_h / 2.0 < rect.top()
                            || !rect.contains(ip)
                        {
                            continue;
                        }
                        let (kind, level) = upgrade_info(up);
                        let lcol = level_color(level);
                        match kind {
                            UpgradeIcon::Glyph(g) => {
                                painter.text(
                                    ip,
                                    egui::Align2::LEFT_CENTER,
                                    g,
                                    icon_font.clone(),
                                    lcol,
                                );
                            }
                            UpgradeIcon::Mineral(tid) => {
                                let r = egui::Rect::from_min_size(
                                    egui::pos2(ip.x, ip.y - icon_h / 2.0),
                                    egui::Vec2::splat(icon_h),
                                );
                                ui.put(r, egui::Image::new(eve_type_icon_url(tid, icon_h)))
                                    .on_hover_text(up);
                                painter.circle_filled(r.right_top(), 3.0, lcol);
                            }
                        }
                    }
                }
            }
        }

        let mut reached_dest = false;
        if let (Some(dest), Some(ps)) = (self.route_destination, player_sys) {
            if ps == dest {
                reached_dest = true;
            } else {
                // Drawing the client's own idea of the route would trace the long k-space path a
                // hole route deliberately skips, so this walks the same graph the waypoints came from.
                let holes = if self.settings.route_via_wormholes {
                    self.wh_adjacency()
                } else {
                    std::collections::HashMap::new()
                };
                let route = self
                    .systems
                    .as_ref()
                    .and_then(|g| g.route_with(ps, dest, true, true, &holes, |_| true));
                if let Some(route) = route {
                    let phase = (ui.input(|i| i.time) * 28.0) as f32;
                    // A J-space leg has no place on the map, so the hop is drawn between the k-space
                    // systems on either side of the hole.
                    let mut last: Option<(i64, egui::Pos2)> = None;
                    let mut jumped_hole = false;
                    for &id in &route {
                        let Some(&p) = pos.get(&id) else {
                            jumped_hole = true;
                            continue;
                        };
                        if let Some((prev_id, prev_p)) = last {
                            let col = self.leg_kind(prev_id, id, jumped_hole).color();
                            dashed_flow(&painter, prev_p, p, col, phase);
                        }
                        last = Some((id, p));
                        jumped_hole = false;
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                }
            }
        }
        if reached_dest {
            self.route_destination = None;
        }

        // The campfire itself rides the label row (see `lead_icons`); only its glow stays on the dot.
        if self.map_overlays.camps {
            for (id, level) in &self.camped_cache {
                if let Some(p) = pos.get(id) {
                    let glow = match level {
                        crate::camp::CampLevel::Likely => 0.30,
                        crate::camp::CampLevel::Possible => 0.20,
                        crate::camp::CampLevel::Flag => 0.12,
                    };
                    painter.circle_filled(*p, dot + 7.0, camp_color(*level).gamma_multiply(glow));
                }
            }
        }

        if self.map_mode == MapMode::Travel {
            let cyan = egui::Color32::from_rgb(0x4F, 0xC3, 0xF7);
            if let Some(direct) = &self.travel_direct_route {
                let gray = egui::Color32::from_rgb(0x9E, 0x9E, 0x9E);
                for w in direct.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, gray));
                    }
                }
            }
            if let Some(base) = self.travel_live.then_some(self.travel_live_base.as_ref()).flatten() {
                let purple = egui::Color32::from_rgb(0x95, 0x75, 0xCD);
                for w in base.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, purple));
                    }
                }
            }
            if let Some(route) = &self.travel_route {
                // A leg through J-space has no position on the k-space map, so it is drawn as one
                // dashed hop between the k-space systems on either side of the hole.
                let mut last: Option<(i64, egui::Pos2)> = None;
                let mut jumped_hole = false;
                for &id in route {
                    let Some(&p) = pos.get(&id) else {
                        jumped_hole = true;
                        continue;
                    };
                    if let Some((prev_id, prev_p)) = last {
                        match self.leg_kind(prev_id, id, jumped_hole) {
                            Leg::Gate => {
                                painter.line_segment([prev_p, p], egui::Stroke::new(2.5, cyan));
                            }
                            kind => {
                                painter.extend(egui::Shape::dashed_line(
                                    &[prev_p, p],
                                    egui::Stroke::new(2.5, kind.color()),
                                    7.0,
                                    5.0,
                                ));
                            }
                        }
                    }
                    last = Some((id, p));
                    jumped_hole = false;
                }
            }
            let mark = |p: egui::Pos2, color: egui::Color32| {
                let r = egui::Rect::from_center_size(p, egui::vec2(14.0, 14.0));
                let st = egui::Stroke::new(2.0, color);
                painter.line_segment([r.left_top(), r.right_top()], st);
                painter.line_segment([r.right_top(), r.right_bottom()], st);
                painter.line_segment([r.right_bottom(), r.left_bottom()], st);
                painter.line_segment([r.left_bottom(), r.left_top()], st);
            };
            for wp in &self.travel_waypoints {
                if let Some(p) = pos.get(wp) {
                    mark(*p, cyan);
                }
            }
            if let Some(p) = self.travel_start.and_then(|s| pos.get(&s)) {
                mark(*p, egui::Color32::from_rgb(0x66, 0xBB, 0x6A));
            }
            if let Some(p) = self.travel_end.and_then(|e| pos.get(&e)) {
                mark(*p, egui::Color32::from_rgb(0xFF, 0xA7, 0x26));
            }
            if let Some(at) = self.travel_changed_at {
                if chrono::Utc::now().timestamp() - at < 6 {
                    let blink = ((ui.input(|i| i.time) * 5.0).sin() * 0.5 + 0.5) as f32;
                    let warn = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
                    for id in &self.travel_changed {
                        if let Some(p) = pos.get(id) {
                            painter.circle_stroke(
                                *p,
                                11.0,
                                egui::Stroke::new(2.5, warn.gamma_multiply(blink)),
                            );
                        }
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                }
            }
        }

        if self.map_mode == MapMode::JumpPlan {
            let jcol = egui::Color32::from_rgb(0x9C, 0x6A, 0xF7);
            let red = crate::theme::standing::HOSTILE;
            let teal = egui::Color32::from_rgb(0x4D, 0xB6, 0xAC);
            for d in self.jump_dockable_ids() {
                if let Some(p) = pos.get(&d) {
                    painter.circle_stroke(*p, 9.0, egui::Stroke::new(1.5, teal));
                }
            }
            for alt in &self.jump_alt {
                if let Some(p) = pos.get(alt) {
                    painter.circle_stroke(*p, 6.0, egui::Stroke::new(1.0, jcol.gamma_multiply(0.5)));
                }
            }
            for leg in &self.jump_legs {
                if leg.valid {
                    for w in leg.path.windows(2) {
                        if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                            painter.line_segment([*p1, *p2], egui::Stroke::new(2.5, jcol));
                        }
                    }
                } else if let (Some(p1), Some(p2)) = (pos.get(&leg.from), pos.get(&leg.to)) {
                    painter.line_segment([*p1, *p2], egui::Stroke::new(2.0, red));
                }
            }
            for sid in &self.jump_route {
                if let Some(p) = pos.get(sid) {
                    painter.circle_filled(*p, 4.0, jcol);
                }
            }
            let gold = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
            for fav in &self.jump_favourites {
                if let Some(p) = pos.get(fav) {
                    painter.circle_filled(*p + egui::vec2(0.0, -11.0), 3.0, gold);
                }
            }
            if let Some(p) = self.jump_plan_from.and_then(|s| pos.get(&s)) {
                painter.circle_stroke(*p, 8.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(0x66, 0xBB, 0x6A)));
            }
            for wp in &self.jump_waypoints {
                if let Some(p) = pos.get(wp) {
                    painter.circle_stroke(*p, 7.0, egui::Stroke::new(2.0, jcol));
                }
            }
            if let Some(p) = self.jump_plan_to.and_then(|s| pos.get(&s)) {
                painter.circle_stroke(*p, 8.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(0xFF, 0xA7, 0x26)));
            }
        }

        let hovered_id = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| resp.hovered())
            .and_then(|p| nearest_system(p, &pos, 8.0));
        // A selected system keeps its hover effects; hovering something else takes over.
        let focus_id = hovered_id.or(self.map_selected);
        if let (true, Some(h_id)) = (self.map_overlays.jump_range, focus_id) {
            if let Some(real_h) = self.map_systems.iter().find(|s| s.id == h_id) {
                let hp = pos[&h_id];
                let band_color = [
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
                    egui::Color32::from_rgb(0xE0, 0xA4, 0x3A),
                    egui::Color32::from_rgb(0xD8, 0x4C, 0x4C),
                ];
                if !schematic {
                    for (i, (name, ly)) in crate::map::JUMP_RANGES.iter().enumerate().rev() {
                        let col = band_color.get(i).copied().unwrap_or(band_color[2]);
                        let r = crate::map::ly_to_pixels(*ly, &bounds, rect, self.map_zoom);
                        painter.circle_stroke(hp, r, egui::Stroke::new(1.5, col.gamma_multiply(0.85)));
                        painter.text(
                            hp + egui::vec2(0.0, -r),
                            egui::Align2::CENTER_BOTTOM,
                            format!("{name} {ly:.0} ly"),
                            egui::FontId::proportional(12.0),
                            col,
                        );
                    }
                }
                // map_draw and map_systems share order, so index zips draw↔real.
                for (i, s) in self.map_draw.iter().enumerate() {
                    if s.id == h_id {
                        continue;
                    }
                    let d = crate::map::ly_distance(real_h, &self.map_systems[i]);
                    if let Some(b) = crate::map::JUMP_RANGES.iter().position(|(_, ly)| d <= *ly) {
                        let col = band_color.get(b).copied().unwrap_or(band_color[2]);
                        painter.circle_filled(pos[&s.id], dot + 4.0, col.gamma_multiply(0.70));
                    }
                }
            }
        }

        if self.map_hover_since.map(|(id, _)| id) != hovered_id {
            self.map_hover_since = hovered_id.map(|id| (id, std::time::Instant::now()));
        }
        let dwelled = self.map_hover_since.is_some_and(|(_, since)| {
            let waited = since.elapsed();
            if waited < MAP_TIP_DELAY {
                // Nothing else will redraw while the pointer sits still, so ask for the frame that
                // brings the tooltip up.
                ui.ctx().request_repaint_after(MAP_TIP_DELAY - waited);
                return false;
            }
            true
        });
        if let Some(h_id) = hovered_id.filter(|_| dwelled) {
            if let Some(ptr) = ui.ctx().pointer_hover_pos() {
                egui::Area::new(ui.id().with("map_hover_tip"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(ptr + egui::vec2(14.0, -6.0))
                    .pivot(egui::Align2::LEFT_BOTTOM)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            self.map_system_tooltip(ui, h_id);
                        });
                    });
            }
        }

        let now_ts = chrono::Utc::now().timestamp();
        let sev_rules = self.settings.severity.clone();
        let intel_map: std::collections::HashMap<i64, (crate::settings::Severity, i64)> = {
            let st = self.intel_state.lock().unwrap();
            let mut m: std::collections::HashMap<i64, (crate::settings::Severity, i64)> =
                std::collections::HashMap::new();
            for r in &st.reports {
                if r.clear || st.is_stale(r) {
                    continue;
                }
                if let Some(s) = r.primary_system() {
                    let sev = severity_of(r, &sev_rules);
                    let e = m.entry(s.id).or_insert((sev, r.received));
                    e.0 = e.0.max(sev);
                    e.1 = e.1.max(r.received);
                }
            }
            m
        };
        let blink = (ui.input(|i| i.time) as f32 * 6.0).sin().abs();
        let mut any_fresh = false;
        // The holder's colour rides the dot at every zoom; the logo only appears once the dots are
        // big enough to hang one on, below which a logo per system is unreadable clutter.
        let icon_px = (dot * 2.6).floor();
        let sov_art = self.sov_art(ui.ctx());
        let show_icons = icon_px >= 10.0;
        for s in &self.map_draw {
            let p = pos[&s.id];
            if !cull.contains(p) {
                continue;
            }
            let art = sov_art.get(&s.id);
            // Zoomed out the dot is the only thing left to say it with, so heat outranks the
            // holder's colour there; zoomed in the number below the dot carries it instead.
            let dot_col = activity_heat
                .get(&s.id)
                .copied()
                .filter(|_| !show_sys_labels)
                .or_else(|| art.and_then(|a| a.dot))
                .unwrap_or_else(|| security_color(s.security));
            painter.circle_filled(p, dot, dot_col);
            if let Some(a) = art.filter(|_| show_icons) {
                // Drawn through the map's clipped painter, not `Image::paint_at`, which would paint
                // into the panel layer and spill the logo over the side bars.
                let hint = egui::SizeHint::Size {
                    width: 64,
                    height: 64,
                    maintain_aspect_ratio: true,
                };
                if let Ok(egui::load::TexturePoll::Ready { texture }) =
                    ui.ctx().try_load_texture(&a.icon, egui::TextureOptions::LINEAR, hint)
                {
                    let r = egui::Rect::from_center_size(p, egui::vec2(icon_px, icon_px));
                    painter.image(
                        texture.id,
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
            if self.settings.bookmarks.contains(&s.id) {
                painter.circle_stroke(
                    p,
                    dot,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(0x4D, 0xB6, 0xAC)),
                );
            }
            if let Some((sev, received)) = intel_map.get(&s.id) {
                let base = severity_color(*sev);
                let fresh = now_ts - received < 15;
                let (fill_a, ring_w) = if fresh {
                    any_fresh = true;
                    (0.45 + 0.45 * blink, 3.0)
                } else {
                    (0.40, 2.5)
                };
                painter.circle_filled(p, dot + 5.0, base.gamma_multiply(fill_a));
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(ring_w, base));
            }
            if let Some((count, has_active)) = char_here.get(&s.id) {
                let blue = if *has_active {
                    egui::Color32::from_rgb(0x4F, 0xC3, 0xF7)
                } else {
                    egui::Color32::from_rgb(0xA8, 0xDE, 0xF7)
                };
                painter.circle_stroke(p, dot + 8.0, egui::Stroke::new(2.5, blue));
                if *count > 1 {
                    painter.text(
                        p + egui::vec2(dot + 9.0, -(dot + 9.0)),
                        egui::Align2::LEFT_BOTTOM,
                        count.to_string(),
                        egui::FontId::proportional(11.0),
                        blue,
                    );
                }
            }
            if Some(s.id) == hovered_id {
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if self.map_selected == Some(s.id) {
                painter.circle_stroke(p, dot + 6.0, egui::Stroke::new(2.5, egui::Color32::WHITE));
            }
            if let Some(row) = label_at.get(&s.id).filter(|_| rect.contains(p)) {
                // The icons always draw: dropping one would silently hide a camp or a hole. Only the
                // name is culled, and when it is, the row re-centres without it.
                if let Some(icons) = lead_icons.get(&s.id) {
                    for (k, (glyph, col)) in icons.iter().enumerate() {
                        painter.text(
                            egui::pos2(row.lead_x + k as f32 * icon_w, row.mid_y),
                            egui::Align2::LEFT_CENTER,
                            *glyph,
                            icon_font.clone(),
                            *col,
                        );
                    }
                }
                if row.name_shown {
                    let at = egui::pos2(row.name_x, row.mid_y);
                    // Outlined, so the name survives whatever it lands on: halos, sov icons, routes.
                    for off in OUTLINE {
                        painter.text(
                            at + off,
                            egui::Align2::LEFT_CENTER,
                            &s.name,
                            name_font.clone(),
                            egui::Color32::BLACK,
                        );
                    }
                    painter.text(
                        at,
                        egui::Align2::LEFT_CENTER,
                        &s.name,
                        name_font.clone(),
                        ui.visuals().text_color(),
                    );
                }
            }
        }
        if any_fresh {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }

        if !show_sys_labels {
            let mut acc: std::collections::HashMap<i64, (egui::Vec2, u32)> =
                std::collections::HashMap::new();
            for s in &self.map_draw {
                let e = acc.entry(s.region_id).or_insert((egui::Vec2::ZERO, 0));
                e.0 += pos[&s.id].to_vec2();
                e.1 += 1;
            }
            let mut labels: Vec<(i64, egui::Pos2)> =
                acc.into_iter().map(|(rid, (sum, n))| (rid, (sum / n as f32).to_pos2())).collect();
            labels.sort_by_key(|(rid, _)| *rid);
            let font = egui::FontId::proportional(16.0);
            for (rid, c) in labels {
                if !rect.contains(c) {
                    continue;
                }
                let Some((_, name)) = self.map_regions.iter().find(|(id, _)| *id == rid) else {
                    continue;
                };
                painter.text(
                    c + egui::vec2(1.0, 1.0),
                    egui::Align2::CENTER_CENTER,
                    name,
                    font.clone(),
                    egui::Color32::from_black_alpha(180),
                );
                painter.text(c, egui::Align2::CENTER_CENTER, name, font.clone(), egui::Color32::from_gray(220));
            }
        }

        self.map_chrome(ui, rect);
    }

    fn draw_threat_view(&mut self, ui: &mut egui::Ui, rect: egui::Rect, player_sys: Option<i64>) {
        use crate::map::MapLayout;
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();

        if resp.dragged() {
            self.map_pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = self.map_zoom;
                let new = (old * (scroll * 0.003).exp()).clamp(0.3, 6.0);
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let rel = m - (rect.center() + self.map_pan);
                    self.map_pan += rel * (1.0 - new / old);
                }
                self.map_zoom = new;
            }
        }

        let Some(graph) = self.systems.clone() else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "SDE not ready.", egui::FontId::proportional(14.0), visuals.weak_text_color());
            return;
        };
        let Some(center) = self.map_threat_center.or(player_sys) else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No centre system. Set an active character, or right-click a system on the map.",
                egui::FontId::proportional(13.0),
                visuals.weak_text_color(),
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            return;
        };
        let depth = self.map_threat_jumps.max(1);

        let (dist, children, order) = bfs_tree(&graph, center, depth, self.threat_include_bridges);
        let leaves = order.iter().filter(|id| children.get(id).map_or(true, |c| c.is_empty())).count();
        let mut frac: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
        let mut next = 0u32;
        assign_fracs(center, &children, leaves.max(1) as f32, &mut next, &mut frac);

        let zoom = self.map_zoom;
        let mut pos: std::collections::HashMap<i64, egui::Pos2> = std::collections::HashMap::new();
        match self.map_layout {
            MapLayout::Radial => {
                let c = rect.center() + self.map_pan;
                let ring = (rect.size().min_elem() * 0.44 / depth as f32) * zoom;
                for &id in &order {
                    let d = dist[&id];
                    if d == 0 {
                        pos.insert(id, c);
                    } else {
                        let ang =
                            frac[&id] * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                        pos.insert(id, c + egui::Vec2::angled(ang) * (d as f32 * ring));
                    }
                }
            }
            _ => {
                let level = (rect.height() * 0.82 / (depth as f32 + 0.5)) * zoom;
                let width = rect.width() * 0.92 * zoom;
                let cx = rect.center().x + self.map_pan.x;
                let top = rect.top() + 34.0 + self.map_pan.y;
                for &id in &order {
                    let x = cx + (frac[&id] - 0.5) * width;
                    let y = top + dist[&id] as f32 * level;
                    pos.insert(id, egui::pos2(x, y));
                }
            }
        }

        let sev_rules = self.settings.severity.clone();
        let intel_map: std::collections::HashMap<i64, (crate::settings::Severity, i64)> = {
            let st = self.intel_state.lock().unwrap();
            let mut m: std::collections::HashMap<i64, (crate::settings::Severity, i64)> =
                std::collections::HashMap::new();
            for r in &st.reports {
                if r.clear || st.is_stale(r) {
                    continue;
                }
                if let Some(sy) = r.primary_system() {
                    let sev = severity_of(r, &sev_rules);
                    let e = m.entry(sy.id).or_insert((sev, r.received));
                    e.0 = e.0.max(sev);
                    e.1 = e.1.max(r.received);
                }
            }
            m
        };
        let now_ts = chrono::Utc::now().timestamp();
        let blink = (ui.input(|i| i.time) as f32 * 6.0).sin().abs();
        let mut any_fresh = false;

        let edge = visuals.weak_text_color().gamma_multiply(0.5);
        let bridge_col = egui::Color32::from_rgb(0x4C, 0xC2, 0x6A);
        for &a in &order {
            for &b in graph.neighbors(a) {
                if a < b {
                    if let (Some(&pa), Some(&pb)) = (pos.get(&a), pos.get(&b)) {
                        if graph.is_bridge(a, b) {
                            painter.line_segment([pa, pb], egui::Stroke::new(2.0, bridge_col));
                        } else {
                            painter.line_segment([pa, pb], egui::Stroke::new(1.0, edge));
                        }
                    }
                }
            }
        }

        let label_max = 3;
        let line_h = 13.0;
        let stagger: std::collections::HashMap<i64, f32> = if matches!(self.map_layout, MapLayout::Radial) {
            std::collections::HashMap::new()
        } else {
            let mut ring: Vec<i64> = order.iter().copied().filter(|id| dist[id] == label_max).collect();
            ring.sort_by(|a, b| frac[a].partial_cmp(&frac[b]).unwrap_or(std::cmp::Ordering::Equal));
            ring.iter().enumerate().map(|(i, id)| (*id, (i % 3) as f32 * line_h)).collect()
        };
        let hovered = ui.input(|i| i.pointer.hover_pos()).and_then(|hp| nearest_system(hp, &pos, 12.0));

        let node_r = (5.5 * zoom.clamp(0.6, 1.6)).max(3.5);
        let font = egui::FontId::proportional((12.0 * zoom).clamp(9.0, 15.0));
        for &id in &order {
            let p = pos[&id];
            let info = graph.info_of(id);
            let sec = info.map(|i| i.security).unwrap_or(0.0);
            let is_center = id == center;
            let r = if is_center { node_r + 2.5 } else { node_r };
            if let Some((isev, received)) = intel_map.get(&id) {
                let base = severity_color(*isev);
                let fresh = now_ts - received < 15;
                let (glow, ring_w) = if fresh {
                    any_fresh = true;
                    (0.30 + 0.35 * blink, 3.0)
                } else {
                    (0.28, 2.5)
                };
                painter.circle_filled(p, r + 6.0, base.gamma_multiply(glow));
                painter.circle_stroke(p, r + 3.0, egui::Stroke::new(ring_w, base));
            }
            painter.circle_filled(p, r, security_color(sec));
            let outline = if is_center {
                egui::Color32::WHITE
            } else if Some(id) == player_sys {
                crate::theme::standing::ALLIANCE
            } else {
                visuals.window_stroke.color
            };
            painter.circle_stroke(p, r, egui::Stroke::new(if is_center { 2.0 } else { 1.0 }, outline));
            if Some(id) == hovered {
                painter.circle_stroke(p, r + 2.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if let (Some(info), true) = (info, dist[&id] <= label_max) {
                let extra = stagger.get(&id).copied().unwrap_or(0.0);
                painter.text(
                    p - egui::vec2(0.0, r + 2.0 + extra),
                    egui::Align2::CENTER_BOTTOM,
                    &info.name,
                    font.clone(),
                    visuals.text_color(),
                );
            }
        }

        if let Some(hid) = hovered {
            if dist[&hid] > label_max {
                if let Some(info) = graph.info_of(hid) {
                    let p = pos[&hid];
                    let anchor = p - egui::vec2(0.0, node_r + 3.0);
                    let g = painter.layout_no_wrap(info.name.clone(), font.clone(), visuals.text_color());
                    let r = egui::Rect::from_min_size(
                        anchor - egui::vec2(g.size().x / 2.0, g.size().y),
                        g.size(),
                    )
                    .expand(3.0);
                    painter.rect_filled(r, 3.0, visuals.window_fill.gamma_multiply(0.92));
                    painter.galley(r.min + egui::vec2(3.0, 3.0), g, visuals.text_color());
                }
            }
        }

        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.clicked() {
            if let Some(id) = pointer.and_then(|p| nearest_system(p, &pos, 12.0)) {
                self.dock_system(id);
            }
        }
        if resp.secondary_clicked() {
            if let Some(id) = pointer.and_then(|p| nearest_system(p, &pos, 12.0)) {
                self.map_threat_center = Some(id);
                self.map_pan = egui::Vec2::ZERO;
                self.map_zoom = 1.0;
            }
        }

        let cname = graph.info_of(center).map(|i| i.name.clone()).unwrap_or_default();
        painter.text(
            rect.left_bottom() + egui::vec2(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            format!("◎ {cname}  ·  ≤{depth} jumps  ·  {} systems", order.len()),
            egui::FontId::proportional(12.0),
            visuals.weak_text_color(),
        );
        if any_fresh {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn map_chrome(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        if self.map_overlay_mode {
            self.map_overlay_controls(ui, rect);
        } else if self.map_controls_hidden {
            egui::Area::new(ui.id().with("map_show_controls"))
                .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if ui
                            .button(egui_phosphor::regular::EYE)
                            .on_hover_text("Show controls")
                            .clicked()
                        {
                            self.map_controls_hidden = false;
                        }
                    });
                });
        } else {
            if !self.left_dock_open {
                egui::Area::new(ui.id().with("reopen_left"))
                    .fixed_pos(rect.left_top() + egui::vec2(6.0, 6.0))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            if ui.button("\u{00BB}").on_hover_text("Show map panel").clicked() {
                                self.left_dock_open = true;
                            }
                        });
                    });
            }
            if (self.map_mode != MapMode::Standard || self.map_docked_system.is_some())
                && !self.right_dock_open
            {
                egui::Area::new(ui.id().with("reopen_right"))
                    .fixed_pos(rect.right_top() + egui::vec2(-38.0, 6.0))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            if ui.button("\u{00AB}").on_hover_text("Show mode panel").clicked() {
                                self.right_dock_open = true;
                            }
                        });
                    });
            }
            self.map_search_overlay(ui, rect);
        }
    }

    fn map_layers_content(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        ui.label(egui::RichText::new(format!("{}  Sovereignty", icon::FLAG)).strong());
        ui.radio_value(&mut self.map_overlays.sov, SovMode::Off, "Off");
        ui.radio_value(&mut self.map_overlays.sov, SovMode::Alliance, "By alliance");
        ui.radio_value(&mut self.map_overlays.sov, SovMode::Coalition, "By coalition");
        ui.separator();
        ui.label(egui::RichText::new(format!("{}  Activity (last hour)", icon::FIRE)).strong());
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::Off, "Off");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::ShipKills, "Ship kills");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::PodKills, "Pod kills");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::NpcKills, "NPC kills");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::Jumps, "Jumps");
        ui.separator();
        ui.checkbox(&mut self.map_overlays.adm, format!("{}  ADM", icon::SHIELD_CHECK));
        ui.checkbox(&mut self.map_overlays.bridges, format!("{}  Jump bridges", icon::ARROWS_LEFT_RIGHT));
        ui.checkbox(&mut self.map_overlays.upgrades, format!("{}  Sov upgrades", icon::MAP_PIN_LINE));
        if self.map_overlays.upgrades {
            ui.indent("upgrade_kinds", |ui| {
                ui.checkbox(&mut self.upgrade_kinds[0], "Ratting");
                ui.checkbox(&mut self.upgrade_kinds[1], "Exploration");
                ui.checkbox(&mut self.upgrade_kinds[2], "Mining");
                ui.checkbox(&mut self.upgrade_kinds[3], "Other");
            });
        }
        ui.checkbox(&mut self.map_overlays.jump_range, format!("{}  Jump range (hover)", icon::CROSSHAIR_SIMPLE));
        ui.separator();
        ui.checkbox(&mut self.map_overlays.wormholes, format!("{}  Wormhole connections", icon::SPIRAL));
        if self.map_overlays.wormholes {
            ui.indent("wh_hubs", |ui| {
                ui.checkbox(&mut self.map_overlays.thera, format!("{}  Thera", icon::PLANET));
                ui.checkbox(&mut self.map_overlays.turnur, format!("{}  Turnur", icon::PLANET));
            });
        }
        ui.checkbox(&mut self.map_overlays.camps, format!("{}  Gate camps", icon::CAMPFIRE));
        if ui
            .checkbox(&mut self.settings.route_via_wormholes, format!("{}  Route via wormholes", icon::SPIRAL))
            .on_hover_text("Routes and Set Destination use scanned holes, with a waypoint at each hole entrance")
            .changed()
        {
            self.needs_save = true;
            // Toggling this changes what the current destination should be, so re-send it.
            self.replan_routes();
        }
        if self.map_overlays.upgrades {
            ui.separator();
            ui.label(egui::RichText::new("Upgrade icons").strong());
            let mut row = |g: &str, txt: &str| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(g).size(16.0));
                    ui.label(txt);
                });
            };
            row(icon::SKULL, "Ratting / threat detection");
            row(icon::BROADCAST, "Exploration / scanning");
            row(icon::RADIOACTIVE, "Cyno");
            row(icon::GEAR, "Other upgrade");
            ui.label(egui::RichText::new("Mining shows the ore icon").weak());
            ui.horizontal(|ui| {
                ui.label("Level:");
                ui.colored_label(level_color(1), "1");
                ui.colored_label(level_color(2), "2");
                ui.colored_label(level_color(3), "3\u{2013}5");
            });
        }
    }

    fn wormhole_section(&self, ui: &mut egui::Ui, id: i64) {
        let now = chrono::Utc::now().timestamp();
        let holes: Vec<&crate::wormholes::Wormhole> = self
            .wh_cache
            .iter()
            .filter(|w| w.system_id == id || w.dest_system_id == Some(id))
            .collect();
        if holes.is_empty() {
            return;
        }
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{}  Wormholes", egui_phosphor::regular::SPIRAL)).strong(),
        );
        for w in holes {
            let here_is_near = w.system_id == id;
            let other_id = if here_is_near { w.dest_system_id } else { Some(w.system_id) };
            let other = other_id
                .and_then(|sid| {
                    self.systems.as_ref().and_then(|g| g.info_of(sid)).map(|i| i.name.clone())
                })
                .unwrap_or_else(|| w.dest.label().to_owned());
            let sig = if here_is_near { w.signature.as_deref() } else { w.dest_signature.as_deref() }
                .unwrap_or("?");
            let mut parts: Vec<String> = Vec::new();
            if let Some(t) = &w.wh_type {
                parts.push(t.clone());
            }
            if let Some(s) = w.effective_size() {
                parts.push(s.label().to_owned());
            }
            parts.push(match w.hours_left(now) {
                Some(h) => format!("< {h}h"),
                None => "expiring".to_owned(),
            });
            ui.label(egui::RichText::new(format!("{sig} → {other}  ({})", parts.join(", "))));
        }
    }

    fn camp_line(&self, ui: &mut egui::Ui, id: i64) {
        let now = chrono::Utc::now().timestamp();
        if let Some(c) = self.camps.lock().unwrap().camp(id, now) {
            let mins = (c.age / 60).max(0);
            let (label, col) = match c.level {
                crate::camp::CampLevel::Likely => {
                    ("Likely gate camp", egui::Color32::from_rgb(0xEF, 0x44, 0x44))
                }
                crate::camp::CampLevel::Possible => {
                    ("Possible camp", egui::Color32::from_rgb(0xFF, 0xA7, 0x26))
                }
                crate::camp::CampLevel::Flag => {
                    ("Recent gate kills", egui::Color32::from_rgb(0xFF, 0xD5, 0x4F))
                }
            };
            let over = (c.span / 60).max(0);
            ui.label(
                egui::RichText::new(format!(
                    "{}  {label}: {} kills over {over}m, last {mins}m ago",
                    egui_phosphor::regular::CAMPFIRE,
                    c.kills,
                ))
                .strong()
                .color(col),
            );
        }
    }

    fn map_system_tooltip(&self, ui: &mut egui::Ui, id: i64) {
        ui.set_max_width(270.0);
        let status = self.system_status.lock().unwrap();
        let flags = status.get(&id).cloned().unwrap_or_default();
        if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(id)) {
            ui.horizontal(|ui| {
                ui.label(security_badge(info.security));
                ui.label(egui::RichText::new(&info.name).strong());
                if let Some(aid) = flags.sov_alliance {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let url = eve_alliance_logo_url(aid, 26.0);
                        let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(26.0)));
                        if let Some(sov) = &flags.sov {
                            r.on_hover_text(sov);
                        }
                    });
                }
            });
        }
        system_chips_ex(ui, &self.systems, &status, id, true, false);
        if let Some(f) = status.get(&id) {
            if f.jumps + f.ship_kills + f.pod_kills + f.npc_kills > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "Last hour — {} jumps · {} ship · {} pod · {} NPC kills",
                        f.jumps, f.ship_kills, f.pod_kills, f.npc_kills
                    ))
                    .weak(),
                );
            }
        }
        drop(status);
        self.camp_line(ui, id);

        let now = chrono::Utc::now().timestamp();
        let state = self.intel_state.lock().unwrap();
        let green = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
        let mut shown = 0;
        for r in state.reports.iter().rev() {
            if !r.systems.iter().any(|s| s.id == id) {
                continue;
            }
            if shown == 0 {
                ui.separator();
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:>6}", fmt_age((now - r.received).max(0))))
                        .monospace()
                        .weak(),
                );
                if let Some(n) = r.count {
                    ui.label(egui::RichText::new(format!("{n}x")).strong());
                }
                if r.clear {
                    ui.label(egui::RichText::new("CLEAR").color(green));
                }
                for sh in &r.ships {
                    ui.label(egui::RichText::new(&sh.name).weak());
                }
                ui.label(egui::RichText::new(format!("- {}", r.reporter)).weak());
            });
            shown += 1;
            if shown >= 4 {
                ui.label(egui::RichText::new("…").weak());
                break;
            }
        }
        if shown == 0 {
            ui.label(egui::RichText::new("Click for details.").weak());
        }
        drop(state);
        self.wormhole_section(ui, id);
    }

    fn map_overlay_controls(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        use egui_phosphor::regular as icon;
        egui::Area::new(ui.id().with("map_overlay_bar"))
            .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.map_overlay_locked {
                            if ui.button(icon::LOCK).on_hover_text("Unlock").clicked() {
                                self.map_overlay_locked = false;
                            }
                            if ui
                                .add(egui::Button::new(icon::CROSSHAIR).selected(self.map_follow))
                                .on_hover_text("Follow active character")
                                .clicked()
                            {
                                self.map_follow = !self.map_follow;
                            }
                            return;
                        }
                        if ui.button(icon::FRAME_CORNERS).on_hover_text("Exit overlay mode").clicked() {
                            self.map_overlay_mode = false;
                        }
                        if ui.button(icon::LOCK_OPEN).on_hover_text("Lock (no move/resize)").clicked() {
                            self.map_overlay_locked = true;
                        }
                        if ui
                            .add(egui::Button::new(icon::CROSSHAIR).selected(self.map_follow))
                            .on_hover_text("Follow active character")
                            .clicked()
                        {
                            self.map_follow = !self.map_follow;
                        }
                        if ui
                            .add(egui::Button::new(icon::CPU).selected(self.settings.map_overlay_smart))
                            .on_hover_text("Smart on-top (above only while EVE is active)")
                            .clicked()
                        {
                            self.settings.map_overlay_smart = !self.settings.map_overlay_smart;
                            self.needs_save = true;
                        }
                        ui.label("Opacity");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.settings.map_overlay_opacity, 0.2..=1.0)
                                    .show_value(false),
                            )
                            .changed()
                        {
                            self.needs_save = true;
                        }
                    });
                });
            });
    }

    fn player_system(&self) -> Option<i64> {
        let p = self.player.lock().unwrap();
        p.locations.get(&self.active_character).map(|(s, _)| *s).or(p.system_id)
    }

    fn set_map_mode(&mut self, new: MapMode) {
        if new == self.map_mode {
            return;
        }
        let keeps_layers = |m: MapMode| matches!(m, MapMode::Standard | MapMode::JumpPlan);
        if keeps_layers(self.map_mode) {
            self.standard_overlays = self.map_overlays;
        }
        self.map_overlays = if keeps_layers(new) {
            self.standard_overlays
        } else {
            new.overlay_preset()
        };
        if new == MapMode::Safety {
            if !self.map_layout.is_threat() {
                self.safety_prev_layout = Some(self.map_layout);
                self.map_layout = crate::map::MapLayout::Tree;
            }
        } else if self.map_mode == MapMode::Safety {
            if let Some(prev) = self.safety_prev_layout.take() {
                self.map_layout = prev;
            }
        }
        self.map_mode = new;
        if new != MapMode::Standard {
            self.right_dock_open = true;
            self.right_dock_tab = RightDockTab::Mode;
        }
        self.needs_save = true;
    }

    fn load_route(&mut self, r: &crate::settings::SavedRoute) {
        let nm = |id: i64| {
            self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default()
        };
        let (s, e) = (nm(r.start), nm(r.end));
        self.travel_start = Some(r.start);
        self.travel_end = Some(r.end);
        self.travel_start_q = s;
        self.travel_end_q = e;
        self.travel_waypoints = r.waypoints.clone();
        if let Some(c) = &r.constraints {
            self.travel_sec = c.sec;
            self.travel_metric = ActivityMode::from_u8(c.metric);
            self.travel_regional_gates = c.regional_gates;
            self.travel_jump_bridges = c.jump_bridges;
            self.travel_avoid_camps = c.avoid_camps;
            self.travel_avoid = c.avoid.clone();
            self.travel_avoid_sov = c.avoid_sov.iter().cloned().collect();
        }
        self.plan_route();
    }

    fn routes_dialog(&mut self, ctx: &egui::Context) {
        if !self.routes_dialog_open {
            return;
        }
        let geo = self.systems.clone();
        let nm = |id: i64| {
            geo.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_else(|| "?".into())
        };
        let mut items: Vec<RouteItem> = Vec::new();
        for r in &self.settings.saved_routes {
            items.push(RouteItem {
                kind: RouteKind::Travel,
                name: r.name.clone(),
                folder: r.folder.clone(),
                from: r.start,
                to: r.end,
                jumps: r.jumps,
                wp: r.waypoints.len(),
            });
        }
        for r in &self.settings.saved_jump_routes {
            items.push(RouteItem {
                kind: RouteKind::Jump,
                name: r.name.clone(),
                folder: r.folder.clone(),
                from: r.from,
                to: r.to,
                jumps: r.jumps,
                wp: r.waypoints.len(),
            });
        }
        let mut folders: Vec<String> = self.settings.route_folders.clone();
        for it in &items {
            if !it.folder.is_empty() && !folders.contains(&it.folder) {
                folders.push(it.folder.clone());
            }
        }
        folders.sort();
        folders.dedup();

        let q = self.route_search.trim().to_lowercase();
        let can_save = match self.route_kind {
            RouteKind::Travel => self.travel_start.is_some() && self.travel_end.is_some(),
            RouteKind::Jump => self.jump_plan_from.is_some() && self.jump_plan_to.is_some(),
        };
        let view = self.route_view;
        let editing = self.route_edit.clone();
        let mut edit_name = self.route_edit_name.clone();
        let mut edit_folder = self.route_edit_folder.clone();
        let kind_label = |k: RouteKind| match k {
            RouteKind::Travel => "Travel",
            RouteKind::Jump => "Jump",
        };

        let mut do_save = false;
        let mut new_folder = false;
        let mut to_load: Option<RouteItem> = None;
        let mut to_delete: Option<RouteItem> = None;
        let mut start_edit: Option<RouteItem> = None;
        let mut commit_edit = false;
        let mut cancel_edit = false;
        let mut open = true;

        egui::Window::new("Routes")
            .open(&mut open)
            .default_width(520.0)
            .default_height(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("route_kind")
                        .selected_text(kind_label(self.route_kind))
                        .width(78.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.route_kind, RouteKind::Jump, "Jump");
                            ui.selectable_value(&mut self.route_kind, RouteKind::Travel, "Travel");
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.route_save_name)
                            .desired_width(130.0)
                            .hint_text("Name"),
                    );
                    egui::ComboBox::from_id_salt("route_save_folder")
                        .selected_text(if self.route_save_folder.is_empty() {
                            "(root)".to_owned()
                        } else {
                            self.route_save_folder.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.route_save_folder, String::new(), "(root)");
                            for f in &folders {
                                ui.selectable_value(&mut self.route_save_folder, f.clone(), f);
                            }
                        });
                    if ui
                        .add_enabled(can_save, egui::Button::new("Save current route"))
                        .on_hover_text(if can_save {
                            "Save the current route (blank name = auto)"
                        } else {
                            "Plan a route first"
                        })
                        .clicked()
                    {
                        do_save = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.route_new_folder)
                            .desired_width(150.0)
                            .hint_text("New folder"),
                    );
                    if ui
                        .add_enabled(!self.route_new_folder.trim().is_empty(), egui::Button::new("Add folder"))
                        .clicked()
                    {
                        new_folder = true;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("View");
                    ui.selectable_value(&mut self.route_view, RouteView::ByName, "By Name");
                    ui.selectable_value(&mut self.route_view, RouteView::ByType, "By Type");
                    ui.selectable_value(&mut self.route_view, RouteView::BySystem, "By System");
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.route_search)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search routes"),
                );
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let visible: Vec<&RouteItem> =
                        items.iter().filter(|it| q.is_empty() || it.name.to_lowercase().contains(&q)).collect();
                    let mut emit = |ui: &mut egui::Ui, it: &RouteItem| {
                        let is_ed = editing
                            .as_ref()
                            .is_some_and(|(k, f, n)| *k == it.kind && *f == it.folder && *n == it.name);
                        match route_item_row(
                            ui,
                            it,
                            &nm(it.from),
                            &nm(it.to),
                            kind_label(it.kind),
                            is_ed,
                            &mut edit_name,
                            &mut edit_folder,
                            &folders,
                        ) {
                            RowAction::Load => to_load = Some(it.clone()),
                            RowAction::Delete => to_delete = Some(it.clone()),
                            RowAction::Edit => start_edit = Some(it.clone()),
                            RowAction::Commit => commit_edit = true,
                            RowAction::Cancel => cancel_edit = true,
                            RowAction::None => {}
                        }
                    };
                    if visible.is_empty() {
                        ui.label(egui::RichText::new("No saved routes yet.").weak());
                    }
                    match view {
                        RouteView::ByName => {
                            let mut v = visible.clone();
                            v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                            for it in v {
                                emit(ui, it);
                            }
                        }
                        RouteView::BySystem => {
                            let mut froms: Vec<i64> = visible.iter().map(|it| it.from).collect();
                            froms.sort();
                            froms.dedup();
                            for sys in froms {
                                let label = nm(sys);
                                egui::CollapsingHeader::new(label).default_open(true).show(ui, |ui| {
                                    for it in visible.iter().filter(|it| it.from == sys) {
                                        emit(ui, it);
                                    }
                                });
                            }
                        }
                        RouteView::ByType => {
                            for (kind, title) in
                                [(RouteKind::Jump, "Jump Routes"), (RouteKind::Travel, "Travel routes")]
                            {
                                let group: Vec<&RouteItem> =
                                    visible.iter().copied().filter(|it| it.kind == kind).collect();
                                if group.is_empty() {
                                    continue;
                                }
                                egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
                                    for it in group.iter().filter(|it| it.folder.is_empty()) {
                                        emit(ui, it);
                                    }
                                    for f in &folders {
                                        let in_f: Vec<&&RouteItem> =
                                            group.iter().filter(|it| &it.folder == f).collect();
                                        if in_f.is_empty() {
                                            continue;
                                        }
                                        egui::CollapsingHeader::new(format!(
                                            "{}  {f}",
                                            egui_phosphor::regular::FOLDER
                                        ))
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            for it in in_f {
                                                emit(ui, it);
                                            }
                                        });
                                    }
                                });
                            }
                        }
                    }
                });
            });

        self.route_edit_name = edit_name;
        self.route_edit_folder = edit_folder;

        if do_save {
            self.save_current_route();
        }
        if new_folder {
            let f = self.route_new_folder.trim().to_owned();
            if !self.settings.route_folders.contains(&f) {
                self.settings.route_folders.push(f);
            }
            self.route_new_folder.clear();
            self.needs_save = true;
        }
        if let Some(it) = to_delete {
            match it.kind {
                RouteKind::Travel => self
                    .settings
                    .saved_routes
                    .retain(|r| !(r.folder == it.folder && r.name == it.name)),
                RouteKind::Jump => self
                    .settings
                    .saved_jump_routes
                    .retain(|r| !(r.folder == it.folder && r.name == it.name)),
            }
            self.needs_save = true;
        }
        if let Some(it) = start_edit {
            self.route_edit = Some((it.kind, it.folder.clone(), it.name.clone()));
            self.route_edit_name = it.name;
            self.route_edit_folder = it.folder;
        }
        if cancel_edit {
            self.route_edit = None;
        }
        if commit_edit {
            if let Some((kind, of, on)) = self.route_edit.take() {
                let (nn, nf) = (self.route_edit_name.trim().to_owned(), self.route_edit_folder.clone());
                if !nn.is_empty() {
                    match kind {
                        RouteKind::Travel => {
                            if let Some(r) = self
                                .settings
                                .saved_routes
                                .iter_mut()
                                .find(|r| r.folder == of && r.name == on)
                            {
                                r.name = nn;
                                r.folder = nf;
                            }
                        }
                        RouteKind::Jump => {
                            if let Some(r) = self
                                .settings
                                .saved_jump_routes
                                .iter_mut()
                                .find(|r| r.folder == of && r.name == on)
                            {
                                r.name = nn;
                                r.folder = nf;
                            }
                        }
                    }
                    self.needs_save = true;
                }
            }
        }
        if let Some(it) = to_load {
            self.load_route_item(&it);
            self.routes_dialog_open = false;
        }
        if !open {
            self.routes_dialog_open = false;
        }
    }

    fn save_current_route(&mut self) {
        let nm = |id: i64| {
            self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default()
        };
        match self.route_kind {
            RouteKind::Travel => {
                if self.travel_start.is_none() || self.travel_end.is_none() {
                    return;
                }
                let name = if self.route_save_name.trim().is_empty() {
                    let mut parts = vec![nm(self.travel_start.unwrap_or(0))];
                    parts.extend(self.travel_waypoints.iter().map(|w| nm(*w)));
                    parts.push(nm(self.travel_end.unwrap_or(0)));
                    parts.join(" \u{2192} ")
                } else {
                    self.route_save_name.trim().to_owned()
                };
                self.settings.saved_routes.push(crate::settings::SavedRoute {
                    name,
                    folder: self.route_save_folder.clone(),
                    start: self.travel_start.unwrap_or(0),
                    end: self.travel_end.unwrap_or(0),
                    waypoints: self.travel_waypoints.clone(),
                    jumps: self.travel_route.as_ref().map(|r| r.len().saturating_sub(1)).unwrap_or(0),
                    constraints: Some(crate::settings::RouteConstraints {
                        sec: self.travel_sec,
                        metric: self.travel_metric.to_u8(),
                        regional_gates: self.travel_regional_gates,
                        jump_bridges: self.travel_jump_bridges,
                        avoid_camps: self.travel_avoid_camps,
                        avoid: self.travel_avoid.clone(),
                        avoid_sov: self.travel_avoid_sov.iter().cloned().collect(),
                    }),
                });
            }
            RouteKind::Jump => {
                if self.jump_plan_from.is_none() || self.jump_plan_to.is_none() {
                    return;
                }
                let name = if self.route_save_name.trim().is_empty() {
                    format!("{} \u{2192} {}", nm(self.jump_plan_from.unwrap_or(0)), nm(self.jump_plan_to.unwrap_or(0)))
                } else {
                    self.route_save_name.trim().to_owned()
                };
                self.settings.saved_jump_routes.push(crate::settings::SavedJumpRoute {
                    name,
                    folder: self.route_save_folder.clone(),
                    from: self.jump_plan_from.unwrap_or(0),
                    waypoints: self.jump_waypoints.clone(),
                    to: self.jump_plan_to.unwrap_or(0),
                    ship: self.jump_ship,
                    jdc: self.jump_jdc,
                    jfc: self.jump_jfc,
                    jumps: self.jump_route.len().saturating_sub(1),
                });
            }
        }
        self.route_save_name.clear();
        self.needs_save = true;
    }

    fn load_route_item(&mut self, it: &RouteItem) {
        match it.kind {
            RouteKind::Travel => {
                if let Some(r) = self
                    .settings
                    .saved_routes
                    .iter()
                    .find(|r| r.folder == it.folder && r.name == it.name)
                    .cloned()
                {
                    self.load_route(&r);
                    self.set_map_mode(MapMode::Travel);
                }
            }
            RouteKind::Jump => {
                if let Some(r) = self
                    .settings
                    .saved_jump_routes
                    .iter()
                    .find(|r| r.folder == it.folder && r.name == it.name)
                    .cloned()
                {
                    self.jump_plan_from = Some(r.from);
                    self.jump_waypoints = r.waypoints;
                    self.jump_plan_to = Some(r.to);
                    self.jump_ship = r.ship.min(crate::jumproute::SHIP_CLASSES.len() - 1);
                    self.jump_jdc = r.jdc.min(5);
                    self.jump_jfc = r.jfc.min(5);
                    self.jump_route_key = None;
                    self.set_map_mode(MapMode::JumpPlan);
                }
            }
        }
    }

    fn next_route_hop(&self) -> Option<i64> {
        let route = self.travel_route.as_ref()?;
        if route.len() < 2 {
            return None;
        }
        if let Some(me) = self.player_system() {
            if let Some(i) = route.iter().position(|&s| s == me) {
                return route.get(i + 1).copied();
            }
        }
        route.get(1).copied()
    }

    fn push_ingame_dest(&mut self) {
        let next = self.next_route_hop();
        if next == self.travel_ingame_dest {
            return;
        }
        if let Some(d) = next {
            let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
            self.set_destination_esi(cid, self.active_character.clone(), d);
        }
        self.travel_ingame_dest = next;
    }

    fn travel_set(&mut self, end: TravelEnd, id: i64) {
        let name = self
            .systems
            .as_ref()
            .and_then(|g| g.info_of(id))
            .map(|i| i.name.clone())
            .unwrap_or_default();
        match end {
            TravelEnd::Start => {
                self.travel_start = Some(id);
                self.travel_start_q = name;
            }
            TravelEnd::Dest => {
                self.travel_end = Some(id);
                self.travel_end_q = name;
            }
        }
        self.travel_waypoints.retain(|&w| w != id);
        self.travel_avoid.retain(|&a| a != id);
        self.plan_route();
    }

    fn clear_travel(&mut self) {
        self.travel_start = None;
        self.travel_end = None;
        self.travel_start_q.clear();
        self.travel_end_q.clear();
        self.travel_waypoints.clear();
        self.travel_avoid.clear();
        self.travel_route = None;
        self.travel_direct_route = None;
    }

    fn travel_input_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.travel_start_q.hash(&mut h);
        self.travel_end_q.hash(&mut h);
        self.travel_start.hash(&mut h);
        self.travel_end.hash(&mut h);
        self.travel_waypoints.hash(&mut h);
        self.travel_avoid.hash(&mut h);
        let mut sov: Vec<&String> = self.travel_avoid_sov.iter().collect();
        sov.sort();
        sov.hash(&mut h);
        self.travel_regional_gates.hash(&mut h);
        self.travel_jump_bridges.hash(&mut h);
        self.travel_avoid_camps.hash(&mut h);
        self.travel_sec.hash(&mut h);
        self.travel_max_ship_kills.hash(&mut h);
        (self.travel_metric as u8).hash(&mut h);
        h.finish()
    }

    fn plan_route(&mut self) {
        let Some(geo) = self.systems.clone() else { return };
        if let Some(store) = self.store.as_ref() {
            if !self.travel_start_q.trim().is_empty() {
                self.travel_start =
                    store.search_systems(&self.travel_start_q, 1).first().map(|(id, _, _)| *id);
            }
            if !self.travel_end_q.trim().is_empty() {
                self.travel_end =
                    store.search_systems(&self.travel_end_q, 1).first().map(|(id, _, _)| *id);
            }
        }
        let (Some(s), Some(e)) = (self.travel_start, self.travel_end) else {
            self.travel_route = None;
            self.travel_direct_route = None;
            self.travel_planned_hash = self.travel_input_hash();
            self.travel_dirty_at = None;
            return;
        };
        let mut points = vec![s];
        points.extend(self.travel_waypoints.iter().copied());
        points.push(e);
        let status = self.system_status.lock().unwrap();
        let max_kills = self.travel_max_ship_kills;
        let metric = self.travel_metric;
        let sec = self.travel_sec;
        let avoid = self.travel_avoid.clone();
        let avoid_sov: std::collections::HashSet<String> =
            self.travel_avoid_sov.iter().map(|s| s.to_lowercase()).collect();
        let camped: std::collections::HashSet<i64> = if self.travel_avoid_camps {
            let now = chrono::Utc::now().timestamp();
            self.camps
                .lock()
                .unwrap()
                .camped(now)
                .into_iter()
                .filter(|(_, l)| *l >= crate::camp::CampLevel::Possible)
                .map(|(id, _)| id)
                .collect()
        } else {
            std::collections::HashSet::new()
        };
        let regional = self.travel_regional_gates;
        let bridges = self.travel_jump_bridges;
        let geo2 = geo.clone();
        let allowed = |sys: i64| {
            if avoid.contains(&sys) || camped.contains(&sys) {
                return false;
            }
            if !avoid_sov.is_empty() {
                if let Some(h) = status.get(&sys).and_then(|f| f.sov.as_deref()) {
                    if avoid_sov.contains(&h.to_lowercase()) {
                        return false;
                    }
                }
            }
            // J-space has no security band to prefer, so the hisec/lowsec/null switches don't apply:
            // filtering Thera out as "null" would silently defeat routing through it.
            let sec_ok = crate::geo::is_wormhole_system(sys)
                || geo2
                    .info_of(sys)
                    .map(|i| {
                        if i.security >= 0.45 {
                            sec[0]
                        } else if i.security > 0.0 {
                            sec[1]
                        } else {
                            sec[2]
                        }
                    })
                    .unwrap_or(true);
            let activity_ok =
                max_kills == 0 || status.get(&sys).map(|f| metric.value(f)).unwrap_or(0) <= max_kills;
            sec_ok && activity_ok
        };
        let holes = if self.settings.route_via_wormholes {
            self.wh_adjacency()
        } else {
            std::collections::HashMap::new()
        };
        let mut route = vec![s];
        let mut ok = true;
        for leg in points.windows(2) {
            match geo.route_with(leg[0], leg[1], regional, bridges, &holes, allowed) {
                Some(seg) => route.extend(seg.into_iter().skip(1)),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        let prev = self.travel_route.clone();
        self.travel_route = ok.then_some(route);
        self.travel_direct_route = geo.route(s, e, true, false, |_| true);
        if self.travel_live {
            if let (Some(p), Some(n)) = (&prev, &self.travel_route) {
                if p != n {
                    let pset: std::collections::HashSet<i64> = p.iter().copied().collect();
                    let newsys: Vec<i64> = n.iter().copied().filter(|s| !pset.contains(s)).collect();
                    if !newsys.is_empty() {
                        let much_longer = n.len() > p.len() + 4;
                        self.travel_changed = newsys;
                        self.travel_changed_at = Some(chrono::Utc::now().timestamp());
                        if much_longer {
                            crate::sound::play_prio("danger", 2, 1.0);
                        }
                    }
                }
            }
        }
        self.travel_planned_hash = self.travel_input_hash();
        self.travel_dirty_at = None;
    }

    fn travel_suggestions(&self, q: &str) -> Vec<SysHit> {
        let q = q.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let Some(store) = self.store.as_ref() else { return Vec::new() };
        store
            .search_systems(q, 8)
            .into_iter()
            .map(|(id, name, sec)| {
                let (c, r) = self
                    .systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| (i.constellation.clone(), i.region.clone()))
                    .unwrap_or_default();
                (id, name, sec, c, r)
            })
            .collect()
    }

    fn jump_click_edit(&mut self, id: i64) {
        if self.jump_plan_from.is_none() {
            self.jump_plan_from = Some(id);
            return;
        }
        if Some(id) == self.jump_plan_from
            || Some(id) == self.jump_plan_to
            || self.jump_waypoints.contains(&id)
        {
            return;
        }
        if let Some(old_dest) = self.jump_plan_to.replace(id) {
            self.jump_waypoints.push(old_dest);
        }
    }

    fn persist_jump_favourites(&mut self) {
        let mut v: Vec<i64> = self.jump_favourites.iter().copied().collect();
        v.sort_unstable();
        self.settings.jump_favourites = v;
        self.needs_save = true;
    }

    fn jump_dockable_ids(&self) -> std::collections::HashSet<i64> {
        let supers = self.jump_ship == 1;
        let Some(g) = &self.systems else { return Default::default() };
        self.settings
            .jump_dock
            .iter()
            .filter(|p| if supers { p.supers } else { p.capitals || p.supers })
            .filter_map(|p| g.lookup(&p.system).map(|s| s.id))
            .collect()
    }

    fn toggle_dock_permit(&mut self, sid: i64, supers: bool) {
        let Some(name) = self.systems.as_ref().and_then(|g| g.info_of(sid).map(|s| s.name.clone())) else {
            return;
        };
        let dock = &mut self.settings.jump_dock;
        let p = match dock.iter_mut().find(|p| p.system.eq_ignore_ascii_case(&name)) {
            Some(p) => p,
            None => {
                dock.push(crate::settings::DockPermit { system: name, capitals: false, supers: false });
                dock.last_mut().unwrap()
            }
        };
        if supers {
            p.supers = !p.supers;
            if p.supers {
                p.capitals = true;
            }
        } else {
            p.capitals = !p.capitals;
        }
        dock.retain(|p| p.capitals || p.supers);
        self.jump_route_key = None;
        self.needs_save = true;
    }

    fn ensure_jump_systems(&mut self) {
        if self.jump_systems.is_none() {
            if let Some(store) = &self.store {
                self.jump_systems = Some(std::sync::Arc::new(store.all_map_systems()));
            }
        }
    }

    fn recompute_jump_route(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.jump_plan_from.hash(&mut h);
        self.jump_plan_to.hash(&mut h);
        self.jump_waypoints.hash(&mut h);
        self.jump_ship.hash(&mut h);
        self.jump_jdc.hash(&mut h);
        let dockable = self.jump_dockable_ids();
        let mut prefer: std::collections::HashSet<i64> = self.jump_favourites.clone();
        prefer.extend(dockable.iter().copied());
        let mut pref_v: Vec<i64> = prefer.iter().copied().collect();
        pref_v.sort_unstable();
        pref_v.hash(&mut h);
        let key = h.finish();
        if self.jump_route_key == Some(key) {
            return;
        }
        self.jump_route_key = Some(key);
        self.jump_legs.clear();
        self.jump_route.clear();
        self.jump_route_err = None;
        let (Some(from), Some(to)) = (self.jump_plan_from, self.jump_plan_to) else { return };
        self.ensure_jump_systems();
        let Some(systems) = self.jump_systems.clone() else { return };
        let class = crate::jumproute::SHIP_CLASSES[self.jump_ship];
        let max_ly = crate::jumproute::max_range_ly(&class, self.jump_jdc);
        let mut anchors = vec![from];
        anchors.extend(self.jump_waypoints.iter().copied());
        anchors.push(to);
        let legs = crate::jumproute::plan(&systems, max_ly, &anchors, &prefer);
        self.jump_route = crate::jumproute::flatten(&legs);
        self.jump_legs = legs;
        self.jump_alt.clear();
        for w in anchors.windows(3) {
            self.jump_alt.extend(crate::jumproute::alternatives(&systems, max_ly, w[0], w[2]));
        }
        self.jump_alt.sort_unstable();
        self.jump_alt.dedup();
    }

    fn jump_plan_content(&mut self, ui: &mut egui::Ui) {
        use crate::jumproute::{max_range_ly, SHIP_CLASSES};
        use egui_phosphor::regular as icon;

        if self.jump_plan_from.is_none() {
            self.jump_plan_from = self.player_system();
        }
        let name_of = |id: Option<i64>, sys: &Option<std::sync::Arc<crate::geo::Systems>>| -> String {
            id.and_then(|i| sys.as_ref().and_then(|g| g.info_of(i).map(|s| s.name.clone())))
                .unwrap_or_else(|| "—".to_string())
        };
        let fmt_min = |m: f64| -> String {
            let t = m.round() as i64;
            if t >= 60 {
                format!("{}h {:02}m", t / 60, t % 60)
            } else {
                format!("{t}m")
            }
        };

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Jump Plan").strong());
        ui.label(
            egui::RichText::new("Fewest-jumps capital route. Set endpoints from the map right-click menu.")
                .weak(),
        );
        ui.separator();

        egui::ComboBox::from_id_salt(ui.id().with("jump_ship"))
            .selected_text(SHIP_CLASSES[self.jump_ship].name)
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for (i, c) in SHIP_CLASSES.iter().enumerate() {
                    ui.selectable_value(&mut self.jump_ship, i, c.name);
                }
            });
        let class = SHIP_CLASSES[self.jump_ship];
        if let Some((jdc, jfc)) = self.jump_skills.lock().unwrap().take() {
            self.jump_jdc = jdc.min(5);
            self.jump_jfc = jfc.min(5);
            self.jump_route_key = None;
        }
        ui.horizontal(|ui| {
            ui.label("JDC").on_hover_text("Jump Drive Calibration (range)");
            ui.add(egui::DragValue::new(&mut self.jump_jdc).range(0..=5));
            ui.label("JFC").on_hover_text("Jump Fuel Conservation (fuel)");
            ui.add(egui::DragValue::new(&mut self.jump_jfc).range(0..=5));
            ui.label(egui::RichText::new(format!("{:.1} ly", max_range_ly(&class, self.jump_jdc))).weak());
        });
        if self.active_character != "No character" {
            let missing_skill =
                self.char_missing_scope(&self.active_character, "esi-skills.read_skills.v1");
            if ui
                .button("Use my skills (ESI)")
                .on_hover_text("Fetch Jump Drive Calibration / Fuel Conservation from ESI (needs the skills scope)")
                .clicked()
            {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                crate::esi::fetch_jump_skills(
                    cid,
                    self.active_character.clone(),
                    self.jump_skills.clone(),
                    ui.ctx().clone(),
                );
            }
            if missing_skill {
                ui.label(
                    egui::RichText::new("Re-auth this character in the Characters tab to grant the skills scope.")
                        .color(crate::theme::standing::WARNING),
                );
            }
        }
        ui.separator();

        let from_name = name_of(self.jump_plan_from, &self.systems);
        let to_name = name_of(self.jump_plan_to, &self.systems);
        ui.horizontal(|ui| {
            ui.label("From");
            ui.label(egui::RichText::new(from_name).strong());
            if ui.small_button(icon::CROSSHAIR).on_hover_text("Set to current system").clicked() {
                self.jump_plan_from = self.player_system();
            }
        });
        let mut remove_wp: Option<usize> = None;
        for (i, wp) in self.jump_waypoints.clone().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label("Via");
                ui.label(egui::RichText::new(name_of(Some(*wp), &self.systems)).strong());
                if ui.small_button(icon::X).on_hover_text("Remove waypoint").clicked() {
                    remove_wp = Some(i);
                }
            });
        }
        if let Some(i) = remove_wp {
            self.jump_waypoints.remove(i);
        }
        ui.horizontal(|ui| {
            ui.label("To");
            ui.label(egui::RichText::new(to_name).strong());
            if self.jump_plan_to.is_some() && ui.small_button(icon::X).on_hover_text("Clear").clicked() {
                self.jump_plan_to = None;
            }
        });
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Swap", icon::SWAP)).clicked() {
                std::mem::swap(&mut self.jump_plan_from, &mut self.jump_plan_to);
            }
            if (!self.jump_waypoints.is_empty() || self.jump_plan_to.is_some())
                && ui.button("Clear").on_hover_text("Clear destination + waypoints").clicked()
            {
                self.jump_waypoints.clear();
                self.jump_plan_to = None;
            }
        });
        ui.label(
            egui::RichText::new("Left-click the map to extend the route (the click becomes the destination); right-click for options.")
                .weak(),
        );
        ui.separator();

        self.recompute_jump_route();
        let any_invalid = self.jump_legs.iter().any(|l| !l.valid);

        if let Some(to) = self.jump_plan_to {
            if !self.settings.jump_dock.is_empty() && !self.jump_dockable_ids().contains(&to) {
                ui.label(
                    egui::RichText::new(format!("{} Destination has no marked dock for this hull.", icon::WARNING))
                        .color(crate::theme::standing::WARNING),
                );
            }
        }

        if ui
            .button(format!("{}  Saved routes\u{2026}", icon::FOLDER))
            .on_hover_text("Save, load and organise routes")
            .clicked()
        {
            self.route_kind = RouteKind::Jump;
            self.routes_dialog_open = true;
        }
        ui.separator();

        if let Some(err) = self.jump_route_err.clone() {
            ui.label(egui::RichText::new(err).color(crate::theme::standing::HOSTILE));
        } else if any_invalid {
            if let Some(bad) = self.jump_legs.iter().find(|l| !l.valid) {
                let a = name_of(Some(bad.from), &self.systems);
                let b = name_of(Some(bad.to), &self.systems);
                ui.label(
                    egui::RichText::new(format!(
                        "{} {a} {} {b} out of range. Add a closer waypoint.",
                        icon::WARNING,
                        icon::ARROW_RIGHT
                    ))
                    .color(crate::theme::standing::HOSTILE),
                );
            }
        } else if self.jump_route.len() >= 2 {
            if let Some(systems) = self.jump_systems.clone() {
                let cost = crate::jumproute::route_cost(&systems, &self.jump_route, &class, self.jump_jfc);
                ui.label(
                    egui::RichText::new(format!("{} jumps · {:.1} ly", cost.jumps, cost.total_ly))
                        .strong()
                        .size(16.0),
                );
                ui.label(format!("{} fuel (approx)", (cost.fuel.round() as i64)));
                ui.label(format!("Final fatigue: {}", fmt_min(cost.final_fatigue_min)));
                ui.label(format!("Total jump delay: {}", fmt_min(cost.total_delay_min)));
                ui.separator();
                ui.label(egui::RichText::new("Hops").strong());
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let route = self.jump_route.clone();
                    for (i, sid) in route.iter().enumerate() {
                        if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(*sid)).cloned() {
                            let sec = (info.security * 10.0).round() / 10.0;
                            let label = format!("{}. {:.1}  {}", i + 1, sec, info.name);
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(label).color(security_color(info.security)),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.dock_system(*sid);
                            }
                        }
                    }
                });
            }
        } else if self.jump_plan_to.is_none() {
            ui.label(
                egui::RichText::new("Right-click a system on the map for \"Plan Jump Route To Here\".")
                    .weak(),
            );
        }
    }

    fn travel_panel_content(&mut self, ui: &mut egui::Ui) {
        fn travel_field(
            ui: &mut egui::Ui,
            q: &mut String,
            sel: &mut usize,
            hint: &str,
            suggestions: &[SysHit],
        ) -> Option<i64> {
            let mut pick = None;
            let resp = ui.add(
                egui::TextEdit::singleline(q).hint_text(hint).desired_width(ui.available_width()),
            );
            if resp.changed() {
                *sel = 0;
            }
            // A singleline TextEdit surrenders focus the instant Enter is pressed, so by now
            // `has_focus` is already false. The key itself is still in the queue, so the accept has
            // to hang off `lost_focus` or Enter would never pick the highlighted suggestion.
            let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if !suggestions.is_empty() && (resp.has_focus() || entered) {
                let n = suggestions.len();
                if resp.has_focus() {
                    let (down, up) = ui.input(|i| {
                        (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp))
                    });
                    if down {
                        *sel = (*sel + 1).min(n - 1);
                    }
                    if up {
                        *sel = sel.saturating_sub(1);
                    }
                    let moving = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
                    let below = resp.rect.left_bottom() + egui::vec2(0.0, 2.0);
                    let width = resp.rect.width();
                    egui::Area::new(ui.id().with(("travel_sugg", hint)))
                        .order(egui::Order::Foreground)
                        .fixed_pos(below)
                        .constrain(true)
                        .show(ui.ctx(), |ui| {
                            ui.set_min_width(width);
                            ui.set_max_width(width);
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                for (i, (id, name, sec, c, r)) in suggestions.iter().enumerate() {
                                    let row = format!("{name}    {sec:.1}\n{c} \u{2022} {r}");
                                    let rr = ui.selectable_label(i == *sel, row);
                                    if rr.hovered() && moving {
                                        *sel = i;
                                    }
                                    if rr.clicked() {
                                        pick = Some(*id);
                                    }
                                }
                            });
                        });
                }
                if entered && pick.is_none() {
                    pick = suggestions.get((*sel).min(n - 1)).map(|x| x.0);
                }
            }
            if pick.is_some() {
                resp.surrender_focus();
            }
            pick
        }

        let name_of = |id: Option<i64>| -> Option<String> {
            id.and_then(|i| self.systems.as_ref().and_then(|g| g.info_of(i)).map(|s| s.name.clone()))
        };
        let start_name = name_of(self.travel_start);
        let end_name = name_of(self.travel_end);
        let key = (
            self.travel_start_q.clone(),
            self.travel_start,
            self.travel_end_q.clone(),
            self.travel_end,
        );
        if key != self.travel_sugg_key {
            let s0 = self.travel_suggestions(&self.travel_start_q);
            let s1 = self.travel_suggestions(&self.travel_end_q);
            self.travel_sugg = (s0, s1);
            self.travel_sugg_key = key;
        }
        let start_suggestions = self.travel_sugg.0.clone();
        let end_suggestions = self.travel_sugg.1.clone();
        if self.travel_wp_q != self.travel_wp_sugg_key {
            self.travel_wp_sugg = self.travel_suggestions(&self.travel_wp_q);
            self.travel_wp_sugg_key = self.travel_wp_q.clone();
        }
        let wp_suggestions = self.travel_wp_sugg.clone();
        // An empty From means "where I am". Skipped while a field is focused, so it cannot overwrite
        // a box the user has just cleared to type into.
        if self.travel_start.is_none()
            && self.travel_start_q.trim().is_empty()
            && ui.memory(|m| m.focused()).is_none()
        {
            if let Some(me) = self.player_system() {
                self.travel_set(TravelEnd::Start, me);
            }
        }
        let mut wp_pick: Option<i64> = None;
        let mut set_dest = false;
        let name_id = |id: i64| -> (i64, String) {
            (
                id,
                self.systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| id.to_string()),
            )
        };
        let wp_names: Vec<(i64, String)> = self.travel_waypoints.iter().map(|&id| name_id(id)).collect();
        let avoid_names: Vec<(i64, String)> = self.travel_avoid.iter().map(|&id| name_id(id)).collect();
        let mut remove_wp: Option<i64> = None;
        let mut remove_avoid: Option<i64> = None;
        let summary = self.travel_route.as_ref().map(|r| {
            let planned = r.len().saturating_sub(1);
            let holes = self
                .systems
                .as_ref()
                .map(|g| r.windows(2).filter(|w| g.is_hole_step(w[0], w[1])).count())
                .unwrap_or(0);
            let mut s = match self.travel_direct_route.as_ref().map(|d| d.len().saturating_sub(1)) {
                Some(direct) if planned > direct => {
                    format!("{planned} jumps \u{2022} direct {direct} (+{})", planned - direct)
                }
                _ => format!("{planned} jumps"),
            };
            if holes > 0 {
                s.push_str(&format!(" \u{2022} {holes} via wormhole"));
                if holes > 1 {
                    s.push('s');
                }
            }
            s
        });
        let mut clear = false;
        let mut start_pick: Option<i64> = None;
        let mut end_pick: Option<i64> = None;
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Travel route").strong().size(15.0));
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label("From");
            if let Some(id) = travel_field(
                ui,
                &mut self.travel_start_q,
                &mut self.travel_start_sel,
                start_name.as_deref().unwrap_or("system"),
                &start_suggestions,
            ) {
                start_pick = Some(id);
            }
            ui.add_space(2.0);
            ui.label("To");
            if let Some(id) = travel_field(
                ui,
                &mut self.travel_end_q,
                &mut self.travel_end_sel,
                end_name.as_deref().unwrap_or("system"),
                &end_suggestions,
            ) {
                end_pick = Some(id);
            }
            ui.label(egui::RichText::new("\u{2026}or right-click a system on the map.").weak());
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Waypoints").strong());
            for (id, name) in &wp_names {
                ui.horizontal(|ui| {
                    if ui.button(egui_phosphor::regular::X).on_hover_text("Remove").clicked() {
                        remove_wp = Some(*id);
                    }
                    ui.label(name);
                });
            }
            if let Some(id) = travel_field(
                ui,
                &mut self.travel_wp_q,
                &mut self.travel_wp_sel,
                "+ add waypoint",
                &wp_suggestions,
            ) {
                wp_pick = Some(id);
            }
            if !avoid_names.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Avoid").strong());
                for (id, name) in &avoid_names {
                    ui.horizontal(|ui| {
                        if ui.button(egui_phosphor::regular::X).on_hover_text("Remove").clicked() {
                            remove_avoid = Some(*id);
                        }
                        ui.label(name);
                    });
                }
            }
            ui.add_space(4.0);
            ui.checkbox(&mut self.travel_live, "Live mode").on_hover_text(
                "Track your position; continuously re-plan and re-route in-game on changes",
            );
            if ui
                .checkbox(&mut self.settings.travel_auto_dest, "Auto-set destination in EVE")
                .on_hover_text("When off, Live mode tracks + re-plans but never writes the route into the game")
                .changed()
            {
                self.needs_save = true;
            }
            if ui
                .button(format!("{}  Saved routes\u{2026}", egui_phosphor::regular::FOLDER))
                .on_hover_text("Save, organise and load named routes")
                .clicked()
            {
                self.route_kind = RouteKind::Travel;
                self.routes_dialog_open = true;
            }
            ui.checkbox(&mut self.travel_regional_gates, "Region-crossing gates");
            ui.checkbox(&mut self.travel_jump_bridges, "Jump bridges");
            ui.checkbox(&mut self.travel_avoid_camps, "Avoid gate camps");
            ui.horizontal(|ui| {
                ui.label("Sec");
                ui.checkbox(&mut self.travel_sec[0], "Hi");
                ui.checkbox(&mut self.travel_sec[1], "Lo");
                ui.checkbox(&mut self.travel_sec[2], "Null");
            });
            let metric_before = self.travel_metric;
            ui.horizontal(|ui| {
                ui.label("Max");
                ui.add(
                    egui::DragValue::new(&mut self.travel_max_ship_kills)
                        .range(0..=20000)
                        .custom_formatter(|n, _| {
                            if n <= 0.0 { "any".to_owned() } else { format!("{n}") }
                        }),
                );
                egui::ComboBox::from_id_salt(ui.id().with("travel_metric"))
                    .selected_text(self.travel_metric.label())
                    .show_ui(ui, |ui| {
                        for m in [
                            ActivityMode::ShipKills,
                            ActivityMode::PodKills,
                            ActivityMode::NpcKills,
                            ActivityMode::Jumps,
                        ] {
                            ui.selectable_value(&mut self.travel_metric, m, m.label());
                        }
                    });
                ui.label("/h");
            });
            if self.travel_metric != metric_before {
                self.map_overlays.activity = self.travel_metric;
            }
            if ui
                .button(format!("Avoid sov held by\u{2026} ({})", self.travel_avoid_sov.len()))
                .clicked()
            {
                self.travel_sov_dialog_open = true;
            }
            ui.add_space(4.0);
            let has_route = self.travel_start.is_some()
                || self.travel_end.is_some()
                || !self.travel_waypoints.is_empty();
            ui.horizontal(|ui| {
                if self.travel_route.is_some()
                    && ui
                        .button("Set destination")
                        .on_hover_text("Write the planned route to EVE as individual waypoints")
                        .clicked()
                {
                    set_dest = true;
                }
                if has_route && ui.button("Clear route").clicked() {
                    clear = true;
                }
            });
            match &summary {
                Some(s) => {
                    ui.label(egui::RichText::new(s).color(egui::Color32::from_rgb(0x4F, 0xC3, 0xF7)).strong());
                }
                None => {
                    ui.label(
                        egui::RichText::new("Set a from / to. The route updates automatically.")
                            .weak(),
                    );
                }
            }
        });
        if let Some(id) = start_pick {
            self.travel_start = Some(id);
            self.travel_start_q =
                self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
            self.travel_start_sel = 0;
            self.plan_route();
        }
        if let Some(id) = end_pick {
            self.travel_end = Some(id);
            self.travel_end_q =
                self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
            self.travel_end_sel = 0;
            self.plan_route();
        }
        if let Some(id) = wp_pick {
            if !self.travel_waypoints.contains(&id) {
                self.travel_waypoints.push(id);
            }
            self.travel_avoid.retain(|&a| a != id);
            self.travel_wp_q.clear();
            self.travel_wp_sel = 0;
        }
        if set_dest {
            if let Some(route) = self.travel_route.clone() {
                let mut seen = std::collections::HashSet::new();
                let unique: Vec<i64> = route.into_iter().filter(|s| seen.insert(*s)).collect();
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                crate::esi::set_route(cid, self.active_character.clone(), unique);
            }
        }
        if let Some(id) = remove_wp {
            self.travel_waypoints.retain(|&w| w != id);
            self.plan_route();
        }
        if let Some(id) = remove_avoid {
            self.travel_avoid.retain(|&a| a != id);
            self.plan_route();
        }
        if clear {
            self.clear_travel();
        }
        if self.travel_live {
            let now_t = ui.input(|i| i.time);
            if let Some(me) = self.player_system() {
                if self.travel_start != Some(me) {
                    self.travel_start = Some(me);
                    self.travel_start_q = self
                        .systems
                        .as_ref()
                        .and_then(|g| g.info_of(me))
                        .map(|i| i.name.clone())
                        .unwrap_or_default();
                }
            }
            if now_t >= self.travel_live_next {
                self.travel_live_next = now_t + 4.0;
                self.plan_route();
                if self.travel_live_base.is_none() {
                    self.travel_live_base = self.travel_route.clone();
                }
            }
            if self.settings.travel_auto_dest {
                self.push_ingame_dest();
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(900));
        } else {
            self.travel_live_base = None;
            self.travel_ingame_dest = None;
        }
        let now = ui.input(|i| i.time);
        let h = self.travel_input_hash();
        if h != self.travel_planned_hash {
            if h != self.travel_pending_hash {
                self.travel_pending_hash = h;
                self.travel_dirty_at = Some(now);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(380));
            } else if let Some(t) = self.travel_dirty_at {
                if now - t >= 0.35 {
                    self.plan_route();
                    self.travel_pending_hash = self.travel_planned_hash;
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
                }
            }
        }
    }

    fn safety_watch(&mut self, ctx: &egui::Context) {
        if self.map_mode != MapMode::Safety {
            self.safety_prev = None;
            return;
        }
        let (Some(me), Some(geo)) = (self.player_system(), self.systems.clone()) else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if now - self.safety_last_scan < 1.0 {
            return;
        }
        self.safety_last_scan = now;
        let range = self.map_threat_jumps;
        let mut current: std::collections::HashSet<i64> = std::collections::HashSet::new();
        {
            let st = self.intel_state.lock().unwrap();
            for r in &st.reports {
                if r.clear {
                    continue;
                }
                if let Some(sys) = r.primary_system() {
                    if geo.jumps(me, sys.id, range).is_some() {
                        current.insert(sys.id);
                    }
                }
            }
        }
        {
            let status = self.system_status.lock().unwrap();
            for (sid, f) in status.iter() {
                if f.ship_kills > 0 && geo.jumps(me, *sid, range).is_some() {
                    current.insert(*sid);
                }
            }
        }
        match self.safety_prev.take() {
            None => {}
            Some(prev) => {
                if current.iter().any(|s| !prev.contains(s)) {
                    crate::sound::play_prio("danger", 2, 1.0);
                    self.flash_until = ctx.input(|i| i.time) + 0.8;
                    ctx.request_repaint();
                }
            }
        }
        self.safety_prev = Some(current);
    }

    fn screen_flash(&self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now >= self.flash_until {
            return;
        }
        let alpha = (((self.flash_until - now) / 0.8).clamp(0.0, 1.0) as f32) * 0.45;
        let painter = ctx
            .layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("safety_flash")));
        painter.rect_filled(
            ctx.content_rect(),
            0.0,
            egui::Color32::from_rgb(0xEF, 0x44, 0x44).gamma_multiply(alpha),
        );
        ctx.request_repaint();
    }

    fn threat_board(&mut self, ui: &mut egui::Ui, hunting: bool) {
        let red = egui::Color32::from_rgb(0xEF, 0x53, 0x50);
        let orange = egui::Color32::from_rgb(0xFF, 0xA7, 0x26);
        let yellow = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
        let green = egui::Color32::from_rgb(0x66, 0xBB, 0x6A);
        let prox = |j: u32| if j <= 1 { red } else if j <= 3 { orange } else { yellow };

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(if hunting { "Hunting board" } else { "Safety watch" })
                .strong()
                .size(15.0),
        );
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Range");
            ui.add(egui::DragValue::new(&mut self.map_threat_jumps).range(1..=15).suffix("j"));
        });
        ui.label(
            egui::RichText::new(if hunting {
                "Targets and activity nearby, nearest first."
            } else {
                "Alarms when a new threat enters range."
            })
            .weak(),
        );

        let me_sys = self.player_system();
        let range = self.map_threat_jumps;
        let mut reports: Vec<(u32, crate::intel::IntelReport)> = Vec::new();
        let mut kills: Vec<(String, u32, u32, u32)> = Vec::new();
        if let (Some(me), Some(geo)) = (me_sys, self.systems.clone()) {
            {
                let st = self.intel_state.lock().unwrap();
                for r in &st.reports {
                    if r.clear {
                        continue;
                    }
                    if let Some(sys) = r.primary_system() {
                        if let Some(j) = geo.jumps(me, sys.id, range) {
                            reports.push((j, r.clone()));
                        }
                    }
                }
            }
            reports.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.received.cmp(&a.1.received)));
            let mut seen = std::collections::HashSet::new();
            reports.retain(|(_, r)| r.primary_system().map(|s| seen.insert(s.id)).unwrap_or(false));
            {
                let status = self.system_status.lock().unwrap();
                for (sid, f) in status.iter() {
                    if f.ship_kills == 0 && f.pod_kills == 0 {
                        continue;
                    }
                    if let Some(j) = geo.jumps(me, *sid, range) {
                        let name = geo.info_of(*sid).map(|i| i.name.clone()).unwrap_or_default();
                        kills.push((name, j, f.ship_kills, f.pod_kills));
                    }
                }
            }
            kills.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
        }

        ui.separator();
        if me_sys.is_none() {
            ui.label(egui::RichText::new("No active-character location.").weak());
            return;
        }
        let danger = !reports.is_empty();
        ui.label(
            egui::RichText::new(format!("Intel within {range}j: {}", reports.len()))
                .strong()
                .size(14.0)
                .color(if danger { red } else { green }),
        );
        let reports_only: Vec<crate::intel::IntelReport> =
            reports.into_iter().map(|(_, r)| r).collect();
        let action = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .id_salt("threat_cards")
            .show(ui, |ui| {
                let action = self.render_intel_cards(ui, &reports_only);
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Kill hotspots (last hour)").strong().size(14.0));
                if kills.is_empty() {
                    ui.label(egui::RichText::new("none in range").weak());
                }
                for (name, j, sk, pk) in kills.iter().take(15) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(name).strong().color(prox(*j)));
                        ui.label(egui::RichText::new(format!("{j}j")).weak());
                        if *sk > 0 {
                            ui.label(egui::RichText::new(format!("{sk} ship")).color(red));
                        }
                        if *pk > 0 {
                            ui.label(egui::RichText::new(format!("{pk} pod")).color(orange));
                        }
                    });
                }
                action
            })
            .inner;
        self.handle_intel_click(action, ui.ctx());
    }

    fn travel_sov_dialog(&mut self, ctx: &egui::Context) {
        if !self.travel_sov_dialog_open {
            return;
        }
        let coalitions: Vec<(String, Vec<String>)> = self
            .settings
            .coalitions
            .iter()
            .map(|c| (c.name.clone(), c.alliances.clone()))
            .collect();
        let in_coalition: std::collections::HashSet<String> =
            coalitions.iter().flat_map(|(_, m)| m.iter().cloned()).collect();
        let mut others: Vec<String> = self
            .settings
            .alliances
            .iter()
            .map(|a| a.name.clone())
            .filter(|n| !in_coalition.contains(n))
            .collect();
        others.sort();
        let npc: Vec<String> = {
            let status = self.system_status.lock().unwrap();
            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for f in status.values() {
                if f.sov_alliance.is_none() {
                    if let Some(h) = &f.sov {
                        set.insert(h.clone());
                    }
                }
            }
            set.into_iter().collect()
        };
        let mut clear = false;
        let keep = Self::dialog_viewport(
            ctx,
            "travel_sov_dialog",
            "EVE Spai \u{2014} Avoid sov",
            [420.0, 600.0],
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Tick coalitions or alliances whose sovereign space the route should \
                         avoid. Manage the groups in Settings \u{2192} Coalitions.",
                    )
                    .weak(),
                );
                if ui.button("Clear all").clicked() {
                    clear = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(egui::RichText::new("Player alliances").strong().size(14.0));
                    for (cname, members) in &coalitions {
                        egui::CollapsingHeader::new(egui::RichText::new(cname).strong())
                            .id_salt(cname)
                            .show(ui, |ui| {
                                let all = !members.is_empty()
                                    && members.iter().all(|m| self.travel_avoid_sov.contains(m));
                                let mut all_mut = all;
                                if ui.checkbox(&mut all_mut, "Avoid entire coalition").changed() {
                                    for m in members {
                                        if all_mut {
                                            self.travel_avoid_sov.insert(m.clone());
                                        } else {
                                            self.travel_avoid_sov.remove(m);
                                        }
                                    }
                                }
                                ui.separator();
                                for m in members {
                                    let mut on = self.travel_avoid_sov.contains(m);
                                    if ui.checkbox(&mut on, m).changed() {
                                        if on {
                                            self.travel_avoid_sov.insert(m.clone());
                                        } else {
                                            self.travel_avoid_sov.remove(m);
                                        }
                                    }
                                }
                            });
                    }
                    if !others.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Independent").weak());
                        for a in &others {
                            let mut on = self.travel_avoid_sov.contains(a);
                            if ui.checkbox(&mut on, a).changed() {
                                if on {
                                    self.travel_avoid_sov.insert(a.clone());
                                } else {
                                    self.travel_avoid_sov.remove(a);
                                }
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("NPC sov").strong().size(14.0));
                    if npc.is_empty() {
                        ui.label(egui::RichText::new("none in the current sov data").weak());
                    }
                    for n in &npc {
                        let mut on = self.travel_avoid_sov.contains(n);
                        if ui.checkbox(&mut on, n).changed() {
                            if on {
                                self.travel_avoid_sov.insert(n.clone());
                            } else {
                                self.travel_avoid_sov.remove(n);
                            }
                        }
                    }
                });
            },
        );
        if clear {
            self.travel_avoid_sov.clear();
        }
        if !keep {
            self.travel_sov_dialog_open = false;
        }
    }

    fn map_area(&mut self, ui: &mut egui::Ui) {
        if !self.map_overlay_mode {
            if self.left_dock_open {
                egui::Panel::left("map_standard_dock")
                    .resizable(true)
                    .default_size(212.0)
                    .size_range(170.0..=300.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("\u{00AB}").on_hover_text("Minimize panel").clicked() {
                                self.left_dock_open = false;
                            }
                            ui.label(egui::RichText::new("Map").strong());
                        });
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            self.map_controls_content(ui);
                        });
                    });
            }
            let has_mode = self.map_mode != MapMode::Standard;
            if self.right_dock_open && (has_mode || self.map_docked_system.is_some()) {
                use egui_phosphor::regular as icon;
                let mut pending: Option<(SystemInfoOut, i64)> = None;
                egui::Panel::right("map_mode_dock")
                    .resizable(true)
                    .default_size(260.0)
                    .size_range(190.0..=380.0)
                    .show_inside(ui, |ui| {
                        let has_system = self.map_docked_system.is_some();
                        if self.right_dock_tab == RightDockTab::System && !has_system {
                            self.right_dock_tab = RightDockTab::Mode;
                        }
                        if self.right_dock_tab == RightDockTab::Mode && !has_mode {
                            self.right_dock_tab = RightDockTab::System;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("\u{00BB}").on_hover_text("Minimize panel").clicked() {
                                self.right_dock_open = false;
                            }
                            if has_mode {
                                let label = match self.map_mode {
                                    MapMode::Travel => "Travel",
                                    MapMode::Safety | MapMode::Hunting => "Threat",
                                    MapMode::JumpPlan => "Jump Plan",
                                    MapMode::Standard => "",
                                };
                                if ui
                                    .selectable_label(self.right_dock_tab == RightDockTab::Mode, label)
                                    .clicked()
                                {
                                    self.right_dock_tab = RightDockTab::Mode;
                                }
                            }
                            if has_system {
                                let name = self
                                    .map_docked_system
                                    .and_then(|sid| {
                                        self.systems.as_ref().and_then(|g| g.info_of(sid).map(|i| i.name.clone()))
                                    })
                                    .unwrap_or_else(|| "System".to_string());
                                if ui
                                    .selectable_label(self.right_dock_tab == RightDockTab::System, name)
                                    .clicked()
                                {
                                    self.right_dock_tab = RightDockTab::System;
                                }
                                if ui.button(icon::ARROW_SQUARE_OUT).on_hover_text("Pop out to window").clicked() {
                                    if let Some(sid) = self.map_docked_system.take() {
                                        self.system_window = Some(sid);
                                        self.focus_window = Some(egui::ViewportId::from_hash_of("system_window"));
                                    }
                                }
                                if ui.button(icon::X).on_hover_text("Close").clicked() {
                                    self.map_docked_system = None;
                                }
                            }
                        });
                        ui.separator();
                        match self.right_dock_tab {
                            RightDockTab::Mode => match self.map_mode {
                                MapMode::Travel => self.travel_panel_content(ui),
                                MapMode::Safety => self.threat_board(ui, false),
                                MapMode::Hunting => self.threat_board(ui, true),
                                MapMode::JumpPlan => self.jump_plan_content(ui),
                                MapMode::Standard => {}
                            },
                            RightDockTab::System => {
                                if let Some(sid) = self.map_docked_system {
                                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        pending = Some((self.system_info_body(ui, sid, true), sid));
                                    });
                                }
                            }
                        }
                    });
                if let Some((out, sid)) = pending {
                    let ctx = ui.ctx().clone();
                    self.apply_system_info_out(out, sid, &ctx, true);
                }
            }
        }
        ui.push_id("map:main", |ui| self.draw_map(ui));
    }

    fn map_controls_content(&mut self, ui: &mut egui::Ui) {
        use crate::map::{MapLayout, MapView};
        use egui_phosphor::regular as icon;

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Mode");
            let mut mode = self.map_mode;
            egui::ComboBox::from_id_salt(ui.id().with("map_mode"))
                .selected_text(mode.label())
                .show_ui(ui, |ui| {
                    for m in [
                        MapMode::Standard,
                        MapMode::Travel,
                        MapMode::Hunting,
                        MapMode::Safety,
                        MapMode::JumpPlan,
                    ] {
                        ui.selectable_value(&mut mode, m, m.label());
                    }
                });
            if mode != self.map_mode {
                self.set_map_mode(mode);
            }
        });
        ui.separator();

        ui.label(egui::RichText::new("View").strong());
        if ui.button(format!("{}  Universe map", icon::GLOBE_HEMISPHERE_WEST)).clicked() {
            self.map_go(MapView::Universe);
        }
        egui::ComboBox::from_id_salt(ui.id().with("map_layout"))
            .selected_text(match self.map_layout {
                MapLayout::Geographic => "3D (geographic)",
                MapLayout::Spaced => "2D (in-game layout)",
                MapLayout::Radial => "Radial (jumps)",
                MapLayout::Tree => "Tree (jumps)",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.map_layout, MapLayout::Geographic, "3D (geographic)");
                ui.selectable_value(&mut self.map_layout, MapLayout::Spaced, "2D (in-game layout)");
                ui.selectable_value(&mut self.map_layout, MapLayout::Radial, "Radial (jumps)");
                ui.selectable_value(&mut self.map_layout, MapLayout::Tree, "Tree (jumps)");
            });
        if self.map_layout.is_threat() {
            ui.horizontal(|ui| {
                ui.label("Max jumps");
                ui.add(egui::DragValue::new(&mut self.map_threat_jumps).range(1..=15).suffix("j"));
            });
            ui.checkbox(&mut self.threat_include_bridges, "Include jump bridges");
        }
        if ui
            .add(egui::Button::new(format!("{}  Follow character", icon::CROSSHAIR)).selected(self.map_follow))
            .clicked()
        {
            self.map_follow = !self.map_follow;
        }
        if ui.button(format!("{}  Reset view", icon::ARROW_COUNTER_CLOCKWISE)).clicked() {
            self.map_pan = egui::Vec2::ZERO;
            self.map_zoom = 1.0;
            self.map_follow = false;
        }
        if self.route_destination.is_some() && ui.button(format!("{}  Clear route", icon::X)).clicked() {
            self.route_destination = None;
        }

        if !self.map_in_popout {
            ui.separator();
            ui.label(egui::RichText::new("Window").strong());
            let active = self.active_character.clone();
            let others: Vec<String> = {
                let p = self.player.lock().unwrap();
                let mut v: Vec<String> =
                    p.locations.keys().filter(|n| !n.eq_ignore_ascii_case(&active)).cloned().collect();
                v.sort();
                v
            };
            if !others.is_empty() {
                ui.menu_button(format!("{}  Pop out character map", icon::USERS_THREE), |ui| {
                    for n in &others {
                        let open = self.map_char_popouts.contains(n);
                        if ui.selectable_label(open, n).clicked() {
                            if open {
                                self.map_char_popouts.retain(|x| x != n);
                                self.map_char_view.remove(n);
                            } else {
                                self.map_char_popouts.push(n.clone());
                            }
                            ui.close();
                        }
                    }
                });
            }
            if !self.map_popped {
                if ui.button(format!("{}  Pop out map window", icon::ARROW_SQUARE_OUT)).clicked() {
                    self.map_popped = true;
                }
            } else {
                if ui
                    .add(egui::Button::new(format!("{}  Keep on top", icon::PUSH_PIN)).selected(self.map_window_on_top))
                    .clicked()
                {
                    self.map_window_on_top = !self.map_window_on_top;
                }
                if ui.button(format!("{}  Overlay mode", icon::FRAME_CORNERS)).clicked() {
                    self.map_overlay_mode = true;
                }
            }
        }

        if !self.map_layout.is_threat() {
            ui.separator();
            egui::CollapsingHeader::new(format!("{}  Layers", icon::STACK_SIMPLE))
                .default_open(true)
                .show(ui, |ui| self.map_layers_content(ui));
        }
    }

    fn map_search_overlay(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        use crate::map::MapView;
        use egui_phosphor::regular as icon;

        enum Hit {
            System { id: i64, name: String, sec: f64 },
            Constellation { name: String, region: i64 },
            Region { id: i64, name: String },
        }
        enum Action {
            Focus(i64),
            Region(i64),
        }
        let hit_action = |h: &Hit| match h {
            Hit::System { id, .. } => Action::Focus(*id),
            Hit::Constellation { region, .. } => Action::Region(*region),
            Hit::Region { id, .. } => Action::Region(*id),
        };

        let screen = ui.ctx().content_rect();
        const SEARCH_PANEL_W: f32 = 320.0;
        let query = self.map_search.trim().to_owned();
        let has_query = !query.is_empty();
        let (down, up, enter, esc) = if has_query {
            ui.input(|i| {
                use egui::Key;
                (
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Escape),
                )
            })
        } else {
            (false, false, false, false)
        };

        if !has_query {
            self.map_search_key.clear();
            self.map_search_sys.clear();
            self.map_search_const.clear();
            self.map_search_reg.clear();
        } else if query != self.map_search_key {
            let (sys, cons, reg) = if let Some(store) = &self.store {
                (
                    store.search_systems(&query, 6),
                    store
                        .search_constellations(&query, 4)
                        .into_iter()
                        .map(|(_c, name, region)| (name, region))
                        .collect::<Vec<_>>(),
                    store.search_regions(&query, 4),
                )
            } else {
                (Vec::new(), Vec::new(), Vec::new())
            };
            self.map_search_sys = sys;
            self.map_search_const = cons;
            self.map_search_reg = reg;
            let ql = query.to_lowercase();
            let mut names: std::collections::BTreeSet<String> = Default::default();
            for u in &self.settings.sov_upgrades {
                for p in split_upgrade_label(&u.upgrade) {
                    if p.to_lowercase().contains(&ql) {
                        names.insert(p.to_owned());
                    }
                }
            }
            self.map_search_upgrades = names.into_iter().take(5).collect();
            self.map_search_key = query.clone();
        }
        let mut hits: Vec<Hit> = Vec::new();
        for (id, name, sec) in &self.map_search_sys {
            hits.push(Hit::System { id: *id, name: name.clone(), sec: *sec });
        }
        for (name, region) in &self.map_search_const {
            hits.push(Hit::Constellation { name: name.clone(), region: *region });
        }
        for (id, name) in &self.map_search_reg {
            hits.push(Hit::Region { id: *id, name: name.clone() });
        }
        let mut sel = self.map_search_sel;
        if hits.is_empty() {
            sel = 0;
        } else {
            if down {
                sel = (sel + 1).min(hits.len() - 1);
            }
            if up {
                sel = sel.saturating_sub(1);
            }
            sel = sel.min(hits.len() - 1);
        }

        let mut action: Option<Action> = None;
        if esc {
            self.map_search.clear();
        }
        if enter && !hits.is_empty() {
            action = Some(hit_action(&hits[sel]));
        }
        let mut chosen_upgrade: Option<String> = None;
        let mut clear_upgrade = false;
        let mut clear_search = false;

        const INPUT_H: f32 = 40.0;
        if has_query {
            let roff = egui::vec2(
                rect.left() - screen.left() + 8.0,
                rect.bottom() - screen.bottom() - 10.0 - INPUT_H,
            );
            egui::Area::new(egui::Id::new("map_search_results"))
                .anchor(egui::Align2::LEFT_BOTTOM, roff)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(SEARCH_PANEL_W);
                        ui.set_max_width(SEARCH_PANEL_W);
                        if let Some(up) = self.map_highlight_upgrade.clone() {
                            if ui
                                .button(format!("{}  {up}  {}", icon::MAP_PIN_LINE, icon::X))
                                .on_hover_text("Clear upgrade highlight")
                                .clicked()
                            {
                                clear_upgrade = true;
                            }
                        }
                        for up in self.map_search_upgrades.clone() {
                            if ui
                                .selectable_label(
                                    self.map_highlight_upgrade.as_deref() == Some(up.as_str()),
                                    format!("{}  {up}", icon::MAP_PIN_LINE),
                                )
                                .clicked()
                            {
                                chosen_upgrade = Some(up);
                                clear_search = true;
                            }
                        }
                        if hits.is_empty() {
                            ui.label(egui::RichText::new("No match").weak());
                        } else {
                            for (i, h) in hits.iter().enumerate().rev() {
                                let label = match h {
                                    Hit::System { name, sec, .. } => egui::RichText::new(
                                        format!("{:.1}  {name}", (sec * 10.0).round() / 10.0),
                                    )
                                    .color(security_color(*sec)),
                                    Hit::Constellation { name, .. } => {
                                        egui::RichText::new(format!("{}  {name}", icon::POLYGON))
                                            .weak()
                                    }
                                    Hit::Region { name, .. } => {
                                        egui::RichText::new(format!("{}  {name}", icon::MAP_TRIFOLD))
                                            .weak()
                                    }
                                };
                                if ui.selectable_label(i == sel, label).clicked() {
                                    action = Some(hit_action(h));
                                }
                            }
                        }
                    });
                });
        }

        let ioff = egui::vec2(
            rect.left() - screen.left() + 8.0,
            rect.bottom() - screen.bottom() - 10.0,
        );
        if self.map_regions.is_empty() {
            if let Some(r) = self.store.as_ref().map(|s| s.regions()) {
                self.map_regions = r;
            }
        }
        let cur_view = self.map_view;
        let cur_region: String = match cur_view {
            MapView::Region(id) => self
                .map_regions
                .iter()
                .find(|(r, _)| *r == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "Region".to_owned()),
            MapView::Universe => "Region".to_owned(),
        };
        let region_list: Vec<(i64, String)> = self
            .map_regions
            .iter()
            .filter(|(_, n)| !is_hidden_region(n))
            .cloned()
            .collect();
        let can_back = !self.map_history.is_empty();
        let can_fwd = !self.map_forward.is_empty();
        let mut nav_back = false;
        let mut nav_fwd = false;
        let mut region_pick: Option<i64> = None;
        egui::Area::new(egui::Id::new("map_search"))
            .anchor(egui::Align2::LEFT_BOTTOM, ioff)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(SEARCH_PANEL_W);
                    ui.set_max_width(SEARCH_PANEL_W);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(can_back, |ui| {
                            if ui.button(icon::ARROW_LEFT).on_hover_text("Back").clicked() {
                                nav_back = true;
                            }
                        });
                        ui.add_enabled_ui(can_fwd, |ui| {
                            if ui.button(icon::ARROW_RIGHT).on_hover_text("Forward").clicked() {
                                nav_fwd = true;
                            }
                        });
                        egui::ComboBox::from_id_salt(ui.id().with("map_region_pick"))
                            .selected_text(cur_region.clone())
                            .show_ui(ui, |ui| {
                                for (rid, rname) in &region_list {
                                    let sel = matches!(cur_view, MapView::Region(r) if r == *rid);
                                    if ui.selectable_label(sel, rname).clicked() {
                                        region_pick = Some(*rid);
                                    }
                                }
                            });
                        ui.label(icon::MAGNIFYING_GLASS);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.map_search)
                                .id(egui::Id::new("map_search_input"))
                                .hint_text("Search system / constellation / region")
                                .desired_width(240.0),
                        );
                        if has_query && ui.button(icon::X).clicked() {
                            clear_search = true;
                        }
                    });
                });
            });
        if nav_back {
            self.map_back();
        }
        if nav_fwd {
            self.map_forward_nav();
        }
        if let Some(id) = region_pick {
            self.map_go(MapView::Region(id));
        }

        if clear_upgrade {
            self.map_highlight_upgrade = None;
        }
        if let Some(up) = chosen_upgrade {
            self.map_highlight_upgrade = Some(up);
        }
        self.map_search_sel = sel;
        match action {
            Some(Action::Focus(id)) => {
                self.map_search.clear();
                self.map_search_sel = 0;
                self.focus_map_on_select(id);
            }
            Some(Action::Region(id)) => {
                self.map_search.clear();
                self.map_search_sel = 0;
                self.map_go(MapView::Region(id));
            }
            None if clear_search => {
                self.map_search.clear();
                self.map_search_sel = 0;
            }
            None => {}
        }
    }

    /// Mean colour of an already-decoded logo. `None` while the image is still loading, so the dot
    /// keeps its security colour until the logo arrives and is then recoloured.
    fn logo_avg_color(&mut self, ctx: &egui::Context, url: &str) -> Option<egui::Color32> {
        if let Some(c) = self.logo_avg.get(url) {
            return Some(*c);
        }
        let hint = egui::SizeHint::Size { width: 32, height: 32, maintain_aspect_ratio: true };
        let egui::load::ImagePoll::Ready { image } = ctx.try_load_image(url, hint).ok()? else {
            return None;
        };
        let col = mean_logo_color(&image)?;
        self.logo_avg.insert(url.to_owned(), col);
        Some(col)
    }

    /// Per-system dot colour and icon for whoever holds the system: the alliance logo and its mean
    /// colour under player sov, the faction logo (at any security) with the dot left alone for NPCs.
    /// One fixed logo size, so zooming does not churn the URL cache; the icon is scaled when drawn.
    fn sov_art(&mut self, ctx: &egui::Context) -> std::collections::HashMap<i64, SovArt> {
        const LOGO_PX: f32 = 64.0;
        if self.map_overlays.sov == SovMode::Off {
            return std::collections::HashMap::new();
        }
        let holders: Vec<(i64, Option<i64>, Option<i64>, Option<String>)> = {
            let status = self.system_status.lock().unwrap();
            self.map_draw
                .iter()
                .filter_map(|s| {
                    let f = status.get(&s.id)?;
                    (f.sov_alliance.is_some() || f.sov_faction.is_some()).then(|| {
                        (s.id, f.sov_alliance, f.sov_faction, f.sov.clone())
                    })
                })
                .collect()
        };
        let coalition = self.map_overlays.sov == SovMode::Coalition;
        let mut out = std::collections::HashMap::new();
        for (id, alliance, faction, holder) in holders {
            let art = match (alliance, faction) {
                (Some(aid), _) => {
                    let url = eve_alliance_logo_url(aid, LOGO_PX);
                    let dot = match holder {
                        // By coalition, the coalition's own colour says more than the logo's mean.
                        Some(name) if coalition => Some(self.coalition_color_of(&name)),
                        // A colour the user picked for this alliance outranks the logo's mean.
                        Some(name) => self
                            .alliance_color_of(&name)
                            .or_else(|| self.logo_avg_color(ctx, &url)),
                        None => self.logo_avg_color(ctx, &url),
                    };
                    SovArt { icon: url, dot }
                }
                (None, Some(fid)) => match crate::factions::corporation_id(fid) {
                    Some(cid) => SovArt { icon: eve_corp_logo_url(cid, LOGO_PX), dot: None },
                    None => continue,
                },
                _ => continue,
            };
            out.insert(id, art);
        }
        out
    }

    fn coalition_color_of(&self, alliance: &str) -> egui::Color32 {
        self.settings
            .coalitions
            .iter()
            .find(|c| c.alliances.iter().any(|a| a.eq_ignore_ascii_case(alliance)))
            .map(Self::coalition_paint)
            .unwrap_or(egui::Color32::from_rgb(0x60, 0x60, 0x60))
    }

    fn focus_map_on_select(&mut self, id: i64) {
        if matches!(self.map_view, crate::map::MapView::Region(_)) {
            if let Some(r) = self.store.as_ref().and_then(|s| s.region_of_system(id)) {
                self.map_go(crate::map::MapView::Region(r));
            }
        }
        self.map_zoom = 18.0;
        self.map_focus = Some(id);
        self.map_selected = Some(id);
    }

    #[allow(deprecated)]
    fn show_map_viewport(&mut self, ctx: &egui::Context) {
        let overlay = self.map_overlay_mode;
        if overlay && self.settings.map_overlay_smart {
            let due = self.eve_focus_checked.map(|t| t.elapsed().as_millis() > 800).unwrap_or(true);
            if due {
                self.eve_focused.store(eve_is_focused(), std::sync::atomic::Ordering::Relaxed);
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        let on_top = if overlay {
            !self.settings.map_overlay_smart
                || self.eve_focused.load(std::sync::atomic::Ordering::Relaxed)
        } else {
            self.map_window_on_top
        };
        let mut keep = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("map_window"),
            egui::ViewportBuilder::default().with_icon(app_icon())
                .with_title("EVE Spai - Map")
                .with_inner_size([960.0, 720.0])
                .with_decorations(!overlay)
                .with_transparent(overlay)
                .with_resizable(!(overlay && self.map_overlay_locked))
                .with_window_level(if on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            |ctx, _class| {
                let frame = if overlay {
                    let a = (self.settings.map_overlay_opacity.clamp(0.2, 1.0) * 255.0) as u8;
                    egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(0x0A, 0x0C, 0x10, a))
                } else {
                    egui::Frame::central_panel(&ctx.style())
                };
                let locked = self.map_overlay_locked;
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    self.map_area(ui);
                    if overlay && !locked {
                        resize_grip(ui);
                    }
                });
                let want = (!overlay, !(overlay && self.map_overlay_locked));
                if self.map_vp_props != Some(want) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(want.0));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(want.1));
                    self.map_vp_props = Some(want);
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            },
        );
        if !keep {
            self.map_popped = false;
            self.map_overlay_mode = false;
            self.map_vp_props = None;
        }
    }

    #[allow(deprecated)]
    fn char_popout_windows(&mut self, ctx: &egui::Context) {
        if self.map_char_popouts.is_empty() {
            return;
        }
        let names = self.map_char_popouts.clone();
        let locs = self.player.lock().unwrap().locations.clone();
        let mut closed: Vec<String> = Vec::new();
        let (sv_view, sv_pan, sv_zoom, sv_focus, sv_follow, sv_rect) = (
            self.map_view,
            self.map_pan,
            self.map_zoom,
            self.map_focus,
            self.map_follow,
            self.map_last_rect,
        );
        self.map_in_popout = true;
        for name in &names {
            let Some(&(sys, _)) = locs.get(name) else { continue };
            let region = self.store.as_ref().and_then(|s| s.region_of_system(sys));
            let (cv, cpan, czoom, centered, crect) =
                *self.map_char_view.entry(name.clone()).or_insert_with(|| {
                    let v = region
                        .map(crate::map::MapView::Region)
                        .unwrap_or(crate::map::MapView::Universe);
                    (v, egui::Vec2::ZERO, 6.0, false, None)
                });
            self.map_view = cv;
            self.map_pan = cpan;
            self.map_zoom = czoom;
            self.map_focus = if centered { None } else { Some(sys) };
            self.map_follow = false;
            self.map_last_rect = crect;
            let mut keep = true;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(format!("charmap_{name}")),
                egui::ViewportBuilder::default().with_icon(app_icon())
                    .with_title(format!("EVE Spai - {name}"))
                    .with_inner_size([640.0, 520.0])
                    .with_min_inner_size([360.0, 280.0]),
                |ctx, _| {
                    egui::CentralPanel::default().show(ctx, |ui| { ui.push_id(name.as_str(), |ui| self.draw_map(ui)); });
                    ontop_pin(ctx, &format!("charmap_{name}"));
                    if ctx.input(|i| i.viewport().close_requested()) {
                        keep = false;
                    }
                },
            );
            self.map_char_view.insert(
                name.clone(),
                (self.map_view, self.map_pan, self.map_zoom, true, self.map_last_rect),
            );
            if !keep {
                closed.push(name.clone());
            }
        }
        self.map_view = sv_view;
        self.map_pan = sv_pan;
        self.map_zoom = sv_zoom;
        self.map_focus = sv_focus;
        self.map_follow = sv_follow;
        self.map_last_rect = sv_rect;
        self.map_in_popout = false;
        for n in closed {
            self.map_char_popouts.retain(|x| x != &n);
            self.map_char_view.remove(&n);
        }
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
        if let Some(store) = &self.store {
            if let Err(e) = store.save_settings(&self.settings) {
                eprintln!("save settings: {e:#}");
            }
        }
        self.needs_save = false;
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar")
            .exact_size(40.0)
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Character").weak());
                    egui::ComboBox::from_id_salt("active_character")
                        .selected_text(&self.active_character)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.active_character,
                                "No character".to_owned(),
                                "No character",
                            );
                            for c in &self.characters {
                                ui.selectable_value(
                                    &mut self.active_character,
                                    c.name.clone(),
                                    &c.name,
                                );
                            }
                        });

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
                                "CPU {:.0}%   RAM {}",
                                self.proc_monitor.cpu_percent,
                                self.proc_monitor.rss_human(),
                            ))
                            .weak(),
                        )
                        .on_hover_text("CPU (share of one core) · resident memory");
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
        egui::Panel::left("nav_rail")
            .resizable(false)
            .exact_size(width)
            .show_inside(ui, |ui| {
                let mut expanded = self.settings.nav_expanded;
                let badged: &[nav::View] = if badge { &[nav::View::Jabber] } else { &[] };
                let selected = nav::rail(ui, self.view, &mut expanded, badged);
                if selected != self.view {
                    self.view = selected;
                }
                if expanded != self.settings.nav_expanded {
                    self.settings.nav_expanded = expanded;
                    self.needs_save = true;
                }
            });
    }

    fn system_info_body(&mut self, ui: &mut egui::Ui, id: i64, docked: bool) -> SystemInfoOut {
        let mut nav: Option<i64> = None;
        let mut show_on_map = false;
        let now = chrono::Utc::now().timestamp();

        let ttl = self.settings.intel_ttl_secs;
        let player_sys = self.player_system();
        let (sys_reports, stale_flags, sys_last_ship): (
            Vec<crate::intel::IntelReport>,
            Vec<bool>,
            std::collections::HashMap<String, (i64, String, i64)>,
        ) = {
            let st = self.intel_state.lock().unwrap();
            let mut reps = Vec::new();
            let mut stale = Vec::new();
            for r in st.reports.iter().rev() {
                if r.systems.iter().any(|s| s.id == id) {
                    stale.push(st.is_stale(r) || (now - r.received) > ttl);
                    reps.push(r.clone());
                }
            }
            (reps, stale, build_last_ship(&st.reports))
        };
        let ship_ids: std::collections::HashSet<i64> =
            sys_reports.iter().flat_map(|r| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
            ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp = cache
                .display_ids(sys_reports.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let status_snapshot = self.system_status.lock().unwrap().clone();
        let mut intel_click: Option<IntelClick> = None;
        let constellation = self.store.as_ref().and_then(|s| s.constellation_of_system(id));
        let region_loc = self.store.as_ref().and_then(|s| s.region_of_system(id));
        let mut open_const: Option<i64> = None;
        let mut open_region: Option<i64> = None;

        let Some(graph) = self.systems.clone() else {
            ui.label("SDE not ready.");
            return SystemInfoOut::default();
        };
        let Some(info) = graph.info_of(id).cloned() else {
            ui.label("Unknown system.");
            return SystemInfoOut::default();
        };

        {
            let status = self.system_status.lock().unwrap();
            let flags = status.get(&id).cloned().unwrap_or_default();
            let adm_color = |adm: f64| {
                if adm >= 5.0 {
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
                } else if adm >= 3.0 {
                    crate::theme::standing::WARNING
                } else {
                    crate::theme::standing::HOSTILE
                }
            };
            ui.horizontal(|ui| {
                ui.label(security_badge(info.security));
                ui.heading(&info.name);
                let teal = egui::Color32::from_rgb(0x4D, 0xB6, 0xAC);
                let marked = self.settings.bookmarks.contains(&id);
                let icon = egui::RichText::new(egui_phosphor::regular::BOOKMARK_SIMPLE)
                    .size(18.0)
                    .color(if marked { teal } else { ui.visuals().weak_text_color() });
                if ui
                    .add(egui::Button::new(icon).frame(false))
                    .on_hover_text(if marked { "Remove bookmark" } else { "Bookmark this system" })
                    .clicked()
                {
                    if marked {
                        self.settings.bookmarks.retain(|&b| b != id);
                    } else {
                        self.settings.bookmarks.push(id);
                    }
                    self.needs_save = true;
                }
                if docked {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(adm) = flags.adm {
                            ui.label(
                                egui::RichText::new(format!("ADM {adm:.1}")).color(adm_color(adm)).strong(),
                            )
                            .on_hover_text("Activity Defense Multiplier");
                        }
                        if let Some(aid) = flags.sov_alliance {
                            let url = eve_alliance_logo_url(aid, 28.0);
                            let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(28.0)));
                            if let Some(sov) = &flags.sov {
                                r.on_hover_text(sov);
                            }
                        }
                    });
                }
            });
            if !docked && (flags.sov_alliance.is_some() || flags.adm.is_some()) {
                egui::Area::new(egui::Id::new("sys_sov"))
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        ui.vertical_centered(|ui| {
                            if let Some(aid) = flags.sov_alliance {
                                let url = eve_alliance_logo_url(aid, 64.0);
                                let r = ui.add(
                                    egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(64.0)),
                                );
                                if let Some(sov) = &flags.sov {
                                    r.on_hover_text(sov);
                                }
                            }
                            if let Some(adm) = flags.adm {
                                let col = if adm >= 5.0 {
                                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
                                } else if adm >= 3.0 {
                                    crate::theme::standing::WARNING
                                } else {
                                    crate::theme::standing::HOSTILE
                                };
                                ui.label(egui::RichText::new(format!("ADM {adm:.1}")).color(col).strong())
                                    .on_hover_text(
                                        "Activity Defense Multiplier (ESI gives only the \
                                         total, not the military/industry/strategic split)",
                                    );
                            }
                        });
                    });
            }
            system_chips_ex(ui, &self.systems, &status, id, false, false);
            ui.horizontal_wrapped(|ui| {
                if let Some((cid, cname)) = &constellation {
                    if ui.link(egui::RichText::new(cname).weak()).clicked() {
                        open_const = Some(*cid);
                    }
                }
                if let Some(r) = region_loc {
                    ui.label(egui::RichText::new("‹").weak());
                    if ui.link(egui::RichText::new(&info.region).weak()).clicked() {
                        open_region = Some(r);
                    }
                }
            });
            let region_ids: Vec<i64> = self
                .store
                .as_ref()
                .and_then(|s| s.region_of_system(id).map(|r| s.region_systems(r)))
                .map(|v| v.into_iter().map(|m| m.id).collect())
                .unwrap_or_default();
            let avg = |sel: &dyn Fn(&crate::systemstatus::SysFlags) -> u32| -> f64 {
                if region_ids.is_empty() {
                    return 0.0;
                }
                let sum: u64 = region_ids.iter().filter_map(|s| status.get(s)).map(|f| sel(f) as u64).sum();
                sum as f64 / region_ids.len() as f64
            };
            let (aj, ak, an) = (avg(&|f| f.jumps), avg(&|f| f.ship_kills), avg(&|f| f.npc_kills));
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Last hour:").weak());
                let stat = |ui: &mut egui::Ui, label: &str, v: u32, avg: f64| {
                    let col = if avg > 0.0 && v as f64 >= 2.0 * avg {
                        crate::theme::standing::HOSTILE
                    } else if avg > 0.0 && v as f64 > avg {
                        crate::theme::standing::WARNING
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.label(egui::RichText::new(format!("{v} {label}")).color(col));
                };
                stat(ui, "jumps", flags.jumps, aj);
                stat(ui, "ship kills", flags.ship_kills, ak);
                stat(ui, "pod kills", flags.pod_kills, ak);
                stat(ui, "NPC kills", flags.npc_kills, an);
            });
        }
        self.camp_line(ui, info.id);
        if let Some(rp) = crate::rats::rat_profile(&info.region) {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{}  rats", egui_phosphor::regular::SKULL)).strong());
                ui.label(egui::RichText::new(rp.faction).strong());
            });
            ui.label(
                egui::RichText::new(format!(
                    "Deals {} / {}   ·   weak to {} / {}",
                    rp.deal[0], rp.deal[1], rp.weak[0], rp.weak[1]
                ))
                .weak(),
            )
            .on_hover_text("Tank against the damage they deal; deal the damage they're weak to.");
            if rp.ewar != "None" {
                ui.label(egui::RichText::new(format!("EWAR: {}", rp.ewar)).weak());
            }
        }
        self.wormhole_section(ui, id);
        let upgrades: Vec<&str> = self
            .settings
            .sov_upgrades
            .iter()
            .filter(|u| u.system.eq_ignore_ascii_case(&info.name))
            .flat_map(|u| split_upgrade_label(&u.upgrade))
            .collect();
        if !upgrades.is_empty() {
            ui.label(egui::RichText::new("Sov upgrades").weak());
            for u in upgrades {
                let (kind, level) = upgrade_info(u);
                let lcol = level_color(level);
                ui.horizontal(|ui| {
                    match kind {
                        UpgradeIcon::Glyph(g) => {
                            ui.label(egui::RichText::new(g).color(lcol).size(16.0));
                        }
                        UpgradeIcon::Mineral(tid) => {
                            let url = eve_type_icon_url(tid, 18.0);
                            ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(18.0, 18.0)));
                        }
                    }
                    ui.label(egui::RichText::new(u).color(crate::theme::standing::CORP));
                });
            }
        }
        let has_char = self.active_character != "No character";
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        let cname = self.active_character.clone();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Show on map").clicked() {
                show_on_map = true;
            }
            if ui.add_enabled(has_char, egui::Button::new("Set Destination")).clicked() {
                self.set_destination_esi(cid.clone(), cname.clone(), id);
                self.route_destination = Some(id);
            }
            if ui.add_enabled(has_char, egui::Button::new("Add Waypoint")).clicked() {
                crate::esi::set_waypoint(cid.clone(), cname.clone(), id, false);
            }
        });
        ui.separator();

        let state = self.intel_state.lock().unwrap();
        let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for r in &state.reports {
            if r.clear || state.is_stale(r) {
                continue;
            }
            for s in &r.systems {
                *counts.entry(s.id).or_default() += 1;
            }
        }
        drop(state);

        ui.label(egui::RichText::new("Neighbours").strong());
        ui.horizontal_wrapped(|ui| {
            for &nid in graph.neighbors(id) {
                if let Some(ni) = graph.info_of(nid) {
                    let cnt = counts.get(&nid).copied().unwrap_or(0);
                    let sec = (ni.security * 10.0).round() / 10.0;
                    let mut label = format!("{sec:.1} {}", ni.name);
                    if cnt > 0 {
                        label.push_str(&format!(" ({cnt})"));
                    }
                    let text = egui::RichText::new(label).color(security_color(ni.security)).strong();
                    let mut btn = egui::Button::new(text);
                    let cross_region = ni.region != info.region && !ni.region.is_empty();
                    let cross_const = ni.constellation != info.constellation;
                    if cross_region {
                        btn = btn.fill(ui.visuals().hyperlink_color.gamma_multiply(0.22));
                    } else if cross_const {
                        btn = btn.fill(ui.visuals().hyperlink_color.gamma_multiply(0.10));
                    }
                    let mut resp = ui.add(btn);
                    let arrow = egui_phosphor::regular::ARROW_RIGHT;
                    if cross_region {
                        resp = resp.on_hover_text(format!("{arrow} {} ({})", ni.constellation, ni.region));
                    } else if cross_const {
                        resp = resp.on_hover_text(format!("{arrow} {}", ni.constellation));
                    }
                    if cnt > 0 {
                        resp = resp.on_hover_text(format!("{cnt} active intel"));
                    }
                    if resp.clicked() {
                        nav = Some(nid);
                    }
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.system_kills_tab, false, "Intel");
            ui.selectable_value(&mut self.system_kills_tab, true, "Recent kills");
        });
        ui.separator();
        if self.system_kills_tab {
            let feed = self
                .system_kills_cache
                .entry(id)
                .or_insert_with(|| {
                    let s =
                        std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle));
                    crate::lookup::spawn_system_kills(id, s.clone(), ui.ctx().clone());
                    s
                })
                .clone();
            egui::ScrollArea::vertical().id_salt("syskills").max_height(280.0).show(ui, |ui| {
                match feed.lock().unwrap().clone() {
                    crate::lookup::LookupState::Done(report) => {
                        self.km_list(ui, &report.kills, report.loading, false);
                    }
                    crate::lookup::LookupState::Failed(e) => {
                        ui.label(egui::RichText::new(e).weak());
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading kills\u{2026}");
                        });
                    }
                }
            });
        } else {
            egui::ScrollArea::vertical().id_salt("sysintel").max_height(280.0).show(ui, |ui| {
                if sys_reports.is_empty() {
                    ui.label(egui::RichText::new("No recent intel.").weak());
                }
                for (i, r) in sys_reports.iter().enumerate() {
                    let from_you = jumps_from_you(
                        &self.systems,
                        player_sys,
                        r.primary_system().map(|s| s.id),
                    );
                    let sev = severity_of(r, &self.settings.severity);
                    let kc = self.kill_cache.clone();
                    let affil = self.affiliations.clone();
                    if let Some(c) = intel_row(
                        ui, r, now, stale_flags[i], from_you, &self.systems, &status_snapshot,
                        &ship_details, &ship_roles, &resolved_pilots, &uncertain, &sys_last_ship,
                        &kc, sev, false,
                    &affil, false, &mut None,
                    ) {
                        intel_click = Some(c);
                    }
                }
            });
        }
        // TODO: neighbouring intel density over time (sparkline) — deferred.
        SystemInfoOut { nav, show_on_map, intel_click, open_const, open_region }
    }

    fn apply_system_info_out(
        &mut self,
        out: SystemInfoOut,
        id: i64,
        ctx: &egui::Context,
        docked: bool,
    ) {
        if let Some(nid) = out.nav {
            if docked {
                self.map_docked_system = Some(nid);
            } else {
                self.system_window = Some(nid);
            }
        }
        if let Some(c) = out.open_const {
            self.constellation_window = Some(c);
            self.focus_window = Some(egui::ViewportId::from_hash_of("constellation_window"));
        }
        if let Some(r) = out.open_region {
            self.region_window = Some(r);
            self.focus_window = Some(egui::ViewportId::from_hash_of("region_window"));
        }
        match out.intel_click {
            Some(IntelClick::System(sid)) => self.open_system(sid),
            Some(IntelClick::Ship(sid)) => self.open_ship(sid),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ctx.clone());
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            Some(IntelClick::Dscan(url)) => self.open_dscan(url, ctx),
            Some(IntelClick::PilotVerdict(name)) => self.open_pilot_verdict(name),
            None => {}
        }
        if out.show_on_map {
            self.view = View::Map;
            if let Some(r) = self.store.as_ref().and_then(|s| s.region_of_system(id)) {
                self.map_go(crate::map::MapView::Region(r));
            }
            self.map_focus = Some(id);
        }
    }

    fn system_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.system_window else {
            return;
        };
        let mut out = SystemInfoOut::default();
        let keep = Self::dialog_viewport(
            ctx,
            "system_window",
            "EVE Spai - System info",
            [470.0, 660.0],
            |ui| {
                out = self.system_info_body(ui, id, false);
            },
        );
        self.apply_system_info_out(out, id, ctx, false);
        if !keep {
            self.system_window = None;
        }
    }

    /// Only a colour the user actually set. No name-hashed fallback: on the map the logo's own mean
    /// colour is a better guess than a random hue.
    fn alliance_color_of(&self, name: &str) -> Option<egui::Color32> {
        self.settings
            .alliances
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .and_then(|a| a.color)
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
    }

    fn coalition_paint(c: &crate::settings::Coalition) -> egui::Color32 {
        c.color
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
            .unwrap_or_else(|| name_color(&c.name))
    }

    fn discover_sov_alliances(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.sov_discover_last < 3.0 {
            return;
        }
        self.sov_discover_last = now;
        let names: std::collections::HashSet<String> = {
            let st = self.system_status.lock().unwrap();
            st.values().filter_map(|f| f.sov.clone()).collect()
        };
        let mut added = false;
        for name in names {
            if !self.settings.alliances.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
                self.settings.alliances.push(crate::settings::AllianceConfig { name, color: None });
                added = true;
            }
        }
        if added {
            self.settings.alliances.sort_by(|a, b| a.name.cmp(&b.name));
            self.needs_save = true;
        }
    }

    fn dominant_alliances(&self, ids: &[i64]) -> Vec<(i64, Option<String>, usize)> {
        let status = self.system_status.lock().unwrap();
        let mut counts: std::collections::HashMap<i64, (Option<String>, usize)> =
            std::collections::HashMap::new();
        for &id in ids {
            if let Some(f) = status.get(&id) {
                if let Some(aid) = f.sov_alliance {
                    let e = counts.entry(aid).or_insert((f.sov.clone(), 0));
                    e.1 += 1;
                    if e.0.is_none() {
                        e.0 = f.sov.clone();
                    }
                }
            }
        }
        let mut v: Vec<(i64, Option<String>, usize)> =
            counts.into_iter().map(|(k, (n, c))| (k, n, c)).collect();
        v.sort_by(|a, b| b.2.cmp(&a.2));
        v.truncate(3);
        v
    }

    fn dominant_logos(&self, ui: &mut egui::Ui, ids: &[i64], area_id: &str) {
        let dom = self.dominant_alliances(ids);
        if dom.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new(area_id.to_owned()))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    for (i, (aid, name, count)) in dom.iter().enumerate() {
                        let sz = if i == 0 { 56.0 } else { 34.0 };
                        let url = eve_alliance_logo_url(aid, sz);
                        let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(sz)));
                        let label = name.clone().unwrap_or_else(|| "Alliance".to_owned());
                        r.on_hover_text(format!("{label} — {count} systems"));
                    }
                });
            });
    }

    fn rat_line(ui: &mut egui::Ui, region_name: &str) {
        if let Some(rp) = crate::rats::rat_profile(region_name) {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{}  rats", egui_phosphor::regular::SKULL)).strong());
                ui.label(egui::RichText::new(rp.faction).strong());
            });
            ui.label(
                egui::RichText::new(format!(
                    "Deals {} / {}   ·   weak to {} / {}",
                    rp.deal[0], rp.deal[1], rp.weak[0], rp.weak[1]
                ))
                .weak(),
            );
            if rp.ewar != "None" {
                ui.label(egui::RichText::new(format!("EWAR: {}", rp.ewar)).weak());
            }
        }
    }

    fn constellation_window(&mut self, ctx: &egui::Context) {
        let Some(cid) = self.constellation_window else { return };
        let Some(store) = &self.store else { return };
        let name = store.constellation_name(cid).unwrap_or_else(|| "Constellation".to_owned());
        let region = store.region_of_constellation(cid);
        let region_name = region.and_then(|r| store.region_name(r)).unwrap_or_default();
        let systems = store.constellation_systems(cid);
        let neighbours = store.constellation_neighbours(cid);
        let sys_ids: Vec<i64> = systems.iter().map(|s| s.id).collect();

        let mut open_region: Option<i64> = None;
        let mut open_constellation: Option<i64> = None;
        let mut open_system: Option<i64> = None;
        let keep = Self::dialog_viewport(
            ctx,
            "constellation_window",
            "EVE Spai - Constellation",
            [420.0, 560.0],
            |ui| {
                ui.heading(&name);
                if let Some(r) = region {
                    if ui.link(egui::RichText::new(format!("◤ {region_name}")).weak()).clicked() {
                        open_region = Some(r);
                    }
                }
                self.dominant_logos(ui, &sys_ids, "constellation_dom");
                Self::rat_line(ui, &region_name);
                if !neighbours.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new("Neighbouring constellations").strong());
                    ui.horizontal_wrapped(|ui| {
                        for (nid, nname) in &neighbours {
                            if ui.button(nname).clicked() {
                                open_constellation = Some(*nid);
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(egui::RichText::new(format!("Systems ({})", systems.len())).strong());
                let h = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("const_sys")
                    .auto_shrink([false, false])
                    .max_height(h)
                    .show(ui, |ui| {
                        for s in &systems {
                            ui.horizontal(|ui| {
                                ui.label(security_badge(s.security));
                                if ui.link(&s.name).clicked() {
                                    open_system = Some(s.id);
                                }
                            });
                        }
                    });
            },
        );
        if let Some(r) = open_region {
            self.region_window = Some(r);
            self.focus_window = Some(egui::ViewportId::from_hash_of("region_window"));
        }
        if let Some(c) = open_constellation {
            self.constellation_window = Some(c);
        }
        if let Some(s) = open_system {
            self.open_system(s);
        }
        if !keep {
            self.constellation_window = None;
        }
    }

    fn region_window(&mut self, ctx: &egui::Context) {
        let Some(rid) = self.region_window else { return };
        let Some(store) = &self.store else { return };
        let name = store.region_name(rid).unwrap_or_else(|| "Region".to_owned());
        let constellations = store.constellations_in_region(rid);
        let neighbours = store.region_neighbours(rid);
        let sys_ids: Vec<i64> = store.region_systems(rid).iter().map(|s| s.id).collect();

        let mut open_constellation: Option<i64> = None;
        let mut open_region: Option<i64> = None;
        let mut show_map = false;
        let keep = Self::dialog_viewport(
            ctx,
            "region_window",
            "EVE Spai - Region",
            [420.0, 580.0],
            |ui| {
                ui.heading(&name);
                self.dominant_logos(ui, &sys_ids, "region_dom");
                Self::rat_line(ui, &name);
                ui.separator();
                if ui.button("Show on map").clicked() {
                    show_map = true;
                }
                if !neighbours.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new("Neighbouring regions").strong());
                    ui.horizontal_wrapped(|ui| {
                        for (nid, nname) in &neighbours {
                            if ui.button(nname).clicked() {
                                open_region = Some(*nid);
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(egui::RichText::new(format!("Constellations ({})", constellations.len())).strong());
                let h = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("region_const")
                    .auto_shrink([false, false])
                    .max_height(h)
                    .show(ui, |ui| {
                        for (cid, cname) in &constellations {
                            if ui.link(cname).clicked() {
                                open_constellation = Some(*cid);
                            }
                        }
                    });
            },
        );
        if let Some(c) = open_constellation {
            self.constellation_window = Some(c);
            self.focus_window = Some(egui::ViewportId::from_hash_of("constellation_window"));
        }
        if let Some(r) = open_region {
            self.region_window = Some(r);
        }
        if show_map {
            self.view = View::Map;
            self.map_go(crate::map::MapView::Region(rid));
        }
        if !keep {
            self.region_window = None;
        }
    }

    fn ship_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.ship_window else {
            return;
        };
        let details = self.store.as_ref().and_then(|s| s.ship_details(id));
        let traits = self.store.as_ref().map(|s| s.ship_traits(id)).unwrap_or_default();
        let roles = derive_roles(&traits);
        let skill_ids: Vec<i64> = {
            let mut s: Vec<i64> = traits.iter().map(|t| t.0).filter(|&s| s > 0).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        self.ensure_type_names(&skill_ids, ctx);
        let names = self.type_names.lock().unwrap().clone();
        let keep = Self::dialog_viewport(ctx, "ship_window", "EVE Spai - Ship", [380.0, 600.0], |ui| {
            ui.horizontal(|ui| {
                let url = eve_type_render_url(id, 96.0);
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(96.0)));
                ui.vertical(|ui| {
                    match &details {
                        Some(d) => {
                            ui.heading(&d.name);
                            let size = hull_size(&d.group);
                            let type_line = if size.is_empty() || d.group.contains(size) {
                                d.group.clone()
                            } else {
                                format!("{size} · {}", d.group)
                            };
                            ui.label(egui::RichText::new(type_line).weak());
                        }
                        None => {
                            ui.heading("Ship");
                            ui.label(egui::RichText::new("No SDE details.").weak());
                        }
                    }
                    role_badges(ui, &roles);
                });
            });
            ui.separator();
            if let Some(d) = &details {
                ship_stats(ui, d);
            }
            if !traits.is_empty() {
                ui.separator();
                let fmt = |bonus: f64, text: &str| {
                    if bonus != 0.0 {
                        format!("• {bonus:.0}% {text}")
                    } else {
                        format!("• {text}")
                    }
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .id_salt("ship_traits")
                    .show(ui, |ui| {
                        let mut skills: Vec<i64> = Vec::new();
                        for (s, _, _) in &traits {
                            if *s > 0 && !skills.contains(s) {
                                skills.push(*s);
                            }
                        }
                        for skill in &skills {
                            let sname =
                                names.get(skill).cloned().unwrap_or_else(|| "…".to_owned());
                            ui.label(egui::RichText::new(format!("{sname} (per level)")).strong());
                            ui.indent(*skill, |ui| {
                                for (s, bonus, text) in &traits {
                                    if s == skill {
                                        ui.label(fmt(*bonus, text));
                                    }
                                }
                            });
                        }
                        let role: Vec<&(i64, f64, String)> =
                            traits.iter().filter(|t| t.0 == -1).collect();
                        if !role.is_empty() {
                            ui.label(egui::RichText::new("Role Bonuses").strong());
                            ui.indent("trait_role", |ui| {
                                for (_, bonus, text) in role {
                                    ui.label(fmt(*bonus, text));
                                }
                            });
                        }
                    });
            }
        });
        if !keep {
            self.ship_window = None;
        }
    }

    fn poll_update_check(&mut self, ctx: &egui::Context) {
        let first = self.update_checked_at.is_none();
        let due = self
            .update_checked_at
            .is_none_or(|t| t.elapsed() >= crate::update::CHECK_EVERY);
        if !due {
            return;
        }
        // A download already finished or is running: leave it be, the dialog owns the flow now.
        {
            let st = self.update.lock().unwrap();
            if st.installing || st.done {
                return;
            }
        }
        self.update_checked_at = Some(std::time::Instant::now());
        if first {
            crate::update::cleanup_old();
        } else {
            // "Ask me again later" means later, and an hour is later.
            self.update_dismissed = false;
        }
        crate::update::spawn_check(
            self.update.clone(),
            self.settings.update_skip_version.clone(),
            false,
            ctx.clone(),
        );
        // Nothing else is guaranteed to wake the app in an hour's time.
        ctx.request_repaint_after(crate::update::CHECK_EVERY);
    }

    fn update_dialog(&mut self, ctx: &egui::Context) {
        let st = self.update.lock().unwrap().clone();
        let Some(av) = st.available.clone() else { return };
        if self.update_dismissed || av.version == self.settings.update_skip_version {
            return;
        }
        let mut close = false;
        let mut start_install = false;
        let mut restart = false;
        egui::Window::new(format!("{}  Update available", egui_phosphor::regular::DOWNLOAD_SIMPLE))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .show(ctx, |ui| {
                if st.done {
                    ui.label(format!("Updated to v{}. It applies on restart.", av.version));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Restart now").clicked() {
                            restart = true;
                        }
                        if ui.button("Later").clicked() {
                            close = true;
                        }
                    });
                    return;
                }
                if st.installing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Downloading update…");
                    });
                    return;
                }
                if let Some(e) = &st.error {
                    ui.colored_label(crate::theme::standing::WARNING, format!("Update failed: {e}"));
                    ui.hyperlink_to("Download manually", &av.html_url);
                    ui.add_space(4.0);
                }
                ui.label(format!(
                    "EVE Spai v{} is available. You have v{}.",
                    av.version,
                    crate::update::current()
                ));
                ui.hyperlink_to("Release notes", &av.html_url);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Yes, update").clicked() {
                        start_install = true;
                    }
                    if ui.button("No").clicked() {
                        self.settings.update_skip_version = av.version.clone();
                        self.needs_save = true;
                        close = true;
                    }
                    if ui.button("Ask me again later").clicked() {
                        self.update_dismissed = true;
                    }
                });
            });

        if start_install {
            match &av.asset_api_url {
                Some(url) => {
                    self.update.lock().unwrap().installing = true;
                    let (upd, url, ctx2) = (self.update.clone(), url.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        // In a machine-wide install the exe dir needs elevation to overwrite, so hand
                        // the swap to an admin helper (UAC prompt); otherwise do it in-process.
                        let res = if crate::update::update_needs_admin() {
                            crate::update::elevated_update(&url)
                        } else {
                            crate::update::download_and_replace(&url)
                        };
                        let mut s = upd.lock().unwrap();
                        s.installing = false;
                        match res {
                            Ok(()) => s.done = true,
                            Err(e) => s.error = Some(format!("{e:#}")),
                        }
                        ctx2.request_repaint();
                    });
                }
                None => {
                    let _ = open::that(&av.html_url);
                    close = true;
                }
            }
        }
        if restart {
            // Closing runs `on_exit` (settings persisted, overlay child shut down) and only then is
            // the single-instance lock free, so main.rs does the relaunch after the loop returns.
            crate::update::request_restart();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if close {
            self.update.lock().unwrap().available = None;
        }
    }

    /// Feedback for a manual "Check for updates": a spinner, then either "you're on the latest" or a
    /// connection error. The automatic hourly check never reaches here (it doesn't set these flags),
    /// and a found update is handled by `update_dialog` instead.
    fn update_check_dialog(&mut self, ctx: &egui::Context) {
        let st = self.update.lock().unwrap().clone();
        let show = st.checking || st.up_to_date || st.check_failed.is_some();
        if !show || st.available.is_some() {
            return;
        }
        let mut close = false;
        egui::Window::new(format!("{}  Check for updates", egui_phosphor::regular::ARROWS_CLOCKWISE))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .show(ctx, |ui| {
                if st.checking {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Checking for updates…");
                    });
                    return;
                }
                if let Some(e) = &st.check_failed {
                    ui.colored_label(
                        crate::theme::standing::WARNING,
                        format!("Couldn't check for updates: {e}"),
                    );
                } else {
                    ui.label(format!(
                        "You're on the latest version (v{}).",
                        crate::update::current()
                    ));
                }
                ui.add_space(6.0);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        if close {
            let mut s = self.update.lock().unwrap();
            s.up_to_date = false;
            s.check_failed = None;
        }
    }

    /// The database couldn't be opened, usually a permissions problem, so intel/settings won't
    /// persist. Surface it once so a broken install isn't silently running degraded.
    fn store_warning_dialog(&mut self, ctx: &egui::Context) {
        let Some(err) = self.store_error.clone() else { return };
        if self.store_warn_dismissed {
            return;
        }
        egui::Window::new(format!("{}  Storage problem", egui_phosphor::regular::WARNING))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label("EVE Spai can't read or write its database, so settings and intel history won't be saved this session.");
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&err).weak().small());
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "This is usually a file-permission issue on the data folder. Check that your \
                         user can write to it, or reinstall to a writable location.",
                    )
                    .small(),
                );
                ui.add_space(8.0);
                if ui.button("Continue anyway").clicked() {
                    self.store_warn_dismissed = true;
                }
            });
    }

    fn poll_dscan_clipboard(&mut self) {
        if !self.settings.dscan_autoprompt {
            return;
        }
        let due = self.dscan_checked.map(|t| t.elapsed().as_millis() > 1200).unwrap_or(true);
        if !due {
            return;
        }
        self.dscan_checked = Some(std::time::Instant::now());
        if self.dscan_prompt.is_some() || self.dscan_share.lock().unwrap().uploading {
            return;
        }
        if self.dscan_clip.is_none() {
            self.dscan_clip = arboard::Clipboard::new().ok();
        }
        let Some(clip) = self.dscan_clip.as_mut() else { return };
        let Ok(text) = clip.get_text() else { return };
        let h = hash_str(&text);
        if h == self.dscan_seen_hash || h == self.dscan_dismissed_hash {
            return;
        }
        self.dscan_seen_hash = h;
        if let Some(n) = crate::dscan::looks_like_dscan(&text) {
            self.dscan_prompt = Some((text, n, PasteKind::Dscan));
        } else if let Some(n) = crate::dscan::looks_like_local(&text) {
            self.dscan_prompt = Some((text, n, PasteKind::Local));
        }
    }

    fn is_imperium(&self) -> bool {
        self.settings.intel_channels.iter().any(|c| c.trim().to_lowercase().ends_with(".imperium"))
    }

    fn dscan_uses_adashboard(&self) -> bool {
        match self.settings.dscan_service {
            crate::settings::DscanService::Auto => self.is_imperium(),
            crate::settings::DscanService::Adashboard => true,
            crate::settings::DscanService::DscanInfo => false,
        }
    }

    fn open_adashboard_intel(&self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text);
        let _ = open::that("https://adashboard.info/intel");
    }

    fn start_dscan_upload(&self, ctx: &egui::Context, text: String) {
        self.dscan_share.lock().unwrap().uploading = true;
        let (share, ctx2) = (self.dscan_share.clone(), ctx.clone());
        std::thread::spawn(move || {
            let res = crate::dscan::upload(&text);
            let mut s = share.lock().unwrap();
            s.uploading = false;
            match res {
                Ok(link) => s.link = Some(link),
                Err(e) => s.error = Some(e.to_string()),
            }
            ctx2.request_repaint();
        });
    }

    #[allow(deprecated)]
    fn dscan_dialog(&mut self, ctx: &egui::Context) {
        let adashboard = self.dscan_uses_adashboard();
        let active = self.dscan_prompt.is_some() || {
            let s = self.dscan_share.lock().unwrap();
            s.uploading || s.link.is_some() || s.error.is_some()
        };
        if !active {
            self.dscan_pos = None;
            self.dscan_link_used = false;
            self.dscan_unfocused_at = None;
        }
        let auto_dscan =
            matches!(self.dscan_prompt, Some((_, _, PasteKind::Dscan))) && self.settings.dscan_autoupload;
        if active && auto_dscan {
            if adashboard {
                if let Some((text, _, _)) = self.dscan_prompt.take() {
                    self.open_adashboard_intel(ctx, text);
                }
            } else {
                let idle = {
                    let s = self.dscan_share.lock().unwrap();
                    !s.uploading && s.link.is_none() && s.error.is_none()
                };
                if idle {
                    if let Some((text, _, _)) = self.dscan_prompt.take() {
                        self.start_dscan_upload(ctx, text);
                    }
                }
            }
        }
        if active && self.dscan_pos.is_none() {
            let (ow, oh, margin) = (300.0_f32, 150.0_f32, 14.0_f32);
            self.dscan_pos = Some(match eve_window_rect() {
                Some((x, y, w, h)) => (
                    ((x + w) as f32 - ow - margin).max(0.0),
                    ((y + h) as f32 - oh - margin).max(0.0),
                ),
                None => (1920.0 - ow - margin, 1080.0 - oh - margin),
            });
        }
        let pos = self.dscan_pos.unwrap_or((200.0, 200.0));
        let share = {
            let s = self.dscan_share.lock().unwrap();
            (s.uploading, s.link.clone(), s.error.clone())
        };
        use egui_phosphor::regular as icon;
        let mut start_upload = false;
        let mut open_adashboard = false;
        let mut do_lookup = false;
        let mut dismiss = false;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("dscan_popup"),
            egui::ViewportBuilder::default().with_icon(app_icon())
                .with_title("EVE Spai - D-scan")
                .with_visible(active)
                .with_window_level(egui::WindowLevel::AlwaysOnTop)
                .with_active(false)
                .with_decorations(true)
                .with_taskbar(false)
                .with_resizable(true)
                .with_position([pos.0, pos.1])
                .with_inner_size([300.0, 118.0]),
            |ctx, _| {
                if !active {
                    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |_ui| {});
                    return;
                }
                ontop_pin(ctx, "dscan_popup");
                let frame = egui::Frame::central_panel(&ctx.style());
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    let title = match &self.dscan_prompt {
                        Some((_, _, PasteKind::Local)) => "Local list",
                        _ => "D-scan",
                    };
                    ui.label(egui::RichText::new(format!("{}  {title}", icon::BROADCAST)).strong());
                    let (uploading, link, error) = (share.0, share.1.clone(), share.2.clone());
                    if let Some(link) = link {
                        ui.label("Shared:");
                        if ui.hyperlink(&link).clicked() {
                            self.dscan_link_used = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button(format!("{}  Copy link", icon::COPY)).clicked() {
                                ui.ctx().copy_text(link.clone());
                                self.dscan_link_used = true;
                            }
                            if ui.button("Close").clicked() {
                                dismiss = true;
                            }
                        });
                    } else if uploading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Uploading to dscan.info…");
                        });
                    } else {
                        if let Some(e) = &error {
                            ui.colored_label(
                                crate::theme::standing::WARNING,
                                format!("Upload failed: {e}"),
                            );
                        }
                        if let Some((_, n, kind)) = &self.dscan_prompt {
                            let (n, kind) = (*n, *kind);
                            let ada = format!("{}  adashboard.info", icon::UPLOAD_SIMPLE);
                            let ada_hint =
                                "Copy and open adashboard.info/intel, then paste it there (Ctrl+V)";
                            match kind {
                                PasteKind::Dscan => {
                                    ui.label(format!("D-scan detected ({n} rows). Share with:"));
                                    ui.horizontal(|ui| {
                                        if ui
                                            .button(format!("{}  dscan.info", icon::UPLOAD_SIMPLE))
                                            .on_hover_text("Upload to dscan.info and get a shareable link")
                                            .clicked()
                                        {
                                            start_upload = true;
                                        }
                                        if ui.button(&ada).on_hover_text(ada_hint).clicked() {
                                            open_adashboard = true;
                                        }
                                        if ui.button("Dismiss").clicked() {
                                            dismiss = true;
                                        }
                                    });
                                    let auto_label = if adashboard {
                                        "Auto-open (also in Settings)"
                                    } else {
                                        "Auto-upload (also in Settings)"
                                    };
                                    if ui.checkbox(&mut self.settings.dscan_autoupload, auto_label).changed()
                                    {
                                        self.needs_save = true;
                                    }
                                }
                                PasteKind::Local => {
                                    ui.label(format!("Local list detected ({n} pilots). Use:"));
                                    ui.horizontal(|ui| {
                                        if ui
                                            .button(format!("{}  Look up", icon::MAGNIFYING_GLASS))
                                            .on_hover_text("Open these pilots in the Lookup view")
                                            .clicked()
                                        {
                                            do_lookup = true;
                                        }
                                        if ui.button(&ada).on_hover_text(ada_hint).clicked() {
                                            open_adashboard = true;
                                        }
                                        if ui.button("Dismiss").clicked() {
                                            dismiss = true;
                                        }
                                    });
                                }
                            }
                        }
                    }
                });
                if ctx.input(|i| i.viewport().close_requested()) {
                    dismiss = true;
                }
                if self.dscan_link_used {
                    if ctx.input(|i| i.viewport().focused).unwrap_or(false) {
                        self.dscan_unfocused_at = None;
                    } else if self
                        .dscan_unfocused_at
                        .get_or_insert_with(std::time::Instant::now)
                        .elapsed()
                        .as_secs_f32()
                        >= 5.0
                    {
                        dismiss = true;
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(500));
                }
            },
        );

        if start_upload {
            if let Some((text, _, _)) = self.dscan_prompt.take() {
                self.start_dscan_upload(ctx, text);
            }
        }
        if open_adashboard {
            if let Some((text, _, _)) = self.dscan_prompt.take() {
                self.open_adashboard_intel(ctx, text);
            }
            dismiss = true;
        }
        if do_lookup {
            if let Some((text, _, _)) = self.dscan_prompt.take() {
                self.add_lookup_names(&text);
                self.view = View::Lookup;
            }
            dismiss = true;
        }
        if dismiss {
            if let Some((text, _, _)) = &self.dscan_prompt {
                self.dscan_dismissed_hash = hash_str(text);
            }
            self.dscan_prompt = None;
            *self.dscan_share.lock().unwrap() = DscanShare::default();
        }
    }

    fn setup_wizard(&mut self, ctx: &egui::Context) {
        if !self.wizard_open {
            return;
        }
        use egui_phosphor::regular as icon;

        #[derive(Clone, Copy, PartialEq)]
        enum S {
            Shortcut,
            Welcome,
            Logs,
            Channels,
            JumpBridges,
            SovUpgrades,
            Jabber,
            Character,
            Theme,
        }
        let mut steps = Vec::new();
        // Offer a launcher entry first, but only when the installer didn't already make one.
        if matches!(crate::tray::menu_entry_exists(), Some(false)) {
            steps.push(S::Shortcut);
        }
        steps.extend([S::Welcome, S::Logs, S::Channels]);
        if self.settings.configuration_pack == "The Imperium" {
            steps.extend([S::JumpBridges, S::SovUpgrades, S::Jabber]);
        }
        steps.extend([S::Character, S::Theme]);
        let last = steps.len() - 1;
        let mut idx = (self.wizard_step as usize).min(last);
        let cur = steps[idx];
        let total = steps.len();

        let mut close = false;
        let mut finish = false;
        egui::Window::new(format!("{}  Setup", icon::MAGIC_WAND))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(format!("Step {} of {total}", idx + 1)).weak().small());
                ui.separator();
                ui.add_space(4.0);
                match cur {
                    S::Shortcut => {
                        let kind = crate::tray::menu_entry_label();
                        ui.heading(format!("{}  Add a shortcut", icon::ROCKET_LAUNCH));
                        ui.label(format!(
                            "EVE Spai has no {kind} yet, so it only launches from where the \
                             binary lives. Add one to start it like any other app.",
                        ));
                        ui.add_space(6.0);
                        match &self.wizard_shortcut {
                            Some(Ok(())) => {
                                ui.label(
                                    egui::RichText::new(format!("{}  Shortcut created", icon::CHECK_CIRCLE))
                                        .color(crate::theme::standing::FRIENDLY),
                                );
                            }
                            _ => {
                                if ui
                                    .button(format!("{}  Create {kind}", icon::PLUS))
                                    .clicked()
                                {
                                    self.wizard_shortcut =
                                        Some(crate::tray::create_menu_entry().map_err(|e| e.to_string()));
                                }
                                if let Some(Err(e)) = &self.wizard_shortcut {
                                    ui.label(
                                        egui::RichText::new(format!("Couldn't create it: {e}"))
                                            .color(crate::theme::standing::WARNING)
                                            .small(),
                                    );
                                }
                            }
                        }
                    }
                    S::Welcome => {
                        ui.heading("Welcome to EVE Spai");
                        ui.label(
                            "A quick setup to get intel flowing. Everything here can be changed \
                             later in Settings, and you can re-run this wizard from there.",
                        );
                    }
                    S::Logs => {
                        ui.heading(format!("{}  EVE chat logs", icon::FOLDER_OPEN));
                        ui.label(
                            "EVE Spai reads your in-game intel-channel logs. Leave blank to \
                             auto-detect the standard location.",
                        );
                        ui.add_space(4.0);
                        let hint = crate::logpaths::chat_logs_dir("")
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "auto-detect".into());
                        let resolved = crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.settings.eve_logs_dir)
                                    .hint_text(hint)
                                    .desired_width(380.0),
                            );
                            if resolved.is_some() {
                                ui.label(
                                    egui::RichText::new(icon::CHECK_CIRCLE)
                                        .color(crate::theme::standing::FRIENDLY),
                                )
                                .on_hover_text("Valid EVE chat-log folder");
                            } else {
                                ui.label(
                                    egui::RichText::new(icon::X_CIRCLE)
                                        .color(crate::theme::standing::HOSTILE),
                                )
                                .on_hover_text("No EVE chat-log folder found here");
                            }
                        });
                        match &resolved {
                            Some(p) => {
                                ui.label(
                                    egui::RichText::new(format!("Using {}", p.display()))
                                        .weak()
                                        .small(),
                                );
                            }
                            None if self.settings.eve_logs_dir.trim().is_empty() => {
                                ui.label(
                                    egui::RichText::new(
                                        "Couldn't auto-detect — enter the path to your EVE \
                                         Chatlogs folder.",
                                    )
                                    .color(crate::theme::standing::WARNING)
                                    .small(),
                                );
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new("That folder has no EVE chat logs.")
                                        .color(crate::theme::standing::WARNING)
                                        .small(),
                                );
                            }
                        }
                    }
                    S::Channels => {
                        ui.heading(format!("{}  Intel channels", icon::BROADCAST));
                        ui.label("Apply your coalition's preset channels, or add them manually.");
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for pack in crate::packs::PACKS {
                                let selected = self.settings.configuration_pack == pack.name;
                                if ui
                                    .add(egui::Button::new(format!("Apply {}", pack.name)).selected(selected))
                                    .clicked()
                                {
                                    for ch in pack.channels {
                                        if !self
                                            .settings
                                            .intel_channels
                                            .iter()
                                            .any(|c| c.eq_ignore_ascii_case(ch))
                                        {
                                            self.settings.intel_channels.push((*ch).to_owned());
                                        }
                                    }
                                    self.settings.configuration_pack = pack.name.to_owned();
                                    self.needs_save = true;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        if ui.button("Configure channels manually…").clicked() {
                            self.intel_channels_open = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} channel(s) configured",
                                self.settings.intel_channels.len()
                            ))
                            .weak(),
                        );
                    }
                    S::JumpBridges => {
                        ui.heading(format!("{}  Jump bridges (optional)", icon::MAP_TRIFOLD));
                        ui.label(
                            "Import your alliance's jump-bridge network so it's drawn on the map \
                             and used for jump-range filters.",
                        );
                        ui.add_space(4.0);
                        if ui.button("Configure jump bridges…").clicked() {
                            self.jump_bridges_open = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} bridge(s) configured",
                                self.settings.jump_bridges.len()
                            ))
                            .weak(),
                        );
                    }
                    S::SovUpgrades => {
                        ui.heading(format!("{}  Sov upgrades (optional)", icon::GEAR_SIX));
                        ui.label(
                            "Paste your alliance's iHub sov-upgrade data for the map overlay \
                             (cyno jammers, Ansiblex enablement, …).",
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    "The data lives in a formatted list linked inside the alliance \
                                     forum post, not on the forum page itself. Open the post, follow \
                                     that link, and copy the list.",
                                )
                                .weak(),
                            );
                        });
                        ui.add_space(4.0);
                        if ui.button("Configure sov upgrades…").clicked() {
                            self.sov_upgrades_open = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} system(s) configured",
                                self.settings.sov_upgrades.len()
                            ))
                            .weak(),
                        );
                    }
                    S::Jabber => {
                        ui.heading(format!("{}  Jabber (optional)", icon::CHAT_TEXT));
                        ui.label("Connect to alliance Jabber (XMPP) for chat and fleet pings.");
                        ui.add_space(4.0);
                        let connected = self.settings.jabber_enabled
                            && crate::jabber::has_password(self.settings.jabber_jid.trim());
                        if connected {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}  Connected as {}",
                                    icon::CHECK_CIRCLE,
                                    self.settings.jabber_jid
                                ))
                                .color(crate::theme::standing::ALLIANCE),
                            );
                        } else {
                            egui::Grid::new("wiz_jabber").num_columns(2).spacing([8.0, 6.0]).show(
                                ui,
                                |ui| {
                                    ui.label("JID");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.jabber_jid)
                                            .hint_text("MyCharacter@goonfleet.com")
                                            .desired_width(260.0),
                                    );
                                    ui.end_row();
                                    ui.label("Server");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.jabber_server)
                                            .hint_text("jabber-server.goonfleet.com")
                                            .desired_width(260.0),
                                    );
                                    ui.end_row();
                                    ui.label("Password");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.jabber_pw_input)
                                            .password(true)
                                            .desired_width(260.0),
                                    );
                                    ui.end_row();
                                },
                            );
                            if ui.button("Connect").clicked() {
                                let jid = self.settings.jabber_jid.trim().to_owned();
                                if !jid.is_empty() && !self.jabber_pw_input.is_empty() {
                                    match crate::jabber::save_password(&jid, &self.jabber_pw_input) {
                                        Ok(()) => {
                                            self.jabber_pw_input.clear();
                                            self.settings.jabber_enabled = true;
                                            self.needs_save = true;
                                        }
                                        Err(e) => {
                                            self.jabber.lock().unwrap().status =
                                                format!("Keychain error: {e}");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    S::Character => {
                        ui.heading(format!("{}  Log in a character", icon::SIGN_IN));
                        ui.label(
                            "Log in with EVE SSO so EVE Spai knows your location for \
                             distance / near-me filters and the map.",
                        );
                        ui.add_space(4.0);
                        if ui.button(format!("{}  Log in with EVE", icon::SIGN_IN)).clicked() {
                            self.start_login(ctx);
                        }
                        if !self.characters.is_empty() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}  {} character(s) linked",
                                    icon::CHECK_CIRCLE,
                                    self.characters.len()
                                ))
                                .color(crate::theme::standing::ALLIANCE),
                            );
                        }
                    }
                    S::Theme => {
                        ui.heading(format!("{}  Theme", icon::PALETTE));
                        ui.label("Pick a colour preset (fine-tune fully in Settings).");
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for preset in Theme::presets() {
                                if ui.button(&preset.name).clicked() {
                                    self.settings.theme = preset.clone();
                                    self.needs_save = true;
                                }
                            }
                        });
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.horizontal(|ui| {
                    let mut step_changed = false;
                    if ui.button("Skip setup").clicked() {
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if idx >= last {
                            if ui.button(format!("{}  Finish", icon::CHECK_CIRCLE)).clicked() {
                                finish = true;
                            }
                        } else if ui.button("Next").clicked() {
                            idx += 1;
                            step_changed = true;
                        }
                        if idx > 0 && ui.button("Back").clicked() {
                            idx -= 1;
                            step_changed = true;
                        }
                    });
                    // Leaving a step closes any config dialog it opened, so it does not
                    // linger over the next step.
                    if step_changed || finish || close {
                        self.intel_channels_open = false;
                        self.jump_bridges_open = false;
                        self.sov_upgrades_open = false;
                    }
                });
            });
        self.wizard_step = idx.min(last) as u8;
        if finish || close {
            self.settings.wizard_done = true;
            self.needs_save = true;
            self.wizard_open = false;
        }
    }

    fn open_filter_picker(&mut self, kind: crate::pickers::PickerKind, rule_idx: usize) {
        use crate::pickers::{
            build_geo_picker, build_ship_tree, seed_selection, FilterPicker, PickerData, PickerKind,
        };
        let Some(store) = &self.store else { return };
        let Some(rule) = self.settings.alerts.rules.get(rule_idx) else { return };
        let mut picker = FilterPicker::new(kind, rule_idx);
        match kind {
            PickerKind::Systems => {
                let (roots, flat) = build_geo_picker(&store.all_systems_geo());
                picker.geo_roots = roots;
                picker.geo_flat = flat;
                picker.geo_regions = rule.regions.iter().cloned().collect();
                picker.geo_consts = rule.constellations.iter().cloned().collect();
                picker.geo_systems = rule.systems.iter().cloned().collect();
            }
            PickerKind::Ships => {
                picker.data = PickerData::Tree(build_ship_tree(&store.all_ships()));
                picker.selected = seed_selection(&rule.ships, &picker.data);
            }
            PickerKind::Channels => {
                let mut opts = self.settings.intel_channels.clone();
                for c in &rule.channels {
                    if !opts.iter().any(|o| o.eq_ignore_ascii_case(c)) {
                        opts.push(c.clone());
                    }
                }
                picker.data = PickerData::List(opts);
                picker.selected = seed_selection(&rule.channels, &picker.data);
            }
            PickerKind::Characters => {
                let mut chars = store.known_pilot_names();
                for c in &self.characters {
                    if !chars.iter().any(|(n, _)| n.eq_ignore_ascii_case(&c.name)) {
                        chars.push((c.name.clone(), c.id));
                    }
                }
                for c in &rule.characters {
                    if !chars.iter().any(|(n, _)| n.eq_ignore_ascii_case(c)) {
                        chars.push((c.clone(), 0));
                    }
                }
                chars.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                picker.data = PickerData::Chars(chars);
                picker.selected = seed_selection(&rule.characters, &picker.data);
            }
        }
        *self.filter_add_result.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.filter_picker = Some(picker);
    }

    fn filter_picker_dialog(&mut self, ctx: &egui::Context) {
        if self.filter_picker.is_none() {
            return;
        }
        let add_res = self.filter_add_result.lock().unwrap_or_else(|e| e.into_inner()).take();
        let mut open = true;
        let mut changed = false;
        let mut add_to_resolve: Option<String> = None;
        {
            let picker = self.filter_picker.as_mut().unwrap();
            if let Some(res) = add_res {
                match res {
                    Ok(name) => {
                        picker.selected.insert(name.clone());
                        picker.add_status = Some(format!("Added {name}"));
                        picker.add_name.clear();
                        changed = true;
                    }
                    Err(e) => picker.add_status = Some(e),
                }
            }
            let title = format!("{}  filter: {}", egui_phosphor::regular::FUNNEL, picker.kind.title());
            let mut actions = crate::pickers::PickerActions::default();
            egui::Window::new(title)
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(340.0)
                .show(ctx, |ui| {
                    actions = crate::pickers::body(ui, picker);
                });
            changed |= actions.changed;
            if actions.add_clicked {
                add_to_resolve = Some(picker.add_name.trim().to_owned());
            }
        }
        if let Some(name) = add_to_resolve {
            if !name.is_empty() {
                self.spawn_char_resolve(name, ctx);
            }
        }
        if changed {
            let sorted = |set: &std::collections::HashSet<String>| {
                let mut v: Vec<String> = set.iter().cloned().collect();
                v.sort_by_key(|s| s.to_lowercase());
                v
            };
            let (kind, idx, sel, geo) = {
                let p = self.filter_picker.as_ref().unwrap();
                (
                    p.kind,
                    p.rule_idx,
                    sorted(&p.selected),
                    (sorted(&p.geo_regions), sorted(&p.geo_consts), sorted(&p.geo_systems)),
                )
            };
            if let Some(rule) = self.settings.alerts.rules.get_mut(idx) {
                use crate::pickers::PickerKind::*;
                match kind {
                    Ships => rule.ships = sel,
                    Channels => rule.channels = sel,
                    Characters => rule.characters = sel,
                    Systems => {
                        rule.regions = geo.0;
                        rule.constellations = geo.1;
                        rule.systems = geo.2;
                    }
                }
                self.needs_save = true;
            }
        }
        if !open {
            self.filter_picker = None;
        }
    }

    fn spawn_char_resolve(&self, name: String, ctx: &egui::Context) {
        let out = self.filter_add_result.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = reqwest::blocking::Client::builder()
                .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|e| e.to_string())
                .and_then(|c| resolve_char_name(&c, &name));
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(res);
            ctx.request_repaint();
        });
    }

    fn alert_rule_config(
        ui: &mut egui::Ui,
        ru: &mut crate::settings::AlertRule,
        i: usize,
        global_volume: f32,
    ) -> (bool, Option<crate::pickers::PickerKind>) {
        use crate::settings::Severity::*;
        let mut changed = false;
        let mut open_picker: Option<crate::pickers::PickerKind> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("if severity ≥");
            egui::ComboBox::from_id_salt(("rsev", i))
                .selected_text(format!("{:?}", ru.min_severity))
                .show_ui(ui, |ui| {
                    for lvl in [Info, Warning, Danger, Critical] {
                        changed |= ui
                            .selectable_value(&mut ru.min_severity, lvl, format!("{lvl:?}"))
                            .changed();
                    }
                });
            ui.label("within");
            let mut mj = ru.max_jumps.unwrap_or(0);
            if ui
                .add(egui::DragValue::new(&mut mj).range(0..=50).custom_formatter(|n, _| {
                    if n == 0.0 { "any".into() } else { format!("{n}j") }
                }))
                .changed()
            {
                ru.max_jumps = if mj == 0 { None } else { Some(mj) };
                changed = true;
            }
            if ru.max_jumps.is_some() {
                changed |= ui
                    .checkbox(&mut ru.count_bridges, "bridges")
                    .on_hover_text(
                        "Count jump bridges in the range. Off = gate-only \
                         (how far a hostile, who can't use your bridges, really is).",
                    )
                    .changed();
            }
            ui.label("count ≥");
            let mut mc = ru.min_count.unwrap_or(0);
            if ui.add(egui::DragValue::new(&mut mc).range(0..=999)).changed() {
                ru.min_count = if mc == 0 { None } else { Some(mc) };
                changed = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("requires:");
            for tag in [
                "bubble", "camp", "cyno", "dropper", "captackled", "kill", "ess", "spike",
                "wormhole", "help",
            ] {
                let label = if tag == "captackled" { "cap tackled" } else { tag };
                let mut on = ru.require.iter().any(|t| t == tag);
                if selectable_chip(ui, on, label).clicked() {
                    on = !on;
                    ru.require.retain(|t| t != tag);
                    if on {
                        ru.require.push(tag.to_owned());
                    }
                    changed = true;
                }
            }
        });
        {
            use crate::pickers::PickerKind;
            let row = |ui: &mut egui::Ui, label: &str, list: &[String], any_hint: &str| -> bool {
                let mut clicked = false;
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.small_button("Edit").clicked() {
                        clicked = true;
                    }
                    let s = if list.is_empty() {
                        any_hint.to_owned()
                    } else if list.len() <= 3 {
                        list.join(", ")
                    } else {
                        format!("{} selected", list.len())
                    };
                    ui.label(egui::RichText::new(s).weak());
                });
                clicked
            };
            let mut want: Option<PickerKind> = None;
            ui.horizontal(|ui| {
                ui.label("location:");
                if ui.small_button("Edit").clicked() {
                    want = Some(PickerKind::Systems);
                }
                let total = ru.regions.len() + ru.constellations.len() + ru.systems.len();
                let s = if total == 0 { "any".to_owned() } else { format!("{total} selected") };
                ui.label(egui::RichText::new(s).weak());
            });
            if row(ui, "channels:", &ru.channels, "any") {
                want = Some(PickerKind::Channels);
            }
            if row(ui, "ships:", &ru.ships, "any") {
                want = Some(PickerKind::Ships);
            }
            if row(ui, "characters:", &ru.characters, "any enabled") {
                want = Some(PickerKind::Characters);
            }
            if let Some(kind) = want {
                open_picker = Some(kind);
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("then:");
            changed |= ui.checkbox(&mut ru.suppress, "suppress").changed();
            if !ru.suppress {
                changed |= ui.checkbox(&mut ru.system_notification, "notify").changed();
                changed |= ui.checkbox(&mut ru.custom_window, "window").changed();
                changed |= ui.checkbox(&mut ru.push, "push").changed();
                ui.label("sound");
                let eff_vol = ru.volume.unwrap_or(global_volume);
                changed |= sound_picker(ui, ("alert_rule", i), true, &mut ru.sound, eff_vol);
                ui.label("severity");
                egui::ComboBox::from_id_salt(("rsevover", i))
                    .selected_text(match ru.severity_override {
                        None => "keep".to_owned(),
                        Some(s) => format!("{s:?}"),
                    })
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(&mut ru.severity_override, None, "keep")
                            .changed();
                        for lvl in [Info, Warning, Danger, Critical] {
                            changed |= ui
                                .selectable_value(
                                    &mut ru.severity_override,
                                    Some(lvl),
                                    format!("{lvl:?}"),
                                )
                                .changed();
                        }
                    })
                    .response
                    .on_hover_text(
                        "Override the alert's severity (sound + colour). Leave 'keep' \
                         to use the event's own severity. Set Info to show it silently.",
                    );
                let mut custom = ru.volume.is_some();
                if ui
                    .checkbox(&mut custom, "custom volume")
                    .on_hover_text("Override the global intel-alert volume for this rule")
                    .changed()
                {
                    ru.volume = if custom { Some(global_volume) } else { None };
                    changed = true;
                }
                if let Some(v) = ru.volume.as_mut() {
                    changed |= volume_slider(ui, v);
                }
            }
            ui.label("cooldown");
            changed |= ui
                .add(egui::DragValue::new(&mut ru.cooldown_secs).range(0..=3600).suffix("s"))
                .changed();
        });
        (changed, open_picker)
    }

    fn alert_rules_editor(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as ic;
        let mut changed = false;
        let mut remove: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;
        let mut dnd: Option<(usize, usize)> = None;
        let mut open_picker: Option<(crate::pickers::PickerKind, usize)> = None;

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Back", ic::ARROW_LEFT))
                .on_hover_text("Back to alerts")
                .clicked()
            {
                self.alert_rules_open = false;
            }
            ui.separator();
            if ui.button(format!("{}  Add rule", ic::PLUS)).clicked() {
                self.settings
                    .alerts
                    .rules
                    .push(crate::settings::AlertRule { expanded: true, ..Default::default() });
                crate::settings::ensure_rule_ids(&mut self.settings.alerts.rules);
                self.alert_selected_rule = self.settings.alerts.rules.last().map(|r| r.id);
                changed = true;
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Top rule wins. A matching rule's actions apply (or it suppresses the alert). \
                 Empty condition fields mean \"any\". Jumps are measured from the rule's \
                 characters (or any enabled character). Drag the handle or use the arrows to reorder.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        ui.separator();

        // Keep the selection valid (first rule by default, cleared rules fall back).
        let ids: Vec<u64> = self.settings.alerts.rules.iter().map(|r| r.id).collect();
        if self.alert_selected_rule.map_or(true, |id| !ids.contains(&id)) {
            self.alert_selected_rule = ids.first().copied();
        }
        let n_rules = self.settings.alerts.rules.len();

        egui::Panel::left("alert_rules_split")
            .resizable(true)
            .default_size(240.0)
            .size_range(180.0..=400.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("alert_rule_list")
                    .show(ui, |ui| {
                        for i in 0..n_rules {
                            let (id, enabled, name) = {
                                let r = &self.settings.alerts.rules[i];
                                (r.id, r.enabled, r.name.clone())
                            };
                            let selected = self.alert_selected_rule == Some(id);
                            let (_, payload) = ui.dnd_drop_zone::<usize, _>(
                                egui::Frame::default().inner_margin(2.0),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        let mut en = enabled;
                                        if ui.checkbox(&mut en, "").changed() {
                                            self.settings.alerts.rules[i].enabled = en;
                                            changed = true;
                                        }
                                        ui.dnd_drag_source(
                                            egui::Id::new(("alert_rule_dnd", id)),
                                            i,
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(ic::DOTS_SIX_VERTICAL).weak(),
                                                )
                                                .on_hover_text("Drag to reorder");
                                            },
                                        );
                                        let label =
                                            if name.is_empty() { "(unnamed rule)" } else { &name };
                                        // Truncate to the space left of the reorder buttons so a
                                        // long name doesn't stretch the card past the panel.
                                        let name_w = (ui.available_width() - 54.0).max(40.0);
                                        let shown = truncate_to(label, fit_chars(name_w));
                                        let txt = if enabled {
                                            egui::RichText::new(&shown)
                                        } else {
                                            egui::RichText::new(&shown).weak().strikethrough()
                                        };
                                        if ui
                                            .add(egui::Button::selectable(selected, txt))
                                            .on_hover_text(label)
                                            .clicked()
                                        {
                                            self.alert_selected_rule = Some(id);
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if i + 1 < n_rules
                                                    && ui
                                                        .small_button(ic::ARROW_DOWN)
                                                        .on_hover_text("Move down")
                                                        .clicked()
                                                {
                                                    move_down = Some(i);
                                                }
                                                if i > 0
                                                    && ui
                                                        .small_button(ic::ARROW_UP)
                                                        .on_hover_text("Move up")
                                                        .clicked()
                                                {
                                                    move_up = Some(i);
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                            if let Some(from) = payload {
                                dnd = Some((*from, i));
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let Some(sel_id) = self.alert_selected_rule else {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Select a rule to configure it.").weak());
                return;
            };
            let Some(idx) = self.settings.alerts.rules.iter().position(|r| r.id == sel_id) else {
                return;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("alert_rule_config")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut en = self.settings.alerts.rules[idx].enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            self.settings.alerts.rules[idx].enabled = en;
                            changed = true;
                        }
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut self.settings.alerts.rules[idx].name)
                                    .desired_width(240.0),
                            )
                            .changed();
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .button(format!("{}  Delete", ic::TRASH))
                                    .on_hover_text("Delete rule")
                                    .clicked()
                                {
                                    remove = Some(idx);
                                }
                            },
                        );
                    });
                    ui.add_space(6.0);
                    let global_volume = self.settings.alerts.alert_volume;
                    let (c, want) = Self::alert_rule_config(
                        ui,
                        &mut self.settings.alerts.rules[idx],
                        idx,
                        global_volume,
                    );
                    changed |= c;
                    if let Some(kind) = want {
                        open_picker = Some((kind, idx));
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Recent matches").strong());
                    ui.add_space(4.0);
                    self.rule_feed_ui(ui, sel_id);
                });
        });

        if let Some(i) = remove {
            let removed_id = self.settings.alerts.rules.get(i).map(|r| r.id);
            self.settings.alerts.rules.remove(i);
            if self.alert_selected_rule == removed_id {
                self.alert_selected_rule = self.settings.alerts.rules.first().map(|r| r.id);
            }
            changed = true;
        }
        if let Some(i) = move_up {
            self.settings.alerts.rules.swap(i, i - 1);
            changed = true;
        }
        if let Some(i) = move_down {
            self.settings.alerts.rules.swap(i, i + 1);
            changed = true;
        }
        if let Some((from, to)) = dnd {
            let len = self.settings.alerts.rules.len();
            if from != to && from < len && to < len {
                let item = self.settings.alerts.rules.remove(from);
                let dst = if from < to { to - 1 } else { to };
                self.settings.alerts.rules.insert(dst, item);
                changed = true;
            }
        }
        if changed {
            self.needs_save = true;
        }
        if let Some((kind, idx)) = open_picker {
            self.open_filter_picker(kind, idx);
        }
    }

    fn severity_window(&mut self, ctx: &egui::Context) {
        if !self.severity_open {
            return;
        }
        let mut changed = false;
        let mut threat_text = self.settings.severity.threat_ships.join("\n");
        let keep = Self::dialog_viewport(
            ctx,
            "severity_window",
            "EVE Spai - Intel severity",
            [620.0, 480.0],
            |ui| {
                ui.label(
                    egui::RichText::new("Pick the severity (card colour) for each condition.")
                        .weak(),
                );
                ui.add_space(4.0);
                let sv = &mut self.settings.severity;
                let combo = |ui: &mut egui::Ui, label: &str, val: &mut crate::settings::Severity| -> bool {
                    use crate::settings::Severity::*;
                    let mut ch = false;
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::ComboBox::from_id_salt(label)
                                .selected_text(format!("{val:?}"))
                                .show_ui(ui, |ui| {
                                    for lvl in [Info, Warning, Danger, Critical] {
                                        if ui.selectable_value(val, lvl, format!("{lvl:?}")).changed() {
                                            ch = true;
                                        }
                                    }
                                });
                        });
                    });
                    ch
                };
                ui.horizontal(|ui| {
                    ui.label("Big-gang threshold (≥)");
                    changed |=
                        ui.add(egui::DragValue::new(&mut sv.big_gang_threshold).range(2..=100)).changed();
                });
                ui.columns(2, |c| {
                    changed |= combo(&mut c[0], "Small gang (< threshold)", &mut sv.small_gang);
                    changed |= combo(&mut c[0], "Big gang (≥ threshold)", &mut sv.big_gang);
                    changed |= combo(&mut c[0], "Bubble", &mut sv.bubble);
                    changed |= combo(&mut c[0], "Gate camp", &mut sv.gate_camp);
                    changed |= combo(&mut c[0], "Spike (local)", &mut sv.spike);
                    changed |= combo(&mut c[0], "Cyno", &mut sv.cyno);
                    changed |= combo(&mut c[1], "Capital tackled", &mut sv.cap_tackled);
                    changed |= combo(&mut c[1], "Kill", &mut sv.kill);
                    changed |= combo(&mut c[1], "No visual", &mut sv.no_visual);
                    changed |= combo(&mut c[1], "Wormhole", &mut sv.wormhole);
                    changed |= combo(&mut c[1], "ESS", &mut sv.ess);
                    changed |= combo(&mut c[1], "High-threat ships", &mut sv.threat_ship);
                });
                ui.separator();
                ui.label(egui::RichText::new("High-threat hulls (one per line)").weak());
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut threat_text)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    sv.threat_ships =
                        threat_text.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect();
                    changed = true;
                }
                if ui.button("Reset to defaults").clicked() {
                    *sv = crate::settings::SeverityRules::default();
                    changed = true;
                }
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.severity_open = false;
        }
    }

    fn coalitions_window(&mut self, ctx: &egui::Context) {
        if !self.coalitions_open {
            return;
        }
        let mut remove: Option<usize> = None;
        let mut add = false;
        let mut reset = false;
        let mut coal_color: Vec<(String, Option<(u8, u8, u8)>)> = Vec::new();
        let mut ally_color: Vec<(usize, Option<(u8, u8, u8)>)> = Vec::new();
        let mut ally_remove: Option<usize> = None;
        let mut ally_assign: Option<(String, Option<String>)> = None;
        let mut ally_add = false;
        let keep = Self::dialog_viewport(
            ctx,
            "coalitions_window",
            "EVE Spai - Coalitions",
            [520.0, 680.0],
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Group alliances into coalitions for the map's sovereignty overlay. \
                         Alliance names must match the sov holder exactly (some end with a \
                         period). Unlisted alliances are shown as independent.",
                    )
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Add coalition").clicked() {
                        add = true;
                    }
                    if ui.button("Reset to defaults").clicked() {
                        reset = true;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("coal_scroll").max_height(280.0).show(ui, |ui| {
                    for (i, (name, alliances)) in self.coal_edit.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Coalition").weak());
                                ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                                let cur = self
                                    .settings
                                    .coalitions
                                    .iter()
                                    .find(|c| c.name == name.trim())
                                    .and_then(|c| c.color);
                                let mut rgb = cur.map(|(r, g, b)| [r, g, b]).unwrap_or_else(|| {
                                    let c = name_color(name);
                                    [c.r(), c.g(), c.b()]
                                });
                                if ui.color_edit_button_srgb(&mut rgb).changed() {
                                    coal_color.push((name.trim().to_owned(), Some((rgb[0], rgb[1], rgb[2]))));
                                }
                                if ui.button("Remove").clicked() {
                                    remove = Some(i);
                                }
                            });
                            ui.add(
                                egui::TextEdit::multiline(alliances)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("One alliance name per line\nGoonswarm Federation"),
                            );
                        });
                    }
                });

                ui.separator();
                ui.label(egui::RichText::new("Alliances (sov holders)").strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.alliance_add)
                            .desired_width(220.0)
                            .hint_text("Add alliance by name"),
                    );
                    if ui.button("Add").clicked() {
                        ally_add = true;
                    }
                });
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("ally_scroll").show(ui, |ui| {
                    for (i, a) in self.settings.alliances.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let mut rgb = a.color.map(|(r, g, b)| [r, g, b]).unwrap_or_else(|| {
                                let c = name_color(&a.name);
                                [c.r(), c.g(), c.b()]
                            });
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                ally_color.push((i, Some((rgb[0], rgb[1], rgb[2]))));
                            }
                            ui.label(&a.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(egui_phosphor::regular::X).clicked() {
                                    ally_remove = Some(i);
                                }
                                let current = self
                                    .settings
                                    .coalitions
                                    .iter()
                                    .find(|c| c.alliances.iter().any(|x| x.eq_ignore_ascii_case(&a.name)))
                                    .map(|c| c.name.clone());
                                egui::ComboBox::from_id_salt(("coal_of", i))
                                    .selected_text(current.clone().unwrap_or_else(|| "—".to_owned()))
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(current.is_none(), "— independent").clicked() {
                                            ally_assign = Some((a.name.clone(), None));
                                        }
                                        for c in &self.settings.coalitions {
                                            if ui
                                                .selectable_label(
                                                    current.as_deref() == Some(c.name.as_str()),
                                                    &c.name,
                                                )
                                                .clicked()
                                            {
                                                ally_assign = Some((a.name.clone(), Some(c.name.clone())));
                                            }
                                        }
                                    });
                            });
                        });
                    }
                });
            },
        );
        if add {
            self.coal_edit.push(("New coalition".to_owned(), String::new()));
        }
        if reset {
            self.coal_edit = crate::settings::default_coalitions()
                .into_iter()
                .map(|c| (c.name, c.alliances.join("\n")))
                .collect();
        }
        if let Some(i) = remove {
            self.coal_edit.remove(i);
        }
        for (name, col) in coal_color {
            if let Some(c) = self.settings.coalitions.iter_mut().find(|c| c.name == name) {
                c.color = col;
                self.needs_save = true;
            }
        }
        for (i, col) in ally_color {
            if let Some(a) = self.settings.alliances.get_mut(i) {
                a.color = col;
                self.needs_save = true;
            }
        }
        if let Some(i) = ally_remove {
            if i < self.settings.alliances.len() {
                self.settings.alliances.remove(i);
                self.needs_save = true;
            }
        }
        if let Some((ally, target)) = ally_assign {
            for c in &mut self.settings.coalitions {
                c.alliances.retain(|x| !x.eq_ignore_ascii_case(&ally));
            }
            if let Some(t) = target {
                if let Some(c) = self.settings.coalitions.iter_mut().find(|c| c.name == t) {
                    c.alliances.push(ally);
                }
            }
            self.coal_edit = self
                .settings
                .coalitions
                .iter()
                .map(|c| (c.name.clone(), c.alliances.join("\n")))
                .collect();
            self.needs_save = true;
        }
        if ally_add {
            let name = self.alliance_add.trim().to_owned();
            if !name.is_empty()
                && !self.settings.alliances.iter().any(|a| a.name.eq_ignore_ascii_case(&name))
            {
                self.settings.alliances.push(crate::settings::AllianceConfig { name, color: None });
                self.settings.alliances.sort_by(|a, b| a.name.cmp(&b.name));
                self.needs_save = true;
            }
            self.alliance_add.clear();
        }
        let parsed: Vec<crate::settings::Coalition> = self
            .coal_edit
            .iter()
            .filter(|(n, _)| !n.trim().is_empty())
            .map(|(n, a)| crate::settings::Coalition {
                name: n.trim().to_owned(),
                alliances: a.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect(),
                color: self
                    .settings
                    .coalitions
                    .iter()
                    .find(|c| c.name == n.trim())
                    .and_then(|c| c.color),
            })
            .collect();
        if parsed != self.settings.coalitions {
            self.settings.coalitions = parsed;
            self.needs_save = true;
        }
        if !keep {
            self.coalitions_open = false;
        }
    }

    fn jump_bridges_window(&mut self, ctx: &egui::Context) {
        if !self.jump_bridges_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "jump_bridges_window",
            "EVE Spai - Jump bridges",
            [440.0, 520.0],
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Paste a jump-bridge list (one bridge per line).").weak(),
                    );
                    ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(
                        "Imperium members: open the alliance jump-bridge map, copy the bridge \
                         list, and paste it here. Each line's first two systems form a bridge \
                         (any separator works).",
                    );
                    ui.hyperlink_to("Imperium stargates", "https://wiki.goonswarm.org/w/Alliance:Stargate");
                });
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .auto_shrink([false, false])
                    .id_salt("jb_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.jb_paste)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g.  1DQ1-A » O-EIMK   (one bridge per line, or paste the whole wiki page)"),
                        );
                    });
                ui.horizontal(|ui| {
                    if ui.button("Add from paste").clicked() {
                        if let Some(g) = self.systems.clone() {
                            for b in parse_bridges(&self.jb_paste, &g) {
                                if !self.settings.jump_bridges.contains(&b) {
                                    self.settings.jump_bridges.push(b);
                                    changed = true;
                                }
                            }
                        }
                        self.jb_paste.clear();
                    }
                    if !self.settings.jump_bridges.is_empty() && ui.button("Delete all").clicked() {
                        self.settings.jump_bridges.clear();
                        changed = true;
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(format!("{} bridges", self.settings.jump_bridges.len())).strong());
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove = None;
                    for (i, b) in self.settings.jump_bridges.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} » {}", b.from, b.to));
                            if ui.button(egui_phosphor::regular::X).clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.jump_bridges.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.jump_bridges_open = false;
        }
    }

    fn sov_upgrades_window(&mut self, ctx: &egui::Context) {
        if !self.sov_upgrades_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "sov_upgrades_window",
            "EVE Spai - Sov upgrades",
            [460.0, 520.0],
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Paste sov-upgrade data (one per line).").weak());
                    ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(
                        "Imperium members: open the forum topic, then follow the link inside it to \
                         the formatted upgrade list and copy THAT. The forum page itself is not the \
                         paste. The first system matched on each line is used; the rest of the line \
                         becomes the upgrade label.",
                    );
                    ui.hyperlink_to(
                        "Equinox upgrades",
                        "https://goonfleet.com/index.php/topic/371770-equinox-upgrade-information-station",
                    );
                });
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .auto_shrink([false, false])
                    .id_salt("sov_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.sov_paste)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g.  1DQ1-A Cynosural Suppression   (or paste the in-game I-Hub window)"),
                        );
                    });
                ui.horizontal(|ui| {
                    if ui.button("Add from paste").clicked() {
                        if let Some(g) = self.systems.clone() {
                            for u in parse_sov_upgrades(&self.sov_paste, &g) {
                                if !self.settings.sov_upgrades.contains(&u) {
                                    self.settings.sov_upgrades.push(u);
                                    changed = true;
                                }
                            }
                        }
                        self.sov_paste.clear();
                    }
                    if !self.settings.sov_upgrades.is_empty() && ui.button("Delete all").clicked() {
                        self.settings.sov_upgrades.clear();
                        changed = true;
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(format!("{} upgrades", self.settings.sov_upgrades.len())).strong());
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove = None;
                    for (i, u) in self.settings.sov_upgrades.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&u.system).strong());
                            ui.label(egui::RichText::new(&u.upgrade).weak());
                            if ui.button(egui_phosphor::regular::X).clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.sov_upgrades.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.sov_upgrades_open = false;
        }
    }

    fn intel_channels_window(&mut self, ctx: &egui::Context) {
        if !self.intel_channels_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "intel_channels_window",
            "EVE Spai - Intel channels",
            [420.0, 480.0],
            |ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "EVE chat channels to watch for intel. Match the in-game channel name.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                if ui.button("Add channel").clicked() {
                    self.settings.intel_channels.push(String::new());
                    changed = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove: Option<usize> = None;
                    for (i, ch) in self.settings.intel_channels.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.text_edit_singleline(ch).changed() {
                                changed = true;
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.intel_channels.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.intel_channels_open = false;
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut new_theme: Option<Theme> = None;
        let imp_target = if self.is_imperium() { "adashboard.info" } else { "dscan.info" };
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
                    if ui
                        .button(format!("{}  Run setup wizard", egui_phosphor::regular::MAGIC_WAND))
                        .clicked()
                    {
                        self.wizard_step = 0;
                        self.wizard_open = true;
                    }
                    ui.separator();

                    ui.label(egui::RichText::new("Theme (3 colours)").strong());
                    ui.horizontal_wrapped(|ui| {
                        for preset in Theme::presets() {
                            if ui.button(&preset.name).clicked() {
                                new_theme = Some(preset.clone());
                            }
                        }
                    });
                    ui.add_space(4.0);

                    changed |= color_row(ui, "Background", &mut self.settings.theme.background);
                    changed |= color_row(ui, "Foreground", &mut self.settings.theme.foreground);
                    changed |= color_row(ui, "Accent", &mut self.settings.theme.accent);

                    ui.separator();

                    ui.label(egui::RichText::new("General").strong());
                    changed |= ui
                        .checkbox(&mut self.settings.use_eve_time, "Show EVE time (UTC)")
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.settings.dscan_autoprompt,
                            "Offer to share d-scans from the clipboard",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.settings.dscan_autoupload,
                            "Auto-upload detected d-scans (skip the prompt)",
                        )
                        .changed();
                    ui.horizontal(|ui| {
                        ui.label("D-scan service");
                        use crate::settings::DscanService as Dsc;
                        egui::ComboBox::from_id_salt("dscan_service")
                            .selected_text(match self.settings.dscan_service {
                                Dsc::Auto => format!("Auto ({imp_target})"),
                                Dsc::DscanInfo => "dscan.info".to_owned(),
                                Dsc::Adashboard => "adashboard.info".to_owned(),
                            })
                            .show_ui(ui, |ui| {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.dscan_service,
                                        Dsc::Auto,
                                        format!("Auto ({imp_target})"),
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.dscan_service,
                                        Dsc::DscanInfo,
                                        "dscan.info",
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.dscan_service,
                                        Dsc::Adashboard,
                                        "adashboard.info (Imperium)",
                                    )
                                    .changed();
                            });
                    })
                    .response
                    .on_hover_text(
                        "Auto uses adashboard.info/intel when an *.imperium intel channel is configured, \
                         else dscan.info. adashboard opens in your browser to paste (it needs your login).",
                    );
                    changed |= ui
                        .checkbox(
                            &mut self.settings.minimize_to_tray,
                            "Close to system tray (keep running)",
                        )
                        .changed();
                    if ui
                        .checkbox(&mut self.settings.autostart, "Start automatically on login")
                        .changed()
                    {
                        if let Err(e) = crate::tray::set_autostart(self.settings.autostart) {
                            eprintln!("[autostart] {e}");
                        }
                        changed = true;
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(format!(
                                "{}  Check for updates",
                                egui_phosphor::regular::ARROWS_CLOCKWISE
                            ))
                            .clicked()
                        {
                            self.update_dismissed = false;
                            self.settings.update_skip_version.clear();
                            changed = true;
                            crate::update::spawn_check(
                                self.update.clone(),
                                String::new(),
                                true,
                                ui.ctx().clone(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .weak(),
                        );
                    });

                    ui.add_space(6.0);
                    ui.label("Fit preview site").on_hover_text("Where the fit window's \"Open in\" button sends a loss");
                    ui.horizontal_wrapped(|ui| {
                        for (id, label) in FIT_SITES {
                            if selectable_chip(ui, self.settings.fit_site == *id, *label).clicked() {
                                self.settings.fit_site = (*id).to_owned();
                                changed = true;
                            }
                        }
                        if selectable_chip(ui, self.settings.fit_site.is_empty(), "Ask each time").clicked() {
                            self.settings.fit_site.clear();
                            changed = true;
                        }
                    });

                    ui.add_space(6.0);
                    let logs_hint = crate::logpaths::chat_logs_dir("")
                        .and_then(|p| p.parent().map(|p| p.display().to_string()))
                        .unwrap_or_else(|| "auto-detect".to_owned());
                    ui.label("EVE chat-log directory");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.eve_logs_dir)
                                .hint_text(logs_hint),
                        )
                        .changed();
                    ui.label("EVE settings directory");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.eve_settings_dir)
                                .hint_text("auto-detect"),
                        )
                        .changed();

                    ui.separator();

                    ui.label(egui::RichText::new("Alerts").strong());
                    changed |= ui
                        .checkbox(&mut self.settings.alert_enabled, "Enable intel alerts")
                        .on_hover_text("Master switch. Configure what fires in the Alerts tab.")
                        .changed();
                    changed |= ui
                        .checkbox(&mut self.settings.alert_only_undocked, "Only alert while undocked")
                        .changed();

                    ui.add_space(6.0);
                    {
                        use crate::settings::OnTop;
                        let a = &mut self.settings.alerts;
                        ui.horizontal(|ui| {
                            ui.label("Alert window stays");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut a.window_timeout)
                                        .range(0.0..=300.0)
                                        .custom_formatter(|n, _| {
                                            if n <= 0.0 { "never hides".to_owned() } else { format!("{n}s") }
                                        }),
                                )
                                .on_hover_text("0 = never auto-hide")
                                .changed();
                            ui.label("· on top");
                            egui::ComboBox::from_id_salt("on_top")
                                .selected_text(match a.on_top {
                                    OnTop::Always => "Always",
                                    OnTop::Smart => "Smart (EVE active)",
                                    OnTop::Never => "Never",
                                })
                                .show_ui(ui, |ui| {
                                    changed |= ui.selectable_value(&mut a.on_top, OnTop::Always, "Always").changed();
                                    changed |= ui.selectable_value(&mut a.on_top, OnTop::Smart, "Smart (only when EVE is active)").changed();
                                    changed |= ui.selectable_value(&mut a.on_top, OnTop::Never, "Never").changed();
                                });
                        });
                        changed |= ui
                            .checkbox(&mut a.compact_mode, "Compact alert window")
                            .on_hover_text("Tighter rows and title bar. Hover cards pop out in their own window.")
                            .changed();
                        ui.label(egui::RichText::new("Sounds (preset: off/info/warning/danger/critical/beep/chime, or a file path)").weak());
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(64.0, ui.spacing().interact_size.y),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label("Volume");
                                },
                            );
                            changed |= volume_slider(ui, &mut a.alert_volume);
                        });
                        let alert_vol = a.alert_volume;
                        for (i, lbl) in ["Info", "Warning", "Danger", "Critical"].iter().enumerate() {
                            if a.sounds.len() <= i {
                                a.sounds.resize(i + 1, "off".to_owned());
                            }
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(64.0, ui.spacing().interact_size.y),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.label(*lbl);
                                    },
                                );
                                changed |= sound_picker(ui, ("severity_sound", i), false, &mut a.sounds[i], alert_vol);
                            });
                        }
                        changed |= ui
                            .checkbox(&mut a.push_enabled, "Mobile push (Pushover)")
                            .on_hover_text("Install the Pushover app; create an application for the token")
                            .changed();
                        if a.push_enabled {
                            ui.horizontal(|ui| {
                                ui.label("App token");
                                changed |= ui.add(egui::TextEdit::singleline(&mut a.pushover_token).desired_width(220.0)).changed();
                            });
                            ui.horizontal(|ui| {
                                ui.label("User key ");
                                changed |= ui.add(egui::TextEdit::singleline(&mut a.pushover_user).desired_width(220.0)).changed();
                            });
                        }
                    }
                    ui.label(
                        egui::RichText::new("Alert rules live in the Alerts tab.").weak(),
                    );

                    ui.separator();

                    ui.label(egui::RichText::new("Battle reports").strong());
                    if ui
                        .checkbox(
                            &mut self.settings.battles_enabled,
                            "Enable battle report generation",
                        )
                        .on_hover_text(
                            "Turn off to stop all battle-report clustering and computation. \
                             Gate-camp warnings and the kill feed keep working.",
                        )
                        .changed()
                    {
                        self.battles_enabled_shared.store(
                            self.settings.battles_enabled,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        changed = true;
                    }

                    ui.separator();

                    ui.label(egui::RichText::new("Configuration packs").strong());
                    ui.label(
                        egui::RichText::new("Apply a coalition's preset intel channels.").weak(),
                    );
                    for pack in crate::packs::PACKS {
                        ui.horizontal(|ui| {
                            if ui.button(format!("Apply {}", pack.name)).clicked() {
                                for ch in pack.channels {
                                    if !self
                                        .settings
                                        .intel_channels
                                        .iter()
                                        .any(|c| c.eq_ignore_ascii_case(ch))
                                    {
                                        self.settings.intel_channels.push((*ch).to_owned());
                                    }
                                }
                                self.settings.configuration_pack = pack.name.to_owned();
                                changed = true;
                            }
                            ui.label(
                                egui::RichText::new(format!("{} channels", pack.channels.len()))
                                    .weak(),
                            );
                        });
                    }
                    if !self.settings.configuration_pack.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "Applied: {}",
                                self.settings.configuration_pack
                            ))
                            .weak(),
                        );
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Intel channels").strong());
                        ui.label(
                            egui::RichText::new(format!("{} configured", self.settings.intel_channels.len()))
                                .weak(),
                        );
                    });
                    if ui.button("Configure intel channels…").clicked() {
                        self.intel_channels_open = true;
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Coalition data").strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "{} bridges · {} upgrades",
                                self.settings.jump_bridges.len(),
                                self.settings.sov_upgrades.len()
                            ))
                            .weak(),
                        );
                    });
                    if ui.button("Configure jump bridges…").clicked() {
                        self.jump_bridges_open = true;
                    }
                    if ui.button("Configure sov upgrades…").clicked() {
                        self.sov_upgrades_open = true;
                    }
                    if ui.button("Configure coalitions…").clicked() {
                        self.coal_edit = self
                            .settings
                            .coalitions
                            .iter()
                            .map(|c| (c.name.clone(), c.alliances.join("\n")))
                            .collect();
                        self.coalitions_open = true;
                    }
                    ui.add_space(12.0);
                    ui.separator();
                    ui.heading("About");
                    ui.label(format!("EVE Spai v{}", env!("CARGO_PKG_VERSION")));
                    ui.horizontal(|ui| {
                        ui.label("Project:");
                        ui.hyperlink_to(
                            "github.com/Amryu/eve-spai",
                            "https://github.com/Amryu/eve-spai",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Community:");
                        ui.hyperlink_to("Discord", "https://discord.gg/u4bDqB9rjn");
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(eve_portrait_url(2119400938_i64, 48.0))
                                .fit_to_exact_size(egui::Vec2::splat(48.0)),
                        );
                        ui.vertical(|ui| {
                            ui.label("Built by Amryu.");
                            ui.label(
                                egui::RichText::new(
                                    "If you find it useful, ISK donations to Amryu in-game are welcome.",
                                )
                                .weak(),
                            );
                        });
                    });
                });

        if let Some(theme) = new_theme {
            self.settings.theme = theme;
            changed = true;
        }
        if changed {
            self.needs_save = true;
        }
    }
}

#[derive(Clone, Default)]
struct AlertConfig {
    enabled: bool,
    alerts: crate::settings::AlertSettings,
    severity: crate::settings::SeverityRules,
    only_undocked: bool,
    disabled: Vec<String>,
    systems: Option<std::sync::Arc<crate::geo::Systems>>,
    ship_index: Option<std::sync::Arc<std::collections::HashMap<String, (i64, String)>>>,
    active_character: String,
    kill_intel: bool,
    kill_intel_jumps: u32,
    intel_max_jumps: u32,
}

#[derive(Default)]
struct AlertRuntime {
    last_alert_time: i64,
    cooldown: std::collections::HashMap<i64, i64>,
    alerted: std::collections::HashMap<u64, i64>,
    fired_ui: Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)>,
    matched_ui: Vec<(crate::intel::IntelReport, crate::settings::Severity, u64, bool)>,
}

struct AlertEngine {
    config: std::sync::Mutex<AlertConfig>,
    runtime: std::sync::Mutex<AlertRuntime>,
    recent: crate::gamewatcher::AlertLog,
    alert_shared: SharedAlertWindow,
    ctx: egui::Context,
    overlay_stdin: std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>>,
    alert_sent_hash: std::sync::Mutex<Option<u64>>,
    ping_sent_hash: std::sync::Mutex<Option<u64>>,
}

impl AlertEngine {
    fn new(
        recent: crate::gamewatcher::AlertLog,
        last_alert_time: i64,
        alert_shared: SharedAlertWindow,
        ctx: egui::Context,
        overlay_stdin: std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>>,
    ) -> Self {
        Self {
            config: std::sync::Mutex::new(AlertConfig::default()),
            runtime: std::sync::Mutex::new(AlertRuntime { last_alert_time, ..Default::default() }),
            recent,
            alert_shared,
            ctx,
            overlay_stdin,
            alert_sent_hash: std::sync::Mutex::new(None),
            ping_sent_hash: std::sync::Mutex::new(None),
        }
    }

    /// Push the enriched `AlertMsg` (resolved pilots, jump distances, ...) from the engine thread,
    /// so it keeps updating while the main window is minimized and its UI loop is parked.
    fn push_overlay_update(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        pilots: &crate::pilot::SharedPilots,
        player: &crate::esi::SharedPlayer,
        system_status: &crate::systemstatus::SharedStatus,
        affiliations: &crate::affiliation::SharedAffil,
        kill_cache: &crate::kills::KillCache,
    ) {
        use std::hash::{Hash, Hasher};
        if self.overlay_stdin.lock().unwrap().is_none() {
            return;
        }
        let cfg = self.config.lock().unwrap().clone();
        let feature = cfg.enabled && cfg.alerts.rules.iter().any(|r| r.enabled && r.custom_window);

        let raw: Vec<(crate::intel::IntelReport, crate::settings::Severity)> = {
            let st = self.alert_shared.lock().unwrap();
            let n = st.feed.len();
            st.feed[n.saturating_sub(50)..].to_vec()
        };
        let feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
            if !feature || raw.is_empty() {
                Vec::new()
            } else {
                let live = intel_state.lock().unwrap();
                raw.iter()
                    .filter_map(|(r, sev)| {
                        live.reports.iter().find(|lr| lr.id == r.id).cloned().map(|lr| (lr, *sev))
                    })
                    .collect()
            };

        let resolved_pilots: std::collections::HashMap<String, i64> = if feed.is_empty() {
            Default::default()
        } else {
            let mut cache = pilots.lock().unwrap();
            cache.display_ids(feed.iter().flat_map(|(r, _)| r.pilots.iter()).map(|s| s.as_str()))
        };
        let uncertain = if feed.is_empty() {
            Default::default()
        } else {
            uncertain_set(&pilots.lock().unwrap(), &resolved_pilots)
        };
        let status = if feed.is_empty() {
            Default::default()
        } else {
            system_status.lock().unwrap().clone()
        };
        let last_ship = if feed.is_empty() {
            Default::default()
        } else {
            build_last_ship(&intel_state.lock().unwrap().reports)
        };

        let player_sys = {
            let p = player.lock().unwrap();
            p.locations.get(&cfg.active_character).map(|(s, _)| *s).or(p.system_id)
        };
        let from_you: Vec<Option<u32>> = feed
            .iter()
            .map(|(r, _)| jumps_from_you(&cfg.systems, player_sys, r.primary_system().map(|s| s.id)))
            .collect();

        let mut kills_send: std::collections::HashMap<i64, crate::kills::KillInfo> = Default::default();
        let mut kill_chars: Vec<i64> = Vec::new();
        {
            let kc = kill_cache.lock().unwrap();
            for (r, _) in &feed {
                for lnk in &r.links {
                    if let Some(kid) = lnk.kill_id {
                        if let Some(Some(info)) = kc.get(&kid) {
                            let info = info.clone();
                            kill_chars.extend(info.victim_char);
                            kill_chars.extend(info.final_blow_char);
                            kills_send.insert(kid, info);
                        }
                    }
                }
            }
        }
        let mut affil_send: std::collections::HashMap<i64, crate::affiliation::Affil> = Default::default();
        {
            let mut ac = affiliations.lock().unwrap();
            for &cid in resolved_pilots.values().chain(kill_chars.iter()) {
                ac.want(cid);
                if let Some(a) = ac.get(cid) {
                    affil_send.insert(cid, a);
                }
            }
        }

        let (fresh, daemon_secs) = {
            let mut st = self.alert_shared.lock().unwrap();
            (std::mem::take(&mut st.focus_pending), st.secs)
        };
        let secs = if !feature || feed.is_empty() {
            0.0
        } else if fresh {
            if daemon_secs.is_finite() { daemon_secs.max(0.0) } else { ALERT_SECS_INFINITE }
        } else {
            ALERT_SECS_REFRESH
        };

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&feed).unwrap_or_default().hash(&mut hasher);
        from_you.hash(&mut hasher);
        hash_sorted_map(&mut hasher, &status);
        hash_sorted_map(&mut hasher, &resolved_pilots);
        hash_sorted_map(&mut hasher, &last_ship);
        hash_sorted_map(&mut hasher, &kills_send);
        hash_sorted_map(&mut hasher, &affil_send);
        let hash = hasher.finish();

        {
            let mut prev = self.alert_sent_hash.lock().unwrap();
            if *prev == Some(hash) && !fresh {
                return;
            }
            *prev = Some(hash);
        }
        let msg = crate::ipc::AlertMsg {
            feed,
            from_you,
            status,
            resolved_pilots,
            uncertain,
            last_ship,
            kills: kills_send,
            affil: affil_send,
            secs,
            focus: fresh,
        };
        crate::ipc::send_shared(&self.overlay_stdin, &crate::ipc::MainToOverlay::Alert(msg));
    }

    /// Forward fleet pings to the overlay from the engine thread, so a ping raises the overlay
    /// window even while the main window is minimized. Config (geometry/on-top) stays on the UI
    /// thread; it doesn't change while minimized.
    fn push_ping_update(&self, ping_shared: &SharedPingWindow) {
        use std::hash::{Hash, Hasher};
        if self.overlay_stdin.lock().unwrap().is_none() {
            return;
        }
        let msg = {
            let mut st = ping_shared.lock().unwrap();
            let raise = std::mem::take(&mut st.raise);
            let pings: Vec<crate::pings::Ping> = st.windows.iter().map(|w| w.ping.clone()).collect();
            crate::ipc::PingMsg {
                pings,
                raise,
                doctrine_url: st.doctrine_url.clone(),
                op_links: st.op_links.clone(),
            }
        };
        let hash = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&msg.pings).unwrap_or_default().hash(&mut h);
            msg.doctrine_url.hash(&mut h);
            let mut ops: Vec<(&String, &String)> = msg.op_links.iter().collect();
            ops.sort();
            ops.hash(&mut h);
            h.finish()
        };
        {
            let mut prev = self.ping_sent_hash.lock().unwrap();
            if Some(hash) == *prev && !msg.raise {
                return;
            }
            *prev = Some(hash);
        }
        crate::ipc::send_shared(&self.overlay_stdin, &crate::ipc::MainToOverlay::Ping(msg));
    }

    fn evaluate(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        player: &std::sync::Mutex<crate::esi::Player>,
    ) -> bool {
        let cfg = self.config.lock().unwrap().clone();
        if !cfg.enabled {
            return false;
        }
        let systems = cfg.systems.clone();
        let acfg = &cfg.alerts;
        let sev_rules = &cfg.severity;
        let only_undocked = cfg.only_undocked;
        let disabled = &cfg.disabled;
        let now = chrono::Utc::now().timestamp();
        let locations: std::collections::HashMap<String, (i64, bool)> =
            player.lock().unwrap().locations.clone();

        // Clear the alert-window snooze when any tracked character undocks (docked -> undocked),
        // then refresh the docked snapshot used to detect that edge.
        {
            let mut st = self.alert_shared.lock().unwrap();
            if st.snooze
                && locations
                    .iter()
                    .any(|(name, (_, docked))| !docked && st.docked_prev.get(name).copied() == Some(true))
            {
                st.snooze = false;
            }
            st.docked_prev = locations.iter().map(|(k, (_, d))| (k.clone(), *d)).collect();
        }

        let char_systems = |chars: &[String]| -> Vec<i64> {
            locations
                .iter()
                .filter(|(name, _)| {
                    if !chars.is_empty() {
                        chars.iter().any(|c| c.eq_ignore_ascii_case(name))
                    } else {
                        !disabled.iter().any(|d| d.eq_ignore_ascii_case(name))
                    }
                })
                .filter(|(_, (_, docked))| !(only_undocked && *docked))
                .map(|(_, (sys, _))| *sys)
                .collect()
        };

        struct Fire {
            id: u64,
            rule_id: u64,
            sys_id: i64,
            title: String,
            text: String,
            body: String,
            report: crate::intel::IntelReport,
            sev: crate::settings::Severity,
            sound: String,
            volume: f32,
            sys: bool,
            win: bool,
            push: bool,
        }
        let mut fired: Vec<Fire> = Vec::new();
        let mut rt = self.runtime.lock().unwrap();
        let mut newest = rt.last_alert_time;
        {
            const KILLMAIL_ALERT_WINDOW: i64 = 600;
            let state = intel_state.lock().unwrap();
            for r in &state.reports {
                if r.primary_system().is_none() && r.gates.is_empty() {
                    continue;
                }
                let fresh = if r.killmail {
                    now - r.received < KILLMAIL_ALERT_WINDOW && !rt.alerted.contains_key(&r.id)
                } else {
                    r.received > rt.last_alert_time
                };
                if !fresh {
                    continue;
                }
                if !r.killmail {
                    newest = newest.max(r.received);
                }
                let sev = severity_of(r, sev_rules);
                let target = r.primary_system().map(|s| s.id);
                let mut chosen: Option<(&crate::settings::AlertRule, Option<u32>)> = None;
                for ru in acfg.rules.iter().filter(|ru| ru.enabled) {
                    let srcs = char_systems(&ru.characters);
                    let jumps = min_jumps_from(&systems, &srcs, target, ru.count_bridges);
                    if rule_matches(ru, r, sev, jumps, &systems) {
                        chosen = Some((ru, jumps));
                        break;
                    }
                }
                let Some((ru, jumps)) = chosen else { continue };
                if ru.suppress {
                    rt.matched_ui.push((r.clone(), sev, ru.id, true));
                    // Dedup suppressed killmails, which are otherwise re-seen every tick within
                    // their window (non-killmails are gated by the fresh/last_alert_time check).
                    rt.alerted.insert(r.id, now);
                    continue;
                }
                let sev = ru.severity_override.unwrap_or(sev);
                let sound = if ru.sound.is_empty() {
                    acfg.sounds.get(sev as usize).cloned().unwrap_or_default()
                } else {
                    ru.sound.clone()
                };
                let volume = ru.volume.unwrap_or(acfg.alert_volume);
                let (sys, win, push, cd) =
                    (ru.system_notification, ru.custom_window, ru.push, ru.cooldown_secs);
                let sys_id = r.primary_system().map_or(0, |s| s.id);
                if now - rt.cooldown.get(&sys_id).copied().unwrap_or(0) < cd {
                    continue;
                }
                if rt.alerted.contains_key(&r.id) {
                    continue;
                }
                let title = r
                    .primary_system()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Intel".to_owned());
                let title = match jumps {
                    Some(j) if j > 0 => format!("{title}: {j} jumps"),
                    Some(_) => format!("{title} (here)"),
                    None => title,
                };
                let text = alert_text(r);
                // Pilot names (even unconfirmed) in the OS notification, capped for readability.
                // alert_text omits them and is shared with pushover/alert-window/log, so append
                // here on the notification body only.
                let names: Vec<&str> = r
                    .pilots
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|n| !crate::intel::is_pilot_stopword(n))
                    .collect();
                let pilots_line = if names.is_empty() {
                    String::new()
                } else {
                    let shown = names.iter().take(5).copied().collect::<Vec<_>>().join(", ");
                    let extra = names.len().saturating_sub(5);
                    if extra > 0 {
                        format!("\n{shown} +{extra} more")
                    } else {
                        format!("\n{shown}")
                    }
                };
                let body = format!("{text}{pilots_line}\n{} · {}", r.reporter, r.channel);
                fired.push(Fire {
                    id: r.id,
                    rule_id: ru.id,
                    sys_id,
                    title,
                    text,
                    body,
                    report: r.clone(),
                    sev,
                    sound,
                    volume,
                    sys,
                    win,
                    push,
                });
            }
        }
        rt.last_alert_time = newest;
        if rt.alerted.len() > 4000 {
            rt.alerted.retain(|_, t| now - *t < 7200);
        }
        if fired.is_empty() {
            return false;
        }
        for f in &fired {
            rt.cooldown.insert(f.sys_id, now);
            rt.alerted.insert(f.id, now);
            rt.fired_ui.push((f.report.clone(), f.sev, f.win));
            rt.matched_ui.push((f.report.clone(), f.sev, f.rule_id, false));
        }
        drop(rt);
        if fired.iter().any(|f| f.win) {
            let timeout = acfg.window_timeout;
            let secs = if timeout <= 0.0 { f32::INFINITY } else { timeout.max(3.0) };
            let pushed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
                fired.iter().filter(|f| f.win).map(|f| (f.report.clone(), f.sev)).collect();
            let mut st = self.alert_shared.lock().unwrap();
            for rs in &pushed {
                st.feed.push(rs.clone());
            }
            let n = st.feed.len();
            if n > 100 {
                st.feed.drain(0..n - 100);
            }
            // Snooze suppresses the popup window only; feed, overlay, sound and push still run.
            if !st.snooze {
                st.secs = secs;
                st.focus_pending = true;
            }
            drop(st);
            crate::ipc::send_shared(
                &self.overlay_stdin,
                &crate::ipc::MainToOverlay::AlertPush(crate::ipc::AlertPush {
                    reports: pushed,
                    secs: if secs.is_finite() { secs } else { ALERT_SECS_INFINITE },
                }),
            );
            self.ctx.request_repaint_of(egui::ViewportId::from_hash_of("alert_window"));
            self.ctx.request_repaint();
        }
        {
            let mut log = self.recent.lock().unwrap();
            for f in &fired {
                log.push((now, f.text.clone()));
            }
            let len = log.len();
            if len > 50 {
                log.drain(0..len - 50);
            }
        }
        for f in &fired {
            if f.sys {
                notify(f.title.clone(), f.body.clone());
            }
            if !f.sound.is_empty() && !f.sound.eq_ignore_ascii_case("off") {
                crate::sound::play_prio(&f.sound, f.sev as u8, f.volume);
            }
            if f.push && acfg.push_enabled {
                crate::push::pushover(&acfg.pushover_token, &acfg.pushover_user, &f.text);
            }
        }
        true
    }

    fn ingest_kills(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        kill_cache: &crate::kills::KillCache,
        killfeed: &crate::zkill::SharedKillFeed,
        player: &std::sync::Mutex<crate::esi::Player>,
        ship_by_id: &std::collections::HashMap<i64, String>,
        store: Option<&crate::store::Store>,
    ) -> bool {
        let cfg = self.config.lock().unwrap().clone();
        if !cfg.kill_intel {
            return false;
        }
        let events: Vec<crate::zkill::KillEvent> =
            std::mem::take(&mut *killfeed.lock().unwrap());
        if events.is_empty() {
            return false;
        }
        let Some(geo) = cfg.systems.clone() else { return false };
        let me = {
            let p = player.lock().unwrap();
            p.locations.get(&cfg.active_character).map(|(s, _)| *s).or(p.system_id)
        };
        let Some(me) = me else { return false };
        let range = match (cfg.kill_intel_jumps, cfg.intel_max_jumps) {
            (0, 0) => 10,
            (0, feed) => feed,
            (k, _) => k,
        };
        // Build cards + enrich/persist WITHOUT holding intel_state: poll_kill_fetches locks
        // kill_cache → intel_state, so nesting intel_state → kill_cache here would ABBA-deadlock.
        let mut reports = Vec::new();
        for ev in events {
            if geo.jumps(me, ev.system_id, range).is_none() {
                continue;
            }
            let Some(report) = kill_report(&ev, &geo, ship_by_id) else { continue };
            kill_cache.lock().unwrap().insert(ev.killmail_id, Some(ev.info.clone()));
            if let Some(store) = store {
                store.add_kill_intel(ev.killmail_id, ev.system_id, ev.ship_type_id, ev.time, ev.value);
                store.save_kill_details(&ev.info);
            }
            reports.push(report);
        }
        if reports.is_empty() {
            return false;
        }
        let mut st = intel_state.lock().unwrap();
        for report in reports {
            st.push(report);
        }
        true
    }

    fn reconcile(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        pilots: &std::sync::Mutex<crate::pilot::PilotCache>,
    ) -> bool {
        let cfg = self.config.lock().unwrap().clone();
        let Some(geo) = cfg.systems.clone() else { return false };
        let ships = cfg.ship_index.clone();
        let mut changed = false;
        // Lock order MUST match the watcher (intel_state → pilots) to avoid an ABBA deadlock.
        let mut st = intel_state.lock().unwrap();
        let mut cache = pilots.lock().unwrap();
        for r in &mut st.reports {
            let original: Vec<String> = std::mem::take(&mut r.pilots);
            let mut new_pilots: Vec<String> = Vec::new();
            for p in original.iter().cloned() {
                if crate::intel::is_pilot_stopword(&p) {
                    continue;
                }
                match cache.get(&p) {
                    Some(Some(_)) if cache.is_hidden(&p) => {
                        changed = true;
                    }
                    Some(Some(_)) => new_pilots.push(p),
                    None => {
                        cache.queue(&p);
                        for w in crate::pilot::name_windows(&p) {
                            cache.queue(&w);
                        }
                        new_pilots.push(p);
                    }
                    Some(None) => {
                        let cover: Vec<String> = cache
                            .cover(&p)
                            .into_iter()
                            .filter(|n| !crate::intel::is_pilot_stopword(n))
                            .collect();
                        if !cover.is_empty() {
                            new_pilots.extend(cover);
                        } else if p.split_whitespace().count() == 2 && !cache.is_reverified(&p) {
                            cache.force_requeue(&p);
                            new_pilots.push(p);
                        } else {
                            for w in crate::pilot::name_windows(&p) {
                                cache.queue(&w);
                            }
                            if let Some(info) = p.split_whitespace().find_map(|t| geo.lookup(t)) {
                                if !r.systems.iter().any(|d| d.id == info.id) {
                                    r.systems.push(crate::intel::DetectedSystem {
                                        id: info.id,
                                        name: info.name.clone(),
                                        security: info.security,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            let mut seen = std::collections::HashSet::new();
            new_pilots.retain(|p| seen.insert(p.to_lowercase()));
            let mut final_pilots: Vec<String> = Vec::new();
            for p in new_pilots {
                if let Some(idx) = &ships {
                    if !p.contains(' ') {
                        if let Some((id, name)) = idx.get(&p.to_lowercase()) {
                            if !r.ships.iter().any(|sh| sh.id == *id) {
                                r.ships.push(crate::intel::DetectedShip {
                                    id: *id,
                                    name: name.clone(),
                                });
                            }
                            continue;
                        }
                    }
                }
                final_pilots.push(p);
            }
            if final_pilots != original {
                changed = true;
            }
            r.pilots = final_pilots;
            let deduped = crate::intel::drop_covered_prefixes(&r.pilots, &r.text);
            if deduped.len() != r.pilots.len() {
                changed = true;
                r.pilots = deduped;
            }
            if r.systems.is_empty() && r.gates.is_empty() && !r.pilots.is_empty() {
                let reserved: std::collections::HashSet<String> = r
                    .pilots
                    .iter()
                    .flat_map(|p| p.split_whitespace())
                    .map(|w| w.to_lowercase())
                    .collect();
                let tokens = crate::intel::tokenize(&r.text);
                let lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
                let (detected, gates, _) =
                    crate::intel::detect_location(&tokens, &lower, &reserved, &geo, None, &[]);
                if !detected.is_empty() || !gates.is_empty() {
                    r.systems = detected;
                    r.gates = gates;
                    changed = true;
                }
            }
            let mut add = 0u32;
            let mut requeue: Vec<String> = Vec::new();
            let pilots_lc: Vec<String> = r.pilots.iter().map(|p| p.to_lowercase()).collect();
            r.name_number_skips.retain(|(cand, num)| {
                if pilots_lc.iter().any(|p| p.contains(&cand.to_lowercase())) {
                    return false;
                }
                match cache.get(cand) {
                    Some(None) => {
                        add += *num;
                        false
                    }
                    Some(Some(_)) => false,
                    None => {
                        requeue.push(cand.clone());
                        true
                    }
                }
            });
            for c in requeue {
                cache.queue(&c);
            }
            if add > 0 {
                r.count_ships = (r.count_ships + add).min(999);
            }
            // Re-derive the count from the pilots that SURVIVED resolution, so a discarded
            // candidate stops inflating it.
            let new_count = crate::intel::derive_count(
                r.count_extra,
                r.count_plus,
                r.count_ships,
                r.pilots.len() as u32,
                r.solo,
            );
            if r.count != new_count {
                r.count = new_count;
                changed = true;
            }
        }
        changed
    }

}

fn celestial_badge_label(name: &str) -> String {
    if let Some((planet_part, moon_n)) = name.split_once(" - Moon ") {
        if let Some(roman) = planet_part.rsplit(' ').next() {
            if let Some(p) = roman_to_int(roman) {
                return format!("Moon {p}-{moon_n}");
            }
        }
    }
    name.to_owned()
}

fn roman_to_int(s: &str) -> Option<i64> {
    let mut total = 0;
    let mut prev = 0;
    for c in s.chars().rev() {
        let v = match c {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1000,
            _ => return None,
        };
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    (total > 0).then_some(total)
}

fn kill_is_noise(ship_name: &str, value: f64) -> bool {
    let lower = ship_name.to_lowercase();
    lower.is_empty()
        || lower.contains("shuttle")
        || lower.contains("mobile ")
        || matches!(lower.as_str(), "reaper" | "impairor" | "ibis" | "velator")
        || (lower.starts_with("capsule") && value < 10_000_000.0)
}

fn kill_report(
    ev: &crate::zkill::KillEvent,
    geo: &crate::geo::Systems,
    ship_by_id: &std::collections::HashMap<i64, String>,
) -> Option<crate::intel::IntelReport> {
        let sys = geo.info_of(ev.system_id)?;
        let ship = ship_by_id
            .get(&ev.ship_type_id)
            .cloned()
            .or_else(|| crate::intel::structure_name_by_type(ev.ship_type_id).map(str::to_owned))
            .unwrap_or_default();
        if kill_is_noise(&ship, ev.value) {
            return None;
        }
        let mut report = crate::intel::IntelReport::default();
        report.received = ev.time;
        report.killmail = true;
        report.near_celestial = ev.info.near_celestial.clone();
        report.channel = "zKill".to_owned();
        report.reporter = "zKill".to_owned();
        report.isk = Some(ev.value as u64);
        report.systems.push(crate::intel::DetectedSystem {
            id: sys.id,
            name: sys.name.clone(),
            security: sys.security,
        });
        report.ships.push(crate::intel::DetectedShip { id: ev.ship_type_id, name: ship.clone() });
        report.text = format!("{} lost in {}", ship, sys.name);
        report.links.push(crate::intel::IntelLink {
            kind: crate::intel::LinkKind::Killmail,
            url: format!("https://zkillboard.com/kill/{}/", ev.killmail_id),
            kill_id: Some(ev.killmail_id),
        });
        Some(report)
}

#[allow(clippy::too_many_arguments)]
fn spawn_alert_daemon(
    engine: std::sync::Arc<AlertEngine>,
    intel_state: std::sync::Arc<std::sync::Mutex<crate::intel::IntelState>>,
    pilots: crate::pilot::SharedPilots,
    player: crate::esi::SharedPlayer,
    killfeed: crate::zkill::SharedKillFeed,
    kill_cache: crate::kills::KillCache,
    system_status: crate::systemstatus::SharedStatus,
    affiliations: crate::affiliation::SharedAffil,
    ping_shared: SharedPingWindow,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let store = crate::store::Store::open().ok();
        let mut ship_by_id: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(400));
            if ship_by_id.is_empty() {
                if let Some(idx) = engine.config.lock().unwrap().ship_index.clone() {
                    for (id, name) in idx.values() {
                        ship_by_id.insert(*id, name.clone());
                    }
                }
            }
            let mut dirty = engine.ingest_kills(
                &intel_state,
                &kill_cache,
                &killfeed,
                &player,
                &ship_by_id,
                store.as_ref(),
            );
            dirty |= engine.reconcile(&intel_state, &pilots);
            dirty |= engine.evaluate(&intel_state, &player);
            engine.push_overlay_update(
                &intel_state,
                &pilots,
                &player,
                &system_status,
                &affiliations,
                &kill_cache,
            );
            engine.push_ping_update(&ping_shared);
            if dirty {
                ctx.request_repaint();
            }
        }
    });
}

impl eframe::App for SpaiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        for c in std::mem::take(&mut self.pending_overlay_clicks) {
            self.act_on_intel_click(c, &ctx);
        }

        if crate::instance::take_raise_request() {
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
                match m {
                    crate::ipc::OverlayToMain::Click(c) => {
                        self.pending_overlay_clicks.push(c);
                        ctx.request_repaint();
                    }
                    crate::ipc::OverlayToMain::Verdict { name, hidden } => {
                        self.apply_pilot_verdict(&name, hidden)
                    }
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
                    crate::ipc::OverlayToMain::Hello => {}
                }
            }
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
            tray.set_attention(self.jabber_has_unread());
        }
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

        self.settings.theme.apply(&ctx);

        self.refresh_characters();
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
        self.poll_dscan_clipboard();
        self.poll_jabber_notify(&ctx);
        self.poll_kill_fetches();
        self.dscan_dialog(&ctx);
        self.ping_rules_dialog(&ctx);
        self.maybe_rebuild_graph(&ctx);
        self.persist_view_options();
        self.discover_sov_alliances(&ctx);
        self.os_notify
            .store(self.settings.alert_combat, std::sync::atomic::Ordering::Relaxed);
        self.drain_alerts();
        self.top_bar(ui);
        self.status_bar(ui);
        self.nav_rail(ui);

        egui::CentralPanel::default().show_inside(ui, |ui| match self.view {
            View::Dashboard => self.dashboard_view(ui),
            View::Map => self.map_view(ui),
            View::Characters => self.characters_view(ui),
            View::Intel => self.intel_view(ui),
            View::Battles => self.battles_view(ui),
            View::Wormholes => self.wormholes_view(ui),
            View::Lookup => self.lookup_view(ui),
            View::Alerts => self.alerts_view(ui),
            View::Jabber => self.jabber_view(ui),
            View::Settings => self.settings_view(ui),
        });

        self.intel_channels_window(&ctx);
        self.jump_bridges_window(&ctx);
        self.sov_upgrades_window(&ctx);
        self.coalitions_window(&ctx);
        self.travel_sov_dialog(&ctx);
        self.severity_window(&ctx);
        self.alert_window(&ctx);
        self.system_window(&ctx);
        self.constellation_window(&ctx);
        self.region_window(&ctx);
        self.ship_window(&ctx);
        self.pilot_window(&ctx);
        self.fit_window(&ctx);
        self.battle_filter_dialog(&ctx);
        self.filter_picker_dialog(&ctx);
        self.verdict_dialog(&ctx);
        self.dscan_view_dialog(&ctx);
        self.fleet_ping_window_ui(&ctx);
        self.routes_dialog(&ctx);
        self.safety_watch(&ctx);
        self.screen_flash(&ctx);
        if let Some(vp) = self.focus_window.take() {
            ctx.send_viewport_cmd_to(vp, egui::ViewportCommand::Focus);
        }
        if self.map_popped {
            self.show_map_viewport(&ctx);
        }
        self.char_popout_windows(&ctx);
        if self.jabber_popped {
            self.show_jabber_viewport(&ctx);
        }

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
        // backbuffer with opaque panels, so they still look solid. A semi-opaque clear used
        // to leak through as a dark/"black" idle alert window. EVE_SPAI_OPAQUE forces a solid
        // clear if a driver mis-presents transparency.
        if crate::transparency_enabled() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            egui::Color32::from_rgb(12, 12, 12).to_normalized_gamma_f32()
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
    let mut out = Vec::new();
    for line in text.lines() {
        let mut ends: Vec<String> = Vec::new();
        for raw in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '\'')) {
            if let Some(name) = resolve_system(graph, raw) {
                if !ends.contains(&name) {
                    ends.push(name);
                }
                if ends.len() == 2 {
                    break;
                }
            }
        }
        if ends.len() == 2 {
            out.push(crate::settings::JumpBridge { from: ends.remove(0), to: ends.remove(0) });
        }
    }
    out
}

fn split_upgrade_label(label: &str) -> Vec<&str> {
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

fn jumps_from_you(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    player_sys: Option<i64>,
    target: Option<i64>,
) -> Option<u32> {
    let (sys, p, t) = (systems.as_ref()?, player_sys?, target?);
    sys.jumps(t, p, 50)
}

fn min_jumps_from(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    srcs: &[i64],
    target: Option<i64>,
    use_bridges: bool,
) -> Option<u32> {
    let (sys, t) = (systems.as_ref()?, target?);
    srcs.iter()
        .filter_map(|&s| {
            if use_bridges {
                sys.jumps(t, s, 50)
            } else {
                sys.jumps_gates_only(t, s, 50)
            }
        })
        .min()
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

struct Convo {
    jid: String,
    name: String,
    unread: bool,
    group: String,
    presence: crate::jabber::Presence,
    status_text: String,
}

fn eve_time_label(ts: i64, now: i64) -> String {
    use chrono::{Datelike, TimeZone, Utc};
    let Some(t) = Utc.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    let n = Utc.timestamp_opt(now, 0).single().unwrap_or(t);
    if t.year() == n.year() && t.ordinal() == n.ordinal() {
        format!("EVE {}", t.format("%H:%M"))
    } else {
        format!("EVE {}", t.format("%Y/%m/%d %H:%M"))
    }
}

fn render_message_body(ui: &mut egui::Ui, body: &str) {
    let mut rest = body;
    while let Some(rel) = rest.find("http") {
        let after = &rest[rel..];
        if after.starts_with("http://") || after.starts_with("https://") {
            if rel > 0 {
                ui.label(&rest[..rel]);
            }
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            let url = &after[..end];
            ui.hyperlink_to(url, url);
            rest = &after[end..];
        } else {
            ui.label(&rest[..rel + 4]);
            rest = &rest[rel + 4..];
        }
    }
    if !rest.is_empty() {
        ui.label(rest);
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

/// A selectable chip whose border is drawn in every state, so hovering doesn't pop a border in and
/// nudge the row. egui's selectable_label hides the frame when inactive+unselected (Button::selectable
/// sets frame_when_inactive(selected)); a plain Button keeps frame_when_inactive on by default, giving
/// a stable box across idle/hover/selected.
fn selectable_chip<'a>(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl egui::IntoAtoms<'a>,
) -> egui::Response {
    ui.add(egui::Button::new(text).selected(selected))
}

// Compact VSCode-style Jabber tabs: uniform height, flush (no rounded box), a leading room icon or
// presence dot, the name, and a trailing close X on hover/active.
const TAB_H: f32 = 24.0;
const TAB_PAD_X: f32 = 8.0;
const TAB_GAP: f32 = 6.0;
const TAB_LEAD_W: f32 = 16.0;
const TAB_CLOSE_W: f32 = 16.0;
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

const UNREAD_RED: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x4C, 0x4C);
/// Backdrop for a chat line that named us. Alpha-blended so it reads on both themes.
const MENTION_BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(0x38, 0x14, 0x14, 0x50);

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
        TAB_GAP + TAB_CLOSE_W
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

/// One compact, flush Jabber tab. Fixed height, optional leading dot/icon, the name, and a trailing
/// close X shown on hover or when active (its slot otherwise carries the unread marker). Returns
/// `(select_clicked, close_clicked)`.
fn jabber_tab_box(
    ui: &mut egui::Ui,
    selected: bool,
    unread: bool,
    mention: bool,
    lead: TabLead,
    closable: bool,
    label: &str,
) -> (bool, bool) {
    let w = jabber_tab_width(ui, closable, unread, label);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, TAB_H), egui::Sense::click());
    let hovered = resp.hovered();
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
    let slot_cx = rect.right() - TAB_PAD_X - TAB_CLOSE_W / 2.0;
    if closable {
        let close_rect =
            egui::Rect::from_center_size(egui::pos2(slot_cx, cy), egui::vec2(TAB_CLOSE_W, TAB_CLOSE_W));
        if hovered || selected {
            let cresp = ui.interact(close_rect, resp.id.with("close"), egui::Sense::click());
            let col = if cresp.hovered() { strong_col } else { weak_col };
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::X,
                egui::FontId::proportional(13.0),
                col,
            );
            if cresp.clicked() {
                close = true;
                select = false;
            }
        } else if unread {
            mark(close_rect.center());
        }
    } else if unread {
        mark(egui::pos2(slot_cx, cy));
    }
    (select, close)
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
    Failed,
}

impl DscanFetch {
    fn snapshot(&self) -> DscanFetch {
        match self {
            DscanFetch::Loading => DscanFetch::Loading,
            DscanFetch::Failed => DscanFetch::Failed,
            DscanFetch::Ready(v) => DscanFetch::Ready(v.clone()),
        }
    }
}

pub(crate) fn fetch_dscan_ships(
    url: &str,
    ship_index: Option<&std::collections::HashMap<String, (i64, String)>>,
) -> Option<Vec<(i64, String, u32)>> {
    let idx = ship_index?;
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
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
            _ => DscanFetch::Failed,
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
        .with_min_inner_size([size[0].min(380.0), size[1].min(320.0)])
        .with_always_on_top();
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
            ontop_pin(ctx, id);
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

fn human_ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 3600 {
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

fn fmt_isk(isk: f64) -> String {
    if isk >= 1e9 {
        format!("{:.1}B", isk / 1e9)
    } else if isk >= 1e6 {
        format!("{:.0}M", isk / 1e6)
    } else if isk >= 1e3 {
        format!("{:.0}k", isk / 1e3)
    } else {
        format!("{isk:.0}")
    }
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

#[derive(Clone, Copy)]
enum TravelEnd {
    Start,
    Dest,
}

/// How a route gets from one system to the next. Each kind draws in its own colour, so a glance at
/// the line says whether you are taking a gate, a bridge, or a hole.
#[derive(Clone, Copy, PartialEq)]
enum Leg {
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
struct SovArt {
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

fn eve_type_icon_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/types/{id}/icon?size={}", eve_img_size(px))
}

fn eve_type_render_url(id: impl std::fmt::Display, px: f32) -> String {
    format!("https://images.evetech.net/types/{id}/render?size={}", eve_img_size(px))
}

fn party_badge(ui: &mut egui::Ui, p: &crate::battle::Party, size: f32, clickable: bool) {
    use crate::battle::PartyKind;
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

fn hull_badge(ui: &mut egui::Ui, type_id: i64, size: f32) {
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

fn side_title(side: &crate::battle::Side) -> String {
    side.coalition
        .clone()
        .or_else(|| side.parties.first().map(|p| p.name.clone()))
        .unwrap_or_else(|| "?".to_owned())
}

fn toolbar_sep(ui: &mut egui::Ui) {
    let h = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, h), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

fn battle_preview_summary(ui: &mut egui::Ui, label: &str, b: &crate::battle::Battle) {
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

fn battle_row(
    ui: &mut egui::Ui,
    b: &crate::battle::Battle,
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
enum ShipHighlight {
    None,
    Hovered,
    Assist,
}

fn ship_row(
    ui: &mut egui::Ui,
    width: f32,
    party: &crate::battle::Party,
    ship: i64,
    pilot: &str,
    name_of: &dyn Fn(i64) -> String,
    lost: Option<&crate::battle::Lost>,
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

#[derive(Clone)]
pub(crate) struct PingShown {
    pub(crate) ping: crate::pings::Ping,
    pub(crate) shown_at: std::time::Instant,
}

#[derive(Default)]
pub(crate) struct PingWindowState {
    pub(crate) windows: Vec<PingShown>,
    pub(crate) raise: bool,
    pub(crate) on_top: crate::settings::OnTop,
    pub(crate) enabled: bool,
    pub(crate) eve_focused: bool,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    pub(crate) doctrine_url: String,
    pub(crate) op_links: std::collections::HashMap<String, String>,
    pub(crate) level_applied: Option<bool>,
    pub(crate) level_at: Option<std::time::Instant>,
    pub(crate) open: bool,
    /// When the window last (re)opened, for the brief post-show geometry re-assert (Windows race).
    pub(crate) geom_at: Option<std::time::Instant>,
    pub(crate) win_pos: Option<(f32, f32)>,
    pub(crate) win_size: Option<(f32, f32)>,
    pub(crate) moved: Option<(f32, f32)>,
    pub(crate) moved_size: Option<(f32, f32)>,
}

pub(crate) type SharedPingWindow = std::sync::Arc<std::sync::Mutex<PingWindowState>>;

/// Decide whether a captured window geometry should replace the stored one: rejects negative
/// (off-screen) coords and ignores sub-`min_delta` jitter. `None` means "leave the stored value".
pub(crate) fn geometry_update(
    prev: Option<(f32, f32)>,
    new: (f32, f32),
    min_delta: f32,
) -> Option<(f32, f32)> {
    // Allow negative coords: a monitor left of / above the primary has negative virtual-desktop
    // coordinates, and dropping them loses which monitor a window was on. Only reject winit garbage
    // (minimized-window sentinels report values around -32000).
    if new.0.abs() > 32000.0 || new.1.abs() > 32000.0 {
        return None;
    }
    match prev {
        Some((a, b)) if (a - new.0).abs() <= min_delta && (b - new.1).abs() <= min_delta => None,
        _ => Some(new),
    }
}

/// Seed size (never position) into an overlay viewport's per-frame builder.
///
/// Overlay viewports (alert + fleet ping) share this one rule so they can't drift apart: NEVER feed
/// the live saved position into the per-frame builder. egui diffs the `ViewportBuilder` every frame,
/// so a per-frame `with_position` issues a reposition command every frame; the render callback then
/// reads the window's actual `outer_rect` (off by WM rounding / decoration), persists it, and it is
/// fed back next frame, oscillating the window between two spots until the values converge. That is
/// the exact bug that regressed the alert window when only ping was fixed. Position is restored ONCE
/// on show via `ViewportCommand::OuterPosition` in the render callback instead. This helper takes no
/// position argument, so there is structurally no way to seed a live position through it.
///
/// Non-Windows seeds the saved size so no frame re-applies the default; Windows starts at the default
/// size and restores the saved size via command on show (same reason position is command-only there).
fn seed_overlay_size(
    b: egui::ViewportBuilder,
    size: Option<(f32, f32)>,
    default: [f32; 2],
) -> egui::ViewportBuilder {
    #[cfg(not(target_os = "windows"))]
    {
        b.with_inner_size(size.map_or(default, |(w, h)| [w, h]))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = size;
        b.with_inner_size(default)
    }
}

pub(crate) fn ping_viewport_builder(
    on_top: bool,
    pos: Option<(f32, f32)>,
    size: Option<(f32, f32)>,
) -> egui::ViewportBuilder {
    let mut b = egui::ViewportBuilder::default()
        .with_icon(app_icon())
        .with_title("EVE Spai \u{2014} Fleet ping")
        .with_min_inner_size([260.0, 100.0])
        .with_resizable(true)
        .with_taskbar(false)
        .with_visible(false)
        .with_window_level(if on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        });
    let _ = pos; // position is command-only on show; see seed_overlay_size
    b = seed_overlay_size(b, size, [520.0, 320.0]);
    #[cfg(target_os = "linux")]
    {
        b = b.with_window_type(egui::X11WindowType::Utility);
    }
    b
}

/// `AlertMsg::secs` sentinels (overlay IPC). A non-negative value resets the overlay's
/// countdown to that number of seconds; these negative sentinels mean "leave the overlay's own
/// countdown running" (content-only refresh) and "reset to an infinite (never auto-hide) timeout"
/// respectively. Negatives avoid serializing a non-finite f32 (serde_json emits JSON `null`).
pub(crate) const ALERT_SECS_REFRESH: f32 = -1.0;
pub(crate) const ALERT_SECS_INFINITE: f32 = -2.0;

/// Hash a `HashMap` into `h` in a key-sorted (order-independent) way, so a re-cloned map with the
/// same contents but different iteration order doesn't trigger a spurious overlay resend.
fn hash_sorted_map<K, V, H>(h: &mut H, m: &std::collections::HashMap<K, V>)
where
    K: Ord + std::hash::Hash,
    V: serde::Serialize,
    H: std::hash::Hasher,
{
    use std::hash::Hash;
    let mut entries: Vec<(&K, String)> =
        m.iter().map(|(k, v)| (k, serde_json::to_string(v).unwrap_or_default())).collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in entries {
        k.hash(h);
        v.hash(h);
    }
}

pub(crate) fn alert_viewport_builder(
    on_top: bool,
    pos: Option<(f32, f32)>,
    size: Option<(f32, f32)>,
) -> egui::ViewportBuilder {
    let mut b = egui::ViewportBuilder::default()
        .with_icon(app_icon())
        .with_title("EVE Spai \u{2014} alerts")
        .with_window_level(if on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        })
        .with_active(false)
        // Linux/X11 maps from creation and stays mapped + transparent + click-through when idle
        // (re-mapping steals focus, winit#1160). Windows starts HIDDEN and the render closure maps
        // it on an alert: a transparent-when-idle window renders as an opaque BLACK SQUARE there
        // (wgpu/DWM doesn't composite the alpha), so hiding when idle avoids it.
        .with_visible(!cfg!(target_os = "windows"))
        .with_decorations(false)
        .with_resizable(true)
        // Floor so the title-bar controls and resize grip always fit, even in compact mode.
        .with_min_inner_size([200.0, 90.0])
        .with_taskbar(false)
        // A normal managed top-level (kept above, off the taskbar). On KWin/Wayland the reliable
        // lever to keep it visible while the main window is minimized is a KWin window rule forcing
        // this window (title "EVE Spai \u{2014} alerts") to Minimized=No.
        //
        // Opaque on Windows: transparency is only for the Linux transparent-when-idle behaviour;
        // Windows hides the window when idle instead, so a transparent (composite-alpha DX12)
        // swapchain buys nothing there and crashes the GPU driver when dragged across monitors.
        .with_transparent(!cfg!(target_os = "windows"))
        .with_mouse_passthrough(true);
    let _ = pos; // position is command-only on show; see seed_overlay_size
    b = seed_overlay_size(b, size, [360.0, 240.0]);
    #[cfg(target_os = "linux")]
    {
        b = b.with_window_type(egui::X11WindowType::Utility);
    }
    b
}

/// Render a compact-mode hover card. Used both for the off-screen sizing pass (to create the popup
/// window at its final size) and for the actual paint in the `alert_tip` viewport.
fn render_tip_content(
    ui: &mut egui::Ui,
    content: &PendingTip,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
) {
    match content {
        PendingTip::Text(t) => {
            ui.label(t);
        }
        PendingTip::System(s) => system_hover(ui, systems, status, s),
        PendingTip::Ship(d, roles) => ship_hover(ui, d, roles),
        PendingTip::Identity {
            alliance,
            alliance_name,
            corp,
            corp_name,
            char_id,
            char_name,
            note,
        } => {
            tooltip_identity(
                ui,
                *alliance,
                alliance_name.clone(),
                *corp,
                corp_name.clone(),
                *char_id,
                char_name.clone(),
            );
            if let Some(n) = note {
                ui.label(egui::RichText::new(n).weak());
            }
        }
    }
}

#[allow(deprecated)]
pub(crate) fn build_alert_viewport_cb(
    alert_shared: SharedAlertWindow,
) -> std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync> {
    std::sync::Arc::new(move |ui: &mut egui::Ui, _class: egui::ViewportClass| {
        let ctx = ui.ctx().clone();
        let mut st = alert_shared.lock().unwrap();
        let active = st.enabled && (st.secs > 0.0 || st.pinned);
        if active {
            st.dismissed = false;
        }
        let want_visible =
            active || (st.enabled && !cfg!(target_os = "windows") && !st.dismissed);
        let want_passthrough = !active;
        if st.applied_visible != Some(want_visible) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(want_visible));
            st.applied_visible = Some(want_visible);
        }
        if st.applied_passthrough != Some(want_passthrough) {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(want_passthrough));
            st.applied_passthrough = Some(want_passthrough);
        }
        if !active {
            st.open = false;
            st.pinned = false;
            st.level_applied = None;
            drop(st);
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(&ctx, |_ui| {});
            return;
        }
        let just_opened = !st.open;
        st.open = true;
        // A pinned window is held open "until closed", so keep it unconditionally on top —
        // otherwise Smart on-top can drop it to a normal level and the EVE client covers it,
        // which reads as the pin "not staying visible" and the window being unmovable.
        let on_top = st.on_top_level || st.pinned;
        std::mem::take(&mut st.focus_pending);
        let feed = st.feed.clone();
        let from_you_pre = st.from_you.clone();
        let systems = st.systems.clone();
        let status = st.status.clone();
        let ship_details = st.ship_details.clone();
        let ship_roles = st.ship_roles.clone();
        let resolved_pilots = st.resolved_pilots.clone();
        let uncertain = st.uncertain.clone();
        let last_ship = st.last_ship.clone();
        let player_sys = st.player_sys;
        let kills = st.kills.clone();
        let affil = st.affil.clone();
        let win_pos = st.win_pos;
        let win_size = st.win_size;
        let secs = st.secs;
        let pinned_in = st.pinned;
        let snooze_in = st.snooze;
        let compact = st.compact;
        let mut level_applied = st.level_applied;
        let mut level_at = st.level_at;
        let mut geom_at = st.geom_at;
        let mut verdict_pending = st.verdict_pending.clone();
        let mut verdict_explained = st.verdict_explained;
        if just_opened {
            level_applied = None;
            geom_at = Some(std::time::Instant::now());
        }
        drop(st);
        let mut verdict_out_new: Vec<(String, bool)> = Vec::new();

        // Overlays request foreground only (via WindowLevel::AlwaysOnTop, a SWP_NOACTIVATE raise);
        // never take keyboard focus, so a new alert can't steal it from the game.
        // Re-assert the saved geometry for a short settle after (re)open. On Windows the window is
        // shown from hidden here and a single restore command races that map, so keep re-sending
        // until it sticks; on Linux the one-shot on open is enough.
        let settle = cfg!(target_os = "windows")
            && geom_at.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(400));
        if just_opened || settle {
            if let Some((w, h)) = win_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
            if let Some((x, y)) = win_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            }
            if settle {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        // Re-assert the level only on change or (re)open — NOT every frame (a viewport
        // command each frame pins egui at vsync).
        if just_opened || level_applied != Some(on_top) {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            level_applied = Some(on_top);
            level_at = Some(std::time::Instant::now());
        }
        // Windows only: the overlay window starts hidden and races the map, so re-assert
        // visibility/level periodically until it sticks. Off Windows this periodic re-raise of the
        // always-on-top window makes KWin/X11 replay a focus/raise-denied "invalid action" sound
        // whenever another app (not EVE) is focused; the on-change asserts above already cover Linux.
        let due = cfg!(target_os = "windows")
            && level_at.is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(800));
        if due {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            if on_top {
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::AlwaysOnTop,
                ));
            }
            level_at = Some(std::time::Instant::now());
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(800));

        let mut hovered = false;
        let mut dismiss = false;
        let mut pinned = pinned_in;
        let mut snooze = snooze_in;
        let mut moved: Option<(f32, f32)> = None;
        let mut moved_size: Option<(f32, f32)> = None;
        let mut compact_toggle: Option<bool> = None;
        let mut tip: Option<(egui::Pos2, PendingTip)> = None;
        let mut clicks: Vec<IntelClick> = Vec::new();
        let now_ts = chrono::Utc::now().timestamp();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x12, 0x14, 0x18))
                    .inner_margin(if compact { 3 } else { 8 }),
            )
            .show(&ctx, |ui| {
                if compact {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                    ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                }
                let mut buttons_left = f32::INFINITY;
                let row = ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  Intel alerts",
                            egui_phosphor::regular::DOTS_SIX
                        ))
                        .strong(),
                    );
                    ui.label(
                        egui::RichText::new(if secs.is_finite() {
                            if compact {
                                fmt_age_compact(secs as i64)
                            } else {
                                format!("{:.0}s", secs)
                            }
                        } else {
                            "\u{221E}".to_owned()
                        })
                        .weak(),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .button(egui_phosphor::regular::X)
                                .on_hover_text("Dismiss")
                                .clicked()
                            {
                                dismiss = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui_phosphor::regular::PUSH_PIN)
                                        .selected(pinned),
                                )
                                .on_hover_text("Pin open (hold until closed)")
                                .clicked()
                            {
                                pinned = !pinned;
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui_phosphor::regular::ALARM)
                                        .selected(snooze),
                                )
                                .on_hover_text("Snooze until I undock (keeps collecting intel)")
                                .clicked()
                            {
                                snooze = !snooze;
                            }
                            let (ticon, thint) = if compact {
                                (egui_phosphor::regular::ARROWS_OUT, "Expand")
                            } else {
                                (egui_phosphor::regular::ARROWS_IN, "Compact mode")
                            };
                            if ui.button(ticon).on_hover_text(thint).clicked() {
                                compact_toggle = Some(!compact);
                            }
                            buttons_left = ui.min_rect().left();
                        },
                    );
                });
                let row_rect = row.response.rect;
                let drag_rect = egui::Rect::from_min_max(
                    row_rect.min,
                    egui::pos2(buttons_left - 6.0, row_rect.max.y),
                );
                let drag =
                    ui.interact(drag_rect, ui.id().with("titledrag"), egui::Sense::drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if compact {
                        ui.spacing_mut().item_spacing.y = 2.0;
                    }
                    if let (Some(kills), Some(affil)) = (&kills, &affil) {
                        for (i, (r, sev)) in feed.iter().enumerate().rev() {
                            let from_you = if i < from_you_pre.len() {
                                from_you_pre[i]
                            } else {
                                jumps_from_you(
                                    &systems,
                                    player_sys,
                                    r.primary_system().map(|s| s.id),
                                )
                            };
                            if let Some(c) = intel_row(
                                ui, r, now_ts, false, from_you, &systems, &status,
                                &ship_details, &ship_roles, &resolved_pilots,
                                &uncertain, &last_ship,
                                kills, *sev, false, affil, compact, &mut tip,
                            ) {
                                match c {
                                    IntelClick::PilotVerdict(name) => verdict_pending = Some(name),
                                    other => clicks.push(other),
                                }
                            }
                        }
                    }
                });
                hovered = ui.ui_contains_pointer();
                resize_grip(ui);
            });
        if let Some(name) = verdict_pending.clone() {
            if !verdict_explained {
                let mut ack = false;
                let resp = egui::Modal::new(egui::Id::new("overlay_verdict_explainer")).show(&ctx, |ui| {
                    ui.set_max_width(320.0);
                    ui.heading("Uncertain pilot (?)");
                    ui.add_space(4.0);
                    ui.label(
                        "A \"?\" means this name matched a real EVE character that looks \
                         inactive. It may be a rarely-used pilot, or a chat word that matches a \
                         character name.",
                    );
                    ui.add_space(6.0);
                    ui.label(
                        "Mark it \"Real pilot\" to keep it, or \"Not a pilot\" to hide it. Your \
                         choice is remembered.",
                    );
                    ui.add_space(8.0);
                    if ui.button("Got it").clicked() {
                        ack = true;
                    }
                });
                hovered = true;
                if ack {
                    verdict_explained = true;
                } else if resp.should_close() {
                    verdict_pending = None;
                }
            } else {
                let mut decision: Option<bool> = None;
                let resp = egui::Modal::new(egui::Id::new("overlay_verdict_popup")).show(&ctx, |ui| {
                    ui.heading(format!("Is \"{name}\" a pilot?"));
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "\"{name}\" matched a character that looks inactive."
                        ))
                        .weak(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Real pilot").clicked() {
                            decision = Some(false);
                        }
                        if ui.button("Not a pilot (hide)").clicked() {
                            decision = Some(true);
                        }
                    });
                });
                hovered = true;
                if let Some(hidden) = decision {
                    verdict_out_new.push((name.clone(), hidden));
                    verdict_pending = None;
                } else if resp.should_close() {
                    verdict_pending = None;
                }
            }
        }
        if let Some(p) = ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y))) {
            moved = Some(p);
        }
        let sz = ctx.screen_rect().size();
        if sz.x > 0.0 && sz.y > 0.0 {
            moved_size = Some((sz.x, sz.y));
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            dismiss = true;
        }

        // Compact-mode hover cards render in their own opaque popup window so they can extend past
        // the small alert window. Shown only on the frames a widget is hovered; egui tears the
        // window down on the first frame we don't show it (pointer left the widget).
        if compact {
            if let Some((anchor, content)) = tip {
                let origin = ctx
                    .input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)))
                    .or(win_pos)
                    .unwrap_or((0.0, 0.0));
                let gx = origin.0 + anchor.x + 12.0;
                let gy = origin.1 + anchor.y + 18.0;
                let tip_systems = systems.clone();
                let tip_status = status.clone();
                const TIP_MAXW: f32 = 360.0;
                const TIP_MARGIN: f32 = 6.0;
                // Measure the card off-screen (invisible sizing pass) so the window is created at its
                // final size. Creating it at a placeholder size and resizing afterward makes the WM
                // visibly animate the resize.
                let measured = {
                    // `invisible` (not `sizing_pass`): a sizing pass reports minimum sizes that
                    // under-measure the real paint and clip the card. This lays out exactly as the
                    // real render, just off-screen and unpainted.
                    let mut mui = egui::Ui::new(
                        ctx.clone(),
                        egui::Id::new("alert_tip_measure"),
                        egui::UiBuilder::new().invisible().max_rect(egui::Rect::from_min_size(
                            egui::pos2(-1.0e6, -1.0e6),
                            egui::vec2(TIP_MAXW, 1.0e5),
                        )),
                    );
                    mui.set_max_width(TIP_MAXW);
                    render_tip_content(&mut mui, &content, &tip_systems, &tip_status);
                    mui.min_rect().size()
                };
                // Window inner area = content + 2*margin + 2*stroke; add a couple px so the real
                // (narrower) render doesn't wrap one extra line and clip. Under-padding here was why
                // tips came out too small.
                let pad = TIP_MARGIN * 2.0 + 2.0 + 3.0;
                let win_w = (measured.x + pad).min(TIP_MAXW + pad);
                let win_h = measured.y + pad;
                #[allow(unused_mut)]
                let mut tip_builder = egui::ViewportBuilder::default()
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_taskbar(false)
                    .with_active(false)
                    .with_transparent(false)
                    .with_mouse_passthrough(true)
                    .with_window_level(if on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    })
                    .with_position([gx, gy])
                    .with_inner_size([win_w, win_h]);
                // The tip maps/unmaps on every hover, and on X11 the WM focuses each newly-mapped
                // MANAGED window regardless of the active hint, stealing focus from the game. Make it
                // unmanaged (override-redirect) with the Tooltip type: no focus, no taskbar entry, no
                // restacking. with_active(false) above is what keeps it from activating on Windows/macOS.
                #[cfg(target_os = "linux")]
                {
                    tip_builder = tip_builder
                        .with_window_type(egui::X11WindowType::Tooltip)
                        .with_override_redirect(true);
                }
                ctx.show_viewport_deferred(
                    egui::ViewportId::from_hash_of("alert_tip"),
                    tip_builder,
                    move |ui: &mut egui::Ui, _class: egui::ViewportClass| {
                        let ctx = ui.ctx().clone();
                        egui::CentralPanel::default()
                            .frame(
                                egui::Frame::new()
                                    .fill(egui::Color32::from_rgb(0x12, 0x14, 0x18))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_rgb(0x33, 0x38, 0x40),
                                    ))
                                    .inner_margin(TIP_MARGIN as i8),
                            )
                            .show(&ctx, |ui| {
                                ui.set_max_width(TIP_MAXW);
                                render_tip_content(ui, &content, &tip_systems, &tip_status);
                            });
                    },
                );
            }
        }

        let dt = ctx.input(|i| i.unstable_dt).min(2.0);
        let ms = if hovered { 100 } else { 1000 };
        ctx.request_repaint_after(std::time::Duration::from_millis(ms));

        let mut st = alert_shared.lock().unwrap();
        if dismiss {
            // Closing overrides a pin: otherwise `active` (secs > 0 || pinned) keeps it open.
            st.secs = 0.0;
            pinned = false;
            st.dismissed = true;
        } else if hovered {
            st.secs = st.secs.max(3.0);
        } else if !pinned && st.secs.is_finite() {
            st.secs = (st.secs - dt).max(0.0);
        }
        st.pinned = pinned;
        st.snooze = snooze;
        // Edge event, not state: only write on an actual toggle. A blind write every render frame
        // would clobber a pending Some with None before the (independently paced) drainer sees it.
        // Apply the new value to st.compact HERE so the overlay flips immediately; the round-trip to
        // main is only for persistence. On Windows the main loop is paused while minimized, so
        // waiting for main to echo the setting back via Config never happens.
        if let Some(v) = compact_toggle {
            st.compact = v;
            st.compact_toggle = compact_toggle;
        }
        st.level_applied = level_applied;
        st.level_at = level_at;
        st.geom_at = geom_at;
        st.clicks.extend(clicks);
        st.verdict_pending = verdict_pending;
        st.verdict_explained = verdict_explained;
        st.verdict_out.extend(verdict_out_new);
        // Don't capture during the open/settle frames, where the window briefly reports its
        // pre-restore geometry (which would overwrite the user's real placement).
        if !just_opened && !settle {
            if let Some(p) = moved {
                st.moved = Some(p);
            }
            if let Some(s) = moved_size {
                st.moved_size = Some(s);
            }
        }
    })
}

#[allow(deprecated)]
pub(crate) fn build_ping_viewport_cb(
    ping_shared: SharedPingWindow,
) -> std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync> {
    std::sync::Arc::new(move |ui: &mut egui::Ui, _class: egui::ViewportClass| {
        let ctx = ui.ctx().clone();
        let mut st = ping_shared.lock().unwrap();
        if !st.enabled || st.windows.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            st.level_applied = None;
            st.open = false;
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        if ctx.input(|i| i.viewport().close_requested()) {
            st.windows.clear();
            st.level_applied = None;
            st.open = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            return;
        }
        let just_opened = !st.open;
        st.open = true;
        if just_opened {
            st.geom_at = Some(std::time::Instant::now());
        }
        let geom_at = st.geom_at;
        let win_pos = st.win_pos;
        let win_size = st.win_size;
        let on_top = st.on_top != crate::settings::OnTop::Never
            && (st.on_top == crate::settings::OnTop::Always || st.eve_focused);
        let due = st
            .level_at
            .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(800));
        if st.level_applied != Some(on_top) || (on_top && due) {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            st.level_applied = Some(on_top);
            st.level_at = Some(std::time::Instant::now());
        }
        // A new ping requests foreground via WindowLevel above (SWP_NOACTIVATE raise); it never
        // takes keyboard focus, so it can't steal it from the game. Clear the flag either way.
        std::mem::take(&mut st.raise);
        let pings = st.windows.clone();
        let systems = st.systems.clone();
        let doctrine_url = st.doctrine_url.clone();
        let op_links = st.op_links.clone();
        drop(st);
        // Re-assert saved geometry for a short settle after (re)open, so Windows' hidden->visible
        // map doesn't race the restore (see the alert window for the same reasoning).
        let settle = cfg!(target_os = "windows")
            && geom_at.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(400));
        if just_opened || settle {
            if let Some((w, h)) = win_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
            if let Some((x, y)) = win_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            }
            if settle {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        let blinking = pings.iter().any(|s| s.shown_at.elapsed().as_secs_f32() < 3.0);
        egui::CentralPanel::default().show(&ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (i, s) in pings.iter().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }
                    let resp = ui
                        .scope(|ui| {
                            render_ping(ui, &s.ping, &systems, true, &doctrine_url, &op_links);
                        })
                        .response;
                    let t = s.shown_at.elapsed().as_secs_f32();
                    if t < 3.0 {
                        let pulse = (t * std::f32::consts::PI * 3.0).sin() * 0.5 + 0.5;
                        let alpha = (pulse * 26.0) as u8;
                        let tint = egui::Color32::from_rgba_unmultiplied(0xff, 0xd1, 0x66, alpha);
                        ui.painter().rect_filled(resp.rect, 4.0, tint);
                    }
                }
            });
        });
        // Capture a user move/resize (not during the open/settle frames, which report the
        // pre-restore geometry). Sent to the main process, which persists it.
        if !just_opened && !settle {
            let moved = ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)));
            let sz = ctx.screen_rect().size();
            let mut st = ping_shared.lock().unwrap();
            if let Some(p) = moved {
                st.moved = Some(p);
            }
            if sz.x > 0.0 && sz.y > 0.0 {
                st.moved_size = Some((sz.x, sz.y));
            }
        }
        if blinking {
            ctx.request_repaint();
        }
        if on_top {
            ctx.request_repaint_after(std::time::Duration::from_millis(800));
        }
    })
}

/// Hover content recorded in compact mode, re-rendered in the separate `alert_tip` popup
/// viewport so it can extend past the small alert window.
pub(crate) enum PendingTip {
    Text(String),
    System(crate::intel::DetectedSystem),
    Ship(crate::store::ShipDetails, Vec<(&'static str, &'static str)>),
    Identity {
        alliance: Option<i64>,
        alliance_name: Option<String>,
        corp: Option<i64>,
        corp_name: Option<String>,
        char_id: Option<i64>,
        char_name: Option<String>,
        note: Option<String>,
    },
}

#[derive(Default)]
pub(crate) struct AlertWindowState {
    pub(crate) feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub(crate) from_you: Vec<Option<u32>>,
    pub(crate) secs: f32,
    pub(crate) pinned: bool,
    pub(crate) focus_pending: bool,
    pub(crate) open: bool,
    pub(crate) level_applied: Option<bool>,
    pub(crate) level_at: Option<std::time::Instant>,
    /// When the window last (re)opened, for the brief post-show geometry re-assert (Windows race).
    pub(crate) geom_at: Option<std::time::Instant>,
    pub(crate) applied_visible: Option<bool>,
    pub(crate) applied_passthrough: Option<bool>,
    pub(crate) enabled: bool,
    pub(crate) on_top_level: bool,
    pub(crate) compact: bool,
    pub(crate) compact_toggle: Option<bool>,
    pub(crate) win_pos: Option<(f32, f32)>,
    pub(crate) win_size: Option<(f32, f32)>,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    pub(crate) status: std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    pub(crate) ship_details: std::collections::HashMap<i64, crate::store::ShipDetails>,
    pub(crate) ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>,
    pub(crate) resolved_pilots: std::collections::HashMap<String, i64>,
    pub(crate) uncertain: std::collections::HashSet<String>,
    pub(crate) last_ship: std::collections::HashMap<String, (i64, String, i64)>,
    pub(crate) player_sys: Option<i64>,
    pub(crate) kills: Option<crate::kills::KillCache>,
    pub(crate) affil: Option<crate::affiliation::SharedAffil>,
    pub(crate) verdict_pending: Option<String>,
    pub(crate) verdict_explained: bool,
    pub(crate) clicks: Vec<IntelClick>,
    pub(crate) verdict_out: Vec<(String, bool)>,
    pub(crate) moved: Option<(f32, f32)>,
    pub(crate) moved_size: Option<(f32, f32)>,
    /// Explicitly dismissed by the user. On Linux this unmaps the otherwise always-mapped window;
    /// cleared when a new alert makes it active again.
    pub(crate) dismissed: bool,
    /// Suppress the alert window from auto-opening. Intel is still collected. Cleared when any
    /// tracked character transitions docked -> undocked (see `docked_prev`).
    pub(crate) snooze: bool,
    /// Last-seen docked state per character, for detecting the undock edge that clears `snooze`.
    pub(crate) docked_prev: std::collections::HashMap<String, bool>,
}

pub(crate) type SharedAlertWindow = std::sync::Arc<std::sync::Mutex<AlertWindowState>>;

#[derive(Clone, Copy, PartialEq)]
struct BattleHover {
    char_id: i64,
    kill_id: Option<i64>,
}

struct LoadedReport {
    title: String,
    battle: crate::battle::Battle,
    inv: crate::battle::Involvement,
    rosters: Vec<Vec<crate::battle::Participant>>,
    sorted: Vec<Vec<crate::battle::Participant>>,
    condensed_rows: Vec<Vec<crate::brview::CondensedRow>>,
    sorted_for: Option<(RosterSort, bool)>,
    hover: Option<BattleHover>,
}

fn battle_detail(
    ui: &mut egui::Ui,
    b: &crate::battle::Battle,
    type_names: &std::collections::HashMap<i64, String>,
    inv: &crate::battle::Involvement,
    rosters: &[Vec<crate::battle::Participant>],
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
        let remaining = crate::battle::BATTLE_WINDOW_SECS - (now - b.end);
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
fn condensed_row(
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

fn resolve_char_name(
    client: &reqwest::blocking::Client,
    name: &str,
) -> Result<String, String> {
    let name = name.trim();
    let body: serde_json::Value = client
        .post("https://esi.evetech.net/latest/universe/ids/?datasource=tranquility")
        .json(&[name])
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("lookup failed: {e}"))?;
    body.get("characters")
        .and_then(|c| c.as_array())
        .and_then(|a| a.iter().find_map(|c| c.get("name").and_then(|n| n.as_str())))
        .map(|s| s.to_owned())
        .ok_or_else(|| format!("No pilot named \"{name}\""))
}

fn rule_matches(
    ru: &crate::settings::AlertRule,
    r: &crate::intel::IntelReport,
    sev: crate::settings::Severity,
    jumps: Option<u32>,
    geo: &Option<std::sync::Arc<crate::geo::Systems>>,
) -> bool {
    if sev < ru.min_severity {
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
        // Distance-limited rule: fire only when the report is provably within range. If the
        // distance can't be measured — no known character location, or an unreachable target
        // (e.g. while you're in a wormhole) — it is NOT within range, so don't fire. (Failing
        // open here flooded alerts for k-space intel while the player sat in w-space.)
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
    kind_label: &str,
    is_editing: bool,
    edit_name: &mut String,
    edit_folder: &mut String,
    folders: &[String],
) -> RowAction {
    let mut act = RowAction::None;
    if is_editing {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(edit_name).desired_width(120.0).hint_text("Name"));
            egui::ComboBox::from_id_salt(("route_edit_folder", kind_label, it.name.as_str()))
                .selected_text(if edit_folder.is_empty() {
                    "(root)".to_owned()
                } else {
                    edit_folder.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(edit_folder, String::new(), "(root)");
                    for f in folders {
                        ui.selectable_value(edit_folder, f.clone(), f);
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
            ui.label(egui::RichText::new(kind_label).weak());
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

fn ontop_pin(ctx: &egui::Context, id: &str) {
    let key = egui::Id::new(("ontop", id));
    let mut on = ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(true));
    egui::Area::new(egui::Id::new(("ontop_area", id)))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-6.0, 6.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if ui
                .selectable_label(on, egui_phosphor::regular::PUSH_PIN)
                .on_hover_text(if on { "Always on top (on)" } else { "Always on top (off)" })
                .clicked()
            {
                on = !on;
                ctx.data_mut(|d| d.insert_temp(key, on));
            }
        });
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

fn open_mumble(link: String) {
    std::thread::spawn(move || {
        let resolved = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(10))
            .build()
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
            ui.hyperlink_to(icon::LINK, link).on_hover_text(link);
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
    let age = (now - p.timestamp()).max(0);
    let ago = if age < 60 {
        format!("{age}s")
    } else if age < 3600 {
        format!("{}m", age / 60)
    } else if age < 86_400 {
        format!("{}h", age / 3600)
    } else {
        format!("{}d", age / 86_400)
    };
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
                            .small_button(format!("{}  Copy", icon::COPY))
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
                            ui.label(format!("Comms: {t}"));
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
                ui.horizontal_wrapped(|ui| {
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
                if !description.is_empty() {
                    ui.label(egui::RichText::new(description).weak());
                }
                let from = source.as_deref().unwrap_or("?");
                let to = target.as_deref().unwrap_or("?");
                ui.label(egui::RichText::new(format!("{from} {} {to}", icon::ARROW_RIGHT)).weak().small());
            }
            Ping::Plain { text, sender, target, .. } => {
                ui.horizontal_wrapped(|ui| {
                    let from = sender.as_deref().unwrap_or("ping");
                    let to = target.as_deref().map(|t| format!(" {} {t}", icon::ARROW_RIGHT)).unwrap_or_default();
                    ui.label(egui::RichText::new(format!("{from}{to}")).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button(format!("{}  Copy", icon::COPY))
                            .on_hover_text("Copy the ping text")
                            .clicked()
                        {
                            ui.ctx().copy_text(p.raw().to_owned());
                        }
                        ui.label(egui::RichText::new(format!("{ago} ago")).weak());
                    });
                });
                ui.label(text);
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

fn severity_of(
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

fn severity_color(s: crate::settings::Severity) -> egui::Color32 {
    use crate::settings::Severity::*;
    match s {
        Info => egui::Color32::from_rgb(0x6E, 0x7A, 0x86),
        Warning => crate::theme::standing::WARNING,
        Danger => egui::Color32::from_rgb(0xE6, 0x6A, 0x2A),
        Critical => crate::theme::standing::HOSTILE,
    }
}

fn build_last_ship(
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

fn uncertain_set(
    cache: &crate::pilot::PilotCache,
    resolved: &std::collections::HashMap<String, i64>,
) -> std::collections::HashSet<String> {
    resolved.keys().filter(|n| cache.is_uncertain(n)).map(|n| n.to_lowercase()).collect()
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
            if allow_default && ui.selectable_label(is_default, "Default").clicked() {
                value.clear();
                changed = true;
            }
            if ui.selectable_label(is_off, "Off").clicked() {
                *value = "off".to_owned();
                changed = true;
            }
            for &p in crate::sound::PRESETS {
                ui.horizontal(|ui| {
                    if ui.selectable_label(value.eq_ignore_ascii_case(p), p).clicked() {
                        *value = p.to_owned();
                        changed = true;
                    }
                    if ui.small_button(icon::PLAY).on_hover_text("Preview").clicked() {
                        crate::sound::play(p, volume);
                    }
                });
            }
            if ui.selectable_label(is_file, format!("{} Custom file…", icon::FOLDER_OPEN)).clicked() {
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

/// A ship-class badge is worth showing only when the class is specific: a generic hull tier
/// (frigate..battleship) is noise, but T2/T3 specialisations and any capital-size hull matter.
fn interesting_ship_class(class: &str) -> bool {
    !matches!(class, "Frigate" | "Destroyer" | "Cruiser" | "Battlecruiser" | "Battleship")
}

fn wormhole_badge_label(r: &crate::intel::IntelReport) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(sig) = &r.wh_sig {
        parts.push(sig.clone());
    }
    if let Some(code) = &r.wh_type {
        if !code.eq_ignore_ascii_case("K162") {
            parts.push(code.clone());
        }
    }
    if let Some(size) = r.wh_size {
        parts.push(size.short().to_string());
    }
    if r.wh_drifter {
        parts.push("Drifter".into());
    }
    if let Some(dest) = r.wh_dest {
        use crate::wormholes::DestClass;
        match dest {
            DestClass::Thera => parts.push("Thera".into()),
            DestClass::Turnur => parts.push("Turnur".into()),
            DestClass::Unknown => {}
            other => parts.push(format!("\u{2192} {}", other.label())),
        }
    }
    let icon = egui_phosphor::regular::SPIRAL;
    if parts.is_empty() {
        icon.to_string()
    } else {
        format!("{icon} {}", parts.join(" "))
    }
}

fn anom_sig_badge_label(kind: crate::intel::AnomKind, code: &str) -> String {
    let word = match kind {
        crate::intel::AnomKind::Anomaly => "Anom",
        crate::intel::AnomKind::Signature => "Sig",
    };
    let icon = egui_phosphor::regular::CROSSHAIR;
    if code.is_empty() {
        format!("{icon} {word}")
    } else {
        format!("{icon} {word} {code}")
    }
}

fn intel_row(
    ui: &mut egui::Ui,
    r: &crate::intel::IntelReport,
    now: i64,
    stale: bool,
    from_you: Option<u32>,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    ship_details: &std::collections::HashMap<i64, crate::store::ShipDetails>,
    ship_roles: &std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>,
    resolved_pilots: &std::collections::HashMap<String, i64>,
    uncertain: &std::collections::HashSet<String>,
    last_ship: &std::collections::HashMap<String, (i64, String, i64)>,
    kills: &crate::kills::KillCache,
    sev: crate::settings::Severity,
    show_reporter: bool,
    affil: &crate::affiliation::SharedAffil,
    compact: bool,
    tip: &mut Option<(egui::Pos2, PendingTip)>,
) -> Option<IntelClick> {
    use egui_phosphor::regular as icon;
    let age = (now - r.received).max(0);
    let green = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
    let warn = crate::theme::standing::WARNING;
    let red = crate::theme::standing::HOSTILE;
    let accent = ui.visuals().hyperlink_color;
    let jumps_color = crate::theme::standing::CORP;

    let is_zkill = r.killmail && r.channel.eq_ignore_ascii_case("zkill");
    let type_icon = if r.clear {
        icon::CHECK_CIRCLE
    } else if is_zkill {
        icon::CROSSHAIR
    } else if r.killmail {
        icon::SKULL
    } else if r.spike || r.camp || r.bubble || r.cyno || r.dropper || r.help {
        icon::WARNING_OCTAGON
    } else if r.no_visual {
        icon::EYE_SLASH
    } else if !r.systems.is_empty() || r.count.is_some() {
        icon::WARNING
    } else {
        icon::INFO
    };
    let tint = if r.clear { green } else { severity_color(sev) };
    let icon_color = if is_zkill { egui::Color32::from_rgb(0xEF, 0x53, 0x50) } else { tint };
    let card_fill = if is_zkill {
        egui::Color32::from_rgb(12, 12, 12).gamma_multiply(if stale { 0.6 } else { 1.0 })
    } else {
        tint.gamma_multiply(if stale { 0.05 } else { 0.13 })
    };

    let toggle_id = egui::Id::new("intel_raw").with(report_key(r));
    let show_raw = !is_zkill && ui.ctx().data(|d| d.get_temp::<bool>(toggle_id).unwrap_or(false));

    let mut clicked: Option<IntelClick> = None;
    let mut consumed = false;
    let resp = egui::Frame::group(ui.style())
        .inner_margin(if compact {
            egui::Margin::symmetric(5, 1)
        } else {
            egui::Margin::symmetric(8, 4)
        })
        .fill(card_fill)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let msg = format!("{}\n{} · {}", r.text, r.reporter, r.channel);
            if show_raw {
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(type_icon).color(icon_color));
                        ui.label(
                            egui::RichText::new(format!("{:>7}", fmt_age(age))).monospace().weak(),
                        );
                        match from_you {
                            Some(0) => {
                                ui.label(egui::RichText::new("here").monospace().color(jumps_color));
                            }
                            Some(j) => {
                                ui.label(
                                    egui::RichText::new(format!("{j}j"))
                                        .monospace()
                                        .color(jumps_color),
                                );
                            }
                            None => {}
                        }
                        for s in &r.systems {
                            ui.label(egui::RichText::new(&s.name).strong().color(accent));
                        }
                    });
                    let body = if r.text.trim().is_empty() { "(no message text)" } else { &r.text };
                    ui.add(egui::Label::new(body).wrap());
                });
                return;
            }
            let mut render = |ui: &mut egui::Ui| {
                ui.spacing_mut().interact_size.y = if compact { 16.0 } else { 28.0 };
                ui.spacing_mut().button_padding.y = if compact { 1.0 } else { 2.0 };
                if compact {
                    // x = gap between chips on a line; y = gap between wrapped lines.
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
                }
                // All chips render as filled Buttons, not raw Frames. A Frame sizes its fill to the
                // content height, which floats taller than the button-height siblings and clips the
                // icon; a Button is bounded to interact_size.y so every chip is the same height.
                let chip = |ui: &mut egui::Ui, text: egui::RichText, fill: egui::Color32| {
                    ui.add(egui::Button::new(text).fill(fill).sense(egui::Sense::hover()))
                };
                let badge_isz = if compact { 16.0 } else { 24.0 };
                let pilot_isz = if compact { 16.0 } else { 20.0 };
                let age_txt = if compact {
                    format!("{:>4}", fmt_age_compact(age))
                } else {
                    format!("{:>7}", fmt_age(age))
                };
                let r1 = ui.label(egui::RichText::new(type_icon).color(icon_color));
                let r2 = ui.label(egui::RichText::new(age_txt).monospace().weak());
                if compact {
                    if r1.hovered() {
                        *tip = Some((r1.rect.right_top(), PendingTip::Text(msg.clone())));
                    }
                    if r2.hovered() {
                        *tip = Some((r2.rect.right_top(), PendingTip::Text(msg.clone())));
                    }
                } else {
                    r1.on_hover_text(&msg);
                    r2.on_hover_text(&msg);
                }
                let jtxt = match from_you {
                    Some(0) => "here".to_owned(),
                    Some(j) => format!("{j}j"),
                    None => String::new(),
                };
                ui.label(egui::RichText::new(format!("{jtxt:>4}")).monospace().color(jumps_color));

                let mut seen_sys = std::collections::HashSet::new();
                for s in &r.systems {
                    if !seen_sys.insert(s.id) {
                        continue;
                    }
                    let scol = security_color(s.security);
                    let text =
                        egui::RichText::new(format!("{} {}", icon::PLANET, s.name)).color(scol).strong();
                    let dim = scol.gamma_multiply(0.5);
                    let fill = egui::Color32::from_rgb(
                        (dim.r() as u16 * 45 / 100 + 0x10) as u8,
                        (dim.g() as u16 * 45 / 100 + 0x10) as u8,
                        (dim.b() as u16 * 45 / 100 + 0x10) as u8,
                    );
                    let panel = ui.add(egui::Button::new(text).fill(fill));
                    if compact {
                        if panel.hovered() {
                            *tip = Some((panel.rect.right_top(), PendingTip::System(s.clone())));
                        }
                    } else {
                        panel.clone().on_hover_ui(|ui| system_hover(ui, systems, status, s));
                    }
                    if panel.clicked() {
                        clicked = Some(IntelClick::System(s.id));
                    }
                }

                if let Some((cname, dm)) = &r.near_celestial {
                    if *dm <= 15_000_000.0 {
                        let km = (dm / 1000.0).round() as i64;
                        let dist = if km >= 1000 {
                            format!("{},{:03} km", km / 1000, km % 1000)
                        } else {
                            format!("{km} km")
                        };
                        let label = celestial_badge_label(cname);
                        let cicon = if cname.contains("gate") {
                            icon::SIGN_IN
                        } else if cname.contains("Moon") {
                            icon::MOON
                        } else if cname.ends_with("station") {
                            icon::MAP_PIN_LINE
                        } else {
                            icon::PLANET
                        };
                        chip(
                            ui,
                            egui::RichText::new(format!("{cicon} {label}  {dist}"))
                                .color(egui::Color32::from_rgb(0x8e, 0xd6, 0xe6))
                                .strong(),
                            egui::Color32::from_rgb(0x10, 0x32, 0x3a),
                        )
                        .on_hover_text(format!("Death {dist} from {cname}"));
                    }
                }

                if let Some(n) = r.count {
                    // Render like the system chips (a Button), not a raw Frame. A Frame sizes its
                    // fill to content height, which floats taller than the button-height siblings;
                    // a Button is bounded to interact_size.y so it matches every other chip.
                    ui.add(
                        egui::Button::new(
                            egui::RichText::new(format!("{} {n}", icon::USERS))
                                .color(egui::Color32::WHITE)
                                .strong(),
                        )
                        .fill(red)
                        .sense(egui::Sense::hover()),
                    )
                    .on_hover_text("hostiles");
                }

                if let Some(isk) = r.isk.filter(|_| !is_zkill) {
                    chip(
                        ui,
                        egui::RichText::new(format!("{} {}", icon::COINS, crate::intel::format_isk(isk)))
                            .color(egui::Color32::from_rgb(0xff, 0xd9, 0x6b))
                            .strong(),
                        egui::Color32::from_rgb(0x4a, 0x3d, 0x10),
                    )
                    .on_hover_text("ISK posted");
                }

                for (name, dist) in &r.structures {
                    let text = match dist {
                        Some(d) => format!("{name}  {d}"),
                        None => name.clone(),
                    };
                    let col = egui::Color32::from_rgb(0xc4, 0xb5, 0xfd);
                    if let Some(tid) = crate::intel::structure_type_id(name) {
                        let url = eve_type_render_url(tid, badge_isz);
                        let img = egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(badge_isz));
                        ui.add(egui::Button::image_and_text(img, egui::RichText::new(text).color(col).strong()))
                            .on_hover_text("Structure");
                        continue;
                    }
                    chip(
                        ui,
                        egui::RichText::new(format!("{} {text}", icon::CASTLE_TURRET)).color(col).strong(),
                        egui::Color32::from_rgb(0x2e, 0x24, 0x4a),
                    )
                    .on_hover_text(match dist {
                        Some(d) => format!("{name}, {d} off"),
                        None => name.clone(),
                    });
                }

                for cel in &r.celestials {
                    let cicon = if cel.starts_with("Moon") {
                        icon::MOON
                    } else if cel.starts_with("Sun") {
                        icon::SUN
                    } else if cel.ends_with("Belt") {
                        icon::GRAINS
                    } else {
                        icon::PLANET
                    };
                    chip(
                        ui,
                        egui::RichText::new(format!("{cicon} {cel}"))
                            .color(egui::Color32::from_rgb(0x8e, 0xd6, 0xe6))
                            .strong(),
                        egui::Color32::from_rgb(0x10, 0x32, 0x3a),
                    )
                    .on_hover_text(format!("{cel} (celestial)"));
                }

                if let Some(probes) = r.probes {
                    chip(
                        ui,
                        egui::RichText::new(format!("{} {probes}", icon::MAGNIFYING_GLASS))
                            .color(egui::Color32::from_rgb(0x7d, 0xd3, 0xde))
                            .strong(),
                        egui::Color32::from_rgb(0x10, 0x3a, 0x40),
                    )
                    .on_hover_text("Scanning probes on D-Scan (someone is scanning)");
                }

                let nothing_else = r.count.is_none()
                    && r.isk.is_none()
                    && r.pilots.is_empty()
                    && r.ships.is_empty()
                    && r.classes.is_empty()
                    && r.gates.is_empty()
                    && r.structures.is_empty()
                    && r.celestials.is_empty()
                    && r.probes.is_none()
                    && r.tackled_targets.is_empty()
                    && r.alliances.is_empty()
                    && r.links.is_empty()
                    && !r.clear
                    && !r.no_visual
                    && !r.spike
                    && !r.camp
                    && !r.help
                    && !r.bubble
                    && !r.nullified
                    && !r.killmail
                    && !r.cyno
                    && !r.dropper
                    && !r.cap_tackled
                    && !r.tackled
                    && !r.wormhole
                    && !r.ess
                    && !r.skyhook
                    && !r.status
                    && !r.diamond_rats
                    && r.anom_sigs.is_empty()
                    && r.movement.is_none();
                if nothing_else && !r.systems.is_empty() {
                    let mut residual = r.text.to_lowercase();
                    for s in &r.systems {
                        residual = residual.replace(&s.name.to_lowercase(), " ");
                    }
                    if residual.chars().any(|c| c.is_alphanumeric()) {
                        ui.add(egui::Label::new(egui::RichText::new(r.text.trim()).weak()).wrap());
                    }
                }

                let ship_panel = |ui: &mut egui::Ui,
                                  sh: &crate::intel::DetectedShip,
                                  tip: &mut Option<(egui::Pos2, PendingTip)>|
                 -> Option<IntelClick> {
                    let url = if crate::intel::structure_name_by_type(sh.id).is_some() {
                        eve_type_render_url(sh.id, badge_isz)
                    } else {
                        eve_type_icon_url(sh.id, badge_isz)
                    };
                    let img = egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(badge_isz));
                    let mut panel =
                        ui.add(egui::Button::image_and_text(img, egui::RichText::new(&sh.name).strong()));
                    if let Some(d) = ship_details.get(&sh.id) {
                        let roles = ship_roles.get(&sh.id).map(|v| v.as_slice()).unwrap_or(&[]);
                        if compact {
                            if panel.hovered() {
                                *tip = Some((
                                    panel.rect.right_top(),
                                    PendingTip::Ship(d.clone(), roles.to_vec()),
                                ));
                            }
                        } else {
                            panel = panel.on_hover_ui(|ui| ship_hover(ui, d, roles));
                        }
                    }
                    panel.clicked().then_some(IntelClick::Ship(sh.id))
                };
                if !is_zkill {
                    for sh in &r.ships {
                        if let Some(c) = ship_panel(ui, sh, tip) {
                            clicked = Some(c);
                        }
                    }
                    for amb in &r.ambiguous_ships {
                        let amber = crate::theme::standing::WARNING;
                        let names = amb
                            .candidates
                            .iter()
                            .map(|(_, n)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(" or ");
                        let btn = egui::Button::new(
                            egui::RichText::new(format!("{}?", amb.abbrev)).color(amber).strong(),
                        )
                        .stroke(egui::Stroke::new(1.0, amber));
                        let (resp, _) = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                            ui.label(egui::RichText::new("Ambiguous abbreviation, could be:").weak());
                            for (id, name) in &amb.candidates {
                                if *id == 0 {
                                    ui.label(name);
                                    continue;
                                }
                                let img = egui::Image::new(eve_type_icon_url(*id, 20.0))
                                    .fit_to_exact_size(egui::Vec2::splat(20.0));
                                if ui
                                    .add(egui::Button::image_and_text(img, egui::RichText::new(name)))
                                    .clicked()
                                {
                                    clicked = Some(IntelClick::Ship(*id));
                                    ui.close();
                                }
                            }
                        });
                        resp.on_hover_text(format!("Could be: {names}"));
                    }
                }

                for class in r.classes.iter().filter(|c| interesting_ship_class(c)) {
                    ui.add(egui::Button::new(egui::RichText::new(class).italics()))
                        .on_hover_text("Ship class, no exact hull reported");
                }

                let tackled_badge = |ui: &mut egui::Ui, label: String| {
                    chip(
                        ui,
                        egui::RichText::new(label)
                            .strong()
                            .color(egui::Color32::from_rgb(0xff, 0x8a, 0x8a)),
                        egui::Color32::from_rgb(0x5a, 0x18, 0x18),
                    );
                };
                for target in &r.tackled_targets {
                    tackled_badge(ui, format!("{target}  TACKLED"));
                }
                if r.tackled && r.tackled_targets.is_empty() && !r.cap_tackled {
                    tackled_badge(ui, "TACKLED".to_string());
                }

                for name in &r.pilots {
                    if crate::intel::is_pilot_stopword(name) {
                        continue;
                    }
                    if !resolved_pilots.contains_key(name) {
                        continue;
                    }
                    let char_id = resolved_pilots.get(name).copied();
                    let aff = char_id.and_then(|cid| {
                        let mut c = affil.lock().unwrap();
                        c.want(cid);
                        c.get(cid)
                    });
                    let is_uncertain = uncertain.contains(&name.to_lowercase());
                    let amber = egui::Color32::from_rgb(0xfb, 0xbf, 0x24);
                    let sz = egui::Vec2::splat(pilot_isz);
                    let img = |url: String| egui::Image::new(url).fit_to_exact_size(sz);
                    let resp = if let Some(cid) = char_id {
                        let mut atoms = egui::Atoms::new(img(eve_portrait_url(cid, pilot_isz)));
                        if let Some(co) = aff.as_ref().and_then(|a| a.corp) {
                            atoms.push_left(img(eve_corp_logo_url(co, pilot_isz)));
                        }
                        if let Some(al) = aff.as_ref().and_then(|a| a.alliance) {
                            atoms.push_left(img(eve_alliance_logo_url(al, pilot_isz)));
                        }
                        atoms.push_right(egui::RichText::new(name));
                        if is_uncertain {
                            atoms.push_right(egui::RichText::new("?").color(amber).strong());
                        }
                        let mut btn = egui::Button::new(atoms);
                        if is_uncertain {
                            btn = btn.fill(egui::Color32::from_rgb(0x3d, 0x30, 0x14));
                        }
                        ui.add(btn)
                    } else {
                        ui.add(egui::Button::new(egui::RichText::new(format!("{} {name}", icon::USER))))
                    };
                    let corp_id = aff.as_ref().and_then(|a| a.corp);
                    let alliance_id = aff.as_ref().and_then(|a| a.alliance);
                    let corp_name = aff.as_ref().and_then(|a| a.corp_name.clone());
                    let alliance_name = aff.as_ref().and_then(|a| a.alliance_name.clone());
                    let hint = if is_uncertain {
                        "Looks inactive - click to mark real or hide"
                    } else {
                        "Click to look up"
                    };
                    let resp = if compact {
                        if resp.hovered() {
                            *tip = Some((
                                resp.rect.right_top(),
                                PendingTip::Identity {
                                    alliance: alliance_id,
                                    alliance_name: alliance_name.clone(),
                                    corp: corp_id,
                                    corp_name: corp_name.clone(),
                                    char_id,
                                    char_name: Some(name.to_string()),
                                    note: Some(hint.to_string()),
                                },
                            ));
                        }
                        resp
                    } else {
                        resp.on_hover_ui(|ui| {
                            tooltip_identity(
                                ui,
                                alliance_id,
                                alliance_name.clone(),
                                corp_id,
                                corp_name.clone(),
                                char_id,
                                Some(name.to_string()),
                            );
                            ui.label(egui::RichText::new(hint).weak());
                        })
                    };
                    if resp.clicked() {
                        clicked = Some(if is_uncertain {
                            IntelClick::PilotVerdict(name.clone())
                        } else {
                            IntelClick::Pilot(name.clone())
                        });
                    }
                }

                let resolving = r.pilots.iter().any(|name| {
                    !crate::intel::is_pilot_stopword(name) && !resolved_pilots.contains_key(name)
                });
                if resolving {
                    let phase = (now as f64 * 2.0) as usize % 3 + 1;
                    let dots = ".".repeat(phase);
                    ui.add_enabled(
                        false,
                        egui::Button::new(egui::RichText::new(format!("{} {dots}", icon::USER)).weak()),
                    )
                    .on_hover_text("Resolving pilot…");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(450));
                }

                if r.ships.is_empty() {
                    let seen: Vec<(i64, String)> = r
                        .pilots
                        .iter()
                        .filter_map(|name| last_ship.get(&name.to_lowercase()))
                        .filter(|(_, _, t)| now - t <= 3600)
                        .map(|(id, ship, _)| (*id, ship.clone()))
                        .collect();
                    if !seen.is_empty() {
                        let row_h = ui.spacing().interact_size.y;
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        let w = ui
                            .painter()
                            .layout_no_wrap("Last seen as:".to_owned(), font, egui::Color32::PLACEHOLDER)
                            .size()
                            .x;
                        ui.add_sized(
                            [w, row_h],
                            egui::Label::new(egui::RichText::new("Last seen as:").weak())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        for (id, ship) in seen {
                            let url = eve_type_icon_url(id, badge_isz);
                            let img = egui::Image::new(url)
                                .fit_to_exact_size(egui::Vec2::splat(badge_isz));
                            let mut panel = ui.add(egui::Button::image_and_text(
                                img,
                                egui::RichText::new(&ship).strong(),
                            ));
                            if let Some(d) = ship_details.get(&id) {
                                let roles =
                                    ship_roles.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
                                if compact {
                                    if panel.hovered() {
                                        *tip = Some((
                                            panel.rect.right_top(),
                                            PendingTip::Ship(d.clone(), roles.to_vec()),
                                        ));
                                    }
                                } else {
                                    panel = panel.on_hover_ui(|ui| ship_hover(ui, d, roles));
                                }
                            }
                            if panel.clicked() {
                                clicked = Some(IntelClick::Ship(id));
                            }
                        }
                    }
                }

                for g in &r.gates {
                    let label = if g.is_empty() {
                        format!("{} gate", icon::SIGN_IN)
                    } else {
                        format!("{} {g} gate", icon::SIGN_IN)
                    };
                    ui.add(
                        egui::Button::new(egui::RichText::new(label).color(accent).strong())
                            .sense(egui::Sense::hover()),
                    );
                }

                for (name, id) in &r.alliances {
                    let url = eve_alliance_logo_url(id, pilot_isz);
                    ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(pilot_isz)))
                        .on_hover_text(name);
                }

                for link in &r.links {
                    use crate::intel::LinkKind;
                    match link.kind {
                        LinkKind::Killmail => {
                            let info = link
                                .kill_id
                                .and_then(|id| kills.lock().unwrap().get(&id).cloned().flatten());
                            {
                                if let Some(inf) = &info {
                                    let sz = egui::Vec2::splat(pilot_isz);
                                    let img = |url: String| {
                                        egui::Image::new(url).fit_to_exact_size(sz)
                                    };
                                    let badge = |ui: &mut egui::Ui,
                                                 alliance: Option<i64>,
                                                 corp: Option<i64>,
                                                 character: Option<i64>,
                                                 title: &str,
                                                 tip: &mut Option<(egui::Pos2, PendingTip)>| {
                                        let parts: Vec<String> = [
                                            alliance.map(|a| eve_alliance_logo_url(a, pilot_isz)),
                                            corp.map(|c| eve_corp_logo_url(c, pilot_isz)),
                                            character.map(|c| eve_portrait_url(c, pilot_isz)),
                                        ]
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                        let Some(first) = parts.first() else { return };
                                        let mut atoms = egui::Atoms::new(img(first.clone()));
                                        for url in parts.iter().skip(1) {
                                            atoms.push_right(img(url.clone()));
                                        }
                                        let zkill = character
                                            .map(|c| format!("https://zkillboard.com/character/{c}/"))
                                            .or_else(|| corp.map(|c| format!("https://zkillboard.com/corporation/{c}/")))
                                            .or_else(|| alliance.map(|a| format!("https://zkillboard.com/alliance/{a}/")));
                                        let resp = ui.add(egui::Button::new(atoms));
                                        if compact {
                                            if resp.hovered() {
                                                let info = character.and_then(|c| {
                                                    let mut a = affil.lock().unwrap();
                                                    a.want(c);
                                                    a.get(c)
                                                });
                                                *tip = Some((
                                                    resp.rect.right_top(),
                                                    PendingTip::Identity {
                                                        alliance: info
                                                            .as_ref()
                                                            .and_then(|i| i.alliance)
                                                            .or(alliance),
                                                        alliance_name: info
                                                            .as_ref()
                                                            .and_then(|i| i.alliance_name.clone()),
                                                        corp: info.as_ref().and_then(|i| i.corp).or(corp),
                                                        corp_name: info
                                                            .as_ref()
                                                            .and_then(|i| i.corp_name.clone()),
                                                        char_id: character,
                                                        char_name: info
                                                            .as_ref()
                                                            .and_then(|i| i.char_name.clone()),
                                                        note: Some(title.to_string()),
                                                    },
                                                ));
                                            }
                                        } else {
                                            resp.clone().on_hover_ui(|ui| {
                                                ui.strong(title);
                                                let info = character.and_then(|c| {
                                                    let mut a = affil.lock().unwrap();
                                                    a.want(c);
                                                    a.get(c)
                                                });
                                                tooltip_identity(
                                                    ui,
                                                    info.as_ref().and_then(|i| i.alliance).or(alliance),
                                                    info.as_ref().and_then(|i| i.alliance_name.clone()),
                                                    info.as_ref().and_then(|i| i.corp).or(corp),
                                                    info.as_ref().and_then(|i| i.corp_name.clone()),
                                                    character,
                                                    info.as_ref().and_then(|i| i.char_name.clone()),
                                                );
                                            });
                                        }
                                        if resp.clicked() {
                                            if let Some(url) = zkill {
                                                let _ = open::that(url);
                                            }
                                        }
                                    };
                                    let fb_alliance =
                                        inf.final_blow_alliance.or_else(|| inf.attacker_alliances.first().copied());
                                    if inf.final_blow_char.is_some()
                                        || inf.final_blow_corp.is_some()
                                        || fb_alliance.is_some()
                                    {
                                        badge(
                                            ui,
                                            fb_alliance,
                                            inf.final_blow_corp,
                                            inf.final_blow_char,
                                            "Attacker (final blow). Click for zKill.",
                                            tip,
                                        );
                                        if inf.attacker_count > 0 {
                                            let (tag, hover) = if inf.attacker_count == 1 {
                                                ("S".to_owned(), "Solo kill".to_owned())
                                            } else {
                                                (
                                                    format!("+{}", inf.attacker_count - 1),
                                                    format!("{} attackers", inf.attacker_count),
                                                )
                                            };
                                            ui.label(
                                                egui::RichText::new(tag)
                                                    .color(egui::Color32::from_rgb(0xfb, 0xbf, 0x24))
                                                    .strong(),
                                            )
                                            .on_hover_text(hover);
                                        }
                                        ui.label(
                                            egui::RichText::new(icon::CARET_RIGHT).color(red).strong(),
                                        );
                                    }
                                    badge(
                                        ui,
                                        inf.victim_alliance,
                                        inf.victim_corp,
                                        inf.victim_char,
                                        "Victim. Click for zKill.",
                                        tip,
                                    );
                                }
                                for sh in &r.ships {
                                    if let Some(c) = ship_panel(ui, sh, tip) {
                                        clicked = Some(c);
                                    }
                                }
                                let lbl = egui::RichText::new(format!("{} zKill", icon::ARROW_SQUARE_OUT))
                                    .color(red)
                                    .strong();
                                if ui.add(egui::Button::new(lbl)).clicked() {
                                    let _ = open::that(&link.url);
                                    consumed = true;
                                }
                                if let Some(inf) = &info {
                                    if inf.value > 0.0 {
                                        ui.label(egui::RichText::new(fmt_isk(inf.value)).weak());
                                    }
                                }
                            }
                        }
                        LinkKind::BattleReport => {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(format!("{} BR", icon::CHART_LINE))
                                        .color(accent)
                                        .strong(),
                                ))
                                .on_hover_text(&link.url)
                                .clicked()
                            {
                                let _ = open::that(&link.url);
                                consumed = true;
                            }
                        }
                        LinkKind::Dscan => {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(format!("{} dscan", icon::SCAN)).color(accent),
                                ))
                                .on_hover_text(&link.url)
                                .clicked()
                            {
                                clicked = Some(IntelClick::Dscan(link.url.clone()));
                            }
                        }
                    }
                }

                let tag = |ui: &mut egui::Ui, txt: &str, col: egui::Color32| {
                    ui.add(
                        egui::Button::new(egui::RichText::new(txt).color(col).strong())
                            .sense(egui::Sense::hover()),
                    );
                };
                if r.status {
                    tag(ui, "STATUS?", egui::Color32::from_rgb(0x7d, 0xd3, 0xde));
                }
                if r.clear {
                    tag(ui, "CLEAR", green);
                }
                if r.no_visual {
                    tag(ui, "NV", warn);
                }
                if r.spike {
                    tag(ui, "SPIKE", red);
                }
                if r.camp {
                    tag(ui, "CAMP", red);
                }
                if r.help {
                    tag(ui, "HELP", red);
                }
                if r.bubble {
                    tag(ui, "BUBBLE", warn);
                }
                if r.nullified {
                    tag(ui, "NULLIFIED", warn);
                }
                if r.killmail && !is_zkill {
                    tag(ui, "KILL", red);
                }
                if r.cyno {
                    tag(ui, "CYNO", red);
                }
                if r.dropper {
                    tag(ui, "DROPPER", red);
                }
                if r.cap_tackled {
                    tag(ui, "CAP TACKLED", red);
                }
                if r.wormhole {
                    tag(ui, &wormhole_badge_label(r), crate::theme::standing::ALLIANCE);
                }
                if r.ess {
                    match &r.ess_time {
                        Some(t) => tag(ui, &format!("ESS {t}"), warn),
                        None => tag(ui, "ESS", warn),
                    }
                }
                if r.filament {
                    tag(ui, "FILAMENT", warn);
                }
                if r.diamond_rats {
                    tag(ui, "\u{25C6} Rats \u{25C6}", red);
                }
                for (kind, code) in &r.anom_sigs {
                    tag(ui, &anom_sig_badge_label(*kind, code), warn);
                }

                if let Some(m) = &r.movement {
                    let hint = match m.jumps {
                        Some(j) => format!("{} {} ({j}j)", icon::ARROW_LEFT, m.from),
                        None => format!("{} {}", icon::ARROW_LEFT, m.from),
                    };
                    ui.label(egui::RichText::new(hint).italics().weak());
                }
                if stale {
                    ui.label(egui::RichText::new("outdated").italics().weak());
                }
            };
            ui.horizontal_wrapped(|ui| {
                render(ui);
                if show_reporter && !is_zkill {
                    ui.label(
                        egui::RichText::new(if r.reporter.eq_ignore_ascii_case(&r.channel) {
                            format!("·  {}", r.reporter)
                        } else {
                            format!("·  {} · {}", r.reporter, r.channel)
                        })
                        .weak(),
                    );
                }
            });
        })
        .response;

    let bg_click = clicked.is_none()
        && !consumed
        && !is_zkill
        && ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|p| resp.rect.contains(p))
        });
    if bg_click {
        ui.ctx().data_mut(|d| d.insert_temp(toggle_id, !show_raw));
    }
    clicked
}

fn tooltip_identity(
    ui: &mut egui::Ui,
    alliance: Option<i64>,
    alliance_name: Option<String>,
    corp: Option<i64>,
    corp_name: Option<String>,
    char_id: Option<i64>,
    char_name: Option<String>,
) {
    let sz = egui::Vec2::splat(22.0);
    let logo_row = |ui: &mut egui::Ui, url: String, name: Option<String>| {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(url).fit_to_exact_size(sz));
            ui.label(name.unwrap_or_else(|| "…".to_owned()));
        });
    };
    if let Some(a) = alliance {
        logo_row(ui, eve_alliance_logo_url(a, 22.0), alliance_name);
    }
    if let Some(c) = corp {
        logo_row(ui, eve_corp_logo_url(c, 22.0), corp_name);
    }
    if let Some(ch) = char_id {
        logo_row(ui, eve_portrait_url(ch, 22.0), char_name);
    } else if let Some(n) = char_name {
        ui.horizontal(|ui| {
            ui.label(egui_phosphor::regular::USER);
            ui.label(n);
        });
    }
}

fn ship_hover(ui: &mut egui::Ui, d: &crate::store::ShipDetails, roles: &[(&'static str, &'static str)]) {
    ui.label(egui::RichText::new(&d.name).strong());
    ui.label(egui::RichText::new(&d.group).weak());
    role_badges(ui, roles);
    ui.separator();
    ship_stats(ui, d);
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

fn fit_url(site: &str, _ship_id: i64, loss: &crate::lookup::Loss) -> String {
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

fn derive_roles(traits: &[(i64, f64, String)]) -> Vec<(&'static str, &'static str)> {
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

fn layer_ehp(hp: f64, r: [u32; 4]) -> f64 {
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
) {
    ui.horizontal(|ui| {
        ui.label(security_badge(s.security));
        ui.label(egui::RichText::new(&s.name).strong());
    });
    system_chips(ui, systems, status, s.id);
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

fn name_color(name: &str) -> egui::Color32 {
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

fn security_color(security: f64) -> egui::Color32 {
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

#[cfg(test)]
mod wh_badge_tests {
    use super::wormhole_badge_label;
    use crate::intel::IntelReport;
    use crate::wormholes::DestClass;

    fn wh(f: impl FnOnce(&mut IntelReport)) -> IntelReport {
        let mut ir = IntelReport { wormhole: true, ..Default::default() };
        f(&mut ir);
        ir
    }

    #[test]
    fn wormhole_badge_composition() {
        let icon = egui_phosphor::regular::SPIRAL;
        assert_eq!(wormhole_badge_label(&wh(|_| {})), icon.to_string());
        let l = wormhole_badge_label(&wh(|ir| {
            ir.wh_sig = Some("ABC-123".into());
            ir.wh_type = Some("S899".into());
            ir.wh_size = Some(crate::wormholes::ShipSize::XLarge);
            ir.wh_drifter = true;
            ir.wh_dest = Some(DestClass::Highsec);
        }));
        assert!(l.starts_with(icon), "{l}");
        for want in ["ABC-123", "S899", "XL", "Drifter", "Highsec"] {
            assert!(l.contains(want), "missing {want} in {l:?}");
        }
        // The card must not contain literal parentheses (they only marked optionality).
        assert!(!l.contains('(') && !l.contains(')'), "unexpected parens in {l:?}");
        assert!(!wormhole_badge_label(&wh(|ir| ir.wh_type = Some("K162".into()))).contains("K162"));
        assert!(wormhole_badge_label(&wh(|ir| ir.wh_dest = Some(DestClass::Thera))).contains("Thera"));
        assert!(wormhole_badge_label(&wh(|ir| ir.wh_dest = Some(DestClass::Turnur))).contains("Turnur"));
        // A frigate-size hole reads "Small", not "Frig".
        let frig = wormhole_badge_label(&wh(|ir| ir.wh_size = Some(crate::wormholes::ShipSize::Frigate)));
        assert!(frig.contains("Small") && !frig.contains("Frig"), "{frig:?}");
    }

    #[test]
    fn only_specific_ship_classes_badge() {
        use super::interesting_ship_class;
        // Generic hull tiers are noise.
        for generic in ["Frigate", "Destroyer", "Cruiser", "Battlecruiser", "Battleship"] {
            assert!(!interesting_ship_class(generic), "{generic} should be hidden");
        }
        // T2/T3 specialisations and capitals are shown.
        for keep in ["Interdictor", "Heavy Interdictor", "Black Ops", "Strategic Cruiser", "Dreadnought", "Titan"] {
            assert!(interesting_ship_class(keep), "{keep} should show");
        }
    }

    #[test]
    fn anom_sig_badge_composition() {
        use super::anom_sig_badge_label;
        use crate::intel::AnomKind;
        let icon = egui_phosphor::regular::CROSSHAIR;
        assert_eq!(anom_sig_badge_label(AnomKind::Anomaly, ""), format!("{icon} Anom"));
        assert_eq!(anom_sig_badge_label(AnomKind::Signature, ""), format!("{icon} Sig"));
        assert_eq!(anom_sig_badge_label(AnomKind::Signature, "ABC-123"), format!("{icon} Sig ABC-123"));
    }
}

#[cfg(test)]
mod wh_overlay_tests {
    use super::*;
    use crate::wormholes::{DestClass, Source, Wormhole};

    fn conn(sys: i64, dest_sys: i64) -> Wormhole {
        Wormhole {
            id: 0,
            system_id: sys,
            signature: None,
            wh_type: None,
            dest: if is_jspace(dest_sys) { DestClass::Wspace } else { DestClass::Nullsec },
            dest_system_id: Some(dest_sys),
            dest_signature: None,
            dest_wh_type: None,
            size: None,
            is_drifter: false,
            reported_at: 0,
            explicit_expiry: None,
            source: Source::Intel,
            updated_at: 0,
        }
    }

    #[test]
    fn chains_through_jspace_and_direct_links() {
        let whs = vec![
            conn(30_000_001, 31_000_001),
            conn(31_000_001, 30_000_002),
            conn(30_000_001, 30_000_003),
        ];
        let o = WhOverlay::build(&whs);
        assert!(o.direct.contains(&(30_000_001, 30_000_003)), "direct: {:?}", o.direct);
        assert!(
            o.chains.iter().any(|&(a, b, h)| (a, b) == (30_000_001, 30_000_002) && h == 1),
            "chains: {:?}",
            o.chains
        );
        assert!(o.jspace_holes.contains(&30_000_001));
        assert!(!o.direct.iter().any(|&(a, b)| is_jspace(a) || is_jspace(b)));
    }
}

#[cfg(test)]
mod kill_noise_tests {
    use super::*;

    #[test]
    fn deployables_and_unknowns_are_noise() {
        assert!(kill_is_noise("", 5_000_000.0));
        assert!(kill_is_noise("Mobile Tractor Unit", 1_000_000.0));
        assert!(kill_is_noise("Mobile Depot", 500_000.0));
        assert!(kill_is_noise("Shuttle", 10_000.0));
        assert!(kill_is_noise("Reaper", 1000.0));
        assert!(kill_is_noise("Capsule", 100_000.0));
        assert!(!kill_is_noise("Stabber", 20_000_000.0));
        assert!(!kill_is_noise("Keepstar", 1e12));
        assert!(!kill_is_noise("Capsule", 500_000_000.0));
    }
}

#[cfg(test)]
mod op_channel_tests {
    use super::*;

    #[test]
    fn op_key_canonicalizes_variants() {
        assert_eq!(op_key("Op 4").as_deref(), Some("op4"));
        assert_eq!(op_key("OP4").as_deref(), Some("op4"));
        assert_eq!(op_key("op 4 - dead keepstars").as_deref(), Some("op4"));
        assert_eq!(op_key("get to OP 9 now").as_deref(), Some("op9"));
        assert_eq!(op_key("stop shooting"), None);
        assert_eq!(op_key("no channel here"), None);
    }
}

#[cfg(test)]
mod activity_label_tests {
    use super::compact_count;

    #[test]
    fn small_counts_stay_exact() {
        assert_eq!(compact_count(0), "0");
        assert_eq!(compact_count(7), "7");
        assert_eq!(compact_count(99), "99");
    }

    #[test]
    fn three_digits_and_up_abbreviate() {
        assert_eq!(compact_count(100), "0.1k");
        assert_eq!(compact_count(234), "0.2k");
        assert_eq!(compact_count(1_234), "1.2k");
        assert_eq!(compact_count(23_400), "23.4k");
    }
}

#[cfg(test)]
mod sov_art_tests {
    use super::*;

    fn img(px: &[egui::Color32]) -> egui::ColorImage {
        egui::ColorImage::new([px.len(), 1], px.to_vec())
    }

    #[test]
    fn mean_ignores_transparent_padding() {
        let clear = egui::Color32::from_rgba_unmultiplied(0xFF, 0x00, 0x00, 0x00);
        let blue = egui::Color32::from_rgb(0x00, 0x00, 0xC0);
        // The red is fully transparent logo padding, so only the blue counts.
        let c = mean_logo_color(&img(&[clear, blue, clear])).unwrap();
        assert_eq!((c.r(), c.g()), (0, 0));
        assert!(c.b() > 0xA0, "b={}", c.b());
    }

    #[test]
    fn mean_lifts_a_dark_logo_into_view() {
        let dark = egui::Color32::from_rgb(0x10, 0x10, 0x20);
        let c = mean_logo_color(&img(&[dark])).unwrap();
        assert!(c.b() >= 100, "a near-black logo must not yield a near-black dot: {c:?}");
    }

    /// Two logos whose averages are both muddy but differently tinted must not collapse to the same
    /// grey, or every alliance looks alike on the map.
    #[test]
    fn a_washed_out_mean_comes_back_saturated() {
        let muddy_red = mean_logo_color(&img(&[egui::Color32::from_rgb(0x70, 0x5A, 0x5A)])).unwrap();
        let muddy_blue = mean_logo_color(&img(&[egui::Color32::from_rgb(0x5A, 0x5A, 0x70)])).unwrap();

        let spread = |c: egui::Color32| {
            let (r, g, b) = (c.r() as i32, c.g() as i32, c.b() as i32);
            r.max(g).max(b) - r.min(g).min(b)
        };
        assert!(spread(muddy_red) > 40, "still grey: {muddy_red:?}");
        assert!(spread(muddy_blue) > 40, "still grey: {muddy_blue:?}");
        // Hue is preserved: the reddish one stays reddish, the bluish one bluish.
        assert!(muddy_red.r() > muddy_red.b());
        assert!(muddy_blue.b() > muddy_blue.r());
    }

    #[test]
    fn a_genuinely_grey_logo_is_left_grey() {
        // No hue to recover, so boosting must not invent one.
        let c = mean_logo_color(&img(&[egui::Color32::from_rgb(0x80, 0x80, 0x80)])).unwrap();
        assert_eq!((c.r(), c.g()), (c.b(), c.b()));
    }

    #[test]
    fn a_fully_transparent_logo_has_no_mean() {
        let clear = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0);
        assert!(mean_logo_color(&img(&[clear])).is_none());
    }

    #[test]
    fn npc_factions_resolve_to_a_logo_corp() {
        use crate::factions::corporation_id;
        assert_eq!(corporation_id(500_010), Some(1_000_127)); // Guristas, holds Venal
        assert_eq!(corporation_id(500_019), Some(1_000_162)); // Sansha, holds Stain
        assert_eq!(corporation_id(1234), None);
    }
}

#[cfg(test)]
mod wh_route_tests {
    use super::*;
    use crate::geo::{SystemInfo, Systems, ZARZAKH};
    use std::collections::HashMap;

    const THERA: i64 = 31_000_005;

    /// Two gate islands, 6 gates apart the long way, with Thera reachable by a hole from each.
    fn systems() -> Systems {
        let mk = |id: i64| SystemInfo {
            id,
            name: format!("S{id}"),
            security: 0.0,
            constellation: String::new(),
            region: String::new(),
            faction: String::new(),
        };
        let ids = [1, 2, 3, 4, 5, 6, 7, THERA, ZARZAKH];
        let by_name: HashMap<String, SystemInfo> =
            ids.into_iter().map(|id| (format!("s{id}"), mk(id))).collect();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in [(1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)] {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        Systems::new(by_name, adj)
    }

    fn holes(edges: &[(i64, i64)]) -> HashMap<i64, Vec<i64>> {
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(a, b) in edges {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        adj
    }

    #[test]
    fn both_sides_of_a_hole_get_a_waypoint() {
        let g = systems();
        // 1 -gate- 2 =hole= Thera =hole= 6 -gate- 7. The client cannot route through the hole, so it
        // needs 2 (fly here, jump) and 6 (resume here), and Thera itself can hold no waypoint.
        let wp = wh_route_waypoints(&g, &holes(&[(2, THERA), (6, THERA)]), 1, 7).unwrap();
        assert_eq!(wp, vec![2, 6, 7]);
    }

    #[test]
    fn the_system_we_are_in_is_not_a_waypoint() {
        let g = systems();
        // Standing on the hole already: nothing to fly to, just jump and carry on.
        let wp = wh_route_waypoints(&g, &holes(&[(1, THERA), (6, THERA)]), 1, 7).unwrap();
        assert_eq!(wp, vec![6, 7]);
    }

    #[test]
    fn map_route_runs_through_the_hole() {
        let g = systems();
        let h = holes(&[(1, THERA), (7, THERA)]);
        let route = g.route_with(1, 7, true, true, &h, |_| true).unwrap();
        assert_eq!(route, vec![1, THERA, 7]);
        assert!(g.is_hole_step(1, THERA) && g.is_hole_step(THERA, 7));
        assert!(!g.is_hole_step(1, 2));
        // Without the holes the same trip is all six gates.
        assert_eq!(g.route(1, 7, true, true, |_| true).unwrap().len() - 1, 6);
    }

    #[test]
    fn a_thera_hub_with_many_holes_still_routes() {
        let g = systems();
        // The map overlay drops a j-space hub above degree 6. Routing must not.
        let h = holes(&[
            (2, THERA),
            (3, THERA),
            (4, THERA),
            (5, THERA),
            (6, THERA),
            (7, THERA),
        ]);
        assert_eq!(wh_route_waypoints(&g, &h, 1, 7).unwrap(), vec![2, 7]);
    }

    #[test]
    fn gates_win_when_the_hole_is_no_shortcut() {
        let g = systems();
        // 1 -> 2 -> 3 is pure gates, so no waypoints beyond the destination.
        assert_eq!(wh_route_waypoints(&g, &holes(&[(1, THERA), (5, THERA)]), 1, 3).unwrap(), vec![3]);
    }

    #[test]
    fn no_route_without_a_connecting_hole() {
        let g = systems();
        assert_eq!(wh_route_waypoints(&g, &holes(&[(1, THERA)]), 1, 99), None);
    }

    #[test]
    fn zarzakh_is_not_a_shortcut() {
        let g = systems();
        // A hole into Zarzakh does not let a route continue out of its far gate.
        let h = holes(&[(1, ZARZAKH), (ZARZAKH, 7)]);
        assert_eq!(wh_route_waypoints(&g, &h, 1, 7).unwrap(), vec![7]);
        // Zarzakh as the destination is still fine.
        assert_eq!(wh_route_waypoints(&g, &h, 1, ZARZAKH).unwrap(), vec![ZARZAKH]);
        assert_eq!(g.route_with(1, 7, true, true, &h, |_| true).unwrap().len() - 1, 6);
    }
}
