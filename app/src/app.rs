//! The application shell: window, nav rail, top/status bars, settings dialog,
//! theme application, and persistence wiring (docs/DESIGN.md §6).

/// Intel feed type filter.
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum IntelTypeFilter {
    All,
    Hostile,
    Clear,
    Kill,
    Threat,
}

/// Sovereignty territory colouring mode.
#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum SovMode {
    Off,
    Alliance,
    Coalition,
}

/// Which ESI activity metric the heat overlay shows.
#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum ActivityMode {
    Off,
    ShipKills,
    PodKills,
    NpcKills,
    Jumps,
}

impl ActivityMode {
    fn value(self, f: &crate::systemstatus::SysFlags) -> u32 {
        match self {
            ActivityMode::Off => 0,
            ActivityMode::ShipKills => f.ship_kills,
            ActivityMode::PodKills => f.pod_kills,
            ActivityMode::NpcKills => f.npc_kills,
            ActivityMode::Jumps => f.jumps,
        }
    }
    /// Approximate "busy" value for scaling the heat colour.
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

/// A system suggestion row: (id, name, security, constellation, region).
type SysHit = (i64, String, f64, String, String);

/// Toggleable map overlays (the top-right Layers menu).
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
    /// Gate-camp markers (red campfire icon) from the live kill feed.
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

/// Map mode. Standard is today's intel map; the others (planned in docs/MAP_MODES.md) add a
/// focused panel and auto-adapt the overlays to surface only what that mode needs.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum MapMode {
    #[default]
    Standard,
    Travel,
    Hunting,
    Safety,
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
    /// Overlay preset for a non-Standard mode: hide the sov/connection clutter, surface
    /// ship-kills (danger), keep jump bridges for the routing-oriented modes.
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
        }
    }
}

/// Map + intel-filter options persisted between sessions (as a JSON blob in
/// settings, so settings.rs needn't know these app types).
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

/// A click on an intel card panel.
#[derive(Clone)]
enum IntelClick {
    System(i64),
    Ship(i64),
    Pilot(String),
    Kill(i64),
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
            IntelTypeFilter::Threat => r.spike || r.camp || r.bubble || r.cyno || r.help || r.tackled || r.cap_tackled,
        }
    }
}

