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
use crate::details::{self, DetailsTarget};
use crate::picker::{self, PickTarget};
use crate::rows::{self, ROW_H};
use crate::{text, SimUi};

/// Fixed columns beside the name: charge, PG, CPU, cap/s, DPS, cycle, effect, state, remove.
struct Cols {
    charge: f32,
    num: f32,
    numbers: bool,
    /// Cycle time and each module's own figure, once there is room for them as well.
    extra: bool,
    info: f32,
    /// Price and time to burnout, wider still.
    more: bool,
}

impl Cols {
    /// The numbers only when the table is wide enough to keep names readable.
    fn for_width(w: f32) -> Self {
        let numbers = w >= 640.0;
        Cols { charge: if w >= 520.0 { 160.0 } else { 120.0 }, num: 58.0, numbers, extra: w >= 840.0, info: 90.0, more: w >= 1000.0 }
    }

    /// The name takes what the fixed cells leave of the row's inner width: icon, charge, the
    /// numbers, the two icon buttons and the gaps between them all.
    fn name_width(&self, inner: f32) -> f32 {
        let fixed = STATE_W + 18.0 + self.charge + if self.numbers { 4.0 * self.num + 4.0 * rows::GAP } else { 0.0 } + if self.extra { self.num + self.info + 2.0 * rows::GAP } else { 0.0 } + if self.more { 2.0 * self.num + 2.0 * rows::GAP } else { 0.0 } + 21.0 + 6.0 * rows::GAP;
        (inner - fixed).max(60.0)
    }
}

/// The state button's column, first in every module row.
const STATE_W: f32 = 21.0;

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
    let can_activate = spai_fit::formats::can_activate(dogma, type_id);
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

/// A module's own figure as the Effect column prints it, and what it is.
fn info_text(i: spai_fit::stats::ModuleInfo) -> (String, &'static str) {
    use spai_fit::stats::InfoKind as K;
    let v = i.value;
    let hp = || format!("{} HP", text::num(v));
    let gj = || format!("{} GJ", text::num(v.abs()));
    let pct = || format!("{v:+.0}%");
    match i.kind {
        K::ShieldBoost => (hp(), tr!("Shield boosted per cycle")),
        K::ArmorRepair => (hp(), tr!("Armor repaired per cycle")),
        K::HullRepair => (hp(), tr!("Hull repaired per cycle")),
        K::RemoteShield => (hp(), tr!("Shield given per cycle")),
        K::RemoteArmor => (hp(), tr!("Armor given per cycle")),
        K::RemoteHull => (hp(), tr!("Hull given per cycle")),
        K::CapInjected => (gj(), tr!("Capacitor injected per charge")),
        K::Neut => (gj(), tr!("Capacitor neutralized per cycle")),
        K::Nos => (gj(), tr!("Capacitor drained per cycle")),
        K::CapTransfer => (gj(), tr!("Capacitor sent per cycle")),
        K::Speed => (pct(), tr!("Speed while running")),
        K::Web => (pct(), tr!("Speed of the target")),
        K::Paint => (pct(), tr!("Signature of the target")),
        K::Damp => (pct(), tr!("The target's lock range or scan resolution")),
        K::Disrupt => (pct(), tr!("The target's weapons, at their strongest")),
        K::Ecm => (format!("{} pts", text::num(v)), tr!("Jam strength")),
        K::Tackle => (format!("{} pts", text::num(v)), tr!("Warp disruption strength")),
    }
}

/// The states every one of the types can take, lowest first.
pub fn common_states(dogma: &Dogma, types: &[TypeId]) -> Vec<State> {
    let mut it = types.iter().map(|t| states_of(dogma, *t));
    let Some(first) = it.next() else { return Vec::new() };
    it.fold(first, |acc, s| acc.into_iter().filter(|x| s.contains(x)).collect())
}

/// The charges every one of the types takes, when they all take the same ones.
pub fn common_charges(dogma: &Dogma, types: &[TypeId]) -> Vec<TypeId> {
    let mut it = types.iter().map(|t| picker::compatible_charges(dogma, *t));
    let Some(first) = it.next() else { return Vec::new() };
    if first.is_empty() || it.any(|l| l != first) {
        return Vec::new();
    }
    first
}

/// A click on a row of the racks: plain picks the row alone, Ctrl toggles it, Shift takes the
/// stretch from the last plain or Ctrl click to it. `order` is every row top to bottom.
pub fn pick(selection: &mut Vec<(Slot, u8)>, anchor: &mut Option<(Slot, u8)>, order: &[(Slot, u8)], hit: (Slot, u8), ctrl: bool, shift: bool) {
    let at = |x: (Slot, u8)| order.iter().position(|o| *o == x);
    match (shift, anchor.and_then(at), at(hit)) {
        (true, Some(a), Some(h)) => {
            let (lo, hi) = (a.min(h), a.max(h));
            let range = order[lo..=hi].to_vec();
            if !ctrl {
                selection.clear();
            }
            for r in range {
                if !selection.contains(&r) {
                    selection.push(r);
                }
            }
        }
        _ if ctrl => {
            match selection.iter().position(|s| *s == hit) {
                Some(i) => {
                    selection.remove(i);
                }
                None => selection.push(hit),
            }
            *anchor = Some(hit);
        }
        _ => {
            selection.clear();
            selection.push(hit);
            *anchor = Some(hit);
        }
    }
}

