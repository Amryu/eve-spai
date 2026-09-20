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
        }
        let dry = self.fleet_backend.is_dry_run();
        let seed_hint = crate::fleets::seed_path_hint();
        let presets = self.settings.fleet_presets.clone();
        let boost_rules = self.settings.fleet_boost_requirements.clone();
        let mut open_boost_editor = false;
        let journal_open = self.fleet_journal_open;
        let detail_tab = self.fleet_detail_tab;
        let mut toggle_journal = false;
        let mut set_tab: Option<DetailTab> = None;
        let mut act_on: Option<Action> = None;
        let mut goto: Option<Page> = None;
        let mut cmd: Option<Cmd> = None;
        let mut refresh = false;
        let mut act = FormAct::default();

        egui::Panel::top("fleet_subnav").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                let tab = page.tab();
                for (p, label) in [(Page::Fleets, "Fleets"), (Page::Start, "Start fleet")] {
                    if selectable_chip(ui, tab == p, label).clicked() && page != p {
                        goto = Some(p);
                    }
                }
                if let Some(id) = page.fleet() {
                    ui.label(egui_phosphor::regular::CARET_RIGHT);
                    let name = st.open.value.as_ref().map(|o| o.fleet.name.clone());
                    ui.label(
                        egui::RichText::new(name.unwrap_or_else(|| id.short().to_owned())).strong(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                Page::Start => start_page(ui, &mut st, &presets, &mut act),
                Page::Tracking(_) => {
                    tracking_page(ui, &mut st, false, detail_tab, &mut set_tab, &mut act_on, &boost_rules, &mut open_boost_editor)
                }
                Page::Historic(_) => {
                    tracking_page(ui, &mut st, true, detail_tab, &mut set_tab, &mut act_on, &boost_rules, &mut open_boost_editor)
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
        if let Some(action) = act_on {
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
        if open_boost_editor {
            self.fleet_boost_editor = true;
        }
        self.fleet_confirm_modal(ui.ctx());
        self.fleet_apply_form(act);
    }

    /// Applies what the start form asked for once the state lock is gone.
    #[cfg(feature = "fleet")]
    fn fleet_apply_form(&mut self, act: FormAct) {
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
        if let Some(label) = act.save_preset {
            let preset =
                self.fleet.lock().unwrap_or_else(|e| e.into_inner()).preset_from_form(&label);
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
                    "{}  Boosts per doctrine...",
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
        let setups = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.setups.clone();
        let placeholder =
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.placeholder;
        let pick_id = egui::Id::new("fleet_boost_editor_pick");
        let mut picked: i32 = ctx.data(|d| {
            d.get_temp(pick_id).unwrap_or_else(|| setups.first().map(|s| s.id.0).unwrap_or(0))
        });

        egui::Window::new("Boosts per doctrine")
            .open(&mut open)
            .default_size([760.0, 520.0])
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
                        egui::ScrollArea::vertical().id_salt("boost_setups").show(ui, |ui| {
                            for s in &setups {
                                let name = s.name.trim();
                                if !tag_matches(name, &query) {
                                    continue;
                                }
                                let n = rules.iter().filter(|r| r.setup_id == s.id.0).count();
                                let label = if n == 0 {
                                    name.to_owned()
                                } else {
                                    format!("{name}  ({n})")
                                };
                                if ui.selectable_label(picked == s.id.0, label).clicked() {
                                    picked = s.id.0;
                                }
                            }
                        });
                    });
                    ui.separator();

                    ui.vertical(|ui| {
                        let name = setups
                            .iter()
                            .find(|s| s.id.0 == picked)
                            .map(|s| s.name.trim().to_owned())
                            .unwrap_or_else(|| "No doctrine".to_owned());
                        let mine = rules.iter().filter(|r| r.setup_id == picked).count();
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(&name).strong());
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
                        egui::ScrollArea::vertical().id_salt("boost_rules").show(ui, |ui| {
                            egui::Grid::new("boost_rule_rows")
                                .num_columns(3)
                                .striped(true)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    for (i, rule) in rules.iter_mut().enumerate() {
                                        if rule.setup_id != picked {
                                            continue;
                                        }
                                        cell(ui, 220.0, |ui| {
                                            egui::ComboBox::from_id_salt(("boost_charge", i))
                                                .selected_text(rule.charge.clone())
                                                .width(210.0)
                                                .show_ui(ui, |ui| {
                                                    for b in COMBAT_BURSTS {
                                                        changed |= ui
                                                            .selectable_value(
                                                                &mut rule.charge,
                                                                b.label().to_owned(),
                                                                format!(
                                                                    "Any {}",
                                                                    b.label().to_lowercase()
                                                                ),
                                                            )
                                                            .changed();
                                                    }
                                                    ui.separator();
                                                    for (n, _) in CHARGES
                                                        .iter()
                                                        .filter(|(_, b)| COMBAT_BURSTS.contains(b))
                                                    {
                                                        changed |= ui
                                                            .selectable_value(
                                                                &mut rule.charge,
                                                                (*n).to_owned(),
                                                                *n,
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
                                                            .selectable_value(
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
                                charge: boosts::Burst::Shield.label().to_owned(),
                                priority: Priority::Medium.as_str().to_owned(),
                            });
                            changed = true;
                        }
                    });
                });
            });

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
    use crate::theme::standing;
    let colour = match tag.colour_class.as_str() {
        "red" => standing::HOSTILE,
        "green" => standing::FRIENDLY,
        "yellow" => standing::WARNING,
        "blue" => ui.visuals().hyperlink_color,
        _ => ui.visuals().weak_text_color(),
    };
    ui.label(egui::RichText::new(tag.name.trim()).color(colour));
}

/// How long the form waits after the last edit before rendering the ping again.
#[cfg(feature = "fleet")]
const PREVIEW_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

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
    pub save_preset: Option<String>,
    pub auto_channels: bool,
    pub free_channels: bool,
    pub force_channels: bool,
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
            save_preset_button(ui, act);
            let layout = if roomy {
                egui::Layout::right_to_left(egui::Align::Center)
            } else {
                egui::Layout::left_to_right(egui::Align::Center)
            };
            if !roomy {
                ui.end_row();
            }
            ui.with_layout(layout, |ui| {
                ui.label(
                    egui::RichText::new("recorded, not sent")
                        .color(crate::theme::standing::WARNING),
                );
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
                let ready = st.start_request().is_some();
                let track = ui
                    .add_enabled(
                        can_start && ready,
                        egui::Button::new(format!(
                            "{}  Track fleet",
                            egui_phosphor::regular::ROCKET_LAUNCH
                        )),
                    )
                    .on_hover_text("Records the request this would send. Nothing leaves the app.");
                let track = if !can_start {
                    track.on_disabled_hover_text(
                        "Your account does not have the startFleet permission.",
                    )
                } else {
                    track.on_disabled_hover_text("Give the fleet a name first.")
                };
                if track.clicked() {
                    act.start = true;
                }
            });
        });
    });

    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            preset_bar(ui, presets, act);
            ui.separator();
            form_grid(ui, st, act);
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
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets").strong());
        if presets.is_empty() {
            ui.label(egui::RichText::new("none saved").weak());
        }
        for (i, p) in presets.iter().enumerate() {
            if ui
                .button(&p.label)
                .on_hover_text("Fill the form from this preset")
                .clicked()
            {
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

/// An action strip sits against the edge of the view, so it keeps the panel's side margins and
/// trims the vertical ones to the gap the buttons already carry.
#[cfg(feature = "fleet")]
fn bar_frame(ui: &egui::Ui) -> egui::Frame {
    let f = egui::Frame::side_top_panel(ui.style());
    egui::Frame { inner_margin: egui::Margin { top: 4, bottom: 4, ..f.inner_margin }, ..f }
}

/// Naming the current form and keeping it. Lives in the action strip so the form can scroll past
/// it without taking the controls along.
#[cfg(feature = "fleet")]
fn save_preset_button(ui: &mut egui::Ui, act: &mut FormAct) {
    let id = ui.id().with("preset_name");
    let mut label: String = ui.data(|d| d.get_temp(id).unwrap_or_default());
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
            egui::TextEdit::singleline(&mut label).hint_text("Preset name").desired_width(160.0),
        );
        let ready = !label.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Save"))
            .on_disabled_hover_text("Name the preset first.")
            .clicked()
        {
            act.save_preset = Some(label.trim().to_owned());
            label.clear();
            open = false;
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(id, label);
        d.insert_temp(open_id, open);
    });
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
fn form_grid(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    if ui.available_width() >= 2.0 * FORM_COL_W + 24.0 {
        ui.columns(2, |cols| {
            form_identity(&mut cols[0], st, act);
            form_running(&mut cols[1], st, act);
        });
    } else {
        form_identity(ui, st, act);
        ui.add_space(6.0);
        form_running(ui, st, act);
    }
}

/// What the fleet is: its name, what it flies, who it is for, where it forms.
#[cfg(feature = "fleet")]
fn form_identity(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    let seed = st.seed.clone();
    let d = &mut st.draft;
    egui::Grid::new("fleet_form_identity")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Name");
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
                        .selectable_value(&mut d.form.setup_id, 0, "== Choose a setup ==")
                        .changed();
                    for s in &seed.setups {
                        act.edited |= ui
                            .selectable_value(&mut d.form.setup_id, s.id.0, s.name.trim())
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
                        ui.selectable_value(&mut d.form.group_id, None, "== None ==").changed();
                    for s in &seed.sigs {
                        let id = Some(crate::fleets::model::GroupId(s.id as i32));
                        act.edited |=
                            ui.selectable_value(&mut d.form.group_id, id, &s.label).changed();
                    }
                },
            );
            ui.end_row();

            ui.label("Formup");
            let current =
                d.formup.as_ref().map(|l| l.label.clone()).unwrap_or_else(|| "none".to_owned());
            egui::ComboBox::from_id_salt("fleet_formup")
                .width(FIELD_W)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for sys in &seed.systems {
                        if ui.selectable_label(false, &sys.label).clicked() {
                            d.formup = Some(sys.clone());
                            act.edited = true;
                        }
                    }
                });
            ui.end_row();

            ui.label("Doctrine notes");
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
            changed |= ui.selectable_value(slot, None, "none").changed();
            for c in list {
                let text = if c.is_in_use {
                    egui::RichText::new(format!("{}  in use", c.name.trim())).weak()
                } else {
                    egui::RichText::new(c.name.trim().to_owned())
                };
                changed |= ui.selectable_value(slot, Some(c.id), text).changed();
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
                let pool: Vec<_> = tags.iter().filter(|t| t.is_primary == primary).cloned().collect();
                act.edited |= tag_field(ui, salt, &pool, &mut st.draft.tags, primary);
                ui.end_row();
            }
        });
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
                        egui::Button::new(format!(
                            "{}  {}",
                            t.name.trim(),
                            egui_phosphor::regular::X
                        ))
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

    egui::Popup::from_toggle_button_response(&open_button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(260.0)
        .show(|ui| {
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
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                let mut any = false;
                for t in pool {
                    let name = t.name.trim();
                    if !tag_matches(name, &query) {
                        continue;
                    }
                    any = true;
                    let on = selected.contains(&t.id);
                    if ui.selectable_label(on, name).clicked() {
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
    let characters = st.seed.characters.clone();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Snowflakes").strong());
        if ui
            .add_enabled(can, egui::Button::new(format!("{}  Add", egui_phosphor::regular::PLUS)))
            .on_disabled_hover_text(
                "Your account does not have the manageFleetSnowflakes permission.",
            )
            .clicked()
        {
            if let Some(c) = characters.first() {
                st.draft.snowflakes.push(Snowflake {
                    id: 0,
                    character_id: c.id,
                    character_name: c.name.clone(),
                    kind: SnowflakeType::Fc,
                });
                act.edited = true;
            }
        }
    });
    let mut drop: Option<usize> = None;
    for (i, s) in st.draft.snowflakes.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("snowflake", i))
                .width(110.0)
                .selected_text(s.kind.label())
                .show_ui(ui, |ui| {
                    for k in SnowflakeType::ALL {
                        act.edited |= ui.selectable_value(&mut s.kind, k, k.label()).changed();
                    }
                });
            ui.label(&s.character_name);
            if ui.small_button(egui_phosphor::regular::X).clicked() {
                drop = Some(i);
            }
        });
    }
    if let Some(i) = drop {
        st.draft.snowflakes.remove(i);
        act.edited = true;
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

/// The word a severe action has to be typed out with, so it cannot be a stray click.
#[cfg(feature = "fleet")]
fn confirm_word(action: &Action) -> Option<&'static str> {
    match action {
        Action::Close => Some("CLOSE"),
        Action::KickAll => Some("KICK ALL"),
        _ => None,
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

    /// Closing a fleet and emptying it are the two that need typing out; the rest are one dialog.
    #[test]
    fn only_the_worst_actions_ask_for_a_word() {
        assert_eq!(confirm_word(&Action::Close), Some("CLOSE"));
        assert_eq!(confirm_word(&Action::KickAll), Some("KICK ALL"));
        assert_eq!(confirm_word(&Action::KickCapsules), None);
        assert_eq!(confirm_word(&Action::Kick { character_id: 1, exclude: false }), None);

        assert_eq!(danger(&Action::Close), Danger::Severe);
        assert_eq!(danger(&Action::KickAll), Danger::Severe);
        assert_eq!(danger(&Action::KickCapsules), Danger::Caution);
        assert_eq!(danger(&Action::SetMotd), Danger::None);
        assert_eq!(danger(&Action::AddWing), Danger::None);

        // Everything that asks for a word also says what it costs.
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
    act_on: &mut Option<Action>,
    boost_rules: &[crate::settings::FleetBoostRequirement],
    open_editor: &mut bool,
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
    let wanted = crate::fleets::boosts::wanted_for(open.fleet.setup_id.0.into(), boost_rules);
    egui::Panel::top("fleet_header").show_inside(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading(&open.fleet.name);
            if let Some(name) = seed.setup_name(open.fleet.setup_id) {
                ui.label(egui::RichText::new(name).weak());
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
        ui.horizontal_wrapped(|ui| {
            let comms = [
                ("Comms", seed.channel_name(&seed.mumble_channels, open.fleet.mumble_channel_id)),
                ("Logi", seed.channel_name(&seed.logi_channels, open.fleet.logi_channel_id)),
                ("Boost", seed.channel_name(&seed.boost_channels, open.fleet.boost_channel_id)),
            ];
            for (label, name) in comms {
                if let Some(n) = name {
                    ui.label(egui::RichText::new(format!("{label}: {n}")).weak());
                }
            }
            if let Some(f) = &open.fleet.formup_location {
                ui.label(egui::RichText::new(format!("Formup: {}", f.label)).weak());
            }
            for t in open.fleet.tag_ids.iter().filter_map(|t| seed.tag(*t)) {
                fleet_tag_chip(ui, t);
            }
        });
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
        });
        ui.add_space(4.0);
    });

    egui::Panel::bottom("fleet_actions").frame(bar_frame(ui)).show_inside(ui, |ui| {
        action_bar(ui, st, read_only, act_on);
    });

    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        let (can_move, can_kick) = (
            !read_only && st.can(Perm::MoveMember),
            !read_only && st.can(Perm::KickMember),
        );
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match tab {
            DetailTab::Members => members_view(ui, &open, can_move, can_kick, act_on),
            DetailTab::Composition => {
                composition_view(ui, &open, &boosts, &wanted, &off_doctrine, open_editor)
            }
        });
    });
}