use crate::auth::{self, AuthStatus, SharedAuth};
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
    /// Live edit buffers for the coalition editor: (name, alliances-one-per-line).
    coal_edit: Vec<(String, String)>,
    /// Text input for manually adding an alliance to the sov list.
    alliance_add: String,
    active_character: String,
    /// Settings changed this frame and should be persisted.
    needs_save: bool,
    /// SDE download/bake state (shared with the background worker).
    sde_status: SharedStatus,
    /// SSO login state (shared with the background login worker).
    auth_status: SharedAuth,
    /// Authenticated characters, refreshed from the store each frame.
    characters: Vec<CharacterRow>,
    /// Live intel reports (shared with the chat-log watcher).
    intel_state: std::sync::Arc<std::sync::Mutex<crate::intel::IntelState>>,
    /// Whether the chat-log watcher has been started (after the SDE is ready).
    watcher_started: bool,
    /// Resolved chat-logs directory, or None if EVE logs weren't found.
    chat_dir: Option<std::path::PathBuf>,
    /// Intel-view filters.
    intel_query: String,
    intel_max_jumps: u32,
    intel_type: IntelTypeFilter,
    /// Clustered battle reports (shared with the zKill feed worker).
    battles: crate::zkill::SharedBattles,
    camps: crate::camp::SharedCamps,
    /// Cached camped-system list (recomputed every couple of seconds) so the overlay doesn't
    /// lock + scan the camp state every frame for every map.
    camped_cache: Vec<i64>,
    camped_cache_at: i64,
    /// Live kill-feed buffer (from zkill) turned into optional kill-intel cards.
    killfeed: crate::zkill::SharedKillFeed,
    /// Ship type id → name, built from the ship index, for naming kill-feed ships.
    ship_by_id: std::collections::HashMap<i64, String>,
    /// Active character name + ESI-resolved system (shared with the location poller).
    player: crate::esi::SharedPlayer,
    /// System graph for UI distance queries (set once the SDE is ready).
    systems: Option<std::sync::Arc<crate::geo::Systems>>,
    /// Jump bridges currently baked into `systems` (to detect config changes).
    bridges_applied: Vec<crate::settings::JumpBridge>,
    /// Live per-system status (incursion/FW/sov), shared with the ESI poller.
    system_status: crate::systemstatus::SharedStatus,
    /// Only alert on reports newer than this (set to launch time to skip backlog).
    last_alert_time: i64,
    /// Per-system alert cooldown (system id -> last alert unix seconds).
    alert_cooldown: std::collections::HashMap<i64, i64>,
    /// Recent fired alerts (unix, text) — shared with the game-log watcher.
    recent_alerts: crate::gamewatcher::AlertLog,
    /// Reports shown in the custom notification window, with their severity.
    alert_feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    /// Seconds the custom notification window stays visible (counts down; 0 = hidden,
    /// paused while hovered).
    alert_window_secs: f32,
    /// True while the alert window is currently shown (to detect re-opening so the
    /// saved geometry is re-applied each time it appears).
    alert_window_open: bool,
    /// Pin: temporarily hold the alert window open (auto-cleared when it closes).
    alert_window_pinned: bool,
    /// Master OS-notification gate (mirrors alerts.system_notifications), shared with
    /// the combat-log watcher.
    os_notify: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Self process resource usage (status bar).
    proc_monitor: crate::procstat::Monitor,
    /// Jabber (XMPP) chat + fleet-ping client state.
    jabber: crate::jabber::SharedJabber,
    jabber_tx: Option<crate::jabber::CmdSender>,
    jabber_popped: bool,
    /// Selected conversation (bare JID) in the Jabber view.
    jabber_chat: Option<String>,
    /// Per-conversation message drafts (keeps a half-typed message when switching
    /// chats; a non-empty draft also keeps an otherwise-empty DM open).
    jabber_drafts: std::collections::HashMap<String, String>,
    /// "Join room" text input (a room JID).
    jabber_room_input: String,
    /// Search filter over the shown contacts.
    jabber_contact_search: String,
    /// "Message someone" input — opens a DM by JID/local part.
    jabber_dm_input: String,
    /// Feedback when a DM target can't be resolved to a real contact.
    jabber_dm_error: String,
    /// Roster list shows the public directory (true) or the private contact list.
    jabber_show_directory: bool,
    /// Directory groups the user has collapsed (session-only).
    jabber_collapsed: std::collections::HashSet<String>,
    /// Our own chosen availability + status text.
    jabber_my_presence: crate::jabber::Presence,
    jabber_my_status: String,
    /// Password field in the Jabber connect form (transient).
    jabber_pw_input: String,
    /// Quick-ping composer state.
    ping_compose_open: bool,
    ping_group_input: String,
    ping_draft: PingDraft,
    /// Notifications/alert-rules dialog open.
    ping_rules_open: bool,
    /// App-start time, to mark the historic/new boundary in conversations.
    session_start: i64,
    /// Whether the EVE client is the focused window (for "smart" always-on-top).
    eve_focused: bool,
    /// Throttle for the EVE-focus check.
    eve_focus_checked: Option<std::time::Instant>,
    /// Throttle for reconciling pilot candidates that turned out not to be characters.
    pilot_reconcile_checked: Option<std::time::Instant>,
    /// Ship name -> (id, name), for reclassifying ship-words wrongly read as pilots.
    ship_index: Option<std::sync::Arc<std::collections::HashMap<String, (i64, String)>>>,
    /// Update-checker state + one-shot startup check + per-session "ask later" flag.
    update: crate::update::SharedUpdate,
    update_checked: bool,
    update_dismissed: bool,
    /// Killmail enrichment cache + fetch channel + open Kill window (kill id).
    kill_cache: crate::kills::KillCache,
    kill_tx: Option<crate::kills::KillSender>,
    kill_window: Option<i64>,
    /// Embedded zKill lookup: pasted/dropped names, one tab each.
    lookup_input: String,
    lookup_tabs: Vec<String>,
    lookup_active: usize,
    lookup_cache: crate::charlookup::LookupCache,
    lookup_tx: Option<crate::charlookup::LookupSender>,
    /// Cached intel-card heights (by report key) for feed virtualization.
    intel_heights: std::collections::HashMap<u64, f32>,
    /// First-run setup wizard (dismissable; re-runnable from Settings).
    wizard_open: bool,
    wizard_step: u8,
    wizard_checked: bool,
    /// System tray (Show / Exit) + whether a real quit was requested.
    tray: Option<crate::tray::TrayCmd>,
    really_exit: bool,
    /// D-scan clipboard sharing.
    dscan_clip: Option<arboard::Clipboard>,
    dscan_checked: Option<std::time::Instant>,
    /// Hash of the last clipboard text we examined / the dismissed one.
    dscan_seen_hash: u64,
    dscan_dismissed_hash: u64,
    /// Pending prompt: (dscan text, row count).
    dscan_prompt: Option<(String, usize)>,
    /// Cached screen position for the d-scan popup (bottom-right of the EVE window).
    dscan_pos: Option<(f32, f32)>,
    /// D-scan popup: whether the shared link was opened/copied, and when it last lost
    /// focus (to auto-close 5 s after the user is done with it).
    dscan_link_used: bool,
    dscan_unfocused_at: Option<std::time::Instant>,
    dscan_share: std::sync::Arc<std::sync::Mutex<DscanShare>>,
    /// Known wormholes (reloaded from the store on a timer; written by the EVE-Scout
    /// poller and the intel watcher).
    wh_cache: Vec<crate::wormholes::Wormhole>,
    wh_reloaded: Option<std::time::Instant>,
    /// Map overlay derived from `wh_cache` (recomputed on reload, not per frame).
    wh_overlay: WhOverlay,
    /// Wormholes-view filters.
    wh_filter_dest: Option<crate::wormholes::DestClass>,
    wh_filter_source: Option<crate::wormholes::Source>,
    wh_filter_expiring: bool,
    // --- Map view state ---
    map_overlays: MapOverlays,
    /// Active map mode; the non-Standard modes auto-adapt the overlays.
    map_mode: MapMode,
    /// The user's saved Standard-mode overlays, restored when returning to Standard.
    standard_overlays: MapOverlays,
    // --- Travel Mode ---
    travel_start: Option<i64>,
    travel_end: Option<i64>,
    travel_start_q: String,
    travel_end_q: String,
    travel_regional_gates: bool,
    travel_jump_bridges: bool,
    /// Route around systems currently flagged as gate camps.
    travel_avoid_camps: bool,
    travel_max_ship_kills: u32,
    /// Allowed security bands for intermediate systems (high / low / null).
    travel_sec: [bool; 3],
    /// Highlighted index in the From/To suggestion dropdowns (keyboard nav).
    travel_start_sel: usize,
    travel_end_sel: usize,
    /// Cached From/To suggestions, keyed by (start_q, start, end_q, end) so the SDE search
    /// (a full-table fuzzy scan) runs only when an input changes, not every frame.
    travel_sugg_key: (String, Option<i64>, String, Option<i64>),
    travel_sugg: (Vec<SysHit>, Vec<SysHit>),
    /// "Add waypoint" input + its cached suggestions and keyboard-nav index.
    travel_wp_q: String,
    travel_wp_sel: usize,
    travel_wp_sugg_key: String,
    travel_wp_sugg: Vec<SysHit>,
    /// Activity metric the max-per-hour cap applies to (ship/pod/NPC kills or jumps).
    travel_metric: ActivityMode,
    /// Route re-plan debounce: the input hash the route was last planned for, the last-seen
    /// input hash, and the egui time (seconds) the inputs last changed.
    travel_planned_hash: u64,
    travel_pending_hash: u64,
    travel_dirty_at: Option<f64>,
    /// The game's own shortest gate route (for the comparison overlay).
    travel_direct_route: Option<Vec<i64>>,
    /// Live Mode: track position, continuously re-plan, and re-route in-game on changes.
    travel_live: bool,
    /// The route as first planned when Live Mode engaged (drawn dimmed for comparison).
    travel_live_base: Option<Vec<i64>>,
    /// Systems newly added by the last live re-plan, to blink briefly.
    travel_changed: Vec<i64>,
    /// Wall-clock time the route last changed under Live Mode (for the blink window).
    travel_changed_at: Option<i64>,
    /// Next Live-Mode re-plan time (egui seconds).
    travel_live_next: f64,
    /// The single in-game destination we last wrote (the next hop on the route), so we only
    /// re-write it when it changes. EVE rejects duplicate waypoints, so we advance one hop at a
    /// time instead of writing the whole (possibly self-revisiting) route at once.
    travel_ingame_dest: Option<i64>,
    /// Ordered intermediate waypoints the route must pass through.
    travel_waypoints: Vec<i64>,
    /// Systems the route must avoid.
    travel_avoid: Vec<i64>,
    /// Sov holders (alliance names) to route around, picked from the coalition tree dialog.
    travel_avoid_sov: std::collections::HashSet<String>,
    /// Whether the avoid-sov coalition tree dialog is open.
    travel_sov_dialog_open: bool,
    travel_route: Option<Vec<i64>>,
    /// System targeted by the map right-click context menu.
    ctx_menu_system: Option<i64>,
    /// Jump-route planner endpoints (seeded from the map; planner is WIP).
    jump_plan_from: Option<i64>,
    jump_plan_to: Option<i64>,
    map_view: crate::map::MapView,
    map_initialized: bool,
    map_history: Vec<crate::map::MapView>,
    map_forward: Vec<crate::map::MapView>,
    map_regions: Vec<(i64, String)>,
    map_systems: Vec<crate::store::MapSystem>,
    map_loaded: Option<crate::map::MapView>,
    map_pan: egui::Vec2,
    /// Last map canvas rect, to keep the centre fixed when the window resizes.
    map_last_rect: Option<egui::Rect>,
    map_zoom: f32,
    map_follow: bool,
    map_popped: bool,
    /// True while drawing a per-character pop-out window, so its controls hide the
    /// main-window management buttons (pop-out, on-top, overlay) that don't apply there.
    map_in_popout: bool,
    /// Per-character pop-out map windows (character names) + their saved view state
    /// (region/universe, pan, zoom, whether we've centred on them yet).
    map_char_popouts: Vec<String>,
    map_char_view: std::collections::HashMap<
        String,
        (crate::map::MapView, egui::Vec2, f32, bool, Option<egui::Rect>),
    >,
    /// Pop-out map window kept above other windows.
    map_window_on_top: bool,
    /// Hide all map control overlays (leaving just a "show" button).
    map_controls_hidden: bool,
    /// Overlay mode: borderless, on-top, draggable-on-empty, minimal controls.
    map_overlay_mode: bool,
    /// Overlay mode locked (no moving or resizing).
    map_overlay_locked: bool,
    /// Last (decorations, resizable) applied to the popped map window, so toggling
    /// overlay↔bordered re-applies them live (the builder only sets them on creation).
    map_vp_props: Option<(bool, bool)>,
    /// Last on-top state applied to the alert window (re-applied when it changes, so
    /// "smart" mode tracks EVE focus and the level is maintained while shown).
    alert_level_applied: Option<bool>,
    /// A window-move drag is in progress (so the map doesn't also pan).
    map_overlay_drag: bool,
    /// How systems are laid out (geographic / spaced / radial / tree).
    map_layout: crate::map::MapLayout,
    /// Jumps shown in the radial/tree threat views.
    map_threat_jumps: u32,
    /// Centre system for the threat views (None = active character's system).
    map_threat_center: Option<i64>,
    /// Include jump bridges in the radial/tree threat-view distance.
    threat_include_bridges: bool,
    /// Coordinates actually drawn (geographic or the 2D layout).
    map_draw: Vec<crate::store::MapSystem>,
    map_draw_spaced: bool,
    map_draw_key: Option<(crate::map::MapView, bool)>,
    /// Per-view caches so multiple map instances (main + pop-outs) don't re-query the DB
    /// and re-project every frame as the view is swapped between them.
    map_systems_cache: std::collections::HashMap<crate::map::MapView, Vec<crate::store::MapSystem>>,
    map_draw_cache:
        std::collections::HashMap<(crate::map::MapView, bool), Vec<crate::store::MapSystem>>,
    /// One-shot: centre the map on this system on the next draw (from intel click).
    map_focus: Option<i64>,
    /// Persistently highlighted system on the map (from a search selection).
    map_selected: Option<i64>,
    /// Destination for the in-app route overlay (set via "Set Destination").
    route_destination: Option<i64>,
    map_search: String,
    map_search_sel: usize,
    /// Cached search results keyed by the query, so the SDE scans run only on input change.
    map_search_key: String,
    map_search_sys: Vec<(i64, String, f64)>,
    map_search_const: Vec<(String, i64)>,
    map_search_reg: Vec<(i64, String)>,
    /// Whether the left (standard) and right (mode) map docks are expanded.
    left_dock_open: bool,
    right_dock_open: bool,
    /// Sov-upgrade overlay sub-filters: show Ratting / Exploration / Mining / Other kinds.
    upgrade_kinds: [bool; 4],
    /// An upgrade name to faint-highlight on the map (from the search).
    map_highlight_upgrade: Option<String>,
    /// System-info window: the system currently shown (if any).
    system_window: Option<i64>,
    /// Open constellation / region info windows (by id).
    constellation_window: Option<i64>,
    region_window: Option<i64>,
    /// A viewport to bring to the foreground this frame (a click updated its data).
    focus_window: Option<egui::ViewportId>,
    /// Ship-info window: the ship type currently shown (if any).
    ship_window: Option<i64>,
    /// Pilot lookup (zKill) input + shared result.
    pilot_query: String,
    pilot_lookup: crate::lookup::SharedLookup,
    /// Per-character killmail feed (Kills/Solo/Losses) for the sidebar lookup tabs, fetched
    /// lazily when a list tab is opened.
    feed_cache: std::collections::HashMap<String, crate::lookup::SharedLookup>,
    pilot_window_open: bool,
    pilot_sort: PilotSort,
    pilot_tab: PilotTab,
    /// Fit window: (ship type id, which fit).
    fit_view: Option<(i64, FitMode)>,
    /// A specific clicked killmail to show in the fit window (takes precedence over fit_view).
    fit_loss: Option<crate::lookup::Loss>,
    /// Resolved pilot-name cache (shared with the chat watcher + resolver thread).
    pilots: crate::pilot::SharedPilots,
    /// Static ship-detail cache (avoids per-frame DB queries).
    ship_cache: std::cell::RefCell<std::collections::HashMap<i64, Option<crate::store::ShipDetails>>>,
    /// Cached role badges per ship id.
    ship_roles_cache: std::cell::RefCell<std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>>,
    /// Type-id → name cache for fit modules (filled on demand via ESI).
    type_names: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, String>>>,
    type_names_loading: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl SpaiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load the Phosphor icon font into the proportional family so icons render
        // inline with text everywhere (nav rail, buttons).
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        // Image loaders so we can show ship icons from EVE's image server.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let store = Store::open().map_err(|e| eprintln!("store: {e:#}")).ok();
        let mut settings = store
            .as_ref()
            .and_then(|s| s.load_settings())
            .unwrap_or_default();

        settings.theme.apply(&cc.egui_ctx);

        let combat_on = settings.alert_combat;
        // Seed the default alert rule once (covers nearby intel out of the box).
        if !settings.alerts.seeded {
            settings.alerts.rules.insert(0, crate::settings::default_rule());
            settings.alerts.seeded = true;
        }
        // Restore persisted map/intel options (or defaults).
        let pv: PersistedView = serde_json::from_str(&settings.view_options).unwrap_or(PersistedView {
            overlays: MapOverlays::default(),
            map_layout: crate::map::MapLayout::Spaced,
            map_threat_jumps: 5,
            intel_max_jumps: 0,
            intel_type: IntelTypeFilter::All,
        });

        // Resolve SDE state from what's already baked; otherwise download on first run.
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

        // Poll the active character's ESI location in the background.
        let player: crate::esi::SharedPlayer =
            std::sync::Arc::new(std::sync::Mutex::new(crate::esi::Player::default()));
        if let Some(store) = &store {
            let _ = store;
            let cid = non_empty_or(&settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
            crate::esi::spawn_location_poller(cid, player.clone(), cc.egui_ctx.clone());
        }

        // Restore persisted fleet pings (kept indefinitely).
        let loaded_pings: Vec<crate::pings::Ping> = store
            .as_ref()
            .map(|s| {
                s.load_pings(2000).into_iter().filter_map(|j| serde_json::from_str(&j).ok()).collect()
            })
            .unwrap_or_default();
        // Restore persisted conversations, grouped by JID (in time order).
        let mut loaded_chats: std::collections::BTreeMap<String, Vec<crate::jabber::ChatMsg>> =
            std::collections::BTreeMap::new();
        if let Some(s) = &store {
            let mut purge: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (jid, sender, body, time, outgoing) in s.load_chats(5000) {
                // Auto-purge DMs to non-existent (syntactically invalid) JIDs.
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
            intel_state: std::sync::Arc::new(std::sync::Mutex::new(crate::intel::IntelState::default())),
            watcher_started: false,
            chat_dir: None,
            intel_query: String::new(),
            intel_max_jumps: pv.intel_max_jumps,
            intel_type: pv.intel_type,
            battles: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            camps: std::sync::Arc::new(std::sync::Mutex::new(crate::camp::CampState::default())),
            camped_cache: Vec::new(),
            camped_cache_at: 0,
            killfeed: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            ship_by_id: std::collections::HashMap::new(),
            player,
            systems: None,
            bridges_applied: Vec::new(),
            system_status: {
                let status: crate::systemstatus::SharedStatus =
                    std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
                crate::systemstatus::spawn(status.clone(), cc.egui_ctx.clone());
                status
            },
            last_alert_time: chrono::Utc::now().timestamp(),
            alert_cooldown: std::collections::HashMap::new(),
            recent_alerts: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            alert_feed: Vec::new(),
            alert_window_secs: 0.0,
            alert_window_open: false,
            alert_window_pinned: false,
            os_notify: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(combat_on)),
            proc_monitor: crate::procstat::Monitor::new(),
            jabber,
            jabber_tx: None,
            jabber_popped: false,
            jabber_chat: None,
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
            ping_compose_open: false,
            ping_group_input: String::new(),
            ping_draft: PingDraft::default(),
            ping_rules_open: false,
            session_start: chrono::Utc::now().timestamp(),
            eve_focused: true,
            eve_focus_checked: None,
            pilot_reconcile_checked: None,
            ship_index: None,
            update: std::sync::Arc::new(std::sync::Mutex::new(crate::update::UpdateState::default())),
            update_checked: false,
            update_dismissed: false,
            kill_cache,
            kill_tx,
            kill_window: None,
            lookup_input: String::new(),
            lookup_tabs: Vec::new(),
            lookup_active: 0,
            lookup_cache,
            lookup_tx,
            intel_heights: std::collections::HashMap::new(),
            wizard_open: false,
            wizard_step: 0,
            wizard_checked: false,
            tray: crate::tray::spawn(cc.egui_ctx.clone()),
            really_exit: false,
            dscan_clip: None,
            dscan_checked: None,
            dscan_seen_hash: 0,
            dscan_dismissed_hash: 0,
            dscan_prompt: None,
            dscan_pos: None,
            dscan_link_used: false,
            dscan_unfocused_at: None,
            dscan_share: std::sync::Arc::new(std::sync::Mutex::new(DscanShare::default())),
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
            travel_avoid: Vec::new(),
            travel_avoid_sov: std::collections::HashSet::new(),
            travel_sov_dialog_open: false,
            travel_route: None,
            ctx_menu_system: None,
            jump_plan_from: None,
            jump_plan_to: None,
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
            map_popped: false,
            map_in_popout: false,
            map_char_popouts: Vec::new(),
            map_char_view: std::collections::HashMap::new(),
            map_window_on_top: false,
            map_controls_hidden: false,
            map_overlay_mode: false,
            map_vp_props: None,
            alert_level_applied: None,
            map_overlay_locked: false,
            map_overlay_drag: false,
            map_layout: pv.map_layout,
            map_threat_jumps: pv.map_threat_jumps,
            map_threat_center: None,
            threat_include_bridges: true,
            map_draw: Vec::new(),
            map_draw_spaced: false,
            map_draw_key: None,
            map_systems_cache: std::collections::HashMap::new(),
            map_draw_cache: std::collections::HashMap::new(),
            map_focus: None,
            map_selected: None,
            route_destination: None,
            map_search: String::new(),
            map_search_sel: 0,
            map_search_key: String::new(),
            map_search_sys: Vec::new(),
            map_search_const: Vec::new(),
            map_search_reg: Vec::new(),
            left_dock_open: true,
            right_dock_open: true,
            upgrade_kinds: [true; 4],
            map_highlight_upgrade: None,
            system_window: None,
            constellation_window: None,
            region_window: None,
            focus_window: None,
            ship_window: None,
            pilot_query: String::new(),
            pilot_lookup: std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle)),
            feed_cache: std::collections::HashMap::new(),
            pilot_window_open: false,
            pilot_sort: PilotSort::MostLost,
            pilot_tab: PilotTab::default(),
            fit_view: None,
            fit_loss: None,
            ship_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            ship_roles_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            type_names: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            type_names_loading: std::sync::Arc::new(std::sync::Mutex::new(false)),
            pilots: {
                let cache: crate::pilot::SharedPilots = std::sync::Arc::new(std::sync::Mutex::new(
                    crate::pilot::PilotCache::default(),
                ));
                crate::pilot::spawn_resolver(cache.clone(), cc.egui_ctx.clone());
                cache
            },
        }
    }

    /// Open the system-info window for a system (from map/intel/search click).
    fn open_system(&mut self, system_id: i64) {
        self.system_window = Some(system_id);
        self.focus_window = Some(egui::ViewportId::from_hash_of("system_window"));
    }

    /// Open the ship-info window for a ship type (from an intel ship panel click).
    fn open_ship(&mut self, ship_id: i64) {
        self.ship_window = Some(ship_id);
        self.focus_window = Some(egui::ViewportId::from_hash_of("ship_window"));
    }

    /// Evaluate new intel against the alert rules and dispatch the resulting actions
    /// (system notification / sound / custom window / push). Only reports newer than
    /// the last watermark are considered.
    fn check_alerts(&mut self) {
        if !self.settings.alert_enabled {
            return;
        }
        let systems = self.systems.clone();
        let now = chrono::Utc::now().timestamp();
        let mut newest = self.last_alert_time;
        let acfg = self.settings.alerts.clone();
        let sev_rules = self.settings.severity.clone();
        let only_undocked = self.settings.alert_only_undocked;
        let disabled = self.settings.intel_disabled_chars.clone();
        // Snapshot every linked character's location (name → system, docked).
        let locations: std::collections::HashMap<String, (i64, bool)> =
            self.player.lock().unwrap().locations.clone();

        // The set of character systems a rule should measure distance from: the
        // rule's characters (if any), else every enabled character; docked ones are
        // dropped when "only alert while undocked" is set.
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
            sys_id: i64,
            title: String,
            text: String,
            body: String,
            report: crate::intel::IntelReport,
            sev: crate::settings::Severity,
            sound: String,
            sys: bool,
            win: bool,
            push: bool,
        }
        let mut fired: Vec<Fire> = Vec::new();
        {
            let state = self.intel_state.lock().unwrap();
            for r in &state.reports {
                if r.received <= self.last_alert_time {
                    continue;
                }
                newest = newest.max(r.received);
                let sev = severity_of(r, &sev_rules);
                let target = r.primary_system().map(|s| s.id);
                // First enabled rule whose conditions all match (jumps measured from
                // the rule's relevant characters).
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
                    continue;
                }
                let sound = if ru.sound.is_empty() {
                    acfg.sounds.get(sev as usize).cloned().unwrap_or_default()
                } else {
                    ru.sound.clone()
                };
                let (sys, win, push, cd) =
                    (ru.system_notification, ru.custom_window, ru.push, ru.cooldown_secs);
                let sys_id = r.primary_system().map_or(0, |s| s.id);
                if now - self.alert_cooldown.get(&sys_id).copied().unwrap_or(0) < cd {
                    continue;
                }
                let title = r
                    .primary_system()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Intel".to_owned());
                let title = match jumps {
                    Some(j) if j > 0 => format!("{title} — {j} jumps"),
                    Some(_) => format!("{title} — here"),
                    None => title,
                };
                let text = alert_text(r);
                let body = format!("{text}\n— {} · {}", r.reporter, r.channel);
                fired.push(Fire {
                    sys_id,
                    title,
                    text,
                    body,
                    report: r.clone(),
                    sev,
                    sound,
                    sys,
                    win,
                    push,
                });
            }
        }
        self.last_alert_time = newest;
        if fired.is_empty() {
            return;
        }

        let mut log = self.recent_alerts.lock().unwrap();
        for f in fired {
            self.alert_cooldown.insert(f.sys_id, now);
            log.push((now, f.text.clone()));
            if f.sys {
                notify(f.title.clone(), f.body.clone());
            }
            if !f.sound.is_empty() && !f.sound.eq_ignore_ascii_case("off") {
                crate::sound::play(&f.sound);
            }
            self.alert_feed.push((f.report.clone(), f.sev));
            let n = self.alert_feed.len();
            if n > 100 {
                self.alert_feed.drain(0..n - 100);
            }
            if f.win {
                self.alert_window_secs = if self.settings.alerts.window_timeout <= 0.0 {
                    f32::INFINITY // never auto-hide
                } else {
                    self.settings.alerts.window_timeout.max(3.0)
                };
            }
            if f.push && acfg.push_enabled {
                crate::push::pushover(&acfg.pushover_token, &acfg.pushover_user, &f.text);
            }
        }
        let len = log.len();
        if len > 50 {
            log.drain(0..len - 50);
        }
    }

    fn alerts_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        if ui
            .checkbox(&mut self.settings.alert_enabled, "Enable intel alerts")
            .on_hover_text("Master switch for all intel alerts")
            .changed()
        {
            self.needs_save = true;
        }
        if !self.settings.alert_enabled {
            ui.colored_label(
                crate::theme::standing::WARNING,
                "Intel alerts are off — no rule will fire until this is enabled.",
            );
        } else if !self.settings.alerts.rules.iter().any(|r| r.enabled) {
            ui.colored_label(
                crate::theme::standing::WARNING,
                "No alert rule is enabled — nothing will fire. Enable or add a rule below.",
            );
        }
        ui.add_space(4.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(egui::RichText::new("Alert rules").strong());
            self.alert_rules_ui(ui);
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Recent alerts").strong());
            self.alert_history_ui(ui);
        });
    }

    /// Render the recent fired alerts as full intel cards (same panels as the feed).
    fn alert_history_ui(&mut self, ui: &mut egui::Ui) {
        if self.alert_feed.is_empty() {
            ui.label(egui::RichText::new("None yet.").weak());
            return;
        }
        let feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
            self.alert_feed.iter().rev().take(60).cloned().collect();
        let ship_ids: std::collections::HashSet<i64> =
            feed.iter().flat_map(|(r, _)| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
            ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
        let resolved_pilots: std::collections::HashMap<String, i64> = {
            let cache = self.pilots.lock().unwrap();
            feed.iter()
                .flat_map(|(r, _)| r.pilots.iter())
                .filter_map(|n| match cache.get(n) {
                    Some(Some(id)) => Some((n.clone(), id)),
                    _ => None,
                })
                .collect()
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
            if let Some(c) = intel_row(
                ui, r, now, false, from_you, &systems, &status, &ship_details, &ship_roles,
                &resolved_pilots, &last_ship, &kc, *sev, true,
            ) {
                click = Some(c);
            }
        }
        match click {
            Some(IntelClick::System(id)) => self.open_system(id),
            Some(IntelClick::Kill(kid)) => self.kill_window = Some(kid),
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
            None => {}
        }
    }

    /// Start (or reflect the enabled state of) the Jabber client. Spawns once the
    /// SDE is loaded (formup locations resolve to systems) and a password is stored.
    fn maybe_start_jabber(&mut self, ctx: &egui::Context) {
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
            ctx.clone(),
        ));
    }

    /// Resolve a room the user typed: a bare local part ("scouts") gets the MUC
    /// conference host appended (configured, or derived as `conference.<jid domain>`).
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

    /// Whether a conversation/feed key is currently muted.
    fn jabber_is_muted(&self, key: &str) -> bool {
        self.settings
            .jabber_muted
            .get(key)
            .is_some_and(|&until| until == i64::MAX || chrono::Utc::now().timestamp() < until)
    }

    /// Any non-muted unread conversation or a new ping → drives the badges.
    fn jabber_has_unread(&self) -> bool {
        let st = self.jabber.lock().unwrap();
        if st.pings_unread && !self.jabber_is_muted(crate::jabber::PING_FEED_KEY) {
            return true;
        }
        st.unread.iter().any(|k| !self.jabber_is_muted(k))
    }

    /// Drain new-message notifications each frame: play sounds (respecting mute) and
    /// flash the taskbar when we're not focused.
    fn poll_jabber_notify(&mut self, ctx: &egui::Context) {
        let events: Vec<(String, bool)> =
            { std::mem::take(&mut self.jabber.lock().unwrap().notify) };
        if events.is_empty() {
            return;
        }
        let mut any = false;
        for (key, is_ping) in events {
            if self.jabber_is_muted(&key) {
                continue;
            }
            // Resolve the ping's matching rule → (suppress, notify, sound).
            let (suppress, notify, snd) = if is_ping {
                let latest = self.jabber.lock().unwrap().pings.last().cloned();
                match latest.as_ref().and_then(|p| self.matching_ping_rule(p)) {
                    Some(r) => (r.suppress, r.notify, r.sound.clone()),
                    None => (false, true, self.settings.jabber_ping_sound.clone()),
                }
            } else {
                (false, true, self.settings.jabber_msg_sound.clone())
            };
            if suppress {
                continue; // no sound, badge or attention for suppressed pings
            }
            any = true;
            if self.settings.jabber_sound_enabled && notify {
                crate::sound::play(&snd);
            }
            // Fleet pings raise a desktop notification with the key details.
            if is_ping && notify {
                let latest = self.jabber.lock().unwrap().pings.last().cloned();
                if let Some(crate::pings::Ping::Fleet { fc, doctrine, .. }) = latest {
                    let body = match doctrine {
                        Some(d) => format!("FC: {fc} \u{00B7} {d}"),
                        None => format!("FC: {fc}"),
                    };
                    notify_os("Fleet ping", &body);
                }
            }
        }
        if any && !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }

    /// The bot JID that broadcast pings are sent to.
    fn ping_bot_jid(&self) -> String {
        let b = self.settings.jabber_ping_bot.trim();
        if !b.is_empty() {
            return if b.contains('@') { b.to_owned() } else { self.full_user_jid(b) };
        }
        let domain = self.settings.jabber_jid.split('@').nth(1).unwrap_or("");
        format!("directorbot@{domain}")
    }

    /// The first enabled ping-alert rule a fleet ping matches (for sound + highlight).
    fn matching_ping_rule(&self, p: &crate::pings::Ping) -> Option<&crate::settings::PingRule> {
        use crate::pings::{PapType, Ping};
        let (fc, pap, doctrine, formup_txt, all) = match p {
            Ping::Fleet { fc, pap, doctrine, formup, description, .. } => {
                let formup_txt = formup
                    .iter()
                    .map(|f| match f {
                        crate::pings::Formup::Text(t) => t.clone(),
                        crate::pings::Formup::System(_) => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let pap_s = match pap {
                    Some(PapType::Strategic) => "strategic",
                    Some(PapType::Peacetime) => "peacetime",
                    _ => "",
                };
                let all = format!("{fc} {} {description}", doctrine.clone().unwrap_or_default());
                (fc.to_lowercase(), pap_s, doctrine.clone().unwrap_or_default().to_lowercase(), formup_txt.to_lowercase(), all.to_lowercase())
            }
            Ping::Plain { text, .. } => {
                (String::new(), "", String::new(), String::new(), text.to_lowercase())
            }
        };
        let has = |field: &str, hay: &str| field.trim().is_empty() || hay.contains(&field.to_lowercase());
        self.settings.jabber_ping_rules.iter().find(|r| {
            r.enabled
                && has(&r.fc, &fc)
                && (r.pap.trim().is_empty() || r.pap.eq_ignore_ascii_case(pap))
                && has(&r.doctrine, &doctrine)
                && has(&r.formup, &formup_txt)
                && has(&r.keyword, &all)
        })
    }

    /// Resolve a DM target the user typed: a bare local part gets the user's own JID
    /// domain appended ("Bob" → "Bob@goonfleet.com").
    fn full_user_jid(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains('@') {
            return input.to_owned();
        }
        let domain = self.settings.jabber_jid.split('@').nth(1).unwrap_or("");
        format!("{input}@{domain}")
    }

    /// The quick-ping composer: pick a group, optionally fill the fleet form, send a
    /// `!bping <group> …` command to the bot.
    fn ping_compose_dialog(&mut self, ctx: &egui::Context) {
        if !self.ping_compose_open {
            return;
        }
        let mut open = true;
        let mut send = false;
        egui::Window::new("Send ping")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Group:");
                    egui::ComboBox::from_id_salt("ping_group")
                        .selected_text(if self.ping_draft.group.is_empty() {
                            "—".to_owned()
                        } else {
                            self.ping_draft.group.clone()
                        })
                        .show_ui(ui, |ui| {
                            for g in self.settings.jabber_ping_groups.clone() {
                                ui.selectable_value(&mut self.ping_draft.group, g.clone(), g);
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.ping_group_input)
                            .hint_text("add group")
                            .desired_width(90.0),
                    );
                    if ui.button("+").clicked() && !self.ping_group_input.trim().is_empty() {
                        let g = self.ping_group_input.trim().to_owned();
                        if !self.settings.jabber_ping_groups.contains(&g) {
                            self.settings.jabber_ping_groups.push(g.clone());
                            self.needs_save = true;
                        }
                        self.ping_draft.group = g;
                        self.ping_group_input.clear();
                    }
                });
                ui.checkbox(&mut self.ping_draft.fleet, "Fleet ping (FC / doctrine / form-up / PAP)");
                if self.ping_draft.fleet {
                    egui::Grid::new("fleet_form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("FC");
                        ui.text_edit_singleline(&mut self.ping_draft.fc);
                        ui.end_row();
                        ui.label("Doctrine");
                        ui.text_edit_singleline(&mut self.ping_draft.doctrine);
                        ui.end_row();
                        ui.label("Form-up");
                        ui.text_edit_singleline(&mut self.ping_draft.formup);
                        ui.end_row();
                        ui.label("PAP");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.ping_draft.pap, 0u8, "None");
                            ui.selectable_value(&mut self.ping_draft.pap, 1u8, "Strategic");
                            ui.selectable_value(&mut self.ping_draft.pap, 2u8, "Peacetime");
                        });
                        ui.end_row();
                    });
                }
                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.ping_draft.msg)
                        .desired_rows(2)
                        .desired_width(380.0),
                );
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Sends:").weak());
                ui.label(egui::RichText::new(self.ping_draft.to_command()).monospace().weak());
                ui.add_space(4.0);
                let ok = !self.ping_draft.group.trim().is_empty()
                    && !self.ping_draft.msg.trim().is_empty();
                if ui.add_enabled(ok, egui::Button::new("Send ping")).clicked() {
                    send = true;
                }
            });
        if send {
            let body = self.ping_draft.to_command();
            let bot = self.ping_bot_jid();
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::Send { to: bot, body });
            }
            self.ping_draft.msg.clear();
            self.ping_compose_open = false;
        } else {
            self.ping_compose_open = open;
        }
    }

    /// Fleet-ping alert rules + notification sound settings.
    fn ping_rules_dialog(&mut self, ctx: &egui::Context) {
        if !self.ping_rules_open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        egui::Window::new("Jabber alerts")
            .collapsible(false)
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                egui::Grid::new("snd").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    changed |= ui
                        .checkbox(&mut self.settings.jabber_sound_enabled, "Notification sounds")
                        .changed();
                    ui.end_row();
                    ui.label("Message sound");
                    ui.horizontal(|ui| {
                        changed |= ui.text_edit_singleline(&mut self.settings.jabber_msg_sound).changed();
                        if ui.button(egui_phosphor::regular::PLAY).on_hover_text("Test").clicked() {
                            crate::sound::play(&self.settings.jabber_msg_sound);
                        }
                    });
                    ui.end_row();
                    ui.label("Default ping sound");
                    ui.horizontal(|ui| {
                        changed |= ui.text_edit_singleline(&mut self.settings.jabber_ping_sound).changed();
                        if ui.button(egui_phosphor::regular::PLAY).on_hover_text("Test").clicked() {
                            crate::sound::play(&self.settings.jabber_ping_sound);
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.label(egui::RichText::new("presets: horn · chime · beep · sweep · info · warning · danger · critical · off, or a file path").weak().small());
                    ui.end_row();
                    ui.label("Ping bot JID");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.jabber_ping_bot)
                                .hint_text("directorbot@…"),
                        )
                        .changed();
                    ui.end_row();
                });
                ui.separator();
                ui.label(
                    egui::RichText::new("Fleet-ping rules — a match plays its sound + highlights the ping.")
                        .weak(),
                );
                let mut remove: Option<usize> = None;
                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;
                let n = self.settings.jabber_ping_rules.len();
                for (i, r) in self.settings.jabber_ping_rules.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.group(|ui| {
                            use egui_phosphor::regular as ic;
                            ui.horizontal(|ui| {
                                changed |= ui.checkbox(&mut r.enabled, "").changed();
                                let tog = if r.expanded { ic::CARET_DOWN } else { ic::CARET_RIGHT };
                                if ui.button(tog).on_hover_text("Expand / collapse").clicked() {
                                    r.expanded = !r.expanded;
                                }
                                if r.expanded {
                                    changed |= ui
                                        .add(egui::TextEdit::singleline(&mut r.name).desired_width(200.0))
                                        .changed();
                                } else {
                                    let nm = if r.name.is_empty() { "(unnamed rule)" } else { &r.name };
                                    let txt = if r.enabled {
                                        egui::RichText::new(nm).strong()
                                    } else {
                                        egui::RichText::new(nm).weak().strikethrough()
                                    };
                                    if ui.add(egui::Label::new(txt).sense(egui::Sense::click())).clicked() {
                                        r.expanded = true;
                                    }
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
                                });
                            });
                            if !r.expanded {
                                return;
                            }
                            egui::Grid::new("rule").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                                let wide = 230.0;
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
                            // Actions — suppress overrides (disables) the others.
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .checkbox(&mut r.suppress, "Suppress")
                                    .on_hover_text("Ignore matching pings — no sound, highlight or push")
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
                                ui.horizontal(|ui| {
                                    ui.label("Sound");
                                    changed |= ui.add(egui::TextEdit::singleline(&mut r.sound).desired_width(160.0)).changed();
                                    if ui.button(ic::PLAY).on_hover_text("Test").clicked() {
                                        crate::sound::play(&r.sound);
                                    }
                                });
                            });
                        });
                    });
                }
                if let Some(i) = remove {
                    self.settings.jabber_ping_rules.remove(i);
                    changed = true;
                }
                if let Some(i) = move_up {
                    self.settings.jabber_ping_rules.swap(i, i - 1);
                    changed = true;
                }
                if let Some(i) = move_down {
                    self.settings.jabber_ping_rules.swap(i, i + 1);
                    changed = true;
                }
                ui.separator();
                if ui.button("+ Add rule").clicked() {
                    self.settings.jabber_ping_rules.push(crate::settings::PingRule::default());
                    changed = true;
                }
            });
        self.ping_rules_open = open;
        if changed {
            self.needs_save = true;
        }
    }

    /// Request zKill/ESI enrichment for any killmail links not yet in the cache.
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

    /// Kill window: victim/attacker icons, system, value, and a zKill button.
    fn kill_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.kill_window else { return };
        use egui_phosphor::regular as icon;
        let info = self.kill_cache.lock().unwrap().get(&id).cloned().flatten();
        let systems = self.systems.clone();
        let red = crate::theme::standing::HOSTILE;
        let img = |ui: &mut egui::Ui, url: String, sz: f32| {
            ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(sz, sz)))
        };
        let mut open = true;
        egui::Window::new(format!("{}  Kill", icon::SKULL))
            .open(&mut open)
            .default_width(360.0)
            .collapsible(false)
            .show(ctx, |ui| {
                match &info {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading kill data from zKillboard\u{2026}");
                        });
                    }
                    Some(k) => {
                        ui.horizontal(|ui| {
                            if let Some(ship) = k.victim_ship {
                                img(ui, format!("https://images.evetech.net/types/{ship}/render?size=64"), 56.0);
                            }
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("VICTIM").color(red).strong());
                                ui.horizontal(|ui| {
                                    if let Some(ch) = k.victim_char {
                                        img(ui, format!("https://images.evetech.net/characters/{ch}/portrait?size=32"), 28.0);
                                    }
                                    if let Some(al) = k.victim_alliance {
                                        img(ui, format!("https://images.evetech.net/alliances/{al}/logo?size=32"), 28.0);
                                    }
                                });
                            });
                        });
                        ui.separator();
                        if let Some(sys) = systems.as_ref().and_then(|g| g.info_of(k.system_id)) {
                            ui.label(format!("System: {}", sys.name));
                        }
                        ui.label(format!("Value: {}", fmt_isk(k.value)));
                        ui.horizontal(|ui| {
                            ui.label(format!("Attackers: {} \u{2014}", k.attacker_count));
                            for al in k.attacker_alliances.iter().take(3) {
                                img(ui, format!("https://images.evetech.net/alliances/{al}/logo?size=32"), 22.0);
                            }
                        });
                        ui.label(egui::RichText::new(&k.time).weak());
                    }
                }
                ui.separator();
                if ui
                    .button(format!("{}  Open on zKillboard", icon::ARROW_SQUARE_OUT))
                    .clicked()
                {
                    let _ = open::that(format!("https://zkillboard.com/kill/{id}/"));
                }
            });
        if !open {
            self.kill_window = None;
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

    /// The Jabber chat client: connection form, conversation list, messages +
    /// composer, and a fleet-ping feed.
    fn jabber_ui(&mut self, ui: &mut egui::Ui) {
        // Connection form when not enabled / no password yet.
        let configured = self.settings.jabber_enabled
            && !self.settings.jabber_jid.trim().is_empty()
            && crate::jabber::has_password(self.settings.jabber_jid.trim());
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
                ui.add(
                    egui::TextEdit::singleline(&mut self.jabber_pw_input)
                        .password(true)
                        .desired_width(260.0),
                );
                ui.end_row();
            });
            if ui.button("Connect").clicked() {
                let jid = self.settings.jabber_jid.trim().to_owned();
                if !jid.is_empty() && !self.jabber_pw_input.is_empty() {
                    if let Err(e) = crate::jabber::save_password(&jid, &self.jabber_pw_input) {
                        self.jabber.lock().unwrap().status = format!("Keychain error: {e}");
                    } else {
                        self.jabber_pw_input.clear();
                        self.settings.jabber_enabled = true;
                        self.needs_save = true;
                    }
                }
            }
            // Show any error/status from the client.
            let status = self.jabber.lock().unwrap().status.clone();
            if !status.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(status).weak());
            }
            return;
        }

        // Snapshot what we need, then render (so we don't hold the lock during UI).
        let (connected, status, convos, sel_msgs, pings, rooms, open_dms) = {
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
            // Joined rooms (with unread flag), sorted by JID.
            let rooms: Vec<(String, bool)> =
                st.rooms.iter().map(|r| (r.clone(), st.unread.contains(r))).collect();
            // Open 1:1 DMs (any chat history that isn't a room) — kept as chips like
            // rooms, independent of the directory/contacts toggle.
            let open_dms: Vec<(String, bool)> = st
                .chats
                .keys()
                .filter(|k| !st.rooms.contains(*k) && k.as_str() != crate::jabber::PING_FEED_KEY && valid_bare_jid(k))
                .map(|k| (k.clone(), st.unread.contains(k)))
                .collect();
            (st.connected, st.status.clone(), convos, sel_msgs, st.pings.clone(), rooms, open_dms)
        };

        let mut presence_changed = false;
        ui.horizontal(|ui| {
            if connected {
                use crate::jabber::Presence;
                let (r, g, b) = self.jabber_my_presence.color();
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::CIRCLE)
                        .color(egui::Color32::from_rgb(r, g, b))
                        .size(10.0),
                );
                // The user's own JID next to the status circle.
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
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::CIRCLE)
                        .color(crate::theme::standing::WARNING)
                        .size(10.0),
                );
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
        // Resizable split: contact list (left) | messages (right).
        egui::Panel::left("jabber_split")
            .resizable(true)
            .default_size(210.0)
            .size_range(150.0..=460.0)
            .show_inside(ui, |ui| {
                if ui
                    .selectable_label(self.jabber_chat.is_none(), format!("{}  Fleet pings ({})", egui_phosphor::regular::MEGAPHONE, pings.len()))
                    .clicked()
                {
                    self.jabber_chat = None;
                    self.jabber.lock().unwrap().pings_unread = false;
                }
                ui.separator();
                // Rooms + DMs, each with its input above, capped so the directory
                // keeps at least half the sidebar height.
                let chips_h = (ui.available_height() * 0.5).max(90.0);
                egui::ScrollArea::vertical()
                    .id_salt("chips")
                    .max_height(chips_h)
                    .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let join_btn = ui
                        .button(egui_phosphor::regular::PLUS)
                        .on_hover_text("Join room")
                        .clicked();
                    let resp = ui.add_sized(
                        [ui.available_width(), 20.0],
                        egui::TextEdit::singleline(&mut self.jabber_room_input)
                            .hint_text("room@conference.…"),
                    );
                    let go =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (join_btn || go) && !self.jabber_room_input.trim().is_empty() {
                        let room = self.full_room_jid(&self.jabber_room_input);
                        self.jabber_room_input.clear();
                        if let Some(tx) = &self.jabber_tx {
                            let _ = tx.send(crate::jabber::Cmd::JoinRoom { room: room.clone() });
                        }
                        if !self.settings.jabber_rooms.contains(&room) {
                            self.settings.jabber_rooms.push(room.clone());
                            self.needs_save = true;
                        }
                        // Open the room immediately so it stays in view.
                        self.jabber_chat = Some(room);
                    }
                });
                let mut leave_room: Option<String> = None;
                for (rjid, unread) in &rooms {
                    ui.horizontal(|ui| {
                        let name = short_chip(rjid.split('@').next().unwrap_or(rjid));
                        let mut txt = egui::RichText::new(format!(
                            "{}  {name}",
                            egui_phosphor::regular::USERS_THREE
                        ));
                        if *unread {
                            txt = txt.strong();
                        }
                        let sel = self.jabber_chat.as_deref() == Some(rjid.as_str());
                        if ui.selectable_label(sel, txt).on_hover_text(rjid).clicked() {
                            self.jabber_chat = Some(rjid.clone());
                            self.jabber.lock().unwrap().unread.remove(rjid);
                        }
                        if *unread {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                    .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                    .size(8.0),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(egui_phosphor::regular::X).small(),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("Leave (keeps history)")
                                .clicked()
                            {
                                leave_room = Some(rjid.clone());
                            }
                        });
                    });
                }
                if let Some(rjid) = leave_room {
                    if let Some(tx) = &self.jabber_tx {
                        let _ = tx.send(crate::jabber::Cmd::LeaveRoom { room: rjid.clone() });
                    }
                    self.settings.jabber_rooms.retain(|r| r != &rjid);
                    self.needs_save = true;
                    if self.jabber_chat.as_deref() == Some(rjid.as_str()) {
                        self.jabber_chat = None;
                    }
                }
                // DM: open a direct conversation by JID / local part — grouped with the
                // room/DM chips above (active DMs live here, not down by the directory).
                ui.horizontal(|ui| {
                    let dm_btn = ui
                        .button(egui_phosphor::regular::CHAT_CIRCLE_DOTS)
                        .on_hover_text("Open DM")
                        .clicked();
                    let resp = ui.add_sized(
                        [ui.available_width(), 20.0],
                        egui::TextEdit::singleline(&mut self.jabber_dm_input)
                            .hint_text("Message someone…"),
                    );
                    let go = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (dm_btn || go) && !self.jabber_dm_input.trim().is_empty() {
                        let input = self.jabber_dm_input.trim().to_owned();
                        // A full JID is trusted; a bare name must resolve to a real
                        // roster contact (so we send to the correct JID, not a guess).
                        // A full JID is used as-is; a bare name resolves to a roster
                        // contact's exact JID, else the domain is auto-amended (a valid
                        // local part); a name with spaces that matches nobody is rejected.
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
                                self.jabber.lock().unwrap().unread.remove(&jid);
                                self.jabber_chat = Some(jid);
                            }
                            None => {
                                self.jabber_dm_error = format!("No contact matching \"{input}\"");
                            }
                        }
                    }
                });
                if !self.jabber_dm_error.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.jabber_dm_error)
                            .color(crate::theme::standing::WARNING)
                            .small(),
                    );
                }                // Open DMs (persistent chips like rooms): started conversations, plus
                // any empty DM that still holds a non-whitespace draft.
                let mut dm_chips = open_dms.clone();
                for (jid, draft) in &self.jabber_drafts {
                    if !draft.trim().is_empty()
                        && jid.as_str() != crate::jabber::PING_FEED_KEY
                        && !rooms.iter().any(|(r, _)| r == jid)
                        && !dm_chips.iter().any(|(d, _)| d == jid)
                    {
                        dm_chips.push((jid.clone(), false));
                    }
                }
                let mut close_dm: Option<String> = None;
                for (djid, unread) in &dm_chips {
                    // Closed DMs are hidden (history kept); re-opening un-hides them.
                    if self.settings.jabber_closed_dms.iter().any(|j| j == djid) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        let pres = convos
                            .iter()
                            .find(|c| &c.jid == djid)
                            .map(|c| c.presence)
                            .unwrap_or_default();
                        let (pr, pg, pb) = pres.color();
                        ui.label(
                            egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                .color(egui::Color32::from_rgb(pr, pg, pb))
                                .size(9.0),
                        );
                        let name = short_chip(djid.split('@').next().unwrap_or(djid));
                        let mut txt = egui::RichText::new(name);
                        if *unread {
                            txt = txt.strong();
                        }
                        let sel = self.jabber_chat.as_deref() == Some(djid.as_str());
                        if ui.selectable_label(sel, txt).on_hover_text(djid).clicked() {
                            self.jabber_chat = Some(djid.clone());
                            self.jabber.lock().unwrap().unread.remove(djid);
                        }
                        if *unread {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                    .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                    .size(8.0),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(egui_phosphor::regular::X).small(),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("Close (keeps history)")
                                .clicked()
                            {
                                close_dm = Some(djid.clone());
                            }
                        });
                    });
                }
                if let Some(jid) = close_dm {
                    if !self.settings.jabber_closed_dms.contains(&jid) {
                        self.settings.jabber_closed_dms.push(jid.clone());
                        self.needs_save = true;
                    }
                    if self.jabber_chat.as_deref() == Some(jid.as_str()) {
                        self.jabber_chat = None;
                    }
                }
                });
                ui.separator();
                // Directory / Contacts toggle (independent of pings/DMs above), each
                // marked when it has unread.
                let contacts: std::collections::HashSet<String> =
                    self.settings.jabber_contacts.iter().cloned().collect();
                let dir_unread = convos.iter().any(|c| c.unread);
                let con_unread = convos.iter().any(|c| c.unread && contacts.contains(&c.jid));
                ui.horizontal(|ui| {
                    let dir = ui.selectable_label(self.jabber_show_directory, "Directory");
                    if dir_unread {
                        ui.scope(|ui| {
                            ui.label(egui::RichText::new(egui_phosphor::regular::CIRCLE).color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C)).size(8.0));
                        });
                    }
                    if dir.clicked() {
                        self.jabber_show_directory = true;
                    }
                    let con = ui.selectable_label(!self.jabber_show_directory, "Contacts");
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
                // Filter: directory shows the whole roster, contacts only the private
                // list; the search box narrows by name or JID.
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
                let mut toggle_contact: Option<(String, bool)> = None; // (jid, add?)
                egui::ScrollArea::vertical().id_salt("convos").auto_shrink([false, false]).show(ui, |ui| {
                    if groups.is_empty() && !show_dir {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("No contacts yet — add people from the Directory.").weak());
                    }
                    for (group, mut members) in groups {
                        // Sort A–Z within each group (case-insensitive).
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
                                // Collapsed group with unread → red dot.
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
                            let dot = egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                .color(egui::Color32::from_rgb(r, g, b))
                                .size(9.0);
                            // Ellipsize to the space left after the dot + star (the
                            // scroll area already excludes the scrollbar from avail).
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
                                ui.label(dot);
                                let clicked = ui.selectable_label(sel, name)
                                    .on_hover_text(&c.name)
                                    .clicked();
                                // Unread DM / group conversation → red dot.
                                if c.unread {
                                    ui.label(
                                        egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                            .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                            .size(8.0),
                                    );
                                }
                                // Add to / remove from the private contact list.
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
                                self.jabber_chat = Some(c.jid.clone());
                                self.jabber.lock().unwrap().unread.remove(&c.jid);
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
        egui::CentralPanel::default().show_inside(ui, |ui| {
                match self.jabber_chat.clone() {
                    None => {
                        ui.horizontal(|ui| {
                            if ui
                                .button(format!("{}  Send ping", egui_phosphor::regular::PAPER_PLANE_TILT))
                                .clicked()
                            {
                                self.ping_compose_open = true;
                            }
                        });
                        ui.separator();
                        // Pre-compute which pings match an alert rule (for highlight).
                        let hl: Vec<bool> =
                            pings.iter().map(|p| self.matching_ping_rule(p).is_some_and(|r| !r.suppress)).collect();
                        egui::ScrollArea::vertical().id_salt("pings").auto_shrink([false, false]).show(ui, |ui| {
                            if pings.is_empty() {
                                ui.label(egui::RichText::new("No pings yet.").weak());
                            }
                            for (i, p) in pings.iter().enumerate().rev() {
                                render_ping(ui, p, &systems, hl[i]);
                            }
                        });
                    }
                    Some(jid) => {
                        use egui_phosphor::regular as icon;
                        let is_room = rooms.iter().any(|(r, _)| r == &jid);
                        let muted = self.jabber_is_muted(&jid);
                        // Header: name, mute menu, and Leave (rooms only).
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
                                    if is_room && ui.button("Leave").clicked() {
                                        if let Some(tx) = &self.jabber_tx {
                                            let _ = tx.send(crate::jabber::Cmd::LeaveRoom {
                                                room: jid.clone(),
                                            });
                                        }
                                        self.settings.jabber_rooms.retain(|r| r != &jid);
                                        self.needs_save = true;
                                        self.jabber_chat = None;
                                    }
                                    // Add/remove this DM partner from the contact list.
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
                        // Measure the height left after the (optional) room header so
                        // the composer always stays on-screen.
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
                                // Tight rows so the time line hugs its message.
                                ui.spacing_mut().item_spacing.y = 1.0;
                                let mut hist_drawn = false;
                                let mut prev_sender: Option<String> = None;
                                let mut prev_time: i64 = 0;
                                for m in &sel_msgs {
                                    // A divider where historic (loaded) messages end.
                                    if !hist_drawn && m.time >= session_start && m.time > 0 {
                                        hist_drawn = true;
                                        prev_sender = None; // don't group across the divider
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("— new —").weak().small());
                                            ui.separator();
                                        });
                                    }
                                    let sender =
                                        if m.outgoing { "\u{0}me".to_owned() } else { m.from.clone() };
                                    // Group consecutive messages from the same sender within 5 min
                                    // (skip the repeated time + name lines until someone else talks
                                    // or 5 min elapses).
                                    let grouped = prev_sender.as_deref() == Some(sender.as_str())
                                        && m.time >= prev_time
                                        && m.time - prev_time < 300;
                                    if !grouped {
                                        // Compact EVE-time line directly above the sender.
                                        ui.add_space(5.0); // gap from the previous message
                                        ui.label(
                                            egui::RichText::new(eve_time_label(m.time, now))
                                                .weak()
                                                .size(9.5),
                                        );
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        if !grouped {
                                            if m.outgoing {
                                                ui.label(
                                                    egui::RichText::new("me:").color(me_col).strong(),
                                                );
                                            } else {
                                                let n = m.from.split('@').next().unwrap_or(&m.from);
                                                // Clickable in rooms → DM that person.
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
                                    prev_sender = Some(sender);
                                    prev_time = m.time;
                                }
                            });
                        ui.horizontal_top(|ui| {
                            // 2-line composer that grows with content (capped); Enter
                            // sends, Shift+Enter inserts a newline.
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
                        // Clicking a room member's name opens a DM with them.
                        if let Some(nick) = dm_click {
                            let dm = self.full_user_jid(&nick);
                            self.jabber.lock().unwrap().unread.remove(&dm);
                            self.jabber_chat = Some(dm);
                        }
                    }
                }
            });
    }

    /// The Jabber chat in its own OS window.
    #[allow(deprecated)]
    fn show_jabber_viewport(&mut self, ctx: &egui::Context) {
        let mut keep = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("jabber_window"),
            egui::ViewportBuilder::default().with_title("EVE Spai — Jabber").with_inner_size([720.0, 560.0]),
            |ctx, _| {
                egui::CentralPanel::default().show(ctx, |ui| self.jabber_ui(ui));
                if ctx.input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            },
        );
        if !keep {
            self.jabber_popped = false;
        }
    }

    /// A pilot candidate that ESI confirms is NOT a character falls back to being a
    /// system if the name contains one ("Amarr slave 3424" → Amarr, once we learn
    /// it's not a real pilot). showinfo-confirmed characters are never demoted.
    /// Turn buffered zkill killmails into intel cards when within range and worth showing
    /// (skips shuttles, rookie corvettes and empty pods).
    fn ingest_killfeed(&mut self) {
        let events: Vec<crate::zkill::KillEvent> =
            std::mem::take(&mut *self.killfeed.lock().unwrap());
        if !self.settings.kill_intel || events.is_empty() {
            return;
        }
        let (Some(me), Some(geo)) = (self.player_system(), self.systems.clone()) else {
            return;
        };
        if self.ship_by_id.is_empty() {
            if let Some(idx) = &self.ship_index {
                for (id, name) in idx.values() {
                    self.ship_by_id.insert(*id, name.clone());
                }
            }
        }
        let range = self.settings.kill_intel_jumps;
        let mut st = self.intel_state.lock().unwrap();
        for ev in events {
            if geo.jumps(me, ev.system_id, range).is_none() {
                continue;
            }
            let Some(sys) = geo.info_of(ev.system_id) else { continue };
            let ship = self.ship_by_id.get(&ev.ship_type_id).cloned().unwrap_or_default();
            let lower = ship.to_lowercase();
            if lower.contains("shuttle")
                || matches!(lower.as_str(), "reaper" | "impairor" | "ibis" | "velator")
                || (lower == "capsule" && ev.value < 10_000_000.0)
            {
                continue;
            }
            let mut report = crate::intel::IntelReport::default();
            report.received = ev.time;
            report.killmail = true;
            report.channel = "zKill".to_owned();
            report.reporter = "zKill".to_owned();
            report.isk = Some(ev.value as u64);
            report.systems.push(crate::intel::DetectedSystem {
                id: sys.id,
                name: sys.name.clone(),
                security: sys.security,
            });
            if ship.is_empty() {
                report.text = format!("Ship lost in {}", sys.name);
            } else {
                report.ships.push(crate::intel::DetectedShip {
                    id: ev.ship_type_id,
                    name: ship.clone(),
                });
                report.text = format!("{} lost in {}", ship, sys.name);
            }
            st.push(report);
        }
    }

    fn reconcile_unresolved_pilots(&mut self) -> bool {
        let due = self.pilot_reconcile_checked.map(|t| t.elapsed().as_millis() > 700).unwrap_or(true);
        if !due {
            return false;
        }
        self.pilot_reconcile_checked = Some(std::time::Instant::now());
        let Some(geo) = self.systems.clone() else { return false };
        let ships = self.ship_index.clone();
        let mut changed = false;
        // Lock order MUST match the watcher (intel_state → pilots); the reverse order
        // here deadlocked the UI thread against the watcher (ABBA).
        let mut st = self.intel_state.lock().unwrap();
        let mut cache = self.pilots.lock().unwrap();
        for r in &mut st.reports {
            let original: Vec<String> = std::mem::take(&mut r.pilots);
            let mut new_pilots: Vec<String> = Vec::new();
            for p in original.iter().cloned() {
                if crate::intel::is_pilot_stopword(&p) {
                    continue; // blacklist overrides any cached/char-linked verdict
                }
                let char_linked = r.char_ids.iter().any(|(n, _)| n.eq_ignore_ascii_case(&p));
                if char_linked {
                    new_pilots.push(p);
                    continue;
                }
                match cache.get(&p) {
                    // Confirmed character — keep as-is.
                    Some(Some(_)) => new_pilots.push(p),
                    // Still pending — keep showing it, and (re)queue the full name + its
                    // sub-spans so a lost queue entry (dropped at the cap, in flight when a
                    // batch failed, or a never-queued 4+ word run) still resolves instead of
                    // staying pending until a restart.
                    None => {
                        cache.queue(&p);
                        for w in crate::pilot::name_windows(&p) {
                            cache.queue(&w);
                        }
                        new_pilots.push(p);
                    }
                    // Not a character as a whole: cover it with confirmed sub-names
                    // (the over-glued run "Wwallddo Lulu Uanid" -> Wwallddo + Lulu Uanid).
                    Some(None) => {
                        let cover: Vec<String> = cache
                            .cover(&p)
                            .into_iter()
                            .filter(|n| !crate::intel::is_pilot_stopword(n))
                            .collect();
                        if !cover.is_empty() {
                            new_pilots.extend(cover);
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
            // A standalone word that is a known ship is the ship, not a pilot — even when
            // a character shares the name ("Buzzard"). Move it to the ship list.
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
            // A name that only appears as the leading words of a longer detected name here
            // ("Gallente Citizen" inside "Gallente Citizen 17120704") is the same span — drop
            // it even when both are real characters.
            let deduped = crate::intel::drop_covered_prefixes(&r.pilots, &r.text);
            if deduped.len() != r.pilots.len() {
                changed = true;
                r.pilots = deduped;
            }
            // Count fallback: a bare number tentatively read as a name component ("Adama
            // 80") is counted after all if ESI says the "{name} {n}" candidate isn't a
            // real character ("Bob 80" -> 80 was a hostile count).
            let mut add = 0u32;
            let mut requeue: Vec<String> = Vec::new();
            r.name_number_skips.retain(|(cand, num)| match cache.get(cand) {
                Some(None) => {
                    add += *num;
                    false
                }
                Some(Some(_)) => false,
                None => {
                    requeue.push(cand.clone());
                    true
                }
            });
            for c in requeue {
                cache.queue(&c);
            }
            if add > 0 {
                r.count = Some(r.count.unwrap_or(0) + add);
                changed = true;
            }
        }
        changed
    }

    /// Reload the wormhole cache from the store (throttled), dropping expired holes.
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

    /// Wormhole-aware route from `from` to `dest`: the entrance system of each hole the
    /// shortest path uses, then the destination. `None` if unreachable; a single-element
    /// `[dest]` means no hole shortens the route (use a plain waypoint).
    fn wh_route_waypoints(&self, from: i64, dest: i64) -> Option<Vec<i64>> {
        use std::collections::{HashMap, HashSet, VecDeque};
        let geo = self.systems.as_ref()?;
        let mut wh_adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(a, b) in &self.wh_overlay.direct {
            wh_adj.entry(a).or_default().push(b);
            wh_adj.entry(b).or_default().push(a);
        }
        for &(a, b, _) in &self.wh_overlay.chains {
            wh_adj.entry(a).or_default().push(b);
            wh_adj.entry(b).or_default().push(a);
        }
        let mut prev: HashMap<i64, (i64, bool)> = HashMap::new();
        let mut visited: HashSet<i64> = HashSet::from([from]);
        let mut q: VecDeque<i64> = VecDeque::from([from]);
        let mut found = from == dest;
        while let Some(u) = q.pop_front() {
            if u == dest {
                found = true;
                break;
            }
            for &v in geo.neighbors(u) {
                if visited.insert(v) {
                    prev.insert(v, (u, false));
                    q.push_back(v);
                }
            }
            for &v in wh_adj.get(&u).into_iter().flatten() {
                if visited.insert(v) {
                    prev.insert(v, (u, true));
                    q.push_back(v);
                }
            }
        }
        if !found {
            return None;
        }
        let mut waypoints = vec![dest];
        let mut cur = dest;
        while let Some(&(p, via_wh)) = prev.get(&cur) {
            if via_wh {
                waypoints.push(p);
            }
            cur = p;
        }
        waypoints.reverse();
        Some(waypoints)
    }

    /// Set the in-game destination, routing via wormholes (waypoints at each hole
    /// entrance) when enabled and that's shorter than the gate route.
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

    /// The Wormholes view (docs/WORMHOLES_AND_NEXT.md W4): a table of known holes
    /// seeded from EVE-Scout (Thera/Turnur) and intel channels.
    fn wormholes_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(format!("{}  Wormholes", egui_phosphor::regular::SPIRAL));
            ui.label(egui::RichText::new(format!("{} known", self.wh_cache.len())).weak());
        });
        // Filters: destination class, source, and "expiring soon".
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
        // Precompute display strings so the table closure only borrows &mut self for
        // the click handlers.
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
                // Prefer the actual far system (with its constellation/region) when
                // known, else the bare destination class.
                let (dest, dest_const, dest_region) = match w.dest_system_id.and_then(info_of) {
                    Some(i) => (i.name, i.constellation, i.region),
                    None => (w.dest.label().to_string(), String::new(), String::new()),
                };
                let life = if w.explicit_expiry.is_some() {
                    // A hole only advertises a coarse maximum ("< Nh"); the real life is
                    // always shorter, so present it as an upper bound.
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
                        // Destination: an arrow icon + the target (clickable when known).
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

    /// Start the chat-log watcher once the SDE is baked (it needs the system index).
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
        self.watcher_started = true; // mark started regardless, so we don't re-detect every frame

        // Build the system graph once, adding any configured jump bridges, and
        // share it with both the chat watcher and the battle (zKill) feed.
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

        // Bake ship role bonuses (invTraits) lazily, once.
        if let Some(store) = &self.store {
            if !store.traits_baked() {
                sde::spawn_traits_bake(store.path().to_path_buf(), ctx.clone());
            }
            // Pre-load remembered pilot names so they're recognised immediately.
            {
                let mut c = self.pilots.lock().unwrap();
                c.preload(&store.known_pilots());
                c.preload_negatives(&store.known_negatives());
            }
        }

        // The battle feed runs whenever the SDE is ready (independent of logs).
        crate::zkill::spawn(
            systems.clone(),
            self.intel_state.clone(),
            self.battles.clone(),
            self.camps.clone(),
            self.killfeed.clone(),
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
                ctx.clone(),
            );
        }

        // Combat alerts from game logs.
        if self.settings.alert_combat {
            if let Some(game_dir) = crate::logpaths::game_logs_dir(&self.settings.eve_logs_dir) {
                crate::gamewatcher::spawn(
                    game_dir,
                    self.recent_alerts.clone(),
                    self.os_notify.clone(),
                    ctx.clone(),
                );
            }
        }
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

        // Full-width filter bar: type · max-jumps · search.
        ui.horizontal(|ui| {
            use IntelTypeFilter::*;
            for (lbl, v) in [
                ("All", All),
                ("Hostile", Hostile),
                ("Clear", Clear),
                ("Kill", Kill),
                ("Threat", Threat),
            ] {
                if ui.selectable_label(self.intel_type == v, lbl).clicked() {
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
        // Strictly newest-first by report time (insertion order can drift once a
        // report is amended and its time refreshed).
        matches.sort_by(|a, b| b.received.cmp(&a.received));
        let last_ship = build_last_ship(&state.reports);

        ui.label(egui::RichText::new(format!("{} reports", matches.len())).weak());
        ui.add_space(4.0);
        // Ship details (cached) for hull names/icons mentioned in the reports.
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
        // Pilot names confirmed as real characters (by the background resolver).
        let resolved_pilots: std::collections::HashMap<String, i64> = {
            let cache = self.pilots.lock().unwrap();
            matches
                .iter()
                .flat_map(|r| r.pilots.iter())
                .filter_map(|name| match cache.get(name) {
                    Some(Some(id)) => Some((name.clone(), id)),
                    _ => None,
                })
                .collect()
        };
        let mut action: Option<IntelClick> = None;
        let ttl = self.settings.intel_ttl_secs;
        {
            let status = self.system_status.lock().unwrap();
            // Cards are variable-height (badges wrap), so render normally rather than
            // with fixed-row virtualisation; cap the count to keep it cheap.
            const CARD_CAP: usize = 250;
            // Bound the height cache (report keys change on merge).
            if self.intel_heights.len() > 2000 {
                self.intel_heights.clear();
            }
            // Virtualised: only on-screen cards are rendered; off-screen ones reserve
            // their cached height. egui doesn't virtualise variable-height content, so
            // with hundreds of cards this is what keeps the per-frame cost low.
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
                        let inner = ui.scope(|ui| {
                            intel_row(
                                ui, r, now, stale, from_you, &systems, &status, &ship_details,
                                &ship_roles, &resolved_pilots, &last_ship, &kc, sev, true,
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
        match action {
            Some(IntelClick::System(id)) => self.open_system(id),
            Some(IntelClick::Kill(kid)) => self.kill_window = Some(kid),
            Some(IntelClick::Ship(id)) => self.open_ship(id),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ui.ctx().clone());
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            None => {}
        }
    }

    /// Overview: at-a-glance summary of live state.
    fn dashboard_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let now = chrono::Utc::now().timestamp();
        let player_sys = self.player_system();
        let systems = self.systems.clone();

        // Active character + location.
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

        // Intel + battle summary.
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
            ui.label(format!("Battles: {battle_count}"));
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // Recent alerts.
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

    /// The Battle Report view: clusters of killmails near the tracked area.
    /// Queue one tab per (de-duplicated) name from a block of pasted/dropped text.
    fn add_lookup_names(&mut self, text: &str) {
        for line in text.lines() {
            let name = line.trim();
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

    /// Embedded zKill lookup: paste/drop pilot names, one tab per pilot.
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
                .hint_text("Pilot names, one per line...")
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
        // For a list tab, lazily fetch this character's killmail feed.
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
                    self.km_list(ui, list, report.loading);
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

    /// Render a looked-up pilot's zKill profile.
    fn lookup_profile(ui: &mut egui::Ui, info: &crate::charlookup::LookupInfo) {
        use egui_phosphor::regular as icon;
        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(format!(
                    "https://images.evetech.net/characters/{}/portrait?size=128",
                    info.char_id
                ))
                .fit_to_exact_size(egui::Vec2::splat(72.0)),
            );
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&info.name).strong().size(18.0));
                if !info.corp.is_empty() {
                    ui.label(egui::RichText::new(&info.corp).weak());
                }
                if !info.alliance.is_empty() {
                    ui.label(egui::RichText::new(&info.alliance).weak());
                }
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
                        egui::Image::new(format!(
                            "https://images.evetech.net/types/{id}/icon?size=32"
                        ))
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

    #[allow(dead_code)] // battles kept for later; not in the nav for now
    fn battles_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        if self.chat_dir.is_none() && self.settings.intel_channels.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Battle reports cluster killmails near systems seen in intel. \
                     Configure intel channels (Settings) so there's an area to watch.",
                )
                .weak(),
            );
        }

        let now = chrono::Utc::now().timestamp();
        let battles = self.battles.lock().unwrap();
        // Only multi-kill clusters count as a "battle".
        let shown: Vec<&crate::battle::Battle> =
            battles.iter().filter(|b| b.kills >= 2).collect();

        if shown.is_empty() {
            ui.label(
                egui::RichText::new("No active battles near the tracked area.").weak(),
            );
            return;
        }

        ui.label(egui::RichText::new(format!("{} battles", shown.len())).weak());
        ui.add_space(4.0);
        let player_sys = self.player_system();
        let systems = self.systems.clone();
        let status = self.system_status.lock().unwrap();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for b in shown {
                // Nearest battle system to the player.
                let from_you = b
                    .systems
                    .iter()
                    .filter_map(|(id, _, _)| jumps_from_you(&systems, player_sys, Some(*id)))
                    .min();
                battle_row(ui, b, now, from_you, &systems, &status);
                ui.add_space(4.0);
            }
        });
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

    /// The Characters view (M1: SSO login + token storage; live ESI data lands later).
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
        for c in &self.characters {
            let scope_count = c.scopes.split(' ').filter(|s| !s.is_empty()).count();
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(c.id);
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

    /// Pilot lookup: resolve a name, pull recent zKill losses, and show the hulls
    /// the pilot flies (click a hull for its ship window).
    /// Cached role badges for a ship (derived from its baked role bonuses).
    fn ship_roles_cached(&self, id: i64) -> Vec<(&'static str, &'static str)> {
        if let Some(r) = self.ship_roles_cache.borrow().get(&id) {
            return r.clone();
        }
        let traits = self.store.as_ref().map(|s| s.ship_traits(id)).unwrap_or_default();
        let roles = derive_roles(&traits);
        self.ship_roles_cache.borrow_mut().insert(id, roles.clone());
        roles
    }

    /// Cached static ship details (avoids a DB query per ship every frame).
    fn ship_details_cached(&self, id: i64) -> Option<crate::store::ShipDetails> {
        if let Some(d) = self.ship_cache.borrow().get(&id) {
            return d.clone();
        }
        let d = self.store.as_ref().and_then(|s| s.ship_details(id));
        self.ship_cache.borrow_mut().insert(id, d.clone());
        d
    }

    /// Pilot lookup window (zKill): hulls flown + fits, in its own OS window.
    fn pilot_window(&mut self, ctx: &egui::Context) {
        use crate::lookup::LookupState;
        if !self.pilot_window_open {
            return;
        }
        let keep = Self::dialog_viewport(ctx, "pilot_window", "EVE Spai — Pilot", [420.0, 560.0], |ui| {
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

    /// A killmail list for a Kills / Solo / Losses tab (newest first, as fetched).
    fn km_list(&mut self, ui: &mut egui::Ui, list: &[crate::lookup::Loss], loading: bool) {
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
                // Skip the noise: pods, rookie corvettes, and the basic NPC racial shuttles.
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
                    let url = format!("https://images.evetech.net/types/{}/icon?size=32", l.ship_type_id);
                    let img = ui.add(
                        egui::Image::new(url)
                            .fit_to_exact_size(egui::Vec2::splat(26.0))
                            .sense(egui::Sense::click()),
                    );
                    let ship = det.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "?".to_owned());
                    let name =
                        ui.add(egui::Label::new(egui::RichText::new(ship).strong()).sense(egui::Sense::click()));
                    if img.on_hover_text("Show fit").clicked() || name.clicked() {
                        clicked = Some(l.clone());
                    }
                    if let Some(sys) = self.systems.as_ref().and_then(|g| g.info_of(l.system_id)) {
                        ui.label(egui::RichText::new(&sys.name).weak());
                    }
                    if l.value > 0.0 {
                        let isk = if l.value >= 1e9 {
                            format!("{:.1}B", l.value / 1e9)
                        } else {
                            format!("{:.0}M", l.value / 1e6)
                        };
                        ui.label(isk);
                    }
                    let age = now - l.time;
                    let age_s = if age < 3600 {
                        format!("{}m", age / 60)
                    } else if age < 86_400 {
                        format!("{}h", age / 3600)
                    } else {
                        format!("{}d", age / 86_400)
                    };
                    ui.label(egui::RichText::new(age_s).weak());
                    if ui.button("\u{2197}").on_hover_text("Open on zKillboard").clicked() {
                        let _ = open::that(format!("https://zkillboard.com/kill/{}/", l.killmail_id));
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
            PilotTab::Kills => return self.km_list(ui, &report.kills, report.loading),
            PilotTab::Solo => return self.km_list(ui, &report.solo, report.loading),
            PilotTab::Losses => return self.km_list(ui, &report.losses, report.loading),
            PilotTab::Overview => {}
        }
        ui.horizontal(|ui| {
            ui.label("Sort:");
            ui.selectable_value(&mut self.pilot_sort, PilotSort::MostLost, "Most lost");
            ui.selectable_value(&mut self.pilot_sort, PilotSort::Recent, "Recent");
        });

        // Aggregate hulls (excluding pods, corvettes and shuttles).
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
                    let url = format!("https://images.evetech.net/types/{ship_id}/icon?size=32");
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

    /// Ensure module type names are resolved (background ESI bulk lookup).
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

    /// Fit window: the pilot's chosen fit for a hull, with EFT copy + open-in-site.
    fn fit_window(&mut self, ctx: &egui::Context) {
        // A clicked killmail (specific fit) takes precedence over the (ship, mode) aggregate.
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

        let keep = Self::dialog_viewport(ctx, "fit_window", "EVE Spai — Fit", [460.0, 620.0], |ui| {
            ui.horizontal(|ui| {
                let url = format!("https://images.evetech.net/types/{ship_id}/icon?size=32");
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
                // Modules sit in their slots (qty 1); loaded charges (qty > 1 in a
                // fitted slot) and cargo are collected into the cargo hold, stacked.
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
                // Save to the active character's in-game fittings.
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

    fn start_sde(&self, ctx: &egui::Context) {
        if let Some(store) = &self.store {
            sde::spawn_download(store.path().to_path_buf(), self.sde_status.clone(), ctx.clone());
        }
    }

    /// Track the last externally-focused window (throttled), so the alert window can
    /// return focus to it. Only meaningful when the custom window is enabled.
    /// Custom notification window: a floating, always-on-top feed of intel that
    /// passed the alert filters. Auto-hides `window_timeout` s after the last alert;
    /// hovering pauses the countdown (and bumps it to ≥3 s).
    #[allow(deprecated)] // CentralPanel::show is correct for a viewport root ctx
    fn alert_window(&mut self, ctx: &egui::Context) {
        // The window stays alive whenever the feature is in use; we never destroy it
        // (which is what made it steal focus on each alert). When idle it's fully
        // transparent and click-through; an alert just makes it opaque + interactive.
        let feature = self.settings.alert_enabled
            && self.settings.alerts.rules.iter().any(|r| r.enabled && r.custom_window);
        if !feature {
            self.alert_window_secs = 0.0;
            self.alert_window_open = false;
            self.alert_window_pinned = false;
            return;
        }
        let active = self.alert_window_secs > 0.0;
        let just_opened = active && !self.alert_window_open;
        self.alert_window_open = active;
        if !active {
            self.alert_window_pinned = false;
        }
        // For "smart" on-top, refresh whether EVE is focused (throttled to ~1 s).
        if self.settings.alerts.on_top == crate::settings::OnTop::Smart {
            let due = self
                .eve_focus_checked
                .map(|t| t.elapsed().as_millis() > 800)
                .unwrap_or(true);
            if due {
                self.eve_focused = eve_is_focused();
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        // Card data for the feed (built only when there's something to show). Swap each
        // snapshot taken when the alert fired for the LIVE reconciled report, so the alert
        // window shows the same resolved pilots/ships as the main feed (the reconcile
        // updates intel_state.reports, not the alert_feed snapshots).
        let feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> = if active {
            let live = self.intel_state.lock().unwrap();
            self.alert_feed
                .iter()
                .rev()
                .take(20)
                .filter_map(|(r, sev)| {
                    // Show the LIVE report only — never the stale snapshot. Match by the
                    // stable report id, not content: an amendment keeps the id but changes
                    // the content (and thus report_key), which used to drop the card here
                    // even though it was still live. Only a truly removed report falls out.
                    let id = r.id;
                    live.reports
                        .iter()
                        .find(|lr| lr.id == id)
                        .cloned()
                        .map(|lr| (lr, *sev))
                })
                .collect()
        } else {
            Vec::new()
        };
        let ship_ids: std::collections::HashSet<i64> =
            feed.iter().flat_map(|(r, _)| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
            ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
        let resolved_pilots: std::collections::HashMap<String, i64> = {
            let cache = self.pilots.lock().unwrap();
            feed.iter()
                .flat_map(|(r, _)| r.pilots.iter())
                .filter_map(|n| match cache.get(n) {
                    Some(Some(id)) => Some((n.clone(), id)),
                    _ => None,
                })
                .collect()
        };
        let status_snapshot = if active {
            self.system_status.lock().unwrap().clone()
        } else {
            Default::default()
        };
        let last_ship =
            if active { build_last_ship(&self.intel_state.lock().unwrap().reports) } else { Default::default() };
        let systems = self.systems.clone();
        let player_sys = self.player_system();
        let now_ts = chrono::Utc::now().timestamp();

        let on_top = self.settings.alerts.on_top != crate::settings::OnTop::Never
            && (self.settings.alerts.on_top == crate::settings::OnTop::Always || self.eve_focused);

        let mut hovered = false;
        let mut dismiss = false;
        let mut moved: Option<(f32, f32)> = None;
        let mut moved_size: Option<(f32, f32)> = None;
        let mut click: Option<IntelClick> = None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("alert_window"),
            egui::ViewportBuilder::default()
                .with_title("EVE Spai — alerts")
                .with_window_level(if on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                })
                .with_active(false)
                // Per-OS idle behaviour:
                // - Windows: close the window when idle (with_active(false) keeps the re-map
                //   from stealing focus via WS_EX_NOACTIVATE).
                // - Linux/X11: re-mapping a window ALWAYS steals focus there (winit maps with
                //   XMapRaised — winit#1160, and egui exposes no _NET_WM_USER_TIME / notification
                //   type to avoid it), so stay mapped and go transparent + click-through when
                //   idle instead of closing.
                .with_visible(if cfg!(target_os = "windows") { active } else { true })
                .with_decorations(false)
                .with_resizable(true)
                .with_taskbar(false)
                .with_transparent(true)
                .with_mouse_passthrough(!active)
                // Fixed initial geometry only. The *saved* position/size are applied via
                // ViewportCommand on open (just_opened); re-applying them every frame here
                // fought the user's drag/resize and made the window shake.
                .with_position([80.0, 80.0])
                .with_inner_size([360.0, 240.0]),
            |ctx, _| {
                if !active {
                    // Closed (with_visible=false) when idle — draw nothing.
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |_ui| {});
                    return;
                }
                // Re-apply the saved geometry when an alert appears (the builder values
                // aren't reliably re-applied after unmapping).
                if just_opened {
                    if let Some((w, h)) = self.settings.alerts.window_size {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                    }
                    if let Some((x, y)) = self.settings.alerts.window_pos {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                    }
                    self.alert_level_applied = None; // force the level to be re-applied
                }
                // Re-assert the level only when it changes or the window (re)opens — NOT
                // every frame. A viewport command each frame forces egui to repaint each
                // frame to process it, which pinned the whole app at vsync (~58 fps) for the
                // entire alert countdown. The window stays mapped now, so on-change is enough.
                if just_opened || self.alert_level_applied != Some(on_top) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }));
                    self.alert_level_applied = Some(on_top);
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(egui::Color32::from_rgb(0x12, 0x14, 0x18)).inner_margin(8))
                    .show(ctx, |ui| {
                        // The top row is a drag handle, up to where the buttons begin
                        // (so it doesn't sit on top of the X / pin hitboxes).
                        let mut buttons_left = f32::INFINITY;
                        let row = ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    format!("{}  Intel alerts", egui_phosphor::regular::DOTS_SIX),
                                )
                                .strong(),
                            );
                            ui.label(
                                egui::RichText::new(if self.alert_window_secs.is_finite() {
                                    format!("{:.0}s", self.alert_window_secs)
                                } else {
                                    "\u{221E}".to_owned() // ∞
                                })
                                .weak(),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(egui_phosphor::regular::X).on_hover_text("Dismiss").clicked() {
                                    dismiss = true;
                                }
                                if ui
                                    .add(
                                        egui::Button::new(egui_phosphor::regular::PUSH_PIN)
                                            .selected(self.alert_window_pinned),
                                    )
                                    .on_hover_text("Pin open (hold until closed)")
                                    .clicked()
                                {
                                    self.alert_window_pinned = !self.alert_window_pinned;
                                }
                                buttons_left = ui.min_rect().left();
                            });
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
                            for (r, sev) in &feed {
                                let from_you = jumps_from_you(
                                    &systems,
                                    player_sys,
                                    r.primary_system().map(|s| s.id),
                                );
                                let kc = self.kill_cache.clone();
                                if let Some(c) = intel_row(
                                    ui, r, now_ts, false, from_you, &systems, &status_snapshot,
                                    &ship_details, &ship_roles, &resolved_pilots, &last_ship, &kc, *sev,
                                    false,
                                ) {
                                    click = Some(c);
                                }
                            }
                        });
                        hovered = ui.ui_contains_pointer();
                        // Bottom-right resize grip (hover highlight + resize cursor).
                        resize_grip(ui);
                    });
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
                // Once the shared link has been opened/copied, close after 5 s without
                // focus, so it doesn't linger over the game.
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
        // A click in the feed opens the relevant window (in the main viewport).
        match click {
            Some(IntelClick::System(id)) => self.open_system(id),
            Some(IntelClick::Kill(kid)) => self.kill_window = Some(kid),
            Some(IntelClick::Ship(id)) => self.open_ship(id),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ctx.clone());
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            None => {}
        }
        // Save a moved position / resized size — but NOT on the open frame, where the window
        // briefly reports its builder default before the saved geometry is re-applied (which
        // would otherwise overwrite the saved position with the default on every open).
        if !just_opened {
            if let Some(p) = moved {
                if self.settings.alerts.window_pos != Some(p) && p.0 >= 0.0 && p.1 >= 0.0 {
                    self.settings.alerts.window_pos = Some(p);
                    self.needs_save = true;
                }
            }
            if let Some(s) = moved_size {
                let prev = self.settings.alerts.window_size;
                if prev.map_or(true, |(w, h)| (w - s.0).abs() > 2.0 || (h - s.1).abs() > 2.0) {
                    self.settings.alerts.window_size = Some(s);
                    self.needs_save = true;
                }
            }
        }
        if dismiss {
            self.alert_window_secs = 0.0;
            return;
        }
        if !active {
            return; // idle: nothing to count down; the window is closed
        }
        // Countdown (paused while hovered; floor of 3 s when hovered). Use unstable_dt:
        // it's the *true* time since the last frame. stable_dt is smoothed/clamped, so
        // after a ~1 s idle it reports a tiny value and the countdown barely moves (this
        // is why it ticked far too slowly). Cap a long idle gap at 2 s.
        let dt = ctx.input(|i| i.unstable_dt).min(2.0);
        if hovered {
            self.alert_window_secs = self.alert_window_secs.max(3.0);
        } else if !self.alert_window_pinned && self.alert_window_secs.is_finite() {
            self.alert_window_secs = (self.alert_window_secs - dt).max(0.0);
        }
        // The countdown uses real elapsed time, so off-hover we only need ~1 fps to
        // refresh the "Ns" label; full rate here would rebuild the main window's intel
        // feed many times a second (this drove the high CPU).
        let ms = if hovered { 100 } else { 1000 };
        ctx.request_repaint_after(std::time::Duration::from_millis(ms));
    }

    /// Persist map overlay + intel-filter options when they change.
    fn persist_view_options(&mut self) {
        let pv = PersistedView {
            // Persist the Standard layers, not a transient mode preset.
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

    /// Rebuild the system graph when the jump-bridge config changes, so stale
    /// bridge edges are removed from the map and routing at runtime.
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
        self.map_loaded = None; // reload map systems with the new connectivity
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

    /// Render the interactive map into `ui` (used in the main panel and the pop-out
    /// window). Full-panel canvas with floating controls.
    #[allow(deprecated)] // show_tooltip_at_pointer: replacement API is heavier
    fn draw_map(&mut self, ui: &mut egui::Ui) {
        use crate::map::MapView;
        if self.map_regions.is_empty() {
            if let Some(store) = &self.store {
                self.map_regions = store.regions();
            }
        }
        // Per-system character presence: system id → (count, includes the active char).
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
            (p.system_id, here)
        };
        if !self.map_initialized {
            // Open on the full universe map (in-game style); navigate in from there.
            self.map_view = MapView::Universe;
            self.map_initialized = true;
        }

        // Follow: keep the view on the player's region.
        if self.map_follow {
            if let (MapView::Region(r), Some(psys)) = (self.map_view, player_sys) {
                if let Some(pr) = self.store.as_ref().and_then(|s| s.region_of_system(psys)) {
                    if pr != r {
                        self.map_view = MapView::Region(pr);
                    }
                }
            }
        }

        // Threat views (radial / tree) are laid out from a centre system by jumps;
        // they don't use the geographic projection at all.
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

        // (Re)load systems for the current view, keeping only gate-connected systems
        // (drops wormhole / abyssal islands that have no K-space connections).
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
                            // Hide permanently inaccessible regions (e.g. UUA-F4).
                            g.info_of(s.id).map(|i| !is_hidden_region(&i.region)).unwrap_or(true)
                        })
                        .collect()
                } else {
                    raw
                };
            }
            self.map_loaded = Some(self.map_view);
        }

        // Drawn coordinates: EVE's flattened 2D layout (position2D) when "Spaced" is
        // on, else raw geographic x/z. The 2D coords are baked, so this is instant.
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

        // Overlay mode fades the whole map to the configured opacity.
        if self.map_overlay_mode {
            ui.set_opacity(self.settings.map_overlay_opacity.clamp(0.2, 1.0));
        }
        let rect = ui.available_rect_before_wrap();
        // On window resize, rescale the pan so the same world point stays centred.
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

        // Mouse back/forward buttons.
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1)) {
            self.map_back();
        }
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra2)) {
            self.map_forward_nav();
        }
        // Drag pans (and disables follow) — unless an overlay window-move is active.
        if resp.dragged() && !self.map_overlay_drag {
            self.map_pan += resp.drag_delta();
            self.map_follow = false;
        }
        if !resp.dragged() {
            self.map_overlay_drag = false;
        }
        // Zoom centred on the cursor.
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
                    let old = self.map_zoom;
                    // Min ~= fit-to-view (can't shrink past the whole map); max lets
                    // individual systems separate.
                    let new = (old * (scroll * 0.003).exp()).clamp(0.7, 60.0);
                    let q = cursor - (rect.center() + self.map_pan);
                    self.map_pan += q * (1.0 - new / old);
                    self.map_zoom = new;
                }
            }
        }
        // Follow: centre the player's system.
        if self.map_follow {
            if let Some(ps) = player_sys.and_then(|id| self.map_draw.iter().find(|s| s.id == id)) {
                let base = crate::map::project(ps.x, ps.z, &bounds, rect, self.map_zoom, egui::Vec2::ZERO);
                self.map_pan = rect.center() - base;
            }
        }

        // Project all systems.
        let mut pos: std::collections::HashMap<i64, egui::Pos2> = std::collections::HashMap::new();
        for s in &self.map_draw {
            pos.insert(s.id, crate::map::project(s.x, s.z, &bounds, rect, self.map_zoom, self.map_pan));
        }

        // One-shot focus from an intel click.
        if let Some(fid) = self.map_focus.take() {
            if let Some(s) = self.map_draw.iter().find(|s| s.id == fid) {
                let base = crate::map::project(s.x, s.z, &bounds, rect, self.map_zoom, egui::Vec2::ZERO);
                self.map_pan = rect.center() - base;
            }
        }

        // Overlay mode: a drag that doesn't start on a system moves the window.
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

        // Click a system: open its info window.
        if resp.clicked() {
            if let Some(click) = ui.input(|i| i.pointer.interact_pos()) {
                if let Some(id) = nearest_system(click, &pos, 10.0) {
                    self.open_system(id);
                }
            }
        }

        // Right-click a system: context menu (destination / waypoint / jump route).
        if resp.secondary_clicked() {
            self.ctx_menu_system =
                ui.input(|i| i.pointer.interact_pos()).and_then(|p| nearest_system(p, &pos, 10.0));
        }
        let ctx_sys = self.ctx_menu_system;
        resp.context_menu(|ui| {
            let Some(sid) = ctx_sys else {
                ui.close();
                return;
            };
            if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(sid)) {
                ui.label(egui::RichText::new(&info.name).strong());
            }
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
            ui.separator();
            if ui.button("Plan Jump Route From Here").clicked() {
                self.jump_plan_from = Some(sid);
                ui.close();
            }
            if ui.button("Plan Jump Route To Here").clicked() {
                self.jump_plan_to = Some(sid);
                ui.close();
            }
            if self.map_mode == MapMode::Travel {
                ui.separator();
                if ui.button("Travel: set as start").clicked() {
                    self.travel_start = Some(sid);
                    self.travel_start_q.clear();
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
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        // Small uniform dots like the in-game star map.
        let dot = (0.7 * self.map_zoom).clamp(0.6, 2.2);
        // Offsets for labels / sov-upgrade icons are in screen pixels, so when zoomed
        // out (systems crowd together) a fixed gap makes them drift onto neighbours.
        // Shrink the gap as we zoom out (full size once reasonably zoomed in).
        let label_off = (self.map_zoom / 8.0).clamp(0.35, 1.0);

        // Sovereignty territory: opaque filled regions per holder. Drawing opaque
        // (rather than translucent) means same-colour overlaps merge into one
        // uniform region instead of darkening per system. Only player-sov nullsec
        // is coloured — NPC sov (no alliance) and hi/low-sec are left clear.
        if self.map_overlays.sov != SovMode::Off {
            // Adaptive radius from the median gate-edge length on screen.
            let mut edge_len: Vec<f32> = Vec::new();
            if let Some(graph) = &self.systems {
                for s in self.map_draw.iter().take(600) {
                    let p1 = pos[&s.id];
                    for &n in graph.neighbors(s.id) {
                        if s.id < n {
                            if let Some(p2) = pos.get(&n) {
                                edge_len.push(p1.distance(*p2));
                            }
                        }
                    }
                }
            }
            edge_len.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let terr =
                edge_len.get(edge_len.len() / 2).copied().unwrap_or(dot * 6.0).max(dot * 3.0) * 0.72;
            // Muted, opaque region colour (keeps dots/labels readable on top).
            let region = |c: egui::Color32| {
                egui::Color32::from_rgb(
                    (c.r() as f32 * 0.5) as u8,
                    (c.g() as f32 * 0.5) as u8,
                    (c.b() as f32 * 0.5) as u8,
                )
            };
            let status = self.system_status.lock().unwrap();
            for s in &self.map_draw {
                let Some(f) = status.get(&s.id) else { continue };
                // Player sovereignty only (NPC sov has no alliance id).
                if f.sov_alliance.is_none() {
                    continue;
                }
                let Some(name) = f.sov.as_deref() else { continue };
                let col = match self.map_overlays.sov {
                    SovMode::Alliance => self.alliance_paint(name),
                    SovMode::Coalition => self
                        .settings
                        .coalitions
                        .iter()
                        .find(|c| c.alliances.iter().any(|a| a.eq_ignore_ascii_case(name)))
                        .map(Self::coalition_paint)
                        .unwrap_or(egui::Color32::from_rgb(0x60, 0x60, 0x60)), // independent
                    SovMode::Off => continue,
                };
                painter.circle_filled(pos[&s.id], terr, region(col));
            }
        }

        // Search highlight: faint background on systems that have a chosen upgrade.
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

        // Configured jump bridges (drawn distinctly, in green, like in-game).
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

        // Cull anything whose bounding box is off-screen — most of the ~13k edges / ~5k nodes
        // are outside the viewport when zoomed in, and drawing them just wastes tessellation.
        let cull = rect.expand(8.0);
        let seg_visible = |a: egui::Pos2, b: egui::Pos2| egui::Rect::from_two_pos(a, b).intersects(cull);

        // Gate links (each pair once); bridges are drawn separately below.
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

        // Wormhole overlay: direct k-space↔k-space holes (teal), chains through
        // J-space (purple, dashed, labelled with the J-space hop count), and a spiral
        // marker on systems that hold a hole into (disconnected) J-space.
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
            for sid in &self.wh_overlay.jspace_holes {
                if let Some(p) = pos.get(sid) {
                    painter.text(
                        *p + egui::vec2(0.0, -dot - 6.0),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::SPIRAL,
                        egui::FontId::proportional(12.0),
                        wh_col,
                    );
                }
            }
            // Thera isn't on the k-space map: place it near its in-view connections
            // (clamped just inside the map) and draw its holes.
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
                    // Stable WORLD position (above the centroid) so it pans/zooms with the map.
                    let mut cx = conns.iter().map(|s| s.x).sum::<f64>() / conns.len() as f64;
                    let min_z = conns.iter().map(|s| s.z).fold(f64::INFINITY, f64::min);
                    let max_z = conns.iter().map(|s| s.z).fold(f64::NEG_INFINITY, f64::max);
                    let mut tz = min_z - (max_z - min_z).max(1.0) * 0.25;
                    // In the 2D layout, anchor Thera between Cobalt Edge and Tenal (its
                    // in-game map location) when both regions are in view.
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
                    let line_col = egui::Color32::from_rgb(0x6E, 0xC8, 0xF0); // blue links
                    let tcol = egui::Color32::from_rgb(0xB0, 0x70, 0xE0); // purple: J-space -1.0
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
            // Turnur badge (the system stays on the map; this just marks it).
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

        // Map overlays (ADM / activity / sov upgrades) as rings/markers behind dots.
        // (Sovereignty territory is drawn separately, below.)
        let ov = self.map_overlays;
        let zoomed = self.map_zoom > 3.0;
        if ov.adm || ov.activity != ActivityMode::Off || ov.upgrades {
            let status = self.system_status.lock().unwrap();
            let mut upgrades_by_system: std::collections::HashMap<String, Vec<&str>> =
                std::collections::HashMap::new();
            if ov.upgrades {
                for u in &self.settings.sov_upgrades {
                    upgrades_by_system
                        .entry(u.system.to_lowercase())
                        .or_default()
                        .push(u.upgrade.as_str());
                }
            }
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
                            // Colored backdrop behind the system (not a ring).
                            painter.circle_filled(p, dot + 7.0, c.gamma_multiply(0.30));
                        }
                    }
                    if ov.activity != ActivityMode::Off {
                        let v = ov.activity.value(f);
                        if v > 0 {
                            let heat = (v as f32 / ov.activity.scale()).min(1.0);
                            let col =
                                egui::Color32::from_rgb(0xFF, (0xC0 as f32 * (1.0 - heat)) as u8, 0x30);
                            painter.circle_filled(p, dot + 3.0 + heat * 6.0, col.gamma_multiply(0.32));
                        }
                    }
                }
                // Sov upgrades: specific, level-coloured icons near the system when
                // zoomed in (mineral image for mining, skull for ratting, dish for
                // exploration). Not a ring.
                if ov.upgrades && zoomed {
                    if let Some(ups) = upgrades_by_system.get(&s.name.to_lowercase()) {
                        // One stored label can list several comma-separated upgrades
                        // ("...Array 3, Exploration Detector 3") — draw an icon for each.
                        let ukinds = self.upgrade_kinds;
                        let parts: Vec<&str> = ups
                            .iter()
                            .flat_map(|u| split_upgrade_label(u))
                            .filter(|up| ukinds[upgrade_kind(up) as usize])
                            .collect();
                        for (k, up) in parts.iter().take(6).enumerate() {
                            // Sit the icons in a row above the system name.
                            let ip = p + egui::vec2(6.0 * label_off + k as f32 * 20.0, -15.0 * label_off);
                            // Skip icons that would spill onto a dock — the mineral image uses
                            // ui.put (not the rect-clipped painter), so it isn't clipped.
                            if ip.x + 20.0 > rect.right() || ip.y - 20.0 < rect.top() || !rect.contains(ip) {
                                continue;
                            }
                            let (kind, level) = upgrade_info(up);
                            let lcol = level_color(level);
                            match kind {
                                UpgradeIcon::Glyph(g) => {
                                    painter.text(
                                        ip,
                                        egui::Align2::LEFT_BOTTOM,
                                        g,
                                        egui::FontId::proportional(16.0),
                                        lcol,
                                    );
                                }
                                UpgradeIcon::Mineral(tid) => {
                                    let sz = 19.0;
                                    let rect = egui::Rect::from_min_size(
                                        egui::pos2(ip.x, ip.y - sz),
                                        egui::Vec2::splat(sz),
                                    );
                                    let url =
                                        format!("https://images.evetech.net/types/{tid}/icon?size=64");
                                    ui.put(rect, egui::Image::new(url)).on_hover_text(*up);
                                    // Level indicator dot (top-right corner).
                                    painter.circle_filled(rect.right_top(), 3.0, lcol);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Player route: animated dashed line flowing toward the destination. Clears itself
        // once the active character reaches the destination.
        let mut reached_dest = false;
        if let (Some(dest), Some(ps), Some(graph)) =
            (self.route_destination, player_sys, self.systems.as_ref())
        {
            if ps == dest {
                reached_dest = true;
            } else if let Some(route) = graph.path(ps, dest) {
                let phase = (ui.input(|i| i.time) * 28.0) as f32;
                let route_col = egui::Color32::from_rgb(0x4F, 0xC3, 0xF7);
                for w in route.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        dashed_flow(&painter, *p1, *p2, route_col, phase);
                    }
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(33)); // dashes
            }
        }
        if reached_dest {
            self.route_destination = None;
        }

        // Gate-camp markers from the live kill feed: a red campfire above the system.
        if self.map_overlays.camps {
            let now = chrono::Utc::now().timestamp();
            if now - self.camped_cache_at >= 2 {
                self.camped_cache = self.camps.lock().unwrap().camped(now);
                self.camped_cache_at = now;
            }
            let red = egui::Color32::from_rgb(0xEF, 0x44, 0x44);
            let font = egui::FontId::proportional(15.0);
            for id in &self.camped_cache {
                if let Some(p) = pos.get(id) {
                    painter.text(
                        *p + egui::vec2(0.0, -11.0),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::CAMPFIRE,
                        font.clone(),
                        red,
                    );
                }
            }
        }

        // Travel Mode: the planned route legs + square markers on the start / waypoints /
        // destination (shown even before a route is computed).
        if self.map_mode == MapMode::Travel {
            let cyan = egui::Color32::from_rgb(0x4F, 0xC3, 0xF7);
            // The game's direct gate route, dimmer and behind the planned one for comparison.
            if let Some(direct) = &self.travel_direct_route {
                let gray = egui::Color32::from_rgb(0x9E, 0x9E, 0x9E);
                for w in direct.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, gray));
                    }
                }
            }
            // Live Mode: the route as first planned, dimmed in purple for comparison.
            if let Some(base) = self.travel_live.then_some(self.travel_live_base.as_ref()).flatten() {
                let purple = egui::Color32::from_rgb(0x95, 0x75, 0xCD);
                for w in base.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, purple));
                    }
                }
            }
            if let Some(route) = &self.travel_route {
                for w in route.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(2.5, cyan));
                    }
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
                mark(*p, egui::Color32::from_rgb(0x66, 0xBB, 0x6A)); // start — green
            }
            if let Some(p) = self.travel_end.and_then(|e| pos.get(&e)) {
                mark(*p, egui::Color32::from_rgb(0xFF, 0xA7, 0x26)); // destination — amber
            }
            // Blink systems newly added by a live re-plan, for a few seconds.
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

        // Jump-range hover. Distances are always true light-years (real coords);
        // in schematic mode we keep the band-coloured highlights but drop the rings
        // (the on-screen distances aren't metric there).
        let hovered_id = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| resp.hovered())
            .and_then(|p| nearest_system(p, &pos, 8.0));
        if let (true, Some(h_id)) = (self.map_overlays.jump_range, hovered_id) {
            if let Some(real_h) = self.map_systems.iter().find(|s| s.id == h_id) {
                let hp = pos[&h_id];
                // One colour per band (capital / black ops / jump freighter).
                let band_color = [
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A), // capital — green
                    egui::Color32::from_rgb(0xE0, 0xA4, 0x3A), // black ops — amber
                    egui::Color32::from_rgb(0xD8, 0x4C, 0x4C), // jump freighter — red
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
                // Highlight each in-range system in the colour of the tightest band.
                // map_draw and map_systems share order, so index zips draw↔real.
                for (i, s) in self.map_draw.iter().enumerate() {
                    if s.id == h_id {
                        continue;
                    }
                    let d = crate::map::ly_distance(real_h, &self.map_systems[i]);
                    if let Some(b) = crate::map::JUMP_RANGES.iter().position(|(_, ly)| d <= *ly) {
                        let col = band_color.get(b).copied().unwrap_or(band_color[2]);
                        // Faint backglow behind the dot (drawn on top later).
                        painter.circle_filled(pos[&s.id], dot + 4.0, col.gamma_multiply(0.30));
                    }
                }
            }
        }

        // Hover tooltip: system info + ESI activity + intel for the hovered system.
        if let Some(h_id) = hovered_id {
            let layer = ui.layer_id();
            egui::show_tooltip_at_pointer(ui.ctx(), layer, ui.id().with("map_hover_tip"), |ui| {
                self.map_system_tooltip(ui, h_id);
            });
        }

        // Systems + overlays. Per system, the highest active severity + latest time.
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
        // Blink phase for fresh intel (≈3 Hz).
        let blink = (ui.input(|i| i.time) as f32 * 6.0).sin().abs();
        let mut any_fresh = false;
        // System labels: always on when viewing a single region, otherwise once zoomed in
        // a bit (a collision check below still drops any that would overlap).
        let show_sys_labels =
            matches!(self.map_view, MapView::Region(_)) || self.map_zoom >= 12.0;
        let mut placed_labels: Vec<egui::Rect> = Vec::new();
        for s in &self.map_draw {
            let p = pos[&s.id];
            if !cull.contains(p) {
                continue; // off-screen system — skip its dot, rings and label
            }
            painter.circle_filled(p, dot, security_color(s.security));
            if let Some((sev, received)) = intel_map.get(&s.id) {
                let base = severity_color(*sev);
                let fresh = now_ts - received < 15;
                let (fill_a, ring_w) = if fresh {
                    any_fresh = true;
                    (0.45 + 0.45 * blink, 3.0)
                } else {
                    (0.40, 2.5)
                };
                // A solid glow behind the dot + an opaque severity-coloured ring.
                painter.circle_filled(p, dot + 5.0, base.gamma_multiply(fill_a));
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(ring_w, base));
            }
            if let Some((count, has_active)) = char_here.get(&s.id) {
                // Regular blue when the active char is here; light blue for other
                // characters only. A larger ring than the red intel ring so they coexist.
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
            if show_sys_labels && rect.contains(p) {
                // Name sits next to the dot; sov-upgrade icons sit above it.
                let anchor = p + egui::vec2(6.0 * label_off, -2.0 * label_off);
                let approx = egui::Rect::from_min_size(
                    anchor,
                    egui::vec2(s.name.len() as f32 * 7.0, 14.0),
                );
                if !placed_labels.iter().any(|r| r.expand(2.0).intersects(approx)) {
                    placed_labels.push(approx);
                    painter.text(
                        anchor,
                        egui::Align2::LEFT_CENTER,
                        &s.name,
                        egui::FontId::proportional(13.0),
                        ui.visuals().text_color(),
                    );
                }
            }
        }
        // Keep animating while any fresh intel is blinking.
        if any_fresh {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }

        // Low zoom: label regions (centroid) instead of every system.
        if !show_sys_labels {
            let mut acc: std::collections::HashMap<i64, (egui::Vec2, u32)> =
                std::collections::HashMap::new();
            for s in &self.map_draw {
                let e = acc.entry(s.region_id).or_insert((egui::Vec2::ZERO, 0));
                e.0 += pos[&s.id].to_vec2();
                e.1 += 1;
            }
            // Deterministic order (HashMap iteration flickers which label wins) and
            // skip any label that would overlap an already-placed one (no z-fighting).
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
                // Always draw every region name (overlap is acceptable — the user wants
                // all of them visible, not collision-pruned).
                // Shadow for legibility over the starfield, then a bright label.
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

    /// Radial / tree "threat" view: lay systems out by jumps from a centre system
    /// (the active character, or one chosen by right-click), out to N jumps.
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
                // Keep the point under the cursor fixed (the layout spreads from rect.center()+pan,
                // scaled by zoom), so scrolling zooms toward the mouse rather than the centre.
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
                "No centre system — set an active character, or right-click a system on the map.",
                egui::FontId::proportional(13.0),
                visuals.weak_text_color(),
            );
            return;
        };
        let depth = self.map_threat_jumps.max(1);

        // BFS tree from the centre.
        let (dist, children, order) = bfs_tree(&graph, center, depth, self.threat_include_bridges);
        let leaves = order.iter().filter(|id| children.get(id).map_or(true, |c| c.is_empty())).count();
        let mut frac: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
        let mut next = 0u32;
        assign_fracs(center, &children, leaves.max(1) as f32, &mut next, &mut frac);

        // Lay out.
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
                // Tree: root at top, levels descend, leaves spread across the width.
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

        // Per-system: highest active severity + latest time (for a graded glow).
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

        // Edges (each undirected pair once). Jump bridges are drawn green.
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

        // Labels: out to 3 jumps; the outermost labelled ring is height-staggered to
        // reduce overlaps; further-out systems reveal their name only on hover.
        let label_max = 3;
        let line_h = 13.0;
        // Stagger only helps the Tree layout, where the outer level is a single horizontal row
        // that crowds. In Radial the ring is spread around a full circle, so each label already
        // clears its neighbours — staggering there just floats labels off their dots.
        let stagger: std::collections::HashMap<i64, f32> = if matches!(self.map_layout, MapLayout::Radial) {
            std::collections::HashMap::new()
        } else {
            let mut ring: Vec<i64> = order.iter().copied().filter(|id| dist[id] == label_max).collect();
            ring.sort_by(|a, b| frac[a].partial_cmp(&frac[b]).unwrap_or(std::cmp::Ordering::Equal));
            ring.iter().enumerate().map(|(i, id)| (*id, (i % 3) as f32 * line_h)).collect()
        };
        let hovered = ui.input(|i| i.pointer.hover_pos()).and_then(|hp| nearest_system(hp, &pos, 12.0));

        // Nodes.
        let node_r = (5.5 * zoom.clamp(0.6, 1.6)).max(3.5);
        let font = egui::FontId::proportional((12.0 * zoom).clamp(9.0, 15.0));
        for &id in &order {
            let p = pos[&id];
            let info = graph.info_of(id);
            let sec = info.map(|i| i.security).unwrap_or(0.0);
            let is_center = id == center;
            let r = if is_center { node_r + 2.5 } else { node_r };
            // Intel: a soft glow + an opaque severity-coloured ring (as in 2D/3D),
            // brighter/blinking while fresh.
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
            // Hover highlight.
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

        // Hovered far-out system: reveal its name with a small backdrop.
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

        // Interaction: left-click opens a system; right-click re-centres on it.
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.clicked() {
            if let Some(id) = pointer.and_then(|p| nearest_system(p, &pos, 12.0)) {
                self.open_system(id);
            }
        }
        if resp.secondary_clicked() {
            if let Some(id) = pointer.and_then(|p| nearest_system(p, &pos, 12.0)) {
                self.map_threat_center = Some(id);
                self.map_pan = egui::Vec2::ZERO;
                self.map_zoom = 1.0;
            }
        }

        // Title chip: centre name + jumps.
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

    /// The map control overlays (or the minimal overlay-mode bar / hidden state).
    fn map_chrome(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        if self.map_overlay_mode {
            self.map_overlay_controls(ui, rect);
        } else if self.map_controls_hidden {
            // Just a small button to bring the controls back.
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
            // Reopen buttons for minimized docks.
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
            if self.map_mode != MapMode::Standard && !self.right_dock_open {
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
            // Controls + layers + legend live in the left dock (see map_area); the search is
            // the only thing still floating over the map.
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
        ui.checkbox(&mut self.map_overlays.thera, format!("{}  Thera", icon::PLANET));
        ui.checkbox(&mut self.map_overlays.turnur, format!("{}  Turnur", icon::PLANET));
        ui.checkbox(&mut self.map_overlays.camps, format!("{}  Gate camps", icon::CAMPFIRE));
        if ui
            .checkbox(&mut self.settings.kill_intel, format!("{}  Kill-feed intel", icon::SKULL))
            .on_hover_text("Show zKill killmails within range as intel cards")
            .changed()
        {
            self.needs_save = true;
        }
        if self.settings.kill_intel {
            ui.indent("kill_intel_range", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Range");
                    if ui
                        .add(egui::DragValue::new(&mut self.settings.kill_intel_jumps).range(1..=20).suffix("j"))
                        .changed()
                    {
                        self.needs_save = true;
                    }
                });
            });
        }
        if ui
            .checkbox(&mut self.settings.route_via_wormholes, format!("{}  Route via wormholes", icon::SPIRAL))
            .on_hover_text("Set Destination adds a waypoint at each hole entrance")
            .changed()
        {
            self.needs_save = true;
        }
        // Sov-upgrade icon legend (only meaningful while that overlay is on).
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

    /// Hover tooltip for a map system: name/security/location, ESI activity, and
    /// any current intel. (Click the system for the full interactive window.)
    /// Wormhole connections in/out of a system (from the cached store), shown in the
    /// map tooltip and the system-info window. No-op if the system has no known holes.
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

    /// A gate-camp warning line (red campfire) for `id`, if it's currently flagged. Shared by
    /// the map tooltip and the system-info window.
    fn camp_line(&self, ui: &mut egui::Ui, id: i64) {
        let now = chrono::Utc::now().timestamp();
        if let Some(c) = self.camps.lock().unwrap().camp(id, now) {
            let mins = (c.age / 60).max(0);
            ui.label(
                egui::RichText::new(format!(
                    "{}  Gate camp \u{2014} {} kills, last {}m ago",
                    egui_phosphor::regular::CAMPFIRE,
                    c.kills,
                    mins
                ))
                .strong()
                .color(egui::Color32::from_rgb(0xEF, 0x44, 0x44)),
            );
        }
    }

    fn map_system_tooltip(&self, ui: &mut egui::Ui, id: i64) {
        // Compact + translucent so it doesn't hide nearby/jumpable systems.
        ui.set_max_width(270.0);
        ui.set_opacity(0.82);
        let status = self.system_status.lock().unwrap();
        let flags = status.get(&id).cloned().unwrap_or_default();
        if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(id)) {
            ui.horizontal(|ui| {
                ui.label(security_badge(info.security));
                ui.label(egui::RichText::new(&info.name).strong());
                // Sov alliance logo, top-right (instead of a "Sov:" text chip).
                if let Some(aid) = flags.sov_alliance {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let url = format!("https://images.evetech.net/alliances/{aid}/logo?size=64");
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

        // Current intel for this system (compact).
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
                ui.label(egui::RichText::new(format!("— {}", r.reporter)).weak());
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

    /// Minimal overlay-mode controls: exit, lock, smart-on-top, opacity. When
    /// locked, only an unlock button shows (everything else hidden).
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
                            // Re-center is still useful while locked.
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
                                egui::Slider::new(&mut self.settings.map_overlay_opacity, 0.3..=1.0)
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

    /// Floating controls over the map (scope, navigation, follow, pop-out).
    /// The active character's current system — from the per-character location map, so the
    /// map and distances follow the character selected at the top of the window, not
    /// whichever character ESI happened to update last.
    fn player_system(&self) -> Option<i64> {
        let p = self.player.lock().unwrap();
        p.locations.get(&self.active_character).map(|(s, _)| *s).or(p.system_id)
    }

    /// Switch map mode, auto-adapting the overlays (saving/restoring the Standard layers).
    fn set_map_mode(&mut self, new: MapMode) {
        if new == self.map_mode {
            return;
        }
        if self.map_mode == MapMode::Standard {
            self.standard_overlays = self.map_overlays; // remember the user's layers
        }
        self.map_overlays = if new == MapMode::Standard {
            self.standard_overlays
        } else {
            new.overlay_preset()
        };
        self.map_mode = new;
        self.needs_save = true;
    }

    /// Compute the Travel route from the typed start/end + the active constraints.
    /// The next system on the planned route after the character's current position (None at the
    /// end or with no route). Used to advance the in-game destination one hop at a time.
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

    /// Set the in-game destination to the next hop on the route (only when it changes), so EVE
    /// follows our exact path without ever needing a duplicate waypoint.
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

    /// Hash of the inputs that affect the planned route, for the re-plan debounce.
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
            self.camps.lock().unwrap().camped(now).into_iter().collect()
        } else {
            std::collections::HashSet::new()
        };
        let regional = self.travel_regional_gates;
        let bridges = self.travel_jump_bridges;
        let geo2 = geo.clone(); // a second handle so the node-mask can read security
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
            let sec_ok = geo2
                .info_of(sys)
                .map(|i| {
                    if i.security >= 0.45 {
                        sec[0] // high
                    } else if i.security > 0.0 {
                        sec[1] // low
                    } else {
                        sec[2] // null
                    }
                })
                .unwrap_or(true);
            let activity_ok =
                max_kills == 0 || status.get(&sys).map(|f| metric.value(f)).unwrap_or(0) <= max_kills;
            sec_ok && activity_ok
        };
        // Stitch each leg start -> wp1 -> ... -> end; an unreachable leg invalidates the route.
        let mut route = vec![s];
        let mut ok = true;
        for leg in points.windows(2) {
            match geo.route(leg[0], leg[1], regional, bridges, &allowed) {
                Some(seg) => route.extend(seg.into_iter().skip(1)),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        let prev = self.travel_route.clone();
        self.travel_route = ok.then_some(route);
        // The game's own shortest gate route (all stargates, no bridges or constraints), shown
        // in a different colour so the player can compare.
        self.travel_direct_route = geo.route(s, e, true, false, |_| true);
        // Live Mode: a deviation (new systems vs the previous route) re-routes in-game, blinks
        // the changed legs, and warns aloud when the detour is much longer.
        if self.travel_live {
            if let (Some(p), Some(n)) = (&prev, &self.travel_route) {
                if p != n {
                    let pset: std::collections::HashSet<i64> = p.iter().copied().collect();
                    let newsys: Vec<i64> = n.iter().copied().filter(|s| !pset.contains(s)).collect();
                    if !newsys.is_empty() {
                        let much_longer = n.len() > p.len() + 4;
                        self.travel_changed = newsys;
                        self.travel_changed_at = Some(chrono::Utc::now().timestamp());
                        // The in-game destination is advanced hop-by-hop by push_ingame_dest;
                        // here we only flag the change visually and warn on a big detour.
                        if much_longer {
                            crate::sound::play("danger");
                        }
                    }
                }
            }
        }
        self.travel_planned_hash = self.travel_input_hash();
        self.travel_dirty_at = None;
    }

    /// Travel Mode side panel: start/end + constraints + a planned, summarised route.
    /// Search systems for the From/To dropdowns: (id, name, security, constellation, region).
    /// Empty when the query is blank or already exactly names the picked system.
    fn travel_suggestions(&self, q: &str, picked: Option<i64>) -> Vec<SysHit> {
        let q = q.trim();
        if q.is_empty() {
            return Vec::new();
        }
        // Already exactly the picked system → no dropdown (and skip the SDE table scan).
        if let Some(pid) = picked {
            if self
                .systems
                .as_ref()
                .and_then(|g| g.info_of(pid))
                .is_some_and(|i| i.name.eq_ignore_ascii_case(q))
            {
                return Vec::new();
            }
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

    /// Travel Mode panel content, rendered inside a docked SidePanel (see `map_area`).
    fn travel_panel_content(&mut self, ui: &mut egui::Ui) {
        // A field with a keyboard-navigable suggestion dropdown (system, sec, const, region).
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
            if !suggestions.is_empty() {
                let focused = resp.has_focus();
                let n = suggestions.len();
                if focused {
                    let (down, up) = ui.input(|i| {
                        (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp))
                    });
                    if down {
                        *sel = (*sel + 1).min(n - 1);
                    }
                    if up {
                        *sel = sel.saturating_sub(1);
                    }
                }
                // While the pointer is actually moving, hovering a row takes over the
                // highlight, so Enter accepts whichever row the mouse is over.
                let moving = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    for (i, (id, name, sec, c, r)) in suggestions.iter().enumerate() {
                        let row = format!("{name}    {sec:.1}\n{c} \u{2022} {r}");
                        let resp = ui.selectable_label(i == *sel, row);
                        if resp.hovered() && moving {
                            *sel = i;
                        }
                        if resp.clicked() {
                            pick = Some(*id);
                        }
                    }
                });
                if focused && pick.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    pick = suggestions.get((*sel).min(n - 1)).map(|x| x.0);
                }
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
            let s0 = self.travel_suggestions(&self.travel_start_q, self.travel_start);
            let s1 = self.travel_suggestions(&self.travel_end_q, self.travel_end);
            self.travel_sugg = (s0, s1);
            self.travel_sugg_key = key;
        }
        let start_suggestions = self.travel_sugg.0.clone();
        let end_suggestions = self.travel_sugg.1.clone();
        if self.travel_wp_q != self.travel_wp_sugg_key {
            self.travel_wp_sugg = self.travel_suggestions(&self.travel_wp_q, None);
            self.travel_wp_sugg_key = self.travel_wp_q.clone();
        }
        let wp_suggestions = self.travel_wp_sugg.clone();
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
            match self.travel_direct_route.as_ref().map(|d| d.len().saturating_sub(1)) {
                Some(direct) if planned > direct => {
                    format!("{planned} jumps \u{2022} direct {direct} (+{})", planned - direct)
                }
                _ => format!("{planned} jumps"),
            }
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
            ui.checkbox(&mut self.travel_regional_gates, "Region-crossing gates");
            ui.checkbox(&mut self.travel_jump_bridges, "Jump bridges");
            ui.checkbox(&mut self.travel_avoid_camps, "Avoid gate camps");
            ui.horizontal(|ui| {
                ui.label("Sec");
                ui.checkbox(&mut self.travel_sec[0], "Hi");
                ui.checkbox(&mut self.travel_sec[1], "Lo");
                ui.checkbox(&mut self.travel_sec[2], "Null");
            });
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
                        egui::RichText::new("Set a from / to \u{2014} the route updates automatically.")
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
                // Dedup: EVE rejects a repeated waypoint, and a route can revisit a system when
                // waypoint legs overlap. (For revisiting routes, Live mode follows the exact path
                // hop by hop instead.)
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
            self.travel_start = None;
            self.travel_end = None;
            self.travel_start_q.clear();
            self.travel_end_q.clear();
            self.travel_waypoints.clear();
            self.travel_avoid.clear();
            self.travel_route = None;
            self.travel_direct_route = None;
        }
        // Live Mode: track the character's current system as the start and re-plan on a timer so
        // changing live data (camps, kills, sov) is picked up.
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
            self.push_ingame_dest();
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(900));
        } else {
            self.travel_live_base = None;
            self.travel_ingame_dest = None;
        }
        // Force the activity overlay to match the route's metric while in Travel mode, so the
        // heat the route reacts to is always visible.
        self.map_overlays.activity = self.travel_metric;
        // Auto-replan: a short debounce after the inputs settle (no Plan button). plan_route
        // stamps travel_planned_hash, so discrete actions (picks/right-click) stay instant.
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

    /// Safety Mode panel: a live, colour-coded read of threats within the threat-view jump
    /// range of the active character \u{2014} nearby non-clear intel as mini-cards and recent
    /// ESI kill hotspots, nearest first. The range is shared with the radial/tree views.
    fn safety_panel_content(&mut self, ui: &mut egui::Ui) {
        let red = egui::Color32::from_rgb(0xEF, 0x53, 0x50);
        let orange = egui::Color32::from_rgb(0xFF, 0xA7, 0x26);
        let yellow = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
        let green = egui::Color32::from_rgb(0x66, 0xBB, 0x6A);
        let prox = |j: u32| if j <= 1 { red } else if j <= 3 { orange } else { yellow };

        ui.add_space(6.0);
        ui.label(egui::RichText::new("Safety watch").strong().size(15.0));
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Watch range");
            ui.add(egui::DragValue::new(&mut self.map_threat_jumps).range(1..=15).suffix("j"));
        });
        ui.label(egui::RichText::new("Shared with the radial / tree view range.").weak());

        struct Threat {
            name: String,
            jumps: u32,
            count: Option<u32>,
            ships: Vec<String>,
            pilots: Vec<String>,
            received: i64,
            camp: bool,
            spike: bool,
        }
        let me_sys = self.player_system();
        let range = self.map_threat_jumps;
        let now = chrono::Utc::now().timestamp();
        let mut threats: Vec<Threat> = Vec::new();
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
                            threats.push(Threat {
                                name: sys.name.clone(),
                                jumps: j,
                                count: r.count,
                                ships: r.classes.clone(),
                                pilots: r.pilots.clone(),
                                received: r.received,
                                camp: r.camp,
                                spike: r.spike,
                            });
                        }
                    }
                }
            }
            // Nearest first, most-recent per system; one card per system.
            threats.sort_by(|a, b| a.jumps.cmp(&b.jumps).then(b.received.cmp(&a.received)));
            let mut seen = std::collections::HashSet::new();
            threats.retain(|t| seen.insert(t.name.clone()));
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
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let danger = !threats.is_empty();
            ui.label(
                egui::RichText::new(format!("Intel within {range}j: {}", threats.len()))
                    .strong()
                    .size(14.0)
                    .color(if danger { red } else { green }),
            );
            for t in &threats {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&t.name).strong().color(prox(t.jumps)));
                        ui.label(egui::RichText::new(format!("{}j", t.jumps)).color(prox(t.jumps)));
                        if let Some(c) = t.count {
                            ui.label(egui::RichText::new(format!("{c} hostiles")).strong().color(red));
                        }
                        if t.camp {
                            ui.label(egui::RichText::new("CAMP").strong().color(red));
                        }
                        if t.spike {
                            ui.label(egui::RichText::new("SPIKE").strong().color(orange));
                        }
                    });
                    if !t.ships.is_empty() {
                        ui.label(t.ships.join(", "));
                    }
                    if !t.pilots.is_empty() {
                        let p = if t.pilots.len() > 5 {
                            format!("{} +{}", t.pilots[..5].join(", "), t.pilots.len() - 5)
                        } else {
                            t.pilots.join(", ")
                        };
                        ui.label(egui::RichText::new(p).weak());
                    }
                    let age = now - t.received;
                    let age_s = if age < 60 {
                        format!("{age}s ago")
                    } else if age < 3600 {
                        format!("{}m ago", age / 60)
                    } else {
                        format!("{}h ago", age / 3600)
                    };
                    ui.label(egui::RichText::new(age_s).weak());
                });
            }

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
        });
    }

    /// The avoid-sov picker: a tree of coalitions → member alliances, plus standalone
    /// sov-holders under "Others". Ticked alliances feed the Travel route's avoid-sov filter.
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
        // NPC sov holders: live systems whose sov has a holder name but no alliance id.
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

    /// Render the map, prefixed by a docked left SidePanel for the active mode's panel (so the
    /// panel and the map never overlap and both reflow when the window is resized).
    fn map_area(&mut self, ui: &mut egui::Ui) {
        // Docks only in normal mode — overlay mode is a minimal borderless map.
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
            // Mode-specific panels dock on the right.
            if self.map_mode != MapMode::Standard && self.right_dock_open {
                egui::Panel::right("map_mode_dock")
                    .resizable(true)
                    .default_size(240.0)
                    .size_range(180.0..=340.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("\u{00BB}").on_hover_text("Minimize panel").clicked() {
                                self.right_dock_open = false;
                            }
                        });
                        match self.map_mode {
                            MapMode::Travel => self.travel_panel_content(ui),
                            MapMode::Safety => self.safety_panel_content(ui),
                            MapMode::Hunting => {
                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("Hunting").strong().size(15.0));
                                ui.separator();
                                ui.label(egui::RichText::new("Live target board — coming soon.").weak());
                            }
                            MapMode::Standard => {}
                        }
                    });
            }
        }
        ui.push_id("map:main", |ui| self.draw_map(ui));
    }

    /// Standard map controls, laid out vertically in the left dock with titled sections and
    /// text-labelled buttons (the dock is wider than the old floating bar).
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
                    for m in [MapMode::Standard, MapMode::Travel, MapMode::Hunting, MapMode::Safety] {
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

        // Combined results: systems, then constellations, then regions. Cached by query so the
        // SDE table scans only run when the input changes (was per-frame — hence the lag).
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

        // Results dropdown (variable size) — a SEPARATE area above the input, so the
        // input box never moves when results change.
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
                        // Sov-upgrade matches (highlight every system that has one).
                        if let Some(up) = self.map_highlight_upgrade.clone() {
                            if ui
                                .button(format!("{}  {up}  {}", icon::MAP_PIN_LINE, icon::X))
                                .on_hover_text("Clear upgrade highlight")
                                .clicked()
                            {
                                clear_upgrade = true;
                            }
                        }
                        let ql = query.to_lowercase();
                        let mut names: std::collections::BTreeSet<String> = Default::default();
                        for u in &self.settings.sov_upgrades {
                            for p in split_upgrade_label(&u.upgrade) {
                                if p.to_lowercase().contains(&ql) {
                                    names.insert(p.to_owned());
                                }
                            }
                        }
                        for up in names.into_iter().take(5) {
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
                        // Results: best match (sel 0) rendered last = nearest the input.
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

        // Search input — its own fixed-size area, so it never jitters.
        let ioff = egui::vec2(
            rect.left() - screen.left() + 8.0,
            rect.bottom() - screen.bottom() - 10.0,
        );
        // Accessible regions for the picker (computed outside the closure so the TextEdit's
        // &mut borrow of self.map_search doesn't clash with reading self.map_regions).
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

    /// Focus a system on the map; if currently in a region view, swap to its region.
    fn focus_map_on_select(&mut self, id: i64) {
        if matches!(self.map_view, crate::map::MapView::Region(_)) {
            if let Some(r) = self.store.as_ref().and_then(|s| s.region_of_system(id)) {
                self.map_go(crate::map::MapView::Region(r)); // resets zoom/pan
            }
        }
        // Zoom in enough that system names show, centre + highlight the selection.
        self.map_zoom = 18.0;
        self.map_focus = Some(id);
        self.map_selected = Some(id);
    }

    /// Render the popped-out map in its own OS window.
    #[allow(deprecated)] // CentralPanel::show is correct for a viewport root ctx
    fn show_map_viewport(&mut self, ctx: &egui::Context) {
        let overlay = self.map_overlay_mode;
        // Overlay mode forces on-top; "smart" keeps it on top only while EVE is the
        // focused window (refreshed throttled, like the alert window).
        if overlay && self.settings.map_overlay_smart {
            let due = self.eve_focus_checked.map(|t| t.elapsed().as_millis() > 800).unwrap_or(true);
            if due {
                self.eve_focused = eve_is_focused();
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        let on_top = if overlay {
            !self.settings.map_overlay_smart || self.eve_focused
        } else {
            self.map_window_on_top
        };
        let mut keep = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("map_window"),
            egui::ViewportBuilder::default()
                .with_title("EVE Spai — Map")
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
                // Translucent backdrop in overlay mode (the content opacity is set
                // inside draw_map); a solid panel otherwise.
                let frame = if overlay {
                    let a = (self.settings.map_overlay_opacity.clamp(0.2, 1.0) * 255.0) as u8;
                    egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(0x0A, 0x0C, 0x10, a))
                } else {
                    egui::Frame::central_panel(&ctx.style())
                };
                let locked = self.map_overlay_locked;
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    self.map_area(ui);
                    // Borderless overlay has no native resize edge — draw a grip
                    // (hidden when locked, which also disables resizing).
                    if overlay && !locked {
                        resize_grip(ui);
                    }
                });
                // Re-apply decorations/resizable on change — the builder only sets
                // them at creation, so toggling overlay↔bordered otherwise left the
                // restored window borderless / non-resizable.
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
            self.map_vp_props = None; // re-apply when the window is re-opened
        }
    }

    /// Per-character pop-out map windows: each renders the map centred on that
    /// character, in their region, with its own pan/zoom (reusing draw_map via a state
    /// swap — viewports render sequentially, so the shared map state is safe to borrow).
    #[allow(deprecated)] // CentralPanel::show is correct for a viewport root ctx
    fn char_popout_windows(&mut self, ctx: &egui::Context) {
        if self.map_char_popouts.is_empty() {
            return;
        }
        let names = self.map_char_popouts.clone();
        let locs = self.player.lock().unwrap().locations.clone();
        let mut closed: Vec<String> = Vec::new();
        // Save the main map's view state once.
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
            // A pop-out centres on its character once; it must NOT inherit the main map's
            // "follow", which would yank it to the active player's system every frame.
            self.map_follow = false;
            // Per-instance last-rect: the resize-rescale must compare against THIS window's
            // previous rect, not another instance's — otherwise it rescales pan every frame.
            self.map_last_rect = crect;
            let mut keep = true;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(format!("charmap_{name}")),
                egui::ViewportBuilder::default()
                    .with_title(format!("EVE Spai — {name}"))
                    .with_inner_size([640.0, 520.0])
                    .with_min_inner_size([360.0, 280.0]),
                |ctx, _| {
                    egui::CentralPanel::default().show(ctx, |ui| { ui.push_id(name.as_str(), |ui| self.draw_map(ui)); });
                    if ctx.input(|i| i.viewport().close_requested()) {
                        keep = false;
                    }
                },
            );
            // Persist this character's view; mark it centred so we don't re-snap.
            self.map_char_view.insert(
                name.clone(),
                (self.map_view, self.map_pan, self.map_zoom, true, self.map_last_rect),
            );
            if !keep {
                closed.push(name.clone());
            }
        }
        // Restore the main map's state; force its next draw to rebuild map_draw.
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

    /// Render `content` as a standalone, non-modal, always-on-top OS window.
    /// Returns false when the window's close button was pressed.
    #[allow(deprecated)] // CentralPanel::show is correct for a viewport root ctx
    fn dialog_viewport(
        parent: &egui::Context,
        id: &str,
        title: &str,
        size: [f32; 2],
        content: impl FnOnce(&mut egui::Ui),
    ) -> bool {
        let mut keep = true;
        let mut content = Some(content);
        parent.show_viewport_immediate(
            egui::ViewportId::from_hash_of(id),
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(size)
                .with_min_inner_size([size[0].min(380.0), size[1].min(320.0)])
                .with_always_on_top(),
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
                        // ESI is "online" once the public status poller has data.
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
                    // Small badge when a newer release is available.
                    if let Some(av) = self.update.lock().unwrap().available.clone() {
                        if av.version != self.settings.update_skip_version {
                            ui.label(
                                egui::RichText::new(format!("● v{} available", av.version))
                                    .color(egui::Color32::from_rgb(0x5a, 0xc8, 0x7a)),
                            )
                            .on_hover_text("A newer version is available — see the update prompt.");
                        }
                    }
                    // Resource usage, right-aligned.
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

    /// System-info window: details, conditions, neighbour navigation (with intel
    /// density), and the intel reported for this system.
    fn system_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.system_window else {
            return;
        };
        let mut nav: Option<i64> = None;
        let mut show_on_map = false;
        let now = chrono::Utc::now().timestamp();

        // Build the data the proper intel cards need (same as the intel feed).
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
        let resolved_pilots: std::collections::HashMap<String, i64> = {
            let cache = self.pilots.lock().unwrap();
            sys_reports
                .iter()
                .flat_map(|r| r.pilots.iter())
                .filter_map(|name| match cache.get(name) {
                    Some(Some(pid)) => Some((name.clone(), pid)),
                    _ => None,
                })
                .collect()
        };
        let status_snapshot = self.system_status.lock().unwrap().clone();
        let mut intel_click: Option<IntelClick> = None;
        let constellation = self.store.as_ref().and_then(|s| s.constellation_of_system(id));
        let region_loc = self.store.as_ref().and_then(|s| s.region_of_system(id));
        let mut open_const: Option<i64> = None;
        let mut open_region: Option<i64> = None;

        let keep = Self::dialog_viewport(
            ctx,
            "system_window",
            "EVE Spai — System info",
            [470.0, 660.0],
            |ui| {
                let Some(graph) = self.systems.clone() else {
                    ui.label("SDE not ready.");
                    return;
                };
                let Some(info) = graph.info_of(id).cloned() else {
                    ui.label("Unknown system.");
                    return;
                };

                {
                    let status = self.system_status.lock().unwrap();
                    let flags = status.get(&id).cloned().unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label(security_badge(info.security));
                        ui.heading(&info.name);
                    });
                    // Sovereignty alliance logo + ADM, floated top-right so they don't
                    // affect the name's line height.
                    if flags.sov_alliance.is_some() || flags.adm.is_some() {
                        egui::Area::new(egui::Id::new("sys_sov"))
                            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                ui.vertical_centered(|ui| {
                                    if let Some(aid) = flags.sov_alliance {
                                        let url = format!(
                                            "https://images.evetech.net/alliances/{aid}/logo?size=128"
                                        );
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
                    // Conditions only — the clickable breadcrumb below shows location.
                    system_chips_ex(ui, &self.systems, &status, id, false, false);
                    // Breadcrumb: navigate up to the constellation / region.
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
                    // Last-hour activity, highlighted vs the region average.
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
                // NPC rats (consistent per region).
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
                // Configured sovereignty upgrades for this system.
                let upgrades: Vec<&str> = self
                    .settings
                    .sov_upgrades
                    .iter()
                    .filter(|u| u.system.eq_ignore_ascii_case(&info.name))
                    .flat_map(|u| split_upgrade_label(&u.upgrade))
                    .collect();
                if !upgrades.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("Sov upgrades:").weak());
                        for u in upgrades {
                            ui.label(egui::RichText::new(u).color(crate::theme::standing::CORP));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Show on map").clicked() {
                        show_on_map = true;
                    }
                    let has_char = self.active_character != "No character";
                    let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                    let cname = self.active_character.clone();
                    ui.add_enabled_ui(has_char, |ui| {
                        if ui.button("Set Destination").clicked() {
                            self.set_destination_esi(cid.clone(), cname.clone(), id);
                            self.route_destination = Some(id); // mirror on the map
                        }
                        if ui.button("Add Waypoint").clicked() {
                            crate::esi::set_waypoint(cid.clone(), cname.clone(), id, false);
                        }
                    });
                });
                ui.separator();

                // Active-intel counts per system (density proxy) + this system's reports.
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
                            // Highlight a jump that leaves the constellation/region.
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
                ui.label(egui::RichText::new("Intel here").strong());
                egui::ScrollArea::vertical().id_salt("sysintel").max_height(280.0).show(ui, |ui| {
                    if sys_reports.is_empty() {
                        ui.label(egui::RichText::new("No recent intel.").weak());
                    }
                    for (i, r) in sys_reports.iter().enumerate() {
                        let from_you =
                            jumps_from_you(&self.systems, player_sys, r.primary_system().map(|s| s.id));
                        let sev = severity_of(r, &self.settings.severity);
                        let kc = self.kill_cache.clone();
                        if let Some(c) = intel_row(
                            ui, r, now, stale_flags[i], from_you, &self.systems, &status_snapshot,
                            &ship_details, &ship_roles, &resolved_pilots, &sys_last_ship, &kc, sev, false,
                        ) {
                            intel_click = Some(c);
                        }
                    }
                });
                // TODO: neighbouring intel density over time (sparkline) — deferred.
            },
        );

        if let Some(nid) = nav {
            self.system_window = Some(nid);
        }
        if let Some(c) = open_const {
            self.constellation_window = Some(c);
            self.focus_window = Some(egui::ViewportId::from_hash_of("constellation_window"));
        }
        if let Some(r) = open_region {
            self.region_window = Some(r);
            self.focus_window = Some(egui::ViewportId::from_hash_of("region_window"));
        }
        // A click inside an intel card (ship / pilot / system).
        match intel_click {
            Some(IntelClick::System(sid)) => self.open_system(sid),
            Some(IntelClick::Kill(kid)) => self.kill_window = Some(kid),
            Some(IntelClick::Ship(sid)) => self.open_ship(sid),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ctx.clone());
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            None => {}
        }
        if show_on_map {
            self.view = View::Map;
            if let Some(r) = self.store.as_ref().and_then(|s| s.region_of_system(id)) {
                self.map_go(crate::map::MapView::Region(r));
            }
            self.map_focus = Some(id);
        }
        if !keep {
            self.system_window = None;
        }
    }

    /// Map colour for an alliance (override from settings, else auto from name).
    fn alliance_paint(&self, name: &str) -> egui::Color32 {
        self.settings
            .alliances
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .and_then(|a| a.color)
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
            .unwrap_or_else(|| name_color(name))
    }

    /// Map colour for a coalition (override, else auto from name).
    fn coalition_paint(c: &crate::settings::Coalition) -> egui::Color32 {
        c.color
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
            .unwrap_or_else(|| name_color(&c.name))
    }

    /// Record any newly-seen sov-holding alliance (from ESI) in the settings list.
    /// Never prunes — alliances persist after they stop holding sov.
    fn discover_sov_alliances(&mut self) {
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

    /// Top sov-holding alliances over a set of systems: (alliance id, name, count).
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

    /// Top-right column of dominant alliance logos (largest first), with the count.
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
                        let url = format!("https://images.evetech.net/alliances/{aid}/logo?size=128");
                        let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(sz)));
                        let label = name.clone().unwrap_or_else(|| "Alliance".to_owned());
                        r.on_hover_text(format!("{label} — {count} systems"));
                    }
                });
            });
    }

    /// Rat-faction summary line (shared by system/constellation/region windows).
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

    /// Constellation info window — navigates up to its region, down to its systems,
    /// and across to neighbouring constellations.
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
            "EVE Spai — Constellation",
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
                // Fill the remaining height; full width; recomputed each frame.
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

    /// Region info window — navigates down to its constellations and across to
    /// neighbouring regions.
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
            "EVE Spai — Region",
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

    /// Ship-info window: render image, hull class, resists, fitting, speed.
    fn ship_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.ship_window else {
            return;
        };
        let details = self.store.as_ref().and_then(|s| s.ship_details(id));
        let traits = self.store.as_ref().map(|s| s.ship_traits(id)).unwrap_or_default();
        let roles = derive_roles(&traits);
        // Resolve skill names (ESI, cached) for the per-skill bonus sections.
        let skill_ids: Vec<i64> = {
            let mut s: Vec<i64> = traits.iter().map(|t| t.0).filter(|&s| s > 0).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        self.ensure_type_names(&skill_ids, ctx);
        let names = self.type_names.lock().unwrap().clone();
        let keep = Self::dialog_viewport(ctx, "ship_window", "EVE Spai — Ship", [380.0, 600.0], |ui| {
            ui.horizontal(|ui| {
                let url = format!("https://images.evetech.net/types/{id}/render?size=128");
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
                // Fill the remaining window height; scroll if it overflows.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .id_salt("ship_traits")
                    .show(ui, |ui| {
                        // Per-skill sections first (specialised → generic), role bonuses last.
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

    /// Jump-bridge configuration: paste a coalition list (any separator); each
    /// line's first two SDE systems become a bridge. Drawn green on the map.
    /// Coalition editor: name + member alliance names (one per line). Unlisted
    /// alliances are independent.
    /// "Update available" prompt: Yes (download + self-replace), No (don't ask again
    /// for this version), or Ask Me Again Later (re-prompt next launch).
    fn update_dialog(&mut self, ctx: &egui::Context) {
        let st = self.update.lock().unwrap().clone();
        let Some(av) = st.available.clone() else { return };
        if self.update_dismissed || av.version == self.settings.update_skip_version {
            return;
        }
        let mut close = false;
        let mut start_install = false;
        egui::Window::new(format!("{}  Update available", egui_phosphor::regular::DOWNLOAD_SIMPLE))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .show(ctx, |ui| {
                if st.done {
                    ui.label(format!("Updated to v{}. Restart EVE Spai to apply.", av.version));
                    if ui.button("OK").clicked() {
                        close = true;
                    }
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
                    "EVE Spai v{} is available — you have v{}.",
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
                        let res = crate::update::download_and_replace(&url);
                        let mut s = upd.lock().unwrap();
                        s.installing = false;
                        match res {
                            Ok(()) => s.done = true,
                            Err(e) => s.error = Some(e.to_string()),
                        }
                        ctx2.request_repaint();
                    });
                }
                // No binary for this platform — send them to the release page.
                None => {
                    let _ = open::that(&av.html_url);
                    close = true;
                }
            }
        }
        if close {
            self.update.lock().unwrap().available = None;
        }
    }

    /// Watch the clipboard (throttled); when it newly holds a d-scan, queue a prompt.
    fn poll_dscan_clipboard(&mut self) {
        if !self.settings.dscan_autoprompt {
            return;
        }
        let due = self.dscan_checked.map(|t| t.elapsed().as_millis() > 1200).unwrap_or(true);
        if !due {
            return;
        }
        self.dscan_checked = Some(std::time::Instant::now());
        // A prompt or upload already in flight — don't poll over it.
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
            self.dscan_prompt = Some((text, n));
        }
    }

    /// Prompt to share a detected d-scan, and show the resulting link.
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

    #[allow(deprecated)] // CentralPanel::show is correct for a viewport root ctx
    fn dscan_dialog(&mut self, ctx: &egui::Context) {
        let active = self.dscan_prompt.is_some() || {
            let s = self.dscan_share.lock().unwrap();
            s.uploading || s.link.is_some() || s.error.is_some()
        };
        if !active {
            self.dscan_pos = None;
            self.dscan_link_used = false;
            self.dscan_unfocused_at = None;
        }
        // Auto-upload mode: skip the prompt and upload straight away.
        if active && self.settings.dscan_autoupload {
            let idle = {
                let s = self.dscan_share.lock().unwrap();
                !s.uploading && s.link.is_none() && s.error.is_none()
            };
            if idle {
                if let Some((text, _)) = self.dscan_prompt.take() {
                    self.start_dscan_upload(ctx, text);
                }
            }
        }
        // Position the popup once, at the bottom-right of the EVE window if we can find
        // it (X11), otherwise a sensible screen position.
        if active && self.dscan_pos.is_none() {
            // Outer window size (inner + the title bar the decorations add) and a small
            // margin, so it sits just inside the EVE window's bottom-right, not touching.
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
        let mut dismiss = false;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("dscan_popup"),
            egui::ViewportBuilder::default()
                .with_title("EVE Spai — D-scan")
                .with_visible(active) // created at startup, just toggled visible
                .with_window_level(egui::WindowLevel::AlwaysOnTop)
                .with_active(false) // do not steal focus from the game
                .with_decorations(true) // border + title bar so it can be dragged
                .with_taskbar(false)
                .with_resizable(true)
                .with_position([pos.0, pos.1])
                .with_inner_size([300.0, 118.0]),
            |ctx, _| {
                if !active {
                    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |_ui| {});
                    return;
                }
                // Re-assert always-on-top each frame; some WMs drop the initial hint.
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::AlwaysOnTop,
                ));
                let frame = egui::Frame::central_panel(&ctx.style());
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    ui.label(egui::RichText::new(format!("{}  D-scan", icon::BROADCAST)).strong());
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
                            ui.label("Uploading to dscan.info...");
                        });
                    } else {
                        if let Some(e) = &error {
                            ui.colored_label(
                                crate::theme::standing::WARNING,
                                format!("Upload failed: {e}"),
                            );
                        }
                        if let Some((_, n)) = &self.dscan_prompt {
                            ui.label(format!("D-scan detected ({n} rows)."));
                            ui.horizontal(|ui| {
                                if ui
                                    .button(format!("{}  Share on dscan.info", icon::UPLOAD_SIMPLE))
                                    .clicked()
                                {
                                    start_upload = true;
                                }
                                if ui.button("Dismiss").clicked() {
                                    dismiss = true;
                                }
                            });
                        }
                        if ui
                            .checkbox(&mut self.settings.dscan_autoupload, "Auto-upload (also in Settings)")
                            .changed()
                        {
                            self.needs_save = true;
                        }
                    }
                });
                if ctx.input(|i| i.viewport().close_requested()) {
                    dismiss = true;
                }
            },
        );

        if start_upload {
            if let Some((text, _)) = self.dscan_prompt.take() {
                self.start_dscan_upload(ctx, text);
            }
        }
        if dismiss {
            if let Some((text, _)) = &self.dscan_prompt {
                self.dscan_dismissed_hash = hash_str(text);
            }
            self.dscan_prompt = None;
            *self.dscan_share.lock().unwrap() = DscanShare::default();
        }
    }

    /// First-run setup wizard (docs/WORMHOLES_AND_NEXT.md A8). Dismissable; can be
    /// re-run from Settings. Walks logs → channels → character → theme.
    fn setup_wizard(&mut self, ctx: &egui::Context) {
        if !self.wizard_open {
            return;
        }
        use egui_phosphor::regular as icon;

        // The active step list — Imperium adds optional jump-bridge / sov-upgrade /
        // jabber steps once that pack is applied.
        #[derive(Clone, Copy, PartialEq)]
        enum S {
            Welcome,
            Logs,
            Channels,
            JumpBridges,
            SovUpgrades,
            Jabber,
            Character,
            Theme,
        }
        let mut steps = vec![S::Welcome, S::Logs, S::Channels];
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
                        // Validate the entered (or auto-detected) location and show a
                        // check / cross.
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
                        }
                        if idx > 0 && ui.button("Back").clicked() {
                            idx -= 1;
                        }
                    });
                });
            });
        self.wizard_step = idx.min(last) as u8;
        if finish || close {
            self.settings.wizard_done = true;
            self.needs_save = true;
            self.wizard_open = false;
        }
    }

    /// Alert-rules editor (inline). Rules are evaluated top-first; the first matching
    /// enabled rule decides the actions (or suppresses the alert).
    fn alert_rules_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut remove: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        ui.label(
            egui::RichText::new(
                "Top rule wins. A matching rule's actions apply (or it suppresses the alert). \
                 Empty condition fields mean \"any\". Jumps are measured from the rule's \
                 characters (or any enabled character).",
            )
            .weak(),
        );
        if ui.button("Add rule").clicked() {
            self.settings
                .alerts
                .rules
                .push(crate::settings::AlertRule { expanded: true, ..Default::default() });
            changed = true;
        }
        let mut move_down: Option<usize> = None;
        use crate::settings::Severity::*;
        let n_rules = self.settings.alerts.rules.len();
        for (i, ru) in self.settings.alerts.rules.iter_mut().enumerate() {
            ui.group(|ui| {
                use egui_phosphor::regular as ic;
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut ru.enabled, "").changed();
                    let toggle = if ru.expanded { ic::CARET_DOWN } else { ic::CARET_RIGHT };
                    if ui.button(toggle).on_hover_text("Expand / collapse").clicked() {
                        ru.expanded = !ru.expanded;
                    }
                    if ru.expanded {
                        changed |= ui
                            .add(egui::TextEdit::singleline(&mut ru.name).desired_width(180.0))
                            .changed();
                    } else {
                        let name = if ru.name.is_empty() { "(unnamed rule)" } else { &ru.name };
                        let txt = if ru.enabled {
                            egui::RichText::new(name).strong()
                        } else {
                            egui::RichText::new(name).weak().strikethrough()
                        };
                        if ui.add(egui::Label::new(txt).sense(egui::Sense::click())).clicked() {
                            ru.expanded = true;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(ic::X).on_hover_text("Delete").clicked() {
                            remove = Some(i);
                        }
                        if i + 1 < n_rules && ui.button(ic::ARROW_DOWN).on_hover_text("Move down").clicked() {
                            move_down = Some(i);
                        }
                        if i > 0 && ui.button(ic::ARROW_UP).on_hover_text("Move up").clicked() {
                            move_up = Some(i);
                        }
                    });
                });
                if !ru.expanded {
                    return;
                }
                // Conditions.
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
                // Condition tags.
                ui.horizontal_wrapped(|ui| {
                    ui.label("requires:");
                    for tag in
                        ["bubble", "camp", "cyno", "captackled", "kill", "ess", "spike", "wormhole", "help"]
                    {
                        let label = if tag == "captackled" { "cap tackled" } else { tag };
                        let mut on = ru.require.iter().any(|t| t == tag);
                        if ui.selectable_label(on, label).clicked() {
                            on = !on;
                            ru.require.retain(|t| t != tag);
                            if on {
                                ru.require.push(tag.to_owned());
                            }
                            changed = true;
                        }
                    }
                });
                // Location filter: systems / constellations / regions (any matches).
                // Constellation/region names contain spaces, so they split on commas
                // only; system codes split on spaces too.
                let loc_field = |ui: &mut egui::Ui, label: &str, list: &mut Vec<String>, split_space: bool| -> bool {
                    let mut ch = false;
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let mut s = list.join(", ");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut s)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("any"),
                            )
                            .changed()
                        {
                            let sep: &[char] = if split_space { &[',', ' '] } else { &[','] };
                            *list = s.split(sep).map(|x| x.trim().to_owned()).filter(|x| !x.is_empty()).collect();
                            ch = true;
                        }
                    });
                    ch
                };
                changed |= loc_field(ui, "systems:", &mut ru.systems, true);
                changed |= loc_field(ui, "constellations:", &mut ru.constellations, false);
                changed |= loc_field(ui, "regions:", &mut ru.regions, false);
                changed |= loc_field(ui, "channels:", &mut ru.channels, false);
                // Characters this rule applies to (empty = any enabled character).
                ui.horizontal(|ui| {
                    ui.label("characters:");
                    let mut s = ru.characters.join(", ");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut s)
                                .desired_width(f32::INFINITY)
                                .hint_text("any enabled"),
                        )
                        .changed()
                    {
                        ru.characters =
                            s.split(',').map(|x| x.trim().to_owned()).filter(|x| !x.is_empty()).collect();
                        changed = true;
                    }
                });
                // Actions.
                ui.horizontal_wrapped(|ui| {
                    ui.label("then:");
                    changed |= ui.checkbox(&mut ru.suppress, "suppress").changed();
                    if !ru.suppress {
                        changed |= ui.checkbox(&mut ru.system_notification, "notify").changed();
                        changed |= ui.checkbox(&mut ru.custom_window, "window").changed();
                        changed |= ui.checkbox(&mut ru.push, "push").changed();
                        ui.label("sound");
                        changed |= ui
                            .add(egui::TextEdit::singleline(&mut ru.sound).desired_width(90.0).hint_text("default"))
                            .changed();
                    }
                    ui.label("cooldown");
                    changed |= ui
                        .add(egui::DragValue::new(&mut ru.cooldown_secs).range(0..=3600).suffix("s"))
                        .changed();
                });
            });
        }
        if let Some(i) = remove {
            self.settings.alerts.rules.remove(i);
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
        if changed {
            self.needs_save = true;
        }
    }

    /// Intel severity configuration dialog.
    fn severity_window(&mut self, ctx: &egui::Context) {
        if !self.severity_open {
            return;
        }
        let mut changed = false;
        let mut threat_text = self.settings.severity.threat_ships.join("\n");
        let keep = Self::dialog_viewport(
            ctx,
            "severity_window",
            "EVE Spai — Intel severity",
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
                // Two columns so the conditions fit without a tall window.
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
        // Deferred edits (avoid borrowing settings.* mutably mid-iteration).
        let mut coal_color: Vec<(String, Option<(u8, u8, u8)>)> = Vec::new();
        let mut ally_color: Vec<(usize, Option<(u8, u8, u8)>)> = Vec::new();
        let mut ally_remove: Option<usize> = None;
        let mut ally_assign: Option<(String, Option<String>)> = None;
        let mut ally_add = false;
        let keep = Self::dialog_viewport(
            ctx,
            "coalitions_window",
            "EVE Spai — Coalitions",
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
                                // Colour (override or auto from name).
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

                // --- Alliances holding sov (auto-discovered from ESI) ---
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
        // Apply deferred alliance/coalition colour + membership edits.
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
            // Keep the editor buffers in step with the changed membership.
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
        // Sync edit buffers back into settings.
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
            "EVE Spai — Jump bridges",
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

    /// Sovereignty-upgrade configuration: paste lines of "<system> <upgrade…>".
    fn sov_upgrades_window(&mut self, ctx: &egui::Context) {
        if !self.sov_upgrades_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "sov_upgrades_window",
            "EVE Spai — Sov upgrades",
            [460.0, 520.0],
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Paste sov-upgrade data (one per line).").weak());
                    ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(
                        "Imperium members: copy the sov-upgrade list from the alliance tool and \
                         paste it here. The first system matched on each line is used; the rest \
                         of the line becomes the upgrade label.",
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
            "EVE Spai — Intel channels",
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
                // Button above the (bounded) list so it never gets pushed off-screen.
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

                    // --- Theme ---
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

                    // --- General ---
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
                            // Re-check even if this version was previously skipped.
                            self.update_dismissed = false;
                            self.settings.update_skip_version.clear();
                            changed = true;
                            crate::update::spawn_check(
                                self.update.clone(),
                                String::new(),
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
                            if ui.selectable_label(self.settings.fit_site == *id, *label).clicked() {
                                self.settings.fit_site = (*id).to_owned();
                                changed = true;
                            }
                        }
                        if ui.selectable_label(self.settings.fit_site.is_empty(), "Ask each time").clicked() {
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

                    // --- Alerts ---
                    ui.label(egui::RichText::new("Alerts").strong());
                    changed |= ui
                        .checkbox(&mut self.settings.alert_enabled, "Enable intel alerts")
                        .on_hover_text("Master switch. Configure what fires in the Alerts tab.")
                        .changed();
                    changed |= ui
                        .checkbox(&mut self.settings.alert_combat, "Combat alerts (under attack / scrambled)")
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
                        // Per-severity sounds (preset name or file path) + test.
                        ui.label(egui::RichText::new("Sounds (preset: off/info/warning/danger/critical/beep/chime, or a file path)").weak());
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
                                changed |= ui
                                    .add(egui::TextEdit::singleline(&mut a.sounds[i]).desired_width(180.0))
                                    .changed();
                                if ui.button(egui_phosphor::regular::PLAY).on_hover_text("Test").clicked() {
                                    crate::sound::play(&a.sounds[i]);
                                }
                            });
                        }
                        // Pushover (mobile push).
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

                    // --- Configuration packs ---
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

                    // --- Intel channels ---
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

                    // --- Jump bridges & sov upgrades ---
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
                            egui::Image::new(
                                "https://images.evetech.net/characters/2119400938/portrait?size=64",
                            )
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

impl eframe::App for SpaiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // System tray: Show brings the window back; Exit quits for real. Closing the
        // window hides to the tray instead of quitting (when enabled).
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

        // Re-apply the theme every frame so colour edits are reflected live (cheap).
        self.settings.theme.apply(&ctx);

        self.refresh_characters();
        self.player.lock().unwrap().active_name = self.active_character.clone();
        self.maybe_start_watcher(&ctx);
        self.maybe_start_jabber(&ctx);
        self.ingest_killfeed();
        if self.reconcile_unresolved_pilots() {
            ctx.request_repaint(); // a cover split the run -> re-render the card now
        }
        self.reload_wormholes();
        if !self.update_checked {
            self.update_checked = true;
            crate::update::cleanup_old();
            crate::update::spawn_check(
                self.update.clone(),
                self.settings.update_skip_version.clone(),
                ctx.clone(),
            );
        }
        self.update_dialog(&ctx);
        if !self.wizard_checked {
            self.wizard_checked = true;
            self.wizard_open = !self.settings.wizard_done;
        }
        self.setup_wizard(&ctx);
        self.poll_dscan_clipboard();
        self.poll_jabber_notify(&ctx);
        self.poll_kill_fetches();
        self.dscan_dialog(&ctx);
        self.ping_compose_dialog(&ctx);
        self.ping_rules_dialog(&ctx);
        self.kill_window(&ctx);
        self.maybe_rebuild_graph(&ctx);
        self.persist_view_options();
        self.discover_sov_alliances();
        self.os_notify
            .store(self.settings.alert_combat, std::sync::atomic::Ordering::Relaxed);
        self.check_alerts();
        self.top_bar(ui);
        self.status_bar(ui);
        self.nav_rail(ui);

        egui::CentralPanel::default().show_inside(ui, |ui| match self.view {
            View::Dashboard => self.dashboard_view(ui),
            View::Map => self.map_view(ui),
            View::Characters => self.characters_view(ui),
            View::Intel => self.intel_view(ui),
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
        // Bring a just-updated window to the foreground.
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

        if self.needs_save {
            self.persist();
        }
    }

    fn on_exit(&mut self) {
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

/// Fire a desktop notification off the UI thread (dbus can block).
/// The currently-active X11 window (id, name), best-effort via xdotool.
#[cfg(not(target_os = "linux"))]
fn active_window() -> Option<(String, String)> {
    None // X11/xdotool only
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

/// Best-effort check whether the EVE client is the focused window (X11 via
/// xdotool/xprop). Returns true when it can't tell (so "smart" ≈ always-on-top).
fn eve_is_focused() -> bool {
    match active_window() {
        Some((_, name)) if !name.is_empty() => {
            let n = name.to_lowercase();
            n.contains("eve") && !n.contains("eve spai")
        }
        _ => true,
    }
}

/// Best-effort EVE client window geometry (x, y, width, height) via xdotool (X11).
/// None when xdotool is missing or no EVE window is found.
#[cfg(not(target_os = "linux"))]
fn eve_window_rect() -> Option<(i32, i32, i32, i32)> {
    None // X11/xdotool only
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

/// Draw a dashed line from `p1` to `p2` whose dashes flow toward `p2` as `phase`
/// increases (the in-game autopilot look).
fn dashed_flow(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32, phase: f32) {
    let dir = p2 - p1;
    let len = dir.length();
    if len < 1.0 {
        return;
    }
    let unit = dir / len;
    let (dash, period) = (6.0f32, 12.0f32);
    // Dashes advance toward p2 (the destination) as `phase` grows.
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

/// Resolve a pasted token to a canonical system name. Exact match only — paste
/// data uses full system names, and prefix matching would resolve stray words to
/// random systems whose name merely starts with the token.
fn resolve_system(graph: &crate::geo::Systems, raw: &str) -> Option<String> {
    let tok = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'');
    if tok.len() < 2 {
        return None;
    }
    graph.lookup(tok).map(|i| i.name.clone())
}

/// Parse a pasted jump-bridge list: the first two systems found on a
/// line are a bridge. Tolerant of arrows/punctuation glued to system codes, so the
/// user can paste a whole wiki page.
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

/// Parse pasted sov upgrades. Primary: the in-game I-Hub copy — a header
/// "Sovereignty Hub <System>" followed by tab-separated "N⇥Upgrade⇥Online/Offline"
/// rows. Fallback: per-line "<system> <upgrade…>".
/// Split a stored sov-upgrade label into individual upgrade names. The in-game / alliance
/// paste packs several into one comma-separated line, sometimes with a "<-" marker.
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
    // Generic fallback.
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

/// Nearest projected system to a point within `threshold` pixels.
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

/// BFS tree from `center` out to `depth` jumps. Returns (distance per system,
/// children in the discovery tree, systems in BFS order).
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
        ns.sort_unstable(); // deterministic layout
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

/// Assign each node a fraction in [0,1] for its angular/horizontal position:
/// leaves are spread evenly, internal nodes sit at the mean of their children.
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

/// Jumps from the player's system to a target system, if both are known.
fn jumps_from_you(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    player_sys: Option<i64>,
    target: Option<i64>,
) -> Option<u32> {
    let (sys, p, t) = (systems.as_ref()?, player_sys?, target?);
    sys.jumps(t, p, 50)
}

/// Fewest jumps from any of `srcs` to `target` (None if none reach it / no target).
/// `use_bridges` counts jump bridges; otherwise the distance is gate-only.
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

/// A New Eden (k-space) system id — these are drawn on the map.
fn is_kspace(id: i64) -> bool {
    (30_000_000..31_000_000).contains(&id)
}
/// A J-space / wormhole-region system id (incl. Thera) — never drawn on the map.
fn is_jspace(id: i64) -> bool {
    (31_000_000..32_000_000).contains(&id)
}

/// Wormhole map overlay: k-space↔k-space links, chains through J-space (with the
/// J-space hop count), and the set of k-space systems that hold a J-space hole.
#[derive(Default, Clone)]
struct WhOverlay {
    /// Direct k-space ↔ k-space wormholes.
    direct: Vec<(i64, i64)>,
    /// k-space ↔ k-space via ≥1 J-space system: (a, b, j-space hops).
    chains: Vec<(i64, i64, usize)>,
    /// k-space systems with a hole leading into J-space.
    jspace_holes: std::collections::HashSet<i64>,
    /// k-space systems with a known wormhole connection to Thera.
    thera_conns: Vec<i64>,
}

impl WhOverlay {
    /// Build the overlay from the known connections. Chains are found by walking the
    /// wormhole graph from each k-space system *through J-space only* until another
    /// k-space system is reached; capped in depth and count to stay sane.
    fn build(whs: &[crate::wormholes::Wormhole]) -> WhOverlay {
        use std::collections::{HashMap, HashSet, VecDeque};
        const MAX_J_HOPS: usize = 4;
        const MAX_CHAINS: usize = 60;
        // A real chain link has a handful of holes; a public hub (Thera) has many. We
        // don't path *through* a high-degree J-space node, so we don't turn every pair
        // of systems sharing Thera into a bogus chain — those show as hole markers.
        const MAX_HUB_DEGREE: usize = 6;

        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut jspace_holes: HashSet<i64> = HashSet::new();
        for w in whs {
            let a = w.system_id;
            if let Some(b) = w.dest_system_id {
                adj.entry(a).or_default().push(b);
                adj.entry(b).or_default().push(a);
                if is_kspace(a) && is_jspace(b) {
                    jspace_holes.insert(a);
                }
                if is_kspace(b) && is_jspace(a) {
                    jspace_holes.insert(b);
                }
            } else if is_kspace(a)
                && matches!(w.dest, crate::wormholes::DestClass::Wspace)
            {
                jspace_holes.insert(a);
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
                        // A k-space node ends the chain — don't path through it.
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

/// A Jabber roster entry / conversation for the contact list.
struct Convo {
    jid: String,
    name: String,
    unread: bool,
    group: String,
    presence: crate::jabber::Presence,
    status_text: String,
}

/// EVE (UTC) timestamp for a chat message: "EVE HH:MM" today, full date otherwise.
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

/// Render a chat message body, turning http(s) URLs into clickable links. Non-link text
/// stays as plain (selectable) labels.
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
            // A bare "http" not starting a URL — emit it and move on.
            ui.label(&rest[..rel + 4]);
            rest = &rest[rel + 4..];
        }
    }
    if !rest.is_empty() {
        ui.label(rest);
    }
}

/// Whether a string is a syntactically valid bare JID (local@domain, no spaces).
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

/// Truncate to `max` chars with an ellipsis.
fn truncate_to(s: &str, max: usize) -> String {
    if max > 1 && s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_owned()
    }
}

/// Truncate a sidebar chip name so a long room/DM name can't widen the panel.
fn short_chip(s: &str) -> String {
    truncate_to(s, 20)
}

/// Roughly how many characters fit in `width` px at the contact-list font size,
/// so names ellipsize to the available space (scrollbar already excluded by egui).
fn fit_chars(width: f32) -> usize {
    (width / 7.5).floor().max(3.0) as usize
}

/// Draft for the quick-ping composer.
#[derive(Default)]
struct PingDraft {
    group: String,
    /// false = plain question, true = the fleet-ping form.
    fleet: bool,
    msg: String,
    fc: String,
    doctrine: String,
    formup: String,
    /// 0 = none, 1 = strategic, 2 = peacetime.
    pap: u8,
}

impl PingDraft {
    /// Build the `!bping <group> …` command body.
    fn to_command(&self) -> String {
        let group = self.group.trim();
        let mut body = format!("!bping {group} {}", self.msg.trim());
        if self.fleet {
            if !self.fc.trim().is_empty() {
                body.push_str(&format!("\nFC Name: {}", self.fc.trim()));
            }
            if !self.formup.trim().is_empty() {
                body.push_str(&format!("\nFormup Location: {}", self.formup.trim()));
            }
            match self.pap {
                1 => body.push_str("\nPAP Type: Strategic"),
                2 => body.push_str("\nPAP Type: Peacetime"),
                _ => {}
            }
            if !self.doctrine.trim().is_empty() {
                body.push_str(&format!("\nDoctrine: {}", self.doctrine.trim()));
            }
        }
        body
    }
}

/// State of an in-flight / completed d-scan upload.
#[derive(Default)]
struct DscanShare {
    uploading: bool,
    link: Option<String>,
    error: Option<String>,
}

/// Stable hash of a string (to detect clipboard changes).
fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Compact "time ago" — minutes under an hour, hours under a day, else days.
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

/// System suffix chips: in-game-style `< Constellation < Region`, NPC faction
/// (rats/sov), and live status (incursion / FW / player sovereignty). Looked up by
/// id internally — no ids are ever shown.
fn system_chips(
    ui: &mut egui::Ui,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    system_id: i64,
) {
    system_chips_ex(ui, systems, status, system_id, true, true);
}

/// As `system_chips`, but `show_sov=false` omits the sov text chip (the system
/// window shows the alliance logo instead).
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
        // Faction = rats / NPC sov; only meaningful in low/null (highsec is CONCORD).
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

/// A weak "Nj" distance-from-you chip (blank if unknown).
fn from_you_chip(ui: &mut egui::Ui, from_you: Option<u32>) {
    if let Some(j) = from_you {
        let txt = if j == 0 { "here".to_owned() } else { format!("{j}j") };
        // Monospace + padded so the jumps chip is a fixed width.
        ui.label(egui::RichText::new(format!("{txt:>4}")).monospace().weak());
    }
}

/// Format ISK compactly: 1.2B / 340M / 5.0k.
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

/// Render one clustered battle.
fn battle_row(
    ui: &mut egui::Ui,
    b: &crate::battle::Battle,
    now: i64,
    from_you: Option<u32>,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
) {
    let span_min = ((b.end - b.start) / 60).max(0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{:>7}", fmt_age(now - b.end))).monospace().weak());
            from_you_chip(ui, from_you);
            // systems involved (with security colour)
            for (id, name, sec) in &b.systems {
                ui.label(security_badge(*sec));
                ui.label(egui::RichText::new(name).strong());
                system_chips(ui, systems, status, *id);
            }
            ui.separator();
            ui.label(format!("{} kills", b.kills));
            ui.label(egui::RichText::new(format!("{} ISK", fmt_isk(b.isk))).weak());
            if span_min > 0 {
                ui.label(egui::RichText::new(format!("over {span_min}m")).weak());
            }
        });
        // Belligerent sides, "vs" separated.
        ui.horizontal_wrapped(|ui| {
            for (i, side) in b.sides.iter().take(2).enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new("vs").strong());
                }
                let mut names: Vec<&str> = side.parties.iter().take(3).map(|s| s.as_str()).collect();
                if side.parties.len() > 3 {
                    names.push("…");
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{} [{}k/{}l, {} lost]",
                        names.join(", "),
                        side.kills,
                        side.losses,
                        fmt_isk(side.isk_lost)
                    ))
                    ,
                );
            }
        });
    });
}

