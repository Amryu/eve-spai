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
    /// What this picker added so far, for a window that stays open between picks.
    pub added: Vec<TypeId>,
    /// For a module slot: which modules to offer, by what the hull can take.
    pub filter: FitFilter,
    /// The tree's first level has been opened once.
    opened_roots: bool,
}

/// Which modules a slot's picker offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FitFilter {
    All,
    /// What the hull allows: ship and group restrictions, rig size, hardpoints it has.
    Hull,
    /// That, and within the hull's whole CPU and powergrid.
    Totals,
    /// That, and within what the rest of the fit leaves.
    #[default]
    Left,
}

/// What the hull and the rest of the fit leave for one slot.
#[derive(Clone, Debug, PartialEq)]
pub struct FitRoom {
    pub ship: TypeId,
    pub ship_group: spai_fit::ids::GroupId,
    pub rig_size: Option<f32>,
    pub cpu_total: f64,
    pub power_total: f64,
    pub cpu_left: f64,
    pub power_left: f64,
    pub turrets_total: u32,
    pub launchers_total: u32,
    pub turrets_free: u32,
    pub launchers_free: u32,
}

/// Whether the module may go in under the filter. CPU and powergrid are the module's own, before
/// skills, so it errs on the side of leaving a module out that skills would let in.
pub fn fits(dogma: &Dogma, t: &spai_fit::dogma::TypeDef, room: &FitRoom, filter: FitFilter) -> bool {
    if filter == FitFilter::All {
        return true;
    }
    let named = |prefix: &str| -> Vec<f32> { t.attrs.iter().filter(|(a, _)| dogma.attr(*a).is_some_and(|d| d.name.starts_with(prefix))).map(|(_, v)| *v).collect() };
    let groups = named("canFitShipGroup");
    let ships = named("canFitShipType");
    if (!groups.is_empty() || !ships.is_empty()) && !groups.iter().any(|g| *g as u32 == u32::from(room.ship_group)) && !ships.iter().any(|s| *s as u32 == room.ship) {
        return false;
    }
    if let (Some(size), Some(rig)) = (dogma.attr_named("rigSize").and_then(|a| t.attr(a.id)), room.rig_size) {
        if size != rig {
            return false;
        }
    }
    let effect = |name: &str| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.name == name));
    let (turret, launcher) = (effect("turretFitted"), effect("launcherFitted"));
    if (turret && room.turrets_total == 0) || (launcher && room.launchers_total == 0) {
        return false;
    }
    if filter == FitFilter::Hull {
        return true;
    }
    let cpu = f64::from(t.attr(attr::CPU).unwrap_or(0.0));
    let power = f64::from(t.attr(attr::POWER).unwrap_or(0.0));
    if filter == FitFilter::Totals {
        return cpu <= room.cpu_total + 1e-6 && power <= room.power_total + 1e-6;
    }
    cpu <= room.cpu_left + 1e-6 && power <= room.power_left + 1e-6 && !(turret && room.turrets_free == 0) && !(launcher && room.launchers_free == 0)
}

impl SimUi {
    /// What the hull and the rest of the fit leave for the module in this slot, the module
    /// already there not counted.
    pub fn fit_room(&mut self, slot: Slot, index: u8) -> Option<FitRoom> {
        let dogma = self.dogma.clone()?;
        let c = self.computed()?;
        let s = self.session()?;
        let ship = dogma.type_(s.fit.ship)?;
        let here = s.module_stats.iter().find(|m| m.slot == slot && m.index == index);
        let (cpu_here, power_here) = here.map_or((0.0, 0.0), |m| (m.cpu, m.power));
        let module_here = s.fit.module(slot, index).and_then(|m| dogma.type_(m.type_id));
        let has = |t: Option<&spai_fit::dogma::TypeDef>, name: &str| t.is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.name == name)));
        let f = &c.fitting;
        Some(FitRoom {
            ship: ship.id,
            ship_group: ship.group,
            rig_size: dogma.attr_named("rigSize").and_then(|a| ship.attr(a.id)),
            cpu_total: f.cpu_total,
            power_total: f.power_total,
            cpu_left: f.cpu_total - f.cpu_used + cpu_here,
            power_left: f.power_total - f.power_used + power_here,
            turrets_total: f.turrets_total,
            launchers_total: f.launchers_total,
            turrets_free: (f.turrets_total + u32::from(has(module_here, "turretFitted"))).saturating_sub(f.turrets_used),
            launchers_free: (f.launchers_total + u32::from(has(module_here, "launcherFitted"))).saturating_sub(f.launchers_used),
        })
    }
}

