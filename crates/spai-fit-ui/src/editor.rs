//! The fit itself: the hull, the sub-fits, what is wrong with it, and the racks as a table with
//! each module's draw beside it.

use egui_phosphor::regular as icon;
use spai_fit::fit::{Slot, State};
use spai_fit::history::EditKind;
use spai_fit::ids::TypeId;
use spai_fit::variations;
use spai_fit::Dogma;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::browser::{self, DropTarget};
use crate::picker::{self, PickTarget};
use crate::rows::{self, ROW_H};
use crate::{text, SimUi};

/// Fixed columns beside the name: charge, PG, CPU, cap/s, DPS, state, remove.
struct Cols {
    charge: f32,
    num: f32,
    numbers: bool,
}

impl Cols {
    /// The numbers only when the table is wide enough to keep names readable.
    fn for_width(w: f32) -> Self {
        let numbers = w >= 640.0;
        Cols { charge: if w >= 520.0 { 160.0 } else { 120.0 }, num: 58.0, numbers }
    }

    /// The name takes what the fixed cells leave of the row's inner width: icon, charge, the
    /// numbers, the two icon buttons and the gaps between them all.
    fn name_width(&self, inner: f32) -> f32 {
        let fixed = 18.0 + self.charge + if self.numbers { 4.0 * self.num + 4.0 * rows::GAP } else { 0.0 } + 2.0 * 21.0 + 5.0 * rows::GAP;
        (inner - fixed).max(60.0)
    }
}

fn rack_list(slot: Slot) -> &'static str {
    match slot {
        Slot::High => "rack/high",
        Slot::Mid => "rack/mid",
        Slot::Low => "rack/low",
        Slot::Rig => "rack/rig",
        Slot::Subsystem => "rack/subsystem",
        Slot::Service => "rack/service",
    }
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

/// The states a module can take, lowest first.
pub fn states_of(dogma: &Dogma, type_id: TypeId) -> Vec<State> {
    let can_activate = spai_fit::formats::default_state(dogma, type_id) == State::Active;
    let can_overload = dogma.type_(type_id).is_some_and(|t| t.attr(spai_fit::dogma::attr::HEAT_DAMAGE).is_some());
    let mut v = vec![State::Offline, State::Online];
    if can_activate {
        v.push(State::Active);
        if can_overload {
            v.push(State::Overload);
        }
    }
    v
}

