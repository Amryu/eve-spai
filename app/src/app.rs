//! The application shell: window, nav rail, top/status bars, settings dialog,
//! theme application, and persistence wiring (docs/DESIGN.md §6).

use crate::auth::{self, AuthStatus, SharedAuth};
use crate::nav::{self, View};
use crate::sde::{self, SdeStatus, SharedStatus};
use crate::settings::Settings;
use crate::store::{CharacterRow, Store};
use crate::theme::{Rgb, Theme};
use crate::views;

pub struct SpaiApp {
    store: Option<Store>,
    settings: Settings,
    view: View,
    settings_open: bool,
    active_character: String,
    /// Settings changed this frame and should be persisted.
    needs_save: bool,
    /// SDE download/bake state (shared with the background worker).
    sde_status: SharedStatus,
    /// Map-view system search box.
    sde_query: String,
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
    /// Intel-view search box.
    intel_query: String,
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
    /// Recent fired alerts (unix, text) for the Alerts view.
    recent_alerts: Vec<(i64, String)>,
}

impl SpaiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load the Phosphor icon font into the proportional family so icons render
        // inline with text everywhere (nav rail, buttons).
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let store = Store::open().map_err(|e| eprintln!("store: {e:#}")).ok();
        let settings = store
            .as_ref()
            .and_then(|s| s.load_settings())
            .unwrap_or_default();

        settings.theme.apply(&cc.egui_ctx);

        // Resolve SDE state from what's already baked; otherwise download on first run.
        let initial = store
            .as_ref()
            .and_then(|s| s.sde_summary())
            .map(|(systems, regions, version)| SdeStatus::Ready {
                systems,
                regions,
                version,
            })
            .unwrap_or_default();
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
            active_character: "No character".to_owned(),
            needs_save: false,
            sde_status,
            sde_query: String::new(),
            auth_status: std::sync::Arc::new(std::sync::Mutex::new(AuthStatus::Idle)),
            characters,
            intel_state: std::sync::Arc::new(std::sync::Mutex::new(crate::intel::IntelState::default())),
            watcher_started: false,
            chat_dir: None,
            intel_query: String::new(),
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
            recent_alerts: Vec::new(),
        }
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

        for (sys_id, text) in hits {
            self.alert_cooldown.insert(sys_id, now);
            self.recent_alerts.push((now, text.clone()));
            notify(text);
        }
        if self.recent_alerts.len() > 50 {
            let drop = self.recent_alerts.len() - 50;
            self.recent_alerts.drain(0..drop);
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
        if self.recent_alerts.is_empty() {
            ui.label(egui::RichText::new("None yet.").weak());
            return;
        }
        let now = chrono::Utc::now().timestamp();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (t, text) in self.recent_alerts.iter().rev() {
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
            crate::watcher::spawn(
                dir,
                self.settings.intel_channels.clone(),
                systems,
                self.intel_state.clone(),
                ctx.clone(),
            );
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

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Filter").weak());
            ui.text_edit_singleline(&mut self.intel_query);
        });
        ui.add_space(6.0);

        let now = chrono::Utc::now().timestamp();
        let query = self.intel_query.trim().to_lowercase();
        let state = self.intel_state.lock().unwrap();

        let matches: Vec<&crate::intel::IntelReport> = state
            .reports
            .iter()
            .rev()
            .filter(|r| {
                query.is_empty()
                    || r.text.to_lowercase().contains(&query)
                    || r.channel.to_lowercase().contains(&query)
                    || r.systems.iter().any(|s| s.name.to_lowercase().contains(&query))
            })
            .collect();

        ui.label(egui::RichText::new(format!("{} reports", matches.len())).weak());
        ui.add_space(4.0);

        let player_sys = self.player.lock().unwrap().system_id;
        let systems = self.systems.clone();
        let status = self.system_status.lock().unwrap();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for r in matches {
                let stale = state.is_stale(r);
                let from_you = jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id));
                intel_row(ui, r, now, stale, from_you, &systems, &status);
                ui.add_space(2.0);
            }
        });
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

    fn start_sde(&self, ctx: &egui::Context) {
        if let Some(store) = &self.store {
            sde::spawn_download(store.path().to_path_buf(), self.sde_status.clone(), ctx.clone());
        }
    }

    /// The Map view (M1: SDE status + system lookup; the rendered map lands in M3).
    fn map_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        let status = self.sde_status.lock().unwrap().clone();
        match status {
            SdeStatus::Ready {
                systems,
                regions,
                version,
            } => {
                ui.label(format!(
                    "Static data ready — {systems} systems, {regions} regions (SDE {version})"
                ));
                ui.add_space(10.0);
                ui.label(egui::RichText::new("System lookup").strong());
                ui.text_edit_singleline(&mut self.sde_query);
                ui.add_space(4.0);

                let results = self
                    .store
                    .as_ref()
                    .map(|s| s.find_systems(&self.sde_query, 14))
                    .unwrap_or_default();
                let systems = self.systems.clone();
                let status = self.system_status.lock().unwrap();
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for r in results {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(security_badge(r.security));
                                ui.label(egui::RichText::new(&r.name).strong());
                                system_chips(ui, &systems, &status, r.id);
                            });
                        }
                    });

                ui.add_space(10.0);
                if ui.button("Re-download static data").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
            SdeStatus::Downloading(msg) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(msg);
                });
            }
            SdeStatus::NotReady => {
                ui.label("Static data has not been downloaded yet.");
                if ui.button("Download static data").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
            SdeStatus::Failed(err) => {
                let warn = crate::theme::standing::WARNING;
                ui.colored_label(warn, format!("SDE download failed: {err}"));
                if ui.button("Retry").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
        }

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(
                "The 2D region map renders here next, using these coordinates. (Milestone M3.)",
            )
            .weak(),
        );
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
                    ui.label("Intel: 0");
                    ui.separator();
                    ui.label(egui::RichText::new("M0 scaffold — no live data yet").weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.small_button("Settings").clicked() {
                            self.settings_open = true;
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

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = self.settings_open;
        let mut changed = false;
        let mut new_theme: Option<Theme> = None;

        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .show(ctx, |ui| {
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
                    ui.label("EVE chat-log directory");
                    changed |= ui
                        .text_edit_singleline(&mut self.settings.eve_logs_dir)
                        .changed();
                    ui.label("EVE settings directory");
                    changed |= ui
                        .text_edit_singleline(&mut self.settings.eve_settings_dir)
                        .changed();

                    ui.separator();

                    // --- EVE SSO ---
                    ui.label(egui::RichText::new("EVE SSO").strong());
                    ui.label(egui::RichText::new("Application client ID (PKCE)").weak());
                    changed |= ui
                        .text_edit_singleline(&mut self.settings.sso_client_id)
                        .changed();
                    ui.label(egui::RichText::new("Callback URL").weak());
                    changed |= ui
                        .text_edit_singleline(&mut self.settings.sso_callback)
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
                    ui.label(egui::RichText::new("Intel channels").strong());
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
                    if ui.button("Add channel").clicked() {
                        self.settings.intel_channels.push(String::new());
                        changed = true;
                    }
                });
            });

        if let Some(theme) = new_theme {
            self.settings.theme = theme;
            changed = true;
        }
        if changed {
            self.needs_save = true;
        }
        self.settings_open = open;
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
            View::Map => self.map_view(ui),
            View::Characters => self.characters_view(ui),
            View::Intel => self.intel_view(ui),
            View::Battles => self.battles_view(ui),
            View::Alerts => self.alerts_view(ui),
            other => views::show(ui, other),
        });

        self.settings_dialog(&ctx);

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
            ui.label(egui::RichText::new(loc).weak().small());
        }
        // Faction = rats / NPC sov; only meaningful in low/null (highsec is CONCORD).
        if !info.faction.is_empty() && info.security < 0.5 {
            ui.label(egui::RichText::new(&info.faction).small().color(standing::NEUTRAL));
        }
    }
    if let Some(f) = status.get(&system_id) {
        if f.incursion {
            ui.label(egui::RichText::new("INCURSION").small().color(standing::ALLIANCE));
        }
        if let Some(fw) = &f.fw {
            ui.label(egui::RichText::new(format!("FW {fw}")).small().color(standing::WARNING));
        }
        if let Some(sov) = &f.sov {
            ui.label(egui::RichText::new(format!("Sov: {sov}")).small().color(standing::CORP));
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
        ui.label(egui::RichText::new(txt).weak().small());
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
                    .small(),
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
fn intel_row(
    ui: &mut egui::Ui,
    r: &crate::intel::IntelReport,
    now: i64,
    stale: bool,
    from_you: Option<u32>,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
) {
    let age = (now - r.received).max(0);
    // Fade older reports toward the background; outdated (cleared) ones fade hard.
    let fade = if stale {
        0.35
    } else {
        1.0 - (age as f32 / crate::intel::DEFAULT_TTL_SECS as f32).clamp(0.0, 0.8)
    };
    let dim = |c: egui::Color32| c.gamma_multiply(fade);
    let text_col = ui.visuals().text_color();

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());

        // Primary, parsed line: age · systems · count · status · movement.
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{:>7}", fmt_age(age))).monospace().weak());
            from_you_chip(ui, from_you);

            for s in &r.systems {
                ui.label(security_badge(s.security).color(dim(security_color(s.security))));
                ui.label(egui::RichText::new(&s.name).strong().color(dim(text_col)));
                system_chips(ui, systems, status, s.id);
            }

            if let Some(n) = r.count {
                ui.label(egui::RichText::new(format!("{n}x")).strong().color(dim(text_col)));
            }

            let tag = |ui: &mut egui::Ui, txt: &str, col: egui::Color32| {
                ui.label(egui::RichText::new(txt).color(dim(col)).strong());
            };
            let green = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
            let warn = crate::theme::standing::WARNING;
            let red = crate::theme::standing::HOSTILE;
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

            if let Some(gate) = &r.gate {
                ui.label(
                    egui::RichText::new(format!("on {gate} gate"))
                        .color(dim(text_col))
                        .italics(),
                );
            }

            if let Some(m) = &r.movement {
                let arrow = egui_phosphor::regular::ARROW_LEFT;
                let hint = match m.jumps {
                    Some(j) => format!("{arrow} {} ({j}j)", m.from),
                    None => format!("{arrow} {}", m.from),
                };
                ui.label(egui::RichText::new(hint).italics().color(dim(text_col)));
            }
            if stale {
                ui.label(egui::RichText::new("· outdated").italics().weak());
            }
        });

        // Secondary, de-emphasised raw message (the exact words matter less).
        ui.horizontal_wrapped(|ui| {
            let faint = text_col.gamma_multiply((fade * 0.7).max(0.3));
            ui.label(egui::RichText::new(format!("{}:", r.reporter)).small().color(faint));
            ui.label(egui::RichText::new(&r.text).small().color(faint));
        });
    });
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
fn security_color(security: f64) -> egui::Color32 {
    let sec = (security * 10.0).round() / 10.0;
    if sec >= 0.5 {
        egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
    } else if sec > 0.0 {
        egui::Color32::from_rgb(0xE0, 0xA4, 0x3A)
    } else {
        egui::Color32::from_rgb(0xD8, 0x4C, 0x4C)
    }
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