/// Every charge at least one of the types takes, for a menu that loads each where it fits.
pub fn any_charges(dogma: &Dogma, types: &[TypeId]) -> Vec<TypeId> {
    let mut out: Vec<TypeId> = Vec::new();
    for t in types {
        for c in picker::compatible_charges(dogma, *t) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out.sort_by_key(|c| spai_fit::formats::name_of(dogma, *c));
    out
}

/// The next state a click moves a module to, round the ones it can take.
/// What a click on the state button (or a double click on the row) asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Click: online and active in turn; a passive module online and offline.
    Toggle,
    /// Right click: overloaded, or back to active.
    Overload,
    /// Ctrl and click: offline, or back online.
    Offline,
}

pub fn gesture_state(dogma: &Dogma, type_id: TypeId, state: State, g: Gesture) -> State {
    let states = states_of(dogma, type_id);
    match g {
        Gesture::Toggle if states.contains(&State::Active) => {
            if state >= State::Active {
                State::Online
            } else {
                State::Active
            }
        }
        Gesture::Toggle => {
            if state == State::Offline {
                State::Online
            } else {
                State::Offline
            }
        }
        Gesture::Overload if states.contains(&State::Overload) => {
            if state == State::Overload {
                State::Active
            } else {
                State::Overload
            }
        }
        Gesture::Overload => state,
        Gesture::Offline => {
            if state == State::Offline {
                State::Online
            } else {
                State::Offline
            }
        }
    }
}

pub fn next_state(dogma: &Dogma, type_id: TypeId, state: State) -> State {
    let states = states_of(dogma, type_id);
    let i = states.iter().position(|s| *s == state).unwrap_or(0);
    states[(i + 1) % states.len()]
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    if !sim.has_hull() {
        // An empty tab keeps the tab row, so it can be closed or left like any other.
        let empty_tab = sim.session().is_some();
        if empty_tab {
            fit_tabs(sim, ui);
        }
        let area = ui.max_rect();
        let drop = ui.interact(area, egui::Id::new("sim_empty_drop"), egui::Sense::hover());
        browser::accept_drop(sim, &drop, DropTarget::Hull, now_ms);
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(if empty_tab { tr!("An empty tab.") } else { tr!("No fit open.") });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr!("Double-click or drop a hull from the browser, open a saved fit, or paste one.")).weak());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let w = 2.0 * 120.0 + ui.spacing().item_spacing.x;
                ui.add_space((ui.available_width() - w) / 2.0);
                if ui.add_sized([120.0, 26.0], egui::Button::new(tr!("Pick a hull"))).clicked() {
                    picker::open(sim, if empty_tab { PickTarget::Ship } else { PickTarget::NewFit });
                }
                if ui.add_sized([120.0, 26.0], egui::Button::new(tr!("Import EFT"))).clicked() {
                    sim.import = Some(crate::ImportDialog::default());
                }
            });
        });
        return;
    }
    egui::Panel::bottom("sim_dock").resizable(true).default_size(sim.dock_h.unwrap_or(crate::dock::DOCK_H)).min_size(90.0).show_inside(ui, |ui| {
        crate::dock::show(sim, ui, now_ms);
    });
    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("sim_editor").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            fit_tabs(sim, ui);
            ui.add_space(4.0);
            header(sim, ui, &dogma, now_ms);
            variation_tabs(sim, ui, now_ms);
            crate::warnings_panel::strip(sim, ui);
            ui.add_space(4.0);
            racks(sim, ui, &dogma, now_ms);
        });
    });
}