/// The next state a click moves a module to, round the ones it can take.
pub fn next_state(dogma: &Dogma, type_id: TypeId, state: State) -> State {
    let states = states_of(dogma, type_id);
    let i = states.iter().position(|s| *s == state).unwrap_or(0);
    states[(i + 1) % states.len()]
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    if sim.session.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(tr!("No fit open."));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr!("Double-click a hull in the browser, open a saved fit, or paste one.")).weak());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let w = 2.0 * 120.0 + ui.spacing().item_spacing.x;
                ui.add_space((ui.available_width() - w) / 2.0);
                if ui.add_sized([120.0, 26.0], egui::Button::new(tr!("New fit"))).clicked() {
                    picker::open(sim, PickTarget::Ship);
                }
                if ui.add_sized([120.0, 26.0], egui::Button::new(tr!("Import EFT"))).clicked() {
                    sim.import = Some(crate::ImportDialog::default());
                }
            });
        });
        return;
    }
    egui::Panel::bottom("sim_dock").resizable(true).default_size(crate::dock::DOCK_H).min_size(90.0).show_inside(ui, |ui| {
        crate::dock::show(sim, ui, now_ms);
    });
    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("sim_editor").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(8.0);
            header(sim, ui, &dogma, now_ms);
            variation_tabs(sim, ui, now_ms);
            crate::warnings_panel::strip(sim, ui);
            ui.add_space(4.0);
            racks(sim, ui, &dogma, now_ms);
        });
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
    let resp = ui
        .horizontal(|ui| {
            ui.add_space(rows::GAP);
            let r = ui.add(egui::Image::new(crate::icons::ship_render_uri(ship)).fit_to_exact_size(egui::vec2(56.0, 56.0)).corner_radius(4.0)).interact(egui::Sense::click());
            if r.on_hover_text(tr!("Change hull: click, or drop a ship here")).clicked() {
                pick_ship = true;
            }
            ui.vertical(|ui| {
                ui.add_space(4.0);
                let w = (ui.available_width() - rows::GAP).max(120.0);
                let edit = ui.add_sized([w, 22.0], egui::TextEdit::singleline(&mut name).hint_text(tr!("Fit name")));
                if edit.changed() {
                    sim.edit(EditKind::Rename, now_ms, |f| f.name = name.clone());
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&ship_name).strong()).truncate());
                    ui.add(egui::Label::new(egui::RichText::new(&group).weak()).truncate());
                    if dogma.traits_of(ship).is_some() && ui.small_button(tr!("Traits")).clicked() {
                        traits_open = true;
                    }
                });
            });
        })
        .response;
    browser::accept_drop(sim, &resp, DropTarget::Hull, now_ms);
    if pick_ship {
        picker::open(sim, PickTarget::Ship);
    }
    if traits_open {
        sim.traits_window = true;
    }
    if sim.traits_window {
        let mut open = true;
        egui::Window::new(trf!("{ship} traits", ship = ship_name)).open(&mut open).collapsible(false).default_width(420.0).show(ui.ctx(), |ui| {
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
        sim.traits_window = open;
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

/// The sub-fits: the base and every variation as a tab, one on at a time (Ctrl-click adds one of
/// another kind); a new one is recorded by editing.
fn variation_tabs(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    use spai_ui::widgets::SteadySelect as _;
    let Some(s) = &sim.session else { return };
    let list: Vec<(u32, String, Option<String>, bool)> = s.fit.variations.iter().map(|v| (v.id, v.name.clone(), v.refit_key.clone(), s.fit.active_variations.contains(&v.id))).collect();
    let none_on = s.fit.active_variations.is_empty();
    let recording = sim.recording.is_some();
    let editing = sim.editing_variation;
    let mut activate: Option<(u32, bool)> = None;
    let mut base = false;
    let mut delete: Option<u32> = None;
    let mut rename: Option<(u32, String)> = None;
    let mut rekey: Option<(u32, String)> = None;
    let mut edit_clicked: Option<Option<u32>> = None;
    let mut record = false;
    let mut keep = false;
    let mut discard = false;
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(rows::GAP);
        ui.add_enabled_ui(!recording, |ui| {
            if ui.menu_label_sized(egui::vec2(64.0, 22.0), none_on && !recording, tr!("Base")).on_hover_text(tr!("The fit as saved, no variation on")).clicked() {
                base = true;
            }
        });
        for (id, name, key, on) in &list {
            if editing == Some(*id) {
                let mut n = name.clone();
                if ui.add(egui::TextEdit::singleline(&mut n).desired_width(140.0).hint_text(tr!("Name"))).changed() {
                    rename = Some((*id, n));
                }
                let mut k = key.clone().unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut k).desired_width(90.0).hint_text(tr!("refit key"))).on_hover_text(tr!("Fits of a doctrine sharing a key refit together, e.g. prop:ab")).changed() {
                    rekey = Some((*id, k));
                }
                if icon_button(ui, icon::CHECK).clicked() {
                    edit_clicked = Some(None);
                }
                continue;
            }
            let label = text::clip(name, 22);
            let w = (label.chars().count() as f32 * 7.5 + 20.0).clamp(64.0, 190.0);
            let r = ui.add_enabled_ui(!recording, |ui| ui.menu_label_sized(egui::vec2(w, 22.0), *on, label)).inner;
            let hover = match key {
                Some(k) => trf!("{name}\nRefit key: {key}\nClick: this one alone. Ctrl-click: with the others.", name = name, key = k),
                None => trf!("{name}\nClick: this one alone. Ctrl-click: with the others.", name = name),
            };
            let r = r.on_hover_text(hover);
            if r.clicked() {
                activate = Some((*id, ui.input(|i| i.modifiers.command)));
            }
            r.context_menu(|ui| {
                if ui.button(tr!("Rename, refit key")).clicked() {
                    edit_clicked = Some(Some(*id));
                    ui.close();
                }
                if ui.button(tr!("Delete")).clicked() {
                    delete = Some(*id);
                    ui.close();
                }
            });
        }
        if recording {
            ui.label(egui::RichText::new(tr!("Recording: change the fit, then")).weak());
            if ui.small_button(trf!("{icon} Keep", icon = icon::CHECK)).on_hover_text(tr!("Keep the changes since the recording started as a variation")).clicked() {
                keep = true;
            }
            if ui.small_button(trf!("{icon} Discard", icon = icon::X)).clicked() {
                discard = true;
            }
        } else if icon_button(ui, icon::PLUS).on_hover_text(tr!("Record a variation: edit the fit, then keep the difference")).clicked() {
            record = true;
        }
    });
    if base {
        sim.edit(EditKind::Variation, now_ms, |f| f.active_variations.clear());
    }
    if let Some((id, with_others)) = activate {
        sim.edit(EditKind::Variation, now_ms, |f| {
            let was_on = f.active_variations.contains(&id);
            if !with_others {
                f.active_variations.clear();
            }
            if was_on && with_others {
                variations::deactivate(f, id);
            } else {
                variations::activate(f, id);
            }
        });
    }
    if let Some(id) = delete {
        sim.edit(EditKind::Variation, now_ms, |f| {
            f.variations.retain(|v| v.id != id);
            f.active_variations.retain(|v| *v != id);
        });
    }
    if let Some((id, n)) = rename {
        sim.edit(EditKind::Rename, now_ms, |f| {
            if let Some(v) = f.variations.iter_mut().find(|v| v.id == id) {
                v.name = n;
            }
        });
    }
    if let Some((id, k)) = rekey {
        sim.edit(EditKind::Rename, now_ms, |f| {
            if let Some(v) = f.variations.iter_mut().find(|v| v.id == id) {
                v.refit_key = Some(k.trim().to_owned()).filter(|k| !k.is_empty());
            }
        });
    }
    if let Some(e) = edit_clicked {
        sim.editing_variation = e;
    }
    if record {
        sim.start_recording(now_ms);
    }
    if keep {
        sim.keep_recording(now_ms);
    }
    if discard {
        sim.discard_recording(now_ms);
    }
}

