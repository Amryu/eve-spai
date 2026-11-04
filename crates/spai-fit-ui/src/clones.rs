//! Implants from a character's clones, as the host reads them: the clone in use and each jump
//! clone, any of which replaces every implant of the fit.

use std::collections::{HashMap, HashSet};

use egui_phosphor::regular as icon;
use spai_fit::history::EditKind;
use spai_fit::ids::{AttrId, TypeId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::rows::{self, GAP};
use crate::{HostRequest, SimUi};

#[derive(Clone, Debug, PartialEq)]
pub struct JumpClone {
    /// Where it sits, or the name the pilot gave it; the active clone goes by being active.
    pub label: String,
    pub active: bool,
    pub implants: Vec<TypeId>,
}

#[derive(Default)]
pub struct ClonesDialog {
    pub character: String,
    pub selected: Option<usize>,
    pub loaded: HashMap<String, Result<Vec<JumpClone>, String>>,
    asked: HashSet<String>,
}

const W: f32 = 480.0;
const ROW_H: f32 = 52.0;
const ICON: f32 = 24.0;

pub fn open(sim: &mut SimUi) {
    let Some(first) = sim.clone_characters.first().cloned() else { return };
    sim.clones_dialog = Some(ClonesDialog { character: first, ..Default::default() });
}

fn slot_of(dogma: &Dogma, t: TypeId) -> Option<u8> {
    dogma.type_(t)?.attr(spai_fit::dogma::attr::IMPLANTNESS).map(|v| v as u8)
}

/// What the fit can wear of a clone, in slot order.
pub fn implants_of(dogma: &Dogma, implants: &[TypeId]) -> Vec<TypeId> {
    let mut v: Vec<(u8, TypeId)> = implants.iter().filter_map(|t| Some((slot_of(dogma, *t)?, *t))).collect();
    v.sort();
    v.into_iter().map(|(_, t)| t).collect()
}

/// The attributes an implant's effects read, with what it has of them: its bonuses, without the
/// volume, slot or skill it needs.
pub fn effect_attrs(dogma: &Dogma, t: TypeId) -> Vec<(AttrId, f64)> {
    let Some(def) = dogma.type_(t) else { return Vec::new() };
    let read: HashSet<AttrId> = def.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).flat_map(|e| e.modifiers.iter().map(|m| m.modifying)).collect();
    def.attrs
        .iter()
        .filter(|(a, v)| *v != 0.0 && read.contains(a) && dogma.attr(*a).is_some_and(|d| !d.display.is_empty()))
        .map(|(a, v)| (*a, f64::from(*v)))
        .collect()
}

impl SimUi {
    /// The clone's implants in place of every implant the fit had, each switched on.
    pub fn use_clone(&mut self, implants: &[TypeId], now_ms: u64) {
        let Some(dogma) = self.dogma.clone() else { return };
        let wanted = implants_of(&dogma, implants);
        self.edit(EditKind::Implants, now_ms, |f| {
            let old = std::mem::replace(&mut f.implants, wanted);
            f.disabled.retain(|t| !old.contains(t) && !f.implants.contains(t));
        });
    }
}

fn count_text(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => tr!("1 implant").to_owned(),
        n => trf!("{n} implants", n = n),
    }
}

fn title(c: &JumpClone) -> String {
    if c.active {
        tr!("Active clone").to_owned()
    } else {
        c.label.clone()
    }
}

