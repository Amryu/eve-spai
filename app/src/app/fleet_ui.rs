//! The fleet dashboard tab: the fleet list, the start form, and a tracked fleet.

use super::*;

#[cfg(feature = "fleet")]
use crate::fleets::{
    backend::Action,
    model::{FleetRow, Perm, TagItem},
    state::{Cmd, Outcome, Page, Slot},
};

impl SpaiApp {
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_view(&mut self, ui: &mut egui::Ui) {
        self.fleet_body(ui);
    }

    #[cfg(not(feature = "fleet"))]
    pub(crate) fn fleet_view(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("This build has no fleet dashboard.").weak());
    }

    /// Hands one command to a worker. Off the UI thread even against the dry run, so the path the
    /// real backend will take is the one used every day.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_dispatch(&mut self, cmd: Cmd) {
        let (gen, backend, tx, ctx) = (
            self.fleet_gen,
            self.fleet_backend.clone(),
            self.fleet_tx.clone(),
            self.ui_ctx.clone(),
        );
        let seed = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.clone();
        std::thread::spawn(move || {
            // A malformed reply drops one command rather than the app, the way the watcher treats
            // its parser.
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::fleets::state::run(backend.as_ref(), &seed, cmd)
            }))
            .unwrap_or_else(|_| Outcome::Failed {
                what: "fleet request",
                why: "the worker panicked".to_owned(),
            });
            let _ = tx.send((gen, out));
            ctx.request_repaint();
        });
    }

    /// Takes whatever the workers finished. Never blocks: a frame that waits on a worker is a
    /// frozen app.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_collect(&mut self) {
        while let Ok(at) = self.fleet_mumble_rx.try_recv() {
            self.fleet_mumble_at = at;
        }
        let cur = self.fleet_gen;
        let mut done: Vec<Outcome> = Vec::new();
        while let Ok((gen, out)) = self.fleet_rx.try_recv() {
            if crate::fleets::state::accepts(cur, gen, &out) {
                done.push(out);
            }
        }
        if done.is_empty() {
            return;
        }
        let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
        for out in done {
            st.apply(out);
        }
    }

    /// Loads what a page shows, on first sight and whenever it changes.
    #[cfg(feature = "fleet")]
    fn fleet_refresh(&mut self, page: &Page) {
        match page {
            Page::Fleets => {
                let skip = {
                    let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                    st.active_strat.begin();
                    st.active_pct.begin();
                    st.history.begin();
                    st.history_skip
                };
                self.fleet_dispatch(Cmd::LoadActive { strategic: true });
                self.fleet_dispatch(Cmd::LoadActive { strategic: false });
                self.fleet_dispatch(Cmd::LoadHistory { skip });
            }
            Page::Tracking(id) | Page::Historic(id) => {
                self.fleet.lock().unwrap_or_else(|e| e.into_inner()).open.begin();
                self.fleet_dispatch(Cmd::Open(id.clone()));
                self.fleet_boosts_read = None;
            }
            Page::Start => {}
        }
    }

    /// Re-reads the open fleet's boost channel off disk, at most every `BOOST_REREAD`.
    ///
    /// It waits for the fleet itself because the channel is never configured: the fleet carries the
    /// id, the reference table names it and that name is the log file's own prefix. A fleet with no
    /// boost channel, or one whose log has not reached this machine, reads nothing and says so.
    #[cfg(feature = "fleet")]
    fn fleet_read_boosts(&mut self) {
        // A screenshot must never read the machine's real chat logs.
        if self.headless {
            return;
        }
        let now = std::time::Instant::now();
        if self.fleet_boosts_read.is_some_and(|t| now.duration_since(t) < BOOST_REREAD) {
            return;
        }
        let Some(dir) = crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir) else { return };
        let cmd = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let Some(open) = st.open.value.as_ref() else { return };
            let Some(channel) =
                st.seed.channel_name(&st.seed.boost_channels, open.fleet.boost_channel_id)
            else {
                return;
            };
            let Some(from) = crate::fleets::model::parse_iso(&open.fleet.started_at) else {
                return;
            };
            let to = open.fleet.closed_at.as_deref().and_then(crate::fleets::model::parse_iso);
            Cmd::ReadBoosts { dir, channel: channel.to_owned(), from, to }
        };
        self.fleet_boosts_read = Some(now);
        self.fleet_dispatch(cmd);
    }

    /// Asks Mumble where it is, off the UI thread and no more than once every `MUMBLE_POLL`.
    ///
    /// A session-bus round trip is fast until the bus is busy or Mumble is wedged, and a frame
    /// that waits on one is a frame that stutters.
    #[cfg(feature = "fleet")]
    fn fleet_poll_mumble(&mut self, ctx: &egui::Context) {
        if self.headless {
            return;
        }
        let now = std::time::Instant::now();
        if self.fleet_mumble_asked.is_some_and(|t| now.duration_since(t) < MUMBLE_POLL) {
            return;
        }
        self.fleet_mumble_asked = Some(now);
        let (tx, ctx) = (self.fleet_mumble_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(crate::mumble::current_url());
            ctx.request_repaint();
        });
    }

    /// The systems the formup field offers, resolved out of the app's own map rather than the
    /// dashboard's reference list, which carries only a handful.
    #[cfg(feature = "fleet")]
    fn fleet_places(&self) -> Places {
        let named = |name: &str| {
            self.systems
                .as_ref()
                .and_then(|g| g.lookup(name))
                .map(|i| (i.id, i.name.clone()))
        };
        let staging = named(&self.settings.rescue_staging_system);
        let recent: Vec<(i64, String)> = self
            .settings
            .fleet_recent_formup
            .iter()
            .filter_map(|n| named(n))
            .filter(|(id, _)| Some(*id) != staging.as_ref().map(|(i, _)| *i))
            .take(3)
            .collect();
        let query: String = self
            .ui_ctx
            .data(|d| d.get_temp(egui::Id::new("fleet_formup_query")).unwrap_or_default());
        let hits = match self.store.as_ref().filter(|_| query.trim().len() >= 2) {
            Some(store) => store
                .search_systems(query.trim(), 8)
                .into_iter()
                .map(|(id, name, _)| (id, name))
                .collect(),
            None => Vec::new(),
        };
        Places { staging, recent, hits }
    }

    #[cfg(feature = "fleet")]
    fn fleet_body(&mut self, ui: &mut egui::Ui) {
        self.fleet_collect();
        if !self.fleet_booted {
            self.fleet_booted = true;
            self.fleet_dispatch(Cmd::Bootstrap);
            let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
            self.fleet_refresh(&page);
        }

        // Cloned before the lock, because the render closures cannot borrow `self` while the state
        // is held. Deferred work goes into the slots below and is applied once it is dropped.
        let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
        if matches!(page, Page::Tracking(_) | Page::Historic(_)) {
            self.fleet_read_boosts();
            self.fleet_poll_mumble(ui.ctx());
        }
        let dry = self.fleet_backend.is_dry_run();
        let seed_hint = crate::fleets::seed_path_hint();
        let presets = self.settings.fleet_presets.clone();
        let boost_rules = self.settings.fleet_boost_requirements.clone();
        let fleet_hulls = self.settings.fleet_hulls.clone();
        let mut open_boost_editor = false;
        let mut sidebar_open = self.fleet_sidebar_open;
        let here = self.fleet_mumble_at.clone();
        let mine = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let me = st.session.as_ref().map(|s| s.character_name.trim().to_lowercase());
            st.open
                .value
                .as_ref()
                .and_then(|o| o.fleet.commander.as_ref())
                .zip(me)
                .is_some_and(|(c, me)| c.label.trim().to_lowercase() == me)
        };
        let mut join_comms: Option<String> = None;
        let places = self.fleet_places();
        let journal_open = self.fleet_journal_open;
        let detail_tab = self.fleet_detail_tab;
        let mut toggle_journal = false;
        let quick_open = self.fleet_quick_open;
        let mut toggle_quick = false;
        let mut set_tab: Option<DetailTab> = None;
        let mut act_on: Vec<Action> = Vec::new();
        let mut goto: Option<Page> = None;
        let mut cmd: Option<Cmd> = None;
        let mut refresh = false;
        let mut act = FormAct::default();

        egui::Panel::top("fleet_subnav").show_inside(ui, |ui| {
            ui.add_space(4.0);
            // Narrow, the status group drops below the tabs instead of landing on top of them.
            let roomy = ui.available_width() >= 900.0;
            ui.horizontal_wrapped(|ui| {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                let tab = page.tab();
                for (p, label) in [(Page::Fleets, "Fleets"), (Page::Start, "Start fleet")] {
                    if selectable_chip(ui, tab == p, label).clicked() && page != p {
                        goto = Some(p);
                    }
                }
                if selectable_chip(
                    ui,
                    quick_open,
                    format!("{}  Quick Fleet", egui_phosphor::regular::LIGHTNING),
                )
                .on_hover_text("Start from a saved preset")
                .clicked()
                {
                    toggle_quick = true;
                }
                if let Some(id) = page.fleet() {
                    ui.label(egui_phosphor::regular::CARET_RIGHT);
                    let name = st.open.value.as_ref().map(|o| o.fleet.name.clone());
                    ui.label(
                        egui::RichText::new(name.unwrap_or_else(|| id.short().to_owned())).strong(),
                    );
                }
                if !roomy {
                    ui.end_row();
                }
                let layout = if roomy {
                    egui::Layout::right_to_left(egui::Align::Center)
                } else {
                    egui::Layout::left_to_right(egui::Align::Center)
                };
                ui.with_layout(layout, |ui| {
                    if dry {
                        ui.label(
                            egui::RichText::new("DRY RUN - nothing is sent")
                                .color(crate::theme::standing::WARNING)
                                .strong(),
                        )
                        .on_hover_text(
                            "Every action records the request it would send and sends none of it.",
                        );
                    }
                    if st.seed.placeholder {
                        ui.label(egui::RichText::new("placeholder data").weak()).on_hover_text(
                            format!("No seed file, so the names are invented. Drop one at {seed_hint}."),
                        );
                    }
                    if let Some(s) = &st.session {
                        ui.label(egui::RichText::new(s.identity.name.clone()).weak())
                            .on_hover_text(format!("Command group: {}", s.identity.command_group));
                    }
                    let n = st.journal.len();
                    if selectable_chip(ui, journal_open, format!("{n} recorded"))
                        .on_hover_text("Requests this tab would have sent. Nothing was.")
                        .clicked()
                    {
                        toggle_journal = true;
                    }
                    // The lock is dropped at the end of the closure, before the deferred work runs.
                    let _ = &mut st;
                });
            });
            ui.add_space(4.0);
        });

        if journal_open {
            egui::Panel::right("fleet_journal")
                .resizable(true)
                .default_size(420.0)
                .size_range(280.0..=720.0)
                .show_inside(ui, |ui| {
                    let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                    journal_pane(ui, &st);
                });
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(e) = st.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(crate::theme::standing::HOSTILE, e);
                    if ui.small_button(egui_phosphor::regular::X).clicked() {
                        st.error = None;
                    }
                });
            }
            match &page {
                Page::Fleets => fleets_page(ui, &mut st, &mut goto, &mut cmd, &mut refresh),
                Page::Start => start_page(ui, &mut st, &presets, &places, &mut act),
                Page::Tracking(_) => {
                    tracking_page(ui, &mut st, false, detail_tab, &mut set_tab, &mut act_on, &boost_rules, &fleet_hulls, &mut open_boost_editor, &mut sidebar_open, mine, here.clone(), &mut join_comms)
                }
                Page::Historic(_) => {
                    tracking_page(ui, &mut st, true, detail_tab, &mut set_tab, &mut act_on, &boost_rules, &fleet_hulls, &mut open_boost_editor, &mut sidebar_open, mine, here.clone(), &mut join_comms)
                }
            }
        });

        if let Some(p) = goto {
            // A new page means results for the old one are no longer wanted.
            self.fleet_gen.page += 1;
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page = p.clone();
            self.fleet_refresh(&p);
        } else if refresh {
            self.fleet_refresh(&page);
        }
        if let Some(c) = cmd {
            self.fleet_dispatch(c);
        }
        if toggle_journal {
            self.fleet_journal_open = !self.fleet_journal_open;
        }
        if let Some(t) = set_tab {
            self.fleet_detail_tab = t;
        }
        for action in act_on {
            if let Some(id) = page.fleet().cloned() {
                // Anything that cannot be taken back asks first. The rest is one click.
                if let Some(question) = confirm_question(&action) {
                    let pilots = self
                        .fleet
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .open
                        .value
                        .as_ref()
                        .map(|o| o.composition.total())
                        .unwrap_or(0);
                    self.fleet_confirm = Some((id, action, question, pilots));
                } else {
                    self.fleet_dispatch(Cmd::Act(id, action));
                }
            }
        }
        if toggle_quick {
            self.fleet_quick_open = !self.fleet_quick_open;
        }
        if open_boost_editor {
            self.fleet_boost_editor = true;
        }
        self.fleet_sidebar_open = sidebar_open;
        if let Some(url) = join_comms {
            crate::mumble::open_url(&url);
        }
        self.quick_fleet_window(ui.ctx(), &presets, &mut act);
        self.fleet_confirm_modal(ui.ctx());
        self.fleet_apply_form(act);
    }

    /// Applies what the start form asked for once the state lock is gone.
    #[cfg(feature = "fleet")]
    fn fleet_apply_form(&mut self, mut act: FormAct) {
        if let Some(i) = act.load_preset {
            if let Some(p) = self.settings.fleet_presets.get(i).cloned() {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                st.apply_preset(&p);
                st.preview.begin();
            }
            self.fleet_preview_now();
        }
        if let Some(i) = act.delete_preset {
            if i < self.settings.fleet_presets.len() {
                self.settings.fleet_presets.remove(i);
                self.needs_save = true;
            }
        }
        if let Some((label, folder)) = act.save_preset {
            let preset = self
                .fleet
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .preset_from_form(&label, &folder);
            match self.settings.fleet_presets.iter_mut().find(|p| p.label == label) {
                Some(slot) => *slot = preset,
                None => self.settings.fleet_presets.push(preset),
            }
            self.needs_save = true;
        }
        if act.auto_channels {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).auto_channels();
            self.fleet_preview_now();
        }
        if act.free_channels {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).free_channels();
            self.fleet_preview_now();
        }
        if act.force_channels {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).force_configured();
            self.fleet_preview_now();
        }
        if let Some(i) = act.quick_preset {
            let presets = self.settings.fleet_presets.clone();
            if let Some(p) = presets.get(i) {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                st.apply_preset(p);
                st.page = Page::Start;
            }
            self.fleet_quick_open = false;
            self.fleet_preview_now();
            // A quick fleet skips reading the form, so whether the FC can actually track it is
            // the one thing worth knowing before the button is pressed.
            act.check_boss = true;
        }
        if let Some(name) = act.search_character.clone() {
            if name.len() >= 3 {
                self.fleet.lock().unwrap_or_else(|e| e.into_inner()).found_characters.begin();
                self.fleet_dispatch(Cmd::Search {
                    kind: crate::fleets::backend::SearchKind::Character,
                    value: name,
                });
            }
        }
        // Remembered only when it is not the staging system: the button back to staging is
        // always there, so keeping it in the recents would waste one of three slots.
        {
            let chosen = self
                .fleet
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .draft
                .formup
                .as_ref()
                .map(|l| l.label.clone());
            if let Some(name) = chosen {
                let staging = self.settings.rescue_staging_system.trim().to_owned();
                let recents = &mut self.settings.fleet_recent_formup;
                if !name.eq_ignore_ascii_case(&staging)
                    && recents.first().map(|s| s.as_str()) != Some(name.as_str())
                {
                    recents.retain(|s| !s.eq_ignore_ascii_case(&name));
                    recents.insert(0, name);
                    recents.truncate(3);
                    self.needs_save = true;
                }
            }
        }
        if act.check_boss {
            let (who, use_backup) = {
                let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                (st.fc(), st.draft.use_backup)
            };
            if let Some((character_id, _)) = who {
                self.fleet_dispatch(Cmd::CheckBoss { character_id, use_backup });
            }
        }
        if act.edited {
            // The site re-renders its preview on every change. Debounced, so typing a fleet name
            // does not spawn a worker per keystroke.
            self.fleet_preview_at = Some(std::time::Instant::now() + PREVIEW_DEBOUNCE);
        }
        if let Some(when) = self.fleet_preview_at {
            if std::time::Instant::now() >= when {
                self.fleet_preview_at = None;
                self.fleet_preview_now();
            } else {
                self.ui_ctx.request_repaint_after(PREVIEW_DEBOUNCE);
            }
        }
        if act.start {
            let req = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).start_request();
            if let Some(req) = req {
                self.fleet_dispatch(Cmd::Start(req));
            }
        }
        if act.ping {
            let req = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).ping_request();
            self.fleet_dispatch(Cmd::Ping(req));
        }
    }

    /// Asks for a fresh preview, superseding any in flight.
    #[cfg(feature = "fleet")]
    fn fleet_preview_now(&mut self) {
        self.fleet_gen.preview += 1;
        let req = {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.preview.begin();
            st.ping_request()
        };
        self.fleet_dispatch(Cmd::Preview(req));
    }

    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.heading("Fleet dashboard");
        ui.label(
            egui::RichText::new(
                "Tracking, pings and composition for fleet commanders. Off by default, and this \
                 build sends nothing: actions are recorded, not issued.",
            )
            .weak(),
        );
        changed |= ui
            .checkbox(&mut self.settings.fleet_enabled, "Enable the fleet dashboard (FC only)")
            .changed();
        if !self.settings.fleet_enabled {
            return changed;
        }
        ui.add_space(4.0);
        egui::Grid::new("fleet_settings_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Acting character");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.settings.fleet_character)
                        .desired_width(220.0),
                )
                .changed();
            ui.end_row();
        });
        ui.add_space(8.0);
        changed |= self.fleet_boost_rules(ui);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "Reference data comes from {}, and falls back to placeholder names when it is \
                 missing. Boost coverage is read from the EVE chat log of whichever boost channel \
                 the fleet uses, so there is nothing to set here.",
                crate::fleets::seed_path_hint()
            ))
            .weak(),
        );
        changed
    }

    /// Which boosts each doctrine wants, and in what order to put them on.
    ///
    /// Its own window rather than a row in settings: there are two dozen doctrines and nine
    /// charges each, which is a page of its own however it is folded into this one.
    #[cfg(feature = "fleet")]
    fn fleet_boost_rules(&mut self, ui: &mut egui::Ui) -> bool {
        let n = self.settings.fleet_boost_requirements.len();
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "{}  Doctrines...",
                    egui_phosphor::regular::SLIDERS_HORIZONTAL
                ))
                .clicked()
            {
                self.fleet_boost_editor = true;
            }
            ui.label(
                egui::RichText::new(match n {
                    0 => "nothing set".to_owned(),
                    n => format!("{n} set"),
                })
                .weak(),
            );
        });
        false
    }

    /// The editor window. Doctrines on the left, that doctrine's boosts on the right.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_boost_editor(&mut self, ctx: &egui::Context) -> bool {
        use crate::fleets::boosts::{self, Priority, CHARGES, COMBAT_BURSTS};
        if !self.fleet_boost_editor {
            return false;
        }
        let mut changed = false;
        let mut open = true;
        let mut add_custom: Option<String> = None;
        // Every ship in the game, for the hull picker's type-ahead.
        let ships: Vec<(i64, String, String)> =
            self.store.as_ref().map(|s| s.all_ships()).unwrap_or_default();
        let mut setups = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.setups.clone();
        // Hand-added doctrines sit beside the dashboard's, with negative ids so the two id spaces
        // cannot collide however the setup list changes upstream.
        for (id, name) in &self.settings.fleet_custom_doctrines {
            setups.push(crate::fleets::model::SetupItem {
                id: crate::fleets::model::SetupId(*id),
                name: name.clone(),
                minimal_opsec_level_description: None,
                priority: 0,
                is_default: false,
            });
        }
        let placeholder =
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.placeholder;
        let pick_id = egui::Id::new("fleet_boost_editor_pick");
        let mut picked: i32 = ctx.data(|d| {
            d.get_temp(pick_id).unwrap_or_else(|| setups.first().map(|s| s.id.0).unwrap_or(0))
        });

        egui::Window::new("Doctrines")
            .open(&mut open)
            .default_size([780.0, 540.0])
            .min_size([560.0, 320.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                if placeholder {
                    ui.label(
                        egui::RichText::new(
                            "These are placeholder doctrines. Drop a seed file in the profile \
                             directory to configure the real ones.",
                        )
                        .color(crate::theme::standing::WARNING),
                    );
                }
                let rules = &mut self.settings.fleet_boost_requirements;
                ui.horizontal_top(|ui| {
                    // Doctrines, with how many boosts each already has.
                    ui.vertical(|ui| {
                        ui.set_width(240.0);
                        let search_id = ui.id().with("search");
                        let mut query: String =
                            ui.data(|d| d.get_temp(search_id).unwrap_or_default());
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut query)
                                    .hint_text("Search doctrines")
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("boost_setups")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 34.0)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                for s in &setups {
                                    let name = s.name.trim();
                                    // FC Choice is the absence of a doctrine, so there is nothing
                                    // to configure for it.
                                    if !crate::fleets::doctrine::is_doctrine(s.id) {
                                        continue;
                                    }
                                    if !tag_matches(name, &query) {
                                        continue;
                                    }
                                    let n = rules.iter().filter(|r| r.setup_id == s.id.0).count();
                                    let label = if n == 0 {
                                        clip(name, 24)
                                    } else {
                                        format!("{}  ({n})", clip(name, 20))
                                    };
                                    if ui
                                        .menu_label(picked == s.id.0, label)
                                        .on_hover_text(name)
                                        .clicked()
                                    {
                                        picked = s.id.0;
                                    }
                                }
                            });
                        // A doctrine the dashboard does not list, so boosts can be set for one
                        // before it exists upstream.
                        let typed = query.trim().to_owned();
                        let known = setups.iter().any(|s| s.name.trim().eq_ignore_ascii_case(&typed));
                        if ui
                            .add_enabled(
                                !typed.is_empty() && !known,
                                egui::Button::new(format!(
                                    "{}  Add \"{}\"",
                                    egui_phosphor::regular::PLUS,
                                    clip(&typed, 14)
                                )),
                            )
                            .on_disabled_hover_text(if typed.is_empty() {
                                "Type a name above first."
                            } else {
                                "That doctrine is already listed."
                            })
                            .clicked()
                        {
                            add_custom = Some(typed);
                        }
                    });
                    ui.separator();

                    ui.vertical(|ui| {
                        // The remaining width, not the content's: otherwise picking a doctrine
                        // with a long name widened the whole window.
                        ui.set_width(ui.available_width());
                        let name = setups
                            .iter()
                            .find(|s| s.id.0 == picked)
                            .map(|s| s.name.trim().to_owned())
                            .unwrap_or_else(|| "No doctrine".to_owned());
                        let mine = rules.iter().filter(|r| r.setup_id == picked).count();
                        let tab_id = egui::Id::new("fleet_doctrine_tab");
                        let mut tab: u8 = ui.data(|d| d.get_temp(tab_id).unwrap_or(0));
                        ui.horizontal(|ui| {
                            for (i, label) in
                                [(0u8, "Boosts"), (1, "Ships"), (2, "Always allowed")]
                            {
                                if selectable_chip(ui, tab == i, label).clicked() {
                                    tab = i;
                                }
                            }
                        });
                        ui.data_mut(|d| d.insert_temp(tab_id, tab));
                        ui.add_space(4.0);
                        if tab == 1 {
                            changed |= hull_editor(
                                ui,
                                picked,
                                &name,
                                "flown by this doctrine",
                                &mut self.settings.fleet_hulls,
                                &ships,
                            );
                            return;
                        }
                        if tab == 2 {
                            changed |= always_allowed(ui, &mut self.settings.fleet_hulls, &ships);
                            return;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(clip(&name, 26)).strong(),
                            )
                            .on_hover_text(&name);
                            let armor = boosts::looks_like_armor(&name);
                            if ui
                                .add_enabled(
                                    mine == 0,
                                    egui::Button::new(format!(
                                        "{}  Fill ({})",
                                        egui_phosphor::regular::PLUS,
                                        if armor { "armor" } else { "shield" }
                                    )),
                                )
                                .on_disabled_hover_text("This doctrine already has boosts set.")
                                .on_hover_text(
                                    "Add every relevant charge at Medium, guessing the tank from \
                                     the name.",
                                )
                                .clicked()
                            {
                                rules.extend(boosts::default_rules(picked.into(), armor));
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    mine == 0,
                                    egui::Button::new(if armor { "Fill (shield)" } else { "Fill (armor)" }),
                                )
                                .on_disabled_hover_text("This doctrine already has boosts set.")
                                .clicked()
                            {
                                rules.extend(boosts::default_rules(picked.into(), !armor));
                                changed = true;
                            }
                            let sources: Vec<(i32, String)> = setups
                                .iter()
                                .filter(|s| s.id.0 != picked)
                                .filter(|s| rules.iter().any(|r| r.setup_id == s.id.0))
                                .map(|s| (s.id.0, s.name.trim().to_owned()))
                                .collect();
                            let mut copy_from: Option<i32> = None;
                            ui.add_enabled_ui(mine == 0 && !sources.is_empty(), |ui| {
                                egui::ComboBox::from_id_salt("boost_copy_from")
                                    .selected_text(format!(
                                        "{}  Copy from",
                                        egui_phosphor::regular::COPY
                                    ))
                                    .width(150.0)
                                    .show_ui(ui, |ui| {
                                        for (id, name) in &sources {
                                            let n = rules
                                                .iter()
                                                .filter(|r| r.setup_id == *id)
                                                .count();
                                            if ui
                                                .menu_label(false, format!("{name}  ({n})"))
                                                .clicked()
                                            {
                                                copy_from = Some(*id);
                                            }
                                        }
                                    });
                            })
                            .response
                            .on_disabled_hover_text(if mine > 0 {
                                "This doctrine already has boosts set."
                            } else {
                                "No other doctrine has boosts set."
                            });
                            if let Some(from) = copy_from {
                                let copied: Vec<_> = rules
                                    .iter()
                                    .filter(|r| r.setup_id == from)
                                    .map(|r| crate::settings::FleetBoostRequirement {
                                        setup_id: picked,
                                        ..r.clone()
                                    })
                                    .collect();
                                rules.extend(copied);
                                changed = true;
                            }
                            if ui
                                .add_enabled(mine > 0, egui::Button::new("Clear"))
                                .on_disabled_hover_text("Nothing to clear.")
                                .clicked()
                            {
                                rules.retain(|r| r.setup_id != picked);
                                changed = true;
                            }
                        });
                        ui.add_space(4.0);

                        let mut remove: Option<usize> = None;
                        // Shield, armor, information, skirmish, each block together, so a glance
                        // says what is covered.
                        let mut order: Vec<usize> = rules
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| r.setup_id == picked)
                            .map(|(i, _)| i)
                            .collect();
                        order.sort_by_key(|i| {
                            let r = &rules[*i];
                            (
                                boosts::burst_of(&r.charge).map(|b| b as u8).unwrap_or(u8::MAX),
                                r.charge.clone(),
                            )
                        });
                        egui::ScrollArea::vertical().id_salt("boost_rules").show(ui, |ui| {
                            egui::Grid::new("boost_rule_rows")
                                .num_columns(3)
                                .striped(true)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    for i in order {
                                        let rule = &mut rules[i];
                                        cell(ui, 220.0, |ui| {
                                            let here = boosts::burst_of(&rule.charge);
                                            let text = egui::RichText::new(rule.charge.clone())
                                                .color(match here {
                                                    Some(b) => burst_colour(ui, b),
                                                    None => ui.visuals().weak_text_color(),
                                                });
                                            egui::ComboBox::from_id_salt(("boost_charge", i))
                                                .selected_text(text)
                                                .width(210.0)
                                                .show_ui(ui, |ui| {
                                                    for (n, b) in CHARGES
                                                        .iter()
                                                        .filter(|(_, b)| COMBAT_BURSTS.contains(b))
                                                    {
                                                        changed |= ui
                                                            .menu_value(
                                                                &mut rule.charge,
                                                                (*n).to_owned(),
                                                                egui::RichText::new(*n)
                                                                    .color(burst_colour(ui, *b)),
                                                            )
                                                            .changed();
                                                    }
                                                });
                                        });
                                        cell(ui, 120.0, |ui| {
                                            let mut prio = Priority::parse(&rule.priority);
                                            egui::ComboBox::from_id_salt(("boost_prio", i))
                                                .selected_text(prio.label())
                                                .width(110.0)
                                                .show_ui(ui, |ui| {
                                                    for p in Priority::ALL {
                                                        if ui
                                                            .menu_value(
                                                                &mut prio,
                                                                p,
                                                                p.label(),
                                                            )
                                                            .changed()
                                                        {
                                                            rule.priority =
                                                                p.as_str().to_owned();
                                                            changed = true;
                                                        }
                                                    }
                                                });
                                        });
                                        if ui
                                            .button(egui_phosphor::regular::TRASH)
                                            .on_hover_text("Remove this boost")
                                            .clicked()
                                        {
                                            remove = Some(i);
                                        }
                                        ui.end_row();
                                    }
                                });
                            if mine == 0 {
                                ui.label(
                                    egui::RichText::new("No boosts set for this doctrine.").weak(),
                                );
                            }
                        });
                        if let Some(i) = remove {
                            rules.remove(i);
                            changed = true;
                        }
                        ui.add_space(4.0);
                        if ui
                            .button(format!("{}  Add a boost", egui_phosphor::regular::PLUS))
                            .clicked()
                        {
                            rules.push(crate::settings::FleetBoostRequirement {
                                setup_id: picked,
                                charge: boosts::CHARGES[0].0.to_owned(),
                                priority: Priority::Medium.as_str().to_owned(),
                            });
                            changed = true;
                        }
                    });
                });
            });

        if let Some(name) = add_custom {
            let next = self
                .settings
                .fleet_custom_doctrines
                .iter()
                .map(|(id, _)| *id)
                .min()
                .unwrap_or(0)
                - 1;
            self.settings.fleet_custom_doctrines.push((next, name));
            picked = next;
            changed = true;
        }
        ctx.data_mut(|d| d.insert_temp(pick_id, picked));
        if !open {
            self.fleet_boost_editor = false;
        }
        changed
    }

    #[cfg(not(feature = "fleet"))]
    pub(crate) fn fleet_boost_editor(&mut self, _ctx: &egui::Context) -> bool {
        false
    }

    #[cfg(not(feature = "fleet"))]
    pub(crate) fn fleet_settings_section(&mut self, _ui: &mut egui::Ui) -> bool {
        false
    }
}

