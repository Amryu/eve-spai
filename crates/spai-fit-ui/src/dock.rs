//! The bays and what the pilot wears, docked under the racks: drones, fighters, implants,
//! boosters and cargo as tabs, as many side by side as the width allows.

use egui_phosphor::regular as icon;
use spai_fit::history::EditKind;
use spai_fit::ids::TypeId;
use spai_fit::Dogma;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::browser::{self, DropTarget};
use crate::picker::{self, PickTarget};
use crate::rows::{self, ROW_H};
use crate::{text, SimUi};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockTab {
    Drones,
    Fighters,
    Implants,
    Boosters,
    Cargo,
}

impl DockTab {
    pub fn name(self) -> &'static str {
        match self {
            DockTab::Drones => tr!("Drones"),
            DockTab::Fighters => tr!("Fighters"),
            DockTab::Implants => tr!("Implants"),
            DockTab::Boosters => tr!("Boosters"),
            DockTab::Cargo => tr!("Cargo"),
        }
    }
}

/// Width one panel wants; the dock shows as many as fit.
pub const PANEL_W: f32 = 300.0;
pub const DOCK_H: f32 = 168.0;

#[derive(Default)]
pub struct DockState {
    /// The leftmost panel shown.
    pub first: usize,
}

/// The tabs the fit has: fighters only for a hull with a fighter bay or a fit carrying some.
pub fn tabs(sim: &SimUi) -> Vec<DockTab> {
    let fighters = sim.session.as_ref().is_some_and(|s| {
        !s.fit.fighters.is_empty() || sim.dogma.as_ref().and_then(|d| d.attr_named("fighterCapacity").map(|a| a.id)).and_then(|a| sim.dogma.as_ref()?.type_(s.fit.ship)?.attr(a)).is_some_and(|v| v > 0.0)
    });
    let mut v = vec![DockTab::Drones];
    if fighters {
        v.push(DockTab::Fighters);
    }
    v.extend([DockTab::Implants, DockTab::Boosters, DockTab::Cargo]);
    v
}

/// How many panels fit side by side in `width`, and which: the chosen one and those after it.
pub fn visible(width: f32, tabs: &[DockTab], first: usize) -> Vec<DockTab> {
    let n = ((width / PANEL_W).floor() as usize).clamp(1, tabs.len().max(1));
    let first = first.min(tabs.len().saturating_sub(n));
    tabs.iter().skip(first).take(n).copied().collect()
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let tabs = tabs(sim);
    let shown = visible(ui.available_width(), &tabs, sim.dock.first);
    ui.horizontal(|ui| {
        use spai_ui::widgets::SteadySelect as _;
        ui.add_space(rows::GAP);
        for (i, t) in tabs.iter().enumerate() {
            let count = match (t, sim.session.as_ref()) {
                (DockTab::Drones, Some(s)) => s.fit.drones.iter().map(|d| d.count).sum::<u32>() as usize,
                (DockTab::Fighters, Some(s)) => s.fit.fighters.iter().map(|f| f.count).sum::<u32>() as usize,
                (DockTab::Implants, Some(s)) => s.fit.implants.len(),
                (DockTab::Boosters, Some(s)) => s.fit.boosters.len(),
                (DockTab::Cargo, Some(s)) => s.fit.cargo.len(),
                _ => 0,
            };
            let label = if count > 0 { format!("{} ({count})", t.name()) } else { t.name().to_owned() };
            if ui.menu_label_sized(egui::vec2(92.0, 22.0), shown.contains(t), label).clicked() {
                sim.dock.first = i;
            }
        }
    });
    ui.add_space(2.0);
    let w = (ui.available_width() - (shown.len() as f32 - 1.0) * rows::GAP) / shown.len() as f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = rows::GAP;
        for t in shown {
            ui.allocate_ui_with_layout(egui::vec2(w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(w);
                panel(sim, ui, &dogma, t, now_ms);
            });
        }
    });
}