/// One tab per open fit, the way the Jabber tab bar works: a click shows it, the X closes the one
/// shown, a right click offers to close the rest; the plus opens a hull picker for another.
fn fit_tabs(sim: &mut SimUi, ui: &mut egui::Ui) {
    let mut activate: Option<usize> = None;
    let mut close: Option<usize> = None;
    let mut close_others: Option<usize> = None;
    let mut add = false;
    let active = sim.active_tab();
    let names: Vec<(String, bool)> = sim.sessions().iter().map(|s| (s.fit.name.clone(), s.dirty)).collect();
    // One row, flush with the edges: as many tabs as fit, the one on screen always among them,
    // the rest behind a caret pinned right before the add button, as on the chat bar.
    let labels: Vec<String> = names.iter().map(|(n, _)| text::clip(n, 22)).collect();
    let widths: Vec<f32> = labels.iter().map(|l| spai_ui::widgets::tab_box_width(ui, l)).collect();
    let plus_w = 26.0;
    let caret_w = 46.0;
    // Clear of the scroll bar that the editor's scroll area lays over its right edge.
    let edge = 6.0;
    let room = ui.available_width() - plus_w - caret_w - edge;
    let on_bar = tabs_that_fit(&widths, room, active);
    let hidden: Vec<usize> = (0..names.len()).filter(|i| !on_bar.contains(i)).collect();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for &i in &on_bar {
            let (name, dirty) = &names[i];
            let hit = spai_ui::widgets::tab_box(ui, egui::Id::new(("sim_tab", i)), i == active, &labels[i], *dirty);
            let r = hit.resp.on_hover_text(name);
            if hit.select {
                activate = Some(i);
            }
            if hit.close {
                close = Some(i);
            }
            r.context_menu(|ui| {
                if ui.button(tr!("Close")).clicked() {
                    close = Some(i);
                    ui.close();
                }
                if names.len() > 1 && ui.button(tr!("Close the others")).clicked() {
                    close_others = Some(i);
                    ui.close();
                }
            });
        }
        let pad = ui.available_width() - plus_w - caret_w - edge;
        if pad > 0.0 {
            ui.add_space(pad);
        }
        let caret = if hidden.is_empty() { icon::CARET_DOWN.to_owned() } else { format!("{} {}", icon::CARET_DOWN, hidden.len()) };
        // Exactly its width, so a wider count never pushes the row past the edge.
        ui.allocate_ui_with_layout(egui::vec2(caret_w, spai_ui::widgets::TAB_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(caret_w);
            let button = egui::Button::new(caret).min_size(egui::vec2(caret_w, spai_ui::widgets::TAB_H)).wrap_mode(egui::TextWrapMode::Truncate).corner_radius(0.0);
            if hidden.is_empty() {
                ui.add_enabled(false, button).on_disabled_hover_text(tr!("Every tab fits on the bar"));
            } else {
                egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                        use spai_ui::widgets::SteadySelect as _;
                        ui.set_min_width(220.0);
                        for &i in &hidden {
                            let (name, dirty) = &names[i];
                            let label = if *dirty { format!("{name}*") } else { name.clone() };
                            if ui.menu_label(false, label).clicked() {
                                    activate = Some(i);
                                    ui.close();
                            }
                        }
                });
            }
        });
        if ui.add_sized([plus_w, spai_ui::widgets::TAB_H], egui::Button::new(icon::PLUS).frame_when_inactive(false)).on_hover_text(tr!("Another fit")).clicked() {
            add = true;
        }
    });
    if let Some(i) = activate {
        sim.activate(i);
    }
    if let Some(i) = close {
        sim.close_tab(i);
    }
    if let Some(keep) = close_others {
        for i in (0..names.len()).rev() {
            if i != keep {
                sim.close_tab(i);
            }
        }
    }
    if add {
        sim.new_empty_tab((ui.input(|i| i.time) * 1000.0) as u64);
    }
}

fn header(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, now_ms: u64) {
    let tips = sim.tip_values();
    let Some(s) = sim.session_mut() else { return };
    let ship = s.fit.ship;
    let ship_name = dogma.type_(ship).map_or_else(|| trf!("Unknown hull {id}", id = ship), |t| t.name.clone());
    let group = dogma.type_(ship).and_then(|t| dogma.group(t.group)).map_or(String::new(), |g| g.name.clone());
    let mut name = s.fit.name.clone();
    let mut pick_ship = false;
    let mut hull_details = false;
    let mut traits_open = false;
    let resp = ui
        .horizontal(|ui| {
            ui.add_space(rows::GAP);
            let r = ui.add(egui::Image::new(crate::icons::ship_render_uri(ship)).fit_to_exact_size(egui::vec2(56.0, 56.0)).corner_radius(4.0)).interact(egui::Sense::click());
            let r = r.on_hover_ui(|ui| details::type_tip_with(ui, dogma, ship, tr!("Click to change the hull, or drop a ship here"), tips.get(&DetailsTarget::Ship)));
            if r.clicked() {
                pick_ship = true;
            }
            r.context_menu(|ui| {
                if ui.button(tr!("Hull details…")).clicked() {
                    hull_details = true;
                    ui.close();
                }
            });
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
    if hull_details {
        sim.open_details(DetailsTarget::Ship);
    }
    if traits_open {
        sim.traits_window = true;
    }
}

/// The sub-fits: the base and every variation as a tab, one on at a time (Ctrl-click adds one of
/// another kind); a new one is recorded by editing.
fn variation_tabs(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    use spai_ui::widgets::SteadySelect as _;
    let Some(s) = sim.session() else { return };
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
        ui.add_space(rows::GAP + STATE_W + rows::GAP + 18.0);
        let name_w = cols.name_width(inner);
        rows::head_cell(ui, name_w, tr!("Module"), false);
        rows::head_cell(ui, cols.charge, tr!("Charge"), false);
        if cols.numbers {
            rows::head_cell(ui, cols.num, tr!("PG"), true);
            rows::head_cell(ui, cols.num, tr!("CPU"), true);
            rows::head_cell(ui, cols.num, tr!("Cap/s"), true);
            rows::head_cell(ui, cols.num, tr!("DPS"), true);
            if cols.extra {
                rows::head_cell(ui, cols.num, tr!("Cycle"), true);
                rows::head_cell(ui, cols.info, tr!("Effect"), true);
            }
            if cols.more {
                rows::head_cell(ui, cols.num, tr!("Cost"), true);
                rows::head_cell(ui, cols.num, tr!("Burnout"), true);
            }
        }
    });
    let fixed = sim.variation_positions();
    let Some(view) = sim.view_fit() else { return };
    sim.selection.retain(|(s, i)| view.module(*s, *i).is_some());
    let stats: Vec<spai_fit::stats::ModuleStat> = sim.session().map(|s| s.module_stats.clone()).unwrap_or_default();
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
        ui.add_space(STATE_W + rows::GAP + 22.0);
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
    accept_module_drop(sim, ui, &r, slot, index, now_ms);
    accept_cargo_drop(sim, &r, slot, index, now_ms);
}