/// Active fleets by kind, then the FC's own history.
#[cfg(feature = "fleet")]
fn fleets_page(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    goto: &mut Option<Page>,
    cmd: &mut Option<Cmd>,
    refresh: &mut bool,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Refresh", egui_phosphor::regular::ARROWS_CLOCKWISE)).clicked() {
            *refresh = true;
        }
        let can_start = st.can(Perm::StartFleet);
        if ui
            .add_enabled(
                can_start,
                egui::Button::new(format!(
                    "{}  Start a fleet",
                    egui_phosphor::regular::ROCKET_LAUNCH
                )),
            )
            .on_disabled_hover_text("Your account does not have the startFleet permission.")
            .clicked()
        {
            *goto = Some(Page::Start);
        }
    });
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let strat = st.active_strat.clone();
        let pct = st.active_pct.clone();
        section(ui, "Strategic", &strat, goto);
        ui.add_space(8.0);
        section(ui, "Peacetime", &pct, goto);
        ui.add_space(8.0);

        let history = st.history.clone();
        let page_size = crate::fleets::backend::calls::HISTORY_PAGE;
        let heading = match history.value.as_ref() {
            Some(p) if p.total > 0 => format!(
                "My history   {}-{} of {}",
                st.history_skip + 1,
                st.history_skip + p.items.len() as u32,
                p.total
            ),
            _ => "My history".to_owned(),
        };
        rows(ui, &heading, history.value.as_ref().map(|p| p.items.as_slice()), &history, goto);
        if let Some(p) = history.value.as_ref() {
            let more = (st.history_skip + page_size) < p.total as u32;
            if st.history_skip > 0 || more {
                ui.horizontal(|ui| {
                    if ui.add_enabled(st.history_skip > 0, egui::Button::new("Newer")).clicked() {
                        st.history_skip = st.history_skip.saturating_sub(page_size);
                        *cmd = Some(Cmd::LoadHistory { skip: st.history_skip });
                    }
                    if ui.add_enabled(more, egui::Button::new("Older")).clicked() {
                        st.history_skip += page_size;
                        *cmd = Some(Cmd::LoadHistory { skip: st.history_skip });
                    }
                });
            }
        }
    });
}

