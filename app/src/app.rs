//! The application shell: window, nav rail, top/status bars, settings dialog,
//! theme application, and persistence wiring (docs/DESIGN.md §6).

/// Intel feed type filter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IntelTypeFilter {
    All,
    Hostile,
    Clear,
    Kill,
    Threat,
}

/// A click on an intel card panel.
#[derive(Clone)]
enum IntelClick {
    System(i64),
    Ship(i64),
    Pilot(String),
}

#[derive(Clone, Copy, PartialEq)]
enum PilotSort {
    MostLost,
    Recent,
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
            IntelTypeFilter::Threat => r.spike || r.camp || r.bubble || r.cyno,
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
    settings_open: bool,
    intel_channels_open: bool,
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
    /// Active character name + ESI-resolved system (shared with the location poller).
    player: crate::esi::SharedPlayer,
    /// System graph for UI distance queries (set once the SDE is ready).
    systems: Option<std::sync::Arc<crate::geo::Systems>>,
    /// Live per-system status (incursion/FW/sov), shared with the ESI poller.
    system_status: crate::systemstatus::SharedStatus,
    /// Only alert on reports newer than this (set to launch time to skip backlog).
    last_alert_time: i64,
    /// Per-system alert cooldown (system id -> last alert unix seconds).
    alert_cooldown: std::collections::HashMap<i64, i64>,
    /// Recent fired alerts (unix, text) — shared with the game-log watcher.
    recent_alerts: crate::gamewatcher::AlertLog,
    // --- Map view state ---
    map_view: crate::map::MapView,
    map_initialized: bool,
    map_history: Vec<crate::map::MapView>,
    map_forward: Vec<crate::map::MapView>,
    map_regions: Vec<(i64, String)>,
    map_systems: Vec<crate::store::MapSystem>,
    map_loaded: Option<crate::map::MapView>,
    map_pan: egui::Vec2,
    map_zoom: f32,
    map_follow: bool,
    map_popped: bool,
    /// Use EVE's flattened 2D layout (position2D, in-game look) vs raw geographic x/z.
    map_spaced: bool,
    /// Coordinates actually drawn (geographic or the 2D layout).
    map_draw: Vec<crate::store::MapSystem>,
    map_draw_spaced: bool,
    map_draw_key: Option<(crate::map::MapView, bool)>,
    /// One-shot: centre the map on this system on the next draw (from intel click).
    map_focus: Option<i64>,
    /// Persistently highlighted system on the map (from a search selection).
    map_selected: Option<i64>,
    /// Destination for the in-app route overlay (set via "Set Destination").
    route_destination: Option<i64>,
    map_search: String,
    map_search_sel: usize,
    /// System-info window: the system currently shown (if any).
    system_window: Option<i64>,
    /// Ship-info window: the ship type currently shown (if any).
    ship_window: Option<i64>,
    /// Pilot lookup (zKill) input + shared result.
    pilot_query: String,
    pilot_lookup: crate::lookup::SharedLookup,
    pilot_window_open: bool,
    pilot_sort: PilotSort,
    /// Fit window: (ship type id, which fit).
    fit_view: Option<(i64, FitMode)>,
    /// Resolved pilot-name cache (shared with the chat watcher + resolver thread).
    pilots: crate::pilot::SharedPilots,
    /// Static ship-detail cache (avoids per-frame DB queries).
    ship_cache: std::cell::RefCell<std::collections::HashMap<i64, Option<crate::store::ShipDetails>>>,
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
        let settings = store
            .as_ref()
            .and_then(|s| s.load_settings())
            .unwrap_or_default();

        settings.theme.apply(&cc.egui_ctx);

        // Resolve SDE state from what's already baked; otherwise download on first run.
        let initial = if store.as_ref().map(|s| s.sde_ready()).unwrap_or(false) {
            SdeStatus::Ready
        } else {
            SdeStatus::default()
        };
        let sde_status: SharedStatus = std::sync::Arc::new(std::sync::Mutex::new(initial));
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

