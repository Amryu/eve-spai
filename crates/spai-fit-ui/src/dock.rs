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
const BOTTOM_PAD: f32 = 6.0;

#[derive(Default)]
pub struct DockState {
    /// The leftmost panel shown.
    pub first: usize,
    /// The hold the Cargo tab shows and adds to; none is the cargo hold.
    pub hold: Option<spai_fit::ids::AttrId>,
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
    // As far below the dock's edge as the panels sit from its sides.
    ui.add_space(7.0);
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
        // The cargo tab keeps its hold's fill at its foot, the list scrolling above it.
        let footer = if tab == DockTab::Cargo { FOOTER_H + ui.spacing().item_spacing.y } else { 0.0 };
        let list_h = (ui.available_height() - footer).max(0.0);
        egui::ScrollArea::vertical().id_salt(("sim_dock", tab.name())).auto_shrink([false, false]).max_height(list_h).min_scrolled_height(list_h).show(ui, |ui| match tab {
            DockTab::Drones => drones(sim, ui, dogma, now_ms),
            DockTab::Fighters => fighters(sim, ui, dogma, now_ms),
            DockTab::Implants => worn(sim, ui, dogma, true, now_ms),
            DockTab::Boosters => worn(sim, ui, dogma, false, now_ms),
            DockTab::Cargo => cargo(sim, ui, dogma, now_ms),
            DockTab::Projected => projected(sim, ui, dogma, now_ms),
        });
        if tab == DockTab::Cargo {
            let holds = sim.computed().map(|c| c.holds.clone()).unwrap_or_default();
            hold_footer(sim, ui, &holds);
        }
    });
    browser::accept_drop(sim, &resp.response, DropTarget::Dock(tab), now_ms);
}

/// What acts on the fit from outside: modules and drones pointed at it, and system effects.
/// What the Projected tab can take, by kind: another fit (its remote modules, drones and boosts),
/// one module or drone, or a system effect.
fn add_projected_menu(sim: &mut SimUi, ui: &mut egui::Ui, _now_ms: u64) {
    let mut target: Option<PickTarget> = None;
    let mut fits = false;
    // A small button like the other tabs' Add, opening the menu of kinds.
    egui::containers::menu::MenuButton::from_button(egui::Button::new(trf!("{icon} Add", icon = icon::PLUS)).small()).ui(ui, |ui| {
        ui.set_min_width(240.0);
        if ui.button(trf!("{icon}  Another fit…", icon = icon::ROCKET)).on_hover_text(tr!("Its webs, paints, neuts, reps and drones act on this fit, and its command bursts boost it")).clicked() {
            fits = true;
            ui.close();
        }
        if ui.button(trf!("{icon}  A module or drone…", icon = icon::CROSSHAIR)).clicked() {
            target = Some(PickTarget::Projected);
            ui.close();
        }
        if ui.button(trf!("{icon}  Weather, phenomena or a system effect…", icon = icon::CLOUD)).clicked() {
            target = Some(PickTarget::Environment);
            ui.close();
        }
    });
    if fits {
        sim.project_picker = Some(Default::default());
    }
    if let Some(t) = target {
        picker::open(sim, t);
    }
}

