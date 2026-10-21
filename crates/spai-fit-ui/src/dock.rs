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
    Projected,
}

impl DockTab {
    pub fn name(self) -> &'static str {
        match self {
            DockTab::Drones => tr!("Drones"),
            DockTab::Fighters => tr!("Fighters"),
            DockTab::Implants => tr!("Implants"),
            DockTab::Boosters => tr!("Boosters"),
            DockTab::Cargo => tr!("Cargo"),
            DockTab::Projected => tr!("Projected"),
        }
    }
}

/// Width one panel wants; the dock shows as many as fit.
pub const PANEL_W: f32 = 300.0;
pub const DOCK_H: f32 = 168.0;
const TAB_W: f32 = 100.0;
const TAB_ROW_H: f32 = 24.0;
/// Room under the panels, so they do not run into the window's edge.
const BOTTOM_PAD: f32 = 8.0;

#[derive(Default)]
pub struct DockState {
    /// The leftmost panel shown.
    pub first: usize,
}

/// The tabs the fit has: fighters only for a hull with a fighter bay or a fit carrying some.
pub fn tabs(sim: &SimUi) -> Vec<DockTab> {
    let fighters = sim.session().is_some_and(|s| {
        !s.fit.fighters.is_empty() || sim.dogma.as_ref().and_then(|d| d.attr_named("fighterCapacity").map(|a| a.id)).and_then(|a| sim.dogma.as_ref()?.type_(s.fit.ship)?.attr(a)).is_some_and(|v| v > 0.0)
    });
    let mut v = vec![DockTab::Drones];
    if fighters {
        v.push(DockTab::Fighters);
    }
    v.extend([DockTab::Implants, DockTab::Boosters, DockTab::Cargo, DockTab::Projected]);
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
    // Each tab put at a rect of its own on one line, so a framed (shown) tab and a bare one
    // cannot sit at different heights; the first starts where the first panel does.
    let (row, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), TAB_ROW_H), egui::Sense::hover());
    {
        use spai_ui::widgets::SteadySelect as _;
        let mut x = row.min.x;
        for (i, t) in tabs.iter().enumerate() {
            let count = match (t, sim.session()) {
                (DockTab::Drones, Some(s)) => s.fit.drones.iter().map(|d| d.count).sum::<u32>() as usize,
                (DockTab::Fighters, Some(s)) => s.fit.fighters.iter().map(|f| f.count).sum::<u32>() as usize,
                (DockTab::Implants, Some(s)) => s.fit.implants.len(),
                (DockTab::Boosters, Some(s)) => s.fit.boosters.len(),
                (DockTab::Cargo, Some(s)) => s.fit.cargo.len(),
                (DockTab::Projected, Some(s)) => s.fit.projected.iter().map(|p| p.count.max(1)).sum::<u32>() as usize + s.fit.projected_fits.iter().map(|p| p.count.max(1)).sum::<u32>() as usize,
                _ => 0,
            };
            let label = if count > 0 { format!("{} ({count})", t.name()) } else { t.name().to_owned() };
            let rect = egui::Rect::from_min_size(egui::pos2(x, row.min.y), egui::vec2(TAB_W, TAB_ROW_H));
            x += TAB_W + rows::GAP;
            if rect.max.x > row.max.x {
                break;
            }
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
            if child.menu_label_sized(rect.size(), shown.contains(t), label).clicked() {
                sim.dock.first = i;
            }
        }
    }
    ui.add_space(4.0);
    let w = (ui.available_width() - (shown.len() as f32 - 1.0) * rows::GAP) / shown.len() as f32;
    // Against what is visible: the panel's rect can run past the window's edge and be clipped.
    let bottom = ui.max_rect().bottom().min(ui.clip_rect().bottom());
    let h = (bottom - ui.cursor().top() - BOTTOM_PAD).max(40.0);
    // Each panel in a rect of its own, so nothing inside can stretch it past the padding.
    let top_left = ui.cursor().min;
    for (k, t) in shown.into_iter().enumerate() {
        let rect = egui::Rect::from_min_size(top_left + egui::vec2(k as f32 * (w + rows::GAP), 0.0), egui::vec2(w, h));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(rect));
            panel(sim, ui, &dogma, t, h, now_ms);
        });
    }
    // The padding is laid out too: a resizable panel takes what its contents fill as its new
    // height, so leaving the strip empty would shrink the dock by that much every frame.
    ui.allocate_space(egui::vec2(ui.available_width(), BOTTOM_PAD.min(ui.available_height().max(0.0))));
}

