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
        doctrines(sim, ui, _now_ms);
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

/// The doctrine presets the server lists, by side; a click opens a copy.
fn doctrines(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr!("Doctrines")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Fetch the presets again")).clicked() {
                sim.request(HostRequest::FetchDoctrines);
            }
        });
    });
    ui.separator();
    if sim.doctrines.is_empty() {
        let note = if sim.access.signed_in { tr!("No presets published yet.") } else { tr!("Sign in with an Imperium character to see the coalition's presets.") };
        ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
        return;
    }
    let mut open: Option<String> = None;
    let mut delete: Option<String> = None;
    for side in crate::DoctrineSide::ALL {
        let list: Vec<(String, String, u32)> = sim.doctrines.iter().filter(|d| d.side == side).map(|d| (d.id.clone(), d.name.clone(), d.ship)).collect();
        if list.is_empty() {
            continue;
        }
        ui.label(egui::RichText::new(side.name()).weak());
        for (id, name, ship) in list {
            ui.horizontal(|ui| {
                ui.set_height(26.0);
                crate::icons::type_icon(ui, ship, 20.0);
                let trash = if sim.access.admin { 28.0 } else { 0.0 };
                let w = (ui.available_width() - trash).max(40.0);
                let ship_name = sim.dogma.as_ref().and_then(|d| d.type_(ship)).map_or(String::new(), |t| t.name.clone());
                if crate::editor::name_cell(ui, w, egui::RichText::new(&name), &trf!("{name}\n{ship}", name = name, ship = ship_name)).clicked() {
                    open = Some(id.clone());
                }
                if sim.access.admin && icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete the preset")).clicked() {
                    delete = Some(id.clone());
                }
            });
        }
    }
    if let Some(id) = open {
        sim.open_doctrine(&id, now_ms);
    }
    if let Some(id) = delete {
        sim.doctrines.retain(|d| d.id != id);
        sim.request(HostRequest::DeleteDoctrine(id));
    }
}