/// The fits projected onto this one: a count of ships, on or off, removable.
fn projected_fits(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = sim.session() else { return };
    let list: Vec<(String, u32, bool, Option<u32>)> = s.fit.projected_fits.iter().map(|p| (p.fit_id.clone(), p.count, p.active, p.variation)).collect();
    for (i, (id, count, active, variation)) in list.into_iter().enumerate() {
        // The source fit's variations, to fly it with one of them.
        let variations: Vec<(u32, String)> = sim.sessions().iter().find(|s| s.id == id).map(|s| &s.fit).or_else(|| sim.fit_docs.get(&id)).map(|f| f.variations.iter().map(|v| (v.id, v.name.clone())).collect()).unwrap_or_default();
        let flying = variation.and_then(|v| variations.iter().find(|(x, _)| *x == v)).map(|(_, n)| n.clone());
        let mut set_variation: Option<Option<u32>> = None;
        let found = sim.sessions().iter().find(|s| s.id == id).map(|s| (s.fit.name.clone(), s.fit.ship)).or_else(|| sim.library.iter().find(|e| e.id == id).map(|e| (e.name.clone(), e.ship)));
        let (name, ship) = found.unwrap_or_else(|| (tr!("A fit no longer saved").to_owned(), 0));
        let mut delta: i32 = 0;
        let mut toggle = false;
        let mut remove = false;
        let r = rows::row(ui, "dock/projected", ("sim_projected_fit", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, ship, 18.0);
            let w = rows::name_w(ui, 5.0);
            let shown = match &flying {
                Some(v) => format!("{name} · {v}"),
                None => name.clone(),
            };
            let label = if active { egui::RichText::new(&shown) } else { egui::RichText::new(&shown).weak() };
            let hull = dogma.type_(ship).map(|t| t.name.clone()).unwrap_or_default();
            rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| {
                ui.label(egui::RichText::new(&name).strong());
                ui.label(egui::RichText::new(hull).weak());
                ui.label(tr!("Its remote modules and drones act on this fit, and its command bursts boost it."));
                if !variations.is_empty() {
                    ui.label(egui::RichText::new(trf!("Flies as: {v}. Right-click to pick another variation.", v = flying.clone().unwrap_or_else(|| tr!("Base").to_owned()))).weak());
                }
            });
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::MINUS)).clicked() {
                delta = -1;
            }
            rows::slot(ui, 1.0, |ui| rows::num_cell(ui, rows::SLOT, count.to_string(), false));
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::PLUS)).clicked() {
                delta = 1;
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, if active { icon::TOGGLE_RIGHT } else { icon::TOGGLE_LEFT })).on_hover_text(if active { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle = true;
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::X)).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        if r.double_clicked() {
            toggle = true;
        }
        r.context_menu(|ui| {
            if !variations.is_empty() {
                ui.menu_button(tr!("Variation"), |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    ui.set_min_width(200.0);
                    if ui.menu_label(variation.is_none() || flying.is_none(), tr!("Base")).clicked() {
                        set_variation = Some(None);
                        ui.close();
                    }
                    for (v, n) in &variations {
                        if ui.menu_label(variation == Some(*v), n).clicked() {
                            set_variation = Some(Some(*v));
                            ui.close();
                        }
                    }
                });
            }
            if ui.button(if active { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle = true;
                ui.close();
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if let Some(v) = set_variation {
            sim.edit(EditKind::Other, now_ms, |f| {
                if let Some(p) = f.projected_fits.get_mut(i) {
                    p.variation = v;
                }
            });
        }
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
            let w = rows::name_w(ui, 5.0);
            let on = if beacon { state != spai_fit::fit::State::Offline } else { state >= spai_fit::fit::State::Active };
            let label = if on { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
            if beacon {
                // One of a kind: no count, the switch where the others have their state.
                for _ in 0..3 {
                    rows::no_slot(ui);
                }
                if rows::slot(ui, 1.0, |ui| icon_button(ui, if on { icon::TOGGLE_RIGHT } else { icon::TOGGLE_LEFT })).on_hover_text(if on { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                    toggle = true;
                }
            } else {
                if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::MINUS)).clicked() {
                    delta = -1;
                }
                rows::slot(ui, 1.0, |ui| rows::num_cell(ui, rows::SLOT, count.to_string(), false));
                if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::PLUS)).clicked() {
                    delta = 1;
                }
                if rows::slot(ui, 1.0, |ui| icon_button(ui, crate::editor::state_icon(state))).on_hover_text(crate::editor::state_name(state)).clicked() {
                    toggle = true;
                }
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::X)).on_hover_text(tr!("Remove")).clicked() {
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
    let tips = sim.tip_values();
    let Some(s) = sim.session() else { return };
    let list: Vec<(TypeId, u32, u32)> = s.fit.drones.iter().map(|d| (d.type_id, d.count, d.in_space())).collect();
    let rolls: Vec<Option<spai_fit::fit::Mutation>> = s.fit.drones.iter().map(|d| d.mutation.clone()).collect();
    if list.is_empty() {
        empty_note(ui, tr!("Drag drones here or double-click them in the browser."));
    }
    for (i, (type_id, count, out)) in list.into_iter().enumerate() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut delta: i32 = 0;
        let mut remove = false;
        let mut launch: Option<u32> = None;
        let mut sim_roll = false;
        let rolled = rolls.get(i).cloned().flatten();
        let r = rows::row(ui, "dock/drones", ("sim_drone", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, crate::mutation::icon_of(dogma, type_id, rolled.as_ref()), 18.0);
            let w = rows::name_w(ui, 5.0);
            let label = egui::RichText::new(&name);
            let label = if rolled.is_some() { label.color(crate::mutation::ROLLED) } else { label };
            let label = if out > 0 { label } else { label.weak() };
            rows::cell(ui, w, label, "").on_hover_ui(|ui| crate::details::type_tip_with(ui, dogma, type_id, "", tips.get(&crate::details::DetailsTarget::Drone(type_id))));
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::MINUS)).on_hover_text(tr!("One fewer in the bay")).clicked() {
                delta = -1;
            }
            rows::slot(ui, 1.0, |ui| rows::num_cell(ui, rows::SLOT, count.to_string(), false));
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::PLUS)).on_hover_text(tr!("One more in the bay")).clicked() {
                delta = 1;
            }
            // How many of the stack are out, read as a share of the bay: "3/5", "0/5" all in the bay.
            let mut n = out;
            let field = rows::slot(ui, 2.0, |ui| {
                ui.spacing_mut().interact_size.y = ROW_H - 8.0;
                ui.spacing_mut().button_padding = egui::vec2(2.0, 0.0);
                ui.add(egui::DragValue::new(&mut n).range(0..=count).custom_formatter(move |v, _| format!("{v:.0}/{count}")).custom_parser(|t| t.split('/').next().and_then(|x| x.trim().parse().ok())))
            });
            if field.on_hover_text(trf!("In space: {n} of {count}. Drag or type how many are launched; 0 keeps them all in the bay.", n = n, count = count)).changed() {
                launch = Some(n);
            }
        });
        // All out, or all back in the bay.
        if r.double_clicked() {
            launch = Some(if out > 0 { 0 } else { count });
        }
        let mut details = false;
        r.context_menu(|ui| {
            if !dogma.mutaplasmids_for(type_id).is_empty() && ui.button(if rolled.is_some() { tr!("Abyssal mutation…") } else { tr!("Roll with a mutaplasmid…") }).clicked() {
                sim_roll = true;
                ui.close();
            }
            if ui.button(if out > 0 { tr!("Keep in the bay") } else { tr!("Launch") }).clicked() {
                launch = Some(if out > 0 { 0 } else { count });
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
        if sim_roll {
            sim.mutation_window = Some(crate::mutation::MutTarget::Drone(i));
        }
        if delta != 0 || remove || launch.is_some() {
            sim.edit(EditKind::Drones, now_ms, |f| {
                if let Some(d) = f.drones.get_mut(i) {
                    if remove {
                        d.count = 0;
                    } else {
                        d.count = (d.count as i32 + delta).max(0) as u32;
                        // None launched is a stack kept in the bay.
                        if let Some(n) = launch {
                            d.active = n > 0;
                            d.launched = (n > 0 && n < d.count).then_some(n);
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
    let tips = sim.tip_values();
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
            let w = rows::name_w(ui, 4.0);
            rows::cell(ui, w, egui::RichText::new(&name), "").on_hover_ui(|ui| crate::details::type_tip_with(ui, dogma, type_id, "", tips.get(&crate::details::DetailsTarget::Fighter(type_id))));
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::MINUS)).clicked() {
                delta = -1;
            }
            rows::slot(ui, 1.0, |ui| rows::num_cell(ui, rows::SLOT, count.to_string(), false));
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::PLUS)).clicked() {
                delta = 1;
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, if active { icon::LIGHTNING } else { icon::CIRCLE })).on_hover_text(if active { tr!("In space") } else { tr!("In the bay") }).clicked() {
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
    let tips = sim.tip_values();
    let Some(s) = sim.session() else { return };
    let mut list: Vec<(TypeId, Option<u8>, bool)> = if implants {
        s.fit.implants.iter().map(|t| (*t, dogma.type_(*t).and_then(|d| d.attr(spai_fit::dogma::attr::IMPLANTNESS)).map(|v| v as u8), false)).collect()
    } else {
        // On when any side effect is counted, picked one by one or all at once.
        s.fit.boosters.iter().map(|b| (b.type_id, dogma.type_(b.type_id).and_then(|d| d.attr(spai_fit::dogma::attr::BOOSTERNESS)).map(|v| v as u8), b.side_on.as_ref().map_or(b.side_effects, |l| !l.is_empty()))).collect()
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
        let has_side = dogma.type_(type_id).is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.usage_chance_attr.is_some())));
        let r = rows::row(ui, if implants { "dock/implants" } else { "dock/boosters" }, ("sim_worn", implants, type_id), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            rows::num_cell(ui, 18.0, slot.map_or(String::new(), |s| s.to_string()), true);
            let w = rows::name_w(ui, if implants { 2.0 } else { 3.0 });
            let label = if on { egui::RichText::new(&name) } else { egui::RichText::new(&name).weak() };
            rows::cell(ui, w, label, "").on_hover_ui(|ui| crate::details::type_tip_with(ui, dogma, type_id, "", tips.get(&if implants { crate::details::DetailsTarget::Implant(type_id) } else { crate::details::DetailsTarget::Booster(type_id) })));
            if !implants {
                // Only boosters that have side effects get the button; the rest keep the column.
                if has_side {
                    let tint = if side_effects { ui.visuals().error_fg_color } else { ui.visuals().weak_text_color() };
                    let hover = if side_effects { tr!("Side effects counted: pick which") } else { tr!("No side effects counted: pick which") };
                    if rows::slot(ui, 1.0, |ui| icon_button(ui, egui::RichText::new(icon::WARNING).color(tint))).on_hover_text(hover).clicked() {
                        toggle_side = true;
                    }
                } else {
                    rows::no_slot(ui);
                }
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, if on { icon::TOGGLE_RIGHT } else { icon::TOGGLE_LEFT })).on_hover_text(if on { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                toggle_on = true;
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::X)).on_hover_text(tr!("Remove")).clicked() {
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
            if !implants && ui.button(tr!("Side effects…")).clicked() {
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
        if toggle_side {
            sim.side_window = Some(type_id);
        }
        if remove {
            sim.edit(if implants { EditKind::Implants } else { EditKind::Boosters }, now_ms, |f| {
                if remove {
                    f.disabled.retain(|t| *t != type_id);
                }
                if implants {
                    f.implants.retain(|t| *t != type_id);
                } else if remove {
                    f.boosters.retain(|b| b.type_id != type_id);
                }
            });
        }
    }
}

/// How full a hold is, as the dropdown and the bar print it: "123 / 400 m³ · 31%".
fn fill_text(h: &spai_fit::stats::Hold) -> String {
    if h.ships {
        return trf!("{used} / {total} ship", used = text::num(h.used), total = text::num(h.capacity));
    }
    let pct = if h.fraction().is_finite() { format!("{:.0}%", (h.fraction() * 100.0).max(0.0)) } else { "—".to_owned() };
    trf!("{used} / {total} m³ · {pct}", used = text::num(h.used.max(0.0)), total = text::num(h.capacity.max(0.0)), pct = pct)
}

/// The hold the tab shows and its fill, on one line at the panel's foot, with a button to switch
/// to another hold when the hull has several.
fn hold_footer(sim: &mut SimUi, ui: &mut egui::Ui, holds: &[spai_fit::stats::Hold]) {
    let Some(shown) = holds.iter().find(|h| h.attr == sim.dock.hold).or_else(|| holds.first()).cloned() else { return };
    sim.dock.hold = shown.attr;
    let over = shown.over();
    let w = ui.available_width();
    ui.allocate_ui_with_layout(egui::vec2(w, FOOTER_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_height(FOOTER_H);
        ui.add_space(rows::GAP);
        let button_w = if holds.len() > 1 { 22.0 + ui.spacing().item_spacing.x } else { 0.0 };
        let bar_w = (ui.available_width() - rows::GAP - button_w).max(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, 20.0), egui::Sense::hover());
        let painter = ui.painter();
        let bar = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 15.0), egui::vec2(rect.width(), 4.0));
        painter.rect_filled(bar, 2.0, ui.visuals().extreme_bg_color);
        let f = shown.fraction().clamp(0.0, 1.0) as f32;
        let colour = if over { ui.visuals().error_fg_color } else { ui.visuals().hyperlink_color };
        painter.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * f, bar.height())), 2.0, colour);
        let font = egui::TextStyle::Body.resolve(ui.style());
        let tc = if over { ui.visuals().error_fg_color } else { ui.visuals().text_color() };
        let galley = painter.layout(format!("{}  {}", shown.name, fill_text(&shown)), font, tc, rect.width());
        painter.with_clip_rect(rect).galley(rect.left_top() + egui::vec2(0.0, -1.0), galley, tc);
        if holds.len() > 1 {
            egui::containers::menu::MenuButton::from_button(egui::Button::new(icon::SWAP).frame_when_inactive(false).min_size(egui::vec2(22.0, 20.0)))
                .ui(ui, |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    ui.set_min_width(260.0);
                    for h in holds {
                        let line = egui::RichText::new(format!("{}  {}", h.name, fill_text(h)));
                        let line = if h.over() { line.color(ui.visuals().error_fg_color) } else { line };
                        if ui.menu_label(h.attr == shown.attr, line).clicked() {
                            sim.dock.hold = h.attr;
                            ui.close();
                        }
                    }
                })
                .0
                .on_hover_text(tr!("Show another hold"));
        }
    });
}

