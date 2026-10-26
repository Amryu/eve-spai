//! One ship of the fleet at a time, in detail: its numbers under the enemy's ewar, what it lands
//! on one target under ours, and every weapon with its charges. Each ewar module of either fleet
//! can be left out, per ship. Charges and hull modes swapped here are kept on the fleet's slot,
//! never on the fit, and can be put back to the fit's.

use std::collections::HashMap;
use std::sync::Arc;

use spai_fit::fit::{Fit, Slot, State, TargetProfile};
use spai_fit::fleet::ammo::{self, AmmoChoice};
use spai_fit::fleet::ewar::{parts_of, plan_targets, planned, EwarOn};
use spai_fit::stats::RemoteKind;
use spai_fit::fleet::sweep::disrupted;
use spai_fit::fleet::{FleetStats, SlotStats};
use spai_fit::ids::TypeId;
use spai_fit::stats::{Computed, WeaponGroup, WeaponKind};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::details::{BAD, GOOD};
use crate::{rows, text, SimUi};

/// The target: a ship of the enemy fleet by slot, or a fit open in a tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShipTarget {
    Enemy(usize),
    Fit(String),
}

/// One ewar module type on one ship of a fleet: theirs (on us) or ours (on the target).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EwarSource {
    pub theirs: bool,
    pub slot: usize,
    pub module: TypeId,
}

/// The tab's settings, kept while the app runs.
#[derive(Clone, Debug, Default)]
pub struct ShipsState {
    /// The fleet slot looked at.
    pub ship: Option<usize>,
    pub target: Option<ShipTarget>,
    /// Metres; none follows the fleet's range.
    pub distance: Option<f32>,
    /// How this ship and the target move, for this page only; none follows the two fleets'.
    pub motion: Option<crate::graphs_panel::Motion>,
    /// Every charge each weapon can load, not only the carried ones.
    pub all_charges: bool,
    /// Per fleet slot, how many fly for this page's figures, over the fleet's count.
    pub count: HashMap<usize, u32>,
    /// How many of their logistics repair the target, for this page; none is all they have.
    pub logi: Option<u32>,
    /// The boosters this page flies with, to try others; none is the fleet's own.
    pub boosts: Option<(Vec<(spai_fit::fleet::BurstKind, String)>, Vec<spai_fit::fit::BoostShortcut>)>,
    /// Where the boosts window opened, while it is open.
    pub boosts_at: Option<egui::Pos2>,
    /// Module states flown on this page only, by (theirs, fleet slot).
    pub states: HashMap<(bool, usize), Vec<(Slot, u8, State)>>,
    /// The module window open on (theirs, fleet slot).
    pub modules_open: Option<(bool, usize)>,
    /// Per ship looked at, how many of each ewar module type count where not all of them do.
    pub ewar_n: HashMap<usize, HashMap<EwarSource, u32>>,
}

/// A change to a slot, made once the view is drawn.
enum Change {
    Charge(usize, TypeId, TypeId),
    /// A weapon stack split, changed, or joined again.
    Split(usize, TypeId, Option<spai_fit::fleet::WeaponSplit>),
    Mode(usize, TypeId),
    Reset(usize),
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui) {
    egui::Frame::NONE.inner_margin(8).show(ui, |ui| body(sim, ui));
}

/// A reset that is always there, so nothing moves once there is something to reset: greyed out
/// until then.
fn reset_button(ui: &mut egui::Ui, active: bool, tip: &str) -> bool {
    ui.add_enabled_ui(active, |ui| spai_ui::widgets::icon_button(ui, egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)).inner.on_hover_text(tip).on_disabled_hover_text(tip).clicked()
}

fn name(dogma: &Dogma, id: TypeId) -> String {
    spai_fit::formats::name_of(dogma, id)
}

/// The targets on offer: the enemy fleet's ships, then the fits open in tabs.
fn targets(sim: &mut SimUi, theirs: Option<&FleetStats>, enemy_name: &str) -> Vec<(ShipTarget, String, Arc<Computed>)> {
    let mut out = Vec::new();
    for s in theirs.iter().flat_map(|t| t.slots.iter()).filter(|s| s.count > 0) {
        out.push((ShipTarget::Enemy(s.slot), trf!("{fleet}: {ship}", fleet = enemy_name, ship = s.name), Arc::new(s.computed.clone())));
    }
    let tabs: Vec<(String, String)> = sim.sessions().iter().filter(|s| s.fit.ship != 0).map(|s| (s.id.clone(), s.fit.name.clone())).collect();
    for (id, n) in tabs {
        if let Some(c) = sim.computed_of(&id) {
            out.push((ShipTarget::Fit(id), n, c));
        }
    }
    out
}

/// What a weapon group lands per second on the target, by damage type: em, thermal, kinetic,
/// explosive.
fn landed(w: &WeaponGroup, d: f64, profile: &TargetProfile) -> [f64; 4] {
    let v = &w.volley;
    let total = v.total();
    if total <= 0.0 {
        return [0.0; 4];
    }
    let per = ammo::applied(w, d, profile) / total;
    [v.em * per, v.thermal * per, v.kinetic * per, v.explosive * per]
}

fn add(a: &mut [f64; 4], b: [f64; 4]) {
    for i in 0..4 {
        a[i] += b[i];
    }
}

/// What gets through a layer of the given resonances.
fn through(d: &[f64; 4], resonance: &[f64; 4]) -> f64 {
    (0..4).map(|i| d[i] * resonance[i]).sum()
}

/// What a kill takes: each weapon group's volleys (cycle, flight, applied damage of one ship),
/// the raw repairs per second their logistics land on the target, and the volley efficiency.
struct Kill {
    volleys: Vec<(f64, f64, f64)>,
    reps: f64,
    efficiency: f32,
}

/// When repairs start after the first hit: a lock and a cycle.
const LOGI_START_S: f64 = 5.0;

/// Seconds until volleys of `ships` (each group's damage times it) break `ehp`, repairs of
/// `reps` per second healing it from `LOGI_START_S` after the first hit, never past full; none
/// within ten minutes.
fn kill_time(volleys: &[(f64, f64, f64)], ships: f64, ehp: f64, reps: f64) -> Option<f64> {
    let mut hits: Vec<(f64, f64)> = Vec::new();
    for (cycle, flight, dmg) in volleys {
        let mut t = *flight;
        while t <= 600.0 {
            hits.push((t, dmg * ships));
            t += cycle;
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let start = hits.first()?.0 + LOGI_START_S;
    let (mut hp, mut last) = (ehp, 0.0f64);
    for (t, dmg) in hits {
        if t > start {
            hp = (hp + reps * (t - last.max(start))).min(ehp);
        }
        last = t;
        hp -= dmg;
        if hp <= 0.0 {
            return Some(t);
        }
    }
    None
}

/// How far a weapon reaches: optimal and falloff, a missile's flight, a smartbomb's radius.
fn reach(w: &WeaponGroup) -> String {
    match (&w.missile, w.kind) {
        (Some(m), WeaponKind::Missile | WeaponKind::Bomb | WeaponKind::Breacher) => text::metres(m.range),
        (_, WeaponKind::Turret | WeaponKind::Drone | WeaponKind::Fighter) if w.falloff > 0.0 => format!("{} + {}", text::metres(w.optimal), text::metres(w.falloff)),
        _ => text::metres(w.optimal),
    }
}

/// What limits it on a small or fast target: a turret's tracking, a missile's explosion.
fn application(w: &WeaponGroup) -> String {
    match &w.missile {
        Some(m) if m.explosion_radius > 0.0 => trf!("{r} m, {v} m/s", r = text::num(m.explosion_radius), v = text::num(m.explosion_velocity)),
        _ if w.tracking > 0.0 => trf!("{t} tracking", t = text::num(w.tracking)),
        _ => String::new(),
    }
}

/// The table's column widths for the width there is: the charge's name takes what the figures
/// leave, and the application column goes first when it is narrow.
#[derive(Clone, Copy)]
struct Cols {
    name: f32,
    reach: f32,
    app: Option<f32>,
    dps: f32,
    target: Option<f32>,
    applied: Option<f32>,
    ehp: Option<f32>,
    ships: Option<f32>,
}

impl Cols {
    fn for_width(w: f32, has_target: bool) -> Cols {
        let (reach, dps, target) = (140.0, 104.0, 104.0);
        let figures = reach + dps + if has_target { target } else { 0.0 };
        // Short of room the figures give up to a quarter of their width before the name does.
        let k = ((w - 4.0 * rows::GAP - 130.0) / figures).clamp(0.75, 1.0);
        let mut c = Cols { name: 0.0, reach: reach * k, app: None, dps: dps * k, target: has_target.then_some(target * k), applied: None, ehp: None, ships: None };
        let mut used = figures * k + 4.0 * rows::GAP;
        // The optional columns as room allows, the most telling first; the name keeps 170.
        let extras: [(f32, fn(&mut Cols, f32)); 4] = [(64.0, |c, x| c.ships = Some(x)), (72.0, |c, x| c.applied = Some(x)), (96.0, |c, x| c.ehp = Some(x)), (120.0, |c, x| c.app = Some(x))];
        for (i, (x, set)) in extras.into_iter().enumerate() {
            if (has_target || i == 3) && w - used - x - rows::GAP >= 170.0 {
                set(&mut c, x);
                used += x + rows::GAP;
            }
        }
        c.name = (w - used).max(110.0);
        c
    }
}

/// A right-aligned heading that explains itself on hover.
fn tip_head(ui: &mut egui::Ui, w: f32, text: &str, tip: &str) {
    ui.scope(|ui| rows::head_cell(ui, w, text, true)).response.interact(egui::Sense::hover()).on_hover_text(tip);
}

/// A charge's figures against the target, with the ship's other weapons as loaded: how much of
/// its damage lands, the target's EHP against the ship's damage with it, and the ships one volley
/// kill takes.
struct OnTarget {
    dps: f64,
    applied: f64,
    ehp: f64,
    ships: Option<f64>,
}

/// A right-aligned figure, its whole text on hover when cut.
fn num(ui: &mut egui::Ui, w: f32, t: egui::RichText) {
    let full = t.text().to_owned();
    let r = ui.allocate_ui_with_layout(egui::vec2(w, rows::ROW_H - 4.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.set_min_width(w);
        ui.set_max_width(w);
        ui.add(egui::Label::new(t).truncate().selectable(false).show_tooltip_when_elided(false))
    });
    if !full.is_empty() {
        r.response.on_hover_text(full);
    }
}

fn head(ui: &mut egui::Ui, c: &Cols, first: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = rows::GAP;
        ui.add_space(rows::GAP);
        rows::head_cell(ui, c.name, first, false);
        rows::head_cell(ui, c.reach, tr!("Reach"), true);
        if let Some(a) = c.app {
            rows::head_cell(ui, a, tr!("Application"), true);
        }
        rows::head_cell(ui, c.dps, tr!("DPS / volley"), true);
        if let Some(t) = c.target {
            rows::head_cell(ui, t, tr!("On target"), true);
        }
        if let Some(x) = c.applied {
            tip_head(ui, x, tr!("Applied"), tr!("The share of its damage that lands: range, tracking or explosion against the target's distance, speed and signature"));
        }
        if let Some(x) = c.ehp {
            tip_head(ui, x, tr!("Target EHP"), tr!("The target's effective hit points against this ship's damage with this charge loaded"));
        }
        if let Some(x) = c.ships {
            tip_head(ui, x, tr!("Ships"), tr!("Ships of this kind one volley kill takes with this charge loaded, the volley efficiency counted"));
        }
    });
}

