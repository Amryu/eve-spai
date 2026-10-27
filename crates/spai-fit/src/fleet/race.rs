//! Two fleets fighting it out at one range, second by second: who kills whom, whether reps keep
//! up, and when a side can no longer break the enemy's reps (loses its critical mass).
//!
//! Each side shoots one primary at a time, chosen by its target order; its logi repair their own
//! primary once locked; a ship made primary switches on its assault damage control if it has
//! one and the fleet allows it; a reactive hardener shifts each of its cycles; disintegrators
//! spool up again on every new target. Volleys land at each weapon's cycle, with a reload after
//! each load.

use super::sweep::{weapons_on, Side};
use super::{Role, SlotStats};
use crate::stats::WeaponGroup;

/// Seconds a step lasts.
const STEP: f64 = 0.25;
/// When a new primary is called, how long until a ship's first volley lands: log-normally spread
/// over the side's ships, most early, a few late. The median and the spread (σ of the log).
pub const REACTION_MEDIAN_S: f64 = 1.5;
pub const REACTION_SPREAD: f64 = 0.6;

/// The reaction delay of the `i`-th of `n` ships: the log-normal's quantiles laid out evenly, so
/// the same fleet always fires the same way.
pub fn reaction(i: usize, n: usize) -> f64 {
    reaction_around(i, n, REACTION_MEDIAN_S)
}

/// `reaction` about another median.
pub fn reaction_around(i: usize, n: usize, median: f64) -> f64 {
    let p = (i as f64 + 0.5) / n.max(1) as f64;
    let z = std::f64::consts::SQRT_2 * super::geometry::erf_inv(2.0 * p - 1.0);
    median * (REACTION_SPREAD * z).exp()
}

/// Whether logi keep a ship of this role locked before it is hurt: the boosters, recons and
/// other support, and the tackle, not the damage dealers or each other.
pub fn prelocked_role(role: Role) -> bool {
    matches!(role, Role::Command | Role::Ewar | Role::Support | Role::Tackle)
}

/// When the `i`-th of `n` logi ships of a slot lands its first repair, from the primary's first
/// damage: its reaction about the fleet's median, its lock unless it held the primary locked
/// already (the first `prelock` share of the slot, on a ship of a role logi keep locked), its
/// place in the spread of arrivals (shuffled against the reaction order), and the cycle an armor
/// repair waits for.
pub fn logi_start(i: usize, n: usize, fleet: &super::Fleet, target: Role, lock: f64, land: f64) -> f64 {
    let prelocked = prelocked_role(target) && (i as f64) < (f64::from(fleet.logi_prelock.clamp(0.0, 1.0)) * n as f64).round();
    let spread = f64::from(fleet.logi_spread_s.max(0.0)) * ((i as f64 * 0.618_033_988_75) + 0.5).fract();
    reaction_around(i, n, f64::from(fleet.logi_reaction_s.max(0.0))) + if prelocked { 0.0 } else { lock } + spread + land
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A ship of `slot` died on `side` (0 for `a`, 1 for `b`).
    Kill { side: usize, slot: usize },
    /// The side's applied damage on its primary no longer beats the reps on it.
    CriticalMassLost { side: usize },
    /// A primary's assault damage control went on.
    Adc { side: usize, slot: usize },
    /// A primary's signature suppressor went on.
    Suppressor { side: usize, slot: usize },
}

#[derive(Clone, Debug, Default)]
pub struct Race {
    /// (seconds, ships alive on a, on b, damage per second a lands, b lands).
    pub timeline: Vec<(f64, u32, u32, f64, f64)>,
    pub events: Vec<(f64, Event)>,
    /// The side left with ships (0 or 1), none when both stand at the end.
    pub winner: Option<usize>,
    pub seconds: f64,
    /// When each side lost its critical mass, if it did.
    pub critical_mass_lost: [Option<f64>; 2],
    /// Per target slot of each side: how its first ship to be primaried fared.
    pub verdicts: [Vec<(usize, Verdict)>; 2],
    /// Raw damage each side sent, and of it what reached a ship already dead.
    pub fired: [f64; 2],
    pub wasted: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// Dead in one volley from full health.
    OneVolley,
    /// Dead before its logi locked it.
    BeforeReps { seconds: f64 },
    /// Dead with reps on it: the damage outpaced them by this much a second.
    Outpaced { seconds: f64, by: f64 },
    /// Called primary and still standing at the end.
    Held,
    /// Never called primary: the fight ended or stalled before its turn.
    NotCalled,
}

