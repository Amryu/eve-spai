//! Fleets: which fits fly in what numbers, what the boosters give everyone, and how one fleet
//! fares against another at a given range.

use serde::{Deserialize, Serialize};

use crate::calc::{Calc, Env};
use crate::dogma::{attr, Dogma};
use crate::fit::{Fit, State, TargetProfile};
use crate::ids::BuffId;
use crate::skills::SkillSet;
use crate::ids::{GroupId, TypeId};
use crate::stats::{application::applied_factor, compute_with, Computed, RemoteKind, WeaponKind};

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
    /// The variation it flies with; none is its base fit.
    pub variation: Option<u32>,
}

impl Default for FleetSlot {
    fn default() -> Self {
        FleetSlot { fit_id: String::new(), rule: CountRule::default(), override_count: None, profile: None, variation: None }
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

/// How a fleet's ships sit around its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Formation {
    /// On a sphere of this radius: a tight ball at 0.
    Ball { radius_m: f32 },
    /// Following the anchor as a swarm: each ship's distance to it bell-curve distributed with
    /// this spread (σ).
    Flock { sigma_m: f32 },
}

impl Default for Formation {
    fn default() -> Self {
        Formation::Ball { radius_m: 5_000.0 }
    }
}

/// What a ship is for in a fight, for the order an enemy shoots a fleet in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// The fleet's main damage dealer.
    Main,
    /// Other damage dealers.
    Mainline,
    Logi,
    /// Anything else that is not a damage dealer: neuts, utility.
    Support,
    Tackle,
    Ewar,
    Command,
}

impl Role {
    /// The order a fleet shoots the enemy in by default: damage first.
    pub const DEFAULT_ORDER: [Role; 7] = [Role::Main, Role::Mainline, Role::Logi, Role::Support, Role::Tackle, Role::Ewar, Role::Command];
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
    /// Generic boosters in place of a fit, at most one per kind: everyone gets their bursts, but
    /// they fly in no slot, so the enemy has nothing to shoot.
    pub boost_shortcuts: Vec<crate::fit::BoostShortcut>,
    /// Which slot is the main damage dealer, for the enemy's alpha and critical mass; the first
    /// slot when unset.
    pub main: Option<usize>,
    /// Metres between the fleets when they fight.
    pub range: f32,
    /// The share of a volley that lands in practice: mechanical failures, lacking skills.
    pub volley_efficiency: f32,
    /// What this fleet keeps on the enemy, typed by hand; without it, what its ships project.
    pub ewar: Ewar,
    #[serde(default = "default_true")]
    pub ewar_auto: bool,
    pub notes: String,
    /// How the ships sit around the anchor.
    pub formation: Formation,
    /// How the fleet moves: its share of the slowest ship's top speed and its heading against
    /// the line to the enemy (0 straight at it, 90 across, 180 away).
    pub motion: (f32, f32),
    /// The order it shoots the enemy's ships in, by role.
    pub target_order: Vec<Role>,
    /// The share of its logistics' repairs that does any good: late locks, misjudged targets,
    /// cycles landing on full health.
    #[serde(default = "default_logi_efficiency")]
    pub logi_efficiency: f32,
    /// Drones and fighters launched: counted in damage per second, never in a volley.
    #[serde(default = "default_true")]
    pub drones: bool,
    /// Pilots switch their assault damage controls on when primaried.
    #[serde(default = "default_true")]
    pub adc: bool,
    /// The fleet it is compared with, by library id.
    pub enemy: Option<String>,
    /// The fight placed in 3D; when set, it gives the range, the headings and the opponent's
    /// motion.
    pub scene: Option<scene::Scene>,
}

fn default_true() -> bool {
    true
}

fn default_logi_efficiency() -> f32 {
    0.75
}