/// One list of active fleets.
#[cfg(feature = "fleet")]
fn section(ui: &mut egui::Ui, title: &str, slot: &Slot<Vec<FleetRow>>, goto: &mut Option<Page>) {
    let count = slot.value.as_ref().map(Vec::len).unwrap_or(0);
    let heading = if count > 0 { format!("{title}   {count}") } else { title.to_owned() };
    rows(ui, &heading, slot.value.as_deref(), slot, goto);
}

/// A heading and its rows, with whatever the slot has to say about how fresh they are.
#[cfg(feature = "fleet")]
fn rows<T>(
    ui: &mut egui::Ui,
    heading: &str,
    items: Option<&[FleetRow]>,
    slot: &Slot<T>,
    goto: &mut Option<Page>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(heading).strong());
        if slot.loading {
            ui.label(egui::RichText::new("loading").weak());
        }
        if slot.stale {
            ui.label(egui::RichText::new("stale").color(crate::theme::standing::WARNING))
                .on_hover_text("The last refresh failed, so this is the previous answer.");
        }
    });
    let Some(items) = items else { return };
    if items.is_empty() {
        ui.label(egui::RichText::new("Nothing here.").weak());
        return;
    }
    let now = chrono::Utc::now().timestamp();
    for row in items {
        if fleet_row(ui, row, now) {
            *goto = Some(if row.closed_at.is_some() {
                Page::Historic(row.id.clone())
            } else {
                Page::Tracking(row.id.clone())
            });
        }
    }
}

/// One fleet, as a clickable card. Returns whether it was clicked.
#[cfg(feature = "fleet")]
fn fleet_row(ui: &mut egui::Ui, row: &FleetRow, now: i64) -> bool {
    let resp = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&row.name).strong());
                if let Some(s) = &row.setup_name {
                    ui.label(egui::RichText::new(s.trim()).weak());
                }
                for t in &row.tags {
                    fleet_tag_chip(ui, t);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let age = crate::fleets::model::parse_iso(&row.started_at)
                        .map(|t| fmt_age_compact(now - t))
                        .unwrap_or_default();
                    ui.label(egui::RichText::new(age).weak());
                    if let Some(c) = row.commander.as_ref().or(row.started_by.as_ref()) {
                        ui.label(egui::RichText::new(c).weak());
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// A tag in its own colour, mapping the server's class names onto the app's palette.
#[cfg(feature = "fleet")]
fn fleet_tag_chip(ui: &mut egui::Ui, tag: &TagItem) {
    ui.label(egui::RichText::new(tag.name.trim()).color(tag_colour(ui, tag)));
}

/// A tag's colour. The two that say what kind of fleet it is are fixed, because the seed carries
/// no colour class for them and grey is the wrong answer for both.
#[cfg(feature = "fleet")]
fn tag_colour(ui: &egui::Ui, tag: &TagItem) -> egui::Color32 {
    use crate::theme::standing;
    match tag.name.trim().to_uppercase().as_str() {
        "STRATEGIC" => return standing::HOSTILE,
        "PEACETIME" => return standing::WARNING,
        _ => {}
    }
    match tag.colour_class.as_str() {
        "red" => standing::HOSTILE,
        "green" => standing::FRIENDLY,
        "yellow" => standing::WARNING,
        "blue" => ui.visuals().hyperlink_color,
        _ => ui.visuals().weak_text_color(),
    }
}

/// How long the form waits after the last edit before rendering the ping again.
#[cfg(feature = "fleet")]
const PREVIEW_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

/// How often Mumble is asked where it is. Someone moving channel mid-fleet is rare, and the
/// answer only drives a pulse.
#[cfg(feature = "fleet")]
const MUMBLE_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the boost channel is re-read. Boosters post once and then argue, so this is about
/// picking up a swap, not about latency.
#[cfg(feature = "fleet")]
const BOOST_REREAD: std::time::Duration = std::time::Duration::from_secs(20);

/// What the start form asked for, applied once the state lock is gone.
#[cfg(feature = "fleet")]
#[derive(Default)]
pub(crate) struct FormAct {
    pub load_preset: Option<usize>,
    pub delete_preset: Option<usize>,
    /// (label, folder) for the preset to keep. An empty folder is the top level.
    pub save_preset: Option<(String, String)>,
    pub auto_channels: bool,
    pub free_channels: bool,
    pub force_channels: bool,
    /// Ask the dashboard whether the chosen FC is boss of a fleet in game.
    pub check_boss: bool,
    /// A preset the Quick Fleet picker chose, which loads the form and goes to it.
    pub quick_preset: Option<usize>,
    /// A character name to look up for the snowflake row.
    pub search_character: Option<String>,
    /// A solar system name to look up for the formup field.
    pub search_system: Option<String>,
    pub edited: bool,
    pub start: bool,
    pub ping: bool,
}

/// The start form on the left, the ping it would send on the right.
#[cfg(feature = "fleet")]
fn start_page(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    presets: &[crate::settings::FleetPreset],
    places: &Places,
    act: &mut FormAct,
) {
    let can_start = st.can(Perm::StartFleet);
    egui::Panel::right("fleet_preview")
        .resizable(true)
        .default_size(360.0)
        .size_range(260.0..=560.0)
        .show_inside(ui, |ui| preview_pane(ui, st));

    // The two buttons that do something live in their own strip rather than at the end of the
    // form: a form long enough to scroll would otherwise hide the thing it is for.
    egui::Panel::bottom("fleet_start_actions").frame(bar_frame(ui)).show_inside(ui, |ui| {
        // Below this the two groups would sit on top of each other, so they stack instead.
        let roomy = ui.available_width() >= 560.0;
        ui.horizontal_wrapped(|ui| {
            save_preset_button(ui, &preset_folders(presets), act);
            // Right-aligned when there is room. Stacked below when there is not, where a
            // right-to-left run would come out back to front.
            if !roomy {
                ui.end_row();
            }
            let layout = if roomy {
                egui::Layout::right_to_left(egui::Align::Center)
            } else {
                egui::Layout::left_to_right(egui::Align::Center)
            };
            ui.with_layout(layout, |ui| {
                let note = |ui: &mut egui::Ui| {
                    ui.label(
                        egui::RichText::new("recorded, not sent")
                            .color(crate::theme::standing::WARNING),
                    );
                };
                let ping = |ui: &mut egui::Ui, act: &mut FormAct| {
                    if ui
                        .add_enabled(
                            can_start,
                            egui::Button::new(format!(
                                "{}  Request ping",
                                egui_phosphor::regular::PAPER_PLANE_TILT
                            )),
                        )
                        .on_disabled_hover_text(
                            "Your account does not have the startFleet permission.",
                        )
                        .clicked()
                    {
                        act.ping = true;
                    }
                };
                let ready = st.start_request().is_some();
                let track = |ui: &mut egui::Ui, act: &mut FormAct| {
                    let resp = ui
                        .add_enabled(
                            can_start && ready,
                            egui::Button::new(format!(
                                "{}  Track fleet",
                                egui_phosphor::regular::ROCKET_LAUNCH
                            )),
                        )
                        .on_hover_text(
                            "Records the request this would send. Nothing leaves the app.",
                        );
                    let resp = if !can_start {
                        resp.on_disabled_hover_text(
                            "Your account does not have the startFleet permission.",
                        )
                    } else {
                        resp.on_disabled_hover_text("Give the fleet a name first.")
                    };
                    if resp.clicked() {
                        act.start = true;
                    }
                };
                if roomy {
                    note(ui);
                    ping(ui, act);
                    track(ui, act);
                } else {
                    track(ui, act);
                    ping(ui, act);
                    note(ui);
                }
            });
        });
    });

    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            preset_bar(ui, presets, act);
            ui.separator();
            form_grid(ui, st, places, act);
            ui.add_space(6.0);
            tag_pickers(ui, st, act);
            ui.add_space(6.0);
            snowflake_rows(ui, st, act);
            ui.add_space(8.0);
        });
    });
}

/// Saved presets, one chip each, plus saving the form as one.
#[cfg(feature = "fleet")]
fn preset_bar(
    ui: &mut egui::Ui,
    presets: &[crate::settings::FleetPreset],
    act: &mut FormAct,
) {
    ui.add_space(4.0);
    // Top level first, then one row per folder. One level only, so a row is the whole depth.
    let mut groups: Vec<(String, Vec<usize>)> = vec![(String::new(), Vec::new())];
    for f in preset_folders(presets) {
        groups.push((f, Vec::new()));
    }
    for (i, p) in presets.iter().enumerate() {
        let f = p.folder.trim();
        if let Some(g) = groups.iter_mut().find(|(name, _)| name == f) {
            g.1.push(i);
        }
    }
    if presets.is_empty() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Presets").strong());
            ui.label(egui::RichText::new("none saved").weak());
        });
    }
    for (folder, idx) in groups.iter().filter(|(_, idx)| !idx.is_empty()) {
        ui.horizontal_wrapped(|ui| {
            let text = if folder.is_empty() { "Presets" } else { folder.as_str() };
            let text = egui::RichText::new(text).strong();
            // The fixed gutter lines the folder rows up, but only where there is room for it:
            // squeezed narrow it would push the last chip past the edge instead of wrapping.
            if ui.available_width() >= 420.0 {
                cell(ui, 96.0, |ui| {
                    ui.label(text);
                });
            } else {
                ui.label(text);
            }
            for &i in idx {
                let p = &presets[i];
                if ui.button(&p.label).on_hover_text("Fill the form from this preset").clicked() {
                    act.load_preset = Some(i);
                }
                if ui
                    .small_button(egui_phosphor::regular::TRASH)
                    .on_hover_text(format!("Forget the {} preset", p.label))
                    .clicked()
                {
                    act.delete_preset = Some(i);
                }
            }
        });
    }
}

/// An action strip sits against the edge of the view, so it keeps the panel's side margins and
/// trims the vertical ones to the gap the buttons already carry.
#[cfg(feature = "fleet")]
fn bar_frame(ui: &egui::Ui) -> egui::Frame {
    let f = egui::Frame::side_top_panel(ui.style());
    // More above than below: the panel draws its separator on the top edge, and flush against the
    // buttons it reads as an underline on the form rather than the top of a strip.
    egui::Frame { inner_margin: egui::Margin { top: 9, bottom: 5, ..f.inner_margin }, ..f }
}

/// Naming the current form and keeping it. Lives in the action strip so the form can scroll past
/// it without taking the controls along.
#[cfg(feature = "fleet")]
fn save_preset_button(ui: &mut egui::Ui, folders: &[String], act: &mut FormAct) {
    let id = ui.id().with("preset_name");
    let mut label: String = ui.data(|d| d.get_temp(id).unwrap_or_default());
    let folder_id = id.with("folder");
    let mut folder: String = ui.data(|d| d.get_temp(folder_id).unwrap_or_default());
    let open_id = id.with("open");
    let mut open: bool = ui.data(|d| d.get_temp(open_id).unwrap_or(false));
    if ui
        .add(egui::Button::new(format!(
            "{}  Save as preset",
            egui_phosphor::regular::BOOKMARK_SIMPLE
        )))
        .on_hover_text("Keep this form as a preset for next time.")
        .clicked()
    {
        open = !open;
    }
    if open {
        ui.add(
            egui::TextEdit::singleline(&mut label).hint_text("Preset name").desired_width(150.0),
        );
        // Typed, not picked: a new folder is made by naming it, and the existing ones are one
        // click away in the dropdown beside it.
        ui.add(
            egui::TextEdit::singleline(&mut folder)
                .hint_text("Folder (optional)")
                .desired_width(130.0),
        );
        let combo = egui::ComboBox::from_id_salt("preset_folder_pick").width(0.0);
        combo.show_ui(ui, |ui| {
            if ui.menu_label(folder.is_empty(), "Top level").clicked() {
                folder.clear();
            }
            for f in folders {
                if ui.menu_label(&folder == f, f).clicked() {
                    folder = f.clone();
                }
            }
        });
        let ready = !label.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Save"))
            .on_disabled_hover_text("Name the preset first.")
            .clicked()
        {
            act.save_preset = Some((label.trim().to_owned(), folder.trim().to_owned()));
            label.clear();
            open = false;
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(id, label);
        d.insert_temp(folder_id, folder);
        d.insert_temp(open_id, open);
    });
}

/// The folders presets are kept in, in order, without repeats.
#[cfg(feature = "fleet")]
fn preset_folders(presets: &[crate::settings::FleetPreset]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in presets {
        let f = p.folder.trim();
        if !f.is_empty() && !out.iter().any(|x| x == f) {
            out.push(f.to_owned());
        }
    }
    out.sort();
    out
}