/// What is thin about the fleet, worst first. Advice, so nothing here blocks anything.
#[cfg(feature = "fleet")]
fn checks_strip(ui: &mut egui::Ui, checks: &[crate::fleets::checks::Check]) {
    use crate::fleets::checks::Level;
    let loud: Vec<_> = checks.iter().filter(|c| c.level != Level::Fine).collect();
    if loud.is_empty() {
        if !checks.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::CHECK_CIRCLE)
                        .color(crate::theme::standing::FRIENDLY),
                );
                ui.label(egui::RichText::new("Logi, interdiction and tackle all fine.").weak());
            });
            ui.add_space(4.0);
        }
        return;
    }
    for c in loud {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(level_icon(c.level)).color(level_colour(c.level)));
            ui.label(egui::RichText::new(&c.what).strong().color(level_colour(c.level)));
            ui.label(&c.detail);
        });
    }
    ui.add_space(4.0);
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

/// Who is on which boost, read out of the fleet's own boost channel.
#[cfg(feature = "fleet")]
fn boost_strip(
    ui: &mut egui::Ui,
    boosts: &[crate::fleets::boosts::Coverage],
    wanted: &[crate::fleets::boosts::Wanted],
    open_editor: &mut bool,
) {
    use crate::fleets::boosts;
    let gaps = boosts::gaps(wanted, boosts);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Boosts").strong());
        if ui
            .add(
                egui::Button::new(egui_phosphor::regular::SLIDERS_HORIZONTAL).frame(false),
            )
            .on_hover_text("Set which boosts this doctrine wants")
            .clicked()
        {
            *open_editor = true;
        }
        if gaps.is_empty() {
            ui.label(
                egui::RichText::new(if wanted.is_empty() {
                    "nothing set for this doctrine".to_owned()
                } else {
                    format!("all {} the doctrine wants are up", wanted.len())
                })
                .weak(),
            );
        } else {
            ui.label(egui::RichText::new("run next").weak());
            for g in &gaps {
                ui.label(
                    egui::RichText::new(&g.what).color(priority_colour(g.priority)).strong(),
                )
                .on_hover_text(format!("{} priority for this doctrine.", g.priority.label()));
            }
        }
    });
    if boosts.is_empty() {
        ui.label(egui::RichText::new("Nobody has posted in the boost channel yet.").weak());
    } else {
        // Same columns as the composition tables below: name, count, then the detail.
        egui::Grid::new("boost_coverage").num_columns(4).striped(true).spacing([12.0, 3.0]).show(
            ui,
            |ui| {
                for c in boosts {
                    cell(ui, COL[0], |ui| {
                        ui.label(&c.what);
                    });
                    cell(ui, 40.0, |ui| {
                        ui.label(egui::RichText::new(format!("{}", c.pilots)).strong())
                            .on_hover_text("Pilots running this.");
                    });
                    cell(ui, 110.0, |ui| {
                        if c.mindlinked > 0 {
                            ui.label(
                                egui::RichText::new(format!("{} ML", c.mindlinked))
                                    .color(crate::theme::chip::ISK)
                                    .strong(),
                            )
                            .on_hover_text("Of those, how many have a mindlink.");
                        } else {
                            ui.label(egui::RichText::new("no ML").weak());
                        }
                    });
                    cell(ui, COL[1] + COL[2], |ui| {
                        // A pilot who typed "skirm" named the burst and no charge, so repeating it
                        // here would read as two facts instead of one.
                        ui.label(
                            egui::RichText::new(if c.generic {
                                "charge not named"
                            } else {
                                c.burst.label()
                            })
                            .weak(),
                        );
                    });
                    ui.end_row();
                }
            },
        );
    }
    ui.add_space(6.0);
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
    act_on: &mut Option<Action>,
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
                *act_on = Some(action);
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
    act_on: &mut Option<Action>,
) {
    use crate::fleets::doctrine::classify;
    let doctrine = open.doctrine.as_ref();
    if open.composition.wings.is_empty() {
        ui.label(egui::RichText::new("Nobody in the fleet.").weak());
        return;
    }
    for wing in &open.composition.wings {
        let pilots: usize = wing.squads.iter().map(|s| s.members.len()).sum();
        egui::CollapsingHeader::new(format!("{}   {pilots}", wing.name))
            .id_salt(("wing", wing.id.0))
            .default_open(true)
            .show(ui, |ui| {
                for squad in &wing.squads {
                    egui::CollapsingHeader::new(format!(
                        "{}   {}",
                        squad.name,
                        squad.members.len()
                    ))
                    .id_salt(("squad", wing.id.0, squad.id.0))
                    .default_open(true)
                    .show(ui, |ui| {
                        // The whole squad body takes a drop, so a pilot can be dragged onto an
                        // empty squad as well as onto one with rows in it.
                        let (_, dropped) = ui.dnd_drop_zone::<DragPilot, _>(
                            egui::Frame::NONE,
                            |ui| {
                                ui.set_min_size(egui::vec2(ui.available_width(), 16.0));
                                if squad.members.is_empty() {
                                    ui.label(egui::RichText::new("Empty").weak());
                                }
                                for m in &squad.members {
                                    let standing =
                                        classify(m.ship_type_id, &m.ship_group, doctrine);
                                    member_row(
                                        ui, m, standing, wing.id, squad.id, can_move, can_kick,
                                        act_on,
                                    );
                                }
                            },
                        );
                        if let Some(p) = dropped {
                            if can_move && (p.wing != wing.id || p.squad != squad.id) {
                                *act_on = Some(Action::Move {
                                    character_id: p.character_id,
                                    wing: wing.id,
                                    squad: squad.id,
                                });
                            }
                        }
                    });
                }
            });
    }
}