/// One ship in the fight.
#[derive(Clone)]
struct Ship {
    slot: usize,
    role: Role,
    hp: [f64; 3],
    max: [f64; 3],
    /// Shield, armor, hull resonances by damage type.
    resonance: [[f64; 4]; 3],
    /// A reactive hardener's resistances now, if it runs one.
    rah: Option<([f64; 4], crate::calc::Rah)>,
    /// Seconds of assault damage control left, and its multiplier; none once used.
    adc: Option<(f64, f64)>,
    adc_until: f64,
    /// A signature suppressor's duration while it runs; none once used.
    sup: Option<f64>,
    sup_until: f64,
    alive: bool,
    /// The damage by type that reached it last, for its hardener to adapt to.
    last_hit: [f64; 4],
    /// A breacher pod's damage a second on it, and until when.
    dot: f64,
    dot_until: f64,
    /// A breach control's share of breacher damage let through and how long it runs; none once used.
    breach: Option<(f64, f64)>,
    breach_until: f64,
}

fn layers(s: &SlotStats) -> ([f64; 3], [[f64; 4]; 3]) {
    let t = &s.computed.tank;
    ([t.shield.hp, t.armor.hp, t.hull.hp], [t.shield.resonance, t.armor.resonance, t.hull.resonance])
}

/// An active assault damage control's resistance multiplier and duration, from its module.
fn adc_of(dogma: &crate::Dogma, s: &SlotStats) -> Option<(f64, f64)> {
    let m = s.fit.modules.iter().find(|m| dogma.type_(m.type_id).is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|x| x.name == "moduleBonusAssaultDamageControl"))))?;
    let t = dogma.type_(m.type_id)?;
    let mult = dogma.attr_named("resistanceMultiplier").and_then(|a| t.attr(a.id))?;
    let duration = t.attr(crate::dogma::attr::DURATION).map_or(0.0, |d| f64::from(d) / 1000.0);
    Some((f64::from(mult), duration))
}

/// A fitted signature suppressor: what running it does to the signature its online bonus leaves
/// (its active multiplier over its passive one), and for how long.
pub fn suppressor_of(dogma: &crate::Dogma, s: &SlotStats) -> Option<(f64, f64)> {
    let named = |t: &crate::dogma::TypeDef, n: &str| dogma.attr_named(n).and_then(|a| t.attr(a.id)).map(f64::from);
    s.fit.modules.iter().filter(|m| m.state >= crate::fit::State::Online).find_map(|m| {
        let t = dogma.type_(m.type_id)?;
        let active = named(t, "signatureSuppressorSignatureRadiusBonusActive")?;
        let passive = named(t, "signatureSuppressorSignatureRadiusBonusPassive").filter(|p| *p > 0.0).unwrap_or(1.0);
        let duration = named(t, "durationHighisGood").map_or(0.0, |d| d / 1000.0);
        Some((active / passive, duration))
    })
}

fn ships(dogma: &crate::Dogma, side: &Side, adc: bool) -> Vec<Ship> {
    let mut out = Vec::new();
    for (k, s) in side.stats.slots.iter().enumerate() {
        let (hp, resonance) = layers(s);
        let rah = s.computed.tank.rah.filter(|r| r.shift > 0.0).map(|r| (r.start, r));
        let sup = if adc { suppressor_of(dogma, s) } else { None };
        let adc = if adc { adc_of(dogma, s) } else { None };
        let breach = breach_of(dogma, s);
        for _ in 0..s.count {
            out.push(Ship { slot: k, role: s.role, hp, max: hp, resonance, rah, adc, adc_until: 0.0, sup: sup.map(|(_, d)| d), sup_until: 0.0, alive: true, last_hit: [0.0; 4], dot: 0.0, dot_until: 0.0, breach, breach_until: 0.0 });
        }
    }
    out
}

/// A fitted breach control: the share of breacher damage it lets through while it runs, and for
/// how long. It goes on when the first pod lands.
fn breach_of(dogma: &crate::Dogma, s: &SlotStats) -> Option<(f64, f64)> {
    s.fit.modules.iter().filter(|m| m.state >= crate::fit::State::Online).find_map(|m| {
        let t = dogma.type_(m.type_id)?;
        let cut = dogma.attr_named("breacherPodActivatedDamageReceivedPercentage").and_then(|a| t.attr(a.id))?;
        let duration = t.attr(crate::dogma::attr::DURATION).map_or(0.0, |d| f64::from(d) / 1000.0);
        Some(((1.0 + f64::from(cut) / 100.0).max(0.0), duration))
    })
}

/// Damage past every resistance, shield first.
fn hit_true(ship: &mut Ship, mut amount: f64) {
    for layer in 0..3 {
        let take = amount.min(ship.hp[layer]);
        ship.hp[layer] -= take;
        amount -= take;
        if amount <= 0.0 {
            return;
        }
    }
    ship.alive = false;
}