impl Default for Fleet {
    fn default() -> Self {
        Fleet {
            id: String::new(),
            name: String::new(),
            size: 50,
            slots: Vec::new(),
            boosters: Vec::new(),
            boost_shortcuts: Vec::new(),
            main: None,
            range: 30_000.0,
            volley_efficiency: 0.85,
            ewar: Ewar::default(),
            ewar_auto: true,
            notes: String::new(),
            formation: Formation::default(),
            motion: (0.0, 90.0),
            target_order: Role::DEFAULT_ORDER.to_vec(),
            adc: true,
            drones: true,
            logi_efficiency: 0.75,
            enemy: None,
            scene: None,
        }
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

impl BurstKind {
    /// The Tech II burst module of the kind, by name.
    pub fn module_name(self) -> &'static str {
        match self {
            BurstKind::Shield => "Shield Command Burst II",
            BurstKind::Armor => "Armor Command Burst II",
            BurstKind::Skirmish => "Skirmish Command Burst II",
            BurstKind::Information => "Information Command Burst II",
            BurstKind::Mining => "Mining Foreman Burst II",
        }
    }

    pub fn mindlink_name(self) -> &'static str {
        match self {
            BurstKind::Shield => "Shield Command Mindlink",
            BurstKind::Armor => "Armored Command Mindlink",
            BurstKind::Skirmish => "Skirmish Command Mindlink",
            BurstKind::Information => "Information Command Mindlink",
            BurstKind::Mining => "Mining Foreman Mindlink",
        }
    }

    /// How hull traits and skills name the kind: "Shield Command", "Armored Command".
    pub fn command_name(self) -> &'static str {
        match self {
            BurstKind::Shield => "Shield Command",
            BurstKind::Armor => "Armored Command",
            BurstKind::Skirmish => "Skirmish Command",
            BurstKind::Information => "Information Command",
            BurstKind::Mining => "Mining Foreman",
        }
    }

    /// The skills that change the kind's bursts, by name: its own command skills and the general
    /// ones. A hull's own skill comes on top, from its traits.
    pub fn skill_names(self) -> Vec<String> {
        let mut v = vec![self.command_name().to_owned()];
        if self != BurstKind::Mining {
            v.push(format!("{} Specialist", self.command_name()));
        }
        v.extend(["Leadership", "Wing Command", "Fleet Command", "Command Burst Specialist"].map(str::to_owned));
        v
    }
}

/// The burst modules of the kind, by meta level: Tech I, Tech II, then the faction ones.
pub fn burst_modules(dogma: &Dogma, kind: BurstKind) -> Vec<crate::ids::TypeId> {
    let mut v: Vec<&crate::dogma::TypeDef> = dogma.types.iter().filter(|t| t.published && t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|x| x.name == kind.effect()))).collect();
    v.sort_by_key(|t| (t.meta_level.unwrap_or(0), t.name.clone()));
    v.into_iter().map(|t| t.id).collect()
}

/// The charges a burst module loads.
pub fn burst_charges(dogma: &Dogma, module: crate::ids::TypeId) -> Vec<crate::ids::TypeId> {
    let Some(m) = dogma.type_(module) else { return Vec::new() };
    let groups: Vec<u32> = m.attrs.iter().filter(|(a, _)| dogma.attr(*a).is_some_and(|x| x.name.starts_with("chargeGroup"))).map(|(_, v)| *v as u32).collect();
    let mut out: Vec<(String, crate::ids::TypeId)> = dogma.types.iter().filter(|t| t.published && groups.contains(&u32::from(t.group))).map(|t| (t.name.clone(), t.id)).collect();
    out.sort();
    out.into_iter().map(|(_, id)| id).collect()
}