/// One of a cargo stack dropped on a slot: fitted there, and what sat there goes into the cargo
/// in its place, so nothing is gained or lost on the way.
fn accept_cargo_drop(sim: &mut SimUi, r: &egui::Response, slot: Slot, index: u8, now_ms: u64) {
    let Some(from) = rows::released::<rows::DragCargo>(r) else { return };
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(item) = sim.session().and_then(|s| s.fit.cargo.get(from.0).cloned()) else { return };
    if dogma.slot_of(item.type_id) != Some(slot) {
        sim.note(trf!("{name} does not go there", name = spai_fit::formats::name_of(&dogma, item.type_id)));
        return;
    }
    let state = spai_fit::formats::default_state(&dogma, item.type_id);
    sim.edit(EditKind::AddModule, now_ms, |f| {
        let Some(c) = f.cargo.get_mut(from.0) else { return };
        c.quantity -= 1;
        let hold = c.hold;
        if c.quantity == 0 {
            f.cargo.remove(from.0);
        }
        if let Some(old) = f.remove_module(slot, index) {
            put_in_cargo(f, old.type_id, hold);
        }
        let mut m = spai_fit::fit::Module::new(item.type_id, slot, index);
        m.state = state;
        f.modules.push(m);
    });
    sim.selection.clear();
}

fn put_in_cargo(f: &mut spai_fit::fit::Fit, type_id: TypeId, hold: Option<spai_fit::ids::AttrId>) {
    match f.cargo.iter_mut().find(|c| c.type_id == type_id && c.hold == hold) {
        Some(c) => c.quantity += 1,
        None => f.cargo.push(spai_fit::fit::CargoItem { type_id, quantity: 1, hold }),
    }
}

/// A fitted module taken off its rack into the cargo hold the tab shows.
pub fn unfit_to_cargo(sim: &mut SimUi, slot: Slot, index: u8, now_ms: u64) {
    let bays: Vec<spai_fit::ids::AttrId> = [spai_fit::stats::ESCAPE_BAY, spai_fit::stats::MAINTENANCE_BAY].iter().filter_map(|n| sim.dogma.as_ref().and_then(|d| d.attr_named(n)).map(|a| a.id)).collect();
    let hold = sim.dock.hold.filter(|h| !bays.contains(h));
    sim.edit(EditKind::Other, now_ms, |f| {
        if let Some(m) = f.remove_module(slot, index) {
            put_in_cargo(f, m.type_id, hold);
        }
    });
    sim.selection.clear();
}

/// A fitted module dropped on a slot of its own rack: moved there, trading places with what sat
/// there, or copied with Shift held on release.
fn accept_module_drop(sim: &mut SimUi, ui: &egui::Ui, r: &egui::Response, slot: Slot, index: u8, now_ms: u64) {
    let Some(from) = rows::released::<rows::DragModule>(r) else { return };
    let rows::DragModule(from_slot, from_index) = *from;
    if from_slot != slot || from_index == index {
        return;
    }
    let copy = ui.input(|i| i.modifiers.shift);
    move_module(sim, slot, from_index, index, copy, now_ms);
}