/// The figures of one weapon with one charge, in the table's columns.
fn figures(ui: &mut egui::Ui, c: &Cols, w: &WeaponGroup, on_target: Option<&OnTarget>, style: &dyn Fn(String) -> egui::RichText) {
    num(ui, c.reach, style(reach(w)));
    if let Some(a) = c.app {
        num(ui, a, style(application(w)));
    }
    num(ui, c.dps, style(format!("{} / {}", text::num(w.dps), text::num(w.volley.total()))));
    // What lands of a volley is the share of the damage per second that does.
    let Some(o) = on_target else { return };
    if let Some(t) = c.target {
        let volley = if w.dps > 0.0 { w.volley.total() * o.dps / w.dps } else { 0.0 };
        num(ui, t, style(format!("{} / {}", text::num(o.dps), text::num(volley))));
    }
    if let Some(x) = c.applied {
        num(ui, x, style(format!("{}%", text::num(o.applied * 100.0))));
    }
    if let Some(x) = c.ehp {
        num(ui, x, style(text::num(o.ehp)));
    }
    if let Some(x) = c.ships {
        num(ui, x, style(o.ships.map_or_else(|| tr!("n/a").to_owned(), |n| format!("{n:.0}"))));
    }
}

/// A name cell led by the type's icon, as the fitting screen's rows are.
fn icon_cell(ui: &mut egui::Ui, w: f32, type_id: Option<TypeId>, text: egui::RichText, full: &str) {
    ui.allocate_ui_with_layout(egui::vec2(w, rows::ROW_H - 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing.x = 6.0;
        match type_id {
            Some(t) => {
                crate::icons::type_icon(ui, t, 16.0);
            }
            None => ui.add_space(16.0),
        }
        let rest = ui.available_width();
        rows::cell(ui, rest, text, full);
    });
}

/// A remote module's strength with its unit.
fn module_amount(kind: spai_fit::stats::RemoteKind, v: f64) -> String {
    use spai_fit::stats::RemoteKind as K;
    match kind {
        K::ArmorRep | K::ShieldRep | K::HullRep => format!("{} HP/s", text::num(v)),
        K::CapTransfer | K::Neut | K::Nos => format!("{} GJ/s", text::num(v)),
        _ => crate::stats_panel::ewar_amount(kind, v),
    }
}

/// The ship's own remote modules: repairs, capacitor, ewar and tackle, each with its strength,
/// its reach and what of it lands at the distance (through the target's resistance for ewar).
fn modules(ui: &mut egui::Ui, dogma: &Dogma, s: &SlotStats, d: f64, target: Option<&Computed>, view_w: f32) {
    let mut groups: Vec<(&spai_fit::stats::RemoteModule, u32)> = Vec::new();
    for m in &s.computed.remote.modules {
        match groups.iter_mut().find(|(g, _)| g.type_id == m.type_id && g.kind == m.kind) {
            Some(g) => g.1 += 1,
            None => groups.push((m, 1)),
        }
    }
    if groups.is_empty() {
        return;
    }
    rows::heading(ui, tr!("Modules"), None, |_| {});
    // The figures give up to two fifths of their width before the name goes below its least.
    let (each, reach, at) = (120.0, 140.0, 120.0);
    let gaps = 5.0 * rows::GAP;
    let k = ((view_w - gaps - 130.0) / (each + reach + at)).clamp(0.6, 1.0);
    let (each_w, reach_w, at_w) = (each * k, reach * k, at * k);
    let name_w = (view_w - each_w - reach_w - at_w - gaps).max(110.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = rows::GAP;
        ui.add_space(rows::GAP);
        rows::head_cell(ui, name_w, tr!("Module"), false);
        rows::head_cell(ui, each_w, tr!("Each"), true);
        rows::head_cell(ui, reach_w, tr!("Reach"), true);
        rows::head_cell(ui, at_w, &trf!("At {d}", d = text::metres(d)), true);
    });
    let list = format!("fleet_ship_mods/{}", s.slot);
    for (m, n) in groups {
        let label = trf!("{n} × {module}", n = n, module = name(dogma, m.type_id));
        let resist = if m.kind.is_assist() { 1.0 } else { target.map_or(1.0, |t| t.resist.of(m.kind)) };
        let at = m.amount * spai_fit::stats::range_factor(d, m.optimal, m.falloff) * resist;
        let reach = if m.falloff > 0.0 { format!("{} + {}", text::metres(m.optimal), text::metres(m.falloff)) } else { text::metres(m.optimal) };
        rows::row(ui, &list, ("sim_fleet_ship_mod", s.slot, m.type_id, m.kind as u8), rows::ROW_H, false, |ui| {
            icon_cell(ui, name_w, Some(m.type_id), egui::RichText::new(&label), &label);
            num(ui, each_w, egui::RichText::new(module_amount(m.kind, m.amount)));
            num(ui, reach_w, egui::RichText::new(reach));
            num(ui, at_w, if at > 0.0 { egui::RichText::new(module_amount(m.kind, at)) } else { egui::RichText::new(tr!("out of reach")).weak() });
        });
    }
    ui.add_space(8.0);
}

/// A fleet's ships as sources of ewar, each module type at the number set for it: a ship with
/// a type set apart flies it as a source of its own at that number, the rest at the ship's count.
fn fleet_ewar(fleet: &FleetStats, theirs: bool, set: &HashMap<EwarSource, u32>, own: Option<usize>) -> Vec<(u32, Computed)> {
    let mut out = Vec::new();
    for x in fleet.slots.iter().filter(|x| x.count > 0) {
        let key = |m: TypeId| EwarSource { theirs, slot: x.slot, module: m };
        let mut rest = x.computed.clone();
        rest.remote.modules.retain(|m| m.kind.is_assist() || !set.contains_key(&key(m.type_id)));
        out.push((x.count, rest));
        let mut types: Vec<TypeId> = x.computed.remote.modules.iter().filter(|m| set.contains_key(&key(m.type_id))).map(|m| m.type_id).collect();
        types.dedup();
        for t in types {
            // The ship looked at always flies its own: one of its slot's at least.
            let n = if !theirs && own == Some(x.slot) { set[&key(t)].max(1) } else { set[&key(t)] };
            if n == 0 {
                continue;
            }
            let mut only = x.computed.clone();
            only.remote.modules.retain(|m| m.type_id == t);
            out.push((n, only));
        }
    }
    out
}

/// Every ewar module type of a fleet's ships, with how many fly: (source, label, count).
fn sources(dogma: &Dogma, fleet: &FleetStats, theirs: bool) -> Vec<(EwarSource, String, u32)> {
    let mut out: Vec<(EwarSource, String, u32)> = Vec::new();
    for s in fleet.slots.iter().filter(|s| s.count > 0) {
        for m in s.computed.remote.modules.iter().filter(|m| !m.kind.is_assist()) {
            let key = EwarSource { theirs, slot: s.slot, module: m.type_id };
            match out.iter_mut().find(|(k, _, _)| *k == key) {
                Some(e) => e.2 += s.count,
                None => out.push((key, trf!("{module} on {ship}", module = name(dogma, m.type_id), ship = s.name), s.count)),
            }
        }
    }
    out
}

/// Everything the view needs from the fleets, gathered before the state is borrowed.
struct Scene {
    mine: Arc<FleetStats>,
    ammo: Arc<Vec<Vec<AmmoChoice>>>,
    theirs: Option<Arc<FleetStats>>,
    offered: Vec<(ShipTarget, String, Arc<Computed>)>,
    range: f32,
    /// How each fleet moves: its share of top speed and its heading to the other.
    motions: ((f32, f32), (f32, f32)),
    /// The share of a volley that lands in practice.
    volley_efficiency: f32,
    /// The enemy's logistics: how many fly, and the share of their repairs that does any good.
    their_logi: (u32, f32),
    /// How each fleet lays its ewar, and over how many primaries.
    our_plan: (spai_fit::fleet::ewar::EwarPlan, u32),
    their_plan: (spai_fit::fleet::ewar::EwarPlan, u32),
    /// Per fleet slot, whether it flies swaps, and its split weapon stacks.
    swapped: Vec<bool>,
    splits: Vec<Vec<spai_fit::fleet::WeaponSplit>>,
}