pub const MAX_RESULTS: usize = 250;

impl Picker {
    pub fn new(target: PickTarget) -> Self {
        Picker { target, query: String::new(), results: Vec::new(), last_query: None, focus: true, takes: None, expanded: Default::default(), added: Vec::new(), filter: FitFilter::default(), opened_roots: false }
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
            PickTarget::Environment => tr!("Weather, phenomena or a system effect").to_owned(),
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
    let capacity = dogma.type_(module).and_then(|t| t.attr(attr::CAPACITY)).filter(|c| *c > 0.0);
    let mut out: Vec<TypeId> = groups
        .iter()
        .flat_map(|g| dogma.types_in_group(*g).iter().copied())
        .filter(|c| {
            dogma.type_(*c).is_some_and(|t| {
                // A sized module takes its own size only: Cap Booster 25 has no size at all and
                // does not go into an ancillary shield booster. Nor does anything bigger than it holds.
                let sized = size.is_none() || t.attr(attr::CHARGE_SIZE) == size;
                let fits = capacity.is_none_or(|cap| t.attr(attr::VOLUME).is_none_or(|v| v <= cap));
                t.published && sized && fits
            })
        })
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
                // Anything can be carried: spare modules, ships in a bay, loot.
                PickTarget::Cargo => t.category != category::SKILL,
                PickTarget::Projected => !crate::browser::environment(dogma, t) && crate::browser::projectable(dogma, t),
                // A beacon with no effects changes nothing and is not offered.
                PickTarget::Environment => (t.group == spai_fit::ids::group::EFFECT_BEACON && !t.effects.is_empty()) || spai_fit::projection::is_generator(dogma, t.id),
            })
            .map(|t| (t.name.to_lowercase(), t.id))
            .collect(),
    };
    out.sort();
    out.into_iter().map(|(_, id)| id).collect()
}