/// Moves the module at `from` to `to` within a rack, swapping with any module there, or copies it.
pub fn move_module(sim: &mut SimUi, slot: Slot, from: u8, to: u8, copy: bool, now_ms: u64) {
    sim.edit(if copy { EditKind::AddModule } else { EditKind::Other }, now_ms, |f| {
        let Some(moving) = f.module(slot, from).cloned() else { return };
        if copy {
            f.remove_module(slot, to);
            f.modules.push(spai_fit::fit::Module { index: to, ..moving });
            return;
        }
        let there = f.remove_module(slot, to);
        f.remove_module(slot, from);
        f.modules.push(spai_fit::fit::Module { index: to, ..moving });
        if let Some(t) = there {
            f.modules.push(spai_fit::fit::Module { index: from, ..t });
        }
    });
    sim.selection.clear();
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
    let mut details: Option<DetailsTarget> = None;
    let selected = sim.selection.contains(&(slot, index));
    let mut handle: Option<egui::Response> = None;
    let tips = sim.tip_values();
    let price = sim.prices.get(&type_id).copied();
    let rolled = sim.view_fit().and_then(|v| v.module(slot, index).and_then(|m| m.mutation.clone()));
    let can_roll = !dogma.mutaplasmids_for(type_id).is_empty();
    let mut roll_it = false;
    // Overloaded: a light red under the row and the state button's flame in red; active: a light green.
    let heat = match state {
        State::Overload => Some(ui.visuals().error_fg_color),
        State::Active => Some(egui::Color32::from_rgb(0x7F, 0xC9, 0x8A)),
        _ => None,
    };
    let r = rows::row_tinted(ui, rack_list(slot), ("sim_mod", slot, index), ROW_H, selected, heat, |ui| {
        if by_variation {
            ui.disable();
        }
        let inner = ui.available_width();
        // The state first, in a cell of its own width: green active, red overloaded.
        let glyph = match state {
            State::Overload => egui::RichText::new(state_icon(state)).color(ui.visuals().error_fg_color),
            State::Active => egui::RichText::new(state_icon(state)).color(egui::Color32::from_rgb(0x7F, 0xC9, 0x8A)),
            _ => egui::RichText::new(state_icon(state)),
        };
        let b = ui.allocate_ui_with_layout(egui::vec2(STATE_W, ROW_H - 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(STATE_W);
            icon_button(ui, glyph)
        }).inner;
        let b = b.on_hover_text(trf!("{state}. Click: online or active. Right-click: overload. Ctrl-click: offline.", state = state_name(state)));
        let ctrl = ui.input(|i| i.modifiers.command);
        if b.clicked() {
            set_state = Some(gesture_state(dogma, type_id, state, if ctrl { Gesture::Offline } else { Gesture::Toggle }));
        }
        if b.secondary_clicked() {
            set_state = Some(gesture_state(dogma, type_id, state, Gesture::Overload));
        }
        // The icon is the handle: dragging it moves the module, with Shift copies it. The rest of
        // the row stays for clicks, menus and the charge button.
        let icon = crate::icons::type_icon(ui, crate::mutation::icon_of(dogma, type_id, rolled.as_ref()), 18.0).interact(egui::Sense::drag()).on_hover_text(tr!("Drag to another slot to move it, with Shift to copy it"));
        handle = Some(icon);
        let name_w = cols.name_width(inner);
        let label = egui::RichText::new(&name);
        let label = if rolled.is_some() { label.color(crate::mutation::ROLLED) } else { label };
        let label = if state == State::Offline { label.weak() } else { label };
        let note = if by_variation { tr!("Set by a variation") } else { "" };
        let fitted = tips.get(&DetailsTarget::Module(slot, index));
        rows::cell(ui, name_w, label, "").on_hover_ui(|ui| details::type_tip_with(ui, dogma, type_id, note, fitted));
        if takes_charges {
            let shown = charge_name.clone().unwrap_or_else(|| tr!("No charge").to_owned());
            // Drawn as the row's own text, no frame: only a hover shade says it can be clicked.
            let (rect, r) = ui.allocate_exact_size(egui::vec2(cols.charge, ROW_H - 4.0), egui::Sense::click());
            if r.hovered() {
                ui.painter().rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(4.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            cell.set_clip_rect(cell.clip_rect().intersect(rect));
            cell.spacing_mut().item_spacing.x = 4.0;
            let label = egui::RichText::new(&shown);
            let label = if charge.is_some() { label } else { label.weak() };
            if let Some(c) = charge {
                cell.add(egui::Image::new(crate::icons::type_icon_uri(c)).fit_to_exact_size(egui::vec2(16.0, 16.0)).corner_radius(2.0));
            }
            cell.add(egui::Label::new(label).truncate().selectable(false).show_tooltip_when_elided(false));
            // What the module holds of it: as many as its capacity takes, and their room.
            let loaded = charge.and_then(|c| {
                let cap = f64::from(dogma.type_(type_id)?.attr(spai_fit::dogma::attr::CAPACITY)?);
                let vol = f64::from(dogma.type_(c)?.volume());
                (vol > 0.0 && cap > 0.0).then(|| {
                    let n = (cap / vol + 1e-6).floor();
                    (n, n * vol)
                })
            });
            let hover = match (&charge_name, loaded) {
                (Some(n), Some((count, m3))) => trf!("{name}: {count} loaded, {m3} m³", name = n, count = text::num(count), m3 = text::num(m3)),
                (Some(n), None) => n.clone(),
                (None, _) => tr!("Click to pick a charge").to_owned(),
            };
            if r.on_hover_text(hover).clicked() {
                open_picker = Some(PickTarget::Charge(slot, index));
            }
        } else {
            // Allocated like the charge cell, item spacing and all, so the columns after it line up.
            ui.allocate_exact_size(egui::vec2(cols.charge, ROW_H - 4.0), egui::Sense::hover());
        }
        if cols.numbers {
            let s = stat.as_ref();
            let off = state == State::Offline;
            rows::num_cell(ui, cols.num, s.map_or(String::new(), |m| text::num(m.power)), off);
            rows::num_cell(ui, cols.num, s.map_or(String::new(), |m| text::num(m.cpu)), off);
            rows::num_cell(ui, cols.num, s.filter(|m| m.cap_per_s != 0.0).map_or(String::new(), |m| text::num(m.cap_per_s)), off);
            rows::num_cell(ui, cols.num, s.filter(|m| m.dps > 0.0).map_or(String::new(), |m| text::num(m.dps)), off);
            if cols.extra {
                rows::num_cell(ui, cols.num, s.and_then(|m| m.cycle_s).map_or(String::new(), |c| format!("{}s", text::num(c))), off);
                let info = s.and_then(|m| m.info);
                let (shown, hover) = info.map_or((String::new(), ""), |i| info_text(i));
                let r = ui.allocate_ui_with_layout(egui::vec2(cols.info, ROW_H - 4.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_min_width(cols.info);
                    ui.set_max_width(cols.info);
                    let t = egui::RichText::new(shown).monospace();
                    ui.add(egui::Label::new(if off { t.weak() } else { t }).truncate().selectable(false).show_tooltip_when_elided(false));
                });
                if !hover.is_empty() {
                    r.response.on_hover_text(hover);
                }
            }
            if cols.more {
                rows::num_cell(ui, cols.num, price.map_or(String::new(), text::num), off);
                let burn = s.and_then(|m| m.burnout_s);
                let r = ui.allocate_ui_with_layout(egui::vec2(cols.num, ROW_H - 4.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_min_width(cols.num);
                    ui.set_max_width(cols.num);
                    let t = egui::RichText::new(burn.map_or(String::new(), text::secs)).monospace();
                    ui.add(egui::Label::new(if off { t.weak() } else { t }).truncate().selectable(false).show_tooltip_when_elided(false));
                });
                if burn.is_some() {
                    r.response.on_hover_text(tr!("Overloaded, how long it lasts on average: the rack heats up from every module overloaded in it, and each cycle may damage it and its neighbours. More modules overloaded beside it, sooner."));
                }
            }
        }
        if icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
            remove = true;
        }
    });
    if !by_variation {
        if let Some(h) = &handle {
            rows::drag_from(ui, h, rows::DragModule(slot, index), &name);
        }
        rows::drag_from(ui, &r, rows::DragModule(slot, index), &name);
    }
    if !by_variation {
        accept_module_drop(sim, ui, &r, slot, index, now_ms);
        accept_cargo_drop(sim, &r, slot, index, now_ms);
        if r.clicked() {
            // Kept for a double click, whose first click would otherwise leave this row alone selected.
            sim.sel_before_click = Some(sim.selection.clone());
            let m = ui.input(|i| i.modifiers);
            let order: Vec<(Slot, u8)> = sim.view_fit().map(|v| Slot::ALL.iter().flat_map(|s| v.rack(*s).map(|m| (m.slot, m.index)).collect::<Vec<_>>()).collect()).unwrap_or_default();
            let mut order = order;
            order.sort_by_key(|(s, i)| (Slot::ALL.iter().position(|x| x == s), *i));
            pick(&mut sim.selection, &mut sim.sel_anchor, &order, (slot, index), m.command, m.shift);
        }
        if r.double_clicked() {
            let ctrl = ui.input(|i| i.modifiers.command);
            set_state = Some(gesture_state(dogma, type_id, state, if ctrl { Gesture::Offline } else { Gesture::Toggle }));
            if let Some(prev) = sim.sel_before_click.take().filter(|p| p.len() > 1 && p.contains(&(slot, index))) {
                sim.selection = prev;
            }
        }
        // A right click on a row of the selection acts on the whole selection; on any other row
        // it picks that row alone first.
        if r.secondary_clicked() && !sim.selection.contains(&(slot, index)) {
            sim.selection = vec![(slot, index)];
            sim.sel_anchor = Some((slot, index));
        }
        let targets: Vec<(Slot, u8)> = if sim.selection.len() > 1 && sim.selection.contains(&(slot, index)) { sim.selection.clone() } else { vec![(slot, index)] };
        let view = sim.view_fit();
        let target_mods: Vec<(TypeId, State, Option<TypeId>)> = targets.iter().filter_map(|(s, i)| view.as_ref().and_then(|v| v.module(*s, *i)).map(|m| (m.type_id, m.state, m.charge))).collect();
        let types: Vec<TypeId> = target_mods.iter().map(|m| m.0).collect();
        let single = targets.len() == 1;
        r.context_menu(|ui| {
            ui.set_min_width(200.0);
            if !single {
                ui.label(egui::RichText::new(trf!("{n} modules", n = targets.len())).weak());
            }
            for st in common_states(dogma, &types) {
                if ui.radio(target_mods.iter().all(|m| m.1 == st), state_name(st)).clicked() {
                    set_state = Some(st);
                    ui.close();
                }
            }
            ui.separator();
            let charges = any_charges(dogma, &types);
            if !charges.is_empty() {
                ui.menu_button(tr!("Charge"), |ui| {
                    ui.set_min_width(260.0);
                    if ui.radio(target_mods.iter().all(|m| m.2.is_none()), tr!("None")).clicked() {
                        set_charge = Some(None);
                        ui.close();
                    }
                    // One submenu per kind: Tech I, Tech II, each faction.
                    for (title, list) in picker::charge_sections(dogma, &charges) {
                        let current = list.iter().any(|c| target_mods.iter().any(|m| m.2 == Some(*c)));
                        let label = if current { egui::RichText::new(&title).strong() } else { egui::RichText::new(&title) };
                        ui.menu_button(label, |ui| {
                            ui.set_min_width(240.0);
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for c in list {
                                    let icon = egui::Image::new(crate::icons::type_icon_uri(c)).fit_to_exact_size(egui::vec2(16.0, 16.0)).corner_radius(2.0);
                                    let item = egui::Button::image_and_text(icon, spai_fit::formats::name_of(dogma, c)).selected(target_mods.iter().all(|m| m.2 == Some(c))).frame_when_inactive(false);
                                    if ui.add(item).clicked() {
                                        set_charge = Some(Some(c));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    }
                });
            }
            let same_type = types.iter().all(|t| *t == type_id);
            let variants: Vec<TypeId> = if same_type { dogma.variants_of(type_id).iter().copied().filter(|v| *v != type_id && dogma.type_(*v).is_some_and(|t| t.published)).collect() } else { Vec::new() };
            if !variants.is_empty() {
                ui.menu_button(tr!("Variants"), |ui| {
                    ui.set_min_width(260.0);
                    if let Some(v) = variants_menu(ui, dogma, variants) {
                        replace = Some(v);
                    }
                });
            }
            if single && ui.button(tr!("Change module…")).clicked() {
                open_picker = Some(PickTarget::Module(slot, index));
                ui.close();
            }
            if single && can_roll && ui.button(if rolled.is_some() { tr!("Abyssal mutation…") } else { tr!("Roll with a mutaplasmid…") }).clicked() {
                roll_it = true;
                ui.close();
            }
            if single && ui.button(tr!("Details…")).clicked() {
                details = Some(DetailsTarget::Module(slot, index));
                ui.close();
            }
            if single && charge.is_some() && ui.button(tr!("Charge details…")).clicked() {
                details = Some(DetailsTarget::Charge(slot, index));
                ui.close();
            }
            ui.separator();
            if ui.button(if single { tr!("Remove").to_owned() } else { trf!("Remove {n} modules", n = targets.len()) }).clicked() {
                remove = true;
                ui.close();
            }
        });
        browser::accept_drop(sim, &r, DropTarget::Module(slot, index), now_ms);
        if let Some(t) = details {
            sim.open_details(t);
        }
        if roll_it {
            sim.mutation_window = Some(crate::mutation::MutTarget::Module(slot, index));
        }
        if let Some(t) = open_picker {
            picker::open(sim, t);
        }
        // Every module of the selection takes the clicked one's new state, as far as it can go:
        // they end up in step rather than each flipping its own way.
        if let Some(st) = set_state {
            sim.edit(EditKind::SetState, now_ms, |f| {
                for (s, i) in &targets {
                    if let Some(m) = f.module_mut(*s, *i) {
                        m.state = states_of(dogma, m.type_id).into_iter().filter(|x| *x <= st).max().unwrap_or(State::Offline);
                    }
                }
            });
        }
        if let Some(c) = set_charge {
            sim.edit(EditKind::SetCharge, now_ms, |f| {
                for (s, i) in &targets {
                    if let Some(m) = f.module_mut(*s, *i) {
                        // A charge goes into the ones that take it; the rest keep theirs.
                        if c.is_none_or(|c| picker::compatible_charges(dogma, m.type_id).contains(&c)) {
                            m.charge = c;
                        }
                    }
                }
            });
        }
        if let Some(v) = replace {
            sim.edit(EditKind::AddModule, now_ms, |f| {
                for (s, i) in &targets {
                    if let Some(m) = f.module_mut(*s, *i) {
                        m.type_id = v;
                    }
                }
            });
        }
        if remove {
            sim.edit(EditKind::RemoveModule, now_ms, |f| {
                for (s, i) in &targets {
                    f.remove_module(*s, *i);
                }
            });
            sim.selection.clear();
        }
    }
}

/// A module's variants: Tech I and II and the like at the top, the storyline, faction, deadspace
/// and officer ones each in a menu of their own, all by meta level with their icons.
fn variants_menu(ui: &mut egui::Ui, dogma: &Dogma, mut variants: Vec<TypeId>) -> Option<TypeId> {
    use spai_fit::ids::meta_group;
    variants.sort_by_key(|v| dogma.type_(*v).map(|t| (t.meta_level.unwrap_or(0), t.name.clone())));
    let group_of = |v: &TypeId| dogma.type_(*v).and_then(|t| t.meta_group);
    let item = |ui: &mut egui::Ui, v: TypeId| {
        let icon = egui::Image::new(crate::icons::type_icon_uri(v)).fit_to_exact_size(egui::vec2(18.0, 18.0));
        let hit = ui.add(egui::Button::image_and_text(icon, spai_fit::formats::name_of(dogma, v)).frame_when_inactive(false)).clicked();
        if hit {
            ui.close();
        }
        hit
    };
    let sub = [(meta_group::STORYLINE, tr!("Storyline")), (meta_group::FACTION, tr!("Faction")), (meta_group::DEADSPACE, tr!("Deadspace")), (meta_group::OFFICER, tr!("Officer"))];
    let mut picked = None;
    for &v in variants.iter().filter(|v| !sub.iter().any(|(g, _)| group_of(v) == Some(*g))) {
        if item(ui, v) {
            picked = Some(v);
        }
    }
    for (g, label) in sub {
        let these: Vec<TypeId> = variants.iter().copied().filter(|v| group_of(v) == Some(g)).collect();
        if these.is_empty() {
            continue;
        }
        ui.menu_button(label, |ui| {
            ui.set_min_width(260.0);
            for v in these {
                if item(ui, v) {
                    picked = Some(v);
                }
            }
        });
    }
    picked
}

/// The hull's traits, laid out like the tooltips, while `traits_window` is on.
pub fn traits_window(sim: &mut SimUi, ctx: &egui::Context) {
    if !sim.traits_window {
        return;
    }
    let (Some(dogma), Some(ship)) = (sim.dogma.clone(), sim.session().map(|s| s.fit.ship)) else { return };
    let dogma = dogma.as_ref();
    let ship_name = spai_fit::formats::name_of(dogma, ship);
        let mut open = true;
        egui::Window::new(trf!("{ship} traits", ship = ship_name)).open(&mut open).collapsible(false).default_width(440.0).show(ctx, |ui| {
            let Some(t) = dogma.traits_of(ship) else {
                ui.label(egui::RichText::new(tr!("This hull has no traits.")).weak());
                return;
            };
            // Each section a heading with the skill's icon, its bonuses as an amount column in the
            // bonus colour beside the sentence, the way the tooltips lay out values.
            let section = |ui: &mut egui::Ui, icon: Option<spai_fit::ids::TypeId>, title: String, note: Option<&str>, bonuses: &[spai_fit::dogma::Bonus], id: (u8, u32)| {
                ui.horizontal(|ui| {
                    if let Some(i) = icon {
                        crate::icons::type_icon(ui, i, 18.0);
                    }
                    ui.label(egui::RichText::new(title).strong());
                    if let Some(n) = note {
                        ui.label(egui::RichText::new(n).weak());
                    }
                });
                egui::Grid::new(("sim_traits", id)).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                    for b in bonuses {
                        let amount = b.amount.map(|a| {
                            let unit = b.unit.and_then(|u| dogma.unit(u)).map_or("", |u| u.display.as_str());
                            format!("{}{unit}", text::num(f64::from(a)))
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            // The sentence's own font: a monospace amount sat lower than the words beside it.
                            ui.label(egui::RichText::new(amount.unwrap_or_default()).color(egui::Color32::from_rgb(0x7F, 0xC9, 0x8A)));
                        });
                        // Top-aligned like the amount, so a sentence that wraps keeps its first line level with it.
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| ui.add(egui::Label::new(&b.text).wrap()));
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
            };
            for (skill, bonuses) in &t.per_skill {
                let skill_name = dogma.type_(*skill).map_or_else(|| format!("Skill {skill}"), |s| s.name.clone());
                section(ui, Some(*skill), skill_name, Some(tr!("per skill level")), bonuses, (0, *skill));
            }
            if !t.role.is_empty() {
                section(ui, None, tr!("Role bonus").to_owned(), None, &t.role, (1, 0));
            }
            if !t.misc.is_empty() {
                section(ui, None, tr!("Also").to_owned(), None, &t.misc, (2, 0));
            }
        });
        sim.traits_window = open;
}

/// Which tabs go on a bar `room` wide, in order: from the first while they fit, the active one
/// put in at the end in place of the last ones when it would be left off.
pub fn tabs_that_fit(widths: &[f32], room: f32, active: usize) -> Vec<usize> {
    let mut bar = Vec::new();
    let mut used = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if used + w > room {
            break;
        }
        used += w;
        bar.push(i);
    }
    if active < widths.len() && !bar.contains(&active) {
        while used + widths[active] > room && !bar.is_empty() {
            used -= widths[bar.pop().expect("not empty")];
        }
        bar.push(active);
    }
    bar
}