/// Width every control in the form shares, so the column reads as one edge rather than a ragged
/// one.
#[cfg(feature = "fleet")]
const FIELD_W: f32 = 260.0;
/// Label gutter beside it.
#[cfg(feature = "fleet")]
const LABEL_W: f32 = 110.0;
/// What one labelled field costs across, grid spacing included.
#[cfg(feature = "fleet")]
const FORM_COL_W: f32 = LABEL_W + 8.0 + FIELD_W;

/// The fields themselves, in two columns when there is room for two.
///
/// The split is what the fleet is on the left and how it runs on the right, so a narrow window
/// stacking them still reads in a sensible order.
#[cfg(feature = "fleet")]
fn form_grid(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    places: &Places,
    act: &mut FormAct,
) {
    if ui.available_width() >= 2.0 * FORM_COL_W + 24.0 {
        ui.columns(2, |cols| {
            form_identity(&mut cols[0], st, places, act);
            form_running(&mut cols[1], st, act);
        });
    } else {
        form_identity(ui, st, places, act);
        ui.add_space(6.0);
        form_running(ui, st, act);
    }
}

/// The systems the formup field offers: the app's staging, the last few chosen instead of it, and
/// whatever the typed query turned up.
#[cfg(feature = "fleet")]
#[derive(Clone, Debug, Default)]
pub(crate) struct Places {
    pub staging: Option<(i64, String)>,
    pub recent: Vec<(i64, String)>,
    pub hits: Vec<(i64, String)>,
}

/// What the fleet is: its name, what it flies, who it is for, where it forms.
#[cfg(feature = "fleet")]
fn form_identity(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    places: &Places,
    act: &mut FormAct,
) {
    let seed = st.seed.clone();
    egui::Grid::new("fleet_form_identity")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("FC");
            ui.vertical(|ui| {
                let signed_in = st.session.as_ref().map(|s| (s.character_id, s.character_name.clone()));
                let current = st
                    .fc()
                    .map(|(_, n)| n)
                    .or_else(|| signed_in.as_ref().map(|(_, n)| n.clone()))
                    .unwrap_or_else(|| "none".to_owned());
                let mut pick = st.draft.fc_character;
                egui::ComboBox::from_id_salt("fleet_fc")
                    .width(FIELD_W)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if let Some((id, name)) = &signed_in {
                            ui.menu_value(&mut pick, None, format!("{name}  (signed in)"));
                            let _ = id;
                        }
                        for c in seed.characters.iter().filter(|c| !c.is_hidden) {
                            ui.menu_value(&mut pick, Some(c.id), &c.name);
                        }
                    });
                if pick != st.draft.fc_character {
                    st.draft.fc_character = pick;
                    act.check_boss = true;
                    act.edited = true;
                }
                boss_line(ui, st, act);
            });
            ui.end_row();

            ui.label("Name");
            let d = &mut st.draft;
            act.edited |= ui
                .add(egui::TextEdit::singleline(&mut d.form.name).desired_width(FIELD_W))
                .changed();
            ui.end_row();

            ui.label("Description");
            act.edited |= ui
                .add(
                    egui::TextEdit::multiline(&mut d.form.description)
                        .desired_rows(2)
                        .desired_width(FIELD_W),
                )
                .changed();
            ui.end_row();

            ui.label("Setup");
            let current = seed
                .setups
                .iter()
                .find(|s| s.id.0 == d.form.setup_id)
                .map(|s| s.name.trim().to_owned())
                .unwrap_or_else(|| "== Choose a setup ==".to_owned());
            egui::ComboBox::from_id_salt("fleet_setup")
                .width(FIELD_W)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    act.edited |= ui
                        .menu_value(&mut d.form.setup_id, 0, "== Choose a setup ==")
                        .changed();
                    for s in &seed.setups {
                        act.edited |= ui
                            .menu_value(&mut d.form.setup_id, s.id.0, s.name.trim())
                            .changed();
                    }
                });
            ui.end_row();

            ui.label("SIG");
            let sig = d
                .form
                .group_id
                .and_then(|g| seed.sigs.iter().find(|s| s.id == g.0 as i64))
                .map(|s| s.label.clone())
                .unwrap_or_else(|| "== None ==".to_owned());
            egui::ComboBox::from_id_salt("fleet_sig").width(FIELD_W).selected_text(sig).show_ui(
                ui,
                |ui| {
                    act.edited |=
                        ui.menu_value(&mut d.form.group_id, None, "== None ==").changed();
                    for s in &seed.sigs {
                        let id = Some(crate::fleets::model::GroupId(s.id as i32));
                        act.edited |=
                            ui.menu_value(&mut d.form.group_id, id, &s.label).changed();
                    }
                },
            );
            let _ = d;
            ui.end_row();

            ui.label("Formup");
            act.edited |= formup_field(ui, &mut st.draft, places, act);
            ui.end_row();

            ui.label("Doctrine notes");
            let d = &mut st.draft;
            let mut notes = d.form.doctrine_notes.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut notes)
                        .hint_text("Optional, shown in the ping")
                        .desired_width(FIELD_W),
                )
                .changed()
            {
                d.form.doctrine_notes = Some(notes).filter(|s| !s.trim().is_empty());
                act.edited = true;
            }
            ui.end_row();
        });
}

/// How it runs: comms, when it closes itself, and the switches.
#[cfg(feature = "fleet")]
fn form_running(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    let seed = st.seed.clone();
    let auto = st.draft.auto;
    let d = &mut st.draft;
    egui::Grid::new("fleet_form_running")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            // A hand-picked channel is what "Force configured" puts back, so the choice is recorded
            // as configured and stops reading as a switch.
            if channel_row(ui, "Mumble", "fleet_mumble", &seed.mumble_channels,
                           &mut d.form.mumble_channel_id, auto.mumble) {
                d.configured.mumble = d.form.mumble_channel_id;
                d.auto.mumble = false;
                act.edited = true;
            }
            if channel_row(ui, "Logi", "fleet_logi", &seed.logi_channels,
                           &mut d.form.logi_channel_id, auto.logi) {
                d.configured.logi = d.form.logi_channel_id;
                d.auto.logi = false;
                act.edited = true;
            }
            if channel_row(ui, "Boost", "fleet_boost", &seed.boost_channels,
                           &mut d.form.boost_channel_id, auto.boost) {
                d.configured.boost = d.form.boost_channel_id;
                d.auto.boost = false;
                act.edited = true;
            }

            ui.label("Auto close");
            ui.horizontal(|ui| {
                let mut kind = d.form.auto_close_type.unwrap_or(1);
                act.edited |= selectable_chip(ui, kind == 0, "Start").clicked().then(|| {
                    kind = 0;
                }).is_some();
                act.edited |= selectable_chip(ui, kind == 1, "FC left").clicked().then(|| {
                    kind = 1;
                }).is_some();
                d.form.auto_close_type = Some(kind);
                let mut mins = d.form.auto_close_time.unwrap_or(30);
                act.edited |=
                    ui.add(egui::DragValue::new(&mut mins).range(1..=600).suffix(" min")).changed();
                d.form.auto_close_time = Some(mins);
            });
            ui.end_row();

            ui.label("Options");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                act.edited |= ui.checkbox(&mut d.form.set_motd, "Set the fleet MOTD").changed();
                act.edited |=
                    ui.checkbox(&mut d.form.is_corporation_fleet, "Corporation fleet").changed();
                act.edited |= ui
                    .checkbox(&mut d.use_backup, "Use the backup key")
                    .on_hover_text("Tracks through the backup ESI key rather than this character.")
                    .changed();
                act.edited |= ui
                    .checkbox(
                        &mut d.form.ignore_participation_requirements,
                        "Ignore participation requirements",
                    )
                    .changed();
            });
            ui.end_row();
        });

    // Outside the grid: three buttons wrap freely at a narrow width, where a grid cell would
    // overlap the row beneath it.
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!("{}  Auto", egui_phosphor::regular::MAGIC_WAND))
            .on_hover_text("Keep the channels already chosen and replace only the ones that are taken.")
            .clicked()
        {
            act.auto_channels = true;
        }
        if ui
            .button(format!("{}  Pick free comms", egui_phosphor::regular::ARROWS_CLOCKWISE))
            .on_hover_text("Start again from the lowest free channel for all three.")
            .clicked()
        {
            act.free_channels = true;
        }
        let configured = st.draft.configured != crate::fleets::state::Configured::default();
        if ui
            .add_enabled(
                configured,
                egui::Button::new(format!("{}  Force configured", egui_phosphor::regular::LOCK)),
            )
            .on_disabled_hover_text("Nothing was configured to go back to.")
            .on_hover_text("Put back the channels this preset asked for, taken or not.")
            .clicked()
        {
            act.force_channels = true;
        }
    });
}

/// Whether the character the fleet would be started as is actually boss of a fleet.
///
/// Only the account's own characters can be checked, which is the same set the picker offers, so
/// there is nothing here to guard against.
#[cfg(feature = "fleet")]
fn boss_line(ui: &mut egui::Ui, st: &crate::fleets::FleetState, act: &mut FormAct) {
    let Some((id, _)) = st.fc() else { return };
    ui.horizontal(|ui| {
        match st.boss.as_ref().filter(|(who, _)| *who == id) {
            Some((_, check)) => {
                let (ok, why) = check.verdict();
                let (glyph, colour) = if ok {
                    (egui_phosphor::regular::CHECK_CIRCLE, crate::theme::standing::FRIENDLY)
                } else {
                    (egui_phosphor::regular::WARNING, crate::theme::standing::WARNING)
                };
                ui.label(egui::RichText::new(glyph).color(colour));
                ui.label(egui::RichText::new(why).color(colour));
            }
            None => {
                ui.label(egui::RichText::new("Fleet boss not checked").weak());
            }
        }
        if ui
            .small_button(egui_phosphor::regular::ARROWS_CLOCKWISE)
            .on_hover_text("Ask again whether this character is boss of a fleet")
            .clicked()
        {
            act.check_boss = true;
        }
    });
}

/// Where the fleet forms up: any solar system, with the ones worth one click in front.
///
/// The field is a search box rather than a list: the dashboard takes a system id, and there are
/// five thousand of them.
#[cfg(feature = "fleet")]
fn formup_field(
    ui: &mut egui::Ui,
    draft: &mut crate::fleets::state::Draft,
    places: &Places,
    act: &mut FormAct,
) -> bool {
    use crate::fleets::model::Labelled;
    let mut changed = false;
    let query_id = egui::Id::new("fleet_formup_query");
    let mut query: String = ui.data(|d| d.get_temp(query_id).unwrap_or_default());
    let at_staging = draft
        .formup
        .as_ref()
        .zip(places.staging.as_ref())
        .is_some_and(|(l, (id, _))| l.id == *id);

    ui.horizontal(|ui| {
        // The chosen system reads as a badge rather than as text in the box, which is a search
        // box and has to stay typable.
        if let Some(l) = draft.formup.clone() {
            if ui
                .add(
                    egui::Button::new(format!("{}  {}", l.label, egui_phosphor::regular::X))
                        .fill(ui.visuals().selection.bg_fill)
                        .stroke(egui::Stroke::new(1.0, ui.visuals().selection.stroke.color)),
                )
                .on_hover_text("Clear the formup location")
                .clicked()
            {
                draft.formup = None;
                changed = true;
            }
        }
        let width = (ui.available_width() - 40.0).clamp(80.0, FIELD_W);
        let field = ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text("Search systems")
                .desired_width(width),
        );
        if field.changed() {
            act.search_system = Some(query.trim().to_owned());
        }
        if let Some((id, name)) = places.staging.clone() {
            if ui
                .add_enabled(
                    !at_staging,
                    egui::Button::new(egui_phosphor::regular::HOUSE).frame(false),
                )
                .on_hover_text(format!("Form up at {name}, the staging set in settings"))
                .on_disabled_hover_text("Already forming up at staging.")
                .clicked()
            {
                draft.formup = Some(Labelled { id, label: name });
                changed = true;
            }
        }

        // Nothing typed yet: the staging system and the last few chosen instead of it.
        let offered: Vec<(i64, String)> = if query.trim().is_empty() {
            places.staging.iter().cloned().chain(places.recent.iter().cloned()).collect()
        } else {
            places.hits.clone()
        };
        let hov_id = egui::Id::new("fleet_formup_hover");
        let was_over: bool = ui.data(|d| d.get_temp(hov_id).unwrap_or(false));
        // Kept open while the pointer is over it, or clicking an item would take focus off the
        // field and close the popup before the click landed.
        let popup = egui::Popup::from_response(&field)
            .open(!offered.is_empty() && (field.has_focus() || was_over))
            .width(FIELD_W)
            .show(|ui| {
                for (id, name) in &offered {
                    let on = draft.formup.as_ref().is_some_and(|l| l.id == *id);
                    if ui.menu_label(on, name.as_str()).clicked() {
                        draft.formup = Some(Labelled { id: *id, label: name.clone() });
                        changed = true;
                        query.clear();
                    }
                }
            });
        let over = popup.as_ref().is_some_and(|r| r.response.contains_pointer());
        ui.data_mut(|d| d.insert_temp(hov_id, over));
    });
    ui.data_mut(|d| d.insert_temp(query_id, query));
    changed
}

/// One comms combo, marking what is taken and what was picked for you.
#[cfg(feature = "fleet")]
fn channel_row(
    ui: &mut egui::Ui,
    label: &str,
    salt: &str,
    list: &[crate::fleets::model::ChannelItem],
    slot: &mut Option<crate::fleets::model::ChannelId>,
    auto: bool,
) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let current = slot
            .and_then(|id| list.iter().find(|c| c.id == id))
            .map(|c| c.name.trim().to_owned())
            .unwrap_or_else(|| "none".to_owned());
        egui::ComboBox::from_id_salt(salt).width(180.0).selected_text(current).show_ui(ui, |ui| {
            changed |= ui.menu_value(slot, None, "none").changed();
            for c in list {
                let text = if c.is_in_use {
                    egui::RichText::new(format!("{}  in use", c.name.trim())).weak()
                } else {
                    egui::RichText::new(c.name.trim().to_owned())
                };
                changed |= ui.menu_value(slot, Some(c.id), text).changed();
            }
        });
        if auto {
            ui.label(
                egui::RichText::new("switched").color(crate::theme::standing::WARNING),
            )
            .on_hover_text("The channel asked for was taken, so this free one was picked.");
        }
        if slot.is_some_and(|id| list.iter().any(|c| c.id == id && c.is_in_use)) {
            ui.label(
                egui::RichText::new("in use").color(crate::theme::standing::WARNING),
            )
            .on_hover_text("Another fleet has this channel.");
        }
    });
    ui.end_row();
    changed
}

/// Primary and secondary tags, as two rows of chips.
#[cfg(feature = "fleet")]
fn tag_pickers(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    let tags = st.seed.tags.clone();
    egui::Grid::new("fleet_form_tags")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            for (title, salt, primary) in
                [("Primary tag", "fleet_tag_primary", true), ("Secondary tags", "fleet_tag_secondary", false)]
            {
                ui.label(title);
                let mut pool: Vec<_> =
                    tags.iter().filter(|t| t.is_primary == primary).cloned().collect();
                ui.vertical(|ui| {
                    if primary {
                        // The two that decide what kind of fleet this is come first, in that
                        // order, with their own pair of buttons above the field. Above rather
                        // than beside, or a narrow window pushes the field off the edge.
                        pool.sort_by_key(|t| (headline_rank(&t.name), t.id.0));
                        act.edited |= headline_tags(ui, &pool, &mut st.draft.tags);
                    }
                    ui.horizontal(|ui| {
                        act.edited |= tag_field(ui, salt, &pool, &mut st.draft.tags, primary);
                    });
                });
                ui.end_row();
            }
        });
}

