//! Choosing a type: a search box over everything that fits the slot, bay or pilot asked for.

use std::sync::Arc;

use spai_fit::dogma::attr;
use spai_fit::fit::Slot;
use spai_fit::formats::{place_of, Place};
use spai_fit::history::EditKind;
use spai_fit::ids::{category, GroupId, TypeId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::SimUi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickTarget {
    /// The hull of the fit on screen.
    Ship,
    /// The hull of a fit of its own, in a new tab.
    NewFit,
    Module(Slot, u8),
    Charge(Slot, u8),
    Drone,
    Fighter,
    Implant,
    Booster,
    Cargo,
    /// A module or drone another ship points at this one.
    Projected,
    /// Wormhole or abyssal weather, an incursion, a system beacon.
    Environment,
}

pub struct Picker {
    pub target: PickTarget,
    pub query: String,
    /// Matching types, by name.
    pub results: Vec<TypeId>,
    last_query: Option<String>,
    focus: bool,
    /// Every type the target takes, for the category tree shown while nothing is typed.
    takes: Option<std::collections::HashSet<TypeId>>,
    /// Market groups opened in that tree.
    pub expanded: std::collections::BTreeSet<spai_fit::ids::MarketGroupId>,
}

pub const MAX_RESULTS: usize = 250;

impl Picker {
    pub fn new(target: PickTarget) -> Self {
        Picker { target, query: String::new(), results: Vec::new(), last_query: None, focus: true, takes: None, expanded: Default::default() }
    }

    pub fn title(&self) -> String {
        match self.target {
            PickTarget::Ship => tr!("Choose a ship").to_owned(),
            PickTarget::NewFit => tr!("Choose a hull for the new fit").to_owned(),
            PickTarget::Module(slot, _) => trf!("Choose a {slot} module", slot = slot_name(slot)),
            PickTarget::Charge(..) => tr!("Choose a charge").to_owned(),
            PickTarget::Drone => tr!("Choose a drone").to_owned(),
            PickTarget::Fighter => tr!("Choose a fighter").to_owned(),
            PickTarget::Implant => tr!("Choose an implant").to_owned(),
            PickTarget::Booster => tr!("Choose a booster").to_owned(),
            PickTarget::Cargo => tr!("Add cargo").to_owned(),
            PickTarget::Projected => tr!("A module or drone acting on this fit").to_owned(),
            PickTarget::Environment => tr!("Weather or a system effect").to_owned(),
        }
    }
}

pub fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::High => tr!("high"),
        Slot::Mid => tr!("mid"),
        Slot::Low => tr!("low"),
        Slot::Rig => tr!("rig"),
        Slot::Subsystem => tr!("subsystem"),
        Slot::Service => tr!("service"),
    }
}

/// The charge groups a module takes, from `chargeGroup1..5`, and its charge size.
pub fn charge_filter(dogma: &Dogma, module: TypeId) -> (Vec<GroupId>, Option<f32>) {
    let Some(t) = dogma.type_(module) else { return (Vec::new(), None) };
    let groups = [attr::CHARGE_GROUP_1, attr::CHARGE_GROUP_2, attr::CHARGE_GROUP_3, attr::CHARGE_GROUP_4, attr::CHARGE_GROUP_5]
        .iter()
        .filter_map(|a| t.attr(*a))
        .filter(|g| *g > 0.0)
        .map(|g| g as GroupId)
        .collect();
    (groups, t.attr(attr::CHARGE_SIZE))
}

/// Charges the module can load, by name.
pub fn compatible_charges(dogma: &Dogma, module: TypeId) -> Vec<TypeId> {
    let (groups, size) = charge_filter(dogma, module);
    let mut out: Vec<TypeId> = groups
        .iter()
        .flat_map(|g| dogma.types_in_group(*g).iter().copied())
        .filter(|c| dogma.type_(*c).is_some_and(|t| t.published && (size.is_none() || t.attr(attr::CHARGE_SIZE).is_none() || t.attr(attr::CHARGE_SIZE) == size)))
        .collect();
    out.sort_by_cached_key(|c| dogma.type_(*c).map(|t| t.name.to_lowercase()).unwrap_or_default());
    out.dedup();
    out
}