        Self {
            store,
            settings,
            view: View::Dashboard,
            settings_open: false,
            intel_channels_open: false,
            active_character: "No character".to_owned(),
            needs_save: false,
            sde_status,
            auth_status: std::sync::Arc::new(std::sync::Mutex::new(AuthStatus::Idle)),
            characters,
            intel_state: std::sync::Arc::new(std::sync::Mutex::new(crate::intel::IntelState::default())),
            watcher_started: false,
            chat_dir: None,
            intel_query: String::new(),
            intel_max_jumps: 0,
            intel_type: IntelTypeFilter::All,
            battles: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            player,
            systems: None,
            system_status: {
                let status: crate::systemstatus::SharedStatus =
                    std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
                crate::systemstatus::spawn(status.clone(), cc.egui_ctx.clone());
                status
            },
            last_alert_time: chrono::Utc::now().timestamp(),
            alert_cooldown: std::collections::HashMap::new(),
            recent_alerts: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            map_view: crate::map::MapView::Universe,
            map_initialized: false,
            map_history: Vec::new(),
            map_forward: Vec::new(),
            map_regions: Vec::new(),
            map_systems: Vec::new(),
            map_loaded: None,
            map_pan: egui::Vec2::ZERO,
            map_zoom: 1.0,
            map_follow: false,
            map_popped: false,
            map_spaced: true,
            map_draw: Vec::new(),
            map_draw_spaced: false,
            map_draw_key: None,
            map_focus: None,
            map_selected: None,
            route_destination: None,
            map_search: String::new(),
            map_search_sel: 0,
            system_window: None,
            ship_window: None,
            pilot_query: String::new(),
            pilot_lookup: std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle)),
            pilot_window_open: false,
            pilot_sort: PilotSort::MostLost,
            fit_view: None,
            ship_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
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
    }

    /// Open the ship-info window for a ship type (from an intel ship panel click).
    fn open_ship(&mut self, ship_id: i64) {
        self.ship_window = Some(ship_id);
    }

    /// Evaluate new intel against alert rules; fire desktop notifications (cooldown
    /// 60 s per system). Only reports newer than launch are considered.
    fn check_alerts(&mut self) {
        let cfg = crate::alerts::AlertConfig {
            enabled: self.settings.alert_enabled,
            within_jumps: self.settings.alert_within_jumps,
        };
        if !cfg.enabled {
            return;
        }
        let player = self.player.lock().unwrap().system_id;
        let systems = self.systems.clone();
        let now = chrono::Utc::now().timestamp();
        let mut hits: Vec<(i64, String)> = Vec::new();
        let mut newest = self.last_alert_time;

        {
            let state = self.intel_state.lock().unwrap();
            for r in &state.reports {
                if r.received <= self.last_alert_time {
                    continue;
                }
                newest = newest.max(r.received);
                if let Some(text) = crate::alerts::evaluate(r, player, systems.as_deref(), &cfg) {
                    let sys_id = r.primary_system().map_or(0, |s| s.id);
                    let last = self.alert_cooldown.get(&sys_id).copied().unwrap_or(0);
                    if now - last >= 60 {
                        hits.push((sys_id, text));
                    }
                }
            }
        }
        self.last_alert_time = newest;

        if !hits.is_empty() {
            let mut log = self.recent_alerts.lock().unwrap();
            for (sys_id, text) in hits {
                self.alert_cooldown.insert(sys_id, now);
                log.push((now, text.clone()));
                notify(text);
            }
            let len = log.len();
            if len > 50 {
                log.drain(0..len - 50);
            }
        }
    }

    fn alerts_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Rule").strong());
        if self.settings.alert_enabled && self.settings.alert_within_jumps > 0 {
            ui.label(format!(
                "Desktop alert on hostiles within {} jumps of the active character.",
                self.settings.alert_within_jumps
            ));
        } else {
            ui.label(egui::RichText::new("Alerts disabled (enable in Settings).").weak());
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Recent alerts").strong());
        let log = self.recent_alerts.lock().unwrap();
        if log.is_empty() {
            ui.label(egui::RichText::new("None yet.").weak());
            return;
        }
        let now = chrono::Utc::now().timestamp();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (t, text) in log.iter().rev() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:>7}", fmt_age(now - t)))
                            .monospace()
                            .weak(),
                    );
                    ui.label(text);
                });
            }
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

        // The battle feed runs whenever the SDE is ready (independent of logs).
        crate::zkill::spawn(
            systems.clone(),
            self.intel_state.clone(),
            self.battles.clone(),
            ctx.clone(),
        );

        if let Some(dir) = self.chat_dir.clone() {
            let ships = std::sync::Arc::new(store.ship_index());
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
                crate::gamewatcher::spawn(game_dir, self.recent_alerts.clone(), ctx.clone());
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

        let player_sys = self.player.lock().unwrap().system_id;
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
        let state = self.intel_state.lock().unwrap();

        let matches: Vec<&crate::intel::IntelReport> = state
            .reports
            .iter()
            .rev()
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
            // Virtualise: only build the cards actually scrolled into view.
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + 18.0;
            egui::ScrollArea::vertical().show_rows(ui, row_h, matches.len(), |ui, range| {
                for i in range {
                    let r = matches[i];
                    // Outdated: superseded by a clear, or older than the configured TTL.
                    let stale = state.is_stale(r) || (now - r.received) > ttl;
                    let from_you =
                        jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id));
                    if let Some(a) = intel_row(
                        ui, r, now, stale, from_you, &systems, &status, &ship_details,
                        &resolved_pilots,
                    ) {
                        action = Some(a);
                    }
                }
            });
        }
        drop(state);
        match action {
            Some(IntelClick::System(id)) => self.open_system(id),
            Some(IntelClick::Ship(id)) => self.open_ship(id),
            Some(IntelClick::Pilot(name)) => {
                self.pilot_query = name;
                crate::lookup::spawn_lookup(self.pilot_query.clone(), self.pilot_lookup.clone(), ui.ctx().clone());
                self.pilot_window_open = true;
            }
            None => {}
        }
    }

    /// Overview: at-a-glance summary of live state.
    fn dashboard_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let now = chrono::Utc::now().timestamp();
        let player_sys = self.player.lock().unwrap().system_id;
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
        let player_sys = self.player.lock().unwrap().system_id;
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
        for c in &self.characters {
            let scope_count = c.scopes.split(' ').filter(|s| !s.is_empty()).count();
            let token_ok = c.expires_at > now;
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
                });
            });
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

    fn pilot_report_ui(&mut self, ui: &mut egui::Ui, report: &crate::lookup::PilotReport) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&report.name).strong());
            ui.label(egui::RichText::new(format!("· {} losses", report.losses.len())).weak());
            if ui.button("zKillboard").clicked() {
                let _ = open::that(format!("https://zkillboard.com/character/{}/", report.character_id));
            }
        });
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
        egui::ScrollArea::vertical().id_salt("pilot_ships").show(ui, |ui| {
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
        let Some((ship_id, mode)) = self.fit_view else {
            return;
        };
        let loss = {
            let state = self.pilot_lookup.lock().unwrap();
            match &*state {
                crate::lookup::LookupState::Done(report) => pick_loss(report, ship_id, mode),
                _ => None,
            }
        };
        if let Some(l) = &loss {
            let mut ids: Vec<i64> = l.items.iter().map(|i| i.type_id).collect();
            ids.push(ship_id);
            self.ensure_type_names(&ids, ctx);
        }
        let ship_name = self.ship_details_cached(ship_id).map(|d| d.name).unwrap_or_default();
        let names = self.type_names.lock().unwrap().clone();
        let mut new_mode = mode;

        let keep = Self::dialog_viewport(ctx, "fit_window", "EVE Spai — Fit", [380.0, 560.0], |ui| {
            ui.horizontal(|ui| {
                let url = format!("https://images.evetech.net/types/{ship_id}/icon?size=32");
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(28.0)));
                ui.heading(&ship_name);
            });
            ui.horizontal(|ui| {
                ui.label("Fit:");
                ui.selectable_value(&mut new_mode, FitMode::Recent, "Most recent");
                ui.selectable_value(&mut new_mode, FitMode::MostUsed, "Most used");
            });
            ui.separator();
            let Some(loss) = &loss else {
                ui.label(egui::RichText::new("No fit found.").weak());
                return;
            };

            egui::ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                use crate::lookup::Slot;
                let section = |ui: &mut egui::Ui, title: &str, slot: Slot| {
                    let items: Vec<&crate::lookup::Item> =
                        loss.items.iter().filter(|i| crate::lookup::slot_of(i.flag) == slot).collect();
                    if items.is_empty() {
                        return;
                    }
                    ui.label(egui::RichText::new(title).strong().color(ui.visuals().hyperlink_color));
                    for it in items {
                        let n = names.get(&it.type_id).cloned().unwrap_or_else(|| "…".to_owned());
                        if it.qty > 1 {
                            ui.label(format!("{n}  ×{}", it.qty));
                        } else {
                            ui.label(n);
                        }
                    }
                    ui.add_space(4.0);
                };
                section(ui, "High", Slot::High);
                section(ui, "Mid", Slot::Mid);
                section(ui, "Low", Slot::Low);
                section(ui, "Rigs", Slot::Rig);
                section(ui, "Subsystems", Slot::Subsystem);
                section(ui, "Cargo & drones", Slot::Cargo);
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Copy EFT").clicked() {
                    ui.ctx().copy_text(eft_string(&ship_name, loss, &names));
                }
                let site = self.settings.fit_site.clone();
                if site.is_empty() {
                    ui.label("Open in:");
                    for (id, label) in [("zkillboard", "zKillboard"), ("osmium", "o.smium")] {
                        if ui.button(label).clicked() {
                            self.settings.fit_site = id.to_owned();
                            self.needs_save = true;
                        }
                    }
                } else if ui.button(format!("Open in {}", site_label(&site))).clicked() {
                    let _ = open::that(fit_url(&site, ship_id, loss));
                }
            });
        });

        if new_mode != mode {
            self.fit_view = Some((ship_id, new_mode));
        } else if !keep {
            self.fit_view = None;
        }
    }

    fn start_sde(&self, ctx: &egui::Context) {
        if let Some(store) = &self.store {
            sde::spawn_download(store.path().to_path_buf(), self.sde_status.clone(), ctx.clone());
        }
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
                    self.draw_map(ui);
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
    fn draw_map(&mut self, ui: &mut egui::Ui) {
        use crate::map::MapView;
        if self.map_regions.is_empty() {
            if let Some(store) = &self.store {
                self.map_regions = store.regions();
            }
        }
        let player_sys = self.player.lock().unwrap().system_id;
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

        // (Re)load systems for the current view, keeping only gate-connected systems
        // (drops wormhole / abyssal islands that have no K-space connections).
        if self.map_loaded != Some(self.map_view) {
            let raw = match self.map_view {
                MapView::Universe => self.store.as_ref().map(|s| s.all_map_systems()),
                MapView::Region(id) => self.store.as_ref().map(|s| s.region_systems(id)),
            }
            .unwrap_or_default();
            self.map_systems = if let Some(g) = &self.systems {
                raw.into_iter().filter(|s| !g.neighbors(s.id).is_empty()).collect()
            } else {
                raw
            };
            self.map_loaded = Some(self.map_view);
        }

        // Drawn coordinates: EVE's flattened 2D layout (position2D) when "Spaced" is
        // on, else raw geographic x/z. The 2D coords are baked, so this is instant.
        let want = (self.map_view, self.map_spaced);
        if self.map_draw_key != Some(want) {
            self.map_draw = if self.map_spaced {
                self.map_systems
                    .iter()
                    .map(|s| crate::store::MapSystem { x: s.x2d, z: s.z2d, ..s.clone() })
                    .collect()
            } else {
                self.map_systems.clone()
            };
            self.map_draw_spaced = self.map_spaced;
            self.map_draw_key = Some(want);
        }
        let schematic = self.map_draw_spaced;

        let Some(bounds) = crate::map::Bounds::of(&self.map_draw) else {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("No systems to show.").weak());
            return;
        };

        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        // Mouse back/forward buttons.
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1)) {
            self.map_back();
        }
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra2)) {
            self.map_forward_nav();
        }
        // Drag pans (and disables follow).
        if resp.dragged() {
            self.map_pan += resp.drag_delta();
            self.map_follow = false;
        }
        // Zoom centred on the cursor.
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
                    let old = self.map_zoom;
                    // Min ~= fit-to-view (can't shrink past the whole map); max lets
                    // individual systems separate.
                    let new = (old * (scroll * 0.0015).exp()).clamp(0.7, 60.0);
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

        // Click a system: open its info window.
        if resp.clicked() {
            if let Some(click) = ui.input(|i| i.pointer.interact_pos()) {
                if let Some(id) = nearest_system(click, &pos, 10.0) {
                    self.open_system(id);
                }
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        // Small uniform dots like the in-game star map.
        let dot = (0.7 * self.map_zoom).clamp(0.6, 2.2);

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

        // Gate links (each pair once); bridges are drawn separately below.
        let line_col = ui.visuals().weak_text_color().gamma_multiply(0.5);
        if let Some(graph) = &self.systems {
            for s in &self.map_draw {
                let p1 = pos[&s.id];
                for &n in graph.neighbors(s.id) {
                    if s.id < n && !bridges.contains(&(s.id, n)) {
                        if let Some(p2) = pos.get(&n) {
                            painter.line_segment([p1, *p2], egui::Stroke::new(1.0, line_col));
                        }
                    }
                }
            }
        }
        let bridge_col = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);
        for &(a, c) in &bridges {
            if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&c)) {
                painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, bridge_col));
            }
        }

        // Player route: animated dashed line flowing toward the destination.
        if let (Some(dest), Some(ps), Some(graph)) =
            (self.route_destination, player_sys, self.systems.as_ref())
        {
            if let Some(route) = graph.path(ps, dest) {
                let phase = (ui.input(|i| i.time) * 28.0) as f32;
                let route_col = egui::Color32::from_rgb(0x4F, 0xC3, 0xF7);
                for w in route.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        dashed_flow(&painter, *p1, *p2, route_col, phase);
                    }
                }
                ui.ctx().request_repaint(); // keep the dashes flowing
            }
        }

        // Jump-range hover. Distances are always true light-years (real coords);
        // in schematic mode we keep the band-coloured highlights but drop the rings
        // (the on-screen distances aren't metric there).
        let hovered_id = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| resp.hovered())
            .and_then(|p| nearest_system(p, &pos, 8.0));
        if let Some(h_id) = hovered_id {
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

        // Systems + overlays.
        let intel_ids: std::collections::HashSet<i64> = {
            let st = self.intel_state.lock().unwrap();
            st.reports
                .iter()
                .filter(|r| !r.clear && !st.is_stale(r))
                .filter_map(|r| r.primary_system().map(|s| s.id))
                .collect()
        };
        // System labels only when few systems are actually on screen (so they don't
        // appear too early / lag). Otherwise region names are labelled below.
        let visible = self.map_draw.iter().filter(|s| rect.contains(pos[&s.id])).count();
        let show_sys_labels = visible <= 60;
        for s in &self.map_draw {
            let p = pos[&s.id];
            painter.circle_filled(p, dot, security_color(s.security));
            if intel_ids.contains(&s.id) {
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(2.0, crate::theme::standing::HOSTILE));
            }
            if player_sys == Some(s.id) {
                painter.circle_stroke(p, dot + 4.0, egui::Stroke::new(2.0, ui.visuals().hyperlink_color));
            }
            if Some(s.id) == hovered_id {
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if self.map_selected == Some(s.id) {
                painter.circle_stroke(p, dot + 6.0, egui::Stroke::new(2.5, egui::Color32::WHITE));
            }
            if show_sys_labels && rect.contains(p) {
                painter.text(
                    p + egui::vec2(6.0, -2.0),
                    egui::Align2::LEFT_CENTER,
                    &s.name,
                    egui::FontId::proportional(13.0),
                    ui.visuals().text_color(),
                );
            }
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
            for (rid, (sum, n)) in acc {
                let c = (sum / n as f32).to_pos2();
                if !rect.contains(c) {
                    continue;
                }
                if let Some((_, name)) = self.map_regions.iter().find(|(id, _)| *id == rid) {
                    painter.text(
                        c,
                        egui::Align2::CENTER_CENTER,
                        name,
                        egui::FontId::proportional(14.0),
                        ui.visuals().weak_text_color(),
                    );
                }
            }
        }

        self.map_controls_overlay(ui, rect);
        self.map_search_overlay(ui, rect);
    }

    /// Floating controls over the map (scope, navigation, follow, pop-out).
    fn map_controls_overlay(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        use crate::map::MapView;
        egui::Area::new(egui::Id::new("map_controls"))
            .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                use egui_phosphor::regular as icon;
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Universe").clicked() {
                            self.map_go(MapView::Universe);
                        }
                        ui.add_enabled_ui(!self.map_history.is_empty(), |ui| {
                            if ui.button(icon::ARROW_LEFT).on_hover_text("Back").clicked() {
                                self.map_back();
                            }
                        });
                        ui.add_enabled_ui(!self.map_forward.is_empty(), |ui| {
                            if ui.button(icon::ARROW_RIGHT).on_hover_text("Forward").clicked() {
                                self.map_forward_nav();
                            }
                        });
                        let current = match self.map_view {
                            MapView::Universe => "Universe".to_owned(),
                            MapView::Region(id) => self
                                .map_regions
                                .iter()
                                .find(|(rid, _)| *rid == id)
                                .map(|(_, n)| n.clone())
                                .unwrap_or_else(|| "Region".to_owned()),
                        };
                        let mut goto: Option<i64> = None;
                        egui::ComboBox::from_id_salt("map_region")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (id, name) in &self.map_regions {
                                    if ui.selectable_label(self.map_view == MapView::Region(*id), name).clicked() {
                                        goto = Some(*id);
                                    }
                                }
                            });
                        if let Some(id) = goto {
                            self.map_go(MapView::Region(id));
                        }
                        if ui.add(egui::Button::new("Follow").selected(self.map_follow)).clicked() {
                            self.map_follow = !self.map_follow;
                        }
                        // EVE's flattened 2D layout (position2D) vs raw geographic x/z.
                        if ui
                            .add(egui::Button::new("Spaced").selected(self.map_spaced))
                            .on_hover_text("EVE's in-game flattened 2D layout")
                            .clicked()
                        {
                            self.map_spaced = !self.map_spaced;
                        }
                        if ui.button("Reset").clicked() {
                            self.map_pan = egui::Vec2::ZERO;
                            self.map_zoom = 1.0;
                            self.map_follow = false;
                        }
                        if ui.button("Pop out").clicked() {
                            self.map_popped = true;
                        }
                        if self.route_destination.is_some() && ui.button("Clear route").clicked() {
                            self.route_destination = None;
                        }
                    });
                });
            });
    }

    /// Search panel at the bottom centre, with a keyboard-navigable dropdown that
    /// opens upward. Selecting a system focuses it (swapping region in region scope).
    fn map_search_overlay(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        use egui_phosphor::regular as icon;
        let mut chosen: Option<i64> = None;
        // Anchor to the map rect's bottom-left corner.
        let screen = ui.ctx().content_rect();
        let offset = egui::vec2(
            rect.left() - screen.left() + 8.0,
            rect.bottom() - screen.bottom() - 10.0,
        );
        egui::Area::new(egui::Id::new("map_search"))
            .anchor(egui::Align2::LEFT_BOTTOM, offset)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let has_query = !self.map_search.trim().is_empty();
                    // Read nav keys (single-line edit ignores up/down, so no conflict).
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
                    if esc {
                        self.map_search.clear();
                    }

                    // Results render above the input (dropdown opens upward).
                    let query = self.map_search.trim().to_owned();
                    if !query.is_empty() {
                        let results = self
                            .store
                            .as_ref()
                            .map(|s| s.search_systems(&query, 8))
                            .unwrap_or_default();
                        if results.is_empty() {
                            ui.label(egui::RichText::new("No match").weak());
                        } else {
                            if down {
                                self.map_search_sel = (self.map_search_sel + 1).min(results.len() - 1);
                            }
                            if up {
                                self.map_search_sel = self.map_search_sel.saturating_sub(1);
                            }
                            self.map_search_sel = self.map_search_sel.min(results.len() - 1);
                            if enter {
                                chosen = Some(results[self.map_search_sel].0);
                            }
                            // Top item nearest the input: render in reverse.
                            for (i, (id, name, sec)) in results.iter().enumerate().rev() {
                                let text = egui::RichText::new(format!(
                                    "{:.1}  {name}",
                                    (sec * 10.0).round() / 10.0
                                ))
                                .color(security_color(*sec));
                                if ui.selectable_label(i == self.map_search_sel, text).clicked() {
                                    chosen = Some(*id);
                                }
                            }
                            ui.separator();
                        }
                    } else {
                        self.map_search_sel = 0;
                    }

                    ui.horizontal(|ui| {
                        ui.label(icon::MAGNIFYING_GLASS);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.map_search)
                                .hint_text("Search system")
                                .desired_width(220.0),
                        );
                        if has_query && ui.button(icon::X).clicked() {
                            self.map_search.clear();
                        }
                    });
                });
            });
        if let Some(id) = chosen {
            self.map_search.clear();
            self.map_search_sel = 0;
            self.focus_map_on_select(id);
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
        self.map_zoom = 12.0;
        self.map_focus = Some(id);
        self.map_selected = Some(id);
    }

    /// Render the popped-out map in its own OS window.
    #[allow(deprecated)] // CentralPanel::show is correct for a viewport root ctx
    fn show_map_viewport(&mut self, ctx: &egui::Context) {
        let mut keep = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("map_window"),
            egui::ViewportBuilder::default()
                .with_title("EVE Spai — Map")
                .with_inner_size([960.0, 720.0]),
            |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| self.draw_map(ui));
                if ctx.input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            },
        );
        if !keep {
            self.map_popped = false;
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
                        let dim = ui.visuals().weak_text_color();
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  ESI offline",
                                egui_phosphor::regular::PLUGS
                            ))
                            .color(dim),
                        );
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(30.0)
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    let intel = self.intel_state.lock().unwrap().reports.len();
                    ui.label(format!("Intel: {intel}"));
                    ui.separator();
                    ui.label(egui::RichText::new(&self.active_character).weak());
                });
            });
    }

    fn nav_rail(&mut self, ui: &mut egui::Ui) {
        let width = if self.settings.nav_expanded {
            nav::WIDTH_EXPANDED
        } else {
            nav::WIDTH_COLLAPSED
        };
        egui::Panel::left("nav_rail")
            .resizable(false)
            .exact_size(width)
            .show_inside(ui, |ui| {
                let mut expanded = self.settings.nav_expanded;
                let selected = nav::rail(ui, self.view, &mut expanded, &mut self.settings_open);
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

                ui.horizontal(|ui| {
                    ui.label(security_badge(info.security));
                    ui.heading(&info.name);
                });
                {
                    // system_chips already shows "< Constellation < Region" + conditions.
                    let status = self.system_status.lock().unwrap();
                    system_chips(ui, &self.systems, &status, id);
                    // ESI activity (last hour).
                    if let Some(f) = status.get(&id) {
                        if f.npc_kills + f.ship_kills + f.pod_kills + f.jumps > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Last hour — jumps {} · ship kills {} · pod kills {} · NPC kills {}",
                                    f.jumps, f.ship_kills, f.pod_kills, f.npc_kills
                                ))
                                .weak(),
                            );
                        }
                    }
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
                            crate::esi::set_waypoint(cid.clone(), cname.clone(), id, true);
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
                            let label = if cnt > 0 {
                                format!(
                                    "{:.1} {} ({cnt})",
                                    (ni.security * 10.0).round() / 10.0,
                                    ni.name
                                )
                            } else {
                                format!("{:.1} {}", (ni.security * 10.0).round() / 10.0, ni.name)
                            };
                            let mut text = egui::RichText::new(label);
                            if cnt > 0 {
                                text = text.color(crate::theme::standing::HOSTILE);
                            }
                            if ui.button(text).clicked() {
                                nav = Some(nid);
                            }
                        }
                    }
                });

                ui.separator();
                ui.label(egui::RichText::new("Intel here").strong());
                egui::ScrollArea::vertical().id_salt("sysintel").max_height(220.0).show(ui, |ui| {
                    let mut any = false;
                    for r in state.reports.iter().rev() {
                        if !r.systems.iter().any(|s| s.id == id) {
                            continue;
                        }
                        any = true;
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
                                ui.label(egui::RichText::new("CLEAR").color(egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)));
                            }
                            for sh in &r.ships {
                                ui.label(egui::RichText::new(&sh.name).weak());
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(&r.reporter).weak());
                            });
                        });
                    }
                    if !any {
                        ui.label(egui::RichText::new("No recent intel.").weak());
                    }
                });
                // TODO: neighbouring intel density over time (sparkline) — deferred.
            },
        );

        if let Some(nid) = nav {
            self.system_window = Some(nid);
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

    /// Ship-info window: render image, hull class, resists, fitting, speed.
    fn ship_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.ship_window else {
            return;
        };
        let details = self.store.as_ref().and_then(|s| s.ship_details(id));
        let keep = Self::dialog_viewport(ctx, "ship_window", "EVE Spai — Ship", [360.0, 540.0], |ui| {
            ui.horizontal(|ui| {
                let url = format!("https://images.evetech.net/types/{id}/render?size=128");
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(96.0)));
                ui.vertical(|ui| match &details {
                    Some(d) => {
                        ui.heading(&d.name);
                        ui.label(egui::RichText::new(&d.group).weak());
                    }
                    None => {
                        ui.heading("Ship");
                        ui.label(egui::RichText::new("No SDE details.").weak());
                    }
                });
            });
            ui.separator();
            if let Some(d) = &details {
                ship_stats(ui, d);
            }
        });
        if !keep {
            self.ship_window = None;
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
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                ui.add_space(4.0);
                if ui.button("Add channel").clicked() {
                    self.settings.intel_channels.push(String::new());
                    changed = true;
                }
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.intel_channels_open = false;
        }
    }

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut changed = false;
        let mut new_theme: Option<Theme> = None;

        let keep = Self::dialog_viewport(
            ctx,
            "settings_window",
            "EVE Spai — Settings",
            [470.0, 700.0],
            |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                        .checkbox(&mut self.settings.alert_enabled, "Desktop alert on nearby hostiles")
                        .changed();
                    ui.horizontal(|ui| {
                        ui.label("Within jumps:");
                        changed |= ui
                            .add(egui::DragValue::new(&mut self.settings.alert_within_jumps).range(0..=20))
                            .changed();
                    });
                    changed |= ui
                        .checkbox(&mut self.settings.alert_combat, "Combat alerts (under attack / scrambled)")
                        .changed();

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
                });
            },
        );

        if let Some(theme) = new_theme {
            self.settings.theme = theme;
            changed = true;
        }
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.settings_open = false;
        }
    }
}

