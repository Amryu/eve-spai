//! Two fleets over the ranges they could fight at: what each lands on the other's first target
//! (spread formations, motion, best carried ammo, the other's ewar), what holds it, and a score
//! for where it would rather be.

use super::ammo::{self, AmmoChoice};
use super::ewar::{cutoffs, ewar_at, EwarOn};
use super::geometry;
use super::{Fleet, FleetStats, Role, SlotStats};
use crate::fit::TargetProfile;
use crate::stats::application::applied_factor;
use crate::stats::{Computed, RemoteKind, WeaponGroup};

/// Positions sampled per fleet for the pairs; enough to show the spread, few enough to sweep.
const SAMPLES: usize = 20;
/// The share of its most a side still lands at its best range.
pub const BEST_SHARE: f64 = 0.9;
/// Share of the score lost when the enemy's webs reach the fleet.
pub const WEBBED_PENALTY: f64 = 0.15;
/// Share lost when its scramblers or disruptors reach it: it cannot leave.
pub const TACKLED_PENALTY: f64 = 0.20;
/// Share lost at full neutralising pressure (drain matching the ships' peak recharge).
pub const NEUT_PENALTY: f64 = 0.30;

/// One side of the fight.
pub struct Side<'a> {
    pub fleet: &'a Fleet,
    pub stats: &'a FleetStats,
    /// Each slot's ammo options (by its place in `stats.slots`); without them the loaded charges.
    pub ammo: &'a [Vec<AmmoChoice>],
}

impl Side<'_> {
    /// The fleet's speed: its share of the slowest ship's top speed.
    fn speed(&self) -> f64 {
        f64::from(self.fleet.motion.0.clamp(0.0, 1.0)) * self.stats.speed_min
    }

    fn ships(&self) -> Vec<(u32, &Computed)> {
        self.stats.slots.iter().filter(|s| s.count > 0).map(|s| (s.count, &s.computed)).collect()
    }
}

/// The slot a side shoots first in the other: its first role present there.
pub fn primary<'a>(shooter: &Fleet, other: &'a FleetStats) -> Option<&'a SlotStats> {
    shooter.target_order.iter().chain(Role::DEFAULT_ORDER.iter()).find_map(|r| other.slots.iter().find(|s| s.role == *r && s.count > 0))
}

/// What one side does to and suffers from the other at one distance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Facing {
    /// Damage per second landing on the other's primary, the whole fleet, best carried ammo.
    pub applied_dps: f64,
    /// The other side's ewar on this fleet, shared over its ships.
    pub webbed: bool,
    pub tackled: bool,
    /// Neutralising per ship over the ships' peak recharge.
    pub neut_pressure: f64,
    /// Applied damage over the best this side manages at any range, less the penalties.
    pub score: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangePoint {
    pub distance: f64,
    pub a: Facing,
    pub b: Facing,
}

#[derive(Clone, Debug, Default)]
pub struct Sweep {
    pub points: Vec<RangePoint>,
    /// The farthest range each side still lands `BEST_SHARE` of its most at.
    pub a_best: f64,
    pub b_best: f64,
    /// Stretches of range where `a` scores above `b`.
    pub a_favoured: Vec<(f64, f64)>,
    /// Each side's ewar and where it stops working: (kind, optimal, outer edge).
    pub a_cutoffs: Vec<(RemoteKind, f64, f64)>,
    pub b_cutoffs: Vec<(RemoteKind, f64, f64)>,
}

/// The weapon as the other side's ewar leaves it: disruptors on turrets and on missiles.
fn disrupted(w: &WeaponGroup, e: &EwarOn) -> WeaponGroup {
    let mut w = w.clone();
    w.tracking *= e.tracking;
    w.optimal *= e.optimal;
    w.falloff *= e.falloff;
    if let Some(m) = &mut w.missile {
        m.explosion_radius *= e.explosion_radius;
        m.explosion_velocity *= e.explosion_velocity;
        m.velocity *= e.missile_velocity;
        m.flight_time_s *= e.flight_time;
        m.range = m.velocity * m.flight_time_s;
    }
    w
}

/// Damage per second `a` lands on `b`'s primary at `distance`.
fn landed(a: &Side, b: &Side, distance: f64) -> f64 {
    let Some(target) = primary(a.fleet, b.stats) else { return 0.0 };
    // a's ewar all on the primary; b's spread over a's ships.
    let on_target = ewar_at(&a.ships(), distance, 1.0);
    let on_a = ewar_at(&b.ships(), distance, f64::from(a.stats.ships.max(1)));
    let pairs = geometry::engagement(a.fleet.formation, SAMPLES.min(a.stats.ships.max(1) as usize), a.speed(), f64::from(a.fleet.motion.1), b.fleet.formation, SAMPLES.min(b.stats.ships.max(1) as usize), b.speed() * on_target.speed, f64::from(b.fleet.motion.1), distance);
    let signature = target.computed.mobility.signature * on_target.signature;
    let velocity = b.speed() * on_target.speed;
    let mut total = 0.0;
    for (k, slot) in a.stats.slots.iter().enumerate().filter(|(_, s)| s.count > 0) {
        let choices = a.ammo.get(k);
        for w in &slot.computed.weapons {
            // The best carried charge for this weapon at this distance, else what it has loaded.
            let w = choices
                .and_then(|cs| cs.iter().find(|c| c.weapon == w.type_id))
                .and_then(|c| ammo::best_at(c, distance, &TargetProfile { signature: signature as f32, velocity: velocity as f32, distance: distance as f32, transversal: velocity as f32, radius: 0.0 }).map(|((charge, _), _)| charge).and_then(|charge| c.options.iter().find(|(x, _, _)| *x == charge).map(|(_, _, g)| g)))
                .unwrap_or(w);
            let w = disrupted(w, &on_a);
            let mut factor = 0.0;
            let mut n: f64 = 0.0;
            for row in &pairs {
                for p in row {
                    let t = TargetProfile { signature: signature as f32, velocity: velocity as f32, distance: p.distance as f32, transversal: p.transversal as f32, radius: 0.0 };
                    factor += applied_factor(&w, p.distance, &t, slot.computed.drone_speed.unwrap_or(0.0), slot.computed.drones.control_range);
                    n += 1.0;
                }
            }
            total += w.dps * f64::from(slot.count) * factor / n.max(1.0);
        }
    }
    total
}

