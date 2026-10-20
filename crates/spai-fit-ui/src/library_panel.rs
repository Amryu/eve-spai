//! Saved fits, and the way to a new one.

use egui_phosphor::regular as icon;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::picker::{self, PickTarget};
use crate::{HostRequest, SimUi};

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, _now_ms: u64) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr!("Fits")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(trf!("{icon} New", icon = icon::PLUS)).on_hover_text(tr!("Start a fit from a hull")).clicked() {
                picker::open(sim, PickTarget::Ship);
            }
        });
    });
    ui.separator();
    let current = sim.session.as_ref().map(|s| s.id.clone());
    let dirty = sim.session.as_ref().is_some_and(|s| s.dirty);
    let mut load: Option<String> = None;
    let mut delete: Option<String> = None;
    let entries = sim.library.clone();
    let row_h = 26.0;
    let row = row_h + ui.spacing().item_spacing.y;
    // A whole number of rows, and only the visible ones laid out: a row cut by the edge would
    // sit over the variations below.
    let list_h = ((ui.available_height() * 0.55 / row).floor() * row).max(row * 3.0);
    egui::ScrollArea::vertical().id_salt("sim_library_list").auto_shrink([false, false]).max_height(list_h).show_rows(ui, row_h, entries.len().max(1), |ui, range| {
        if entries.is_empty() {
            ui.label(egui::RichText::new(tr!("No saved fits yet.")).weak());
        }
        for e in entries.iter().skip(range.start).take(range.end - range.start) {
            let selected = current.as_deref() == Some(e.id.as_str());
            ui.horizontal(|ui| {
                ui.set_height(row_h);
                crate::icons::type_icon(ui, e.ship, 20.0);
                let name_w = (ui.available_width() - 28.0).max(40.0);
                let label = if selected && dirty { format!("{}*", e.name) } else { e.name.clone() };
                let label = if selected { egui::RichText::new(label).strong() } else { egui::RichText::new(label) };
                let ship = sim.dogma.as_ref().and_then(|d| d.type_(e.ship)).map_or(String::new(), |t| t.name.clone());
                if crate::editor::name_cell(ui, name_w, label, &trf!("{name}\n{ship}", name = e.name, ship = ship)).clicked() {
                    load = Some(e.id.clone());
                }
                if icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                    delete = Some(e.id.clone());
                }
            });
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().id_salt("sim_variations_list").auto_shrink([false, false]).show(ui, |ui| {
        crate::variations_panel::show(sim, ui, _now_ms);
    });
    if let Some(id) = load {
        if current.as_deref() != Some(id.as_str()) {
            sim.request(HostRequest::LoadFit(id));
        }
    }
    if let Some(id) = delete {
        if current.as_deref() == Some(id.as_str()) {
            sim.session = None;
        }
        sim.library.retain(|e| e.id != id);
        sim.request(HostRequest::DeleteFit(id));
    }
}