/// Everything the target accepts, those matching the query first by name, at most `MAX_RESULTS`.
pub fn results(dogma: &Dogma, target: PickTarget, module_type: Option<TypeId>, query: &str) -> Vec<TypeId> {
    let q = query.trim().to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    let matches = |name: &str| {
        let n = name.to_lowercase();
        words.iter().all(|w| n.contains(w))
    };
    let mut out: Vec<(String, TypeId)> = match target {
        PickTarget::Charge(..) => module_type.map(|m| compatible_charges(dogma, m)).unwrap_or_default().into_iter().filter_map(|id| dogma.type_(id)).filter(|t| matches(&t.name)).map(|t| (t.name.to_lowercase(), t.id)).collect(),
        _ => dogma
            .types
            .iter()
            // Beacons are not on the market and not published; they are still what weather is.
            .filter(|t| (t.published || (target == PickTarget::Environment && t.group == spai_fit::ids::group::EFFECT_BEACON)) && matches(&t.name))
            .filter(|t| match target {
                PickTarget::Ship | PickTarget::NewFit => t.category == category::SHIP,
                PickTarget::Module(slot, _) => (t.category == category::MODULE || t.category == category::SUBSYSTEM) && dogma.slot_of(t.id) == Some(slot),
                PickTarget::Charge(..) => false,
                PickTarget::Drone => t.category == category::DRONE,
                PickTarget::Fighter => t.category == category::FIGHTER,
                PickTarget::Implant => place_of(dogma, t) == Place::Implant,
                PickTarget::Booster => place_of(dogma, t) == Place::Booster,
                PickTarget::Cargo => t.category == category::CHARGE,
                PickTarget::Projected => t.group != spai_fit::ids::group::EFFECT_BEACON && crate::browser::projectable(dogma, t),
                PickTarget::Environment => t.group == spai_fit::ids::group::EFFECT_BEACON,
            })
            .map(|t| (t.name.to_lowercase(), t.id))
            .collect(),
    };
    out.sort();
    out.into_iter().map(|(_, id)| id).collect()
}

/// Whether the picker shows market categories while nothing is typed: not for one weapon's
/// charges, a short list already, nor for system effects, which are not on the market.
fn has_tree(target: PickTarget) -> bool {
    !matches!(target, PickTarget::Charge(..) | PickTarget::Environment)
}

pub fn open(sim: &mut SimUi, target: PickTarget) {
    sim.picker = Some(Picker::new(target));
}

/// How many of the types under `g` the picker takes, through every subgroup.
fn taken_under(tree: &crate::browser::Tree, takes: &std::collections::HashSet<TypeId>, g: spai_fit::ids::MarketGroupId) -> usize {
    tree.types(g).iter().filter(|t| takes.contains(t)).count() + tree.children(g).iter().map(|c| taken_under(tree, takes, *c)).sum::<usize>()
}