/// The two primary tags every fleet is one of, in the order they belong in.
#[cfg(feature = "fleet")]
const HEADLINE_TAGS: [&str; 2] = ["PEACETIME", "STRATEGIC"];

/// Sort key that floats the headline tags to the top of the primary list.
#[cfg(feature = "fleet")]
fn headline_rank(name: &str) -> usize {
    HEADLINE_TAGS.iter().position(|h| h.eq_ignore_ascii_case(name.trim())).unwrap_or(HEADLINE_TAGS.len())
}

/// Peacetime or strategic, one click apart, since it is the first thing an FC sets and the thing
/// most often set wrong.
#[cfg(feature = "fleet")]
fn headline_tags(
    ui: &mut egui::Ui,
    pool: &[TagItem],
    selected: &mut std::collections::BTreeSet<crate::fleets::model::TagId>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for name in HEADLINE_TAGS {
            let Some(t) = pool.iter().find(|t| t.name.trim().eq_ignore_ascii_case(name)) else {
                continue;
            };
            let on = selected.contains(&t.id);
            let text = egui::RichText::new(t.name.trim()).color(tag_colour(ui, t)).strong();
            if selectable_chip(ui, on, text).clicked() && !on {
                // One primary tag, so picking one drops whatever else was there.
                for other in pool {
                    selected.remove(&other.id);
                }
                selected.insert(t.id);
                changed = true;
            }
        }
    });
    changed
}

/// Whether a tag survives what has been typed into the search box.
///
/// Every word has to appear somewhere in the name, so "struct bash" finds "Structure Bash" without
/// the words being adjacent or in that order.
#[cfg(feature = "fleet")]
fn tag_matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    query.split_whitespace().all(|w| name.contains(&w.to_lowercase()))
}

/// A field of tag badges that opens a searchable list.
///
/// `single` is the primary row, where picking one replaces whatever was there: the dashboard takes
/// exactly one, so the field enforces it rather than letting the request be refused later.
#[cfg(feature = "fleet")]
fn tag_field(
    ui: &mut egui::Ui,
    salt: &str,
    pool: &[TagItem],
    selected: &mut std::collections::BTreeSet<crate::fleets::model::TagId>,
    single: bool,
) -> bool {
    let mut changed = false;
    let mut remove: Option<crate::fleets::model::TagId> = None;

    // The badges sit inside a frame the size of the other controls, so the row reads as a field
    // rather than as a loose run of chips.
    let frame = egui::Frame::new()
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .corner_radius(ui.visuals().widgets.inactive.corner_radius)
        .inner_margin(egui::Margin::symmetric(6, 4));
    let inner = frame.show(ui, |ui| {
        ui.set_width(FIELD_W - 12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let chosen: Vec<&TagItem> = pool.iter().filter(|t| selected.contains(&t.id)).collect();
            if chosen.is_empty() {
                ui.label(
                    egui::RichText::new(if single { "none" } else { "none selected" }).weak(),
                );
            }
            for t in chosen {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "{}  {}",
                                t.name.trim(),
                                egui_phosphor::regular::X
                            ))
                            .color(tag_colour(ui, t)),
                        )
                        .fill(ui.visuals().selection.bg_fill)
                        .stroke(egui::Stroke::new(1.0, ui.visuals().selection.stroke.color)),
                    )
                    .on_hover_text("Remove this tag")
                    .clicked()
                {
                    remove = Some(t.id);
                }
            }
        });
    });

    let open_button = ui
        .button(egui_phosphor::regular::CARET_DOWN)
        .on_hover_text(if single { "Choose the primary tag" } else { "Choose secondary tags" });
    let _ = inner;

    // As wide as the window allows: the tags are short, so a wide popup fits three or four to a
    // row instead of one, and the whole list is visible without scrolling at all.
    let screen = ui.ctx().content_rect();
    let width = (screen.width() - 80.0).clamp(280.0, 720.0);
    let height = (screen.height() * 0.5).clamp(180.0, 420.0);

    egui::Popup::from_toggle_button_response(&open_button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(width)
        .show(|ui| {
            ui.set_min_width(width);
            let search_id = ui.id().with((salt, "search"));
            let mut query: String = ui.data(|d| d.get_temp(search_id).unwrap_or_default());
            let edit = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Search tags")
                    .desired_width(f32::INFINITY),
            );
            if edit.changed() {
                ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    let mut any = false;
                    ui.horizontal_wrapped(|ui| {
                        for t in pool {
                            let name = t.name.trim();
                            if !tag_matches(name, &query) {
                                continue;
                            }
                            any = true;
                            let on = selected.contains(&t.id);
                            let text = egui::RichText::new(name).color(tag_colour(ui, t));
                            if selectable_chip(ui, on, text).clicked() {
                                if on {
                                    selected.remove(&t.id);
                                } else {
                                    if single {
                                        // Exactly one primary, so the others in this pool go.
                                        for other in pool {
                                            selected.remove(&other.id);
                                        }
                                    }
                                    selected.insert(t.id);
                                }
                                changed = true;
                            }
                        }
                    });
                    if !any {
                        ui.label(egui::RichText::new("Nothing matches.").weak());
                    }
                });
        });

    if let Some(id) = remove {
        selected.remove(&id);
        changed = true;
    }
    changed
}

/// The pilots called out in the ping.
#[cfg(feature = "fleet")]
fn snowflake_rows(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    use crate::fleets::model::{Snowflake, SnowflakeType};
    let can = st.can(Perm::ManageFleetSnowflakes);
    let hits = st.found_characters.clone();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Snowflakes").strong());
        ui.label(egui::RichText::new("who gets named in the ping").weak());
    });

    let mut drop: Option<usize> = None;
    if st.draft.snowflakes.is_empty() {
        ui.label(egui::RichText::new("None.").weak());
    }
    for (i, s) in st.draft.snowflakes.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            cell(ui, 110.0, |ui| {
                egui::ComboBox::from_id_salt(("snowflake", i))
                    .width(100.0)
                    .selected_text(s.kind.label())
                    .show_ui(ui, |ui| {
                        for k in SnowflakeType::ALL {
                            act.edited |= ui.menu_value(&mut s.kind, k, k.label()).changed();
                        }
                    });
            });
            cell(ui, 180.0, |ui| {
                ui.label(&s.character_name);
            });
            if ui
                .add_enabled(can, egui::Button::new(egui_phosphor::regular::TRASH).frame(false))
                .on_disabled_hover_text(
                    "Your account does not have the manageFleetSnowflakes permission.",
                )
                .on_hover_text(format!("Drop {}", s.character_name))
                .clicked()
            {
                drop = Some(i);
            }
        });
    }
    if let Some(i) = drop {
        st.draft.snowflakes.remove(i);
        act.edited = true;
    }

    // Typed, searched, then added: a snowflake names a real character in the ping, so a name
    // nobody has is worse than none at all.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let name_id = egui::Id::new("fleet_snowflake_name");
        let mut name: String = ui.data(|d| d.get_temp(name_id).unwrap_or_default());
        let kind_id = egui::Id::new("fleet_snowflake_kind");
        let mut kind: SnowflakeType = ui.data(|d| d.get_temp(kind_id).unwrap_or_default());

        cell(ui, 110.0, |ui| {
            egui::ComboBox::from_id_salt("snowflake_new_kind")
                .width(100.0)
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for k in SnowflakeType::ALL {
                        ui.menu_value(&mut kind, k, k.label());
                    }
                });
        });
        let edit = ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Character name")
                .desired_width(180.0),
        );
        if edit.changed() {
            act.search_character = Some(name.trim().to_owned());
        }
        if hits.loading {
            ui.label(egui::RichText::new("looking").weak());
        }
        ui.data_mut(|d| {
            d.insert_temp(name_id, name.clone());
            d.insert_temp(kind_id, kind);
        });

        // Exactly one match on the typed name is the only case that can be added without a pick.
        let exact = hits.value.as_ref().and_then(|v| {
            v.iter().find(|l| l.label.trim().eq_ignore_ascii_case(name.trim()))
        });
        let already = |id: i64| st.draft.snowflakes.iter().any(|s| s.character_id == id);
        let ready = can && exact.is_some_and(|l| !already(l.id));
        let add = ui
            .add_enabled(ready, egui::Button::new(format!("{}  Add", egui_phosphor::regular::PLUS)))
            .on_disabled_hover_text(if !can {
                "Your account does not have the manageFleetSnowflakes permission."
            } else if name.trim().is_empty() {
                "Type a character name."
            } else if exact.is_none() {
                "No character by that name."
            } else {
                "Already a snowflake."
            });
        if add.clicked() {
            if let Some(l) = exact {
                st.draft.snowflakes.push(Snowflake {
                    id: 0,
                    character_id: l.id,
                    character_name: l.label.clone(),
                    kind,
                });
                act.edited = true;
                ui.data_mut(|d| d.insert_temp(name_id, String::new()));
            }
        }
    });

    // Anything the search turned up that is not the exact name, one click away.
    if let Some(v) = hits.value.as_ref().filter(|v| v.len() > 1) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Did you mean").weak());
            for l in v.iter().take(6) {
                if selectable_chip(ui, false, l.label.trim()).clicked() {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("fleet_snowflake_name"), l.label.clone())
                    });
                }
            }
        });
    }
}

/// The ping and MOTD the dashboard would render, refreshed as the form changes.
#[cfg(feature = "fleet")]
fn preview_pane(ui: &mut egui::Ui, st: &crate::fleets::FleetState) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Ping preview").strong());
        if st.preview.loading {
            ui.label(egui::RichText::new("rendering").weak());
        }
    });
    ui.separator();
    let Some(p) = st.preview.value.as_ref() else {
        ui.label(egui::RichText::new("Fill the form to see the ping.").weak());
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (title, text) in [("Ping", &p.ping), ("MOTD", &p.motd)] {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong());
                if ui.small_button(egui_phosphor::regular::COPY).on_hover_text("Copy").clicked() {
                    ui.ctx().copy_text(text.clone());
                }
            });
            ui.add(
                egui::Label::new(egui::RichText::new(text.clone()).monospace()).wrap(),
            );
            ui.add_space(6.0);
        }
    });
}

/// What this tab would have sent, newest first.
#[cfg(feature = "fleet")]
fn journal_pane(ui: &mut egui::Ui, st: &crate::fleets::FleetState) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Recorded requests").strong());
    ui.label(
        egui::RichText::new("Built and kept, never sent. This is what the real client would post.")
            .weak(),
    );
    ui.separator();
    if st.journal.is_empty() {
        ui.label(egui::RichText::new("Nothing yet.").weak());
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, rec) in st.journal.iter().enumerate().rev() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(rec.line()).monospace());
                if ui.small_button(egui_phosphor::regular::COPY).on_hover_text("Copy").clicked() {
                    let text = match rec.pretty_body() {
                        Some(b) => format!("{}\n{b}", rec.line()),
                        None => rec.line(),
                    };
                    ui.ctx().copy_text(text);
                }
            });
            if let Some(body) = rec.pretty_body() {
                egui::CollapsingHeader::new("body").id_salt(("journal", i)).show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(body).monospace()).wrap());
                });
            }
            ui.add_space(4.0);
        }
    });
}

/// The question a destructive action asks before it is recorded, or nothing when it is harmless.
#[cfg(feature = "fleet")]
fn confirm_question(action: &Action) -> Option<&'static str> {
    Some(match action {
        Action::Close => "Close this fleet?",
        Action::KickAll => "Kick everyone out of the fleet?",
        Action::KickCapsules => "Kick every pod out of the fleet?",
        Action::Kick { .. } | Action::KickMany { .. } => "Kick them out of the fleet?",
        _ => return None,
    })
}

/// How much damage an action does if it was not meant.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Danger {
    None,
    /// Undoable, but somebody has to be re-invited.
    Caution,
    /// Takes the whole fleet with it.
    Severe,
}

#[cfg(feature = "fleet")]
fn danger(action: &Action) -> Danger {
    match action {
        Action::Close | Action::KickAll => Danger::Severe,
        Action::KickCapsules | Action::Kick { .. } | Action::KickMany { .. } => Danger::Caution,
        _ => Danger::None,
    }
}

/// What a severe action costs, spelled out rather than implied.
#[cfg(feature = "fleet")]
fn confirm_consequence(action: &Action, pilots: usize) -> Option<String> {
    match action {
        Action::Close => Some(format!(
            "The fleet stops being tracked and {pilots} pilots stop earning PAPs for it."
        )),
        Action::KickAll => {
            Some(format!("All {pilots} pilots are removed. They have to be invited back one by one."))
        }
        Action::KickCapsules => Some("Every pilot in a pod is removed.".to_owned()),
        _ => None,
    }
}

#[cfg(all(test, feature = "fleet"))]
mod confirm_tests {
    use super::*;

    /// Folders come out of the presets themselves, once each, sorted, with the top level implied.
    #[test]
    fn preset_folders_are_listed_once_each() {
        let p = |label: &str, folder: &str| crate::settings::FleetPreset {
            label: label.to_owned(),
            folder: folder.to_owned(),
            ..Default::default()
        };
        let presets = vec![
            p("Home Defence", "Home"),
            p("Roam", ""),
            p("Tower bash", "Structures"),
            p("Entosis", " Home "),
            p("Move op", "  "),
        ];
        assert_eq!(preset_folders(&presets), vec!["Home".to_owned(), "Structures".to_owned()]);
        assert!(preset_folders(&[]).is_empty());
    }

    /// Peacetime and strategic sort ahead of everything else, in that order.
    #[test]
    fn the_headline_tags_come_first() {
        let mut names = vec!["Corp", "STRATEGIC", "SIG/SQUAD", "PEACETIME", "Incursion-VG"];
        names.sort_by_key(|n| headline_rank(n));
        assert_eq!(&names[..2], &["PEACETIME", "STRATEGIC"]);
        // Case and stray spaces come from the API, not from the user.
        assert_eq!(headline_rank(" peacetime "), 0);
        assert_eq!(headline_rank("Corp"), 2);
    }

    /// What the tag search keeps and what it drops.
    #[test]
    fn the_tag_search_matches_every_word_anywhere() {
        assert!(tag_matches("Structure Bash", ""));
        assert!(tag_matches("Structure Bash", "  "));
        assert!(tag_matches("Structure Bash", "bash"));
        assert!(tag_matches("Structure Bash", "STRUCT"));
        // Out of order and not adjacent.
        assert!(tag_matches("Structure Bash", "bash struct"));
        assert!(!tag_matches("Structure Bash", "roam"));
        assert!(!tag_matches("Structure Bash", "bash roam"));
    }