/// Whether a hull's traits strengthen the kind's bursts: a line on burst strength naming the kind's
/// command, or no kind at all.
pub fn hull_boosts(dogma: &Dogma, hull: crate::ids::TypeId, kind: BurstKind) -> bool {
    let Some(t) = dogma.traits_of(hull) else { return false };
    let lines = t.role.iter().chain(t.misc.iter()).chain(t.per_skill.iter().flat_map(|(_, b)| b.iter()));
    let named = BurstKind::ALL.map(|k| k.command_name().to_lowercase());
    lines.into_iter().any(|b| {
        let text = b.text.to_lowercase();
        // Strength, not range or how many fit: "bonus to Shield Command burst strength and duration".
        text.contains("burst") && text.contains("strength") && (text.contains(&kind.command_name().to_lowercase()) || !named.iter().any(|n| text.contains(n.as_str())))
    })
}

/// The hulls that strengthen the kind's bursts, by name.
pub fn boosting_hulls(dogma: &Dogma, kind: BurstKind) -> Vec<crate::ids::TypeId> {
    let mut v: Vec<&crate::dogma::TypeDef> = dogma.traits.iter().filter(|t| hull_boosts(dogma, t.type_id, kind)).filter_map(|t| dogma.type_(t.type_id)).filter(|t| t.published && t.category == crate::ids::category::SHIP).collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v.into_iter().map(|t| t.id).collect()
}

/// The skills the shortcut's pilot has that matter to it: the kind's and the hull's own.
pub fn shortcut_skills(dogma: &Dogma, b: &crate::fit::BoostShortcut) -> Vec<crate::ids::TypeId> {
    let mut v: Vec<crate::ids::TypeId> = b.kind.skill_names().iter().filter_map(|n| dogma.type_named(n)).map(|t| t.id).collect();
    if let Some(t) = b.hull.and_then(|h| dogma.traits_of(h)) {
        for (skill, lines) in &t.per_skill {
            if lines.iter().any(|l| { let t = l.text.to_lowercase(); t.contains("burst") && t.contains("strength") }) && !v.contains(skill) {
                v.push(*skill);
            }
        }
    }
    v
}

/// The booster the shortcut stands for: the kind's Tech II burst once per charge switched on, on
/// its hull (or a hull without burst bonuses), with the mindlink if it has one, and its pilot.
pub fn shortcut_booster(dogma: &Dogma, b: &crate::fit::BoostShortcut) -> Option<(Fit, SkillSet)> {
    use crate::fit::{Module, Slot};
    let module = b.module.or_else(|| dogma.type_named(b.kind.module_name()).map(|t| t.id))?;
    let hull = b.hull.or_else(|| plain_hull(dogma, b.kind))?;
    let mut fit = Fit::new(hull, "boost");
    for (i, charge) in burst_charges(dogma, module).into_iter().filter(|c| !b.charges_off.contains(c)).enumerate() {
        fit.modules.push(Module::new(module, Slot::High, i as u8).with_charge(charge).with_state(State::Active));
    }
    if b.mindlink {
        fit.implants.extend(dogma.type_named(b.kind.mindlink_name()).map(|t| t.id));
    }
    let mut skills = SkillSet::all_v();
    for (s, l) in &b.skills {
        skills.set(*s, *l);
    }
    Some((fit, skills))
}

/// The fit as a slot flies it: the variation picked, or its base, whatever was last switched on
/// in the editor.
pub fn flown(dogma: &Dogma, fit: &Fit, variation: Option<u32>) -> Fit {
    let mut f = fit.clone();
    f.active_variations = variation.filter(|v| f.variations.iter().any(|x| x.id == *v)).into_iter().collect();
    crate::variations::resolve(dogma, &f)
}

/// Whether the fit carries a burst module of the kind.
pub fn fit_has_burst(dogma: &Dogma, fit: &Fit, kind: BurstKind) -> bool {
    fit.modules.iter().any(|m| dogma.type_(m.type_id).is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|x| x.name == kind.effect()))))
}