fn panel(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, tab: DockTab, height: f32, now_ms: u64) {
    let frame = egui::Frame::NONE.fill(ui.visuals().faint_bg_color).corner_radius(4.0).inner_margin(egui::Margin { left: 0, right: 0, top: 2, bottom: 6 });
    // The frame's margins come on top of what is set inside it.
    let inner = (height - 8.0).max(24.0);
    let (resp, _) = ui.dnd_drop_zone::<rows::DragItem, ()>(frame, |ui| {
        ui.set_min_height(inner);
        ui.set_max_height(inner);
        rows::heading(ui, tab.name(), None, |ui| {
            if tab == DockTab::Projected {
                add_projected_menu(sim, ui, now_ms);
            } else if ui.small_button(trf!("{icon} Add", icon = icon::PLUS)).clicked() {
                let target = match tab {
                    DockTab::Drones => PickTarget::Drone,
                    DockTab::Fighters => PickTarget::Fighter,
                    DockTab::Implants => PickTarget::Implant,
                    DockTab::Boosters => PickTarget::Booster,
                    DockTab::Cargo => PickTarget::Cargo,
                    DockTab::Projected => PickTarget::Projected,
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
            DockTab::Projected => projected(sim, ui, dogma, now_ms),
        });
    });
    browser::accept_drop(sim, &resp.response, DropTarget::Dock(tab), now_ms);
}

/// What acts on the fit from outside: modules and drones pointed at it, and system effects.
/// What the Projected tab can take, by kind: another fit (its remote modules, drones and boosts),
/// one module or drone, or a system effect.
fn add_projected_menu(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let current = sim.session().map(|s| s.id.clone()).unwrap_or_default();
    let mut fits: Vec<(String, String)> = sim.sessions().iter().filter(|s| s.id != current && s.fit.ship != 0).map(|s| (s.id.clone(), s.fit.name.clone())).collect();
    for e in &sim.library {
        if e.id != current && !fits.iter().any(|(id, _)| *id == e.id) {
            fits.push((e.id.clone(), e.name.clone()));
        }
    }
    fits.sort_by_key(|(_, n)| n.to_lowercase());
    let mut pick_fit: Option<String> = None;
    let mut target: Option<PickTarget> = None;
    // A small button like the other tabs' Add, opening the menu of kinds.
    egui::containers::menu::MenuButton::from_button(egui::Button::new(trf!("{icon} Add", icon = icon::PLUS)).small()).ui(ui, |ui| {
        ui.set_min_width(240.0);
        ui.menu_button(trf!("{icon}  Another fit", icon = icon::ROCKET), |ui| {
            ui.set_min_width(260.0);
            if fits.is_empty() {
                ui.label(egui::RichText::new(tr!("No other fit: open or save one first.")).weak());
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (id, name) in &fits {
                    if ui.button(text::clip(name, 40)).clicked() {
                        pick_fit = Some(id.clone());
                        ui.close();
                    }
                }
            });
        })
        .response
        .on_hover_text(tr!("Its webs, paints, neuts, reps and drones act on this fit, and its command bursts boost it"));
        if ui.button(trf!("{icon}  A module or drone…", icon = icon::CROSSHAIR)).clicked() {
            target = Some(PickTarget::Projected);
            ui.close();
        }
        if ui.button(trf!("{icon}  Weather, phenomena or a system effect…", icon = icon::CLOUD)).clicked() {
            target = Some(PickTarget::Environment);
            ui.close();
        }
    });
    if let Some(id) = pick_fit {
        sim.edit(EditKind::Other, now_ms, |f| match f.projected_fits.iter_mut().find(|p| p.fit_id == id) {
            Some(p) => p.count += 1,
            None => f.projected_fits.push(spai_fit::fit::ProjectedFit { fit_id: id, ..Default::default() }),
        });
    }
    if let Some(t) = target {
        picker::open(sim, t);
    }
}

/// The fits projected onto this one: a count of ships, on or off, removable.
fn projected_fits(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = sim.session() else { return };
    let list: Vec<(String, u32, bool)> = s.fit.projected_fits.iter().map(|p| (p.fit_id.clone(), p.count, p.active)).collect();
    for (i, (id, count, active)) in list.into_iter().enumerate() {
        let found = sim.sessions().iter().find(|s| s.id == id).map(|s| (s.fit.name.clone(), s.fit.ship)).or_else(|| sim.library.iter().find(|e| e.id == id).map(|e| (e.name.clone(), e.ship)));
        let (name, ship) = found.unwrap_or_else(|| (tr!("A fit no longer saved").to_owned(), 0));
        let mut delta: i32 = 0;
        let mut toggle = false;
        let mut remove = false;
        let r = rows::row(ui, "dock/projected", ("sim_projected_fit", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, ship, 18.0);
            let w = ui.available_width() - 5.0 * 26.0 - 5.0 * rows::GAP;
            let label = if active { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            let hull = dogma.type_(ship).map(|t| t.name.clone()).unwrap_or_default();
            rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| {
                ui.label(egui::RichText::new(&name).strong());
                ui.label(egui::RichText::new(hull).weak());
                ui.label(tr!("Its remote modules and drones act on this fit, and its command bursts boost it."));
            });
            if icon_button(ui, icon::MINUS).clicked() {
                delta = -1;
            }
            rows::num_cell(ui, 22.0, count.to_string(), false);
            if icon_button(ui, icon::PLUS).clicked() {
                delta = 1;
            }
            if icon_button(ui, if active { icon::TOGGLE_RIGHT } else { icon::TOGGLE_LEFT }).on_hover_text(if active { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle = true;
            }
            if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        if r.double_clicked() {
            toggle = true;
        }
        r.context_menu(|ui| {
            if ui.button(if active { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if delta != 0 || toggle || remove {
            sim.edit(EditKind::Other, now_ms, |f| {
                if let Some(p) = f.projected_fits.get_mut(i) {
                    p.count = if remove { 0 } else { (p.count as i32 + delta).max(0) as u32 };
                    if toggle {
                        p.active = !p.active;
                    }
                }
                f.projected_fits.retain(|p| p.count > 0);
            });
        }
    }
}

fn projected(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    projected_fits(sim, ui, dogma, now_ms);
    let Some(s) = sim.session() else { return };
    let list: Vec<(TypeId, u32, spai_fit::fit::State)> = s.fit.projected.iter().map(|p| (p.type_id, p.count, p.state)).collect();
    if list.is_empty() && s.fit.projected_fits.is_empty() {
        empty_note(ui, tr!("Other fits with their webs, reps and boosts, single modules or drones, and wormhole or abyssal weather."));
    }
    for (i, (type_id, count, state)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let beacon = dogma.type_(type_id).is_some_and(|t| crate::browser::environment(dogma, t));
        let mut delta: i32 = 0;
        let mut toggle = false;
        let mut remove = false;
        let r = rows::row(ui, "dock/projected", ("sim_projected", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            // The count between minus and plus is one more cell than the buttons.
            let cells = if beacon { 2.0 } else { 5.0 };
            let w = ui.available_width() - cells * 26.0 - cells * rows::GAP;
            let on = if beacon { state != spai_fit::fit::State::Offline } else { state >= spai_fit::fit::State::Active };
            let label = if on { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
            if beacon {
                if icon_button(ui, if on { icon::TOGGLE_RIGHT } else { icon::TOGGLE_LEFT }).on_hover_text(if on { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                    toggle = true;
                }
            } else {
                if icon_button(ui, icon::MINUS).clicked() {
                    delta = -1;
                }
                rows::num_cell(ui, 22.0, count.to_string(), false);
                if icon_button(ui, icon::PLUS).clicked() {
                    delta = 1;
                }
                if icon_button(ui, crate::editor::state_icon(state)).on_hover_text(crate::editor::state_name(state)).clicked() {
                    toggle = true;
                }
            }
            if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        if r.double_clicked() {
            toggle = true;
        }
        let mut details = false;
        r.context_menu(|ui| {
            let on = if beacon { state != spai_fit::fit::State::Offline } else { state >= spai_fit::fit::State::Active };
            if ui.button(if on { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle = true;
                ui.close();
            }
            if ui.button(tr!("Details…")).clicked() {
                details = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if details {
            sim.open_details(crate::details::DetailsTarget::Type(type_id));
        }
        if delta != 0 || toggle || remove {
            sim.edit(EditKind::Other, now_ms, |f| {
                if let Some(p) = f.projected.get_mut(i) {
                    p.count = if remove { 0 } else { (p.count as i32 + delta).max(0) as u32 };
                    if toggle {
                        p.state = match (beacon, p.state) {
                            (true, spai_fit::fit::State::Offline) => spai_fit::fit::State::Active,
                            (true, _) => spai_fit::fit::State::Offline,
                            (false, st) if st >= spai_fit::fit::State::Active => spai_fit::fit::State::Online,
                            (false, _) => spai_fit::fit::State::Active,
                        };
                    }
                }
                f.projected.retain(|p| p.count > 0);
            });
        }
    }
}

fn empty_note(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
    });
}

fn drones(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = sim.session() else { return };
    let list: Vec<(TypeId, u32, bool, u32)> = s.fit.drones.iter().map(|d| (d.type_id, d.count, d.active, d.in_space())).collect();
    if list.is_empty() {
        empty_note(ui, tr!("Drag drones here or double-click them in the browser."));
    }
    for (i, (type_id, count, active, out)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut delta: i32 = 0;
        let mut toggle = false;
        let mut remove = false;
        let mut launch: Option<u32> = None;
        let r = rows::row(ui, "dock/drones", ("sim_drone", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            const OUT_W: f32 = 58.0;
            let w = ui.available_width() - 4.0 * 26.0 - OUT_W - 5.0 * rows::GAP;
            let label = if active { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
            if icon_button(ui, icon::MINUS).clicked() {
                delta = -1;
            }
            rows::num_cell(ui, 22.0, count.to_string(), false);
            if icon_button(ui, icon::PLUS).clicked() {
                delta = 1;
            }
            // How many of the stack are out; the rest stay in the bay.
            let mut n = if active { out } else { count };
            let field = ui.add_enabled(active, egui::DragValue::new(&mut n).range(1..=count.max(1)).suffix(tr!(" out")));
            if field.on_hover_text(tr!("How many of these are in space")).changed() {
                launch = Some(n);
            }
            if icon_button(ui, if active { icon::LIGHTNING } else { icon::CIRCLE }).on_hover_text(if active { tr!("In space") } else { tr!("In the bay") }).clicked() {
                toggle = true;
            }
        });
        if r.double_clicked() {
            toggle = true;
        }
        let mut details = false;
        r.context_menu(|ui| {
            if ui.button(if active { tr!("Keep in the bay") } else { tr!("Launch") }).clicked() {
                toggle = true;
                ui.close();
            }
            if ui.button(tr!("Details…")).clicked() {
                details = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if details {
            sim.open_details(crate::details::DetailsTarget::Drone(type_id));
        }
        if delta != 0 || toggle || remove || launch.is_some() {
            sim.edit(EditKind::Drones, now_ms, |f| {
                if let Some(d) = f.drones.get_mut(i) {
                    if remove {
                        d.count = 0;
                    } else {
                        d.count = (d.count as i32 + delta).max(0) as u32;
                        if toggle {
                            d.active = !d.active;
                        }
                        if let Some(n) = launch {
                            d.launched = (n < d.count).then_some(n);
                        }
                        // Fewer in the bay than were out: those left are all out.
                        if d.launched.is_some_and(|n| n >= d.count) {
                            d.launched = None;
                        }
                    }
                }
                f.drones.retain(|d| d.count > 0);
            });
        }
    }
}

fn fighters(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = sim.session() else { return };
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
            rows::cell(ui, w.max(40.0), egui::RichText::new(&name), "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
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
        let mut details = false;
        r.context_menu(|ui| {
            if ui.button(tr!("Details…")).clicked() {
                details = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if details {
            sim.open_details(crate::details::DetailsTarget::Fighter(type_id));
        }
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
    let Some(s) = sim.session() else { return };
    let mut list: Vec<(TypeId, Option<u8>, bool)> = if implants {
        s.fit.implants.iter().map(|t| (*t, dogma.type_(*t).and_then(|d| d.attr(spai_fit::dogma::attr::IMPLANTNESS)).map(|v| v as u8), false)).collect()
    } else {
        s.fit.boosters.iter().map(|b| (b.type_id, dogma.type_(b.type_id).and_then(|d| d.attr(spai_fit::dogma::attr::BOOSTERNESS)).map(|v| v as u8), b.side_effects)).collect()
    };
    list.sort_by_key(|(_, slot, _)| slot.unwrap_or(99));
    if list.is_empty() {
        empty_note(ui, if implants { tr!("No implants. Drag some here; one per slot.") } else { tr!("No boosters. Drag some here; one per slot.") });
    }
    let disabled: Vec<TypeId> = sim.session().map(|s| s.fit.disabled.clone()).unwrap_or_default();
    for (type_id, slot, side_effects) in list {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut remove = false;
        let mut toggle_side = false;
        let mut toggle_on = false;
        let on = !disabled.contains(&type_id);
        let r = rows::row(ui, if implants { "dock/implants" } else { "dock/boosters" }, ("sim_worn", implants, type_id), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            rows::num_cell(ui, 18.0, slot.map_or(String::new(), |s| s.to_string()), true);
            let extra = if implants { 52.0 } else { 78.0 };
            let w = ui.available_width() - extra - 3.0 * rows::GAP;
            let label = if on { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
            if icon_button(ui, if on { icon::TOGGLE_RIGHT } else { icon::TOGGLE_LEFT }).on_hover_text(if on { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle_on = true;
            }
            if !implants && icon_button(ui, if side_effects { icon::WARNING } else { icon::WARNING_DIAMOND }).on_hover_text(if side_effects { tr!("Side effects on") } else { tr!("Side effects off") }).clicked() {
                toggle_side = true;
            }
            if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        if r.double_clicked() {
            toggle_on = true;
        }
        let mut details = false;
        r.context_menu(|ui| {
            if ui.button(if on { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle_on = true;
                ui.close();
            }
            if !implants && ui.button(if side_effects { tr!("Side effects off") } else { tr!("Side effects on") }).clicked() {
                toggle_side = true;
                ui.close();
            }
            if ui.button(tr!("Details…")).clicked() {
                details = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if details {
            sim.open_details(if implants { crate::details::DetailsTarget::Implant(type_id) } else { crate::details::DetailsTarget::Booster(type_id) });
        }
        if toggle_on {
            sim.edit(if implants { EditKind::Implants } else { EditKind::Boosters }, now_ms, |f| {
                if f.disabled.contains(&type_id) {
                    f.disabled.retain(|t| *t != type_id);
                } else {
                    f.disabled.push(type_id);
                }
            });
        }
        if remove || toggle_side {
            sim.edit(if implants { EditKind::Implants } else { EditKind::Boosters }, now_ms, |f| {
                if remove {
                    f.disabled.retain(|t| *t != type_id);
                }
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
    let Some(s) = sim.session() else { return };
    let list: Vec<(TypeId, u32)> = s.fit.cargo.iter().map(|c| (c.type_id, c.quantity)).collect();
    if list.is_empty() {
        empty_note(ui, tr!("Anything carried: spare ammo, paste, cap charges, refits. The warnings read it."));
    }
    for (i, (type_id, qty)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut remove = false;
        let mut set: Option<u32> = None;
        let r = rows::row(ui, "dock/cargo", ("sim_cargo", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            let w = ui.available_width() - 70.0 - 26.0 - 2.0 * rows::GAP;
            rows::cell(ui, w.max(40.0), egui::RichText::new(&name), "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
            let mut q = qty;
            if ui.add_sized([70.0, ROW_H - 4.0], egui::DragValue::new(&mut q).range(1..=1_000_000).prefix("x")).changed() {
                set = Some(q);
            }
            if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        let mut details = false;
        r.context_menu(|ui| {
            if ui.button(tr!("Details…")).clicked() {
                details = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if details {
            sim.open_details(crate::details::DetailsTarget::Type(type_id));
        }
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