impl eframe::App for SpaiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Re-apply the theme every frame so colour edits are reflected live (cheap).
        self.settings.theme.apply(&ctx);

        self.refresh_characters();
        self.player.lock().unwrap().active_name = self.active_character.clone();
        self.maybe_start_watcher(&ctx);
        self.check_alerts();
        self.top_bar(ui);
        self.status_bar(ui);
        self.nav_rail(ui);

        egui::CentralPanel::default().show_inside(ui, |ui| match self.view {
            View::Dashboard => self.dashboard_view(ui),
            View::Map => self.map_view(ui),
            View::Characters => self.characters_view(ui),
            View::Intel => self.intel_view(ui),
            View::Battles => self.battles_view(ui),
            View::Alerts => self.alerts_view(ui),
        });

        self.settings_dialog(&ctx);
        self.intel_channels_window(&ctx);
        self.system_window(&ctx);
        self.ship_window(&ctx);
        self.pilot_window(&ctx);
        self.fit_window(&ctx);
        if self.map_popped {
            self.show_map_viewport(&ctx);
        }

        if self.needs_save {
            self.persist();
        }
    }

    fn on_exit(&mut self) {
        self.persist();
    }
}

/// Fire a desktop notification off the UI thread (dbus can block).
fn notify(text: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .summary("EVE Spai")
            .body(&text)
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
    let mut d = -(phase % period);
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

/// Jumps from the player's system to a target system, if both are known.
fn jumps_from_you(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    player_sys: Option<i64>,
    target: Option<i64>,
) -> Option<u32> {
    let (sys, p, t) = (systems.as_ref()?, player_sys?, target?);
    sys.jumps(t, p, 50)
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
    use crate::theme::standing;
    if let Some(info) = systems.as_ref().and_then(|s| s.info_of(system_id)) {
        let loc = match (info.constellation.as_str(), info.region.as_str()) {
            ("", "") => String::new(),
            ("", r) => format!("< {r}"),
            (c, "") => format!("< {c}"),
            (c, r) => format!("< {c} < {r}"),
        };
        if !loc.is_empty() {
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
        if let Some(sov) = &f.sov {
            ui.label(egui::RichText::new(format!("Sov: {sov}")).color(standing::CORP));
        }
    }
}

/// A weak "Nj" distance-from-you chip (blank if unknown).
fn from_you_chip(ui: &mut egui::Ui, from_you: Option<u32>) {
    if let Some(j) = from_you {
        let txt = if j == 0 {
            "here".to_owned()
        } else {
            format!("{j}j")
        };
        ui.label(egui::RichText::new(txt).weak());
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
fn intel_row(
    ui: &mut egui::Ui,
    r: &crate::intel::IntelReport,
    now: i64,
    stale: bool,
    from_you: Option<u32>,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    ship_details: &std::collections::HashMap<i64, crate::store::ShipDetails>,
    resolved_pilots: &std::collections::HashMap<String, i64>,
) -> Option<IntelClick> {
    use egui_phosphor::regular as icon;
    let age = (now - r.received).max(0);
    let green = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
    let warn = crate::theme::standing::WARNING;
    let red = crate::theme::standing::HOSTILE;
    let accent = ui.visuals().hyperlink_color;
    let jumps_color = crate::theme::standing::CORP;

    // Report type drives the background tint and a leading icon.
    let (tint, type_icon) = if r.clear {
        (green, icon::CHECK_CIRCLE)
    } else if r.killmail {
        (egui::Color32::from_rgb(0x8A, 0x2A, 0x2A), icon::SKULL)
    } else if r.spike || r.camp || r.bubble || r.cyno {
        (red, icon::WARNING_OCTAGON)
    } else if r.no_visual {
        (warn, icon::EYE_SLASH)
    } else if !r.systems.is_empty() || r.count.is_some() {
        (red, icon::WARNING)
    } else {
        (ui.visuals().weak_text_color(), icon::INFO)
    };

    let mut clicked: Option<IntelClick> = None;
    let resp = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 4))
        .fill(tint.gamma_multiply(if stale { 0.05 } else { 0.13 }))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Reporter + channel pinned to the right; everything else fills from
                // the left and stays vertically centred.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{} · {}", r.reporter, r.channel)).weak());
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let h = ui.text_style_height(&egui::TextStyle::Body);
                let col = |ui: &mut egui::Ui, w: f32, add: &dyn Fn(&mut egui::Ui)| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(w, h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| add(ui),
                    );
                };
                // Fixed columns so time/jumps line up across rows.
                col(ui, 16.0, &|ui| {
                    ui.label(egui::RichText::new(type_icon).color(tint));
                });
                col(ui, 58.0, &|ui| {
                    ui.label(egui::RichText::new(fmt_age(age)).monospace().weak());
                });
                col(ui, 40.0, &|ui| {
                    if let Some(j) = from_you {
                        let t = if j == 0 { "here".to_owned() } else { format!("{j}j") };
                        // Distinct from the (weak) time column.
                        ui.label(egui::RichText::new(t).monospace().color(jumps_color));
                    }
                });

                // Hostile-count panel.
                if let Some(n) = r.count {
                    ui.label(egui::RichText::new(format!("{} {n}", icon::USERS)).color(red).strong())
                        .on_hover_text("hostiles");
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
                        panel = panel.on_hover_ui(|ui| ship_hover(ui, d));
                    }
                    if panel.clicked() {
                        clicked = Some(IntelClick::Ship(sh.id));
                    }
                }

                // Pilot panels (only names confirmed as real characters by ESI).
                for name in &r.pilots {
                    if !resolved_pilots.contains_key(name) {
                        continue;
                    }
                    let txt = egui::RichText::new(format!("{} {name}", icon::USER));
                    if ui.add(egui::Button::new(txt)).on_hover_text("Look up pilot").clicked() {
                        clicked = Some(IntelClick::Pilot(name.clone()));
                    }
                }

                // Gate panel.
                if let Some(g) = &r.gate {
                    ui.label(
                        egui::RichText::new(format!("{} {g} gate", icon::SIGN_IN)).color(accent).strong(),
                    );
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
                if r.bubble {
                    tag(ui, "BUBBLE", warn);
                }
                if r.killmail {
                    tag(ui, "KILL", red);
                }
                if r.cyno {
                    tag(ui, "CYNO", red);
                }
                if r.wormhole {
                    tag(ui, "WH", crate::theme::standing::ALLIANCE);
                }
                if r.ess {
                    tag(ui, "ESS", warn);
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
                    });
                });
            });
        })
        .response;

    // Raw message available on hover, never shown inline.
    resp.on_hover_text(&r.text);
    clicked
}