    /// How loud each action is, and what its dialog says it costs.
    #[test]
    fn the_worst_actions_say_what_they_cost() {
        assert_eq!(danger(&Action::Close), Danger::Severe);
        assert_eq!(danger(&Action::KickAll), Danger::Severe);
        assert_eq!(danger(&Action::KickCapsules), Danger::Caution);
        assert_eq!(danger(&Action::SetMotd), Danger::None);
        assert_eq!(danger(&Action::AddWing), Danger::None);

        for a in [Action::Close, Action::KickAll] {
            let line = confirm_consequence(&a, 42).expect("a consequence");
            assert!(line.contains("42"), "{line}");
        }
        assert!(confirm_consequence(&Action::SetMotd, 42).is_none());
    }

    /// Anything that asks a question has a danger level, and anything dangerous asks.
    #[test]
    fn a_dangerous_action_always_asks_first() {
        for a in [
            Action::Close,
            Action::KickAll,
            Action::KickCapsules,
            Action::Kick { character_id: 1, exclude: false },
        ] {
            assert_ne!(danger(&a), Danger::None, "{a:?}");
            assert!(confirm_question(&a).is_some(), "{a:?}");
        }
        assert!(confirm_question(&Action::SetMotd).is_none());
    }
}

/// Which half of a fleet's page is showing.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum DetailTab {
    /// Who is in the fleet, by wing and squad, the way the dashboard lists them.
    #[default]
    Members,
    /// What they are flying, grouped by hull and judged against the doctrine.
    Composition,
}

/// A tracked fleet, or a closed one read back.
#[cfg(feature = "fleet")]
fn tracking_page(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    read_only: bool,
    tab: DetailTab,
    set_tab: &mut Option<DetailTab>,
    act_on: &mut Vec<Action>,
    boost_rules: &[crate::settings::FleetBoostRequirement],
    hulls: &[crate::settings::FleetHull],
    open_editor: &mut bool,
    sidebar: &mut bool,
    mine: bool,
    here: Option<String>,
    join: &mut Option<String>,
) {
    let Some(open) = st.open.value.clone() else {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(if st.open.loading {
                "Loading the fleet."
            } else {
                "No fleet open."
            })
            .weak())
        });
        return;
    };

    let seed = st.seed.clone();
    let boosts = st.boosts.clone();
    let off_doctrine = st.off_doctrine.clone();
    // Rebuilt from settings rather than taken from the snapshot, so editing the hull list shows
    // up without waiting for the next poll.
    let open = crate::fleets::state::OpenFleet {
        doctrine: crate::fleets::doctrine::configured(
            open.fleet.setup_id,
            seed.setup_name(open.fleet.setup_id).unwrap_or_default(),
            open.doctrine.clone(),
            hulls,
            wanted_tank(&crate::fleets::boosts::wanted_for(
                open.fleet.setup_id.0.into(),
                boost_rules,
            )),
        ),
        ..open
    };
    let wanted = crate::fleets::boosts::wanted_for(open.fleet.setup_id.0.into(), boost_rules);
    egui::Panel::top("fleet_header").show_inside(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading(&open.fleet.name);
            if let Some(name) = seed.setup_name(open.fleet.setup_id) {
                ui.label(egui::RichText::new(name).weak());
            }
            for t in open.fleet.tag_ids.iter().filter_map(|t| seed.tag(*t)) {
                fleet_tag_chip(ui, t);
            }
            if read_only {
                ui.label(
                    egui::RichText::new("closed").color(crate::theme::standing::WARNING),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let now = chrono::Utc::now().timestamp();
                if let Some(t) = crate::fleets::model::parse_iso(&open.fleet.started_at) {
                    let end = open
                        .fleet
                        .closed_at
                        .as_deref()
                        .and_then(crate::fleets::model::parse_iso)
                        .unwrap_or(now);
                    ui.label(egui::RichText::new(fmt_age(end - t)).strong())
                        .on_hover_text("How long the fleet has been up.");
                }
                if let Some(c) = &open.fleet.commander {
                    ui.label(egui::RichText::new(&c.label).weak());
                }
            });
        });
        if let Some(f) = &open.fleet.formup_location {
            ui.label(egui::RichText::new(format!("Formup: {}", f.label)).weak());
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (t, label) in
                [(DetailTab::Members, "Members"), (DetailTab::Composition, "Composition")]
            {
                if selectable_chip(ui, tab == t, label).clicked() {
                    *set_tab = Some(t);
                }
            }
            ui.label(
                egui::RichText::new(format!("{} pilots", open.composition.total())).weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if selectable_chip(
                    ui,
                    *sidebar,
                    format!("{}  Settings", egui_phosphor::regular::SLIDERS_HORIZONTAL),
                )
                .on_hover_text("Change the doctrine or the comms of this fleet")
                .clicked()
                {
                    *sidebar = !*sidebar;
                }
                // Right-to-left, so writing these after Settings puts them before it.
                comms_buttons(ui, &seed, &open, mine, here.as_deref(), join);
            });
        });
        ui.add_space(4.0);
    });

    egui::Panel::bottom("fleet_actions").frame(bar_frame(ui)).show_inside(ui, |ui| {
        action_bar(ui, st, read_only, act_on);
    });

    if *sidebar {
        egui::Panel::right("fleet_sidebar")
            .resizable(true)
            .default_size(330.0)
            .size_range(270.0..=540.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    readiness_pane(ui, &open, &boosts, &wanted, &mut st.boosts_forced, open_editor);
                    ui.separator();
                    fleet_sidebar(ui, st, &open, read_only, act_on);
                });
            });
    }

    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        let (can_move, can_kick) = (
            !read_only && st.can(Perm::MoveMember),
            !read_only && st.can(Perm::KickMember),
        );
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match tab {
            DetailTab::Members => members_view(ui, &open, can_move, can_kick, act_on),
            DetailTab::Composition => {
                composition_view(ui, &open, &off_doctrine)
            }
        });
    });
}

/// Join the fleet's own comms and the command channel its FC belongs in.
///
/// On the FC's own fleet the buttons also say whether Mumble is actually there: a fleet running
/// with its commander in the wrong channel is a fleet nobody can reach, and it is the kind of
/// mistake that goes unnoticed until it matters.
#[cfg(feature = "fleet")]
fn comms_buttons(
    ui: &mut egui::Ui,
    seed: &crate::fleets::seed::Seed,
    open: &crate::fleets::state::OpenFleet,
    mine: bool,
    here: Option<&str>,
    join: &mut Option<String>,
) {
    use crate::fleets::comms;
    let tags: Vec<_> = open.fleet.tag_ids.iter().filter_map(|t| seed.tag(*t)).cloned().collect();
    let sector = comms::sector(&tags);
    let op_name = seed.channel_name(&seed.mumble_channels, open.fleet.mumble_channel_id);
    let op_url = op_name.and_then(comms::op_url);
    let command_url =
        open.fleet.mumble_channel_id.map(|c| comms::command_url(sector, c.0));

    // Drawn in a right-to-left strip, so the order here is the reverse of how it reads: the
    // fleet's own comms end up first.
    for (label, url) in [
        (format!("Join {} command", sector.label()), command_url),
        ("Join comms".to_owned(), op_url),
    ] {
        let Some(url) = url else { continue };
        // Only a fleet this account is running is worth nagging about: being outside somebody
        // else's comms is the normal state of affairs.
        let away = mine && here.is_some_and(|h| !crate::mumble::in_channel(h, &url));
        let text = egui::RichText::new(format!("{}  {label}", egui_phosphor::regular::HEADPHONES));
        let button = match pulse_fill(ui, away) {
            Some(c) => egui::Button::new(text).fill(c),
            None => egui::Button::new(text),
        };
        let tip = match (mine, here) {
            (true, Some(_)) if away => "Your fleet, and Mumble is somewhere else.".to_owned(),
            (true, Some(_)) => "You are in this channel.".to_owned(),
            (true, None) => "Mumble is not running, so there is nothing to check against."
                .to_owned(),
            _ => format!("Opens {}", crate::mumble::channel_path(&url).unwrap_or_default()),
        };
        if ui.add(button).on_hover_text(tip).clicked() {
            *join = Some(url);
        }
    }
}

/// Which hulls a doctrine flies, and which are welcome in any fleet.
///
/// The dashboard hands over no hulls at all, so without this list every ship in the fleet reads as
/// out of doctrine.
#[cfg(feature = "fleet")]
fn hull_editor(
    ui: &mut egui::Ui,
    setup_id: i32,
    setup_name: &str,
    hint: &str,
    hulls: &mut Vec<crate::settings::FleetHull>,
    ships: &[(i64, String, String)],
) -> bool {
    use crate::fleets::doctrine::Tank;
    let mut changed = false;

    for (owner, title, hint) in [(setup_id, setup_name, hint)] {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(clip(title, 26)).strong()).on_hover_text(title);
            ui.label(egui::RichText::new(hint).weak());
        });

        let mut remove: Option<usize> = None;
        egui::Grid::new(("hull_rows", owner)).num_columns(3).striped(true).spacing([10.0, 3.0]).show(
            ui,
            |ui| {
                for (i, h) in hulls.iter_mut().enumerate() {
                    if h.setup_id != owner {
                        continue;
                    }
                    cell(ui, 200.0, |ui| {
                        ui.label(&h.name);
                    });
                    cell(ui, 120.0, |ui| {
                        // Only worth setting for a hull that suits one kind of fleet; a doctrine's
                        // own ships are its tank by definition.
                        let mut tank = Tank::parse(&h.tank);
                        egui::ComboBox::from_id_salt(("hull_tank", owner, i))
                            .selected_text(tank.map(|t| t.label()).unwrap_or("any tank"))
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for t in [None, Some(Tank::Shield), Some(Tank::Armor)] {
                                    let label = t.map(|t| t.label()).unwrap_or("any tank");
                                    if ui.menu_value(&mut tank, t, label).changed() {
                                        h.tank =
                                            t.map(|t| t.label().to_owned()).unwrap_or_default();
                                        changed = true;
                                    }
                                }
                            });
                    });
                    if ui
                        .button(egui_phosphor::regular::TRASH)
                        .on_hover_text(format!("Drop {}", h.name))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            },
        );
        if let Some(i) = remove {
            hulls.remove(i);
            changed = true;
        }

        // Typed and picked, so the name matches what the composition will carry.
        let q_id = ui.id().with(("hull_query", owner));
        let mut query: String = ui.data(|d| d.get_temp(q_id).unwrap_or_default());
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut query).hint_text("Add a hull").desired_width(200.0),
            );
            let hits: Vec<&(i64, String, String)> = if query.trim().len() >= 2 {
                ships
                    .iter()
                    .filter(|(_, n, _)| tag_matches(n, &query))
                    .filter(|(id, n, _)| {
                        !hulls.iter().any(|h| {
                            h.setup_id == owner
                                && (h.type_id == *id || h.name.eq_ignore_ascii_case(n))
                        })
                    })
                    .take(10)
                    .collect()
            } else {
                Vec::new()
            };
            let hov_id = ui.id().with(("hull_popup_hover", owner));
            let was_over: bool = ui.data(|d| d.get_temp(hov_id).unwrap_or(false));
            // Kept open while the pointer is over it: clicking an item takes focus off the field,
            // and a popup that closes on the press never sees the click.
            let popup = egui::Popup::from_response(&field)
                .open(!hits.is_empty() && (field.has_focus() || was_over))
                .width(260.0)
                .show(|ui| {
                    for (id, n, g) in &hits {
                        if ui.menu_label(false, format!("{n}   {g}")).clicked() {
                            hulls.push(crate::settings::FleetHull {
                                setup_id: owner,
                                type_id: *id,
                                name: n.clone(),
                                tank: String::new(),
                            });
                            changed = true;
                            query.clear();
                        }
                    }
                });
            let over = popup.as_ref().is_some_and(|r| r.response.contains_pointer());
            ui.data_mut(|d| d.insert_temp(hov_id, over));
            if ships.is_empty() {
                ui.label(egui::RichText::new("no ship data loaded").weak());
            }
        });
        ui.data_mut(|d| d.insert_temp(q_id, query));
        ui.add_space(8.0);
    }
    changed
}

/// The hulls every fleet takes, whatever it is flying.
///
/// The groups are built in and not worth a list to maintain; anything else is named here, and a
/// hull that only suits one kind of fleet carries its tank.
#[cfg(feature = "fleet")]
fn always_allowed(
    ui: &mut egui::Ui,
    hulls: &mut Vec<crate::settings::FleetHull>,
    ships: &[(i64, String, String)],
) -> bool {
    ui.label(
        egui::RichText::new(
            "Never out of doctrine, in any fleet. The fleet commander's own ship is exempt too.",
        )
        .weak(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Always:").strong());
        for g in crate::fleets::doctrine::SUPPORT_GROUPS {
            ui.label(egui::RichText::new(*g).color(ui.visuals().hyperlink_color));
        }
    });
    ui.add_space(8.0);
    hull_editor(ui, 0, "Hulls", "welcome in every fleet: cynos, bridges, scouts", hulls, ships)
}

/// Shortens a label so a long one cannot widen the panel it sits in.
#[cfg(feature = "fleet")]
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}\u{2026}", kept.trim_end())
}

/// One colour per burst, so shield, armor, information and skirmish read apart at a glance.
#[cfg(feature = "fleet")]
fn burst_colour(ui: &egui::Ui, burst: crate::fleets::boosts::Burst) -> egui::Color32 {
    use crate::fleets::boosts::Burst;
    use crate::theme::{chip, standing};
    match burst {
        Burst::Shield => ui.visuals().hyperlink_color,
        Burst::Armor => chip::ISK,
        Burst::Information => chip::STRUCTURE,
        Burst::Skirmish => standing::FRIENDLY,
        Burst::Mining => ui.visuals().weak_text_color(),
    }
}