/// Whether an alert rule's conditions all match a report.
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
    // Channel filter: each entry is a case-insensitive regex (falls back to a plain
    // substring match if it isn't a valid regex).
    if !ru.channels.is_empty() {
        let ch = r.channel.to_lowercase();
        let matched = ru.channels.iter().any(|pat| {
            let p = pat.to_lowercase();
            match regex::Regex::new(&format!("(?i){pat}")) {
                Ok(re) => re.is_match(&r.channel),
                Err(_) => ch.contains(&p),
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
    // Location filter: systems, constellations, and/or regions (any may match).
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
    for tag in &ru.require {
        let ok = match tag.to_lowercase().as_str() {
            "bubble" => r.bubble,
            "camp" => r.camp,
            "cyno" => r.cyno,
            "captackled" | "cap" => r.cap_tackled,
            "tackled" | "point" | "scram" => r.tackled,
            "kill" | "killmail" => r.killmail,
            "ess" => r.ess,
            "wormhole" | "wh" => r.wormhole,
            "spike" => r.spike,
            "skyhook" => r.skyhook,
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

/// A concise one-line alert string for a report.
/// Show a desktop notification (best-effort, off the UI thread).
fn notify_os(summary: &str, body: &str) {
    let (summary, body) = (summary.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().summary(&summary).body(&body).show();
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
    if r.camp {
        parts.push("gate camp".into());
    }
    if r.cyno {
        parts.push("CYNO".into());
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

/// Render a parsed fleet ping (or plain broadcast) as a card.
fn render_ping(
    ui: &mut egui::Ui,
    p: &crate::pings::Ping,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    highlight: bool,
) {
    use crate::pings::{Comms, Formup, PapType, Ping};
    use egui_phosphor::regular as icon;
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
    // "Time since the ping went out".
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
        ui.set_min_width(ui.available_width()); // full-width card
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
                            ui.horizontal(|ui| {
                                ui.label(format!("Comms: {channel}"));
                                ui.hyperlink_to(icon::LINK, link);
                            });
                        }
                        Comms::Text(t) => {
                            ui.label(format!("Comms: {t}"));
                        }
                    }
                }
                if let Some(d) = doctrine {
                    ui.label(format!("Doctrine: {d}"));
                }
                if !description.is_empty() {
                    ui.label(egui::RichText::new(description).weak());
                }
                let from = source.as_deref().unwrap_or("?");
                let to = target.as_deref().unwrap_or("?");
                ui.label(egui::RichText::new(format!("— {from} {} {to}", icon::ARROW_RIGHT)).weak().small());
            }
            Ping::Plain { text, sender, target, .. } => {
                ui.horizontal_wrapped(|ui| {
                    let from = sender.as_deref().unwrap_or("ping");
                    let to = target.as_deref().map(|t| format!(" {} {t}", icon::ARROW_RIGHT)).unwrap_or_default();
                    ui.label(egui::RichText::new(format!("{from}{to}")).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("{ago} ago")).weak());
                    });
                });
                ui.label(text);
            }
        }
    });
}

