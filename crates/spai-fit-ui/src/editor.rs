//! The fit itself: hull, racks, bays and what the pilot wears.

use egui_phosphor::regular as icon;
use spai_fit::fit::{Slot, State};
use spai_fit::history::EditKind;
use spai_fit::ids::TypeId;
use spai_fit::Dogma;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::picker::{self, PickTarget};
use crate::{text, SimUi};

const ROW_H: f32 = 26.0;

/// A name in a fixed-width cell, left-aligned and cut with an ellipsis, clickable; the whole name
/// shows on hover.
pub fn name_cell(ui: &mut egui::Ui, width: f32, text: egui::RichText, full: &str) -> egui::Response {
    let r = ui
        .allocate_ui_with_layout(egui::vec2(width, ROW_H - 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()))
        })
        .inner;
    r.on_hover_text(full)
}

pub fn state_icon(state: State) -> &'static str {
    match state {
        State::Offline => icon::POWER,
        State::Online => icon::CIRCLE,
        State::Active => icon::LIGHTNING,
        State::Overload => icon::FIRE,
    }
}

pub fn state_name(state: State) -> &'static str {
    match state {
        State::Offline => tr!("Offline"),
        State::Online => tr!("Online"),
        State::Active => tr!("Active"),
        State::Overload => tr!("Overloaded"),
    }
}

/// The next state a click moves a module to, skipping what the module cannot do.
pub fn next_state(dogma: &Dogma, type_id: TypeId, state: State) -> State {
    let can_activate = spai_fit::formats::default_state(dogma, type_id) == State::Active;
    let can_overload = dogma.type_(type_id).is_some_and(|t| t.attr(spai_fit::dogma::attr::HEAT_DAMAGE).is_some());
    match state {
        State::Offline => State::Online,
        State::Online if can_activate => State::Active,
        State::Online => State::Offline,
        State::Active if can_overload => State::Overload,
        State::Active => State::Offline,
        State::Overload => State::Offline,
    }
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    if sim.session.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(tr!("No fit open. Pick a hull to start one, or open a saved fit from the library."));
            ui.add_space(8.0);
            if ui.button(tr!("New fit")).clicked() {
                picker::open(sim, PickTarget::Ship);
            }
        });
        return;
    }
    egui::ScrollArea::vertical().id_salt("sim_editor").auto_shrink([false, false]).show(ui, |ui| {
        header(sim, ui, &dogma, now_ms);
        ui.add_space(6.0);
        let computed = sim.computed();
        let slots = computed.as_ref().map(|c| c.fitting.slots).unwrap_or_default();
        for (i, slot) in Slot::ALL.iter().enumerate() {
            let (used, total) = slots[i];
            if total == 0 && used == 0 {
                continue;
            }
            rack(sim, ui, &dogma, *slot, used, total, now_ms);
        }
        bays(sim, ui, &dogma, now_ms);
    });
}

fn header(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = &mut sim.session else { return };
    let ship = s.fit.ship;
    let ship_name = dogma.type_(ship).map_or_else(|| trf!("Unknown hull {id}", id = ship), |t| t.name.clone());
    let group = dogma.type_(ship).and_then(|t| dogma.group(t.group)).map_or(String::new(), |g| g.name.clone());
    let mut name = s.fit.name.clone();
    let mut pick_ship = false;
    let mut traits_open = false;
    ui.horizontal(|ui| {
        let r = ui.add(egui::Image::new(crate::icons::ship_render_uri(ship)).fit_to_exact_size(egui::vec2(64.0, 64.0)).corner_radius(4.0));
        if r.on_hover_text(tr!("Change hull")).interact(egui::Sense::click()).clicked() {
            pick_ship = true;
        }
        ui.vertical(|ui| {
            let w = (ui.available_width() - 120.0).max(120.0);
            let edit = ui.add(egui::TextEdit::singleline(&mut name).desired_width(w).hint_text(tr!("Fit name")));
            if edit.changed() {
                let now = now_ms;
                sim.edit(EditKind::Rename, now, |f| f.name = name.clone());
            }
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(&ship_name).strong()).truncate());
                ui.add(egui::Label::new(egui::RichText::new(&group).weak()).truncate());
                if dogma.traits_of(ship).is_some() && ui.small_button(tr!("Traits")).clicked() {
                    traits_open = true;
                }
            });
        });
    });
    if pick_ship {
        picker::open(sim, PickTarget::Ship);
    }
    if traits_open {
        ui.ctx().memory_mut(|m| m.data.insert_temp(egui::Id::new("sim_traits_open"), true));
    }
    let mut show_traits = ui.ctx().memory(|m| m.data.get_temp::<bool>(egui::Id::new("sim_traits_open")).unwrap_or(false));
    if show_traits {
        egui::Window::new(trf!("{ship} traits", ship = ship_name)).open(&mut show_traits).collapsible(false).default_width(420.0).show(ui.ctx(), |ui| {
            if let Some(t) = dogma.traits_of(ship) {
                for (skill, bonuses) in &t.per_skill {
                    let skill_name = dogma.type_(*skill).map_or_else(|| format!("Skill {skill}"), |s| s.name.clone());
                    ui.label(egui::RichText::new(trf!("{skill} bonuses (per skill level)", skill = skill_name)).strong());
                    for b in bonuses {
                        ui.label(bonus_line(dogma, b));
                    }
                    ui.add_space(4.0);
                }
                if !t.role.is_empty() {
                    ui.label(egui::RichText::new(tr!("Role bonus")).strong());
                    for b in &t.role {
                        ui.label(bonus_line(dogma, b));
                    }
                }
                if !t.misc.is_empty() {
                    ui.add_space(4.0);
                    for b in &t.misc {
                        ui.label(bonus_line(dogma, b));
                    }
                }
            }
        });
        ui.ctx().memory_mut(|m| m.data.insert_temp(egui::Id::new("sim_traits_open"), show_traits));
    }
}

