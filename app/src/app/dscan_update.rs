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

    /// The user's download source choices, as the updater reads them.
    pub(crate) fn update_prefs(&self) -> crate::update::Prefs {
        crate::update::Prefs { mirror_ok: self.settings.update_mirror_ok, prefer_mirror: self.settings.update_mirror_ok && self.settings.update_prefer_mirror }
    }

    /// Where "Download manually" leads: the mirror for a user who prefers it.
    pub(crate) fn update_manual_url(&self, av: &crate::update::Available) -> String {
        if self.update_prefs().prefer_mirror { crate::update::mirror_download_url() } else { av.html_url.clone() }
    }

    pub(crate) fn restart_for_update(&mut self, ctx: &egui::Context) {
        // Without this the minimize-to-tray interceptor cancels the close and hides the window,
        // stranding the app in the tray with the relaunch never reached.
        self.really_exit = true;
        // Closing runs `on_exit` (settings persisted) and only then is
        // the single-instance lock free, so main.rs does the relaunch after the loop returns.
        crate::update::request_restart();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub(crate) fn update_dialog(&mut self, ctx: &egui::Context) {
        let st = self.update.lock().unwrap().clone();
        let Some(av) = st.available.clone() else { return };
        if av.version == self.settings.update_skip_version {
            return;
        }
        let busy = st.installing || st.done;
        // Put away while busy, the status bar carries it; put away before, it asks again in an hour.
        if (busy && st.hidden) || (!busy && self.update_dismissed) {
            return;
        }
        let mut open = true;
        let mut start_install = false;
        let mut restart = false;
        let mut hide = false;
        let mut answer: Option<bool> = None;
        egui::Window::new(trf!("{icon}  Update available", icon = egui_phosphor::regular::DOWNLOAD_SIMPLE))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                if st.done {
                    ui.label(trf!("Updated to v{v}. It applies on restart.", v = av.version));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr!("Restart now")).clicked() {
                            restart = true;
                        }
                        if ui.button(tr!("Later")).on_hover_text(tr!("The status bar keeps a restart button")).clicked() {
                            hide = true;
                        }
                    });
                    return;
                }
                if st.installing {
                    match st.progress {
                        Some(p) => {
                            let frac = if p.total > 0 { p.received as f32 / p.total as f32 } else { 0.0 };
                            ui.label(match p.source {
                                crate::update::Source::Github => trf!("Downloading v{v} from GitHub", v = av.version),
                                crate::update::Source::Mirror => trf!("Downloading v{v} from eve-spai.com", v = av.version),
                            });
                            ui.add(egui::ProgressBar::new(frac).desired_width(400.0).text(trf!(
                                "{got} of {total} · {rate}",
                                got = fmt_bytes(p.received),
                                total = fmt_bytes(p.total),
                                rate = fmt_rate(p.rate)
                            )));
                        }
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(tr!("Downloading update…"));
                            });
                        }
                    }
                    if let (Some(rate), None) = (st.slow, st.use_mirror) {
                        ui.add_space(6.0);
                        ui.colored_label(crate::theme::standing::WARNING, trf!("GitHub is slow right now ({rate}).", rate = fmt_rate(rate)));
                        ui.add(egui::Label::new(tr!("Download from eve-spai.com instead? The file is checked against GitHub's own checksum either way.")).wrap());
                        ui.checkbox(&mut self.update_always_mirror, tr!("Always download updates from eve-spai.com"));
                        ui.horizontal(|ui| {
                            if ui.button(tr!("Use eve-spai.com")).clicked() {
                                answer = Some(true);
                            }
                            if ui.button(tr!("Keep GitHub")).clicked() {
                                answer = Some(false);
                            }
                        });
                    }
                    ui.add_space(6.0);
                    if ui.button(tr!("Hide")).on_hover_text(tr!("The download goes on, its progress in the status bar")).clicked() {
                        hide = true;
                    }
                    return;
                }
                if let Some(e) = &st.error {
                    ui.colored_label(crate::theme::standing::WARNING, trf!("Update failed: {e}", e = e));
                    ui.hyperlink_to(tr!("Download manually"), self.update_manual_url(&av));
                    ui.add_space(4.0);
                }
                ui.label(trf!("EVE Spai v{v} is available. You have v{v2}.", v = av.version, v2 = crate::update::current()));
                ui.hyperlink_to(tr!("Release notes"), &av.html_url);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr!("Yes, update")).clicked() {
                        start_install = true;
                    }
                    if ui.button(tr!("No")).clicked() {
                        self.settings.update_skip_version = av.version.clone();
                        self.needs_save = true;
                    }
                    if ui.button(tr!("Ask me again later")).clicked() {
                        self.update_dismissed = true;
                    }
                });
            });
        if !open {
            if busy {
                hide = true;
            } else {
                self.update_dismissed = true;
            }
        }
        if hide {
            self.update.lock().unwrap().hidden = true;
        }
        if let Some(yes) = answer {
            if yes {
                self.settings.update_mirror_ok = true;
                self.settings.update_prefer_mirror = self.update_always_mirror;
                self.needs_save = true;
            }
            let mut s = self.update.lock().unwrap();
            s.use_mirror = Some(yes);
            s.slow = None;
        }
        if start_install {
            if av.asset.is_some() {
                {
                    let mut s = self.update.lock().unwrap();
                    s.installing = true;
                    s.error = None;
                    s.progress = None;
                    s.slow = None;
                    s.use_mirror = None;
                    s.hidden = false;
                }
                let (upd, ctx2, prefs) = (self.update.clone(), ctx.clone(), self.update_prefs());
                std::thread::spawn(move || {
                    let res = crate::update::install(&av, prefs, &upd, &ctx2);
                    let mut s = upd.lock().unwrap();
                    s.installing = false;
                    s.progress = None;
                    s.slow = None;
                    match res {
                        Ok(()) => s.done = true,
                        Err(e) => {
                            s.error = Some(format!("{e:#}"));
                            // A failure needs reading, wherever the window was.
                            s.hidden = false;
                        }
                    }
                    ctx2.request_repaint();
                });
            } else {
                let _ = crate::launch::that(&self.update_manual_url(&av));
                self.update_dismissed = true;
            }
        }
        if restart {
            self.restart_for_update(ctx);
        }
    }

    /// The update in the status bar: what is available, how far the download is, a question about
    /// GitHub's speed, or the restart that finishes it. A click brings the window back.
    pub(crate) fn update_status(&mut self, ui: &mut egui::Ui) {
        let st = self.update.lock().unwrap().clone();
        let Some(av) = st.available.clone() else { return };
        if av.version == self.settings.update_skip_version {
            return;
        }
        let green = egui::Color32::from_rgb(0x5a, 0xc8, 0x7a);
        let (text, colour, tip) = if st.done {
            (trf!("● v{v} ready · restart", v = av.version), green, tr!("Restart to finish the update"))
        } else if st.installing && st.slow.is_some() && st.use_mirror.is_none() {
            (tr!("● GitHub is slow · switch?").to_owned(), crate::theme::standing::WARNING, tr!("Show the update window"))
        } else if st.installing {
            let pct = st.progress.filter(|p| p.total > 0).map_or(0, |p| (p.received * 100 / p.total) as u32);
            let rate = st.progress.map_or(String::new(), |p| format!(" · {}", fmt_rate(p.rate)));
            (trf!("● Updating to v{v}: {pct}%{rate}", v = av.version, pct = pct, rate = rate), green, tr!("Show the update window"))
        } else if st.error.is_some() {
            (tr!("● Update failed").to_owned(), crate::theme::standing::WARNING, tr!("Show the update window"))
        } else {
            (trf!("● v{v} available", v = av.version), green, tr!("A newer version is available. Click to see it."))
        };
        let r = ui.add(egui::Label::new(egui::RichText::new(text).color(colour)).sense(egui::Sense::click())).on_hover_text(tip);
        if r.clicked() {
            if st.done {
                self.restart_for_update(ui.ctx());
            } else {
                self.update.lock().unwrap().hidden = false;
                self.update_dismissed = false;
            }
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
        egui::Window::new(trf!("{icon}  Check for updates", icon = egui_phosphor::regular::ARROWS_CLOCKWISE))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .show(ctx, |ui| {
                if st.checking {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr!("Checking for updates…"));
                    });
                    return;
                }
                if let Some(e) = &st.check_failed {
                    ui.colored_label(
                        crate::theme::standing::WARNING,
                        trf!("Couldn't check for updates: {e}", e = e),
                    );
                } else {
                    ui.label(trf!("You're on the latest version (v{v}).", v = crate::update::current()));
                }
                ui.add_space(6.0);
                if ui.button(tr!("OK")).clicked() {
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
        egui::Window::new(trf!("{icon}  Storage problem", icon = egui_phosphor::regular::WARNING))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(tr!("EVE Spai can't read or write its database, so settings and intel history won't be saved this session."));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&err).weak());
                ui.add_space(4.0);
                // A full disk fails the same way as a permissions problem, so say which it is.
                if self.disk_level == crate::disk::Level::Critical {
                    ui.label(
                        tr!("The disk holding the data folder is full. Free some space and restart; \
                         nothing here is a permission problem."),
                    );
                } else {
                    ui.label(
                        tr!("This is usually a file-permission issue on the data folder. Check that \
                         your user can write to it, or reinstall to a writable location."),
                    );
                }
                if locked {
                    ui.add_space(8.0);
                    ui.label(
                        tr!("Your saved settings could not be read, and could not be backed up either, \
                         so saving now would overwrite the only copy with defaults."),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(tr!("Keep my old settings")).clicked() {
                            self.store_warn_dismissed = true;
                        }
                        if ui.button(tr!("Start fresh")).clicked() {
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
                if ui.button(tr!("Continue anyway")).clicked() {
                    self.store_warn_dismissed = true;
                }
            });
    }

    pub(crate) fn poll_dscan_clipboard(&mut self, ctx: &egui::Context) {
        // The rescuee reads the clipboard for d-scans while its panel shows, whatever the prompt
        // setting: the scan is what the FC needs.
        let rescuee_reads = self.rescuee_panel_shown() && self.settings.rescuee_read_dscan;
        if !self.settings.dscan_autoprompt && !rescuee_reads {
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
            if rescuee_reads {
                // The panel shows it and sends it on; no prompt over the game.
                self.rescuee_take_dscan(ctx, &text);
                return;
            }
            // During a rescue, also capture the dscan breakdown onto the rescue state so the FC
            // sees what is on grid without leaving the window.
            if self.rescue_on() && self.rescue.lock().unwrap().active {
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
        let _ = crate::launch::that("https://adashboard.info/intel");
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
                .with_title(tr!("EVE Spai - D-scan"))
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
                        ui.label(tr!("Shared:"));
                        if ui.hyperlink(&link).clicked() {
                            self.dscan_link_used = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button(trf!("{icon}  Copy link", icon = icon::COPY)).clicked() {
                                ui.ctx().copy_text(link.clone());
                                self.dscan_link_used = true;
                            }
                            if ui.button(tr!("Close")).clicked() {
                                dismiss = true;
                            }
                        });
                    } else if uploading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr!("Uploading to dscan.info…"));
                        });
                    } else {
                        if let Some(e) = &error {
                            ui.colored_label(
                                crate::theme::standing::WARNING,
                                trf!("Upload failed: {e}", e = e),
                            );
                        }
                        if let Some((_, n, kind)) = &self.dscan_prompt {
                            let (n, kind) = (*n, *kind);
                            let ada = format!("{}  adashboard.info", icon::UPLOAD_SIMPLE);
                            let ada_hint =
                                tr!("Copy and open adashboard.info/intel, then paste it there (Ctrl+V)");
                            match kind {
                                PasteKind::Dscan => {
                                    ui.label(trf!("D-scan detected ({n} rows). Share with:", n = n));
                                    ui.horizontal(|ui| {
                                        if ui
                                            .button(trf!("{icon}  dscan.info", icon = icon::UPLOAD_SIMPLE))
                                            .on_hover_text(tr!("Upload to dscan.info and get a shareable link"))
                                            .clicked()
                                        {
                                            start_upload = true;
                                        }
                                        if ui.button(&ada).on_hover_text(ada_hint).clicked() {
                                            open_adashboard = true;
                                        }
                                        if ui.button(tr!("Dismiss")).clicked() {
                                            dismiss = true;
                                        }
                                    });
                                    let auto_label = if adashboard {
                                        tr!("Auto-open (also in Settings)")
                                    } else {
                                        tr!("Auto-upload (also in Settings)")
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