/// Compute an intel report's severity from the configurable rules (highest match).
fn severity_of(
    r: &crate::intel::IntelReport,
    rules: &crate::settings::SeverityRules,
) -> crate::settings::Severity {
    use crate::settings::Severity::*;
    let mut s = Info;
    if let Some(n) = r.count {
        s = s.max(if n >= rules.big_gang_threshold { rules.big_gang } else { rules.small_gang });
    } else if !r.systems.is_empty() && !r.clear && !r.killmail && !r.status {
        // A sighting with no count — but not a bare status request ("status?"), which
        // stays Info.
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

/// Card tint colour for a severity level.
fn severity_color(s: crate::settings::Severity) -> egui::Color32 {
    use crate::settings::Severity::*;
    match s {
        Info => egui::Color32::from_rgb(0x6E, 0x7A, 0x86),
        Warning => crate::theme::standing::WARNING,
        Danger => egui::Color32::from_rgb(0xE6, 0x6A, 0x2A),
        Critical => crate::theme::standing::HOSTILE,
    }
}

/// Latest reported ship per pilot (lower-cased name → (ship id, name, time)), so a
/// later sighting without a ship can show a "last seen" ship badge. Only recorded
/// when a report ties exactly one pilot to one ship (a clear 1:1 association).
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

/// Age as s / m+s / h+m pairs (only seconds when under a minute).
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

/// Render a single intel report row in the concise, parsed format. `stale` means a
/// later "clear" has outdated it; `from_you` is jumps from the active character.
/// Render one intel report as typed, clickable panels (no raw message inline; the
/// raw text is available on hover). Returns a clicked system id to focus the map.
/// A bottom-right resize grip for a borderless viewport (the map overlay, the alert
/// window): hover highlights the diagonal ticks and shows the resize cursor; dragging
/// begins a window resize. Call last so it paints over the content.
fn resize_grip(ui: &mut egui::Ui) {
    const SZ: f32 = 18.0;
    let corner = ui.max_rect().right_bottom();
    let rect = egui::Rect::from_min_max(corner - egui::vec2(SZ, SZ), corner);
    let resp = ui.interact(rect, ui.id().with("resize_grip"), egui::Sense::drag());
    let hot = resp.hovered() || resp.dragged();
    let col = if hot {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let painter = ui.painter();
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

/// Stable-ish key for caching an intel card's measured height (feed virtualisation).
fn report_key(r: &crate::intel::IntelReport) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    r.received.hash(&mut h);
    r.reporter.hash(&mut h);
    r.text.len().hash(&mut h);
    h.finish()
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
    last_ship: &std::collections::HashMap<String, (i64, String, i64)>,
    kills: &crate::kills::KillCache,
    sev: crate::settings::Severity,
    show_reporter: bool,
) -> Option<IntelClick> {
    use egui_phosphor::regular as icon;
    let age = (now - r.received).max(0);
    let green = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
    let warn = crate::theme::standing::WARNING;
    let red = crate::theme::standing::HOSTILE;
    let accent = ui.visuals().hyperlink_color;
    let jumps_color = crate::theme::standing::CORP;

    // Report type drives the background tint and a leading icon.
    // Leading icon by report kind; the card tint is the configurable severity.
    let type_icon = if r.clear {
        icon::CHECK_CIRCLE
    } else if r.killmail {
        icon::SKULL
    } else if r.spike || r.camp || r.bubble || r.cyno || r.help {
        icon::WARNING_OCTAGON
    } else if r.no_visual {
        icon::EYE_SLASH
    } else if !r.systems.is_empty() || r.count.is_some() {
        icon::WARNING
    } else {
        icon::INFO
    };
    let tint = if r.clear { green } else { severity_color(sev) };

    // Clicking a card toggles between the parsed view and the raw message (with a minimal
    // elapsed · jumps · system header). State is keyed by the report so it sticks per card.
    let toggle_id = egui::Id::new("intel_raw").with(report_key(r));
    let show_raw = ui.ctx().data(|d| d.get_temp::<bool>(toggle_id).unwrap_or(false));

    let mut clicked: Option<IntelClick> = None;
    let resp = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 4))
        .fill(tint.gamma_multiply(if stale { 0.05 } else { 0.13 }))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // The raw message lives on the non-interactive left columns so it never
            // competes with (or hides) the ship/system/pilot panel tooltips.
            let msg = format!("{}\n— {} · {}", r.text, r.reporter, r.channel);
            if show_raw {
                // Raw view: a minimal header (elapsed · jumps · system), then the plain message.
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(type_icon).color(tint));
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
                    ui.label(body);
                });
                return;
            }
            let mut render = |ui: &mut egui::Ui| {
                // Plain inline widgets (no fixed-size sub-uis — those break wrapping
                // inside horizontal_wrapped and make the card grow vertically).
                ui.label(egui::RichText::new(type_icon).color(tint)).on_hover_text(&msg);
                // Fixed-width (monospace + padded) so the counting-up age doesn't shift
                // the rest of the row.
                ui.label(
                    egui::RichText::new(format!("{:>7}", fmt_age(age))).monospace().weak(),
                )
                .on_hover_text(&msg);
                // Always reserve the jumps slot (padded) so it doesn't reflow when the
                // distance is computed retroactively.
                let jtxt = match from_you {
                    Some(0) => "here".to_owned(),
                    Some(j) => format!("{j}j"),
                    None => String::new(),
                };
                ui.label(egui::RichText::new(format!("{jtxt:>4}")).monospace().color(jumps_color));

                // Hostile-count badge — prominent; the number is what matters most.
                if let Some(n) = r.count {
                    egui::Frame::new()
                        .fill(red)
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{} {n}", icon::USERS))
                                    .color(egui::Color32::WHITE)
                                    .strong()
                                    .size(16.0),
                            );
                        })
                        .response
                        .on_hover_text("hostiles");
                }

                // ISK amount posted (ESS bank, loot, bounty) — "300M", "1.5B".
                if let Some(isk) = r.isk {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(0x4a, 0x3d, 0x10))
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} {}",
                                    icon::COINS,
                                    crate::intel::format_isk(isk)
                                ))
                                .color(egui::Color32::from_rgb(0xff, 0xd9, 0x6b))
                                .strong(),
                            );
                        })
                        .response
                        .on_hover_text("ISK posted");
                }

                // Structures mentioned (Keepstar, Fortizar, …) + distance off, if given.
                for (name, dist) in &r.structures {
                    let label = match dist {
                        Some(d) => format!("{} {name}  {d}", icon::CASTLE_TURRET),
                        None => format!("{} {name}", icon::CASTLE_TURRET),
                    };
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(0x2e, 0x24, 0x4a))
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .color(egui::Color32::from_rgb(0xc4, 0xb5, 0xfd))
                                    .strong(),
                            );
                        })
                        .response
                        .on_hover_text(match dist {
                            Some(d) => format!("{name}, {d} off"),
                            None => name.clone(),
                        });
                }

                // Scanning probes (Core/Combat Scanner Probes) — a badge, not the Probe frigate.
                if let Some(probes) = r.probes {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(0x10, 0x3a, 0x40))
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{} {probes}", icon::MAGNIFYING_GLASS))
                                    .color(egui::Color32::from_rgb(0x7d, 0xd3, 0xde))
                                    .strong(),
                            );
                        })
                        .response
                        .on_hover_text("Scanning probes on D-Scan (someone is scanning)");
                }

                // Clickable system panels.
                for s in &r.systems {
                    let scol = security_color(s.security);
                    let text =
                        egui::RichText::new(format!("{} {}", icon::PLANET, s.name)).color(scol).strong();
                    let panel = ui
                        .add(egui::Button::new(text).fill(scol.gamma_multiply(0.12)))
                        .on_hover_ui(|ui| system_hover(ui, systems, status, s));
                    if panel.clicked() {
                        clicked = Some(IntelClick::System(s.id));
                    }
                }

                // Ship panels with the real EVE hull icon (click -> ship window).
                let ship_icon = ui.text_style_height(&egui::TextStyle::Body);
                for sh in &r.ships {
                    let url = format!("https://images.evetech.net/types/{}/icon?size=32", sh.id);
                    let img = egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(ship_icon));
                    let mut panel = ui
                        .add(egui::Button::image_and_text(img, egui::RichText::new(&sh.name).strong()));
                    if let Some(d) = ship_details.get(&sh.id) {
                        let roles = ship_roles.get(&sh.id).map(|v| v.as_slice()).unwrap_or(&[]);
                        panel = panel.on_hover_ui(|ui| ship_hover(ui, d, roles));
                    }
                    if panel.clicked() {
                        clicked = Some(IntelClick::Ship(sh.id));
                    }
                }

                // Ship classes named only by keyword (no specific hull) — a lighter,
                // italic chip so they read as a type rather than an exact ship.
                for class in &r.classes {
                    ui.add(egui::Button::new(egui::RichText::new(class).italics()))
                        .on_hover_text("Ship class — no exact hull was reported");
                }

                // "<ship/type> TACKLED" badges (a generic one if no target was named).
                let tackled_badge = |ui: &mut egui::Ui, label: String| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(0x5a, 0x18, 0x18))
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .strong()
                                    .color(egui::Color32::from_rgb(0xff, 0x8a, 0x8a)),
                            );
                        });
                };
                for target in &r.tackled_targets {
                    tackled_badge(ui, format!("{target}  TACKLED"));
                }
                if r.tackled && r.tackled_targets.is_empty() && !r.cap_tackled {
                    tackled_badge(ui, "TACKLED".to_string());
                }

                // Pilot panels: names confirmed as real characters — either by ESI
                // (resolved_pilots) or authoritatively by an in-game showinfo char link
                // (char_ids), which always wins regardless of the ESI cache state.
                for name in &r.pilots {
                    // A blacklisted word is never a pilot, even if it's cached/char-linked.
                    if crate::intel::is_pilot_stopword(name) {
                        continue;
                    }
                    let char_linked =
                        r.char_ids.iter().any(|(n, _)| n.eq_ignore_ascii_case(name));
                    if !char_linked && !resolved_pilots.contains_key(name) {
                        continue;
                    }
                    let txt = egui::RichText::new(format!("{} {name}", icon::USER));
                    if ui.add(egui::Button::new(txt)).on_hover_text("Look up pilot").clicked() {
                        clicked = Some(IntelClick::Pilot(name.clone()));
                    }
                }

                // No ship reported now: show each pilot's most recent (≤60 min) hull
                // under a "Last seen as:" label, with the regular ship tooltip.
                if r.ships.is_empty() {
                    let seen: Vec<(i64, String)> = r
                        .pilots
                        .iter()
                        .filter_map(|name| last_ship.get(&name.to_lowercase()))
                        .filter(|(_, _, t)| now - t <= 3600)
                        .map(|(id, ship, _)| (*id, ship.clone()))
                        .collect();
                    if !seen.is_empty() {
                        ui.label(egui::RichText::new("Last seen as:").weak());
                        for (id, ship) in seen {
                            let url = format!("https://images.evetech.net/types/{id}/icon?size=32");
                            let img = egui::Image::new(url)
                                .fit_to_exact_size(egui::Vec2::splat(ship_icon));
                            let mut panel = ui.add(egui::Button::image_and_text(
                                img,
                                egui::RichText::new(&ship).strong(),
                            ));
                            if let Some(d) = ship_details.get(&id) {
                                let roles =
                                    ship_roles.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
                                panel = panel.on_hover_ui(|ui| ship_hover(ui, d, roles));
                            }
                            if panel.clicked() {
                                clicked = Some(IntelClick::Ship(id));
                            }
                        }
                    }
                }

                // Gate panels (a card may name several gates).
                for g in &r.gates {
                    ui.label(
                        egui::RichText::new(format!("{} {g} gate", icon::SIGN_IN)).color(accent).strong(),
                    );
                }

                // Alliance logos for shorthand mentions (frat, init, …).
                for (name, id) in &r.alliances {
                    let url = format!("https://images.evetech.net/alliances/{id}/logo?size=32");
                    ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(20.0, 20.0)))
                        .on_hover_text(name);
                }

                // External link badges (killmail / battle report / dscan).
                for link in &r.links {
                    use crate::intel::LinkKind;
                    match link.kind {
                        LinkKind::Killmail => {
                            // Rich KILL badge: victim ship + portrait + KILL; opens the
                            // Kill window. Icons appear once zKill/ESI enrichment lands.
                            let info = link
                                .kill_id
                                .and_then(|id| kills.lock().unwrap().get(&id).cloned().flatten());
                            ui.horizontal(|ui| {
                                let al_logo = |ui: &mut egui::Ui, al: i64, hover: &str| {
                                    ui.add(egui::Image::new(format!(
                                        "https://images.evetech.net/alliances/{al}/logo?size=32"
                                    ))
                                    .fit_to_exact_size(egui::vec2(18.0, 18.0)))
                                    .on_hover_text(hover);
                                };
                                if let Some(inf) = &info {
                                    // Dominant alliance per side: top attacker ⚔ victim.
                                    if let Some(al) = inf.attacker_alliances.first() {
                                        al_logo(ui, *al, "Top attacker alliance");
                                        ui.label(
                                            egui::RichText::new(icon::CARET_RIGHT).color(red).small(),
                                        );
                                    }
                                    if let Some(ship) = inf.victim_ship {
                                        ui.add(egui::Image::new(format!(
                                            "https://images.evetech.net/types/{ship}/icon?size=32"
                                        ))
                                        .fit_to_exact_size(egui::vec2(18.0, 18.0)));
                                    }
                                    if let Some(ch) = inf.victim_char {
                                        ui.add(egui::Image::new(format!(
                                            "https://images.evetech.net/characters/{ch}/portrait?size=32"
                                        ))
                                        .fit_to_exact_size(egui::vec2(18.0, 18.0)));
                                    }
                                    if let Some(al) = inf.victim_alliance {
                                        al_logo(ui, al, "Victim alliance");
                                    }
                                }
                                let lbl = egui::RichText::new(format!("{} KILL", icon::SKULL))
                                    .color(red)
                                    .strong();
                                if ui.add(egui::Button::new(lbl)).clicked() {
                                    if let Some(id) = link.kill_id {
                                        clicked = Some(IntelClick::Kill(id));
                                    } else {
                                        let _ = open::that(&link.url);
                                    }
                                }
                                if let Some(inf) = &info {
                                    if inf.value > 0.0 {
                                        ui.label(egui::RichText::new(fmt_isk(inf.value)).weak());
                                    }
                                }
                            });
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
                            }
                        }
                        LinkKind::Dscan => {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(format!("{} dscan", icon::RADIO))
                                        .color(accent),
                                ))
                                .on_hover_text(&link.url)
                                .clicked()
                            {
                                let _ = open::that(&link.url);
                            }
                        }
                    }
                }

                // Status flags.
                let tag = |ui: &mut egui::Ui, txt: &str, col: egui::Color32| {
                    ui.label(egui::RichText::new(txt).color(col).strong());
                };
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
                if r.killmail {
                    tag(ui, "KILL", red);
                }
                if r.cyno {
                    tag(ui, "CYNO", red);
                }
                if r.cap_tackled {
                    tag(ui, "CAP TACKLED", red);
                }
                if r.wormhole {
                    tag(ui, "WH", crate::theme::standing::ALLIANCE);
                }
                if r.ess {
                    match &r.ess_time {
                        Some(t) => tag(ui, &format!("ESS {t}"), warn),
                        None => tag(ui, "ESS", warn),
                    }
                }
                if r.skyhook {
                    tag(ui, "SKYHOOK", warn);
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
            // Everything in one wrapping row: badges then reporter·channel at the
            // end (wraps to the next line only if it doesn't fit — no forced row,
            // no vertical stretch).
            ui.horizontal_wrapped(|ui| {
                render(ui);
                if show_reporter {
                    ui.label(
                        egui::RichText::new(format!("·  {} · {}", r.reporter, r.channel)).weak(),
                    );
                }
            });
        })
        .response;

    // A primary click inside the card that no inner system/ship/pilot link consumed toggles
    // the raw message. Detected via input (not a card-wide click widget) so it doesn't steal
    // the inner links' clicks — the card frame would otherwise sit on top of them.
    let bg_click = clicked.is_none()
        && ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|p| resp.rect.contains(p))
        });
    if bg_click {
        ui.ctx().data_mut(|d| d.insert_temp(toggle_id, !show_raw));
    }
    clicked
}

