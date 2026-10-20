//! Saved fits and the doctrine presets, and the way to a new fit.

use egui_phosphor::regular as icon;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::picker::{self, PickTarget};
use crate::rows::{self, ROW_H};
use crate::{HostRequest, SimUi};

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    rows::heading(ui, tr!("Fits"), None, |ui| {
        if ui.small_button(trf!("{icon} New", icon = icon::PLUS)).on_hover_text(tr!("Start a fit from a hull")).clicked() {
            picker::open(sim, PickTarget::Ship);
        }
    });
    let current = sim.session.as_ref().map(|s| s.id.clone());
    let dirty = sim.session.as_ref().is_some_and(|s| s.dirty);
    let mut load: Option<String> = None;
    let mut delete: Option<String> = None;
    let entries = sim.library.clone();
    let row = ROW_H + ui.spacing().item_spacing.y;
    // A whole number of rows, and only the visible ones laid out: a row cut by the edge would
    // sit over the doctrines below.
    let list_h = ((ui.available_height() * 0.55 / row).floor() * row).max(row * 3.0);
    egui::ScrollArea::vertical().id_salt("sim_library_list").auto_shrink([false, false]).max_height(list_h).show_rows(ui, ROW_H, entries.len().max(1), |ui, range| {
        if entries.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(rows::GAP);
                ui.add(egui::Label::new(egui::RichText::new(tr!("No saved fits yet.")).weak()).wrap());
            });
        }
        for e in entries.iter().skip(range.start).take(range.end - range.start) {
            let selected = current.as_deref() == Some(e.id.as_str());
            let ship = sim.dogma.as_ref().and_then(|d| d.type_(e.ship)).map_or(String::new(), |t| t.name.clone());
            let r = rows::row(ui, "fits", ("sim_lib", &e.id), ROW_H, selected, |ui| {
                crate::icons::type_icon(ui, e.ship, 18.0);
                let w = ui.available_width() - 26.0 - rows::GAP;
                let label = if selected && dirty { format!("{}*", e.name) } else { e.name.clone() };
                let label = if selected { egui::RichText::new(label).strong() } else { egui::RichText::new(label) };
                rows::cell(ui, w.max(40.0), label, &trf!("{name}\n{ship}", name = e.name, ship = ship));
                if icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                    delete = Some(e.id.clone());
                }
            });
            if r.clicked() {
                load = Some(e.id.clone());
            }
            r.context_menu(|ui| {
                if ui.button(tr!("Open")).clicked() {
                    load = Some(e.id.clone());
                    ui.close();
                }
                if ui.button(tr!("Delete")).clicked() {
                    delete = Some(e.id.clone());
                    ui.close();
                }
            });
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().id_salt("sim_doctrine_list").auto_shrink([false, false]).show(ui, |ui| {
        doctrines(sim, ui, now_ms);
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
    rows::heading(ui, tr!("Doctrines"), None, |ui| {
        if icon_button(ui, icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Fetch the presets again")).clicked() {
            sim.request(HostRequest::FetchDoctrines);
        }
    });
    if sim.doctrines.is_empty() {
        let note = if sim.access.signed_in { tr!("No presets published yet.") } else { tr!("Sign in with an Imperium character to see the coalition's presets.") };
        ui.horizontal(|ui| {
            ui.add_space(rows::GAP);
            ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
        });
        return;
    }
    let mut open: Option<String> = None;
    let mut delete: Option<String> = None;
    for side in crate::DoctrineSide::ALL {
        let list: Vec<(String, String, u32)> = sim.doctrines.iter().filter(|d| d.side == side).map(|d| (d.id.clone(), d.name.clone(), d.ship)).collect();
        if list.is_empty() {
            continue;
        }
        ui.horizontal(|ui| {
            ui.add_space(rows::GAP);
            ui.label(egui::RichText::new(side.name()).weak());
        });
        for (id, name, ship) in list {
            let ship_name = sim.dogma.as_ref().and_then(|d| d.type_(ship)).map_or(String::new(), |t| t.name.clone());
            let admin = sim.access.admin;
            let r = rows::row(ui, "doctrines", ("sim_doc", &id), ROW_H, false, |ui| {
                crate::icons::type_icon(ui, ship, 18.0);
                let trash = if admin { 26.0 + rows::GAP } else { 0.0 };
                let w = ui.available_width() - trash;
                rows::cell(ui, w.max(40.0), egui::RichText::new(&name), &trf!("{name}\n{ship}", name = name, ship = ship_name));
                if admin && icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete the preset")).clicked() {
                    delete = Some(id.clone());
                }
            });
            if r.clicked() {
                open = Some(id.clone());
            }
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