fn racks(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let computed = sim.computed();
    let slots = computed.as_ref().map(|c| c.fitting.slots).unwrap_or_default();
    let cols = Cols::for_width(ui.available_width());
    // The column heads once, over the first rack.
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = rows::GAP;
        let inner = ui.available_width() - 2.0 * rows::GAP;
        ui.add_space(rows::GAP + 18.0);
        let name_w = cols.name_width(inner);
        rows::head_cell(ui, name_w, tr!("Module"), false);
        rows::head_cell(ui, cols.charge, tr!("Charge"), false);
        if cols.numbers {
            rows::head_cell(ui, cols.num, tr!("PG"), true);
            rows::head_cell(ui, cols.num, tr!("CPU"), true);
            rows::head_cell(ui, cols.num, tr!("Cap/s"), true);
            rows::head_cell(ui, cols.num, tr!("DPS"), true);
        }
    });
    let fixed = sim.variation_positions();
    let Some(view) = sim.view_fit() else { return };
    let stats: Vec<spai_fit::stats::ModuleStat> = sim.session.as_ref().map(|s| s.module_stats.clone()).unwrap_or_default();
    for (i, slot) in Slot::ALL.iter().enumerate() {
        let (used, total) = slots[i];
        if total == 0 && used == 0 {
            continue;
        }
        let title = match slot {
            Slot::High => tr!("High slots"),
            Slot::Mid => tr!("Mid slots"),
            Slot::Low => tr!("Low slots"),
            Slot::Rig => tr!("Rigs"),
            Slot::Subsystem => tr!("Subsystems"),
            Slot::Service => tr!("Service slots"),
        };
        let over = used > total;
        let count = trf!("{used} / {total}", used = used, total = total);
        let count = if over { egui::RichText::new(count).color(ui.visuals().error_fg_color) } else { egui::RichText::new(count).weak() };
        rows::heading(ui, title, Some(count), |_| {});
        let rows_n = used.max(total).min(16) as u8;
        for index in 0..rows_n {
            match view.module(*slot, index).map(|m| (m.type_id, m.state, m.charge)) {
                Some((type_id, state, charge)) => {
                    let stat = stats.iter().find(|m| m.slot == *slot && m.index == index).cloned();
                    module_row(sim, ui, dogma, &cols, *slot, index, type_id, state, charge, stat, fixed.contains(&(*slot, index)), now_ms);
                }
                None => empty_row(sim, ui, *slot, index, now_ms),
            }
        }
        ui.add_space(4.0);
    }
}

fn empty_row(sim: &mut SimUi, ui: &mut egui::Ui, slot: Slot, index: u8, now_ms: u64) {
    let r = rows::row(ui, rack_list(slot), ("sim_empty", slot, index), ROW_H, false, |ui| {
        ui.add_space(22.0);
        let w = ui.available_width();
        rows::cell(ui, w, egui::RichText::new(trf!("Empty {slot} slot", slot = picker::slot_name(slot))).weak(), "");
    });
    if r.double_clicked() {
        picker::open(sim, PickTarget::Module(slot, index));
    }
    r.context_menu(|ui| {
        if ui.button(tr!("Add a module…")).clicked() {
            picker::open(sim, PickTarget::Module(slot, index));
            ui.close();
        }
    });
    browser::accept_drop(sim, &r, DropTarget::Module(slot, index), now_ms);
}

