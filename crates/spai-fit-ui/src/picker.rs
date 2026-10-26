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
    pub(crate) takes: Option<std::collections::HashSet<TypeId>>,
    /// Market groups opened in that tree.
    pub expanded: std::collections::BTreeSet<spai_fit::ids::MarketGroupId>,
    /// For a module slot: which modules to offer, by what the hull can take.
    pub filter: FitFilter,
    /// The tree's first level has been opened once.
    opened_roots: bool,
    /// For the cargo: only what the fit uses up, its charges, paste and fuel.
    pub consumables: bool,
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
    /// The CPU and powergrid a module of the slot would take on this hull with these skills: role
    /// bonuses cut some launchers' powergrid by 99%.
    pub costs: std::collections::HashMap<TypeId, (f64, f64)>,
    /// A subsystem slot's own group: core, defensive, offensive or propulsion.
    pub slot_group: Option<spai_fit::ids::GroupId>,
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
    if room.slot_group.is_some_and(|g| t.group != g) {
        return false;
    }
    // A subsystem goes on its own strategic cruiser only.
    if dogma.attr_named("fitsToShipType").and_then(|a| t.attr(a.id)).is_some_and(|s| s as u32 != room.ship) {
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
    let (cpu, power) = room.costs.get(&t.id).copied().unwrap_or((f64::from(t.attr(attr::CPU).unwrap_or(0.0)), f64::from(t.attr(attr::POWER).unwrap_or(0.0))));
    if filter == FitFilter::Totals {
        return cpu <= room.cpu_total + 1e-6 && power <= room.power_total + 1e-6;
    }
    cpu <= room.cpu_left + 1e-6 && power <= room.power_left + 1e-6 && !(turret && room.turrets_free == 0) && !(launcher && room.launchers_free == 0)
}

impl SimUi {
    /// What the hull and the rest of the fit leave for the module in this slot, figured with the
    /// module already there taken out: its own cost, and what it adds to the hull's output or to
    /// the other modules' costs.
    pub fn fit_room(&mut self, slot: Slot, index: u8) -> Option<FitRoom> {
        let dogma = self.dogma.clone()?;
        if !self.has_hull() {
            return None;
        }
        let outside = self.outside();
        let skills = self.skills();
        let s = self.session()?;
        let ship = dogma.type_(s.fit.ship)?;
        let mut rest = spai_fit::variations::resolve(&dogma, &s.fit);
        rest.remove_module(slot, index);
        let (rest, env) = spai_fit::projection::expand(&dogma, &rest, &|id| outside.fit(id).cloned(), skills);
        let calc = spai_fit::calc::Calc::new(&dogma, &rest, skills, &env);
        let f = spai_fit::stats::fitting(&calc, &rest);
        let costs = slot_costs(&dogma, &rest, skills, &env, slot);
        Some(FitRoom {
            ship: ship.id,
            ship_group: ship.group,
            rig_size: dogma.attr_named("rigSize").and_then(|a| ship.attr(a.id)),
            cpu_total: f.cpu_total,
            power_total: f.power_total,
            cpu_left: f.cpu_total - f.cpu_used,
            power_left: f.power_total - f.power_used,
            turrets_total: f.turrets_total,
            launchers_total: f.launchers_total,
            turrets_free: f.turrets_total.saturating_sub(f.turrets_used),
            launchers_free: f.launchers_total.saturating_sub(f.launchers_used),
            costs,
            slot_group: (slot == Slot::Subsystem).then(|| dogma.subsystem_groups(ship.id).get(usize::from(index)).copied()).flatten(),
        })
    }
}

