//! The update check and its dialogs, the store warning, and d-scan pasting and sharing.

use super::*;

impl SpaiApp {
    pub(crate) fn poll_update_check(&mut self, ctx: &egui::Context) {
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

    pub(crate) fn update_dialog(&mut self, ctx: &egui::Context) {
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
            // Without this the minimize-to-tray interceptor cancels the close and hides the window,
            // stranding the app in the tray with the relaunch never reached.
            self.really_exit = true;
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
    pub(crate) fn update_check_dialog(&mut self, ctx: &egui::Context) {
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
    pub(crate) fn store_warning_dialog(&mut self, ctx: &egui::Context) {
        let Some(err) = self.store_error.clone() else { return };
        if self.store_warn_dismissed {
            return;
        }
        let locked = self.store.as_ref().is_some_and(crate::store::Store::settings_locked);
        egui::Window::new(format!("{}  Storage problem", egui_phosphor::regular::WARNING))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label("EVE Spai can't read or write its database, so settings and intel history won't be saved this session.");
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&err).weak());
                ui.add_space(4.0);
                // A full disk fails the same way as a permissions problem, so say which it is.
                if self.disk_level == crate::disk::Level::Critical {
                    ui.label(
                        "The disk holding the data folder is full. Free some space and restart; \
                         nothing here is a permission problem.",
                    );
                } else {
                    ui.label(
                        "This is usually a file-permission issue on the data folder. Check that \
                         your user can write to it, or reinstall to a writable location.",
                    );
                }
                if locked {
                    ui.add_space(8.0);
                    ui.label(
                        "Your saved settings could not be read, and could not be backed up either, \
                         so saving now would overwrite the only copy with defaults.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Keep my old settings").clicked() {
                            self.store_warn_dismissed = true;
                        }
                        if ui.button("Start fresh").clicked() {
                            if let Some(store) = &self.store {
                                store.unlock_settings();
                            }
                            self.needs_save = true;
                            self.store_warn_dismissed = true;
                        }
                    });
                    return;
                }
                ui.add_space(8.0);
                if ui.button("Continue anyway").clicked() {
                    self.store_warn_dismissed = true;
                }
            });
    }

    pub(crate) fn poll_dscan_clipboard(&mut self, ctx: &egui::Context) {
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
            // During a rescue, also capture the dscan breakdown onto the rescue state so the FC
            // sees what is on grid without leaving the window.
            #[cfg(feature = "fleet")]
            if self.settings.fc_rescue_enabled && self.rescue.lock().unwrap().active {
                let parsed = crate::rescue::parse_raw_dscan(&text);
                if !parsed.is_empty() {
                    self.rescue.lock().unwrap().dscan = Some(parsed);
                }
            }
            self.dscan_prompt = Some((text, n, PasteKind::Dscan));
        } else if crate::dscan::looks_like_local(&text).is_some() {
            let names = crate::localscan::names_of(&text);
            self.lookup_load(names, ctx);
            self.view = View::Lookup;
        }
    }

    pub(crate) fn is_imperium(&self) -> bool {
        self.settings.intel_channels.iter().any(|c| c.trim().to_lowercase().ends_with(".imperium"))
    }

    pub(crate) fn dscan_uses_adashboard(&self) -> bool {
        match self.settings.dscan_service {
            crate::settings::DscanService::Auto => self.is_imperium(),
            crate::settings::DscanService::Adashboard => true,
            crate::settings::DscanService::DscanInfo => false,
        }
    }

    pub(crate) fn open_adashboard_intel(&self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text);
        let _ = open::that("https://adashboard.info/intel");
    }

    pub(crate) fn start_dscan_upload(&self, ctx: &egui::Context, text: String) {
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
    pub(crate) fn dscan_dialog(&mut self, ctx: &egui::Context) {
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
                    let title = "D-scan";
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
        if dismiss {
            if let Some((text, _, _)) = &self.dscan_prompt {
                self.dscan_dismissed_hash = hash_str(text);
            }
            self.dscan_prompt = None;
            *self.dscan_share.lock().unwrap() = DscanShare::default();
        }
    }
}