fn gather(sim: &mut SimUi) -> Option<Scene> {
    let _t = spai_ui::timing::span("sim: ships gather");
    let mut fleet = sim.fleet.as_ref()?.fleet.clone();
    // Boosters tried here fly in a copy of the fleet: the fleet keeps its own.
    if let Some((boosters, shortcuts)) = &sim.fleet_ships.boosts {
        fleet.boosters = boosters.clone();
        fleet.boost_shortcuts = shortcuts.clone();
    }
    let mut enemy = sim.fleet.as_ref().and_then(|f| f.enemy.clone()).and_then(|id| sim.enemy_fleet(&id));
    // Module states set here fly in the copies too.
    for ((theirs, k), states) in &sim.fleet_ships.states {
        let f = if *theirs { enemy.as_mut() } else { Some(&mut fleet) };
        if let Some(slot) = f.and_then(|f| f.slots.get_mut(*k)) {
            slot.states = states.clone();
        }
    }
    let mine = sim.cached_stats(&fleet);
    let ammo = sim.cached_ammo(&mine);
    let theirs = enemy.as_ref().map(|e| sim.cached_stats(e));
    let offered = targets(sim, theirs.as_deref(), enemy.as_ref().map_or("", |e| e.name.as_str()));
    let swapped = fleet.slots.iter().map(|s| !s.charges.is_empty() || s.mode.is_some() || !s.splits.is_empty()).collect();
    let splits = fleet.slots.iter().map(|s| s.splits.clone()).collect();
    let their_plan = enemy.as_ref().map_or_else(Default::default, |e| (e.ewar_plan.clone(), e.split_fire));
    let motions = (fleet.motion, enemy.as_ref().map_or((1.0, 90.0), |e| e.motion));
        let their_logi = (
        theirs.as_ref().map_or(0, |t| t.slots.iter().filter(|x| x.computed.remote.modules.iter().any(|m| matches!(m.kind, RemoteKind::ArmorRep | RemoteKind::ShieldRep | RemoteKind::HullRep))).map(|x| x.count).sum()),
        enemy.as_ref().map_or(1.0, |e| e.logi_efficiency),
    );
    Some(Scene { mine, ammo, theirs, offered, range: fleet.range, motions, volley_efficiency: fleet.volley_efficiency, their_logi, our_plan: (fleet.ewar_plan.clone(), fleet.split_fire), their_plan, swapped, splits })
}