/// What each module of the slot costs fitted to `fit`. Fitting costs are changed by group or by
/// required skill, so one module of each group and skill set stands in, offline, for the rest:
/// the share of its own CPU and powergrid it keeps is theirs too.
fn slot_costs(dogma: &Dogma, fit: &spai_fit::fit::Fit, skills: &spai_fit::skills::SkillSet, env: &spai_fit::calc::Env, slot: Slot) -> std::collections::HashMap<TypeId, (f64, f64)> {
    type Class = (spai_fit::ids::GroupId, Vec<TypeId>);
    let class_of = |t: &spai_fit::dogma::TypeDef| -> Class {
        let mut req: Vec<TypeId> = t.required_skills.iter().map(|(s, _)| *s).collect();
        req.sort_unstable();
        (t.group, req)
    };
    let modules: Vec<&spai_fit::dogma::TypeDef> = dogma.types.iter().filter(|t| t.published && (t.category == category::MODULE || t.category == category::SUBSYSTEM) && dogma.slot_of(t.id) == Some(slot)).collect();
    let mut reps: Vec<(Class, TypeId)> = Vec::new();
    for t in &modules {
        let c = class_of(t);
        if !reps.iter().any(|(r, _)| *r == c) {
            reps.push((c, t.id));
        }
    }
    // Stand-ins after the slots a hull can have, as many as the index leaves room for.
    const FIRST: u8 = 16;
    let mut share: std::collections::HashMap<Class, (f64, f64)> = std::collections::HashMap::new();
    for chunk in reps.chunks(usize::from(u8::MAX - FIRST)) {
        let mut f = fit.clone();
        for (i, (_, id)) in chunk.iter().enumerate() {
            f.modules.push(spai_fit::fit::Module::new(*id, slot, FIRST + i as u8).with_state(spai_fit::fit::State::Offline));
        }
        let calc = spai_fit::calc::Calc::new(dogma, &f, skills, env);
        for (i, (class, id)) in chunk.iter().enumerate() {
            let Some(m) = calc.module(slot, FIRST + i as u8) else { continue };
            let t = dogma.type_(*id);
            let ratio = |a: spai_fit::ids::AttrId| {
                let base = t.and_then(|t| t.attr(a)).map_or(0.0, f64::from);
                if base > 0.0 { calc.value(m, a) / base } else { 1.0 }
            };
            share.insert(class.clone(), (ratio(attr::CPU), ratio(attr::POWER)));
        }
    }
    modules
        .iter()
        .filter_map(|t| {
            let (c, p) = share.get(&class_of(t))?;
            Some((t.id, (f64::from(t.attr(attr::CPU).unwrap_or(0.0)) * c, f64::from(t.attr(attr::POWER).unwrap_or(0.0)) * p)))
        })
        .collect()
}

pub const MAX_RESULTS: usize = 250;

/// Width of each of the CPU, powergrid and capacitor columns of a module slot's picker.
const COST_W: f32 = 50.0;

/// A module's own CPU, powergrid and capacitor per second, before skills.
pub fn costs(dogma: &Dogma, id: TypeId) -> [f64; 3] {
    let Some(t) = dogma.type_(id) else { return [0.0; 3] };
    let a = |id: spai_fit::ids::AttrId| f64::from(t.attr(id).unwrap_or(0.0));
    let cap = t
        .effects
        .iter()
        .filter_map(|(e, _)| dogma.effect(*e))
        .find_map(|e| Some((e.discharge_attr?, e.duration_attr?)))
        .map(|(need, dur)| if a(dur) > 0.0 { a(need) / (a(dur) / 1000.0) } else { 0.0 })
        .unwrap_or(0.0);
    [a(attr::CPU), a(attr::POWER), cap]
}

/// A subsystem's high, mid and low slots.
pub fn slots_given(dogma: &Dogma, id: TypeId) -> [f64; 3] {
    let Some(t) = dogma.type_(id) else { return [0.0; 3] };
    [attr::HI_SLOT_MODIFIER, attr::MED_SLOT_MODIFIER, attr::LOW_SLOT_MODIFIER].map(|a| f64::from(t.attr(a).unwrap_or(0.0)))
}

/// A row's three figures: a subsystem's slots, any other module's costs.
fn row_figures(dogma: &Dogma, id: TypeId, subsystem: bool) -> [f64; 3] {
    if subsystem { slots_given(dogma, id) } else { costs(dogma, id) }
}