/// Whether the fleet can fight, and why it reads that way.
///
/// The numbers are the point: "Logi danger" on its own is an argument, "3 of 40, two Guardians in
/// a shield fleet" is something to act on.
#[cfg(feature = "fleet")]
fn readiness_pane(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    coverage: &[crate::fleets::boosts::Coverage],
    wanted: &[crate::fleets::boosts::Wanted],
    marks: &mut crate::fleets::boosts::Forced,
    open_editor: &mut bool,
) {
    use crate::fleets::{checks, logi};
    let comp = &open.composition;
    if comp.total() == 0 {
        return;
    }
    let tank = wanted_tank(wanted);
    let report = logi::report(comp, open.doctrine.as_ref(), tank);

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Readiness").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(egui_phosphor::regular::SLIDERS_HORIZONTAL).frame(false))
                .on_hover_text("Set which boosts this doctrine wants")
                .clicked()
            {
                *open_editor = true;
            }
        });
    });
    ui.add_space(2.0);

    status_line(ui, &checks::logi_from(&report));
    detail_line(
        ui,
        format!(
            "{} brought, {} usable, wants {} {}",
            report.brought(),
            report.counted,
            report.size.label(),
            tank.map(|t| t.label()).unwrap_or("logi"),
        ),
        None,
    );
    for r in &report.rejected {
        detail_line(
            ui,
            format!("{}, {}", r.pilot, r.why.label()),
            Some((r.ship.clone(), crate::theme::standing::HOSTILE)),
        );
    }
    ui.add_space(6.0);

    // The pane's own headline, short: the gaps are listed as chips under it rather than run
    // together into a sentence that wraps three lines in a sidebar.
    let long = checks::boosts(wanted, coverage);
    let gaps = crate::fleets::boosts::gaps_with(wanted, coverage, marks);
    let level = match gaps.first().map(|g| g.priority) {
        Some(crate::fleets::boosts::Priority::High) => checks::Level::Danger,
        Some(crate::fleets::boosts::Priority::Medium) => checks::Level::Warning,
        _ => checks::Level::Fine,
    };
    let short = match (wanted.is_empty(), gaps.len()) {
        (true, _) => "Nothing set for this doctrine.".to_owned(),
        (false, 0) => format!("All {} covered.", wanted.len()),
        (false, n) => format!("{n} of {} not covered.", wanted.len()),
    };
    status_line(ui, &checks::Check { detail: short, level, ..long });
    if !gaps.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            ui.label(egui::RichText::new("Not covered").weak());
            for g in &gaps {
                if gap_chip(ui, g).clicked() {
                    marks.insert(g.what.clone(), true);
                }
            }
        });
    }
    // Anything the FC judged for themselves, and a way back.
    let by_hand: Vec<(String, bool)> = marks.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let mut undo: Option<String> = None;
    if !by_hand.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            ui.label(egui::RichText::new("By hand").weak());
            for (what, on) in &by_hand {
                let text = egui::RichText::new(format!(
                    "{what} {}",
                    if *on { "covered" } else { "not covered" }
                ));
                if ui
                    .add(
                        egui::Button::new(text)
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .stroke(egui::Stroke::new(1.0, ui.visuals().weak_text_color())),
                    )
                    .on_hover_text("Go back to what the channel says")
                    .clicked()
                {
                    undo = Some(what.clone());
                }
            }
        });
    }
    if let Some(what) = undo {
        marks.remove(&what);
    }
    detail_line(ui, checks::booster_line(comp, open.doctrine.as_ref()), None);
    if coverage.is_empty() {
        detail_line(ui, "Nobody has posted in the boost channel.".to_owned(), None);
    }
    for c in coverage {
        let ml = if c.mindlinked > 0 {
            format!("{} with ML", c.mindlinked)
        } else {
            "no ML".to_owned()
        };
        let colour = burst_colour(ui, c.burst);
        // Both halves take the click, not just the last one laid out.
        let resp = ui
            .horizontal_wrapped(|ui| {
                ui.add_space(14.0);
                let name = ui.add(
                    egui::Label::new(egui::RichText::new(&c.what).color(colour))
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .sense(egui::Sense::click()),
                );
                let count = ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("{} pilot(s), {ml}", c.pilots)).weak(),
                    )
                    .sense(egui::Sense::click()),
                );
                name.union(count)
            })
            .inner;
        if resp.on_hover_text("Mark this one as not covered").clicked() {
            marks.insert(c.what.clone(), false);
        }
    }
    ui.add_space(6.0);

    for check in [checks::interdiction(comp), checks::tackle(comp)] {
        status_line(ui, &check);
    }
    ui.add_space(4.0);
}

/// One boost nobody is on. The important ones carry a filled background, since a colour alone on
/// a red or amber theme is not much of a difference.
#[cfg(feature = "fleet")]
fn gap_chip(ui: &mut egui::Ui, g: &crate::fleets::boosts::Wanted) -> egui::Response {
    use crate::fleets::boosts::Priority;
    let colour = priority_colour(g.priority);
    let text = egui::RichText::new(&g.what).color(colour).strong();
    let button = egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend);
    let button = match g.priority {
        Priority::High => button
            .fill(colour.gamma_multiply(0.25))
            .stroke(egui::Stroke::new(1.0, colour)),
        Priority::Medium => button.stroke(egui::Stroke::new(1.0, colour.gamma_multiply(0.6))),
        Priority::Low => button.frame(false),
    };
    ui.add(button).on_hover_text(format!(
        "{} priority. Click to mark it covered.",
        g.priority.label()
    ))
}

/// One check as a coloured headline plus its reason.
#[cfg(feature = "fleet")]
fn status_line(ui: &mut egui::Ui, check: &crate::fleets::checks::Check) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(level_icon(check.level)).color(level_colour(check.level)));
        ui.label(egui::RichText::new(&check.what).strong().color(level_colour(check.level)));
        ui.label(&check.detail);
    });
}

/// An indented line under a status, optionally led by a coloured name.
#[cfg(feature = "fleet")]
fn detail_line(ui: &mut egui::Ui, text: String, lead: Option<(String, egui::Color32)>) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(14.0);
        if let Some((name, colour)) = lead {
            ui.add(
                egui::Label::new(egui::RichText::new(name).color(colour))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        }
        ui.label(egui::RichText::new(text).weak());
    });
}

/// Which way the fleet tanks, taken from the boosts its doctrine asks for. The user maintains that
/// list, so it beats guessing from the doctrine's name.
#[cfg(feature = "fleet")]
fn wanted_tank(wanted: &[crate::fleets::boosts::Wanted]) -> Option<crate::fleets::logi::Tank> {
    use crate::fleets::boosts::{burst_of, Burst};
    use crate::fleets::logi::Tank;
    let mut shield = 0usize;
    let mut armor = 0usize;
    for w in wanted {
        match burst_of(&w.what).or_else(|| Burst::parse(&w.what)) {
            Some(Burst::Shield) => shield += 1,
            Some(Burst::Armor) => armor += 1,
            _ => {}
        }
    }
    match shield.cmp(&armor) {
        std::cmp::Ordering::Greater => Some(Tank::Shield),
        std::cmp::Ordering::Less => Some(Tank::Armor),
        std::cmp::Ordering::Equal => None,
    }
}

/// What can still be changed about a fleet that is already up: what it flies and where it talks.
///
/// Staged rather than live, because every field here is a request: changing the setup three times
/// while making up your mind would be three PUTs and three MOTDs.
#[cfg(feature = "fleet")]
fn fleet_sidebar(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    open: &crate::fleets::state::OpenFleet,
    read_only: bool,
    act_on: &mut Vec<Action>,
) {
    let seed = st.seed.clone();
    let fleet = open.fleet.clone();
    let can = st.can(Perm::AccessFleet) && !read_only;
    let e = &mut st.edit;

    ui.add_space(4.0);
    ui.label(egui::RichText::new("Fleet settings").strong());
    ui.add_space(4.0);

    egui::Grid::new("fleet_sidebar_grid")
        .num_columns(2)
        .min_col_width(60.0)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Setup");
            let current = seed
                .setups
                .iter()
                .find(|s| s.id == e.setup_id)
                .map(|s| s.name.trim().to_owned())
                .unwrap_or_else(|| "== Choose a setup ==".to_owned());
            egui::ComboBox::from_id_salt("sidebar_setup")
                .width(190.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for s in &seed.setups {
                        ui.menu_value(&mut e.setup_id, s.id, s.name.trim());
                    }
                });
            ui.end_row();

            for (label, salt, list, slot) in [
                ("Comms", "sidebar_mumble", &seed.mumble_channels, &mut e.mumble),
                ("Logi", "sidebar_logi", &seed.logi_channels, &mut e.logi),
                ("Boost", "sidebar_boost", &seed.boost_channels, &mut e.boost),
            ] {
                ui.label(label);
                let current = slot
                    .and_then(|id| list.iter().find(|c| c.id == id))
                    .map(|c| c.name.trim().to_owned())
                    .unwrap_or_else(|| "none".to_owned());
                egui::ComboBox::from_id_salt(salt).width(190.0).selected_text(current).show_ui(
                    ui,
                    |ui| {
                        ui.menu_value(slot, None, "none");
                        for c in list {
                            let text = if c.is_in_use {
                                egui::RichText::new(format!("{}  in use", c.name.trim())).weak()
                            } else {
                                egui::RichText::new(c.name.trim().to_owned())
                            };
                            ui.menu_value(slot, Some(c.id), text);
                        }
                    },
                );
                ui.end_row();
            }
        });

    ui.add_space(6.0);
    ui.label("Tags");
    let tags = seed.tags.clone();
    for (salt, primary) in [("track_tag_primary", true), ("track_tag_secondary", false)] {
        let mut pool: Vec<_> = tags.iter().filter(|t| t.is_primary == primary).cloned().collect();
        if primary {
            pool.sort_by_key(|t| (headline_rank(&t.name), t.id.0));
            headline_tags(ui, &pool, &mut e.tags);
        }
        ui.horizontal(|ui| {
            tag_field(ui, salt, &pool, &mut e.tags, primary);
        });
    }

    ui.add_space(6.0);
    ui.checkbox(&mut e.set_motd, "Re-set the MOTD")
        .on_hover_text("The MOTD names the channels, so it goes stale when they change.");
    ui.add_space(6.0);

    let dirty = e.differs(&fleet);
    ui.horizontal(|ui| {
        let apply = ui
            .add_enabled(
                can && dirty,
                egui::Button::new(format!("{}  Apply", egui_phosphor::regular::CHECK)),
            )
            .on_disabled_hover_text(if read_only {
                "Closed fleet, read-only."
            } else if !dirty {
                "Nothing changed."
            } else {
                "Your account does not have the accessFleet permission."
            });
        if apply.clicked() {
            act_on.push(Action::Update(Box::new(e.applied(&fleet))));
            if e.set_motd {
                act_on.push(Action::SetMotd);
            }
        }
        if ui
            .add_enabled(dirty, egui::Button::new("Discard"))
            .on_disabled_hover_text("Nothing changed.")
            .clicked()
        {
            e.of = None;
            e.seed(&fleet);
        }
    });
    if dirty {
        ui.label(
            egui::RichText::new("Not applied yet.").color(crate::theme::standing::WARNING),
        );
    }
}

#[cfg(feature = "fleet")]
fn level_colour(level: crate::fleets::checks::Level) -> egui::Color32 {
    use crate::fleets::checks::Level;
    match level {
        Level::Fine => crate::theme::standing::FRIENDLY,
        Level::Warning => crate::theme::standing::WARNING,
        Level::Danger => crate::theme::chip::TACKLED,
        Level::Critical => crate::theme::standing::HOSTILE,
    }
}

#[cfg(feature = "fleet")]
fn level_icon(level: crate::fleets::checks::Level) -> &'static str {
    use crate::fleets::checks::Level;
    use egui_phosphor::regular as icon;
    match level {
        Level::Fine => icon::CHECK_CIRCLE,
        Level::Warning | Level::Danger => icon::WARNING,
        Level::Critical => icon::WARNING_OCTAGON,
    }
}

#[cfg(feature = "fleet")]
fn priority_colour(p: crate::fleets::boosts::Priority) -> egui::Color32 {
    use crate::fleets::boosts::Priority;
    match p {
        Priority::High => crate::theme::standing::HOSTILE,
        Priority::Medium => crate::theme::standing::WARNING,
        Priority::Low => crate::theme::standing::NEUTRAL,
    }
}

/// What can be done to the fleet, and what this account may not do.
#[cfg(feature = "fleet")]
fn action_bar(
    ui: &mut egui::Ui,
    st: &crate::fleets::FleetState,
    read_only: bool,
    act_on: &mut Vec<Action>,
) {
    use egui_phosphor::regular as icon;
    ui.horizontal_wrapped(|ui| {
        if read_only {
            ui.label(
                egui::RichText::new("Closed fleet, read-only.")
                    .color(crate::theme::standing::WARNING),
            );
            return;
        }
        // Inviting needs somebody to invite, which is a picker this page does not have yet.
        let actions: [(&str, &str, Action); 5] = [
            (icon::MEGAPHONE, "Set MOTD", Action::SetMotd),
            (icon::STACK, "Add wing", Action::AddWing),
            (icon::PROHIBIT, "Kick pods", Action::KickCapsules),
            (icon::SIGN_OUT, "Kick everyone", Action::KickAll),
            (icon::X_CIRCLE, "Close fleet", Action::Close),
        ];
        for (glyph, label, action) in actions {
            let perm = action.perm();
            let allowed = st.can(perm);
            let text = egui::RichText::new(format!("{glyph}  {label}"));
            let button = match danger(&action) {
                Danger::Severe => egui::Button::new(text.color(crate::theme::standing::HOSTILE))
                    .stroke(egui::Stroke::new(1.0, crate::theme::standing::HOSTILE)),
                Danger::Caution => egui::Button::new(text.color(crate::theme::standing::WARNING)),
                Danger::None => egui::Button::new(text),
            };
            let resp = ui.add_enabled(allowed, button).on_disabled_hover_text(format!(
                "Your account does not have the {} permission.",
                perm.as_str()
            ));
            let resp = if allowed {
                resp.on_hover_text("Records the request this would send. Nothing leaves the app.")
            } else {
                resp
            };
            if resp.clicked() {
                act_on.push(action);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("recorded, not sent").color(crate::theme::standing::WARNING),
            );
        });
    });
}

/// Who is in the fleet, by wing and squad.
#[cfg(feature = "fleet")]
fn members_view(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    can_move: bool,
    can_kick: bool,
    act_on: &mut Vec<Action>,
) {
    use crate::fleets::model::Seat;
    let comp = &open.composition;
    if comp.wings.is_empty() && comp.commander.is_none() {
        ui.label(egui::RichText::new("Nobody in the fleet.").weak());
        return;
    }
    let mut drop_on: Option<(i64, Seat)> = None;
    // A roster reads as a table, so the rows sit closer together than the app's default rhythm.
    ui.spacing_mut().item_spacing.y = 2.0;
    // Every column but the name is anchored to this edge, so the tree's indentation eats into the
    // name and nothing else steps right as it nests.
    let left = ui.max_rect().left();

    // Indented like a wing, so the fleet commander has the same left rail as everything under it.
    ui.indent("fleet_boss", |ui| {
        commander_seat(ui, open, left, Seat::Boss, comp.commander.as_ref(), can_move, can_kick,
                       act_on, &mut drop_on);
    });

    for wing in &comp.wings {
        let pilots: usize = wing.squads.iter().map(|s| s.members.len() + usize::from(s.commander.is_some())).sum::<usize>()
            + usize::from(wing.commander.is_some());
        egui::CollapsingHeader::new(format!("{}   {pilots}", wing.name))
            .id_salt(("wing", wing.id.0))
            .default_open(true)
            .show(ui, |ui| {
                commander_seat(ui, open, left, Seat::WingCommander(wing.id),
                               wing.commander.as_ref(), can_move, can_kick, act_on, &mut drop_on);
                for squad in &wing.squads {
                    let n = squad.members.len() + usize::from(squad.commander.is_some());
                    egui::CollapsingHeader::new(format!("{}   {n}", squad.name))
                        .id_salt(("squad", wing.id.0, squad.id.0))
                        .default_open(true)
                        .show(ui, |ui| {
                            commander_seat(
                                ui,
                                open,
                                left,
                                Seat::SquadCommander(wing.id, squad.id),
                                squad.commander.as_ref(),
                                can_move,
                                can_kick,
                                act_on,
                                &mut drop_on,
                            );
                            // The whole squad body takes a drop, so a pilot can be dragged onto an
                            // empty squad as well as onto one with rows in it.
                            let (_, dropped) =
                                ui.dnd_drop_zone::<DragPilot, _>(egui::Frame::NONE, |ui| {
                                    ui.set_min_size(egui::vec2(ui.available_width(), 16.0));
                                    if squad.members.is_empty() {
                                        ui.label(egui::RichText::new("Empty").weak());
                                    }
                                    for m in &squad.members {
                                        member_row(ui, open, left, m,
                                                   Seat::Squad(wing.id, squad.id), None,
                                                   can_move, can_kick, act_on);
                                    }
                                });
                            if let Some(p) = dropped {
                                drop_on =
                                    Some((p.character_id, Seat::Squad(wing.id, squad.id)));
                            }
                        });
                    ui.add_space(2.0);
                }
            });
        ui.add_space(2.0);
    }

    // A move that would change nothing is not a request worth recording.
    if let Some((character_id, seat)) = drop_on {
        if can_move && comp.seat_of(character_id) != Some(seat) {
            let (wing, squad) = seat.ids();
            act_on.push(Action::Move { character_id, wing, squad });
        }
    }
}