fn body(sim: &mut SimUi, ui: &mut egui::Ui) {
    let _t = spai_ui::timing::span("sim: ships");
    let Some(dogma) = sim.dogma.clone() else { return };
    let boosting = sim.boosting_fits();
    let library = sim.library.clone();
    let doctrine_fits: Vec<(String, String)> = sim.doctrines.iter().map(|d| (d.id.clone(), trf!("{name} (doctrine)", name = d.name))).collect();
    let fleet_boosts = sim.fleet.as_ref().map(|f| (f.fleet.boosters.clone(), f.fleet.boost_shortcuts.clone()));
    let Some(scene) = gather(sim) else {
        ui.label(egui::RichText::new(tr!("No fleet open.")).weak());
        return;
    };
    let ships: Vec<(usize, &SlotStats)> = scene.mine.slots.iter().enumerate().filter(|(_, s)| s.count > 0).collect();
    if ships.is_empty() {
        ui.label(egui::RichText::new(tr!("The fleet has no ships yet.")).weak());
        return;
    }
    let open_fit = sim.fleet_ships.modules_open.and_then(|key| page_fit(sim, &dogma, key).map(|f| (key, f)));
    let st = &mut sim.fleet_ships;
    if st.ship.is_none_or(|k| !ships.iter().any(|(_, s)| s.slot == k)) {
        st.ship = ships.first().map(|(_, s)| s.slot);
    }
    if st.target.as_ref().is_none_or(|t| !scene.offered.iter().any(|(o, _, _)| o == t)) {
        st.target = scene.offered.first().map(|(t, _, _)| t.clone());
    }
    let wide = ui.available_width() >= 720.0;
    // The ships to pick from: a column of their own where there is room, else a menu.
    if wide {
        let mut pick = None;
        egui::Panel::left("sim_fleet_ships_list").resizable(false).exact_size(210.0).frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 0, right: 8, top: 0, bottom: 0 })).show_inside(ui, |ui| {
            ui.label(egui::RichText::new(tr!("Ships")).strong());
            egui::ScrollArea::vertical().id_salt("sim_fleet_ships_pick").auto_shrink([false, false]).show(ui, |ui| {
                for (_, s) in &ships {
                    let label = trf!("{name} × {n}", name = s.name, n = s.count);
                    let r = rows::row(ui, "fleet_ships_pick", ("sim_fleet_ships_pick", s.slot), rows::ROW_H, st.ship == Some(s.slot), |ui| {
                        let w = ui.available_width();
                        icon_cell(ui, w, Some(s.ship), egui::RichText::new(&label), &trf!("{label} · {hull}", label = label, hull = name(&dogma, s.ship)));
                    });
                    if r.clicked() {
                        pick = Some(s.slot);
                    }
                }
            });
        });
        if let Some(k) = pick {
            st.ship = Some(k);
        }
    }
    let Some((pos, s)) = ships.iter().find(|(_, s)| Some(s.slot) == st.ship).copied() else { return };
    let k = s.slot;
    let mut changes: Vec<Change> = Vec::new();
    let mut distance = st.distance.unwrap_or(scene.range);
    let mut motion = st.motion.unwrap_or(crate::graphs_panel::Motion { shooter: scene.motions.0, target: scene.motions.1 });
    let ours_sources = sources(&dogma, &scene.mine, false);
    let their_sources = scene.theirs.as_ref().map(|t| sources(&dogma, t, true)).unwrap_or_default();
    // Measured outside the scroll area: inside, a row too wide would widen what it is measured by.
    let pad = if wide { 10.0 } else { 0.0 };
    let view_w = ui.available_width() - 12.0 - pad;
    let mut detail = ui.new_child(egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap().with_min_x(ui.available_rect_before_wrap().min.x + pad)));
    let ui = &mut detail;
    egui::ScrollArea::vertical().id_salt("sim_fleet_ship_detail").auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(view_w);
        // The ship: which, its mode, and the way back to its fit, on one line of fixed height so
        // what is on it never moves the rest.
        ui.allocate_ui_with_layout(egui::vec2(view_w, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_height(30.0);
            let hull = name(&dogma, s.ship);
            if !wide {
                egui::ComboBox::from_id_salt("sim_fleet_ships_pick_menu").selected_text(text::clip(&s.name, 20)).width(150.0).show_ui(ui, |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    for (_, o) in &ships {
                        if ui.menu_label(o.slot == k, trf!("{name} × {n}", name = o.name, n = o.count)).clicked() {
                            st.ship = Some(o.slot);
                        }
                    }
                });
            }
            let modes = dogma.modes_of(s.ship);
            let swapped = scene.swapped.get(k).copied().unwrap_or(false);
            let title = trf!("{name} · {hull}", name = s.name, hull = hull);
            let others = if wide { 0.0 } else { 160.0 } + if modes.is_empty() { 0.0 } else { 150.0 } + 2.0 * 30.0 + 90.0;
            let title_w = (ui.available_width() - others).max(120.0);
            ui.allocate_ui_with_layout(egui::vec2(title_w, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_width(title_w);
                ui.add(egui::Label::new(egui::RichText::new(&title).strong().size(16.0)).truncate()).on_hover_text(&title);
            });
            // How many fly, for this page only: the fleet keeps its own count.
            let mut n = st.count.get(&k).copied().unwrap_or(s.count);
            let r = ui.add(egui::DragValue::new(&mut n).range(1..=2000).prefix("× ")).on_hover_text(trf!("How many fly, for this page's figures only; the fleet flies {n}", n = s.count));
            if r.changed() {
                if n == s.count {
                    st.count.remove(&k);
                } else {
                    st.count.insert(k, n);
                }
            }
            if reset_button(ui, st.count.contains_key(&k), &trf!("The fleet's count, {n}", n = s.count)) {
                st.count.remove(&k);
            }
            if !modes.is_empty() {
                let flying = s.fit.mode.or(modes.first().copied());
                let short = |m: TypeId| {
                    let n = name(&dogma, m);
                    n.strip_prefix(hull.as_str()).map_or(n.clone(), |r| r.trim().to_owned())
                };
                egui::ComboBox::from_id_salt(("sim_fleet_ship_mode", k)).selected_text(flying.map(short).unwrap_or_default()).width(130.0).show_ui(ui, |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    for m in &modes {
                        if ui.menu_label(flying == Some(*m), short(*m)).clicked() && flying != Some(*m) {
                            changes.push(Change::Mode(k, *m));
                        }
                    }
                });
            }
            if reset_button(ui, swapped, tr!("Its fit's charges and mode again")) {
                changes.push(Change::Reset(k));
            }
        });
        // What it fights: the target, how far, how fast, and whose ewar counts.
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            if scene.offered.is_empty() {
                ui.label(egui::RichText::new(tr!("No target: pick a fleet under Against, or open a fit in a tab.")).weak());
            } else {
                ui.label(tr!("Target"));
                let current = scene.offered.iter().find(|(t, _, _)| Some(t) == st.target.as_ref()).map(|(_, n, _)| n.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("sim_fleet_ships_target").selected_text(text::clip(&current, 30)).width(220.0).show_ui(ui, |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    for (t, n, _) in &scene.offered {
                        if ui.menu_label(st.target.as_ref() == Some(t), text::clip(n, 44)).clicked() {
                            st.target = Some(t.clone());
                        }
                    }
                });
                ui.label(tr!("at"));
                let r = ui.add(egui::DragValue::new(&mut distance).range(0.0..=300_000.0).speed(100.0).custom_formatter(|v, _| text::metres(v)).custom_parser(|s| crate::graphs_panel::leading_number(s).map(|v| if s.contains("km") { v * 1000.0 } else { v })));
                if r.changed() {
                    st.distance = Some(distance);
                }
                if reset_button(ui, st.distance.is_some(), tr!("Follow the fleet's range")) {
                    st.distance = None;
                    distance = scene.range;
                }
            }
            ewar_menu(ui, st, k, &ours_sources, &their_sources);
            // Boosters to try on this page, as the fleet sets them.
            let tried = st.boosts.is_some();
            let label = if tried { format!("{} {}", egui_phosphor::regular::LIGHTNING, tr!("Boosts, tried here")) } else { format!("{} {}", egui_phosphor::regular::LIGHTNING, tr!("Boosts")) };
            // A window, not a menu: the dropdowns inside open popups of their own, which a menu
            // counts as clicks outside and closes on.
            let r = ui.add(egui::Button::new(label).selected(st.boosts_at.is_some()));
            if r.clicked() {
                st.boosts_at = if st.boosts_at.is_some() { None } else { Some(r.rect.left_bottom() + egui::vec2(0.0, 4.0)) };
            }
            if let Some(at) = st.boosts_at {
                let mut open = true;
                egui::Window::new(tr!("Boosts"))
                    .id(egui::Id::new("sim_fleet_ships_boosts"))
                    .open(&mut open)
                    .default_pos(at)
                    .resizable(false)
                    .collapsible(false)
                    .show(ui.ctx(), |ui| {
                        // Fixed: content sized from what it is given would grow the window each frame.
                        ui.set_width(460.0);
                        ui.label(egui::RichText::new(tr!("For this page only: the fleet keeps its own boosters.")).weak());
                        let (mut boosters, mut shortcuts) = st.boosts.clone().or_else(|| fleet_boosts.clone()).unwrap_or_default();
                        let (changed, _) = crate::fleet_panel::boosters_ui(ui, &mut boosters, &mut shortcuts, &boosting, Some(&dogma), &library, &doctrine_fits, "sim_fleet_ships_booster");
                        if changed {
                            st.boosts = Some((boosters, shortcuts));
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if reset_button(ui, tried, tr!("The fleet's boosters")) {
                                st.boosts = None;
                            }
                            ui.label(egui::RichText::new(tr!("The fleet's boosters")).weak());
                        });
                    });
                if !open {
                    st.boosts_at = None;
                }
            }
            if scene.their_logi.0 > 0 {
                ui.allocate_ui_with_layout(egui::vec2(200.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(tr!("Their logi"));
                    let mut n = st.logi.unwrap_or(scene.their_logi.0);
                    let r = ui.add(egui::DragValue::new(&mut n).range(0..=2000).suffix(trf!(" of {n}", n = scene.their_logi.0)));
                    if r.on_hover_text(tr!("How many of their logistics repair the target, for this page's kill times")).changed() {
                        st.logi = (n != scene.their_logi.0).then_some(n);
                    }
                    if reset_button(ui, st.logi.is_some(), tr!("All their logistics")) {
                        st.logi = None;
                    }
                });
            }
        });
        // How the two move: each circle its top speed, the point how much and which way, 90
        // degrees across the line between them. This page's own, the fleets untouched.
        let dials = ui.allocate_ui_with_layout(egui::vec2(view_w, 124.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let before = motion;
            crate::graphs_panel::motion_dial(ui, tr!("This ship"), &mut motion.shooter);
            crate::graphs_panel::motion_dial(ui, tr!("Target"), &mut motion.target);
            let changed = motion != before;
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr!("Motion")).strong());
                    if reset_button(ui, st.motion.is_some(), tr!("Back to how the two fleets are set to move")) {
                        st.motion = None;
                        motion = crate::graphs_panel::Motion { shooter: scene.motions.0, target: scene.motions.1 };
                    }
                });
                ui.add(egui::Label::new(egui::RichText::new(tr!("How this ship and the target move, for this page; the fleets keep their own.")).weak()).wrap());
            });
            changed
        });
        if dials.inner {
            st.motion = Some(motion);
        }
        ui.add_space(4.0);
        let d = f64::from(distance);
        let set = st.ewar_n.get(&k).cloned().unwrap_or_default();
        let target = scene.offered.iter().find(|(t, _, _)| Some(t) == st.target.as_ref()).map(|(_, _, c)| c.clone());
        // Each fleet's ewar with the modules left out taken off its ships.
        let ours = fleet_ewar(&scene.mine, false, &set, Some(k));
        let theirs = scene.theirs.as_ref().map(|t| fleet_ewar(t, true, &set, None)).unwrap_or_default();
        let our_count: u32 = scene.mine.slots.iter().map(|x| x.count).sum();
        let their_count: u32 = scene.theirs.as_ref().map_or(1, |t| t.ships.max(1));
        let on_target = target.as_ref().map_or_else(EwarOn::default, |t| planned(&ours.iter().map(|(n, c)| (*n, c)).collect::<Vec<_>>(), d, &scene.our_plan.0, scene.our_plan.1, their_count, Some(t)));
        // Each side's ewar as its fleet lays it: focused on what it shoots, or spread.
        let their_ships: Vec<(u32, &Computed)> = theirs.iter().map(|(n, c)| (*n, c)).collect();
        let on_us = planned(&their_ships, d, &scene.their_plan.0, scene.their_plan.1, our_count, Some(&s.computed));
        // The target at its share of its webbed top speed, the speed across the line of fire what
        // both ships' headings leave, this ship's own speed under their webs.
        let profile = target.as_ref().map(|t| {
            let base = TargetProfile { signature: (t.mobility.signature * on_target.signature) as f32, velocity: (t.mobility.max_velocity * on_target.speed) as f32, transversal: 0.0, distance, radius: 0.0, resist: [0.0; 4] };
            motion.apply(&base, s.computed.mobility.max_velocity * on_us.speed)
        });
        let layers: Option<[&spai_fit::stats::Layer; 3]> = target.as_ref().map(|t| [&t.tank.shield, &t.tank.armor, &t.tank.hull]);
        let resonance = layers.and_then(|l| l.iter().find(|l| l.hp > 0.0).map(|l| l.resonance)).unwrap_or([1.0; 4]);
        let choices: &[AmmoChoice] = scene.ammo.get(pos).map(|v| v.as_slice()).unwrap_or(&[]);
        let weapons: Vec<WeaponGroup> = s.computed.weapons.iter().map(|w| disrupted(w, &on_us)).collect();
        let hit = |w: &WeaponGroup| profile.as_ref().map_or([0.0; 4], |p| landed(w, d, p));
        let mut now = [0.0f64; 4];
        for w in &weapons {
            add(&mut now, hit(w));
        }
        // One volley of the weapons that fire together, as listed and as it lands.
        let volley = weapons.iter().filter(|w| w.alpha()).fold((0.0, 0.0, 0.0), |(raw, applied, on), w| {
            let h = hit(w);
            let share = |v: f64| if w.dps > 0.0 { v / w.dps } else { 0.0 };
            (raw + w.volley.total(), applied + w.volley.total() * share(h.iter().sum()), on + w.volley.total() * share(through(&h, &resonance)))
        });
        // A group's figures on the target when it stands in for `instead`, the rest as loaded.
        let eff = f64::from(scene.volley_efficiency.clamp(0.01, 1.0));
        let on_target_with = |g: &WeaponGroup, instead: Option<&WeaponGroup>| -> Option<OnTarget> {
            let t = target.as_ref()?;
            let h = hit(g);
            let mut mix = now;
            let mut ship_volley = volley.1;
            let applied_volley = |w: &WeaponGroup, h: &[f64; 4]| if w.alpha() && w.dps > 0.0 { w.volley.total() * h.iter().sum::<f64>() / w.dps } else { 0.0 };
            if let Some(old) = instead {
                let oh = hit(old);
                for i in 0..4 {
                    mix[i] += h[i] - oh[i];
                }
                ship_volley += applied_volley(g, &h) - applied_volley(old, &oh);
            }
            let ehp = spai_fit::fleet::sweep::ehp_against(&t.tank, &spai_fit::stats::Damage { em: mix[0], thermal: mix[1], kinetic: mix[2], explosive: mix[3] });
            Some(OnTarget {
                dps: through(&h, &resonance),
                applied: if g.dps > 0.0 { h.iter().sum::<f64>() / g.dps } else { 0.0 },
                ehp,
                ships: (ship_volley > 0.0).then(|| (ehp / (ship_volley * eff)).ceil()),
            })
        };
        // Each side's ewar on its ship module by module, for the tooltips: over as many ships as
        // the side's plan shares the kind.
        let our_ships: Vec<(u32, &Computed)> = ours.iter().map(|(n, c)| (*n, c)).collect();
        let their_parts = |kind: RemoteKind, part: usize| parts_of(&their_ships, d, plan_targets(&scene.their_plan.0, kind, scene.their_plan.1, our_count), Some(&s.computed), kind, part);
        let our_parts = |kind: RemoteKind, part: usize| parts_of(&our_ships, d, plan_targets(&scene.our_plan.0, kind, scene.our_plan.1, their_count), target.as_deref(), kind, part);
        let flying_n = st.count.get(&k).copied().unwrap_or(s.count);
        // Their logistics on the target: that many of theirs, each slot shared out alike, what the
        // game lets land of it on one ship, the share that does good, what the hull takes in.
        let logi_n = st.logi.unwrap_or(scene.their_logi.0);
        let reps = match (&scene.theirs, &target) {
            (Some(th), Some(t)) if scene.their_logi.0 > 0 => {
                let frac = f64::from(logi_n) / f64::from(scene.their_logi.0);
                let landing = spai_fit::fleet::reps_on_one(th, &|_, x| (f64::from(x.count) * frac).round() as u32);
                landing.iter().sum::<f64>() * f64::from(scene.their_logi.1.clamp(0.0, 1.0)) * t.rep_intake
            }
            _ => 0.0,
        };
        // Each weapon group's volleys: every cycle, landing after its flight, its applied damage.
        let volleys: Vec<(f64, f64, f64)> = weapons
            .iter()
            .filter(|w| w.cycle_s > 0.0 && w.dps > 0.0)
            .map(|w| {
                let flight = w.missile.as_ref().filter(|m| m.velocity > 0.0).map_or(0.0, |m| d / m.velocity);
                (w.cycle_s, flight, w.volley.total() * hit(w).iter().sum::<f64>() / w.dps)
            })
            .collect();
        let kill = Kill { volleys, reps, efficiency: scene.volley_efficiency };
        // The two cards: this ship under their ewar, and the target under ours.
        let side_by_side = view_w >= 640.0;
        let card_w = if side_by_side { (view_w - 12.0) / 2.0 } else { view_w };
        // The module window's buttons, on the cards' title rows: this ship, and an enemy target.
        let toggle = std::cell::Cell::new(None);
        let modules_button = |ui: &mut egui::Ui, key: (bool, usize)| {
            let on = st.modules_open == Some(key);
            let changed = st.states.get(&key).is_some_and(|v| !v.is_empty());
            let label = format!("{} {}", egui_phosphor::regular::TOGGLE_RIGHT, if changed { tr!("Modules, changed here") } else { tr!("Modules") });
            if ui.add(egui::Button::new(label).selected(on)).on_hover_text(tr!("Switch modules on or off for this page")).clicked() {
                toggle.set(Some(key));
            }
        };
        let target_key = match &st.target {
            Some(ShipTarget::Enemy(e)) => Some((true, *e)),
            _ => None,
        };
        let ship = |ui: &mut egui::Ui| card_with(ui, card_w, tr!("This ship"), |ui| modules_button(ui, (false, k)), |ui| ship_card(ui, &dogma, s, &on_us, &their_parts, target.as_deref(), &profile));
        let on = |ui: &mut egui::Ui| {
            card_with(ui, card_w, tr!("On the target"), |ui| { if let Some(key) = target_key { modules_button(ui, key) } }, |ui| match (&target, &profile, layers) {
                (Some(t), Some(p), Some(_)) => target_card(ui, &dogma, t, p, &on_target, &our_parts, &weapons, &now, &resonance, flying_n, volley, &kill),
                _ => {
                    ui.label(egui::RichText::new(tr!("No target.")).weak());
                }
            })
        };
        if side_by_side {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ship(ui);
                on(ui);
            });
        } else {
            ship(ui);
            ui.add_space(8.0);
            on(ui);
        }
        if let Some(key) = toggle.take() {
            st.modules_open = if st.modules_open == Some(key) { None } else { Some(key) };
        }
        if let Some((key, (fit_name, base, now))) = &open_fit {
            if st.modules_open == Some(*key) {
                modules_window(ui.ctx(), &dogma, st, *key, fit_name, base, now);
            }
        }
        ui.add_space(8.0);
        // Support ships lead with what they are flown for: their repairs, capacitor and ewar.
        use spai_fit::fleet::Role;
        let support = matches!(s.role, Role::Logi | Role::Support | Role::Tackle | Role::Ewar | Role::Command);
        if support {
            modules(ui, &dogma, s, d, target.as_deref(), view_w);
        }
        if !weapons.is_empty() {
            rows::heading(ui, tr!("Weapons"), None, |ui| {
                ui.checkbox(&mut st.all_charges, tr!("All charges")).on_hover_text(tr!("Every charge the weapons can load, not only the carried ones"));
            });
        }
        // The weapons, a stack of each type, each with its charges; a stack split in two shows
        // each part with its own.
        let cols = Cols::for_width(view_w - 4.0, profile.is_some());
        let mut seen: Vec<TypeId> = Vec::new();
        for w in &weapons {
            if seen.contains(&w.type_id) {
                continue;
            }
            seen.push(w.type_id);
            let stack: Vec<&WeaponGroup> = weapons.iter().filter(|x| x.type_id == w.type_id).collect();
            let total: u32 = stack.iter().map(|x| x.count).sum();
            let choice = choices.iter().find(|c| c.weapon == w.type_id);
            let split = scene.splits.get(k).and_then(|v| v.iter().find(|x| x.weapon == w.type_id)).copied();
            let can_split = choice.is_some() && total >= 2 && spai_fit::fleet::splittable(w.kind);
            let title = match split {
                Some(sp) => {
                    let [a, b] = sp.counts(total);
                    trf!("{n} × {weapon} · split {a} + {b}", n = total, weapon = name(&dogma, w.type_id), a = a, b = b)
                }
                None => trf!("{n} × {weapon}", n = total, weapon = name(&dogma, w.type_id)),
            };
            // The title takes what the buttons leave, so a long one is cut, not the buttons.
            let buttons_w = match split {
                // Join and the two target buttons, with the spacing between them.
                Some(_) => 2.0 * 104.0 + 70.0 + 4.0 * 8.0,
                None if can_split => 100.0,
                None => 0.0,
            };
            ui.allocate_ui_with_layout(egui::vec2(view_w, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(rows::GAP);
                crate::icons::type_icon(ui, w.type_id, 20.0);
                let title_w = (ui.available_width() - buttons_w).max(80.0);
                ui.allocate_ui_with_layout(egui::vec2(title_w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_width(title_w);
                    ui.add(egui::Label::new(egui::RichText::new(&title).strong()).truncate().selectable(false)).on_hover_text(&title);
                });
                // Left to right in a region of their own: the sized toggles place themselves at
                // the cursor's left edge, which a right-to-left layout leaves at minus infinity.
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    ui.add_space((ui.available_width() - buttons_w).max(0.0));
                    match split {
                        Some(sp) => {
                            let size = egui::vec2(104.0, 22.0);
                            if ui.menu_label_sized(size, !sp.apart, tr!("One target")).on_hover_text(tr!("Both parts on the same target")).clicked() && sp.apart {
                                changes.push(Change::Split(k, w.type_id, Some(spai_fit::fleet::WeaponSplit { apart: false, ..sp })));
                            }
                            if ui.menu_label_sized(size, sp.apart, tr!("Two targets")).on_hover_text(tr!("Each part on a target of its own: each must kill its own in one volley")).clicked() && !sp.apart {
                                changes.push(Change::Split(k, w.type_id, Some(spai_fit::fleet::WeaponSplit { apart: true, ..sp })));
                            }
                            if ui.button(tr!("Join")).on_hover_text(tr!("One stack again, all on the first part's charge")).clicked() {
                                changes.push(Change::Split(k, w.type_id, None));
                                changes.push(Change::Charge(k, w.type_id, sp.charges[0]));
                            }
                        }
                        None if can_split => {
                            if ui.button(format!("{} {}", egui_phosphor::regular::ARROWS_SPLIT, tr!("Split"))).on_hover_text(tr!("Split the stack in two, each part with its own charge")).clicked() {
                                let loaded = choice.and_then(|c| c.loaded).or(w.charge).unwrap_or_default();
                                changes.push(Change::Split(k, w.type_id, Some(spai_fit::fleet::WeaponSplit { weapon: w.type_id, first: (total / 2) as u8, charges: [loaded, loaded], apart: false })));
                            }
                        }
                        None => {}
                    }
                });
            });
            let list = format!("fleet_ship/{k}/{}", w.type_id);
            let Some(c) = choice else {
                head(ui, &cols, tr!("Charge"));
                for g in &stack {
                    let on = on_target_with(g, Some(g));
                    rows::row(ui, &list, ("sim_fleet_ship_plain", k, g.type_id, g.charge), rows::ROW_H, true, |ui| {
                        let label = g.charge.map(|c| name(&dogma, c)).unwrap_or_else(|| tr!("No charge").to_owned());
                        icon_cell(ui, cols.name, g.charge, egui::RichText::new(&label), &label);
                        figures(ui, &cols, g, on.as_ref(), &|t| egui::RichText::new(t));
                    });
                }
                ui.add_space(6.0);
                continue;
            };
            // The parts to list: the whole stack, or each part of a split with its modules.
            let parts: Vec<(Option<usize>, u32, Option<TypeId>)> = match split {
                Some(sp) => {
                    let n = sp.counts(total);
                    vec![(Some(0), n[0], Some(sp.charges[0])), (Some(1), n[1], Some(sp.charges[1]))]
                }
                None => vec![(None, total, c.loaded.or(w.charge))],
            };
            for (part, n, loaded) in parts {
                if let (Some(p), Some(sp)) = (part, split) {
                    ui.allocate_ui_with_layout(egui::vec2(view_w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add_space(rows::GAP + 26.0);
                        ui.label(egui::RichText::new(if p == 0 { tr!("First part") } else { tr!("Second part") }).strong());
                        if p == 0 {
                            let mut first = u32::from(sp.first);
                            if ui.add(egui::DragValue::new(&mut first).range(1..=total - 1).suffix(trf!(" of {n}", n = total))).changed() {
                                changes.push(Change::Split(k, w.type_id, Some(spai_fit::fleet::WeaponSplit { first: first as u8, ..sp })));
                            }
                        } else {
                            ui.label(egui::RichText::new(trf!("{n} of {total}", n = n, total = total)).weak());
                        }
                    });
                }
                head(ui, &cols, tr!("Charge"));
                // A list per part: a part's heading sits between them.
                let part_list = format!("{list}/{part:?}");
                // A part's figures are its share of the whole stack's.
                let share = |g: &WeaponGroup| {
                    let mut g = disrupted(g, &on_us);
                    let f = f64::from(n) / f64::from(total.max(1));
                    g.dps *= f;
                    g.volley = g.volley.scaled(f);
                    g.count = n;
                    g
                };
                let mut options: Vec<&(TypeId, bool, WeaponGroup)> = c.options.iter().filter(|(ch, carried, _)| st.all_charges || *carried || loaded == Some(*ch)).collect();
                options.sort_by(|a, b| b.2.reach().total_cmp(&a.2.reach()));
                // What this part flies now, to stand each charge in for.
                let current = c.options.iter().find(|(ch, _, _)| loaded == Some(*ch)).map(|(_, _, ow)| share(ow));
                let best = options.iter().map(|(ch, _, ow)| (*ch, through(&hit(&share(ow)), &resonance))).max_by(|a, b| a.1.total_cmp(&b.1)).filter(|_| profile.is_some()).map(|(ch, _)| ch);
                for (ch, carried, ow) in options {
                    let ow = share(ow);
                    let is = loaded == Some(*ch);
                    let style = |t: String| {
                        let r = egui::RichText::new(t);
                        if is {
                            r.strong()
                        } else if *carried {
                            r
                        } else {
                            r.weak()
                        }
                    };
                    let on = on_target_with(&ow, current.as_ref());
                    let r = rows::row(ui, &part_list, ("sim_fleet_ship_o", k, w.type_id, part, *ch), rows::ROW_H, is, |ui| {
                        let label = name(&dogma, *ch);
                        let shown = if best == Some(*ch) { format!("{label}  {}", egui_phosphor::regular::STAR) } else { label.clone() };
                        icon_cell(ui, cols.name, Some(*ch), style(shown), &label);
                        figures(ui, &cols, &ow, on.as_ref(), &style);
                    });
                    let r = r.on_hover_text(match (is, *carried, best == Some(*ch)) {
                        (true, _, _) => tr!("Loaded"),
                        (false, true, true) => tr!("Carried, and the best on the target here. Click to fly with it"),
                        (false, true, false) => tr!("Carried. Click to fly with it"),
                        (false, false, true) => tr!("Not carried, and the best on the target here. Click to fly with it"),
                        (false, false, false) => tr!("Not carried. Click to fly with it"),
                    });
                    if r.clicked() && !is {
                        match (part, split) {
                            (Some(p), Some(sp)) => {
                                let mut charges = sp.charges;
                                charges[p] = *ch;
                                changes.push(Change::Split(k, w.type_id, Some(spai_fit::fleet::WeaponSplit { charges, ..sp })));
                            }
                            _ => changes.push(Change::Charge(k, w.type_id, *ch)),
                        }
                    }
                }
                ui.add_space(6.0);
            }
        }
        if !support {
            modules(ui, &dogma, s, d, target.as_deref(), view_w);
        }
        if weapons.is_empty() && s.computed.remote.modules.is_empty() {
            ui.label(egui::RichText::new(tr!("No weapons.")).weak());
        }
    });
    apply(sim, &dogma, changes);
}