/// Charges sorted the way a pilot looks for them: Tech I, Tech II, then each faction on its own,
/// then the rest. A faction's name is what its charge adds to the Tech I one: "Republic Fleet"
/// for Republic Fleet EMP S over EMP S.
pub fn charge_sections(dogma: &Dogma, charges: &[TypeId]) -> Vec<(String, Vec<TypeId>)> {
    let mut t1 = Vec::new();
    let mut t2 = Vec::new();
    let mut factions: Vec<(String, Vec<TypeId>)> = Vec::new();
    let mut other = Vec::new();
    for &c in charges {
        let Some(t) = dogma.type_(c) else { continue };
        match t.meta_group {
            Some(2) => t2.push(c),
            Some(4) => {
                let parent = t.variation_parent.and_then(|p| dogma.type_(p)).map(|p| p.name.clone());
                let faction = parent
                    .as_deref()
                    .and_then(|p| t.name.strip_suffix(p))
                    .map(|f| f.trim().to_owned())
                    .filter(|f| !f.is_empty())
                    .unwrap_or_else(|| t.name.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
                match factions.iter_mut().find(|(n, _)| *n == faction) {
                    Some((_, v)) => v.push(c),
                    None => factions.push((faction, vec![c])),
                }
            }
            Some(1) | None => t1.push(c),
            _ => other.push(c),
        }
    }
    factions.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    if !t1.is_empty() {
        out.push((tr!("Tech I").to_owned(), t1));
    }
    if !t2.is_empty() {
        out.push((tr!("Tech II").to_owned(), t2));
    }
    for (name, v) in factions {
        out.push((trf!("Faction: {name}", name = name), v));
    }
    if !other.is_empty() {
        out.push((tr!("Other").to_owned(), other));
    }
    out
}

/// Whether the picker shows market categories while nothing is typed: not for one weapon's
/// charges, a short list already, nor for system effects, which are not on the market.
/// Whether the picker stays open after a pick: for the bays and lists that take many of a kind,
/// not for one slot, one charge or the hull.
pub fn stays_open(target: PickTarget) -> bool {
    matches!(target, PickTarget::Drone | PickTarget::Fighter | PickTarget::Implant | PickTarget::Booster | PickTarget::Cargo | PickTarget::Projected | PickTarget::Environment)
}

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
        PickTarget::Charge(slot, index) => {
            // A row of a multi-selection loads every picked module that takes the charge.
            let targets: Vec<(Slot, u8)> = if sim.selection.len() > 1 && sim.selection.contains(&(slot, index)) { sim.selection.clone() } else { vec![(slot, index)] };
            sim.edit(EditKind::SetCharge, now_ms, |f| {
                for (s, i) in &targets {
                    if let Some(m) = f.module_mut(*s, *i) {
                        if compatible_charges(&dogma, m.type_id).contains(&type_id) {
                            m.charge = Some(type_id);
                        }
                    }
                }
            });
        }
        PickTarget::Drone | PickTarget::Fighter => sim.edit(EditKind::Drones, now_ms, |f| {
            spai_fit::formats::add_item(&dogma, f, type_id, 1);
        }),
        PickTarget::Cargo => crate::browser::add_cargo(sim, type_id, now_ms),
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
    let room = match sim.picker.as_ref().map(|p| p.target) {
        Some(PickTarget::Module(slot, index)) => sim.fit_room(slot, index),
        _ => None,
    };
    let Some(picker) = &mut sim.picker else { return };
    if picker.last_query.as_deref() != Some(picker.query.as_str()) {
        let mut all = results(&dogma, picker.target, module_type, &picker.query);
        if let Some(room) = &room {
            all.retain(|id| dogma.type_(*id).is_some_and(|t| fits(&dogma, t, room, picker.filter)));
        }
        if picker.query.trim().is_empty() && picker.takes.is_none() {
            picker.takes = Some(all.iter().copied().collect());
        }
        all.truncate(MAX_RESULTS);
        picker.results = all;
        picker.last_query = Some(picker.query.clone());
    }
    let tree = (has_tree(picker.target) && picker.query.trim().is_empty()).then(|| sim.browser.tree(&dogma));
    let Some(picker) = &mut sim.picker else { return };
    // The first level open from the start, those with something to offer.
    if let (Some(tree), Some(takes), false) = (&tree, &picker.takes, picker.opened_roots) {
        for g in tree.roots() {
            if taken_under(tree, takes, *g) > 0 {
                picker.expanded.insert(*g);
            }
        }
        picker.opened_roots = true;
    }
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
        if room.is_some() {
            use spai_ui::widgets::SteadySelect as _;
            ui.horizontal(|ui| {
                // What the hull cannot fit at all is never offered; the choice is only how much room counts.
                let w = egui::vec2(((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(40.0), 22.0);
                for (f, label, hint) in [
                    (FitFilter::Left, tr!("Fits now"), tr!("Within the CPU, powergrid and hardpoints the rest of the fit leaves")),
                    (FitFilter::Totals, tr!("Fits empty"), tr!("Within the hull's whole CPU and powergrid")),
                ] {
                    if ui.menu_label_sized(w, picker.filter == f, label).on_hover_text(hint).clicked() && picker.filter != f {
                        picker.filter = f;
                        picker.last_query = None;
                        picker.takes = None;
                    }
                }
            });
        }
        if !picker.added.is_empty() {
            let last = picker.added.last().and_then(|t| dogma.type_(*t)).map_or(String::new(), |t| t.name.clone());
            let line = trf!("{icon} Added {n}, last {name}. Pick more, or close.", icon = egui_phosphor::regular::CHECK, n = picker.added.len(), name = crate::text::clip(&last, 30));
            ui.add(egui::Label::new(egui::RichText::new(line).color(ui.visuals().hyperlink_color)).truncate());
        }
        if matches!(target, PickTarget::Charge(..)) && picker.query.trim().is_empty() {
            let sections = charge_sections(&dogma, &picker.results);
            egui::ScrollArea::vertical().id_salt("sim_picker_charges").auto_shrink([false, false]).show(ui, |ui| {
                for (title, list) in &sections {
                    crate::rows::row(ui, "picker", ("sim_pick_section", title.as_str()), 24.0, false, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(title).strong()).selectable(false));
                    });
                    for &id in list {
                        let Some(t) = dogma.type_(id) else { continue };
                        let r = crate::rows::row(ui, "picker", ("sim_pick", id), 24.0, false, |ui| {
                            ui.add_space(14.0);
                            crate::icons::type_icon(ui, id, 18.0);
                            let w = ui.available_width();
                            crate::rows::cell(ui, w, egui::RichText::new(t.name.as_str()), "").on_hover_ui(|ui| crate::details::type_tip(ui, &dogma, id, ""));
                        });
                        if r.clicked() {
                            chosen = Some(id);
                        }
                    }
                }
            });
            return;
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
        if stays_open(target) {
            if let Some(p) = sim.picker.as_mut() {
                p.added.push(id);
            }
        } else {
            sim.picker = None;
        }
        apply(sim, target, id, now_ms);
    } else if !open {
        sim.picker = None;
    }
}