/// A pilot in flight between two squads.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Debug)]
struct DragPilot {
    character_id: i64,
    wing: crate::fleets::model::WingId,
    squad: crate::fleets::model::SquadId,
}

/// Column widths every fleet table shares, so the member list and the composition line up.
#[cfg(feature = "fleet")]
const COL: [f32; 4] = [190.0, 150.0, 120.0, 90.0];

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
fn member_row(
    ui: &mut egui::Ui,
    m: &crate::fleets::model::Member,
    standing: crate::fleets::doctrine::Standing,
    wing: crate::fleets::model::WingId,
    squad: crate::fleets::model::SquadId,
    can_move: bool,
    can_kick: bool,
    act_on: &mut Option<Action>,
) {
    ui.horizontal(|ui| {
        let id = egui::Id::new(("fleet_pilot", m.character_id));
        cell(ui, COL[0], |ui| {
            if can_move {
                let payload = DragPilot { character_id: m.character_id, wing, squad };
                ui.dnd_drag_source(id, payload, |ui| {
                    ui.label(format!("{}  {}", egui_phosphor::regular::DOTS_SIX_VERTICAL, m.name));
                })
                .response
                .on_hover_text("Drag onto another squad to move this pilot.");
            } else {
                ui.label(&m.name);
            }
        });
        cell(ui, COL[1], |ui| {
            ui.label(
                egui::RichText::new(&m.ship_type_name).color(standing_colour(ui, standing)),
            )
            .on_hover_text(standing.label());
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
            *act_on = Some(Action::Kick { character_id: m.character_id, exclude: false });
        }
    });
}

/// What the fleet is flying, and whether the doctrine asked for it.
#[cfg(feature = "fleet")]
fn composition_view(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    boosts: &[crate::fleets::boosts::Coverage],
    wanted: &[crate::fleets::boosts::Wanted],
    off_doctrine: &[crate::fleets::doctrine::OffDoctrine],
    open_editor: &mut bool,
) {
    use crate::fleets::doctrine::{by_category, by_ship, unexpected_pilots, Standing};
    let doctrine = open.doctrine.as_ref();
    let lines = by_ship(&open.composition, doctrine);
    if lines.is_empty() {
        ui.label(egui::RichText::new("Nobody in the fleet.").weak());
        return;
    }
    let total = open.composition.total().max(1);

    checks_strip(ui, &crate::fleets::checks::hulls(&open.composition));
    boost_strip(ui, boosts, wanted, open_editor);

    if let Some(missing) = doctrine.map(|d| d.missing(&open.composition)) {
        if !missing.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Nobody flying").strong());
                ui.label(
                    egui::RichText::new(missing.join(", "))
                        .color(crate::theme::standing::WARNING),
                );
            });
            ui.add_space(4.0);
        }
    }

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
        Standing::Unexpected => crate::theme::standing::HOSTILE,
    }
}