/// The fit a fleet slot flies on this page, as (ship's name, the fleet's, with this page's
/// module states): ours, or (`theirs`) the enemy fleet's.
fn page_fit(sim: &SimUi, dogma: &Dogma, (theirs, k): (bool, usize)) -> Option<(String, Fit, Fit)> {
    let fleet = if theirs { sim.fleet.as_ref().and_then(|f| f.enemy.clone()).and_then(|id| sim.enemy_fleet(&id))? } else { sim.fleet.as_ref()?.fleet.clone() };
    let slot = fleet.slots.get(k)?;
    let doc = sim.fit_docs.get(&slot.fit_id)?;
    let mut plain = slot.clone();
    plain.states.clear();
    let base = plain.adjusted(spai_fit::fleet::flown(dogma, doc, slot.variation));
    let mut now = base.clone();
    for (sl, i, state) in sim.fleet_ships.states.get(&(theirs, k)).into_iter().flatten() {
        if let Some(m) = now.module_mut(*sl, *i) {
            m.state = *state;
        }
    }
    Some((doc.name.clone(), base, now))
}

/// A small fitting window: each module that switches, its state to set for this page only.
fn modules_window(ctx: &egui::Context, dogma: &Dogma, st: &mut ShipsState, key: (bool, usize), ship: &str, base: &Fit, now: &Fit) {
    use crate::editor::{gesture_state, state_icon, state_name, states_of, Gesture};
    let mut open = true;
    let mut set: Option<(Slot, u8, State)> = None;
    let mut reset = false;
    egui::Window::new(trf!("Modules · {name}", name = ship)).id(egui::Id::new("sim_fleet_ships_modules")).default_pos(ctx.content_rect().center() - egui::vec2(170.0, 120.0)).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.set_min_width(320.0);
        ui.label(egui::RichText::new(tr!("This page only")).weak());
        let mut mods: Vec<&spai_fit::fit::Module> = now.modules.iter().filter(|m| states_of(dogma, m.type_id).len() > 1).collect();
        mods.sort_by_key(|m| (m.slot, m.index));
        let mut rack = None;
        for m in mods {
            // A heading over each rack, as on the fitting screen.
            if rack != Some(m.slot) {
                rack = Some(m.slot);
                let title = match m.slot {
                    Slot::High => tr!("High slots"),
                    Slot::Mid => tr!("Mid slots"),
                    Slot::Low => tr!("Low slots"),
                    Slot::Rig => tr!("Rigs"),
                    Slot::Subsystem => tr!("Subsystems"),
                    Slot::Service => tr!("Service slots"),
                };
                rows::heading(ui, title, None, |_| {});
            }
            let changed = base.module(m.slot, m.index).is_some_and(|b| b.state != m.state);
            rows::row(ui, &format!("fleet_ship_modules/{:?}", m.slot), ("sim_fleet_ship_mod", m.slot, m.index), rows::ROW_H, false, |ui| {
                let glyph = match m.state {
                    State::Overload => egui::RichText::new(state_icon(m.state)).color(ui.visuals().error_fg_color),
                    State::Active => egui::RichText::new(state_icon(m.state)).color(egui::Color32::from_rgb(0x7F, 0xC9, 0x8A)),
                    _ => egui::RichText::new(state_icon(m.state)),
                };
                let b = spai_ui::widgets::icon_button(ui, glyph).on_hover_text(trf!("{state}. Click: online or active. Right-click: overload. Ctrl-click: offline.", state = state_name(m.state)));
                let ctrl = ui.input(|i| i.modifiers.command);
                if b.clicked() {
                    set = Some((m.slot, m.index, gesture_state(dogma, m.type_id, m.state, if ctrl { Gesture::Offline } else { Gesture::Toggle })));
                }
                if b.secondary_clicked() {
                    set = Some((m.slot, m.index, gesture_state(dogma, m.type_id, m.state, Gesture::Overload)));
                }
                crate::icons::type_icon(ui, m.type_id, 18.0);
                let label = egui::RichText::new(name(dogma, m.type_id));
                let label = if m.state == State::Offline { label.weak() } else if changed { label.color(ui.visuals().warn_fg_color) } else { label };
                let w = ui.available_width();
                rows::cell(ui, w, label, "");
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if reset_button(ui, st.states.get(&key).is_some_and(|v| !v.is_empty()), tr!("Reset to the fit")) {
                reset = true;
            }
            ui.label(egui::RichText::new(tr!("Reset to the fit")).weak());
        });
    });
    let list = st.states.entry(key).or_default();
    if reset {
        list.clear();
    }
    if let Some(set) = set {
        *list = page_states_after(dogma, base, now, set);
    }
    if !open {
        st.modules_open = None;
    }
}