/// A hull that gives the kind no bonus, for a shortcut without one: a battlecruiser or bigger
/// that is not among the boosting hulls.
fn plain_hull(dogma: &Dogma, kind: BurstKind) -> Option<crate::ids::TypeId> {
    let groups: Vec<&str> = vec!["Battlecruiser", "Attack Battlecruiser", "Battleship"];
    dogma.types.iter().filter(|t| t.published && t.category == crate::ids::category::SHIP).filter(|t| dogma.groups.iter().any(|g| g.id == t.group && groups.contains(&g.name.as_str()))).find(|t| !hull_boosts(dogma, t.id, kind)).map(|t| t.id).or_else(|| dogma.types.iter().find(|t| t.category == crate::ids::category::SHIP && !hull_boosts(dogma, t.id, kind)).map(|t| t.id))
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
    for b in fleet.boost_shortcuts.iter().filter(|b| b.active) {
        if let Some((fit, sk)) = shortcut_booster(dogma, b) {
            for (id, value) in bursts_of(dogma, &fit, &sk) {
                env.offer(dogma, id, value);
            }
        }
    }
    for (_, fit_id) in &fleet.boosters {
        let Some(fit) = fits(fit_id) else { continue };
        let fit = flown(dogma, &fit, fleet.slots.iter().find(|s| &s.fit_id == fit_id).and_then(|s| s.variation));
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
    /// The fleet slot it is, its hull and what it does.
    pub slot: usize,
    pub ship: TypeId,
    pub role: Role,
    /// The fit as it flies (variations applied) and its pilot, for working it out again.
    pub fit: Fit,
    pub skills: SkillSet,
}

#[derive(Clone, Debug, Default)]
pub struct FleetStats {
    pub name: String,
    /// What the fleet's ships project on one target: every web, painter and disruptor fitted,
    /// times the ships that carry it.
    pub projected: Ewar,
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
    /// The boosters' buffs every ship flies with.
    pub env: Env,
}

impl FleetStats {
    /// The main slot: by its role, so a slot left out (its fit missing) does not shift it.
    pub fn main(&self) -> Option<&SlotStats> {
        self.slots.iter().find(|s| s.role == Role::Main).or_else(|| self.slots.first())
    }
}

/// Computes every slot with the fleet's bursts applied.
pub fn stats(dogma: &Dogma, fleet: &Fleet, fits: &dyn Fn(&str) -> Option<Fit>, skills: &dyn Fn(Option<&str>) -> SkillSet) -> FleetStats {
    let env = env_of(dogma, fleet, fits, skills);
    let counts = resolve_counts(fleet);
    let mut out = FleetStats { name: fleet.name.clone(), main: fleet.main.unwrap_or(0), speed_min: f64::INFINITY, env: env.clone(), ..FleetStats::default() };
    for (index, (slot, count)) in fleet.slots.iter().zip(counts).enumerate() {
        let Some(fit) = fits(&slot.fit_id) else { continue };
        let fit = flown(dogma, &fit, slot.variation);
        let pilot = skills(slot.profile.as_deref());
        let calc = Calc::new(dogma, &fit, &pilot, &env);
        let mut computed = compute_with(&calc, &fit);
        if !fleet.drones {
            computed.weapons.retain(|w| w.alpha());
        }
        let n = f64::from(count);
        out.dps += computed.dps() * n;
        out.volley += computed.weapons.iter().filter(|w| w.alpha()).map(|w| w.volley.total()).sum::<f64>() * n;
        for m in &computed.remote.modules {
            let slot = match m.kind {
                RemoteKind::Web => &mut out.projected.webs,
                RemoteKind::Painter => &mut out.projected.painters,
                RemoteKind::TrackingDisruptor => &mut out.projected.tracking_disruptors,
                _ => continue,
            };
            *slot += count;
        }
        out.logi_rep += (computed.remote.armor_rep + computed.remote.shield_rep) * n;
        out.neut += computed.remote.neut * n;
        if count > 0 {
            out.speed_min = out.speed_min.min(computed.mobility.max_velocity);
            out.speed_max = out.speed_max.max(computed.mobility.max_velocity);
        }
        out.ships += count;
        let role = role_of(dogma, fleet, index, fit.ship);
        out.slots.push(SlotStats { fit_id: slot.fit_id.clone(), name: fit.name.clone(), count, computed, slot: index, ship: fit.ship, role, fit, skills: pilot });
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

/// Remote repairs on one ship lose effect with all the repairs it receives of the same layer
/// (since April 2019; the multiplier went from 10 to 20 in May 2025). With `total` the sum of
/// every repair's `amount / floor(cycle)` per second, a repair of `rrps` lands at
/// `1 - ((rrps + m) / (total + m) - 1)^2` of itself, `m = REP_ADDITION + REP_MULTIPLIER·rrps`.
/// It gives the figures published in 2019 with the old multiplier: fifteen 1600 HP/s repairs
/// at 77% each, half at thirty-eight.
pub const REP_ADDITION: f64 = 7000.0;
pub const REP_MULTIPLIER: f64 = 20.0;

/// What lands per second of `reps` on one layer of one ship: each `(amount per cycle, cycle s)`.
pub fn rep_landing(reps: &[(f64, f64)]) -> f64 {
    // The game counts a repair's share by its cycle in whole seconds.
    let rrps = |amount: f64, cycle: f64| amount / cycle.floor().max(1.0);
    let total: f64 = reps.iter().map(|(a, c)| rrps(*a, *c)).sum();
    reps.iter()
        .map(|(a, c)| {
            let r = rrps(*a, *c);
            let m = REP_ADDITION + r * REP_MULTIPLIER;
            let mult = 1.0 - ((r + m) / (total + m) - 1.0).powi(2);
            mult.max(0.0) * a / c.max(1e-9)
        })
        .sum()
}

/// A fleet's remote repairs as they would land all on one ship, by layer (shield, armor, hull):
/// every repair module of every ship, `n` ships of each slot (its count unless given).
pub fn reps_on_one(stats: &FleetStats, ships: &dyn Fn(usize, &SlotStats) -> u32) -> [f64; 3] {
    let mut lists: [Vec<(f64, f64)>; 3] = Default::default();
    for (k, s) in stats.slots.iter().enumerate() {
        let n = ships(k, s);
        for m in &s.computed.remote.modules {
            let layer = match m.kind {
                RemoteKind::ShieldRep => 0,
                RemoteKind::ArmorRep => 1,
                RemoteKind::HullRep => 2,
                _ => continue,
            };
            for _ in 0..n {
                lists[layer].push((m.amount * m.cycle_s, m.cycle_s));
            }
        }
    }
    [rep_landing(&lists[0]), rep_landing(&lists[1]), rep_landing(&lists[2])]
}

fn ships(s: &FleetStats) -> Vec<(u32, &Computed)> {
    s.slots.iter().filter(|x| x.count > 0).map(|x| (x.count, &x.computed)).collect()
}

/// The ewar a fleet keeps on the enemy: what its ships project unless typed by hand.
pub fn ewar_of(fleet: &Fleet, stats: &FleetStats) -> Ewar {
    if fleet.ewar_auto {
        stats.projected
    } else {
        fleet.ewar
    }
}

/// `a` shooting at `b`'s main hull at `a.range`, with `a`'s ewar on it and `b`'s on `a`'s guns.
pub fn against(dogma: &Dogma, a: &Fleet, a_stats: &FleetStats, b: &Fleet, b_stats: &FleetStats) -> Against {
    let main = b_stats.slots.iter().position(|s| s.role == Role::Main).unwrap_or(0);
    against_slot(dogma, a, a_stats, b, b_stats, main)
}

/// As `against`, on a ship of B's `slot` instead of its main hull.
pub fn against_slot(dogma: &Dogma, a: &Fleet, a_stats: &FleetStats, b: &Fleet, b_stats: &FleetStats, slot: usize) -> Against {
    let mut out = Against::default();
    let Some(target) = b_stats.slots.get(slot).or_else(|| b_stats.main()) else { return out };
    let t = &target.computed;
    let a_ewar = ewar_of(a, a_stats);
    let b_ewar = ewar_of(b, b_stats);
    let range = f64::from(a.range);
    // The ships' own modules by range, all on the main hull; counts typed by hand at full strength.
    let (velocity, signature) = if a.ewar_auto {
        let e = ewar::ewar_on(&ships(a_stats), range, 1.0, Some(t));
        (t.mobility.max_velocity * e.speed, t.mobility.signature * e.signature)
    } else {
        ewar_on_target(&a_ewar, t.mobility.max_velocity, t.mobility.signature)
    };
    let on_shooters = b.ewar_auto.then(|| ewar::ewar_at(&ships(b_stats), range, f64::from(a_stats.ships.max(1))));
    out.target_velocity = velocity;
    out.target_signature = signature;
    let profile = TargetProfile { signature: signature as f32, velocity: velocity as f32, transversal: velocity as f32, distance: a.range, radius: 0.0 };
    // Tracking disruptors on the shooters: weaker range and tracking, penalised.
    let td: Vec<(crate::dogma::Op, f64, bool)> = (0..b_ewar.tracking_disruptors).map(|_| (crate::dogma::Op::PostPercent, -17.19, true)).collect();
    let td_factor = crate::calc::combine(1.0, true, &td);
    let mut main_volley_per_ship = 0.0;
    for (i, slot) in a_stats.slots.iter().enumerate() {
        let n = f64::from(slot.count);
        let mut dps = 0.0;
        let mut volley = 0.0;
        for w in &slot.computed.weapons {
            let mut w = match &on_shooters {
                Some(e) => sweep::disrupted(w, e),
                None => w.clone(),
            };
            if on_shooters.is_none() && w.kind == WeaponKind::Turret {
                w.optimal *= td_factor;
                w.falloff *= td_factor;
                w.tracking *= td_factor;
            }
            let f = applied_factor(&w, f64::from(a.range), &profile, slot.computed.drone_speed.unwrap_or(0.0), slot.computed.drones.control_range);
            dps += w.dps * f;
            if w.alpha() {
                volley += w.volley.total() * f;
            }
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
    let landing: f64 = reps_on_one(b_stats, &|_, s| s.count).iter().sum();
    out.break_point = landing * f64::from(b.logi_efficiency.clamp(0.0, 1.0)) * t.rep_intake + t.tank.reps.armor + t.tank.reps.shield + t.tank.reps.hull;
    out.margin = out.applied_dps * eff - out.break_point;
    let _ = dogma;
    out
}

/// Hull groups by what they do in a fleet.
const LOGI_GROUPS: [GroupId; 3] = [832, 1527, 1538];
const COMMAND_GROUPS: [GroupId; 2] = [540, 1534];
const TACKLE_GROUPS: [GroupId; 2] = [541, 894];
const EWAR_GROUPS: [GroupId; 3] = [833, 906, 893];

/// A hull's size class, as the overview's ship symbols show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HullClass {
    Frigate,
    Destroyer,
    Cruiser,
    Battlecruiser,
    Battleship,
    Capital,
    Industrial,
}

pub fn hull_class(dogma: &Dogma, ship: TypeId) -> HullClass {
    let group = dogma.type_(ship).and_then(|t| dogma.groups.iter().find(|g| g.id == t.group)).map_or("", |g| g.name.as_str());
    match group {
        "Frigate" | "Assault Frigate" | "Interceptor" | "Covert Ops" | "Electronic Attack Ship" | "Logistics Frigate" | "Stealth Bomber" | "Expedition Frigate" | "Prototype Exploration Ship" | "Corvette" | "Shuttle" | "Capsule" => HullClass::Frigate,
        "Destroyer" | "Interdictor" | "Command Destroyer" | "Tactical Destroyer" => HullClass::Destroyer,
        "Cruiser" | "Heavy Assault Cruiser" | "Heavy Interdiction Cruiser" | "Logistics" | "Force Recon Ship" | "Combat Recon Ship" | "Strategic Cruiser" | "Flag Cruiser" => HullClass::Cruiser,
        "Combat Battlecruiser" | "Attack Battlecruiser" | "Command Ship" => HullClass::Battlecruiser,
        "Battleship" | "Black Ops" | "Marauder" => HullClass::Battleship,
        "Carrier" | "Dreadnought" | "Lancer Dreadnought" | "Force Auxiliary" | "Supercarrier" | "Titan" | "Command Carrier" | "Capital Industrial Ship" | "Freighter" | "Jump Freighter" => HullClass::Capital,
        _ => HullClass::Industrial,
    }
}

/// A slot's role: the main slot, logi, command, tackle and ewar by hull group, damage dealers
/// (fill slots) as mainline, the rest support.
pub fn role_of(dogma: &Dogma, fleet: &Fleet, slot: usize, ship: TypeId) -> Role {
    if fleet.main.unwrap_or(0) == slot {
        return Role::Main;
    }
    let Some(s) = fleet.slots.get(slot) else { return Role::Support };
    let group = dogma.type_(ship).map(|t| t.group);
    match group {
        Some(g) if LOGI_GROUPS.contains(&g) => Role::Logi,
        Some(g) if COMMAND_GROUPS.contains(&g) => Role::Command,
        Some(g) if TACKLE_GROUPS.contains(&g) => Role::Tackle,
        Some(g) if EWAR_GROUPS.contains(&g) => Role::Ewar,
        _ if matches!(s.rule, CountRule::Fill { .. }) => Role::Mainline,
        _ => Role::Support,
    }
}

/// What a hull does in a fleet, by its group: the rule a doctrine preset gets as a slot.
pub fn role_rule(dogma: &Dogma, ship: TypeId) -> CountRule {
    let Some(group) = dogma.type_(ship).map(|t| t.group) else { return CountRule::Fill { min: 0, max: 1000 } };
    if LOGI_GROUPS.contains(&group) {
        CountRule::Percent(15.0)
    } else if COMMAND_GROUPS.contains(&group) {
        CountRule::Fixed(2)
    } else if TACKLE_GROUPS.contains(&group) {
        CountRule::Fixed(3)
    } else if EWAR_GROUPS.contains(&group) {
        CountRule::Fixed(2)
    } else {
        CountRule::Fill { min: 0, max: 1000 }
    }
}

/// A fleet from a doctrine's presets: every fit a slot with its role's rule, the first line ship
/// the main, and the booster fits by the bursts they carry.
pub fn from_doctrine(dogma: &Dogma, name: &str, fits: &[(String, Fit)], skills: &SkillSet) -> Fleet {
    let mut fleet = Fleet { name: name.to_owned(), ..Fleet::default() };
    for (id, fit) in fits {
        let rule = role_rule(dogma, fit.ship);
        if fleet.main.is_none() && matches!(rule, CountRule::Fill { .. }) {
            fleet.main = Some(fleet.slots.len());
        }
        fleet.slots.push(FleetSlot { fit_id: id.clone(), rule, override_count: None, profile: None, variation: None });
        let calc = Calc::new(dogma, fit, skills, &Env::default());
        for (i, it) in calc.modules() {
            if it.state < State::Active {
                continue;
            }
            for kind in BurstKind::ALL {
                if calc.effect_names(i).any(|n| n == kind.effect()) && !fleet.boosters.iter().any(|(k, _)| *k == kind) {
                    fleet.boosters.push((kind, id.clone()));
                }
            }
        }
    }
    fleet
}

pub mod ammo;
pub mod ewar;
pub mod geometry;
pub mod race;
pub mod scene;
pub mod sweep;

#[cfg(test)]
mod tests;
