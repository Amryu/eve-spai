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
    let p = (i as f64 + 0.5) / n.max(1) as f64;
    let z = std::f64::consts::SQRT_2 * super::geometry::erf_inv(2.0 * p - 1.0);
    REACTION_MEDIAN_S * (REACTION_SPREAD * z).exp()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A ship of `slot` died on `side` (0 for `a`, 1 for `b`).
    Kill { side: usize, slot: usize },
    /// The side's applied damage on its primary no longer beats the reps on it.
    CriticalMassLost { side: usize },
    /// A primary's assault damage control went on.
    Adc { side: usize, slot: usize },
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// Dead in one volley from full health.
    OneVolley,
    /// Dead before its logi locked it.
    BeforeReps { seconds: f64 },
    /// Dead with reps on it: the damage outpaced them by this much a second.
    Outpaced { seconds: f64, by: f64 },
    /// Never died while primary.
    Held,
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
    alive: bool,
    /// The damage by type that reached it last, for its hardener to adapt to.
    last_hit: [f64; 4],
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

fn ships(dogma: &crate::Dogma, side: &Side, adc: bool) -> Vec<Ship> {
    let mut out = Vec::new();
    for (k, s) in side.stats.slots.iter().enumerate() {
        let (hp, resonance) = layers(s);
        let rah = s.computed.tank.rah.filter(|r| r.shift > 0.0).map(|r| (r.start, r));
        let adc = if adc { adc_of(dogma, s) } else { None };
        for _ in 0..s.count {
            out.push(Ship { slot: k, role: s.role, hp, max: hp, resonance, rah, adc, adc_until: 0.0, alive: true, last_hit: [0.0; 4] });
        }
    }
    out
}

/// The next primary: the first alive ship of the first role in the order present.
fn pick(order: &[Role], ships: &[Ship]) -> Option<usize> {
    order.iter().chain(Role::DEFAULT_ORDER.iter()).find_map(|r| ships.iter().position(|s| s.alive && s.role == *r)).or_else(|| ships.iter().position(|s| s.alive))
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

/// One side's guns: per weapon group (slot, the group, its share landing on a target of each slot).
struct Guns {
    groups: Vec<(usize, WeaponGroup, Vec<f64>)>,
}

fn guns(own: &Side, other: &Side, distance: f64) -> Guns {
    let mut groups = Vec::new();
    for (k, s) in own.stats.slots.iter().enumerate() {
        if s.count == 0 {
            continue;
        }
        let per_target: Vec<Vec<(WeaponGroup, f64)>> = other.stats.slots.iter().map(|t| weapons_on(own, other, k, t, distance)).collect();
        for (g, w) in s.computed.weapons.iter().enumerate() {
            let shares: Vec<f64> = per_target.iter().map(|v| v.get(g).map_or(0.0, |(_, f)| *f)).collect();
            let w = per_target.first().and_then(|v| v.get(g)).map_or_else(|| w.clone(), |(w, _)| w.clone());
            groups.push((k, w, shares));
        }
    }
    Guns { groups }
}

/// A side's logi: per ship, its repairs per second by layer (shield, armor, hull), its lock time
/// on the given signature and its reach.
fn logi(side: &Side) -> Vec<(usize, [f64; 3], f64)> {
    side.stats
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.computed.remote.modules.iter().any(|m| m.kind.is_rep()))
        .map(|(k, s)| {
            let r = &s.computed.remote;
            (k, [r.shield_rep, r.armor_rep, r.hull_rep], r.rep_range)
        })
        .collect()
}