/// This page's module states once one is set: the differences from the fit, the one just set
/// among them, and whatever a group limit then holds back offline, as the game would.
pub(crate) fn page_states_after(dogma: &Dogma, base: &Fit, now: &Fit, (slot, index, state): (Slot, u8, State)) -> Vec<(Slot, u8, State)> {
    let mut next = now.clone();
    if let Some(m) = next.module_mut(slot, index) {
        m.state = state;
    }
    let mut keep = Some((slot, index));
    for _ in 0..4 {
        let shown = next.clone();
        let calc = spai_fit::calc::Calc::new(dogma, &shown, &spai_fit::skills::SkillSet::all_v(), &spai_fit::calc::Env::default());
        if !spai_fit::calc::yield_to_limits(dogma, &mut next, &shown, calc.capped(), keep.take()) {
            break;
        }
    }
    let mut out: Vec<(Slot, u8, State)> = next.modules.iter().filter(|m| base.module(m.slot, m.index).is_some_and(|b| b.state != m.state)).map(|m| (m.slot, m.index, m.state)).collect();
    out.sort_by_key(|(s, i, _)| (*s, *i));
    out
}

/// A titled box of fixed width, `head` at the right of its title row.
fn card_with(ui: &mut egui::Ui, w: f32, title: &str, head: impl FnOnce(&mut egui::Ui), body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.fill(ui.visuals().faint_bg_color).corner_radius(4.0).inner_margin(8).show(ui, |ui| {
        ui.set_width(w - 16.0);
        ui.vertical(|ui| {
            ui.set_width(w - 16.0);
            // A fixed height, so the button does not change the row's.
            ui.allocate_ui_with_layout(egui::vec2(w - 16.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_size(egui::vec2(w - 16.0, 24.0));
                ui.label(egui::RichText::new(title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), head);
            });
            ui.add_space(2.0);
            body(ui);
        });
    });
}


/// A figure changed by ewar: the plain one, then what it becomes, coloured by whether that is
/// better or worse for us.
fn changed(v: f64, f: f64, unit: &str, higher_is_good: bool) -> (String, Option<egui::Color32>) {
    if (f - 1.0).abs() < 0.005 {
        return (format!("{} {unit}", text::num(v)), None);
    }
    let good = (f > 1.0) == higher_is_good;
    (format!("{} → {} {unit}", text::num(v), text::num(v * f)), Some(if good { GOOD } else { BAD }))
}

fn grid_row(ui: &mut egui::Ui, label: &str, (value, color): (String, Option<egui::Color32>)) {
    crate::stats_panel::line_colored(ui, label, value, color);
}

fn plain(v: String) -> (String, Option<egui::Color32>) {
    (v, None)
}