impl SpaiApp {
    /// Asks before anything that cannot be taken back.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_confirm_modal(&mut self, ctx: &egui::Context) {
        let Some((id, action, question, pilots)) = self.fleet_confirm.clone() else { return };
        let word = confirm_word(&action);
        let typed_id = egui::Id::new("fleet_confirm_typed");
        let mut typed: String = ctx.data(|d| d.get_temp(typed_id).unwrap_or_default());
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
            if let Some(w) = word {
                ui.add_space(6.0);
                ui.label(format!("Type {w} to confirm."));
                ui.add(egui::TextEdit::singleline(&mut typed).desired_width(200.0));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    decided = Some(false);
                }
                let armed = word.is_none_or(|w| typed.trim().eq_ignore_ascii_case(w));
                if ui
                    .add_enabled(
                        armed,
                        egui::Button::new(
                            egui::RichText::new("Do it").color(crate::theme::standing::HOSTILE),
                        )
                        .stroke(egui::Stroke::new(1.0, crate::theme::standing::HOSTILE)),
                    )
                    .on_disabled_hover_text("Type the word above first.")
                    .clicked()
                {
                    decided = Some(true);
                }
            });
        });
        ctx.data_mut(|d| d.insert_temp(typed_id, typed));
        match decided {
            Some(true) => {
                self.fleet_confirm = None;
                ctx.data_mut(|d| d.insert_temp(typed_id, String::new()));
                self.fleet_dispatch(Cmd::Act(id, action));
            }
            Some(false) => {
                self.fleet_confirm = None;
                ctx.data_mut(|d| d.insert_temp(typed_id, String::new()));
            }
            None => {}
        }
    }
}
