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
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for r in results {
                            ui.horizontal(|ui| {
                                ui.label(security_badge(r.security));
                                ui.label(egui::RichText::new(r.name).strong());
                                ui.label(egui::RichText::new(r.region).weak());
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
        self.top_bar(ui);
        self.status_bar(ui);
        self.nav_rail(ui);

        egui::CentralPanel::default().show_inside(ui, |ui| match self.view {
            View::Map => self.map_view(ui),
            View::Characters => self.characters_view(ui),
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

/// Returns `value` trimmed if non-empty, otherwise the fallback.
fn non_empty_or(value: &str, fallback: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        fallback.to_owned()
    } else {
        v.to_owned()
    }
}

/// A coloured security-status label, e.g. `0.9` (green) … `-0.3` (red).
fn security_badge(security: f64) -> egui::RichText {
    let sec = (security * 10.0).round() / 10.0;
    let color = if sec >= 0.5 {
        egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
    } else if sec > 0.0 {
        egui::Color32::from_rgb(0xE0, 0xA4, 0x3A)
    } else {
        egui::Color32::from_rgb(0xD8, 0x4C, 0x4C)
    };
    egui::RichText::new(format!("{sec:.1}")).color(color).monospace()
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