/// This ship as the enemy's ewar leaves it.
fn ship_card(ui: &mut egui::Ui, dogma: &Dogma, s: &SlotStats, e: &EwarOn, theirs: &dyn Fn(RemoteKind, usize) -> Vec<(TypeId, f64, f64)>, target: Option<&Computed>, profile: &Option<TargetProfile>) {
    let c = &s.computed;
    let speed = c.mobility.max_velocity;
    tip_row(ui, tr!("Speed"), changed(speed, e.speed, "m/s", true), |ui| {
        ewar_tip(ui, dogma, tr!("Top speed"), &theirs(RemoteKind::Web, 0), &format!("{} m/s", text::num(speed)), &format!("{} m/s", text::num(speed * e.speed)), tr!("Their webs on this ship, the strongest first, each past the first stacking-penalised."));
    });
    let sig = c.mobility.signature;
    tip_row(ui, tr!("Signature"), changed(sig, e.signature, "m", false), |ui| {
        ewar_tip(ui, dogma, tr!("Signature"), &theirs(RemoteKind::Painter, 0), &format!("{} m", text::num(sig)), &format!("{} m", text::num(sig * e.signature)), tr!("Their painters stack with this ship's own signature modifiers, a running microwarpdrive first."));
    });
    let lock = c.sensors.lock_range / 1000.0;
    tip_row(ui, tr!("Lock range"), changed(lock, e.lock_range, "km", true), |ui| {
        ewar_tip(ui, dogma, tr!("Lock range"), &theirs(RemoteKind::Damp, 0), &format!("{} km", text::num(lock)), &format!("{} km", text::num(lock * e.lock_range)), tr!("Their dampeners on this ship."));
    });
    let scan = c.sensors.scan_resolution;
    tip_row(ui, tr!("Scan resolution"), changed(scan, e.scan_resolution, "mm", true), |ui| {
        ewar_tip(ui, dogma, tr!("Scan resolution"), &theirs(RemoteKind::Damp, 1), &format!("{} mm", text::num(scan)), &format!("{} mm", text::num(scan * e.scan_resolution)), tr!("Their dampeners on this ship."));
    });
    if let (Some(_), Some(p)) = (target, profile) {
        let t = spai_fit::stats::lock_time(scan * e.scan_resolution, f64::from(p.signature));
        grid_row(ui, tr!("Locks the target in"), plain(text::secs(t)));
    }
    let cap = match (c.cap.stable_at, c.cap.lasts_s) {
        (Some(p), _) => trf!("stable at {p}%", p = text::num(p * 100.0)),
        (None, Some(t)) => trf!("lasts {t}", t = text::secs(t)),
        _ => "-".to_owned(),
    };
    grid_row(ui, tr!("Capacitor"), plain(cap));
    if e.neut > 0.0 {
        let regen = c.cap.peak_regen;
        let v = trf!("{n} GJ/s of {r} at peak", n = text::num(e.neut), r = text::num(regen));
        tip_row(ui, tr!("Neutralised"), (v, (e.neut >= regen).then_some(BAD)), |ui| {
            let rows: Vec<(String, String)> = theirs(RemoteKind::Neut, 0)
                .iter()
                .map(|(t, n, _)| (trf!("{n} × {module}", n = format!("{:.1}", n), module = name(dogma, *t)), String::new()))
                .chain([(tr!("Drained per second").to_owned(), format!("{} GJ/s", text::num(e.neut))), (tr!("Recharge at its peak").to_owned(), format!("{} GJ/s", text::num(regen)))])
                .collect();
            breakdown(ui, tr!("Neutralised"), &rows, tr!("Drained faster than it recharges at its best, the capacitor runs dry."));
        });
    }
    grid_row(ui, tr!("EHP"), plain(text::num(c.tank.ehp)));
    ui.add_space(2.0);
    crate::stats_panel::resist_table(ui, &c.tank);
    ui.add_space(2.0);
    let held: Vec<&str> = [(e.scrammed, tr!("scrammed")), (!e.scrammed && e.pointed, tr!("pointed")), (e.webbed, tr!("webbed"))].into_iter().filter(|(on, _)| *on).map(|(_, t)| t).collect();
    if !held.is_empty() {
        grid_row(ui, tr!("Held"), (held.join(", "), Some(BAD)));
    }
}

/// The target as our ewar leaves it, and what this ship and its slot land on it: how much of the
/// listed damage lands (application: range, speed, signature) apart from what gets through its
/// resistances.
#[allow(clippy::too_many_arguments)]
fn target_card(ui: &mut egui::Ui, dogma: &Dogma, t: &Computed, p: &TargetProfile, e: &EwarOn, ours: &dyn Fn(RemoteKind, usize) -> Vec<(TypeId, f64, f64)>, weapons: &[WeaponGroup], now: &[f64; 4], resonance: &[f64; 4], count: u32, volley: (f64, f64, f64), kill: &Kill) {
    let efficiency = kill.efficiency;
    tip_row(ui, tr!("Signature"), changed(t.mobility.signature, e.signature, "m", true), |ui| {
        ewar_tip(ui, dogma, tr!("Signature"), &ours(RemoteKind::Painter, 0), &format!("{} m", text::num(t.mobility.signature)), &format!("{} m", text::num(t.mobility.signature * e.signature)), tr!("Painters stack with the ship's own signature modifiers, a running microwarpdrive first."));
    });
    tip_row(ui, tr!("Speed / across"), (format!("{} / {} m/s", text::num(f64::from(p.velocity)), text::num(f64::from(p.transversal))), (e.speed < 0.995).then_some(GOOD)), |ui| {
        let full = if e.speed > 0.0 { f64::from(p.velocity) / e.speed } else { f64::from(p.velocity) };
        ewar_tip(ui, dogma, tr!("Speed"), &ours(RemoteKind::Web, 0), &format!("{} m/s", text::num(full)), &format!("{} m/s", text::num(f64::from(p.velocity))), tr!("Its speed as the circles set it, under our webs; across the line of fire is what both headings leave of the two speeds."));
    });
    grid_row(ui, tr!("Angular velocity"), plain(format!("{:.4} rad/s", f64::from(p.transversal) / f64::from(p.distance).max(1.0))));
    // Against what this ship fires now, damage type by damage type; uniform with no damage.
    let mix = spai_fit::stats::Damage { em: now[0], thermal: now[1], kinetic: now[2], explosive: now[3] };
    let ehp = spai_fit::fleet::sweep::ehp_against(&t.tank, &mix);
    tip_row(ui, tr!("EHP against our damage"), plain(text::num(ehp)), |ui| {
        let total = mix.total().max(1e-9);
        let rows = vec![
            (tr!("Against our damage").to_owned(), text::num(ehp)),
            (tr!("Against even damage").to_owned(), text::num(t.tank.ehp)),
            (tr!("Our damage, EM / thermal / kinetic / explosive").to_owned(), format!("{:.0} / {:.0} / {:.0} / {:.0}%", mix.em / total * 100.0, mix.thermal / total * 100.0, mix.kinetic / total * 100.0, mix.explosive / total * 100.0)),
        ];
        breakdown(ui, tr!("Effective hit points"), &rows, tr!("Each layer's hit points over the share of our damage its resistances let through."));
    });
    ui.add_space(2.0);
    crate::stats_panel::resist_table(ui, &t.tank);
    ui.add_space(2.0);
    let raw: f64 = weapons.iter().map(|w| w.dps).sum();
    if raw <= 0.0 {
        return;
    }
    let applied: f64 = now.iter().sum();
    let landed = through(now, resonance);
    tip_row(ui, tr!("DPS listed / applied / after resists"), plain(format!("{} / {} / {}", text::num(raw), text::num(applied), text::num(landed))), |ui| {
        breakdown(ui, tr!("Damage per second"), &[(tr!("Listed").to_owned(), text::num(raw)), (tr!("Applied: range, speed, signature").to_owned(), text::num(applied)), (tr!("After its resistances").to_owned(), text::num(landed))], tr!("Applied is what lands of the listed damage: range, tracking or explosion against its distance, speed and signature. Its resistances come after."));
    });
    tip_row(ui, tr!("Volley listed / applied / after resists"), plain(format!("{} / {} / {}", text::num(volley.0), text::num(volley.1), text::num(volley.2))), |ui| {
        breakdown(ui, tr!("One volley of the weapons that fire together"), &[(tr!("Listed").to_owned(), text::num(volley.0)), (tr!("Applied: range, speed, signature").to_owned(), text::num(volley.1)), (tr!("After its resistances").to_owned(), text::num(volley.2))], "");
    });
    tip_row(ui, tr!("Application"), plain(format!("{}%", text::num(applied / raw * 100.0))), |ui| {
        breakdown(ui, tr!("Application"), &[], tr!("The share of the listed damage that lands: the weapons' range, tracking or explosion against the target's distance, speed across and signature. Resistances are apart."));
    });
    if applied > 0.0 {
        grid_row(ui, tr!("Through its resistances"), plain(format!("{}%", text::num(landed / applied * 100.0))));
    }
    // Ships to volley it off: its EHP against our damage over what one volley lands in practice,
    // with a margin of two and a half, called in fives.
    let eff = f64::from(efficiency.clamp(0.01, 1.0));
    if volley.1 > 0.0 {
        let need = ehp / (volley.1 * eff);
        let called = called_mass(need);
        tip_row(ui, tr!("Critical mass"), plain(trf!("{n} ships", n = text::num(called))), |ui| {
            let rows = vec![
                (tr!("EHP against our damage").to_owned(), text::num(ehp)),
                (tr!("Applied volley of one ship").to_owned(), text::num(volley.1)),
                (tr!("Volley efficiency").to_owned(), format!("{}%", text::num(eff * 100.0))),
                (tr!("Ships to kill it, exactly").to_owned(), format!("{need:.1}")),
                (tr!("Called: plus 2.5, up to a five").to_owned(), text::num(called)),
            ];
            breakdown(ui, tr!("Critical mass"), &rows, tr!("Ships of this kind to volley one off the field at once."));
        });
    }
    // Volley by volley, against what its logistics put back.
    let total_hp: f64 = [&t.tank.shield, &t.tank.armor, &t.tank.hull].iter().map(|l| l.hp).sum();
    let reps_ehp = if total_hp > 0.0 { kill.reps * ehp / total_hp } else { 0.0 };
    for (n, label) in [(1u32, tr!("One ship kills it in").to_owned()), (count, trf!("All {n} kill it in", n = count))] {
        let at = kill_time(&kill.volleys, f64::from(n) * eff, ehp, reps_ehp);
        let first = kill.volleys.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
        let shown = match at {
            None => tr!("never").to_owned(),
            Some(t) if t <= first + 1e-6 => tr!("the first volley").to_owned(),
            Some(t) => text::secs(t),
        };
        tip_row(ui, &label, plain(shown), |ui| {
            let cycle = kill.volleys.iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
            let per_volley: f64 = kill.volleys.iter().map(|v| v.2).sum::<f64>() * f64::from(n) * eff;
            let rows = vec![
                (tr!("EHP against our damage").to_owned(), text::num(ehp)),
                (tr!("Each volley, all of them").to_owned(), text::num(per_volley)),
                (tr!("Every").to_owned(), text::secs(cycle)),
                (tr!("First lands after").to_owned(), text::secs(first.max(0.0))),
                (tr!("Their logistics put back").to_owned(), trf!("{v} EHP/s", v = text::num(reps_ehp))),
            ];
            breakdown(ui, &label, &rows, tr!("Volley by volley, each weapon on its own cycle and landing after its flight, the volley efficiency counted; their logistics start five seconds after the first hit and never heal past full."));
        });
    }
}