fn panel(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, tab: DockTab, now_ms: u64) {
    let frame = egui::Frame::NONE.fill(ui.visuals().faint_bg_color).corner_radius(4.0).inner_margin(egui::Margin::symmetric(0, 2));
    let (resp, _) = ui.dnd_drop_zone::<rows::DragItem, ()>(frame, |ui| {
        ui.set_min_height(ui.available_height());
        rows::heading(ui, tab.name(), None, |ui| {
            if ui.small_button(trf!("{icon} Add", icon = icon::PLUS)).clicked() {
                let target = match tab {
                    DockTab::Drones => PickTarget::Drone,
                    DockTab::Fighters => PickTarget::Fighter,
                    DockTab::Implants => PickTarget::Implant,
                    DockTab::Boosters => PickTarget::Booster,
                    DockTab::Cargo => PickTarget::Cargo,
                };
                picker::open(sim, target);
            }
        });
        egui::ScrollArea::vertical().id_salt(("sim_dock", tab.name())).auto_shrink([false, false]).show(ui, |ui| match tab {
            DockTab::Drones => drones(sim, ui, dogma, now_ms),
            DockTab::Fighters => fighters(sim, ui, dogma, now_ms),
            DockTab::Implants => worn(sim, ui, dogma, true, now_ms),
            DockTab::Boosters => worn(sim, ui, dogma, false, now_ms),
            DockTab::Cargo => cargo(sim, ui, dogma, now_ms),
        });
    });
    browser::accept_drop(sim, &resp.response, DropTarget::Dock(tab), now_ms);
}

fn empty_note(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
    });
}