#[allow(clippy::too_many_arguments)]
fn module_row(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, cols: &Cols, slot: Slot, index: u8, type_id: TypeId, state: State, charge: Option<TypeId>, stat: Option<spai_fit::stats::ModuleStat>, by_variation: bool, now_ms: u64) {
    let name = spai_fit::formats::name_of(dogma, type_id);
    let takes_charges = !picker::charge_filter(dogma, type_id).0.is_empty();
    let charge_name = charge.and_then(|c| dogma.type_(c)).map(|t| t.name.clone());
    let mut open_picker: Option<PickTarget> = None;
    let mut set_state: Option<State> = None;
    let mut set_charge: Option<Option<TypeId>> = None;
    let mut replace: Option<TypeId> = None;
    let mut remove = false;
    let r = rows::row(ui, rack_list(slot), ("sim_mod", slot, index), ROW_H, false, |ui| {
        if by_variation {
            ui.disable();
        }
        let inner = ui.available_width();
        crate::icons::type_icon(ui, type_id, 18.0);
        let name_w = cols.name_width(inner);
        let label = egui::RichText::new(&name);
        let label = if state == State::Offline { label.weak() } else { label };
        let hover = if by_variation { trf!("{name}\nSet by a variation", name = name) } else { name.clone() };
        if rows::cell(ui, name_w, label, &hover).interact(egui::Sense::click()).clicked() {
            open_picker = Some(PickTarget::Module(slot, index));
        }
        if takes_charges {
            let shown = charge_name.clone().unwrap_or_else(|| tr!("No charge").to_owned());
            let r = ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 1.0);
                ui.add_sized([cols.charge, ROW_H - 4.0], egui::Button::new(egui::RichText::new(text::clip(&shown, 24)).weak()).wrap_mode(egui::TextWrapMode::Truncate))
            })
            .inner;
            if r.on_hover_text(charge_name.clone().unwrap_or_default()).clicked() {
                open_picker = Some(PickTarget::Charge(slot, index));
            }
        } else {
            ui.add_space(cols.charge);
        }
        if cols.numbers {
            let s = stat.as_ref();
            let off = state == State::Offline;
            rows::num_cell(ui, cols.num, s.map_or(String::new(), |m| text::num(m.power)), off);
            rows::num_cell(ui, cols.num, s.map_or(String::new(), |m| text::num(m.cpu)), off);
            rows::num_cell(ui, cols.num, s.filter(|m| m.cap_per_s != 0.0).map_or(String::new(), |m| text::num(m.cap_per_s)), off);
            rows::num_cell(ui, cols.num, s.filter(|m| m.dps > 0.0).map_or(String::new(), |m| text::num(m.dps)), off);
        }
        if icon_button(ui, state_icon(state)).on_hover_text(state_name(state)).clicked() {
            set_state = Some(next_state(dogma, type_id, state));
        }
        if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
            remove = true;
        }
    });
    if !by_variation {
        if r.double_clicked() {
            set_state = Some(next_state(dogma, type_id, state));
        }
        r.context_menu(|ui| {
            ui.set_min_width(200.0);
            for st in states_of(dogma, type_id) {
                if ui.radio(st == state, state_name(st)).clicked() {
                    set_state = Some(st);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr!("Change module…")).clicked() {
                open_picker = Some(PickTarget::Module(slot, index));
                ui.close();
            }
            let variants: Vec<TypeId> = dogma.variants_of(type_id).iter().copied().filter(|v| *v != type_id && dogma.type_(*v).is_some_and(|t| t.published)).collect();
            if !variants.is_empty() {
                ui.menu_button(tr!("Variants"), |ui| {
                    ui.set_min_width(260.0);
                    for v in variants {
                        if ui.button(spai_fit::formats::name_of(dogma, v)).clicked() {
                            replace = Some(v);
                            ui.close();
                        }
                    }
                });
            }
            if takes_charges {
                ui.menu_button(tr!("Charge"), |ui| {
                    ui.set_min_width(260.0);
                    if ui.radio(charge.is_none(), tr!("None")).clicked() {
                        set_charge = Some(None);
                        ui.close();
                    }
                    let list = picker::compatible_charges(dogma, type_id);
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for c in list {
                            if ui.radio(charge == Some(c), spai_fit::formats::name_of(dogma, c)).clicked() {
                                set_charge = Some(Some(c));
                                ui.close();
                            }
                        }
                    });
                });
            }
            ui.separator();
            if ui.button(tr!("Remove")).clicked() {
                remove = true;
                ui.close();
            }
        });
        browser::accept_drop(sim, &r, DropTarget::Module(slot, index), now_ms);
    }
    if let Some(t) = open_picker {
        picker::open(sim, t);
    }
    if let Some(st) = set_state {
        sim.edit(EditKind::SetState, now_ms, |f| {
            if let Some(m) = f.module_mut(slot, index) {
                m.state = st;
            }
        });
    }
    if let Some(c) = set_charge {
        sim.edit(EditKind::SetCharge, now_ms, |f| {
            if let Some(m) = f.module_mut(slot, index) {
                m.charge = c;
            }
        });
    }
    if let Some(v) = replace {
        sim.edit(EditKind::AddModule, now_ms, |f| {
            if let Some(m) = f.module_mut(slot, index) {
                m.type_id = v;
            }
        });
    }
    if remove {
        sim.edit(EditKind::RemoveModule, now_ms, |f| {
            f.remove_module(slot, index);
        });
    }
}