/// The three cells of a row: its own figures, or with `base` what swapping it in changes. More
/// slots is better, more cost worse.
fn cost_cells(ui: &mut egui::Ui, costs: [f64; 3], base: Option<[f64; 3]>, subsystem: bool) {
    ui.spacing_mut().item_spacing.x = 0.0;
    for (i, v) in costs.into_iter().enumerate() {
        let fmt = |v: f64| if (subsystem || i < 2) && v.fract().abs() < 0.05 { format!("{v:.0}") } else { format!("{v:.1}") };
        match base {
            Some(b) => {
                let d = v - b[i];
                if d.abs() < 0.05 {
                    crate::rows::num_cell(ui, COST_W, "0", true);
                } else {
                    let text = if d > 0.0 { format!("+{}", fmt(d)) } else { format!("-{}", fmt(-d)) };
                    crate::rows::num_cell_colored(ui, COST_W, text, if (d > 0.0) != subsystem { crate::details::BAD } else { crate::details::GOOD });
                }
            }
            None if v.abs() < 0.05 => crate::rows::num_cell(ui, COST_W, "-", true),
            None => crate::rows::num_cell(ui, COST_W, fmt(v), false),
        }
    }
}

/// The header over the cost columns, at the right edge.
fn cost_header(ui: &mut egui::Ui, swapping: bool, subsystem: bool) {
    let hint = match (subsystem, swapping) {
        (true, true) => tr!("Slot change if swapped in: high, mid, low"),
        (true, false) => tr!("Slots given: high, mid, low"),
        (false, true) => tr!("What swapping it in changes, before skills: CPU, powergrid, capacitor per second"),
        (false, false) => tr!("The module's own needs, before skills: CPU, powergrid, capacitor per second"),
    };
    let heads = if subsystem { [tr!("Low"), tr!("Mid"), tr!("High")] } else { [tr!("Cap/s"), tr!("PG"), tr!("CPU")] };
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 18.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // The rows below keep a gap at each edge.
        ui.add_space(crate::rows::GAP);
        for h in heads {
            crate::rows::head_cell(ui, COST_W, h, true);
        }
    })
    .response
    .on_hover_text(hint);
}