/// Hover tooltip for a ship panel: group, resists, tank, drones, hardpoints, speed.
fn ship_hover(ui: &mut egui::Ui, d: &crate::store::ShipDetails, roles: &[(&'static str, &'static str)]) {
    ui.label(egui::RichText::new(&d.name).strong());
    ui.label(egui::RichText::new(&d.group).weak());
    role_badges(ui, roles); // icons only — bonus text is in the ship window
    ui.separator();
    ship_stats(ui, d);
}

/// Resists / tank / hardpoints / drones / speed for a ship.
/// Pick the loss whose fit to show: latest, or the most common fit signature.
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

/// Cargo contents of a loss (type_id → quantity), auto-stacked: cargo/drone-bay
/// items plus any loaded charges (qty > 1 items found in fitted slots).
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

/// EFT (paste-able) fit string. Slot order: low, mid, high, rig, subsystem, cargo.
/// Loaded charges are moved to cargo (stacked) rather than left on modules.
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
        _ => 4, // subsystem
    };
    for it in &loss.items {
        let s = crate::lookup::slot_of(it.flag);
        // Only modules (qty 1) belong in a fitted slot; charges go to cargo below.
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

/// Online fit sites the user can open a loss in. (id, label.)
const FIT_SITES: &[(&str, &str)] =
    &[("eveship", "EVEShip.fit"), ("workbench", "EVE Workbench"), ("zkillboard", "zKillboard")];

fn site_label(site: &str) -> &str {
    FIT_SITES.iter().find(|(id, _)| *id == site).map(|(_, l)| *l).unwrap_or(site)
}

fn fit_url(site: &str, _ship_id: i64, loss: &crate::lookup::Loss) -> String {
    match site {
        // EVEShip.fit imports a killmail directly and renders the full fit.
        "eveship" => format!("https://eveship.fit/?fit=killmail:{}/{}", loss.killmail_id, loss.hash),
        // EVE Workbench has no kill-import URL; open the importer (paste the EFT).
        "workbench" => "https://eveworkbench.com/fitting".to_owned(),
        _ => format!("https://zkillboard.com/kill/{}/", loss.killmail_id),
    }
}

/// Icon for a sov upgrade on the map.
enum UpgradeIcon {
    /// A mineral/ore item image (type id), for mining arrays.
    Mineral(i64),
    /// A Phosphor glyph (ratting/exploration/cyno/other).
    Glyph(&'static str),
}

/// Categorise a sov upgrade by name → (icon, level 0–5).
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

/// Classify a sov upgrade into one of the filterable kinds (mirrors `upgrade_info`).
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

/// Colour for a sov-upgrade level: white / green / red for level 1 / 2 / 3+.
fn level_color(l: u8) -> egui::Color32 {
    match l {
        2 => egui::Color32::from_rgb(0x5A, 0xC8, 0x6A), // green
        3..=5 => egui::Color32::from_rgb(0xE5, 0x4B, 0x4B), // red
        _ => egui::Color32::WHITE, // level 1 / unknown
    }
}

/// Regions that are not reachable in-game and shouldn't appear on the map.
fn is_hidden_region(region: &str) -> bool {
    // Inaccessible space, hidden from the map and the region picker: wormhole and Jove
    // regions carry a digit in their name (A-R00001, UUA-F4, A821-A…). Pochven
    // (Triglavian) IS shown — it's a real, navigable k-space region.
    region.chars().any(|c| c.is_ascii_digit())
}

/// Broad hull-size class for a ship group (Frigate … Capital).
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

/// Derive the ship's role badges (tank / weapon / utility) from its bonus text.
fn derive_roles(traits: &[(i64, f64, String)]) -> Vec<(&'static str, &'static str)> {
    use egui_phosphor::regular as i;
    let t: String = traits.iter().map(|x| x.2.to_lowercase()).collect::<Vec<_>>().join(" | ");
    let has = |k: &str| t.contains(k);
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    if has("shield") {
        out.push((i::SHIELD, "Shield"));
    }
    if has("armor") {
        // A helmet — clearly distinct from the shield glyph (like the in-game icon).
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

/// Render the role badges as a row of icons with hover labels.
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

/// Effective HP of a layer against an even (omni) damage profile.
fn layer_ehp(hp: f64, r: [u32; 4]) -> f64 {
    if hp <= 0.0 {
        return 0.0;
    }
    let avg_resist = (r[0] + r[1] + r[2] + r[3]) as f64 / 4.0 / 100.0;
    hp / (1.0 - avg_resist).max(0.01)
}

fn ship_stats(ui: &mut egui::Ui, d: &crate::store::ShipDetails) {
    // Damage-type colours (EM / thermal / kinetic / explosive), aligned in columns.
    let dmg_col = [
        egui::Color32::from_rgb(0x5A, 0xA9, 0xE0), // EM — blue
        egui::Color32::from_rgb(0xD6, 0x45, 0x45), // Thermal — red
        egui::Color32::from_rgb(0x9A, 0xA3, 0xA8), // Kinetic — grey
        egui::Color32::from_rgb(0xD6, 0xA6, 0x45), // Explosive — orange
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
                // Background bar sized to the resist %, white text on top.
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

/// Hover tooltip for a system panel: security, location, live conditions.
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

/// Returns `value` trimmed if non-empty, otherwise the fallback.
fn non_empty_or(value: &str, fallback: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        fallback.to_owned()
    } else {
        v.to_owned()
    }
}

/// Colour for a security status: green (hi-sec) / amber (lo-sec) / red (null).
/// Stable pseudo-id from a coalition name (so it gets a consistent colour).
fn coalition_hash(name: &str) -> i64 {
    let mut h: u64 = 1469598103934665603;
    for b in name.to_lowercase().bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    h as i64
}

/// A stable, distinct-ish colour for an alliance id (sovereignty overlay tint).
fn alliance_color(id: i64) -> egui::Color32 {
    let h = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    egui::Color32::from_rgb(
        0x50 | ((h >> 16) as u8 >> 1),
        0x50 | ((h >> 8) as u8 >> 1),
        0x50 | ((h) as u8 >> 1),
    )
}

/// Auto colour generated from a name (alliance / coalition) when not overridden.
fn name_color(name: &str) -> egui::Color32 {
    alliance_color(coalition_hash(name))
}

/// EVE's in-game security-status colours, keyed by security rounded to 0.1.
/// Anything <= 0.0 is the null-sec red.
fn security_color(security: f64) -> egui::Color32 {
    const COLORS: [(u8, u8, u8); 11] = [
        (0xB0, 0x3A, 0x9A), // 0.0 and below — null-sec reddish purple
        (0xD7, 0x30, 0x00), // 0.1
        (0xF0, 0x48, 0x00), // 0.2
        (0xF0, 0x60, 0x00), // 0.3
        (0xD7, 0x77, 0x00), // 0.4
        (0xEF, 0xEF, 0x00), // 0.5
        (0x8F, 0xEF, 0x2F), // 0.6
        (0x00, 0xF0, 0x00), // 0.7
        (0x00, 0xEF, 0x47), // 0.8
        (0x48, 0xF0, 0xC0), // 0.9
        (0x2F, 0xEF, 0xEF), // 1.0
    ];
    let idx = (security * 10.0).round().clamp(0.0, 10.0) as usize;
    let (r, g, b) = COLORS[idx];
    egui::Color32::from_rgb(r, g, b)
}

/// A coloured security-status label, e.g. `0.9` (green) … `-0.3` (red).
fn security_badge(security: f64) -> egui::RichText {
    let sec = (security * 10.0).round() / 10.0;
    egui::RichText::new(format!("{sec:.1}"))
        .color(security_color(security))
        .monospace()
}

/// A labelled sRGB colour picker row; returns true if the colour changed.
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
        // kA(30000001) — J1(31000001) — kB(30000002): a 1-J-hop chain.
        // kA — kC(30000003): a direct k↔k hole.
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
        // kA holds a hole into J-space (J1), so it's marked.
        assert!(o.jspace_holes.contains(&30_000_001));
        // A pure J-space system is never a chain endpoint.
        assert!(!o.direct.iter().any(|&(a, b)| is_jspace(a) || is_jspace(b)));
    }
}