/// One commander seat. Holds at most one pilot, which is what makes it a seat rather than a list.
#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn commander_seat(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    left: f32,
    seat: crate::fleets::model::Seat,
    holder: Option<&crate::fleets::model::Member>,
    can_move: bool,
    can_kick: bool,
    act_on: &mut Vec<Action>,
    drop_on: &mut Option<(i64, crate::fleets::model::Seat)>,
) {
    let title = match seat {
        crate::fleets::model::Seat::Boss => "FC",
        crate::fleets::model::Seat::WingCommander(_) => "WC",
        _ => "SC",
    };
    let (_, dropped) = ui.dnd_drop_zone::<DragPilot, _>(egui::Frame::NONE, |ui| {
        ui.set_min_width(ui.available_width());
        match holder {
            Some(m) => {
                member_row(ui, open, left, m, seat, Some(title), can_move, can_kick, act_on)
            }
            None => {
                ui.horizontal(|ui| {
                    seat_badge(ui, title, seat);
                    ui.label(egui::RichText::new("empty").weak());
                });
            }
        }
    });
    if let Some(p) = dropped {
        // One commander per seat: the holder has to be moved out before anyone else moves in, so
        // the drop is refused rather than silently sending a request the server will reject.
        if holder.is_none() {
            *drop_on = Some((p.character_id, seat));
        }
    }
}

/// A pilot in flight between two squads.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Debug)]
struct DragPilot {
    character_id: i64,
    seat: crate::fleets::model::Seat,
}

/// Column widths every fleet table shares, so the member list and the composition line up.
#[cfg(feature = "fleet")]
const COL: [f32; 4] = [190.0, 150.0, 120.0, 90.0];
/// The FC / WC / SC column, present on every roster row so the names align.
#[cfg(feature = "fleet")]
const BADGE_W: f32 = 36.0;
/// The name column at the top of the tree. Nesting comes out of this one.
#[cfg(feature = "fleet")]
const NAME_W: f32 = 230.0;
/// Ship icon beside a hull name.
#[cfg(feature = "fleet")]
const SHIP_ICON: f32 = 18.0;

/// Which seat a roster row is, when it is one.
#[cfg(feature = "fleet")]
fn seat_badge(ui: &mut egui::Ui, title: &str, seat: crate::fleets::model::Seat) {
    cell(ui, BADGE_W, |ui| {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(title).strong().color(ui.visuals().hyperlink_color))
            .on_hover_text(seat.label());
    });
}

/// Lays out one cell of a fleet table at a fixed width.
#[cfg(feature = "fleet")]
fn cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui);
        },
    );
}

/// One pilot: draggable by the name, with a kick of their own.
#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn member_row(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    left: f32,
    m: &crate::fleets::model::Member,
    seat: crate::fleets::model::Seat,
    badge: Option<&str>,
    can_move: bool,
    can_kick: bool,
    act_on: &mut Vec<Action>,
) {
    use crate::fleets::doctrine::Standing;
    let standing =
        crate::fleets::doctrine::classify_in(&open.composition, m, open.doctrine.as_ref());
    // A tint rather than coloured text: on a red or orange theme a hostile-coloured ship name is
    // barely a shade away from a normal one.
    let tint = match standing {
        Standing::Unexpected => Some(crate::theme::standing::HOSTILE.gamma_multiply(0.18)),
        Standing::WrongTank => Some(crate::theme::standing::WARNING.gamma_multiply(0.16)),
        _ => None,
    };
    let frame = match tint {
        Some(c) => egui::Frame::NONE.fill(c).inner_margin(egui::Margin::symmetric(0, 1)),
        None => egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 1)),
    };
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            let id = egui::Id::new(("fleet_pilot", m.character_id));
            // Every row carries the badge column, filled or not, so a commander's name starts at
            // the same edge as the pilots under them.
            match badge {
                Some(b) => seat_badge(ui, b, seat),
                None => cell(ui, BADGE_W, |_| {}),
            }
            // The nesting is paid for out of the name column, so the ship, group, role and kick
            // sit on the same edge whatever depth the row is at.
            let indent = (ui.max_rect().left() - left).max(0.0);
            cell(ui, (NAME_W - indent).max(60.0), |ui| {
                if can_move {
                    let payload = DragPilot { character_id: m.character_id, seat };
                    ui.dnd_drag_source(id, payload, |ui| {
                        ui.label(format!(
                            "{}  {}",
                            egui_phosphor::regular::DOTS_SIX_VERTICAL,
                            m.name
                        ));
                    })
                    .response
                    .on_hover_text("Drag onto another squad to move this pilot.");
                } else {
                    ui.label(&m.name);
                }
            });
            cell(ui, COL[1], |ui| {
                ui.add(
                    egui::Image::new(eve_type_icon_url(m.ship_type_id, SHIP_ICON))
                        .fit_to_exact_size(egui::Vec2::splat(SHIP_ICON)),
                );
                ui.label(&m.ship_type_name).on_hover_text(standing.label());
            });
            cell(ui, COL[2], |ui| {
                ui.label(egui::RichText::new(&m.ship_group).weak());
            });
            cell(ui, COL[3], |ui| {
                ui.label(egui::RichText::new(&m.role).weak());
            });
            if ui
                .add_enabled(
                    can_kick,
                    egui::Button::new(egui_phosphor::regular::SIGN_OUT).frame(false),
                )
                .on_disabled_hover_text("Your account does not have the kickMember permission.")
                .on_hover_text(format!("Kick {}", m.name))
                .clicked()
            {
                act_on.push(Action::Kick { character_id: m.character_id, exclude: false });
            }
        });
    });
}

/// What the fleet is flying, and whether the doctrine asked for it.
#[cfg(feature = "fleet")]
fn composition_view(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    off_doctrine: &[crate::fleets::doctrine::OffDoctrine],
) {
    use crate::fleets::doctrine::{by_category, by_ship, unexpected_pilots, Standing};
    let doctrine = open.doctrine.as_ref();
    let lines = by_ship(&open.composition, doctrine);
    if lines.is_empty() {
        ui.label(egui::RichText::new("Nobody in the fleet.").weak());
        return;
    }
    // Everything above the tables lives in the readiness pane now, so the tab is the tables.
    let total = open.composition.total().max(1);

    // The doctrine hulls are the ones an FC counts one by one, so they stay per hull. Everything
    // else reads as a role with the hulls behind it.
    let core: Vec<_> = lines.iter().filter(|l| l.standing == Standing::Doctrine).collect();
    if !core.is_empty() {
        section_head(ui, "Doctrine", core.iter().map(|l| l.count).sum(), Standing::Doctrine);
        egui::Grid::new("comp_doctrine").num_columns(4).striped(true).spacing([12.0, 3.0]).show(
            ui,
            |ui| {
                for l in &core {
                    share_row(ui, &l.name, l.count, total, None);
                }
            },
        );
        ui.add_space(6.0);
    }

    for (title, standing) in
        [("Support", Standing::Support), ("Not in doctrine", Standing::Unexpected)]
    {
        let group: Vec<_> =
            lines.iter().filter(|l| l.standing == standing).cloned().collect();
        if group.is_empty() {
            continue;
        }
        section_head(ui, title, group.iter().map(|l| l.count).sum(), standing);
        egui::Grid::new(("comp", title)).num_columns(4).striped(true).spacing([12.0, 3.0]).show(
            ui,
            |ui| {
                for c in by_category(&group) {
                    let hulls: Vec<String> =
                        c.ships.iter().map(|(n, k)| format!("{n} {k}")).collect();
                    share_row(
                        ui,
                        c.category.label(),
                        c.count,
                        total,
                        Some(hulls.join(", ")),
                    );
                }
            },
        );
        ui.add_space(6.0);
    }

    let odd = unexpected_pilots(&lines);
    off_doctrine_report(ui, off_doctrine, open.at, odd);
}

/// The heading over one composition table.
#[cfg(feature = "fleet")]
fn section_head(
    ui: &mut egui::Ui,
    title: &str,
    pilots: usize,
    standing: crate::fleets::doctrine::Standing,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().color(standing_colour(ui, standing)));
        ui.label(egui::RichText::new(format!("{pilots}")).weak());
    });
}

/// One row of a composition table: what, how many, what share, and what it is made of.
#[cfg(feature = "fleet")]
fn share_row(ui: &mut egui::Ui, name: &str, count: usize, total: usize, detail: Option<String>) {
    let share = count as f32 / total as f32;
    cell(ui, COL[0], |ui| {
        ui.label(name);
    });
    cell(ui, 40.0, |ui| {
        ui.label(egui::RichText::new(format!("{count}")).strong());
    });
    cell(ui, 110.0, |ui| {
        ui.label(egui::RichText::new(format!("{:.0}%", share * 100.0)).weak());
        ui.add(egui::ProgressBar::new(share).desired_width(70.0).desired_height(6.0));
    });
    cell(ui, COL[1] + COL[2], |ui| {
        if let Some(d) = detail {
            ui.label(egui::RichText::new(d).weak());
        }
    });
    ui.end_row();
}

/// Who has been in the wrong ship long enough that it was not a mistake on undock.
#[cfg(feature = "fleet")]
fn off_doctrine_report(
    ui: &mut egui::Ui,
    rows: &[crate::fleets::doctrine::OffDoctrine],
    now: i64,
    odd: usize,
) {
    use crate::fleets::doctrine::{lingering, OFF_DOCTRINE_GRACE};
    let late = lingering(rows, now, OFF_DOCTRINE_GRACE);
    if late.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Off doctrine")
                .strong()
                .color(crate::theme::standing::HOSTILE),
        );
        let _ = odd;
        ui.label(egui::RichText::new(format!("{}", late.len())).weak()).on_hover_text(format!(
            "In a hull the doctrine never asked for, for more than {} minutes.",
            OFF_DOCTRINE_GRACE / 60
        ));
    });
    egui::Grid::new("comp_off_doctrine").num_columns(3).striped(true).spacing([12.0, 3.0]).show(
        ui,
        |ui| {
            for r in &late {
                cell(ui, COL[0], |ui| {
                    ui.label(&r.name);
                });
                cell(ui, 150.0, |ui| {
                    ui.label(egui::RichText::new(fmt_age(now - r.since)).strong());
                });
                cell(ui, COL[1] + COL[2], |ui| {
                    ui.label(egui::RichText::new(&r.ship).color(crate::theme::standing::HOSTILE));
                });
                ui.end_row();
            }
        },
    );
    ui.add_space(6.0);
}

/// Doctrine reads as normal, support as a quiet aside, anything else as a problem.
#[cfg(feature = "fleet")]
fn standing_colour(ui: &egui::Ui, standing: crate::fleets::doctrine::Standing) -> egui::Color32 {
    use crate::fleets::doctrine::Standing;
    match standing {
        Standing::Doctrine => ui.visuals().text_color(),
        Standing::Support => ui.visuals().hyperlink_color,
        Standing::WrongTank => crate::theme::standing::WARNING,
        Standing::Unexpected => crate::theme::standing::HOSTILE,
    }
}

impl SpaiApp {
    /// The preset picker: type, pick, and the start form comes up filled in.
    ///
    /// A window rather than a menu, because there are enough presets across enough folders that
    /// the search is the point.
    #[cfg(feature = "fleet")]
    pub(crate) fn quick_fleet_window(
        &mut self,
        ctx: &egui::Context,
        presets: &[crate::settings::FleetPreset],
        act: &mut FormAct,
    ) {
        if !self.fleet_quick_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Quick Fleet")
            .open(&mut open)
            .default_size([340.0, 420.0])
            .collapsible(false)
            .show(ctx, |ui| {
                if presets.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No presets yet. Fill the start form and save it as one.",
                        )
                        .weak(),
                    );
                    return;
                }
                let search_id = ui.id().with("quick_search");
                let mut query: String = ui.data(|d| d.get_temp(search_id).unwrap_or_default());
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("Search presets")
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                if edit.changed() {
                    ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
                }
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut any = false;
                    let mut groups: Vec<String> = vec![String::new()];
                    groups.extend(preset_folders(presets));
                    for folder in groups {
                        let hits: Vec<usize> = presets
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| p.folder.trim() == folder)
                            // The folder name counts as part of what a preset is called, so
                            // typing the folder finds everything in it.
                            .filter(|(_, p)| {
                                tag_matches(&format!("{} {}", p.folder, p.label), &query)
                            })
                            .map(|(i, _)| i)
                            .collect();
                        if hits.is_empty() {
                            continue;
                        }
                        any = true;
                        if !folder.is_empty() {
                            ui.label(egui::RichText::new(&folder).strong());
                        }
                        for i in hits {
                            let p = &presets[i];
                            // The description first, then the fleet name, and neither when it
                            // only repeats the label the button already carries.
                            let sub = [p.description.as_str(), p.name.as_str()]
                                .into_iter()
                                .map(str::trim)
                                .find(|s| !s.is_empty() && !s.eq_ignore_ascii_case(p.label.trim()))
                                .unwrap_or_default();
                            let resp = ui.add(
                                egui::Button::new(format!("{}   {sub}", p.label))
                                    .min_size(egui::vec2(ui.available_width(), 0.0)),
                            );
                            if resp.clicked() {
                                act.quick_preset = Some(i);
                            }
                        }
                        ui.add_space(4.0);
                    }
                    if !any {
                        ui.label(egui::RichText::new("Nothing matches.").weak());
                    }
                });
            });
        if !open {
            self.fleet_quick_open = false;
        }
    }

    /// Asks before anything that cannot be taken back.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_confirm_modal(&mut self, ctx: &egui::Context) {
        let Some((id, action, question, pilots)) = self.fleet_confirm.clone() else { return };
        let mut decided: Option<bool> = None;
        egui::Modal::new(egui::Id::new("fleet_confirm")).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.heading(egui::RichText::new(question).color(match danger(&action) {
                Danger::Severe => crate::theme::standing::HOSTILE,
                _ => ui.visuals().text_color(),
            }));
            if let Some(line) = confirm_consequence(&action, pilots) {
                ui.label(line);
            }
            ui.label(
                egui::RichText::new("This build records the request and sends nothing.").weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    decided = Some(false);
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Do it").color(crate::theme::standing::HOSTILE),
                        )
                        .stroke(egui::Stroke::new(1.0, crate::theme::standing::HOSTILE)),
                    )
                    .clicked()
                {
                    decided = Some(true);
                }
            });
        });
        match decided {
            Some(true) => {
                self.fleet_confirm = None;
                self.fleet_dispatch(Cmd::Act(id, action));
            }
            Some(false) => self.fleet_confirm = None,
            None => {}
        }
    }
}