fn bonus_line(dogma: &Dogma, b: &spai_fit::dogma::Bonus) -> String {
    match b.amount {
        Some(a) => {
            let unit = b.unit.and_then(|u| dogma.unit(u)).map_or("", |u| u.display.as_str());
            format!("{}{unit} {}", text::num(f64::from(a)), b.text)
        }
        None => b.text.clone(),
    }
}

fn rack(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, slot: Slot, used: u32, total: u32, now_ms: u64) {
    let title = match slot {
        Slot::High => tr!("High slots"),
        Slot::Mid => tr!("Mid slots"),
        Slot::Low => tr!("Low slots"),
        Slot::Rig => tr!("Rigs"),
        Slot::Subsystem => tr!("Subsystems"),
        Slot::Service => tr!("Service slots"),
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong());
        let over = used > total;
        let count = trf!("{used} / {total}", used = used, total = total);
        let count = if over { egui::RichText::new(count).color(ui.visuals().error_fg_color) } else { egui::RichText::new(count).weak() };
        ui.label(count);
    });
    let rows = used.max(total);
    let Some(s) = &sim.session else { return };
    let positions: Vec<(u8, Option<(TypeId, State, Option<TypeId>)>)> = (0..rows.min(16) as u8).map(|i| (i, s.fit.module(slot, i).map(|m| (m.type_id, m.state, m.charge)))).collect();
    for (index, module) in positions {
        match module {
            Some((type_id, state, charge)) => module_row(sim, ui, dogma, slot, index, type_id, state, charge, now_ms),
            None => {
                ui.horizontal(|ui| {
                    ui.set_height(ROW_H);
                    ui.add_space(24.0);
                    let r = ui.add(egui::Button::new(egui::RichText::new(trf!("Empty {slot} slot", slot = picker::slot_name(slot))).weak()).frame(false));
                    if r.clicked() {
                        picker::open(sim, PickTarget::Module(slot, index));
                    }
                });
            }
        }
    }
    ui.add_space(4.0);
}

#[allow(clippy::too_many_arguments)]
fn module_row(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, slot: Slot, index: u8, type_id: TypeId, state: State, charge: Option<TypeId>, now_ms: u64) {
    let name = dogma.type_(type_id).map_or_else(|| trf!("Unknown type {id}", id = type_id), |t| t.name.clone());
    let takes_charges = !picker::charge_filter(dogma, type_id).0.is_empty();
    let charge_name = charge.and_then(|c| dogma.type_(c)).map(|t| t.name.clone());
    ui.horizontal(|ui| {
        ui.set_height(ROW_H);
        crate::icons::type_icon(ui, type_id, 20.0);
        let buttons_w = 2.0 * 28.0;
        let charge_w = if takes_charges { 150.0_f32.min((ui.available_width() - buttons_w) * 0.4) } else { 0.0 };
        let name_w = (ui.available_width() - charge_w - buttons_w - 3.0 * ui.spacing().item_spacing.x).max(60.0);
        let label = egui::RichText::new(&name);
        let label = if state == State::Offline { label.weak() } else { label };
        if name_cell(ui, name_w, label, &name).clicked() {
            picker::open(sim, PickTarget::Module(slot, index));
        }
        if takes_charges {
            let shown = charge_name.clone().unwrap_or_else(|| tr!("No charge").to_owned());
            let r = ui.add_sized([charge_w, ROW_H - 4.0], egui::Button::new(egui::RichText::new(text::clip(&shown, 22)).weak()).wrap_mode(egui::TextWrapMode::Truncate));
            if r.on_hover_text(charge_name.unwrap_or_default()).clicked() {
                picker::open(sim, PickTarget::Charge(slot, index));
            }
        }
        let r = icon_button(ui, state_icon(state)).on_hover_text(state_name(state));
        if r.clicked() {
            let next = next_state(dogma, type_id, state);
            sim.edit(EditKind::SetState, now_ms, |f| {
                if let Some(m) = f.module_mut(slot, index) {
                    m.state = next;
                }
            });
        }
        if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
            sim.edit(EditKind::RemoveModule, now_ms, |f| {
                f.remove_module(slot, index);
            });
        }
    });
}