/// The next primary: the first alive ship of the first role in the order present.
/// `pick` past the ships other fire groups already shoot; one of those when nothing else is left.
fn pick_except(order: &[Role], ships: &[Ship], taken: &[usize]) -> Option<usize> {
    let free = |i: usize, s: &Ship| s.alive && !taken.contains(&i);
    order
        .iter()
        .chain(Role::DEFAULT_ORDER.iter())
        .find_map(|r| ships.iter().enumerate().position(|(i, s)| free(i, s) && s.role == *r))
        .or_else(|| ships.iter().enumerate().position(|(i, s)| free(i, s)))
        .or_else(|| ships.iter().position(|s| s.alive))
}

/// How many ships besides the primary a weapon's arc or area damage reaches among `n` alive.
fn extra_targets(memo: &mut std::collections::HashMap<(usize, u32, u64, u64), f64>, formation: super::Formation, n: usize, w: &WeaponGroup) -> f64 {
    if n < 2 || (w.arc_targets == 0 && w.splash <= 0.0) {
        return 0.0;
    }
    *memo.entry((n, w.arc_targets, w.arc_range.to_bits(), w.splash.to_bits())).or_insert_with(|| {
        if w.arc_targets > 0 {
            super::geometry::arc_reach(formation, n, w.arc_range, w.arc_targets)
        } else {
            super::geometry::splash_reach(formation, n, w.splash)
        }
    })
}

/// Damage per type landing on `ship`, layer by layer, with its hardener and ADC.
fn hit(ship: &mut Ship, damage: [f64; 4], now: f64) {
    if damage.iter().sum::<f64>() > 0.0 {
        ship.last_hit = damage;
    }
    let adc = if now < ship.adc_until { ship.adc.map_or(1.0, |(m, _)| m) } else { 1.0 };
    let mut left = damage;
    for layer in 0..3 {
        let mut res = ship.resonance[layer];
        if layer == 1 {
            if let Some((r, rah)) = &ship.rah {
                res = std::array::from_fn(|i| rah.armor_before[i] * (1.0 - r[i]));
            }
        }
        let through: f64 = (0..4).map(|i| left[i] * res[i] * adc).sum();
        if through <= ship.hp[layer] {
            ship.hp[layer] -= through;
            return;
        }
        // What this layer could not hold carries on, scaled back to raw damage.
        let share = 1.0 - ship.hp[layer] / through.max(1e-9);
        ship.hp[layer] = 0.0;
        left = left.map(|x| x * share);
    }
    ship.alive = false;
}

/// One side's guns: per weapon group (slot, the group, its share landing on a target of each
/// slot, and on one whose signature suppressor runs).
struct Guns {
    /// Per weapon group: its slot, the group, its share on each target slot (plain and under a
    /// suppressor), and whether it is the second part of a stack split onto two targets.
    groups: Vec<(usize, WeaponGroup, Vec<f64>, Vec<f64>, bool)>,
}

fn guns(dogma: &crate::Dogma, own: &Side, other: &Side, distance: f64) -> Guns {
    // Each target slot as it is, and shrunk by its running suppressor where it has one.
    let shrunk: Vec<Option<SlotStats>> = other
        .stats
        .slots
        .iter()
        .map(|t| {
            let (ratio, _) = suppressor_of(dogma, t)?;
            let mut t = t.clone();
            t.computed.mobility.signature *= ratio;
            t.computed.signature_mods.push((crate::dogma::Op::PostMul, ratio, false));
            Some(t)
        })
        .collect();
    let mut groups = Vec::new();
    for (k, s) in own.stats.slots.iter().enumerate() {
        if s.count == 0 {
            continue;
        }
        let per_target: Vec<Vec<(WeaponGroup, f64)>> = other.stats.slots.iter().map(|t| weapons_on(own, other, k, t, distance)).collect();
        let per_shrunk: Vec<Option<Vec<(WeaponGroup, f64)>>> = shrunk.iter().map(|t| t.as_ref().map(|t| weapons_on(own, other, k, t, distance))).collect();
        let apart = own.fleet.slots.get(s.slot).and_then(|x| x.splits.iter().find(|sp| sp.apart).copied());
        for (g, w) in s.computed.weapons.iter().enumerate() {
            let shares: Vec<f64> = per_target.iter().map(|v| v.get(g).map_or(0.0, |(_, f)| *f)).collect();
            let suppressed: Vec<f64> = per_shrunk.iter().zip(&shares).map(|(v, plain)| v.as_ref().and_then(|v| v.get(g)).map_or(*plain, |(_, f)| *f)).collect();
            let w = per_target.first().and_then(|v| v.get(g)).map_or_else(|| w.clone(), |(w, _)| w.clone());
            match apart.filter(|sp| sp.weapon == w.type_id) {
                // Both parts on one charge come as one group: shared out by their modules.
                Some(sp) if sp.charges[0] == sp.charges[1] => {
                    let [a, b] = sp.counts(w.count);
                    for (n, second) in [(a, false), (b, true)] {
                        let f = f64::from(n) / f64::from(w.count.max(1));
                        let mut part = w.clone();
                        part.dps *= f;
                        part.volley = part.volley.scaled(f);
                        part.count = n;
                        groups.push((k, part, shares.clone(), suppressed.clone(), second));
                    }
                }
                Some(sp) => groups.push((k, w.clone(), shares, suppressed, w.charge == Some(sp.charges[1]))),
                None => groups.push((k, w, shares, suppressed, false)),
            }
        }
    }
    Guns { groups }
}

