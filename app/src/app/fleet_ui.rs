//! The fleet dashboard tab: the fleet list, the start form, boost coverage, and a tracked fleet.

use super::*;

#[cfg(feature = "fleet")]
use crate::fleets::Page;

impl SpaiApp {
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_view(&mut self, ui: &mut egui::Ui) {
        self.fleet_body(ui);
    }

    #[cfg(not(feature = "fleet"))]
    pub(crate) fn fleet_view(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("This build has no fleet dashboard.").weak());
    }

    #[cfg(feature = "fleet")]
    fn fleet_body(&mut self, ui: &mut egui::Ui) {
        // Deferred, like every other view here: the sub-nav sits inside a closure that cannot
        // borrow `self` once the state lock is held.
        let mut goto: Option<Page> = None;
        let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();

        egui::Panel::top("fleet_subnav").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let tab = page.tab();
                for (p, label) in [
                    (Page::Fleets, "Fleets"),
                    (Page::Start, "Start fleet"),
                    (Page::Boosts, "Boosts"),
                ] {
                    if selectable_chip(ui, tab == p, label).clicked() {
                        goto = Some(p);
                    }
                }
                if let Page::Tracking(id) | Page::Historic(id) = &page {
                    ui.label(egui_phosphor::regular::CARET_RIGHT);
                    ui.label(egui::RichText::new(short_fleet_id(id)).strong());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("DRY RUN - nothing is sent")
                            .color(crate::theme::standing::WARNING)
                            .strong(),
                    )
                    .on_hover_text(
                        "Every action records the request it would send and sends none of it.",
                    );
                });
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            ui.add_space(8.0);
            let what = match page {
                Page::Fleets => "The fleet list is not built yet.",
                Page::Start => "The start form is not built yet.",
                Page::Boosts => "Boost coverage is not built yet.",
                Page::Tracking(_) => "The tracking view is not built yet.",
                Page::Historic(_) => "The historic view is not built yet.",
            };
            ui.vertical_centered(|ui| ui.label(egui::RichText::new(what).weak()));
        });

        if let Some(p) = goto {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page = p;
        }
    }
}

impl SpaiApp {
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

/// The tail of a fleet uuid, which is all of it anyone reads.
#[cfg(feature = "fleet")]
fn short_fleet_id(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

#[cfg(all(test, feature = "fleet"))]
mod tests {
    use super::short_fleet_id;

    #[test]
    fn a_fleet_id_shows_its_first_group() {
        assert_eq!(short_fleet_id("439f833e-2aca-44c3-a82d-4bc8a07e489a"), "439f833e");
        assert_eq!(short_fleet_id("nodashes"), "nodashes");
        assert_eq!(short_fleet_id(""), "");
    }
}