/// The other side's hold on this one at `distance`: webs, tackle, neutralising against recharge.
fn held(own: &Side, other: &Side, distance: f64) -> (bool, bool, f64) {
    let e = ewar_at(&other.ships(), distance, f64::from(own.stats.ships.max(1)));
    let ships = f64::from(own.stats.ships.max(1));
    let regen: f64 = own.stats.slots.iter().map(|s| s.computed.cap.peak_regen * f64::from(s.count)).sum::<f64>() / ships;
    let pressure = if regen > 0.0 { e.neut / regen } else if e.neut > 0.0 { 1.0 } else { 0.0 };
    (e.webbed, e.pointed, pressure)
}

/// Both fleets over `ranges` (ascending, metres).
pub fn sweep(a: &Side, b: &Side, ranges: &[f64]) -> Sweep {
    let mut points: Vec<RangePoint> = ranges
        .iter()
        .map(|&d| {
            let (aw, at, an) = held(a, b, d);
            let (bw, bt, bn) = held(b, a, d);
            RangePoint {
                distance: d,
                a: Facing { applied_dps: landed(a, b, d), webbed: aw, tackled: at, neut_pressure: an, score: 0.0 },
                b: Facing { applied_dps: landed(b, a, d), webbed: bw, tackled: bt, neut_pressure: bn, score: 0.0 },
            }
        })
        .collect();
    let a_max = points.iter().map(|p| p.a.applied_dps).fold(0.0, f64::max);
    let b_max = points.iter().map(|p| p.b.applied_dps).fold(0.0, f64::max);
    let score = |f: &Facing, max: f64| {
        let base = if max > 0.0 { f.applied_dps / max } else { 0.0 };
        let penalty = if f.webbed { WEBBED_PENALTY } else { 0.0 } + if f.tackled { TACKLED_PENALTY } else { 0.0 } + NEUT_PENALTY * f.neut_pressure.min(1.0);
        base * (1.0 - penalty.min(0.9))
    };
    for p in &mut points {
        p.a.score = score(&p.a, a_max);
        p.b.score = score(&p.b, b_max);
    }
    // The farthest range a side still lands most of its best at: where it wants to be.
    let reach = |f: &dyn Fn(&RangePoint) -> f64, max: f64| points.iter().filter(|p| max > 0.0 && f(p) >= max * BEST_SHARE).map(|p| p.distance).fold(0.0, f64::max);
    let a_best = reach(&|p| p.a.applied_dps, a_max);
    let b_best = reach(&|p| p.b.applied_dps, b_max);
    // Runs of consecutive ranges where `a` scores above `b`.
    let mut a_favoured: Vec<(f64, f64)> = Vec::new();
    let mut run: Option<(f64, f64)> = None;
    for p in &points {
        if p.a.score > p.b.score {
            run = Some(run.map_or((p.distance, p.distance), |(from, _)| (from, p.distance)));
        } else if let Some(r) = run.take() {
            a_favoured.push(r);
        }
    }
    a_favoured.extend(run);
    Sweep { points, a_best, b_best, a_favoured, a_cutoffs: cutoffs(&a.ships()), b_cutoffs: cutoffs(&b.ships()) }
}

/// The ranges to sweep: every kilometre out to the farthest reach on either side, and the
/// cutoffs of weapons and ewar, so their edges show.
pub fn ranges(a: &FleetStats, b: &FleetStats) -> Vec<f64> {
    let reach = |s: &FleetStats| s.slots.iter().flat_map(|x| x.computed.weapons.iter().map(|w| w.missile.as_ref().map_or(w.optimal + 3.0 * w.falloff, |m| m.long_range()))).fold(10_000.0, f64::max);
    let far = reach(a).max(reach(b)) * 1.1;
    let mut v: Vec<f64> = (0..=200).map(|i| far * f64::from(i) / 200.0).collect();
    for s in [a, b] {
        for (_, o, e) in cutoffs(&s.slots.iter().map(|x| (x.count, &x.computed)).collect::<Vec<_>>()) {
            for x in [o, e] {
                if x < far {
                    v.push(x);
                    v.push(x + 1.0);
                }
            }
        }
    }
    v.sort_by(|x, y| x.total_cmp(y));
    v.dedup();
    v
}