fn drones(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = &sim.session else { return };
    let list: Vec<(TypeId, u32, bool)> = s.fit.drones.iter().map(|d| (d.type_id, d.count, d.active)).collect();
    if list.is_empty() {
        empty_note(ui, tr!("Drag drones here or double-click them in the browser."));
    }
    for (i, (type_id, count, active)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut delta: i32 = 0;
        let mut toggle = false;
        let mut remove = false;
        let r = rows::row(ui, "dock/drones", ("sim_drone", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            let w = ui.available_width() - 4.0 * 26.0 - 4.0 * rows::GAP;
            let label = if active { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            rows::cell(ui, w.max(40.0), label, &name);
            if icon_button(ui, icon::MINUS).clicked() {
                delta = -1;
            }
            rows::num_cell(ui, 22.0, count.to_string(), false);
            if icon_button(ui, icon::PLUS).clicked() {
                delta = 1;
            }
            if icon_button(ui, if active { icon::LIGHTNING } else { icon::CIRCLE }).on_hover_text(if active { tr!("In space") } else { tr!("In the bay") }).clicked() {
                toggle = true;
            }
        });
        if r.double_clicked() {
            toggle = true;
        }
        r.context_menu(|ui| {
            if ui.button(if active { tr!("Keep in the bay") } else { tr!("Launch") }).clicked() {
                toggle = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if delta != 0 || toggle || remove {
            sim.edit(EditKind::Drones, now_ms, |f| {
                if let Some(d) = f.drones.get_mut(i) {
                    if remove {
                        d.count = 0;
                    } else {
                        d.count = (d.count as i32 + delta).max(0) as u32;
                        if toggle {
                            d.active = !d.active;
                        }
                    }
                }
                f.drones.retain(|d| d.count > 0);
            });
        }
    }
}

fn fighters(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = &sim.session else { return };
    let list: Vec<(TypeId, u32, bool)> = s.fit.fighters.iter().map(|d| (d.type_id, d.count, d.active)).collect();
    if list.is_empty() {
        empty_note(ui, tr!("Drag fighter squadrons here."));
    }
    for (i, (type_id, count, active)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut delta: i32 = 0;
        let mut toggle = false;
        let mut remove = false;
        let r = rows::row(ui, "dock/fighters", ("sim_fighter", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            let w = ui.available_width() - 4.0 * 26.0 - 4.0 * rows::GAP;
            rows::cell(ui, w.max(40.0), egui::RichText::new(&name), &name);
            if icon_button(ui, icon::MINUS).clicked() {
                delta = -1;
            }
            rows::num_cell(ui, 22.0, count.to_string(), false);
            if icon_button(ui, icon::PLUS).clicked() {
                delta = 1;
            }
            if icon_button(ui, if active { icon::LIGHTNING } else { icon::CIRCLE }).clicked() {
                toggle = true;
            }
        });
        r.context_menu(|ui| {
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if delta != 0 || toggle || remove {
            sim.edit(EditKind::Drones, now_ms, |f| {
                if let Some(d) = f.fighters.get_mut(i) {
                    d.count = if remove { 0 } else { (d.count as i32 + delta).max(0) as u32 };
                    if toggle {
                        d.active = !d.active;
                    }
                }
                f.fighters.retain(|d| d.count > 0);
            });
        }
    }
}

/// Implants or boosters, each with the slot it takes.
fn worn(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, implants: bool, now_ms: u64) {
    let Some(s) = &sim.session else { return };
    let mut list: Vec<(TypeId, Option<u8>, bool)> = if implants {
        s.fit.implants.iter().map(|t| (*t, dogma.type_(*t).and_then(|d| d.attr(spai_fit::dogma::attr::IMPLANTNESS)).map(|v| v as u8), false)).collect()
    } else {
        s.fit.boosters.iter().map(|b| (b.type_id, dogma.type_(b.type_id).and_then(|d| d.attr(spai_fit::dogma::attr::BOOSTERNESS)).map(|v| v as u8), b.side_effects)).collect()
    };
    list.sort_by_key(|(_, slot, _)| slot.unwrap_or(99));
    if list.is_empty() {
        empty_note(ui, if implants { tr!("No implants. Drag some here; one per slot.") } else { tr!("No boosters. Drag some here; one per slot.") });
    }
    for (type_id, slot, side_effects) in list {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut remove = false;
        let mut toggle_side = false;
        let r = rows::row(ui, if implants { "dock/implants" } else { "dock/boosters" }, ("sim_worn", implants, type_id), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            rows::num_cell(ui, 18.0, slot.map_or(String::new(), |s| s.to_string()), true);
            let extra = if implants { 26.0 } else { 52.0 };
            let w = ui.available_width() - extra - 2.0 * rows::GAP;
            rows::cell(ui, w.max(40.0), egui::RichText::new(&name), &name);
            if !implants && icon_button(ui, if side_effects { icon::WARNING } else { icon::WARNING_DIAMOND }).on_hover_text(if side_effects { tr!("Side effects on") } else { tr!("Side effects off") }).clicked() {
                toggle_side = true;
            }
            if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        r.context_menu(|ui| {
            if !implants && ui.button(if side_effects { tr!("Side effects off") } else { tr!("Side effects on") }).clicked() {
                toggle_side = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if remove || toggle_side {
            sim.edit(if implants { EditKind::Implants } else { EditKind::Boosters }, now_ms, |f| {
                if implants {
                    f.implants.retain(|t| *t != type_id);
                } else if remove {
                    f.boosters.retain(|b| b.type_id != type_id);
                } else if let Some(b) = f.boosters.iter_mut().find(|b| b.type_id == type_id) {
                    b.side_effects = !b.side_effects;
                }
            });
        }
    }
}

fn cargo(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = &sim.session else { return };
    let list: Vec<(TypeId, u32)> = s.fit.cargo.iter().map(|c| (c.type_id, c.quantity)).collect();
    if list.is_empty() {
        empty_note(ui, tr!("Spare ammo, paste and cap charges go here; the warnings read it."));
    }
    for (i, (type_id, qty)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut remove = false;
        let mut set: Option<u32> = None;
        let r = rows::row(ui, "dock/cargo", ("sim_cargo", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            let w = ui.available_width() - 70.0 - 26.0 - 2.0 * rows::GAP;
            rows::cell(ui, w.max(40.0), egui::RichText::new(&name), &name);
            let mut q = qty;
            if ui.add_sized([70.0, ROW_H - 4.0], egui::DragValue::new(&mut q).range(1..=1_000_000).prefix("x")).changed() {
                set = Some(q);
            }
            if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        r.context_menu(|ui| {
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if remove || set.is_some() {
            sim.edit(EditKind::Cargo, now_ms, |f| {
                if remove {
                    f.cargo.remove(i);
                } else if let (Some(c), Some(q)) = (f.cargo.get_mut(i), set) {
                    c.quantity = q;
                }
            });
        }
    }
    let _ = text::num;
}