fn bays(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let Some(s) = &sim.session else { return };
    let drones: Vec<(TypeId, u32, bool)> = s.fit.drones.iter().map(|d| (d.type_id, d.count, d.active)).collect();
    let implants: Vec<TypeId> = s.fit.implants.clone();
    let boosters: Vec<TypeId> = s.fit.boosters.iter().map(|b| b.type_id).collect();
    let cargo: Vec<(TypeId, u32)> = s.fit.cargo.iter().map(|c| (c.type_id, c.quantity)).collect();

    section(ui, tr!("Drones"), |ui| {
        if ui.small_button(trf!("{icon} Add", icon = icon::PLUS)).clicked() {
            picker::open(sim, PickTarget::Drone);
        }
    });
    for (i, (type_id, count, active)) in drones.iter().enumerate() {
        let name = dogma.type_(*type_id).map_or_else(|| trf!("Unknown type {id}", id = type_id), |t| t.name.clone());
        ui.horizontal(|ui| {
            ui.set_height(ROW_H);
            crate::icons::type_icon(ui, *type_id, 20.0);
            let name_w = (ui.available_width() - 150.0).max(60.0);
            name_cell(ui, name_w, egui::RichText::new(&name), &name);
            if icon_button(ui, icon::MINUS).clicked() {
                sim.edit(EditKind::Drones, now_ms, |f| {
                    if let Some(d) = f.drones.get_mut(i) {
                        d.count = d.count.saturating_sub(1);
                    }
                    f.drones.retain(|d| d.count > 0);
                });
            }
            ui.add_sized([28.0, ROW_H - 4.0], egui::Label::new(count.to_string()));
            if icon_button(ui, icon::PLUS).clicked() {
                sim.edit(EditKind::Drones, now_ms, |f| {
                    if let Some(d) = f.drones.get_mut(i) {
                        d.count += 1;
                    }
                });
            }
            let r = icon_button(ui, if *active { icon::LIGHTNING } else { icon::CIRCLE }).on_hover_text(if *active { tr!("In space") } else { tr!("In the bay") });
            if r.clicked() {
                sim.edit(EditKind::Drones, now_ms, |f| {
                    if let Some(d) = f.drones.get_mut(i) {
                        d.active = !d.active;
                    }
                });
            }
        });
    }
    section(ui, tr!("Implants"), |ui| {
        if ui.small_button(trf!("{icon} Add", icon = icon::PLUS)).clicked() {
            picker::open(sim, PickTarget::Implant);
        }
    });
    for (i, type_id) in implants.iter().enumerate() {
        simple_row(sim, ui, dogma, *type_id, None, now_ms, move |f| {
            f.implants.remove(i);
        });
    }
    section(ui, tr!("Boosters"), |ui| {
        if ui.small_button(trf!("{icon} Add", icon = icon::PLUS)).clicked() {
            picker::open(sim, PickTarget::Booster);
        }
    });
    for (i, type_id) in boosters.iter().enumerate() {
        simple_row(sim, ui, dogma, *type_id, None, now_ms, move |f| {
            f.boosters.remove(i);
        });
    }
    section(ui, tr!("Cargo"), |ui| {
        if ui.small_button(trf!("{icon} Add", icon = icon::PLUS)).clicked() {
            picker::open(sim, PickTarget::Cargo);
        }
    });
    for (i, (type_id, qty)) in cargo.iter().enumerate() {
        simple_row(sim, ui, dogma, *type_id, Some(*qty), now_ms, move |f| {
            f.cargo.remove(i);
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str, right: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong());
        right(ui);
    });
}

fn simple_row(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, type_id: TypeId, qty: Option<u32>, now_ms: u64, remove: impl FnOnce(&mut spai_fit::fit::Fit)) {
    let name = dogma.type_(type_id).map_or_else(|| trf!("Unknown type {id}", id = type_id), |t| t.name.clone());
    ui.horizontal(|ui| {
        ui.set_height(ROW_H);
        crate::icons::type_icon(ui, type_id, 20.0);
        let name_w = (ui.available_width() - 90.0).max(60.0);
        name_cell(ui, name_w, egui::RichText::new(&name), &name);
        if let Some(q) = qty {
            ui.add_sized([50.0, ROW_H - 4.0], egui::Label::new(egui::RichText::new(trf!("x{n}", n = q)).weak()));
        }
        if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
            sim.edit(EditKind::Cargo, now_ms, remove);
        }
    });
}