/// A side's logi: per ship, its repairs per second by layer (shield, armor, hull), its lock time
/// on the given signature and its reach.
/// A side's logi: per slot, each ship's repairs per second by layer (shield, armor, hull), and
/// how long after its lock the first repair lands: an armor or hull repair at its cycle's end, a
/// shield one at its start.
fn logi(side: &Side) -> Vec<(usize, [f64; 3], f64)> {
    side.stats
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.computed.remote.modules.iter().any(|m| m.kind.is_rep()))
        .map(|(k, s)| {
            let r = &s.computed.remote;
            let land = r.modules.iter().filter(|m| matches!(m.kind, crate::stats::RemoteKind::ArmorRep | crate::stats::RemoteKind::HullRep)).map(|m| m.cycle_s).fold(0.0, f64::max);
            (k, [r.shield_rep, r.armor_rep, r.hull_rep], land)
        })
        .collect()
}

/// Whether a side still breaks its primary: the damage getting through beats the repairs on it,
/// or one volley of the side kills it from full health, which no repair answers.
pub fn holds_critical_mass(through: f64, reps: f64, volley: f64, full_ehp: f64) -> bool {
    reps <= 0.0 || through > reps || volley >= full_ehp
}

/// The fight at `distance`, for at most `limit` seconds.
pub fn race(dogma: &crate::Dogma, a: &Side, b: &Side, distance: f64, limit: f64) -> Race {
    let sides = [a, b];
    let mut fleets = [ships(dogma, a, a.fleet.adc), ships(dogma, b, b.fleet.adc)];
    let guns = [guns(dogma, a, b, distance), guns(dogma, b, a, distance)];
    let logis = [logi(a), logi(b)];
    let mut out = Race::default();
    // Each side's fire groups: a slot's ships by turns, each group on its own primary. A side
    // with a stack split onto two targets has a second primary per group, which that stack's
    // second part shoots.
    let split = [a.fleet.split_fire.max(1) as usize, b.fleet.split_fire.max(1) as usize];
    let doubled = [0, 1].map(|side| guns[side].groups.iter().any(|g| g.4));
    let groups = [0, 1].map(|side| split[side] * if doubled[side] { 2 } else { 1 });
    // The group a ship's weapon group fires with: its turn, in the second set for a second part.
    let aim = |side: usize, ship: usize, second: bool| ship % split[side] + if second { split[side] } else { 0 };
    let mut primary: [Vec<Option<usize>>; 2] = [vec![None; groups[0]], vec![None; groups[1]]];
    for side in 0..2 {
        for g in 0..groups[side] {
            let taken: Vec<usize> = primary[side].iter().flatten().copied().collect();
            primary[side][g] = pick_except(&sides[side].fleet.target_order, &fleets[1 - side], &taken);
        }
    }
    // Per side and group: when its primary was called, when it first took damage, when repairs
    // first landed on it, and the ship whose verdict is still to come with when it was called.
    let mut since: [Vec<f64>; 2] = [vec![0.0; groups[0]], vec![0.0; groups[1]]];
    let mut first_hit: [Vec<Option<f64>>; 2] = [vec![None; groups[0]], vec![None; groups[1]]];
    let mut reps_landed: [Vec<Option<f64>>; 2] = [vec![None; groups[0]], vec![None; groups[1]]];
    let mut verdict_open: [Vec<Option<(usize, f64)>>; 2] = [primary[0].iter().map(|p| p.map(|p| (p, 0.0))).collect(), primary[1].iter().map(|p| p.map(|p| (p, 0.0))).collect()];
    // Volleys on their way, per side: when they land, on which ship, by damage type. Missiles
    // fly for distance over velocity; the rest land as they are fired.
    let mut in_flight: [Vec<(f64, usize, [f64; 4])>; 2] = [Vec::new(), Vec::new()];
    // Per side, by ships alive, how many more of them a vorton's arc strikes.
    let mut arc_memo: [std::collections::HashMap<(usize, u32, u64, u64), f64>; 2] = Default::default();
    // Per side, the slots of it that have been called primary.
    let mut called: [Vec<bool>; 2] = [vec![false; a.stats.slots.len()], vec![false; b.stats.slots.len()]];
    // Per side, per weapon group, each ship's next volley: after its reaction to the first call.
    // With the volleys fired since the last reload.
    let mut next_fire: [Vec<Vec<(f64, u32)>>; 2] = [0, 1].map(|side| {
        guns[side].groups.iter().map(|(k, _, _, _, _)| {
            let n = sides[side].stats.slots.get(*k).map_or(0, |s| s.count as usize);
            (0..n).map(|i| (reaction(i, n), 0)).collect()
        }).collect()
    });
    for side in 0..2 {
        for p in primary[side].clone().into_iter().flatten() {
            start_adc(&mut fleets[1 - side][p], 0.0, &mut out, 1 - side);
        }
    }
    let mut t = 0.0;
    while t < limit {
        let mut landed = [0.0f64; 2];
        // Both sides fire on the ships alive at the step's start: neither shoots first.
        let alive_at: [Vec<usize>; 2] = [0, 1].map(|side| (0..sides[side].stats.slots.len()).map(|k| fleets[side].iter().filter(|s| s.alive && s.slot == k).count()).collect());
        for side in 0..2 {
            let gn = groups[side];
            let eff = f64::from(sides[side].fleet.volley_efficiency.clamp(0.01, 1.0));
            for g in 0..gn {
                if let Some(p) = primary[side][g] {
                    called[1 - side][fleets[1 - side][p].slot] = true;
                }
            }
            // What lands on each ship this step so far: the volleys due now, then each turret
            // shot fired below.
            let mut pending: std::collections::HashMap<usize, [f64; 4]> = std::collections::HashMap::new();
            for (at, ship, d) in &in_flight[side] {
                if *at <= t + 1e-9 {
                    let x = pending.entry(*ship).or_insert([0.0; 4]);
                    (0..4).for_each(|i| x[i] += d[i]);
                }
            }
            // Fire: each ship once its reaction is over and its guns have cycled since its last
            // volley, on its group's primary.
            for (wg, (k, w, plain, suppressed, second)) in guns[side].groups.iter().enumerate() {
                let alive = alive_at[side].get(*k).copied().unwrap_or(0);
                if alive == 0 || w.cycle_s <= 0.0 {
                    continue;
                }
                let next = &mut next_fire[side][wg];
                next.truncate(alive);
                let mut firing = vec![0.0f64; gn];
                // A turret lands as it fires: one whose primary the shots already landing kill
                // holds its volley for the next primary instead of spending it on a wreck. A
                // missile in flight when its target dies is lost.
                let hitscan = w.dot.is_none() && w.missile.as_ref().is_none_or(|m| m.velocity <= 0.0);
                for (i, (at, fired)) in next.iter_mut().enumerate() {
                    let g = aim(side, i, *second);
                    let Some(p) = primary[side][g] else { continue };
                    if *at > t {
                        continue;
                    }
                    if hitscan {
                        let mut after = fleets[1 - side][p].clone();
                        if let Some(d) = pending.get(&p) {
                            hit(&mut after, *d, t);
                        }
                        if !after.alive {
                            continue;
                        }
                        let target_slot = fleets[1 - side][p].slot;
                        let shares = if fleets[1 - side][p].sup_until > t { suppressed } else { plain };
                        let share = shares.get(target_slot).copied().unwrap_or(0.0);
                        let spool = w.spool_at(t - since[side][g]) / (1.0 + w.spool_max * w.spool_frac);
                        let scale = eff * share * spool;
                        let v = &w.volley;
                        let x = pending.entry(p).or_insert([0.0; 4]);
                        for (n, d) in [v.em, v.thermal, v.kinetic, v.explosive].into_iter().enumerate() {
                            x[n] += d * scale;
                        }
                    }
                    firing[g] += 1.0;
                    *at += w.cycle_s;
                    *fired += 1;
                    if w.shots.is_some_and(|n| n > 0 && *fired >= n) {
                        *at += w.reload_s;
                        *fired = 0;
                    }
                }
                for g in 0..gn {
                    let Some(p) = primary[side][g] else { continue };
                    if firing[g] == 0.0 {
                        continue;
                    }
                    let target_slot = fleets[1 - side][p].slot;
                    let shares = if fleets[1 - side][p].sup_until > t { suppressed } else { plain };
                    let share = shares.get(target_slot).copied().unwrap_or(0.0);
                    let spool = w.spool_at(t - since[side][g]) / (1.0 + w.spool_max * w.spool_frac);
                    let v = &w.volley;
                    let scale = firing[g] * eff * share * spool;
                    let flight = w.missile.as_ref().filter(|m| m.velocity > 0.0).map_or(0.0, |m| distance / m.velocity);
                    if let Some(d) = w.dot.filter(|_| share > 0.0) {
                        // One pod's damage runs on a ship at a time: the strongest, renewed.
                        let ship = &mut fleets[1 - side][p];
                        let total: f64 = ship.max.iter().sum();
                        ship.dot = if ship.dot_until > t { ship.dot.max(d.on(total)) } else { d.on(total) };
                        ship.dot_until = t + flight + d.duration_s;
                        // Its breach control goes on once, as the first pod lands.
                        if let Some((cut, duration)) = ship.breach.filter(|b| b.1 > 0.0) {
                            ship.breach_until = t + flight + duration;
                            ship.breach = Some((cut, 0.0));
                        }
                        continue;
                    }
                    in_flight[side].push((t + flight, p, [v.em * scale, v.thermal * scale, v.kinetic * scale, v.explosive * scale]));
                    out.fired[side] += v.total() * scale;
                    // A vorton's arc, a bomb's blast or a doomsday's beam goes on to the ships near
                    // the primary, as many as its formation puts within reach: whole strikes on the
                    // first, a share on the last.
                    if w.arc_targets > 0 || w.splash > 0.0 {
                        let others: Vec<usize> = (0..fleets[1 - side].len()).filter(|j| *j != p && fleets[1 - side][*j].alive).collect();
                        let n = others.len() + 1;
                        let reach = extra_targets(&mut arc_memo[1 - side], sides[1 - side].fleet.formation, n, w);
                        for (j, o) in others.iter().enumerate() {
                            let part = (reach - j as f64).clamp(0.0, 1.0);
                            if part <= 0.0 {
                                break;
                            }
                            let share = plain.get(fleets[1 - side][*o].slot).copied().unwrap_or(0.0);
                            let scale = firing[g] * eff * share * spool * part;
                            in_flight[side].push((t, *o, [v.em * scale, v.thermal * scale, v.kinetic * scale, v.explosive * scale]));
                        }
                    }
                }
            }
            // What lands now, per ship; on a ship already dead it is lost.
            let mut arriving: Vec<(usize, [f64; 4])> = Vec::new();
            in_flight[side].retain(|(at, ship, d)| {
                if *at > t + 1e-9 {
                    return true;
                }
                if fleets[1 - side][*ship].alive {
                    match arriving.iter_mut().find(|(s, _)| s == ship) {
                        Some((_, x)) => (0..4).for_each(|i| x[i] += d[i]),
                        None => arriving.push((*ship, *d)),
                    }
                } else {
                    out.wasted[side] += d.iter().sum::<f64>();
                }
                false
            });
            // The timeline carries what lands per second sustained, not one step's volley, a
            // vorton's arc included.
            let alive_other = fleets[1 - side].iter().filter(|s| s.alive).count();
            let arcs: Vec<f64> = guns[side].groups.iter().map(|(_, w, _, _, _)| 1.0 + extra_targets(&mut arc_memo[1 - side], sides[1 - side].fleet.formation, alive_other, w)).collect();
            for g in 0..gn {
                let Some(p) = primary[side][g] else { continue };
                let target_slot = fleets[1 - side][p].slot;
                let suppressing = fleets[1 - side][p].sup_until > t;
                landed[side] += guns[side].groups.iter().zip(&arcs).map(|((k, w, plain, suppressed, second), arc)| {
                    if *second != (g >= split[side]) {
                        return 0.0;
                    }
                    let shares = if suppressing { suppressed } else { plain };
                    let alive = alive_at[side].get(*k).copied().unwrap_or(0) as f64 / split[side] as f64;
                    let spool = w.spool_at(t - since[side][g]) / (1.0 + w.spool_max * w.spool_frac);
                    if w.dot.is_some() {
                        return 0.0;
                    }
                    w.dps * spool * alive * arc * shares.get(target_slot).copied().unwrap_or(0.0)
                }).sum::<f64>() * eff;
            }
            // Breacher pods tick on whatever they were put on, through no resistance.
            for (i, ship) in fleets[1 - side].iter_mut().enumerate().filter(|(_, s)| s.alive && s.dot_until > t) {
                let cut = if t < ship.breach_until { ship.breach.map_or(1.0, |b| b.0) } else { 1.0 };
                let rate = ship.dot * cut;
                landed[side] += rate;
                hit_true(ship, rate * STEP);
                // A ship the pods finish counts as a kill like one a volley does.
                if !ship.alive && !arriving.iter().any(|(p, _)| *p == i) {
                    arriving.push((i, [0.0; 4]));
                }
            }
            let full: Vec<bool> = arriving.iter().map(|(p, _)| fleets[1 - side][*p].hp == fleets[1 - side][*p].max).collect();
            for (p, d) in &arriving {
                hit(&mut fleets[1 - side][*p], *d, t);
                for g in 0..gn {
                    if primary[side][g] == Some(*p) && first_hit[side][g].is_none() && d.iter().sum::<f64>() > 0.0 {
                        first_hit[side][g] = Some(t);
                    }
                }
            }
            // Repairs from the target side's logi: none before a primary is hurt. They share out
            // over the hurt primaries by turns; each logi ship after its own reaction, its lock
            // on its ship and its first cycle, times the logistics efficiency, lessened as they
            // pile up on one ship as the game does it (`rep_landing`), times what the ship takes
            // in. Only damage present is healed, so what overreps is lost.
            let hurt: Vec<usize> = (0..gn).filter(|g| first_hit[side][*g].is_some() && primary[side][*g].is_some_and(|p| fleets[1 - side][p].alive)).collect();
            let mut reps_on = vec![0.0f64; gn];
            let logi_eff = f64::from(sides[1 - side].fleet.logi_efficiency.clamp(0.0, 1.0));
            for (turn, g) in hurt.iter().enumerate() {
                let (Some(p), Some(hit_at)) = (primary[side][*g], first_hit[side][*g]) else { continue };
                // Another group on the same ship shares its repairs, not doubles them.
                if hurt[..turn].iter().any(|x| primary[side][*x] == Some(p)) {
                    continue;
                }
                let target_slot = fleets[1 - side][p].slot;
                let intake = sides[1 - side].stats.slots[target_slot].computed.rep_intake;
                let signature = sides[1 - side].stats.slots[target_slot].computed.mobility.signature;
                let active: Vec<u32> = sides[1 - side].stats.slots.iter().enumerate().map(|(k, _)| {
                    let Some((_, _, land)) = logis[1 - side].iter().find(|(x, _, _)| *x == k) else { return 0 };
                    let n = alive_at[1 - side].get(k).copied().unwrap_or(0);
                    let lock = sides[1 - side].stats.slots[k].computed.sensors.lock_time_s(signature);
                    let role = fleets[1 - side][p].role;
                    (0..n).filter(|i| i % hurt.len() == turn && t >= hit_at + logi_start(*i, n, sides[1 - side].fleet, role, lock, *land)).count() as u32
                }).collect();
                if active.iter().any(|n| *n > 0) {
                    let since = reps_landed[side][*g].map_or(0.0, |at| t - at);
                    let landing = super::reps_on_one_at(sides[1 - side].stats, &|k, _| active[k], Some(since));
                    let ship = &mut fleets[1 - side][p];
                    for layer in 0..3 {
                        let rate = landing[layer] * logi_eff * intake;
                        reps_on[*g] += rate;
                        ship.hp[layer] = (ship.hp[layer] + rate * STEP).min(ship.max[layer]);
                    }
                }
            }
            for g in 0..gn {
                if reps_on[g] > 0.0 && reps_landed[side][g].is_none() {
                    reps_landed[side][g] = Some(t);
                }
            }
            // Critical mass, on the first group's primary: the damage landing there no longer
            // beats the repairs on it, and one volley of the side no longer kills it outright,
            // which no repair answers.
            if let (None, Some(p), Some(_)) = (out.critical_mass_lost[side], primary[side][0], reps_landed[side][0]) {
                let target_slot = fleets[1 - side][p].slot;
                let suppressing = fleets[1 - side][p].sup_until > t;
                let sustained: f64 = guns[side].groups.iter().filter(|g| !g.4).map(|(k, w, plain, suppressed, _)| {
                    let shares = if suppressing { suppressed } else { plain };
                    let alive = alive_at[side].get(*k).copied().unwrap_or(0) as f64 / split[side] as f64;
                    w.dps * alive * shares.get(target_slot).copied().unwrap_or(0.0)
                }).sum::<f64>() * eff;
                // Through the layer now being worn down.
                let ship = &fleets[1 - side][p];
                let layer = ship.hp.iter().position(|h| *h > 0.0).unwrap_or(2);
                let avg_res = ship.resonance[layer].iter().sum::<f64>() / 4.0;
                let volley: f64 = guns[side].groups.iter().filter(|g| !g.4 && g.1.alpha()).map(|(k, w, plain, suppressed, _)| {
                    let shares = if suppressing { suppressed } else { plain };
                    let alive = alive_at[side].get(*k).copied().unwrap_or(0) as f64 / split[side] as f64;
                    w.volley.total() * alive * shares.get(target_slot).copied().unwrap_or(0.0)
                }).sum::<f64>() * eff;
                let full: f64 = (0..3).map(|l| ship.max[l] / (ship.resonance[l].iter().sum::<f64>() / 4.0).max(1e-6)).sum();
                if !holds_critical_mass(sustained * avg_res, reps_on[0], volley, full) {
                    out.critical_mass_lost[side] = Some(t);
                    out.events.push((t, Event::CriticalMassLost { side }));
                }
            }
            // Kills: the verdict for a slot's first called ship, then a new call for every group
            // that shot it.
            for (n_arrived, (p, _)) in arriving.iter().enumerate() {
                if fleets[1 - side][*p].alive {
                    continue;
                }
                let target_slot = fleets[1 - side][*p].slot;
                out.events.push((t, Event::Kill { side: 1 - side, slot: target_slot }));
                for g in 0..gn {
                    if primary[side][g] != Some(*p) {
                        continue;
                    }
                    if let Some((open, at)) = verdict_open[side][g] {
                        if open == *p && !out.verdicts[1 - side].iter().any(|(s, _)| *s == target_slot) {
                            let seconds = t - at;
                            let v = if full[n_arrived] && seconds < STEP * 2.0 {
                                Verdict::OneVolley
                            } else if reps_landed[side][g].is_none() && !logis[1 - side].is_empty() {
                                Verdict::BeforeReps { seconds }
                            } else {
                                Verdict::Outpaced { seconds, by: (landed[side] / split[side] as f64 - reps_on[g]).max(0.0) }
                            };
                            out.verdicts[1 - side].push((target_slot, v));
                        }
                    }
                    let taken: Vec<usize> = primary[side].iter().enumerate().filter(|(x, _)| *x != g).filter_map(|(_, q)| *q).filter(|q| fleets[1 - side][*q].alive).collect();
                    primary[side][g] = pick_except(&sides[side].fleet.target_order, &fleets[1 - side], &taken);
                    since[side][g] = t + STEP;
                    first_hit[side][g] = None;
                    reps_landed[side][g] = None;
                    // A new call: the ships keep the next primaries locked, so a gun that has
                    // cycled shoots it from the next step, a held volley at once. Only the
                    // opening call waits for the ships to react.
                    for next in next_fire[side].iter_mut() {
                        for (_, (at, _)) in next.iter_mut().enumerate().filter(|(i, _)| i % split[side] == g % split[side]) {
                            *at = at.max(since[side][g]);
                        }
                    }
                    verdict_open[side][g] = primary[side][g].map(|n| (n, t + STEP));
                    if let Some(n) = primary[side][g] {
                        start_adc(&mut fleets[1 - side][n], t + STEP, &mut out, 1 - side);
                    }
                }
            }
        }
        // Reactive hardeners take a step at the end of each of their cycles.
        for fleet in &mut fleets {
            for s in fleet.iter_mut().filter(|s| s.alive) {
                if let Some((r, rah)) = &mut s.rah {
                    if rah.cycle_s > 0.0 && ((t + STEP) / rah.cycle_s).floor() != (t / rah.cycle_s).floor() {
                        let sum: f64 = s.last_hit.iter().sum();
                        if sum > 0.0 {
                            *r = crate::calc::patch::adapt_step(*r, rah.armor_before, s.last_hit.map(|x| x / sum), rah.shift);
                        }
                    }
                }
            }
        }
        let alive = [fleets[0].iter().filter(|s| s.alive).count() as u32, fleets[1].iter().filter(|s| s.alive).count() as u32];
        out.timeline.push((t, alive[0], alive[1], landed[0], landed[1]));
        t += STEP;
        if alive[0] == 0 || alive[1] == 0 {
            out.winner = if alive[0] > 0 { Some(0) } else if alive[1] > 0 { Some(1) } else { None };
            break;
        }
    }
    out.seconds = t;
    for side in 0..2 {
        for (k, s) in sides[side].stats.slots.iter().enumerate() {
            if s.count > 0 && !out.verdicts[side].iter().any(|(x, _)| *x == k) && fleets[side].iter().any(|x| x.slot == k && x.alive) {
                out.verdicts[side].push((k, if called[side][k] { Verdict::Held } else { Verdict::NotCalled }));
            }
        }
    }
    out
}

/// A ship just called primary switches on its assault damage control and its signature
/// suppressor, each once per fight: their reactivation delays outlast any fight here.
fn start_adc(ship: &mut Ship, now: f64, out: &mut Race, side: usize) {
    if let Some(duration) = ship.sup.take() {
        ship.sup_until = now + duration;
        out.events.push((now, Event::Suppressor { side, slot: ship.slot }));
    }
    if let Some((_, duration)) = ship.adc {
        ship.adc_until = now + duration;
        out.events.push((now, Event::Adc { side, slot: ship.slot }));
        // Once per fight: its reactivation delay outlasts any fight here.
        let mult = ship.adc.map_or(1.0, |(m, _)| m);
        ship.adc = Some((mult, 0.0));
    }
}