/// Hover tooltip for a ship panel: group, resists, tank, drones, hardpoints, speed.
fn ship_hover(ui: &mut egui::Ui, d: &crate::store::ShipDetails) {
    ui.label(egui::RichText::new(&d.name).strong());
    ui.label(egui::RichText::new(&d.group).weak());
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

/// EFT (paste-able) fit string. Slot order: low, mid, high, rig, subsystem, cargo.
fn eft_string(
    ship: &str,
    loss: &crate::lookup::Loss,
    names: &std::collections::HashMap<i64, String>,
) -> String {
    use crate::lookup::Slot;
    let name = |id: i64| names.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"));
    let mut sections: Vec<Vec<String>> = vec![Vec::new(); 6];
    let idx = |s: Slot| match s {
        Slot::Low => 0,
        Slot::Mid => 1,
        Slot::High => 2,
        Slot::Rig => 3,
        Slot::Subsystem => 4,
        _ => 5,
    };
    for it in &loss.items {
        let s = crate::lookup::slot_of(it.flag);
        let bucket = &mut sections[idx(s)];
        if matches!(s, Slot::Cargo | Slot::Other) {
            bucket.push(if it.qty > 1 {
                format!("{} x{}", name(it.type_id), it.qty)
            } else {
                name(it.type_id)
            });
        } else {
            for _ in 0..it.qty.max(1) {
                bucket.push(name(it.type_id));
            }
        }
    }
    let mut out = format!("[{ship}, EVE Spai]\n");
    for (i, sec) in sections.iter().enumerate() {
        for line in sec {
            out.push_str(line);
            out.push('\n');
        }
        if i < 5 {
            out.push('\n');
        }
    }
    out
}

/// EVE "DNA" fitting string (fitted slots only) for o.smium and similar.
fn dna_string(ship_id: i64, loss: &crate::lookup::Loss) -> String {
    use crate::lookup::Slot;
    let mut counts: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for it in &loss.items {
        if matches!(crate::lookup::slot_of(it.flag), Slot::Cargo | Slot::Other) {
            continue;
        }
        *counts.entry(it.type_id).or_insert(0) += it.qty.max(1);
    }
    let mut s = format!("{ship_id}:");
    for (t, q) in counts {
        s.push_str(&format!("{t};{q}:"));
    }
    s.push(':');
    s
}

fn site_label(site: &str) -> &str {
    match site {
        "zkillboard" => "zKillboard",
        "osmium" => "o.smium",
        other => other,
    }
}

fn fit_url(site: &str, ship_id: i64, loss: &crate::lookup::Loss) -> String {
    match site {
        "osmium" => format!("https://o.smium.org/loadout/dna/{}", dna_string(ship_id, loss)),
        _ => format!("https://zkillboard.com/kill/{}/", loss.killmail_id),
    }
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
                ui.label(egui::RichText::new(format!("{}%", r[i])).color(dmg_col[i]));
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