/// A bay carrying other fits, assembled: the frigate escape bay takes one frigate that cannot fit a
/// nullifier, the maintenance bay any ships whose assembled volume fits what is left of it.
fn ship_bay(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, bay: &spai_fit::stats::Hold, now_ms: u64) {
    let Some(bay_attr) = bay.attr else { return };
    let Some(s) = sim.session() else { return };
    let own = s.id.clone();
    let carried: Vec<(usize, spai_fit::fit::Carried)> = s.fit.carried.iter().cloned().enumerate().filter(|(_, c)| c.bay == bay_attr).collect();
    let mut remove: Option<usize> = None;
    let mut add: Option<(String, TypeId)> = None;
    for (i, c) in &carried {
        let name = sim.library.iter().find(|e| e.id == c.fit_id).map_or_else(|| tr!("A fit no longer saved").to_owned(), |e| e.name.clone());
        let hull = spai_fit::formats::name_of(dogma, c.ship);
        rows::row(ui, "dock/cargo", ("sim_bay", *i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, c.ship, 18.0);
            let w = rows::name_w(ui, if bay.ships { 1.0 } else { 3.0 });
            rows::cell(ui, w, egui::RichText::new(&name), &hull);
            if !bay.ships {
                let m3 = dogma.type_(c.ship).map_or(0.0, |t| f64::from(t.volume()));
                rows::slot(ui, 2.0, |ui| rows::num_cell(ui, rows::slots_w(2.0), format!("{} m³", text::num(m3)), true));
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::X)).on_hover_text(tr!("Take it out")).clicked() {
                remove = Some(*i);
            }
        });
    }
    if carried.is_empty() {
        empty_note(ui, if bay.ships { tr!("Room for one frigate, carried as a fit of its own. Frigates that can fit an interdiction nullifier do not go in.") } else { tr!("Ships carried assembled, each a fit of its own, by their assembled volume.") });
    }
    let free = bay.capacity - bay.used;
    let fits_in = |ship: TypeId| {
        if bay.ships {
            bay.used < bay.capacity && spai_fit::stats::fits_escape_bay(dogma, ship)
        } else {
            dogma.type_(ship).is_some_and(|t| f64::from(t.volume()) <= free)
        }
    };
    let choices: Vec<(String, String, TypeId)> = sim.library.iter().filter(|e| e.id != own && fits_in(e.ship)).map(|e| (e.id.clone(), e.name.clone(), e.ship)).collect();
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        ui.menu_button(trf!("{icon} Put a fit in the bay", icon = icon::ROCKET), |ui| {
            ui.set_min_width(260.0);
            if choices.is_empty() {
                ui.label(egui::RichText::new(if bay.ships { tr!("No saved fit goes in: it takes one frigate, and only one.") } else { tr!("No saved fit fits in the room left.") }).weak());
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (id, name, ship) in &choices {
                    let label = format!("{name}  ·  {}", spai_fit::formats::name_of(dogma, *ship));
                    let item = egui::Button::image_and_text(egui::Image::new(crate::icons::type_icon_uri(*ship)).fit_to_exact_size(egui::vec2(16.0, 16.0)), label).frame_when_inactive(false);
                    if ui.add(item).clicked() {
                        add = Some((id.clone(), *ship));
                        ui.close();
                    }
                }
            });
        });
    });
    if let Some(i) = remove {
        sim.edit(EditKind::Cargo, now_ms, move |f| {
            if i < f.carried.len() {
                f.carried.remove(i);
            }
        });
    }
    if let Some((fit_id, ship)) = add {
        sim.edit(EditKind::Cargo, now_ms, move |f| f.carried.push(spai_fit::fit::Carried { fit_id, ship, bay: bay_attr }));
    }
}

