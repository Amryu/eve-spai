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
        let mut goto: Option<Page> = None;
        let mut cmd: Option<Cmd> = None;
        let mut refresh = false;

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
                    if n > 0 {
                        ui.label(egui::RichText::new(format!("{n} recorded")).weak())
                            .on_hover_text("Requests this tab would have sent.");
                    }
                    // The lock is dropped at the end of the closure, before the deferred work runs.
                    let _ = &mut st;
                });
            });
            ui.add_space(4.0);
        });

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
                Page::Start => {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("The start form is not built yet.").weak())
                    });
                }
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