/// The critical mass as a fleet calls it: what it takes plus two and a half, up to the next five.
fn called_mass(need: f64) -> f64 {
    ((need + 2.5) / 5.0).ceil() * 5.0
}

/// A card's line with a tooltip of its own.
fn tip_row(ui: &mut egui::Ui, label: &str, value: (String, Option<egui::Color32>), tip: impl FnOnce(&mut egui::Ui)) {
    let r = ui.scope(|ui| crate::stats_panel::line_colored(ui, label, value.0, value.1)).response.interact(egui::Sense::hover());
    r.on_hover_ui(|ui| {
        ui.set_max_width(380.0);
        tip(ui);
    });
}

/// A tooltip's table: a title, label and value lines, and a note below.
fn breakdown(ui: &mut egui::Ui, title: &str, rows: &[(String, String)], note: &str) {
    ui.label(egui::RichText::new(title).strong());
    if !rows.is_empty() {
        // Plain cells: a cell laid out right to left takes all the width there is and runs over
        // its neighbour.
        egui::Grid::new(("sim_fleet_ship_tip", title)).num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            for (l, v) in rows {
                ui.label(l);
                ui.label(egui::RichText::new(v).monospace());
                ui.end_row();
            }
        });
    }
    if !note.is_empty() {
        ui.add_space(2.0);
        ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
    }
}

/// How one figure got where ewar left it: every module type that lands with its strength at the
/// distance, the shares the stacking penalty leaves the strongest ones, and the figure before and
/// after.
fn ewar_tip(ui: &mut egui::Ui, dogma: &Dogma, title: &str, parts: &[(TypeId, f64, f64)], before: &str, after: &str, note: &str) {
    let landing: f64 = parts.iter().map(|(_, n, _)| *n).sum();
    let count = |n: f64| if (n - n.round()).abs() < 0.01 { format!("{:.0}", n) } else { format!("{:.1}", n) };
    let mut rows: Vec<(String, String)> = parts.iter().map(|(t, n, pct)| (trf!("{n} × {module}", n = count(*n), module = name(dogma, *t)), format!("{:+.1}% each", pct))).collect();
    if landing > 1.0 {
        // The k-th strongest keeps PENALTY_FACTOR^(k²) of its effect.
        let shares: Vec<String> = (0..(landing.ceil() as usize).min(6)).map(|k| format!("{:.0}%", spai_fit::calc::PENALTY_FACTOR.powi((k * k) as i32) * 100.0)).collect();
        rows.push((tr!("Stacking, strongest first").to_owned(), shares.join(", ")));
    }
    rows.push((tr!("Without ewar").to_owned(), before.to_owned()));
    rows.push((tr!("Under it").to_owned(), after.to_owned()));
    if parts.is_empty() {
        breakdown(ui, title, &rows[rows.len() - 2..], tr!("No ewar of this kind reaches it here."));
    } else {
        breakdown(ui, title, &rows, note);
    }
}

/// The menu of every ewar module either fleet flies, each with how many of it count for this
/// ship: all, none, or a number between. It stays open while it is set.
fn ewar_menu(ui: &mut egui::Ui, st: &mut ShipsState, k: usize, ours: &[(EwarSource, String, u32)], theirs: &[(EwarSource, String, u32)]) {
    let set = st.ewar_n.entry(k).or_default();
    let total = ours.len() + theirs.len();
    let on = ours.iter().chain(theirs).filter(|(src, _, _)| set.get(src) != Some(&0)).count();
    let label = trf!("{icon} Ewar {on} of {total}", icon = egui_phosphor::regular::SLIDERS_HORIZONTAL, on = on, total = total);
    let config = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    egui::containers::menu::MenuButton::new(label).config(config).ui(ui, |ui| {
        ui.set_min_width(360.0);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for (title, list) in [(tr!("Theirs, on this ship"), theirs), (tr!("Ours, on the target"), ours)] {
                ui.label(egui::RichText::new(title).strong());
                if list.is_empty() {
                    ui.label(egui::RichText::new(tr!("None.")).weak());
                }
                for (src, label, n) in list {
                    // This ship's own modules stay on: one at least, its own.
                    let own = !src.theirs && src.slot == k;
                    let least = u32::from(own);
                    ui.horizontal(|ui| {
                        let mut count = set.get(src).copied().unwrap_or(*n).clamp(least, *n);
                        let mut counted = count > 0;
                        let r = ui.add_enabled(!own, egui::Checkbox::without_text(&mut counted)).on_disabled_hover_text(tr!("This ship's own: it always flies it"));
                        let d = ui.add(egui::DragValue::new(&mut count).range(least..=*n).suffix(trf!(" of {n}", n = n)));
                        ui.add(egui::Label::new(label.as_str()).truncate());
                        if r.changed() {
                            count = if counted { *n } else { 0 };
                        }
                        if r.changed() || d.changed() {
                            if count == *n {
                                set.remove(src);
                            } else {
                                set.insert(*src, count);
                            }
                        }
                    });
                }
                ui.add_space(6.0);
            }
            ui.label(egui::RichText::new(tr!("Focused or spread as each fleet lays it, under Ewar on the enemy in the fleet's settings.")).weak());
        });
    });
}

/// The changes into the fleet's slots: a charge or mode the fit has anyway is no swap.
fn apply(sim: &mut SimUi, dogma: &Dogma, changes: Vec<Change>) {
    if changes.is_empty() {
        return;
    }
    let fits: Vec<Option<spai_fit::fit::Fit>> = sim.fleet.as_ref().map(|f| f.fleet.slots.iter().map(|s| sim.fit_docs.get(&s.fit_id).map(|fit| spai_fit::fleet::flown(dogma, fit, s.variation))).collect()).unwrap_or_default();
    let Some(session) = sim.fleet.as_mut() else { return };
    for c in changes {
        match c {
            Change::Charge(k, weapon, charge) => {
                let Some(slot) = session.fleet.slots.get_mut(k) else { continue };
                slot.charges.retain(|(w, _)| *w != weapon);
                let fitted = fits.get(k).and_then(|f| f.as_ref()).and_then(|f| f.modules.iter().find(|m| m.type_id == weapon)).and_then(|m| m.charge);
                if fitted != Some(charge) {
                    slot.charges.push((weapon, charge));
                }
            }
            Change::Split(k, weapon, split) => {
                let Some(slot) = session.fleet.slots.get_mut(k) else { continue };
                slot.splits.retain(|x| x.weapon != weapon);
                if let Some(sp) = split {
                    // The split says each part's charge; a single swap of the stack no longer does.
                    slot.charges.retain(|(w, _)| *w != weapon);
                    slot.splits.push(sp);
                }
            }
            Change::Mode(k, mode) => {
                let Some(slot) = session.fleet.slots.get_mut(k) else { continue };
                let fitted = fits.get(k).and_then(|f| f.as_ref()).and_then(|f| f.mode);
                slot.mode = (fitted != Some(mode)).then_some(mode);
            }
            Change::Reset(k) => {
                if let Some(slot) = session.fleet.slots.get_mut(k) {
                    slot.charges.clear();
                    slot.splits.clear();
                    slot.mode = None;
                }
            }
        }
    }
    session.dirty = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use spai_fit::fleet::ewar::ewar_on;
    use spai_fit::fit::Module;
    use spai_fit::fleet::{CountRule, Fleet, FleetSlot};
    use spai_fit::skills::SkillSet;
    use spai_fit::test_support::{dataset, ids};

    #[test]
    fn a_kill_comes_volley_by_volley_and_repairs_put_some_back() {
        // A volley of 30 every 5 s, the first after 2 s, on 100 EHP.
        let v = [(5.0, 2.0, 30.0)];
        assert_eq!(kill_time(&v, 1.0, 100.0, 0.0), Some(17.0), "four volleys");
        assert_eq!(kill_time(&v, 4.0, 100.0, 0.0), Some(2.0), "four ships at once: the first volley, after its flight");
        assert_eq!(kill_time(&v, 1.0, 100.0, 1.0), Some(17.0), "1 EHP/s from 7 s puts back 10 before the fourth");
        assert_eq!(kill_time(&v, 1.0, 100.0, 3.0), Some(22.0), "3 EHP/s takes a fifth");
        assert_eq!(kill_time(&v, 1.0, 100.0, 7.0), None, "repairs above its damage per second: never");
    }

    #[test]
    fn the_critical_mass_is_called_with_a_margin_in_fives() {
        assert_eq!(called_mass(41.2), 45.0);
        assert_eq!(called_mass(37.5), 40.0);
        assert_eq!(called_mass(42.5), 45.0);
        assert_eq!(called_mass(2.0), 5.0);
    }

    #[test]
    fn each_ewar_module_counts_at_the_number_set_for_it() {
        let d = dataset();
        let mut web = Fit::new(ids::SHIP, "webber");
        web.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
        let fleet = Fleet { size: 6, slots: vec![FleetSlot { fit_id: "webber".into(), rule: CountRule::Fixed(6), ..FleetSlot::default() }], ..Fleet::default() };
        let stats = spai_fit::fleet::stats(&d, &fleet, &|_| Some(web.clone()), &|_| SkillSet::all_v());
        let target = Fit::new(ids::SHIP, "target");
        let t = spai_fit::stats::compute(&d, &target, &SkillSet::all_v(), &spai_fit::calc::Env::default());
        let speed = |set: &HashMap<EwarSource, u32>| {
            let ships = fleet_ewar(&stats, false, set, None);
            ewar_on(&ships.iter().map(|(n, c)| (*n, c)).collect::<Vec<_>>(), 1000.0, 1.0, Some(&t)).speed
        };
        let src = EwarSource { theirs: false, slot: 0, module: ids::WEB };
        let all = speed(&HashMap::new());
        let none = speed(&HashMap::from([(src, 0)]));
        let one = speed(&HashMap::from([(src, 1)]));
        assert!((none - 1.0).abs() < 1e-9, "left out, it slows nothing");
        assert!(all < one && one < 1.0, "one web slows less than six: {one} vs {all}");
        assert!((speed(&HashMap::from([(src, 6)])) - all).abs() < 1e-9, "all six as set is all of them");
    }
}
