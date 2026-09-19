//! The fleet dashboard tab: the fleet list, the start form, and a tracked fleet.

use super::*;

#[cfg(feature = "fleet")]
use crate::fleets::{
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
            }
            Page::Start => {}
        }
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
        let dry = self.fleet_backend.is_dry_run();
        let seed_hint = crate::fleets::seed_path_hint();
        let presets = self.settings.fleet_presets.clone();
        let journal_open = self.fleet_journal_open;
        let mut toggle_journal = false;
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
                Page::Tracking(_) | Page::Historic(_) => {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("The tracking view is not built yet.").weak())
                    });
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

/// What the start form asked for, applied once the state lock is gone.
#[cfg(feature = "fleet")]
#[derive(Default)]
pub(crate) struct FormAct {
    pub load_preset: Option<usize>,
    pub delete_preset: Option<usize>,
    pub save_preset: Option<String>,
    pub auto_channels: bool,
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
    egui::Panel::bottom("fleet_start_actions").show_inside(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
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
                track.on_disabled_hover_text("Your account does not have the startFleet permission.")
            } else {
                track.on_disabled_hover_text("Give the fleet a name first.")
            };
            if track.clicked() {
                act.start = true;
            }
            if ui
                .add_enabled(
                    can_start,
                    egui::Button::new(format!(
                        "{}  Request ping",
                        egui_phosphor::regular::PAPER_PLANE_TILT
                    )),
                )
                .on_disabled_hover_text("Your account does not have the startFleet permission.")
                .clicked()
            {
                act.ping = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("recorded, not sent")
                        .color(crate::theme::standing::WARNING),
                );
            });
        });
        ui.add_space(4.0);
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
    ui.horizontal(|ui| {
        let id = ui.id().with("preset_name");
        let mut label: String = ui.data(|d| d.get_temp(id).unwrap_or_default());
        ui.add(
            egui::TextEdit::singleline(&mut label)
                .hint_text("Save the form as...")
                .desired_width(180.0),
        );
        let ready = !label.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new(format!("{}  Save", egui_phosphor::regular::COPY)))
            .on_disabled_hover_text("Name the preset first.")
            .clicked()
        {
            act.save_preset = Some(label.trim().to_owned());
            label.clear();
        }
        ui.data_mut(|d| d.insert_temp(id, label));
    });
}

/// The fields themselves.
#[cfg(feature = "fleet")]
fn form_grid(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    let seed = st.seed.clone();
    let auto = st.draft.auto;
    let d = &mut st.draft;
    egui::Grid::new("fleet_start_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name");
        act.edited |= ui
            .add(egui::TextEdit::singleline(&mut d.form.name).desired_width(260.0))
            .changed();
        ui.end_row();

        ui.label("Description");
        act.edited |= ui
            .add(
                egui::TextEdit::multiline(&mut d.form.description)
                    .desired_rows(2)
                    .desired_width(260.0),
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
        egui::ComboBox::from_id_salt("fleet_setup").width(260.0).selected_text(current).show_ui(
            ui,
            |ui| {
                act.edited |= ui.selectable_value(&mut d.form.setup_id, 0, "== Choose a setup ==")
                    .changed();
                for s in &seed.setups {
                    act.edited |=
                        ui.selectable_value(&mut d.form.setup_id, s.id.0, s.name.trim()).changed();
                }
            },
        );
        ui.end_row();

        ui.label("SIG");
        let sig = d
            .form
            .group_id
            .and_then(|g| seed.sigs.iter().find(|s| s.id == g.0 as i64))
            .map(|s| s.label.clone())
            .unwrap_or_else(|| "== None ==".to_owned());
        egui::ComboBox::from_id_salt("fleet_sig").width(260.0).selected_text(sig).show_ui(ui, |ui| {
            act.edited |= ui.selectable_value(&mut d.form.group_id, None, "== None ==").changed();
            for s in &seed.sigs {
                let id = Some(crate::fleets::model::GroupId(s.id as i32));
                act.edited |= ui.selectable_value(&mut d.form.group_id, id, &s.label).changed();
            }
        });
        ui.end_row();

        act.edited |= channel_row(ui, "Mumble", "fleet_mumble", &seed.mumble_channels,
                                  &mut d.form.mumble_channel_id, auto.mumble);
        act.edited |= channel_row(ui, "Logi", "fleet_logi", &seed.logi_channels,
                                  &mut d.form.logi_channel_id, auto.logi);
        act.edited |= channel_row(ui, "Boost", "fleet_boost", &seed.boost_channels,
                                  &mut d.form.boost_channel_id, auto.boost);

        ui.label("");
        if ui
            .button(format!("{}  Pick free comms", egui_phosphor::regular::ARROWS_CLOCKWISE))
            .on_hover_text("Take the lowest free channel for anything already claimed.")
            .clicked()
        {
            act.auto_channels = true;
        }
        ui.end_row();

        ui.label("Auto close");
        ui.horizontal(|ui| {
            let mut kind = d.form.auto_close_type.unwrap_or(1);
            act.edited |= ui.selectable_value(&mut kind, 0, "Start").changed();
            act.edited |= ui.selectable_value(&mut kind, 1, "FC left").changed();
            d.form.auto_close_type = Some(kind);
            let mut mins = d.form.auto_close_time.unwrap_or(30);
            act.edited |= ui
                .add(egui::DragValue::new(&mut mins).range(1..=600).suffix(" min"))
                .changed();
            d.form.auto_close_time = Some(mins);
        });
        ui.end_row();

        ui.label("Formup");
        ui.horizontal(|ui| {
            let current =
                d.formup.as_ref().map(|l| l.label.clone()).unwrap_or_else(|| "none".to_owned());
            egui::ComboBox::from_id_salt("fleet_formup").width(180.0).selected_text(current).show_ui(
                ui,
                |ui| {
                    for sys in &seed.systems {
                        if ui.selectable_label(false, &sys.label).clicked() {
                            d.formup = Some(sys.clone());
                            act.edited = true;
                        }
                    }
                },
            );
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

        ui.label("Doctrine notes");
        let mut notes = d.form.doctrine_notes.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut notes)
                    .hint_text("Optional, shown in the ping")
                    .desired_width(260.0),
            )
            .changed()
        {
            d.form.doctrine_notes = Some(notes).filter(|s| !s.trim().is_empty());
            act.edited = true;
        }
        ui.end_row();
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
            ui.label(egui::RichText::new("auto").weak())
                .on_hover_text("Picked because it was free.");
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
    for (title, primary) in [("Primary tags", true), ("Secondary tags", false)] {
        ui.label(egui::RichText::new(title).strong());
        ui.horizontal_wrapped(|ui| {
            for t in tags.iter().filter(|t| t.is_primary == primary) {
                let on = st.draft.tags.contains(&t.id);
                if ui.selectable_label(on, t.name.trim()).clicked() {
                    if on {
                        st.draft.tags.remove(&t.id);
                    } else {
                        st.draft.tags.insert(t.id);
                    }
                    act.edited = true;
                }
            }
        });
    }
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