pub fn window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(mut d) = sim.clones_dialog.take() else { return };
    if d.asked.insert(d.character.clone()) {
        sim.request(HostRequest::FetchClones(d.character.clone()));
    }
    let characters = sim.clone_characters.clone();
    let mut open = true;
    let mut apply: Option<Vec<TypeId>> = None;
    let mut details: Option<TypeId> = None;
    let mut close = false;
    let size = egui::vec2(W, 420.0);
    egui::Window::new(tr!("Implants from a clone")).id(egui::Id::new("sim_clones")).open(&mut open).collapsible(false).resizable(false).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        ui.set_width(W);
        let shown = d.character.clone();
        crate::combo::combo(ui, "sim_clones_character", ui.available_width(), shown.as_str(), |ui| {
            use spai_ui::widgets::SteadySelect as _;
            ui.set_min_width(W - 20.0);
            for c in &characters {
                if ui.menu_label(*c == shown, c.as_str()).clicked() {
                    d.character = c.clone();
                    d.selected = None;
                    ui.close();
                }
            }
        });
        ui.add_space(6.0);
        let list: &[JumpClone] = match d.loaded.get(&d.character) {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new(tr!("Reading the clones from the ESI…")).weak());
                });
                &[]
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(ui.visuals().error_fg_color));
                &[]
            }
            Some(Ok(list)) => {
                if list.is_empty() {
                    ui.label(egui::RichText::new(tr!("The ESI lists no clones for this character.")).weak());
                }
                list
            }
        };
        let mut picked: Option<usize> = None;
        if !list.is_empty() {
            egui::ScrollArea::vertical().id_salt("sim_clones_list").max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                for (i, c) in list.iter().enumerate() {
                    let implants = implants_of(&dogma, &c.implants);
                    let mut inner_hover = false;
                    let (mut click, mut double) = (false, false);
                    let r = rows::row(ui, "clones", ("sim_clone", i), ROW_H, d.selected == Some(i), |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.horizontal(|ui| {
                                let count = egui::RichText::new(count_text(implants.len())).weak();
                                let count_w = 90.0;
                                let text = egui::RichText::new(title(c));
                                rows::cell(ui, (ui.available_width() - count_w - GAP).max(40.0), if c.active { text.strong() } else { text }, "");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(count).selectable(false));
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                if implants.is_empty() {
                                    ui.add(egui::Label::new(egui::RichText::new(tr!("No implants")).weak()).selectable(false));
                                    return;
                                }
                                let step = ICON + 4.0;
                                let room = ((ui.available_width() + 4.0) / step).floor().max(0.0) as usize;
                                let fit = if room < implants.len() { room.saturating_sub(1) } else { implants.len() };
                                for t in &implants[..fit] {
                                    let resp = implant_icon(ui, &dogma, *t);
                                    inner_hover |= resp.hovered();
                                    click |= resp.clicked();
                                    double |= resp.double_clicked();
                                    resp.context_menu(|ui| {
                                        if ui.button(tr!("Details…")).clicked() {
                                            details = Some(*t);
                                            ui.close();
                                        }
                                    });
                                }
                                if fit < implants.len() {
                                    let rest: Vec<String> = implants[fit..].iter().map(|t| spai_fit::formats::name_of(&dogma, *t)).collect();
                                    let resp = ui.add(egui::Label::new(egui::RichText::new(format!("+{}", rest.len())).weak()).selectable(false)).on_hover_text(rest.join("\n"));
                                    inner_hover |= resp.hovered();
                                }
                            });
                        });
                    });
                    if !inner_hover {
                        r.clone().on_hover_ui(|ui| summary(ui, &dogma, i, c, &implants));
                    }
                    if r.clicked() || click {
                        picked = Some(i);
                    }
                    if r.double_clicked() || double {
                        apply = Some(c.implants.clone());
                    }
                }
            });
        }
        if let Some(i) = picked {
            d.selected = Some(i);
        }
        ui.add_space(6.0);
        let chosen = d.selected.and_then(|i| list.get(i));
        ui.horizontal(|ui| {
            let use_it = ui.add_enabled(chosen.is_some(), egui::Button::new(trf!("{icon}  Use these implants", icon = icon::CHECK)));
            let use_it = use_it.on_hover_text(tr!("Replaces every implant of the fit, emptying the slots the clone has nothing in"));
            if use_it.clicked() {
                apply = chosen.map(|c| c.implants.clone());
            }
            if ui.button(tr!("Cancel")).clicked() {
                close = true;
            }
        });
    });
    if let Some(t) = details {
        sim.open_details(crate::details::DetailsTarget::Type(t));
    }
    if let Some(implants) = apply {
        sim.use_clone(&implants, now_ms);
        return;
    }
    if open && !close {
        sim.clones_dialog = Some(d);
    }
}

/// One implant by its icon, its name on hover and its details on a right click.
fn implant_icon(ui: &mut egui::Ui, dogma: &Dogma, t: TypeId) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ICON, ICON), egui::Sense::click());
    egui::Image::new(crate::icons::type_icon_uri(t)).corner_radius(3.0).paint_at(ui, rect);
    if resp.hovered() || resp.context_menu_opened() {
        ui.painter().rect_stroke(rect.expand(1.0), 3.0, ui.visuals().widgets.hovered.fg_stroke, egui::StrokeKind::Outside);
    }
    resp.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(spai_fit::formats::name_of(dogma, t)).strong());
        if let Some(s) = slot_of(dogma, t) {
            ui.label(egui::RichText::new(trf!("Slot {slot}", slot = s)).weak());
        }
        ui.separator();
        ui.label(egui::RichText::new(tr!("Right-click for details")).weak().italics());
    })
}

/// What a clone's implants do, each under its name: their bonuses in two aligned columns.
fn summary(ui: &mut egui::Ui, dogma: &Dogma, row: usize, c: &JumpClone, implants: &[TypeId]) {
    ui.set_max_width(340.0);
    ui.label(egui::RichText::new(title(c)).strong());
    ui.label(egui::RichText::new(count_text(implants.len())).weak());
    for t in implants {
        ui.add_space(4.0);
        let slot = slot_of(dogma, *t).map_or(String::new(), |s| format!("{s}  "));
        ui.label(format!("{slot}{}", spai_fit::formats::name_of(dogma, *t)));
        let attrs = effect_attrs(dogma, *t);
        if attrs.is_empty() {
            continue;
        }
        egui::Grid::new(("sim_clone_tip", row, *t)).num_columns(2).spacing([16.0, 1.0]).show(ui, |ui| {
            for (a, v) in attrs {
                ui.label(egui::RichText::new(crate::details::label_of(dogma, a)).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(crate::details::value_text(dogma, v, dogma.attr(a).and_then(|d| d.unit))).monospace());
                });
                ui.end_row();
            }
        });
    }
    ui.separator();
    ui.label(egui::RichText::new(tr!("Double-click to use these implants")).weak().italics());
}