/// The cargo panel's foot: the hold's fill and the switch.
const FOOTER_H: f32 = 24.0;

fn cargo(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let holds = sim.computed().map(|c| c.holds.clone()).unwrap_or_default();
    if let Some(first) = holds.iter().find(|h| h.attr == sim.dock.hold).or_else(|| holds.first()) {
        sim.dock.hold = first.attr;
    }
    let hold = sim.dock.hold;
    if let Some(bay) = holds.iter().find(|h| h.attr == hold && h.fits).cloned() {
        ship_bay(sim, ui, dogma, &bay, now_ms);
        return;
    }
    let Some(s) = sim.session() else { return };
    let list: Vec<(usize, TypeId, u32)> = s.fit.cargo.iter().enumerate().filter(|(_, c)| c.hold == hold).map(|(i, c)| (i, c.type_id, c.quantity)).collect();
    if list.is_empty() {
        empty_note(ui, if hold.is_none() { tr!("Anything carried: spare ammo, paste, cap charges, refits. The warnings read it.") } else { tr!("Nothing in this hold yet. Add or drop items while it is shown.") });
    }
    let others: Vec<(Option<spai_fit::ids::AttrId>, String)> = holds.iter().filter(|h| h.attr != hold).map(|h| (h.attr, h.name.clone())).collect();
    for (i, type_id, qty) in list.into_iter() {
        let name = spai_fit::formats::name_of(dogma, type_id);
        let mut remove = false;
        let mut set: Option<u32> = None;
        let r = rows::row(ui, "dock/cargo", ("sim_cargo", i), ROW_H, false, |ui| {
            crate::icons::type_icon(ui, type_id, 18.0);
            let w = rows::name_w(ui, 4.0);
            rows::cell(ui, w, egui::RichText::new(&name), "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, type_id, ""));
            let mut q = qty;
            let r = rows::slot(ui, 3.0, |ui| {
                ui.spacing_mut().interact_size.y = ROW_H - 8.0;
                ui.add_sized([rows::slots_w(3.0), ROW_H - 8.0], egui::DragValue::new(&mut q).range(1..=1_000_000).prefix("x"))
            });
            if r.changed() {
                set = Some(q);
            }
            if rows::slot(ui, 1.0, |ui| icon_button(ui, icon::X)).on_hover_text(tr!("Remove")).clicked() {
                remove = true;
            }
        });
        let mut details = false;
        let mut move_to: Option<Option<spai_fit::ids::AttrId>> = None;
        r.context_menu(|ui| {
            if ui.button(tr!("Details…")).clicked() {
                details = true;
                ui.close();
            }
            if !others.is_empty() {
                ui.menu_button(tr!("Move to"), |ui| {
                    for (a, name) in &others {
                        if ui.button(name).clicked() {
                            move_to = Some(*a);
                            ui.close();
                        }
                    }
                });
            }
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if details {
            sim.open_details(crate::details::DetailsTarget::Type(type_id));
        }
        if let Some(to) = move_to {
            sim.edit(EditKind::Cargo, now_ms, |f| {
                let Some(c) = f.cargo.get(i).cloned() else { return };
                f.cargo.remove(i);
                match f.cargo.iter_mut().find(|x| x.type_id == c.type_id && x.hold == to) {
                    Some(x) => x.quantity += c.quantity,
                    None => f.cargo.push(spai_fit::fit::CargoItem { hold: to, ..c }),
                }
            });
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
}
