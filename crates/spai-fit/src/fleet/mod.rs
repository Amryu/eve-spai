//! Fleets: which fits fly in what numbers, what the boosters give everyone, and how one fleet
//! fares against another at a given range.

use serde::{Deserialize, Serialize};

use crate::calc::{Calc, Env};
use crate::dogma::{attr, Dogma};
use crate::fit::{Fit, State, TargetProfile};
use crate::ids::BuffId;
use crate::skills::SkillSet;
use crate::stats::{application::applied_factor, compute_with, Computed, WeaponKind};

/// How many of a fit fly, for a fleet of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CountRule {
    /// Always this many.
    Fixed(u32),
    /// This share of the fleet, in percent.
    Percent(f32),
    /// Whatever the others leave, between these bounds: the line ship.
    Fill { min: u32, max: u32 },
}

impl Default for CountRule {
    fn default() -> Self {
        CountRule::Fixed(1)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FleetSlot {
    pub fit_id: String,
    pub rule: CountRule,
    /// A number typed by hand, over the rule.
    pub override_count: Option<u32>,
    /// The skills profile the fit flies with; `None` for the fleet's default.
    pub profile: Option<String>,
}

impl Default for FleetSlot {
    fn default() -> Self {
        FleetSlot { fit_id: String::new(), rule: CountRule::default(), override_count: None, profile: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BurstKind {
    Shield,
    Armor,
    Skirmish,
    Information,
    Mining,
}

impl BurstKind {
    pub const ALL: [BurstKind; 5] = [BurstKind::Shield, BurstKind::Armor, BurstKind::Skirmish, BurstKind::Information, BurstKind::Mining];

    /// The burst module's effect in the data.
    pub fn effect(self) -> &'static str {
        match self {
            BurstKind::Shield => "moduleBonusWarfareLinkShield",
            BurstKind::Armor => "moduleBonusWarfareLinkArmor",
            BurstKind::Skirmish => "moduleBonusWarfareLinkSkirmish",
            BurstKind::Information => "moduleBonusWarfareLinkInfo",
            BurstKind::Mining => "moduleBonusWarfareLinkMining",
        }
    }
}

/// What the enemy is expected to keep on this fleet's ships.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ewar {
    pub webs: u32,
    pub painters: u32,
    pub tracking_disruptors: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fleet {
    pub id: String,
    pub name: String,
    pub size: u32,
    pub slots: Vec<FleetSlot>,
    /// One booster fit per burst kind; its bursts reach the whole fleet.
    pub boosters: Vec<(BurstKind, String)>,
    /// Which slot is the main damage dealer, for the enemy's alpha and critical mass; the first
    /// slot when unset.
    pub main: Option<usize>,
    /// Metres between the fleets when they fight.
    pub range: f32,
    /// The share of a volley that lands in practice: mechanical failures, lacking skills.
    pub volley_efficiency: f32,
    /// What this fleet keeps on the enemy.
    pub ewar: Ewar,
    pub notes: String,
}

impl Default for Fleet {
    fn default() -> Self {
        Fleet { id: String::new(), name: String::new(), size: 50, slots: Vec::new(), boosters: Vec::new(), main: None, range: 30_000.0, volley_efficiency: 0.85, ewar: Ewar::default(), notes: String::new() }
    }
}

/// How many of each slot fly: fixed and percent shares first, the fill slots dividing what is
/// left within their bounds, a typed number over everything.
pub fn resolve_counts(fleet: &Fleet) -> Vec<u32> {
    let size = fleet.size;
    let mut counts: Vec<u32> = fleet
        .slots
        .iter()
        .map(|s| match (s.override_count, s.rule) {
            (Some(n), _) => n,
            (None, CountRule::Fixed(n)) => n,
            (None, CountRule::Percent(p)) => (f64::from(size) * f64::from(p.max(0.0)) / 100.0).round() as u32,
            (None, CountRule::Fill { .. }) => 0,
        })
        .collect();
    let fills: Vec<usize> = fleet.slots.iter().enumerate().filter(|(_, s)| s.override_count.is_none() && matches!(s.rule, CountRule::Fill { .. })).map(|(i, _)| i).collect();
    if !fills.is_empty() {
        let taken: u32 = counts.iter().sum();
        let mut left = size.saturating_sub(taken);
        let share = left / fills.len() as u32;
        for (k, &i) in fills.iter().enumerate() {
            let CountRule::Fill { min, max } = fleet.slots[i].rule else { continue };
            let want = if k + 1 == fills.len() { left } else { share };
            let n = want.clamp(min, max.max(min));
            counts[i] = n;
            left = left.saturating_sub(n);
        }
    }
    counts
}

/// The bursts a booster fit gives: every active burst module's charge names up to four buffs.
pub fn bursts_of(dogma: &Dogma, fit: &Fit, skills: &SkillSet) -> Vec<(BuffId, f64)> {
    let calc = Calc::new(dogma, fit, skills, &Env::default());
    let mut out = Vec::new();
    for (i, it) in calc.modules() {
        if it.state < State::Active || it.charge.is_none() {
            continue;
        }
        if !calc.effect_names(i).any(|n| BurstKind::ALL.iter().any(|k| k.effect() == n)) {
            continue;
        }
        for (id_attr, value_attr) in [
            (attr::WARFARE_BUFF_1_ID, attr::WARFARE_BUFF_1_VALUE),
            (attr::WARFARE_BUFF_2_ID, attr::WARFARE_BUFF_2_VALUE),
            (attr::WARFARE_BUFF_3_ID, attr::WARFARE_BUFF_3_VALUE),
            (attr::WARFARE_BUFF_4_ID, attr::WARFARE_BUFF_4_VALUE),
        ] {
            let id = calc.value(i, id_attr);
            if id > 0.0 {
                out.push((id as BuffId, calc.value(i, value_attr)));
            }
        }
    }
    out
}

/// The fleet's environment: the strongest of every buff its boosters give.
pub fn env_of(dogma: &Dogma, fleet: &Fleet, fits: &dyn Fn(&str) -> Option<Fit>, skills: &dyn Fn(Option<&str>) -> SkillSet) -> Env {
    let mut env = Env::default();
    for (_, fit_id) in &fleet.boosters {
        let Some(fit) = fits(fit_id) else { continue };
        let profile = fleet.slots.iter().find(|s| &s.fit_id == fit_id).and_then(|s| s.profile.clone());
        for (id, value) in bursts_of(dogma, &fit, &skills(profile.as_deref())) {
            env.offer(dogma, id, value);
        }
    }
    env
}

/// One slot of a fleet, computed.
#[derive(Clone, Debug)]
pub struct SlotStats {
    pub fit_id: String,
    pub name: String,
    pub count: u32,
    pub computed: Computed,
}

#[derive(Clone, Debug, Default)]
pub struct FleetStats {
    pub name: String,
    pub ships: u32,
    pub slots: Vec<SlotStats>,
    pub dps: f64,
    pub volley: f64,
    /// Remote repairs the fleet's logistics give per second, armour plus shield.
    pub logi_rep: f64,
    pub neut: f64,
    /// The slowest and the fastest fit's top speed.
    pub speed_min: f64,
    pub speed_max: f64,
    /// Which slot is the main damage dealer.
    pub main: usize,
}

impl FleetStats {
    pub fn main(&self) -> Option<&SlotStats> {
        self.slots.get(self.main)
    }
}

/// Computes every slot with the fleet's bursts applied.
pub fn stats(dogma: &Dogma, fleet: &Fleet, fits: &dyn Fn(&str) -> Option<Fit>, skills: &dyn Fn(Option<&str>) -> SkillSet) -> FleetStats {
    let env = env_of(dogma, fleet, fits, skills);
    let counts = resolve_counts(fleet);
    let mut out = FleetStats { name: fleet.name.clone(), main: fleet.main.unwrap_or(0), speed_min: f64::INFINITY, ..FleetStats::default() };
    for (slot, count) in fleet.slots.iter().zip(counts) {
        let Some(fit) = fits(&slot.fit_id) else { continue };
        let fit = crate::variations::resolve(dogma, &fit);
        let calc = Calc::new(dogma, &fit, &skills(slot.profile.as_deref()), &env);
        let computed = compute_with(&calc, &fit);
        let n = f64::from(count);
        out.dps += computed.dps() * n;
        out.volley += computed.volley() * n;
        out.logi_rep += (computed.remote.armor_rep + computed.remote.shield_rep) * n;
        out.neut += computed.remote.neut * n;
        if count > 0 {
            out.speed_min = out.speed_min.min(computed.mobility.max_velocity);
            out.speed_max = out.speed_max.max(computed.mobility.max_velocity);
        }
        out.ships += count;
        out.slots.push(SlotStats { fit_id: slot.fit_id.clone(), name: fit.name.clone(), count, computed });
    }
    if !out.speed_min.is_finite() {
        out.speed_min = 0.0;
    }
    if out.main >= out.slots.len() {
        out.main = 0;
    }
    out
}

/// What the enemy's ewar does to a target: webs cut its speed, painters grow its signature, both
/// stacking-penalised like the modules they stand for.
pub fn ewar_on_target(ewar: &Ewar, velocity: f64, signature: f64) -> (f64, f64) {
    let webs: Vec<(crate::dogma::Op, f64, bool)> = (0..ewar.webs).map(|_| (crate::dogma::Op::PostPercent, -60.0, true)).collect();
    let painters: Vec<(crate::dogma::Op, f64, bool)> = (0..ewar.painters).map(|_| (crate::dogma::Op::PostPercent, 30.0, true)).collect();
    (crate::calc::combine(velocity, true, &webs), crate::calc::combine(signature, true, &painters))
}

/// How one side's fire lands on the other's main hull.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Against {
    /// Damage per second that lands on the enemy main hull, the whole fleet.
    pub applied_dps: f64,
    pub applied_volley: f64,
    /// Ships of the main slot needed to volley one enemy main hull, after the efficiency.
    pub critical_mass: Option<u32>,
    /// Enemy main hulls one full volley kills.
    pub kills_per_volley: f64,
    /// Repairs the enemy can put on one ship: its logistics plus the hull's own.
    pub break_point: f64,
    /// Applied DPS above the break point; negative when the enemy out-repairs this fleet.
    pub margin: f64,
    /// The enemy main hull's speed and signature as this fleet's ewar leaves them.
    pub target_velocity: f64,
    pub target_signature: f64,
}

/// `a` shooting at `b`'s main hull at `a.range`, with `a.ewar` on it and `b.ewar` on `a`'s guns.
pub fn against(dogma: &Dogma, a: &Fleet, a_stats: &FleetStats, b: &Fleet, b_stats: &FleetStats) -> Against {
    let mut out = Against::default();
    let Some(target) = b_stats.main() else { return out };
    let t = &target.computed;
    let (velocity, signature) = ewar_on_target(&a.ewar, t.mobility.max_velocity, t.mobility.signature);
    out.target_velocity = velocity;
    out.target_signature = signature;
    let profile = TargetProfile { signature: signature as f32, velocity: velocity as f32, transversal: velocity as f32, distance: a.range, radius: 0.0 };
    // Tracking disruptors on the shooters: weaker range and tracking, penalised.
    let td: Vec<(crate::dogma::Op, f64, bool)> = (0..b.ewar.tracking_disruptors).map(|_| (crate::dogma::Op::PostPercent, -17.19, true)).collect();
    let td_factor = crate::calc::combine(1.0, true, &td);
    let mut main_volley_per_ship = 0.0;
    for (i, slot) in a_stats.slots.iter().enumerate() {
        let n = f64::from(slot.count);
        let mut dps = 0.0;
        let mut volley = 0.0;
        for w in &slot.computed.weapons {
            let mut w = w.clone();
            if w.kind == WeaponKind::Turret {
                w.optimal *= td_factor;
                w.falloff *= td_factor;
                w.tracking *= td_factor;
            }
            let f = applied_factor(&w, f64::from(a.range), &profile, slot.computed.drone_speed.unwrap_or(0.0), slot.computed.drones.control_range);
            dps += w.dps * f;
            volley += w.volley.total() * f;
        }
        out.applied_dps += dps * n;
        out.applied_volley += volley * n;
        if i == a_stats.main {
            main_volley_per_ship = volley;
        }
    }
    let ehp = t.tank.ehp;
    let eff = f64::from(a.volley_efficiency.clamp(0.01, 1.0));
    out.critical_mass = (main_volley_per_ship > 0.0 && ehp > 0.0).then(|| (ehp / (main_volley_per_ship * eff)).ceil() as u32);
    out.kills_per_volley = if ehp > 0.0 { out.applied_volley * eff / ehp } else { 0.0 };
    out.break_point = b_stats.logi_rep + t.tank.reps.armor + t.tank.reps.shield + t.tank.reps.hull;
    out.margin = out.applied_dps * eff - out.break_point;
    let _ = dogma;
    out
}

#[cfg(test)]
mod tests;