/// A market group the picker takes something from, and when open what it holds.
#[allow(clippy::too_many_arguments)]
fn tree_rows(ui: &mut egui::Ui, dogma: &Dogma, tree: &crate::browser::Tree, takes: &std::collections::HashSet<TypeId>, expanded: &mut std::collections::BTreeSet<spai_fit::ids::MarketGroupId>, g: spai_fit::ids::MarketGroupId, depth: usize, chosen: &mut Option<TypeId>) {
    let n = taken_under(tree, takes, g);
    if n == 0 {
        return;
    }
    let open = expanded.contains(&g);
    let indent = depth as f32 * 14.0;
    let r = crate::rows::row(ui, "picker", ("sim_pick_group", g), 24.0, false, |ui| {
        ui.add_space(indent);
        ui.add(egui::Label::new(if open { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT }).selectable(false));
        let w = ui.available_width();
        crate::rows::cell(ui, w, egui::RichText::new(format!("{}  ({n})", tree.name(g))), "");
    });
    if r.clicked() {
        if open {
            expanded.remove(&g);
        } else {
            expanded.insert(g);
        }
    }
    if !open {
        return;
    }
    for c in tree.children(g).to_vec() {
        tree_rows(ui, dogma, tree, takes, expanded, c, depth + 1, chosen);
    }
    for id in tree.types(g).iter().copied().filter(|t| takes.contains(t)) {
        let Some(t) = dogma.type_(id) else { continue };
        let r = crate::rows::row(ui, "picker", ("sim_pick", id), 24.0, false, |ui| {
            ui.add_space(indent + 18.0);
            crate::icons::type_icon(ui, id, 18.0);
            let w = ui.available_width();
            crate::rows::cell(ui, w, egui::RichText::new(t.name.as_str()), "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, id, ""));
        });
        if r.clicked() {
            *chosen = Some(id);
        }
    }
}

/// Puts the chosen type where the picker was opened for.
pub fn apply(sim: &mut SimUi, target: PickTarget, type_id: TypeId, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    match target {
        PickTarget::NewFit => sim.new_fit(type_id, now_ms),
        PickTarget::Ship => {
            if sim.session().is_some() {
                let name = dogma.type_(type_id).map(|t| t.name.clone()).unwrap_or_default();
                sim.edit(EditKind::Other, now_ms, |f| {
                    if f.ship == 0 {
                        f.name = name;
                    }
                    f.ship = type_id;
                });
            } else {
                sim.new_fit(type_id, now_ms);
            }
        }
        PickTarget::Module(slot, index) => sim.edit(EditKind::AddModule, now_ms, |f| {
            f.remove_module(slot, index);
            let mut m = spai_fit::fit::Module::new(type_id, slot, index);
            m.state = spai_fit::formats::default_state(&dogma, type_id);
            f.modules.push(m);
        }),
        PickTarget::Charge(slot, index) => sim.edit(EditKind::SetCharge, now_ms, |f| {
            if let Some(m) = f.module_mut(slot, index) {
                m.charge = Some(type_id);
            }
        }),
        PickTarget::Drone | PickTarget::Fighter | PickTarget::Cargo => sim.edit(EditKind::Drones, now_ms, |f| {
            spai_fit::formats::add_item(&dogma, f, type_id, 1);
        }),
        PickTarget::Implant | PickTarget::Booster => {
            crate::browser::quick_add(sim, type_id, now_ms);
        }
        PickTarget::Projected | PickTarget::Environment => {
            crate::browser::add_projected(sim, type_id, now_ms);
        }
    }
}

pub fn show_window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    let Some(dogma): Option<Arc<Dogma>> = sim.dogma.clone() else { return };
    let module_type = match sim.picker.as_ref().map(|p| p.target) {
        Some(PickTarget::Charge(slot, index)) => sim.session().and_then(|s| s.fit.module(slot, index)).map(|m| m.type_id),
        _ => None,
    };
    let Some(picker) = &mut sim.picker else { return };
    if picker.last_query.as_deref() != Some(picker.query.as_str()) {
        let mut all = results(&dogma, picker.target, module_type, &picker.query);
        if picker.query.trim().is_empty() && picker.takes.is_none() {
            picker.takes = Some(all.iter().copied().collect());
        }
        all.truncate(MAX_RESULTS);
        picker.results = all;
        picker.last_query = Some(picker.query.clone());
    }
    let tree = (has_tree(picker.target) && picker.query.trim().is_empty()).then(|| sim.browser.tree(&dogma));
    let Some(picker) = &mut sim.picker else { return };
    let mut open = true;
    let mut chosen: Option<TypeId> = None;
    let target = picker.target;
    let title = picker.title();
    let size = egui::vec2(420.0, 460.0);
    egui::Window::new(title).id(egui::Id::new("sim_picker")).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        let edit = ui.add(egui::TextEdit::singleline(&mut picker.query).hint_text(tr!("Search by name")).desired_width(f32::INFINITY));
        if picker.focus {
            edit.request_focus();
            picker.focus = false;
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            chosen = picker.results.first().copied();
        }
        if let (Some(tree), Some(takes)) = (&tree, &picker.takes) {
            egui::ScrollArea::vertical().id_salt("sim_picker_tree").auto_shrink([false, false]).show(ui, |ui| {
                for g in tree.roots().to_vec() {
                    tree_rows(ui, &dogma, tree, takes, &mut picker.expanded, g, 0, &mut chosen);
                }
            });
            return;
        }
        let count = picker.results.len();
        let note = match count {
            n if n >= MAX_RESULTS => trf!("First {n} matches, keep typing", n = MAX_RESULTS),
            1 => tr!("1 match").to_owned(),
            n => trf!("{n} matches", n = n),
        };
        ui.add(egui::Label::new(egui::RichText::new(note).weak()).truncate());
        egui::ScrollArea::vertical().id_salt("sim_picker_list").auto_shrink([false, false]).show_rows(ui, 24.0, count, |ui, range| {
            for i in range {
                let id = picker.results[i];
                let Some(t) = dogma.type_(id) else { continue };
                let group = dogma.group(t.group).map_or("", |g| g.name.as_str());
                let r = crate::rows::row(ui, "picker", ("sim_pick", id), 24.0, false, |ui| {
                    crate::icons::type_icon(ui, id, 18.0);
                    let w = (ui.available_width() - 120.0).max(80.0);
                    crate::rows::cell(ui, w, egui::RichText::new(t.name.as_str()), "").on_hover_ui(|ui| crate::details::type_tip(ui, &dogma, id, ""));
                    ui.add(egui::Label::new(egui::RichText::new(crate::text::clip(group, 18)).weak()).truncate());
                });
                if r.clicked() {
                    chosen = Some(id);
                }
            }
        });
    });
    if let Some(id) = chosen {
        sim.picker = None;
        apply(sim, target, id, now_ms);
    } else if !open {
        sim.picker = None;
    }
}