impl Picker {
    pub fn new(target: PickTarget) -> Self {
        Picker { target, query: String::new(), results: Vec::new(), last_query: None, focus: true, takes: None, expanded: Default::default(), filter: FitFilter::default(), opened_roots: false, consumables: false }
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
    dogma.charge_filter(module)
}

/// Charges the module can load, by name.
pub fn compatible_charges(dogma: &Dogma, module: TypeId) -> Vec<TypeId> {
    dogma.compatible_charges(module)
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
                PickTarget::Fighter => t.category == category::FIGHTER && !structure_fighter(dogma, t),
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

/// Structures launch fighters of their own, which no ship can.
pub fn structure_fighter(dogma: &Dogma, t: &spai_fit::dogma::TypeDef) -> bool {
    t.category == category::FIGHTER && dogma.group(t.group).is_some_and(|g| g.name.starts_with("Structure"))
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
    // A subsystem slot takes three: a list, not a tree to open.
    !matches!(target, PickTarget::Charge(..) | PickTarget::Environment | PickTarget::Module(Slot::Subsystem, _))
}

/// A subsystem slot's kind, by the group it takes.
pub fn subsystem_kind(group: spai_fit::ids::GroupId) -> &'static str {
    match group {
        958 => tr!("core"),
        954 => tr!("defensive"),
        956 => tr!("offensive"),
        957 => tr!("propulsion"),
        _ => tr!("subsystem"),
    }
}

/// What to call slot `index` of `slot` on `ship`: its kind for a subsystem slot.
pub fn slot_label(dogma: &Dogma, ship: TypeId, slot: Slot, index: u8) -> String {
    match dogma.subsystem_groups(ship).get(usize::from(index)).filter(|_| slot == Slot::Subsystem) {
        Some(g) => trf!("{kind} subsystem", kind = subsystem_kind(*g)),
        None => slot_name(slot).to_owned(),
    }
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
fn tree_rows(ui: &mut egui::Ui, dogma: &Dogma, tree: &crate::browser::Tree, takes: &std::collections::HashSet<TypeId>, expanded: &mut std::collections::BTreeSet<spai_fit::ids::MarketGroupId>, g: spai_fit::ids::MarketGroupId, depth: usize, chosen: &mut Option<TypeId>, cols: Option<Option<[f64; 3]>>) {
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
        tree_rows(ui, dogma, tree, takes, expanded, c, depth + 1, chosen, cols);
    }
    for id in tree.types(g).iter().copied().filter(|t| takes.contains(t)) {
        let Some(t) = dogma.type_(id) else { continue };
        let r = crate::rows::row(ui, "picker", ("sim_pick", id), 24.0, false, |ui| {
            ui.add_space(indent + 18.0);
            crate::icons::type_icon(ui, id, 18.0);
            let w = (ui.available_width() - cols.map_or(0.0, |_| 3.0 * COST_W + ui.spacing().item_spacing.x)).max(60.0);
            crate::rows::cell(ui, w, egui::RichText::new(t.name.as_str()), "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, id, ""));
            if let Some(base) = cols {
                cost_cells(ui, costs(dogma, id), base, false);
            }
        });
        r.context_menu(|ui| crate::compare::menu(ui, dogma, id));
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

/// The module a charge is picked for, as the fit shows it: a variation may have put another in
/// the slot than the base fit holds.
pub(crate) fn charge_module(sim: &SimUi, target: PickTarget) -> Option<TypeId> {
    let PickTarget::Charge(slot, index) = target else { return None };
    sim.shown_fit()?.module(slot, index).map(|m| m.type_id)
}

pub fn show_window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    let Some(dogma): Option<Arc<Dogma>> = sim.dogma.clone() else { return };
    let module_type = sim.picker.as_ref().and_then(|p| charge_module(sim, p.target));
    // Figured only when the results are, as it runs the fit's numbers again.
    let stale = sim.picker.as_ref().is_some_and(|p| p.last_query.as_deref() != Some(p.query.as_str()));
    let room = match sim.picker.as_ref().map(|p| p.target) {
        Some(PickTarget::Module(slot, index)) if stale => sim.fit_room(slot, index),
        _ => None,
    };
    // The cargo hold and a fleet hangar can be narrowed to what the fit uses up.
    let fleet_hangar = dogma.attr_named("fleetHangarCapacity").map(|a| a.id);
    let consumable_hold = matches!(sim.picker.as_ref().map(|p| p.target), Some(PickTarget::Cargo)) && (sim.dock.hold.is_none() || sim.dock.hold == fleet_hangar);
    let useful = if consumable_hold && sim.picker.as_ref().is_some_and(|p| p.consumables) { sim.shown_fit().map(|f| consumables_for(&dogma, &f)) } else { None };
    // A module slot's rows carry their costs; with a module in the slot, what swapping changes.
    let cols: Option<Option<[f64; 3]>> = match sim.picker.as_ref().map(|p| p.target) {
        Some(PickTarget::Module(slot, index)) => Some(sim.session().and_then(|s| spai_fit::variations::resolve(&dogma, &s.fit).module(slot, index).map(|m| row_figures(&dogma, m.type_id, slot == Slot::Subsystem)))),
        _ => None,
    };
    let subsystem = matches!(sim.picker.as_ref().map(|p| p.target), Some(PickTarget::Module(Slot::Subsystem, _)));
    let Some(picker) = &mut sim.picker else { return };
    if picker.last_query.as_deref() != Some(picker.query.as_str()) {
        let mut all = results(&dogma, picker.target, module_type, &picker.query);
        if let Some(room) = &room {
            all.retain(|id| dogma.type_(*id).is_some_and(|t| fits(&dogma, t, room, picker.filter)));
        }
        if let Some(useful) = &useful {
            all.retain(|id| useful.contains(id));
        }
        if picker.query.trim().is_empty() && picker.takes.is_none() {
            picker.takes = Some(all.iter().copied().collect());
        }
        all.truncate(MAX_RESULTS);
        picker.results = all;
        picker.last_query = Some(picker.query.clone());
    }
    let tree = (has_tree(picker.target) && picker.query.trim().is_empty()).then(|| sim.browser.tree(&dogma));
    let ship_now = sim.view_fit().map(|f| f.ship);
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
    let title = match (target, ship_now) {
        (PickTarget::Module(Slot::Subsystem, i), Some(ship)) => trf!("Choose a {slot}", slot = slot_label(&dogma, ship, Slot::Subsystem, i)),
        _ => picker.title(),
    };
    let size = egui::vec2(if cols.is_some() { 580.0 } else { 420.0 }, 460.0);
    egui::Window::new(title).id(egui::Id::new("sim_picker")).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        let edit = ui.add(egui::TextEdit::singleline(&mut picker.query).hint_text(tr!("Search by name")).desired_width(f32::INFINITY));
        // The results follow the query a pass late; drawing this pass would show the old ones.
        if edit.changed() {
            ui.ctx().request_discard("picker query");
        }
        if picker.focus {
            edit.request_focus();
            picker.focus = false;
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            chosen = picker.results.first().copied();
        }
        // Shown for every module slot: the room itself is only worked out when the list is redone.
        if matches!(target, PickTarget::Module(..)) && ship_now.is_some() {
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
                        ui.ctx().request_discard("picker filter");
                    }
                }
            });
        }
        if consumable_hold {
            let r = ui.checkbox(&mut picker.consumables, tr!("Charges & consumables only")).on_hover_text(tr!("Charges for the modules fitted or carried, nanite repair paste, and the fuel the ship or its modules burn"));
            if r.changed() {
                picker.last_query = None;
                picker.takes = None;
                picker.opened_roots = false;
                ui.ctx().request_discard("picker filter");
            }
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
            if let Some(base) = cols {
                cost_header(ui, base.is_some(), subsystem);
            }
            egui::ScrollArea::vertical().id_salt("sim_picker_tree").auto_shrink([false, false]).show(ui, |ui| {
                for g in tree.roots().to_vec() {
                    tree_rows(ui, &dogma, tree, takes, &mut picker.expanded, g, 0, &mut chosen, cols);
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
        if let Some(base) = cols {
            cost_header(ui, base.is_some(), subsystem);
        }
        egui::ScrollArea::vertical().id_salt("sim_picker_list").auto_shrink([false, false]).show_rows(ui, 24.0, count, |ui, range| {
            for i in range {
                let id = picker.results[i];
                let Some(t) = dogma.type_(id) else { continue };
                let group = dogma.group(t.group).map_or("", |g| g.name.as_str());
                let r = crate::rows::row(ui, "picker", ("sim_pick", id), 24.0, false, |ui| {
                    crate::icons::type_icon(ui, id, 18.0);
                    let extra = cols.map_or(0.0, |_| 3.0 * COST_W + ui.spacing().item_spacing.x);
                    let w = (ui.available_width() - 120.0 - extra).max(80.0);
                    crate::rows::cell(ui, w, egui::RichText::new(t.name.as_str()), "").on_hover_ui(|ui| crate::details::type_tip(ui, &dogma, id, ""));
                    let g = (ui.available_width() - extra).max(0.0);
                    crate::rows::cell(ui, g, egui::RichText::new(group).weak(), group);
                    if let Some(base) = cols {
                        cost_cells(ui, row_figures(&dogma, id, subsystem), base, subsystem);
                    }
                });
                r.context_menu(|ui| crate::compare::menu(ui, &dogma, id));
                if r.clicked() {
                    chosen = Some(id);
                }
            }
        });
    });
    if let Some(id) = chosen {
        if !stays_open(target) {
            sim.picker = None;
        }
        apply(sim, target, id, now_ms);
    } else if !open {
        sim.picker = None;
    }
}

/// What the fit uses up: every charge a fitted or carried module takes, nanite repair paste, and the
/// fuel its hull's jump drive or its modules (siege, cynos, jump portals) consume.
pub fn consumables_for(dogma: &Dogma, fit: &spai_fit::fit::Fit) -> std::collections::HashSet<TypeId> {
    let mut out = std::collections::HashSet::new();
    let modules = fit.modules.iter().map(|m| m.type_id).chain(fit.cargo.iter().map(|c| c.type_id).filter(|t| dogma.slot_of(*t).is_some()));
    let fuel = ["consumptionType", "jumpDriveConsumptionType"].iter().filter_map(|n| dogma.attr_named(n).map(|a| a.id)).collect::<Vec<_>>();
    for m in modules {
        out.extend(compatible_charges(dogma, m));
        if let Some(t) = dogma.type_(m) {
            out.extend(fuel.iter().filter_map(|a| t.attr(*a)).map(|v| v as TypeId));
        }
    }
    if let Some(t) = dogma.type_(fit.ship) {
        out.extend(fuel.iter().filter_map(|a| t.attr(*a)).map(|v| v as TypeId));
    }
    if let Some(paste) = dogma.type_named("Nanite Repair Paste") {
        out.insert(paste.id);
    }
    out
}