/// The fight at `distance`, for at most `limit` seconds.
pub fn race(dogma: &crate::Dogma, a: &Side, b: &Side, distance: f64, limit: f64) -> Race {
    let sides = [a, b];
    let mut fleets = [ships(dogma, a, a.fleet.adc), ships(dogma, b, b.fleet.adc)];
    let guns = [guns(a, b, distance), guns(b, a, distance)];
    let logis = [logi(a), logi(b)];
    let mut out = Race::default();
    let mut primary = [pick(&a.fleet.target_order, &fleets[1]), pick(&b.fleet.target_order, &fleets[0])];
    // When the current primary was picked, for spool, lock and the first-volley verdict.
    let mut since = [0.0f64; 2];
    let mut verdict_open: [Option<(usize, f64)>; 2] = [primary[0].map(|p| (p, 0.0)), primary[1].map(|p| (p, 0.0))];
    // Per side, per weapon group, each ship's next volley: after its reaction to the first call.
    // With the volleys fired since the last reload.
    let mut next_fire: [Vec<Vec<(f64, u32)>>; 2] = [0, 1].map(|side| {
        guns[side].groups.iter().map(|(k, _, _)| {
            let n = sides[side].stats.slots.get(*k).map_or(0, |s| s.count as usize);
            (0..n).map(|i| (reaction(i, n), 0)).collect()
        }).collect()
    });
    for side in 0..2 {
        if let Some(p) = primary[side] {
            start_adc(&mut fleets[1 - side][p], 0.0, &mut out, 1 - side);
        }
    }
    let mut t = 0.0;
    while t < limit {
        let mut landed = [0.0f64; 2];
        // Both sides fire on the ships alive at the step's start: neither shoots first.
        let alive_at: [Vec<usize>; 2] = [0, 1].map(|side| (0..sides[side].stats.slots.len()).map(|k| fleets[side].iter().filter(|s| s.alive && s.slot == k).count()).collect());
        for side in 0..2 {
            let Some(p) = primary[side] else { continue };
            let target_slot = fleets[1 - side][p].slot;
            let mut damage = [0.0f64; 4];
            for (g, (k, w, shares)) in guns[side].groups.iter().enumerate() {
                let alive = alive_at[side].get(*k).copied().unwrap_or(0);
                if alive == 0 || w.cycle_s <= 0.0 {
                    continue;
                }
                // Each ship fires once its reaction to the call is over and its guns have cycled
                // since its last volley: volleys spread out rather than landing at once.
                let elapsed = t - since[side];
                let next = &mut next_fire[side][g];
                next.truncate(alive);
                let mut firing = 0.0;
                for (at, fired) in next.iter_mut() {
                    if *at <= t {
                        firing += 1.0;
                        *at += w.cycle_s;
                        *fired += 1;
                        if w.shots.is_some_and(|n| n > 0 && *fired >= n) {
                            *at += w.reload_s;
                            *fired = 0;
                        }
                    }
                }
                if firing == 0.0 {
                    continue;
                }
                let eff = f64::from(sides[side].fleet.volley_efficiency.clamp(0.01, 1.0));
                let share = shares.get(target_slot).copied().unwrap_or(0.0);
                let spool = w.spool_at(elapsed) / (1.0 + w.spool_max * w.spool_frac);
                let v = &w.volley;
                let scale = firing * eff * share * spool;
                damage[0] += v.em * scale;
                damage[1] += v.thermal * scale;
                damage[2] += v.kinetic * scale;
                damage[3] += v.explosive * scale;
            }
            // The timeline carries what lands per second sustained, not one step's volley.
            landed[side] = guns[side].groups.iter().map(|(k, w, shares)| {
                let alive = alive_at[side].get(*k).copied().unwrap_or(0) as f64;
                let spool = w.spool_at(t - since[side]) / (1.0 + w.spool_max * w.spool_frac);
                w.dps * spool * alive * shares.get(target_slot).copied().unwrap_or(0.0)
            }).sum::<f64>() * f64::from(sides[side].fleet.volley_efficiency.clamp(0.01, 1.0));
            let full = fleets[1 - side][p].hp == fleets[1 - side][p].max;
            hit(&mut fleets[1 - side][p], damage, t);
            // Reps from the target side's logi, once locked, on its primary.
            let lock = logis[1 - side].first().map_or(f64::INFINITY, |(k, _, _)| sides[1 - side].stats.slots[*k].computed.sensors.lock_time_s(sides[1 - side].stats.slots[target_slot].computed.mobility.signature));
            let mut reps_on = 0.0;
            if fleets[1 - side][p].alive && t - since[side] >= lock {
                for (k, r, _) in &logis[1 - side] {
                    let n = alive_at[1 - side].get(*k).copied().unwrap_or(0) as f64;
                    let ship = &mut fleets[1 - side][p];
                    for layer in 0..3 {
                        let add = r[layer] * n * STEP;
                        reps_on += r[layer] * n;
                        ship.hp[layer] = (ship.hp[layer] + add).min(ship.max[layer]);
                    }
                }
            }
            // Critical mass: the damage landing no longer beats the reps on the primary.
            if out.critical_mass_lost[side].is_none() && t - since[side] > lock + 1.0 {
                let sustained: f64 = guns[side].groups.iter().map(|(k, w, shares)| {
                    let alive = alive_at[side].get(*k).copied().unwrap_or(0) as f64;
                    w.dps * alive * shares.get(target_slot).copied().unwrap_or(0.0)
                }).sum::<f64>() * f64::from(sides[side].fleet.volley_efficiency.clamp(0.01, 1.0));
                // Through the layer now being worn down.
                let ship = &fleets[1 - side][p];
                let layer = ship.hp.iter().position(|h| *h > 0.0).unwrap_or(2);
                let avg_res = ship.resonance[layer].iter().sum::<f64>() / 4.0;
                if reps_on > 0.0 && sustained * avg_res <= reps_on {
                    out.critical_mass_lost[side] = Some(t);
                    out.events.push((t, Event::CriticalMassLost { side }));
                }
            }
            if !fleets[1 - side][p].alive {
                out.events.push((t, Event::Kill { side: 1 - side, slot: target_slot }));
                if let Some((open, at)) = verdict_open[side] {
                    if open == p && !out.verdicts[1 - side].iter().any(|(s, _)| *s == target_slot) {
                        let seconds = t - at;
                        let v = if full && seconds < STEP * 2.0 {
                            Verdict::OneVolley
                        } else if lock.is_finite() && seconds < lock {
                            Verdict::BeforeReps { seconds }
                        } else {
                            Verdict::Outpaced { seconds, by: (landed[side] - reps_on).max(0.0) }
                        };
                        out.verdicts[1 - side].push((target_slot, v));
                    }
                }
                primary[side] = pick(&sides[side].fleet.target_order, &fleets[1 - side]);
                since[side] = t + STEP;
                // A new call: no ship fires before it reacts, nor before its guns have cycled.
                for next in next_fire[side].iter_mut() {
                    let n = next.len();
                    for (i, (at, _)) in next.iter_mut().enumerate() {
                        *at = at.max(since[side] + reaction(i, n));
                    }
                }
                verdict_open[side] = primary[side].map(|n| (n, t + STEP));
                if let Some(n) = primary[side] {
                    start_adc(&mut fleets[1 - side][n], t + STEP, &mut out, 1 - side);
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
                out.verdicts[side].push((k, Verdict::Held));
            }
        }
    }
    out
}

fn start_adc(ship: &mut Ship, now: f64, out: &mut Race, side: usize) {
    if let Some((_, duration)) = ship.adc {
        ship.adc_until = now + duration;
        out.events.push((now, Event::Adc { side, slot: ship.slot }));
        // Once per fight: its reactivation delay outlasts any fight here.
        let mult = ship.adc.map_or(1.0, |(m, _)| m);
        ship.adc = Some((mult, 0.0));
    }
}
