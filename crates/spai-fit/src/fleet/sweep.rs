//! Two fleets over the ranges they could fight at: what each lands on the other's first target
//! (spread formations, motion, best carried ammo, the other's ewar), what holds it, and a score
//! for where it would rather be.

use super::ammo::{self, AmmoChoice};
use super::ewar::{cutoffs, planned, planned_average, EwarOn};
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
    pub fn speed(&self) -> f64 {
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
    /// One volley of the whole fleet on it, drones and fighters left out.
    pub applied_volley: f64,
    /// The other side's ewar on this fleet, shared over its ships.
    pub webbed: bool,
    pub tackled: bool,
    /// Neutralising per ship over the ships' peak recharge.
    pub neut_pressure: f64,
    /// The other side's primaries killed per minute (applied damage over its effective hit
    /// points), less the penalties.
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
    /// Stretches of range where `a` scores clearly above `b`, and the other way round.
    pub a_favoured: Vec<(f64, f64)>,
    pub b_favoured: Vec<(f64, f64)>,
    /// Each side's ewar and where it stops working: (kind, optimal, outer edge).
    pub a_cutoffs: Vec<(RemoteKind, f64, f64)>,
    pub b_cutoffs: Vec<(RemoteKind, f64, f64)>,
}

/// The weapon as the other side's ewar leaves it: disruptors on turrets and on missiles.
pub fn disrupted(w: &WeaponGroup, e: &EwarOn) -> WeaponGroup {
    let mut w = w.clone();
    // A vorton has no tracking to disrupt; guidance disruptors work its explosion.
    if w.kind != crate::stats::WeaponKind::Vorton {
        w.tracking *= e.tracking;
        w.optimal *= e.optimal;
        w.falloff *= e.falloff;
    }
    if let Some(m) = &mut w.missile.as_mut().filter(|_| w.kind == crate::stats::WeaponKind::Vorton) {
        m.explosion_radius *= e.explosion_radius;
        m.explosion_velocity *= e.explosion_velocity;
    } else if let Some(m) = &mut w.missile {
        m.explosion_radius *= e.explosion_radius;
        m.explosion_velocity *= e.explosion_velocity;
        m.velocity *= e.missile_velocity;
        m.flight_time_s *= e.flight_time;
        m.range = m.velocity * m.flight_time_s;
    }
    w
}

/// Each weapon of `a`'s slot `k` as it fights `target` (a slot of `b`) at `distance`: with its
/// best carried charge, disrupted by `b`'s ewar shared over `a`, and the share of its damage that
/// lands, averaged over the formations' pairs.
pub fn weapons_on(a: &Side, b: &Side, k: usize, target: &SlotStats, distance: f64) -> Vec<(WeaponGroup, f64)> {
    shares_on(a, b, k, target, distance).into_iter().map(|(w, f)| {
        let mean = f.iter().sum::<f64>() / (f.len().max(1) as f64);
        (w, mean)
    }).collect()
}

/// As `weapons_on`, with the applied share of every shooter and target pair, not their mean.
pub fn shares_on(a: &Side, b: &Side, k: usize, target: &SlotStats, distance: f64) -> Vec<(WeaponGroup, Vec<f64>)> {
    let Some(slot) = a.stats.slots.get(k) else { return Vec::new() };
    let on_target = planned(&a.ships(), distance, &a.fleet.ewar_plan, a.fleet.split_fire, b.stats.ships, Some(&target.computed));
    let on_a = planned_average(&b.ships(), distance, &b.fleet.ewar_plan, b.fleet.split_fire, a.stats.ships, None);
    let pairs = geometry::engagement(a.fleet.formation, SAMPLES.min(a.stats.ships.max(1) as usize), a.speed(), f64::from(a.fleet.motion.1), b.fleet.formation, SAMPLES.min(b.stats.ships.max(1) as usize), b.speed() * on_target.speed, f64::from(b.fleet.motion.1), distance);
    let signature = target.computed.mobility.signature * on_target.signature;
    let velocity = b.speed() * on_target.speed;
    let choices = a.ammo.get(k);
    let shares_of = |w: &WeaponGroup| -> Vec<f64> {
        pairs
            .iter()
            .flatten()
            .map(|p| {
                let t = TargetProfile { signature: signature as f32, velocity: velocity as f32, distance: p.distance as f32, transversal: p.transversal as f32, ..TargetProfile::default() };
                applied_factor(w, p.distance, &t, slot.computed.drone_speed.unwrap_or(0.0), slot.computed.drones.control_range)
            })
            .collect()
    };
    let mean_dps = |w: &WeaponGroup, shares: &[f64]| w.dps * shares.iter().sum::<f64>() / (shares.len().max(1) as f64);
    slot.computed
        .weapons
        .iter()
        .map(|w| {
            // The carried charge that lands most over the formations' pairs, not at the anchors'
            // distance alone: a spread fleet has ships past a short charge's reach long before
            // the anchors are. Else what it has loaded.
            let options = choices.and_then(|cs| cs.iter().find(|c| c.weapon == w.type_id)).map(|c| c.options.iter().filter(|(_, carried, _)| *carried).map(|(_, _, g)| disrupted(g, &on_a)).collect::<Vec<_>>()).unwrap_or_default();
            let best = options.into_iter().map(|g| {
                let shares = shares_of(&g);
                (mean_dps(&g, &shares), g, shares)
            }).max_by(|x, y| x.0.total_cmp(&y.0));
            match best {
                Some((_, g, shares)) => (g, shares),
                None => {
                    let w = disrupted(w, &on_a);
                    let shares = shares_of(&w);
                    (w, shares)
                }
            }
        })
        .collect()
}

/// Damage per second and one volley `a` lands on `b`'s primary at `distance`.
fn landed(a: &Side, b: &Side, distance: f64) -> (f64, f64) {
    let Some(target) = primary(a.fleet, b.stats) else { return (0.0, 0.0) };
    let mut out = (0.0, 0.0);
    for (k, s) in a.stats.slots.iter().enumerate().filter(|(_, s)| s.count > 0) {
        for (w, f) in weapons_on(a, b, k, target, distance) {
            out.0 += w.dps * f * f64::from(s.count);
            if w.alpha() {
                out.1 += w.volley.total() * f * f64::from(s.count);
            }
        }
    }
    out
}

/// The other side's hold on this one at `distance`: webs, tackle, neutralising against recharge.
fn held(own: &Side, other: &Side, distance: f64) -> (bool, bool, f64) {
    // As the other side lays its ewar: focused on what it shoots, or spread over this fleet.
    let e = planned_average(&other.ships(), distance, &other.fleet.ewar_plan, other.fleet.split_fire, own.stats.ships, None);
    let ships = f64::from(own.stats.ships.max(1));
    let regen: f64 = own.stats.slots.iter().map(|s| s.computed.cap.peak_regen * f64::from(s.count)).sum::<f64>() / ships;
    let pressure = if regen > 0.0 { e.neut / regen } else if e.neut > 0.0 { 1.0 } else { 0.0 };
    (e.webbed, e.pointed, pressure)
}

/// Both fleets over `ranges` (ascending, metres).
pub fn sweep(a: &Side, b: &Side, ranges: &[f64]) -> Sweep {
    finish(a, b, ranges.iter().map(|&d| point(a, b, d)).collect())
}

/// Both fleets at one range, unscored: a sweep worked out a few ranges at a time.
pub fn point(a: &Side, b: &Side, d: f64) -> RangePoint {
    let (aw, at, an) = held(a, b, d);
    let (bw, bt, bn) = held(b, a, d);
    let ((ad, av), (bd, bv)) = (landed(a, b, d), landed(b, a, d));
    RangePoint {
        distance: d,
        a: Facing { applied_dps: ad, applied_volley: av, webbed: aw, tackled: at, neut_pressure: an, score: 0.0 },
        b: Facing { applied_dps: bd, applied_volley: bv, webbed: bw, tackled: bt, neut_pressure: bn, score: 0.0 },
    }
}

/// The sweep of these points, ascending: each scored, the best ranges and the favoured stretches.
pub fn finish(a: &Side, b: &Side, mut points: Vec<RangePoint>) -> Sweep {
    let a_max = points.iter().map(|p| p.a.applied_dps).fold(0.0, f64::max);
    let b_max = points.iter().map(|p| p.b.applied_dps).fold(0.0, f64::max);
    let ehp = |shooter: &Side, other: &Side| primary(shooter.fleet, other.stats).map_or(0.0, |t| t.computed.tank.ehp);
    let (a_ehp, b_ehp) = (ehp(a, b), ehp(b, a));
    let score = |f: &Facing, target_ehp: f64| {
        let base = if target_ehp > 0.0 { 60.0 * f.applied_dps / target_ehp } else { 0.0 };
        let penalty = if f.webbed { WEBBED_PENALTY } else { 0.0 } + if f.tackled { TACKLED_PENALTY } else { 0.0 } + NEUT_PENALTY * f.neut_pressure.min(1.0);
        base * (1.0 - penalty.min(0.9))
    };
    for p in &mut points {
        p.a.score = score(&p.a, a_ehp);
        p.b.score = score(&p.b, b_ehp);
    }
    // The farthest range a side still lands most of its best at: where it wants to be.
    let reach = |f: &dyn Fn(&RangePoint) -> f64, max: f64| points.iter().filter(|p| max > 0.0 && f(p) >= max * BEST_SHARE).map(|p| p.distance).fold(0.0, f64::max);
    let a_best = reach(&|p| p.a.applied_dps, a_max);
    let b_best = reach(&|p| p.b.applied_dps, b_max);
    let a_favoured = runs(&points, |p| p.a.score > p.b.score * (1.0 + MARGIN) && p.a.score > 0.0);
    let b_favoured = runs(&points, |p| p.b.score > p.a.score * (1.0 + MARGIN) && p.b.score > 0.0);
    Sweep { points, a_best, b_best, a_favoured, b_favoured, a_cutoffs: cutoffs(&a.ships()), b_cutoffs: cutoffs(&b.ships()) }
}

/// By what share one score must lead the other for a range to count as favouring it.
pub const MARGIN: f64 = 0.05;

/// Stretches of consecutive ranges that pass `keep`, single points dropped.
fn runs(points: &[RangePoint], keep: impl Fn(&RangePoint) -> bool) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut run: Option<(f64, f64)> = None;
    for p in points {
        if keep(p) {
            run = Some(run.map_or((p.distance, p.distance), |(from, _)| (from, p.distance)));
        } else if let Some(r) = run.take() {
            out.push(r);
        }
    }
    out.extend(run);
    out.retain(|(f, t)| t > f);
    out
}

/// The ranges to sweep: every kilometre out to the farthest reach on either side, and the
/// cutoffs of weapons and ewar, so their edges show.
pub fn ranges(a: &FleetStats, b: &FleetStats) -> Vec<f64> {
    let reach = |s: &FleetStats| s.slots.iter().flat_map(|x| x.computed.weapons.iter().map(|w| w.reach())).fold(10_000.0, f64::max);
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

/// The angular velocities of `a`'s ships on `b`'s ships at `distance`, ascending: how spread
/// formations and motion widen what the guns must track.
pub fn angular(a: &Side, b: &Side, distance: f64) -> Vec<f64> {
    let on_b = planned(&a.ships(), distance, &a.fleet.ewar_plan, a.fleet.split_fire, b.stats.ships, None);
    let pairs = geometry::engagement(a.fleet.formation, SAMPLES.min(a.stats.ships.max(1) as usize), a.speed(), f64::from(a.fleet.motion.1), b.fleet.formation, SAMPLES.min(b.stats.ships.max(1) as usize), b.speed() * on_b.speed, f64::from(b.fleet.motion.1), distance);
    let mut v: Vec<f64> = pairs.iter().flatten().map(|p| p.angular).collect();
    v.sort_by(|x, y| x.total_cmp(y));
    v
}

/// The angular velocity at which each of `a`'s turret groups still hits `target` half the time,
/// the other side's disruptors counted: (slot, the group, rad/s).
pub fn half_hit_angular(a: &Side, b: &Side, target: &SlotStats, distance: f64) -> Vec<(usize, WeaponGroup, f64)> {
    let on_a = planned_average(&b.ships(), distance, &b.fleet.ewar_plan, b.fleet.split_fire, a.stats.ships, None);
    let on_target = planned(&a.ships(), distance, &a.fleet.ewar_plan, a.fleet.split_fire, b.stats.ships, Some(&target.computed));
    let signature = target.computed.mobility.signature * on_target.signature;
    let mut out = Vec::new();
    for (k, slot) in a.stats.slots.iter().enumerate().filter(|(_, s)| s.count > 0) {
        for w in slot.computed.weapons.iter().filter(|w| w.kind == crate::stats::WeaponKind::Turret && w.signature_resolution > 0.0) {
            let w = disrupted(w, &on_a);
            out.push((k, w.clone(), w.tracking * signature / w.signature_resolution));
        }
    }
    out
}

/// How one weapon group of a shooter fares on one target: the charge it fires (its best carried
/// one there), the target's effective hit points against that charge's damage, and the share of
/// its raw damage that lands, on average and at the bottom and top 5% of shooter and target pairs.
#[derive(Clone, Debug)]
pub struct Application {
    pub slot: usize,
    pub weapon: WeaponGroup,
    pub target_ehp: f64,
    pub mean: f64,
    pub low: f64,
    pub high: f64,
}

pub fn application(a: &Side, b: &Side, target: &SlotStats, distance: f64) -> Vec<Application> {
    let mut out = Vec::new();
    for k in (0..a.stats.slots.len()).filter(|k| a.stats.slots[*k].count > 0) {
        for (w, mut f) in shares_on(a, b, k, target, distance) {
            if f.is_empty() {
                continue;
            }
            f.sort_by(|x, y| x.total_cmp(y));
            let at = |q: f64| f[((f.len() - 1) as f64 * q).round() as usize];
            let mean = f.iter().sum::<f64>() / f.len() as f64;
            out.push(Application { slot: k, target_ehp: ehp_against(&target.computed.tank, &w.volley), mean, low: at(0.05), high: at(0.95), weapon: w });
        }
    }
    out
}

/// Effective hit points against a damage split: each layer's hit points over the share of that
/// damage its resistances let through.
pub fn ehp_against(t: &crate::stats::Tank, d: &crate::stats::Damage) -> f64 {
    let total = d.total();
    if total <= 0.0 {
        return t.ehp;
    }
    let w = [d.em / total, d.thermal / total, d.kinetic / total, d.explosive / total];
    [&t.shield, &t.armor, &t.hull].iter().map(|l| {
        let through: f64 = (0..4).map(|i| w[i] * l.resonance[i]).sum();
        if through > 0.0 { l.hp / through } else { f64::INFINITY }
    }).sum()
}

/// Each charged weapon of `a`'s slot `k` with its best carried charge over `ranges` against
/// `target`, as the fight leaves it at each range: the target slowed by `a`'s webs and grown by
/// its painters, crossing at its fleet's speed, the options under `b`'s disruptors.
pub fn ammo_bands(a: &Side, b: &Side, k: usize, target: &SlotStats, ranges: &[f64]) -> Vec<(crate::ids::TypeId, Vec<ammo::Band>)> {
    let Some(choices) = a.ammo.get(k) else { return Vec::new() };
    // The ewar each way and the formations' pairs per range, worked out once for every weapon.
    let slot = &a.stats.slots[k];
    let at: Vec<(f64, Vec<TargetProfile>, EwarOn)> = ranges
        .iter()
        .map(|&d| {
            let on_target = planned(&a.ships(), d, &a.fleet.ewar_plan, a.fleet.split_fire, b.stats.ships, Some(&target.computed));
            let on_a = planned_average(&b.ships(), d, &b.fleet.ewar_plan, b.fleet.split_fire, a.stats.ships, None);
            let velocity = b.speed() * on_target.speed;
            let signature = (target.computed.mobility.signature * on_target.signature) as f32;
            let pairs = geometry::engagement(a.fleet.formation, SAMPLES.min(a.stats.ships.max(1) as usize), a.speed(), f64::from(a.fleet.motion.1), b.fleet.formation, SAMPLES.min(b.stats.ships.max(1) as usize), velocity, f64::from(b.fleet.motion.1), d);
            let probes = pairs.iter().flatten().map(|p| TargetProfile { signature, velocity: velocity as f32, distance: p.distance as f32, transversal: p.transversal as f32, radius: 0.0, resist: [0.0; 4] }).collect();
            (d, probes, on_a)
        })
        .collect();
    // What a charge lands, averaged over the pairs: the same measure the sweep picks by.
    let landed = |g: &WeaponGroup, probes: &[TargetProfile], on_a: &EwarOn| {
        let g = disrupted(g, on_a);
        let sum: f64 = probes.iter().map(|t| applied_factor(&g, f64::from(t.distance), t, slot.computed.drone_speed.unwrap_or(0.0), slot.computed.drones.control_range)).sum();
        g.dps * sum / (probes.len().max(1) as f64)
    };
    choices
        .iter()
        .map(|c| {
            let pick = |d: f64| {
                let (_, probes, on_a) = at.iter().find(|(x, _, _)| *x == d)?;
                let best = |carried_only: bool| c.options.iter().filter(|(_, carried, _)| !carried_only || *carried).map(|(ch, _, g)| (*ch, landed(g, probes, on_a))).max_by(|x, y| x.1.total_cmp(&y.1));
                let carried = best(true)?;
                let any = best(false).filter(|(ch, v)| *ch != carried.0 && *v > carried.1 * 1.0001);
                Some((carried, any))
            };
            (c.weapon, ammo::bands_by(ranges, &pick))
        })
        .collect()
}

/// One charge a weapon group could fire at a ship, and how it would fare there.
#[derive(Clone, Debug)]
pub struct ChargeOption {
    pub slot: usize,
    pub weapon: crate::ids::TypeId,
    pub charge: crate::ids::TypeId,
    pub carried: bool,
    /// The group with this charge, under the other side's disruptors.
    pub group: WeaponGroup,
    pub target_ehp: f64,
    pub mean: f64,
    pub low: f64,
    pub high: f64,
}

/// Every charge option of every charged weapon of `a` on `target` at `distance`, the way
/// `application` works one out: pair by pair over both formations, ewar on both.
pub fn charge_options(a: &Side, b: &Side, target: &SlotStats, distance: f64) -> Vec<ChargeOption> {
    let on_target = planned(&a.ships(), distance, &a.fleet.ewar_plan, a.fleet.split_fire, b.stats.ships, Some(&target.computed));
    let on_a = planned_average(&b.ships(), distance, &b.fleet.ewar_plan, b.fleet.split_fire, a.stats.ships, None);
    let pairs = geometry::engagement(a.fleet.formation, SAMPLES.min(a.stats.ships.max(1) as usize), a.speed(), f64::from(a.fleet.motion.1), b.fleet.formation, SAMPLES.min(b.stats.ships.max(1) as usize), b.speed() * on_target.speed, f64::from(b.fleet.motion.1), distance);
    let signature = target.computed.mobility.signature * on_target.signature;
    let velocity = b.speed() * on_target.speed;
    let mut out = Vec::new();
    for (k, slot) in a.stats.slots.iter().enumerate().filter(|(_, s)| s.count > 0) {
        for c in a.ammo.get(k).into_iter().flatten() {
            for (charge, carried, w) in &c.options {
                let w = disrupted(w, &on_a);
                let mut f: Vec<f64> = pairs
                    .iter()
                    .flatten()
                    .map(|p| {
                        let t = TargetProfile { signature: signature as f32, velocity: velocity as f32, distance: p.distance as f32, transversal: p.transversal as f32, ..TargetProfile::default() };
                        applied_factor(&w, p.distance, &t, slot.computed.drone_speed.unwrap_or(0.0), slot.computed.drones.control_range)
                    })
                    .collect();
                if f.is_empty() {
                    continue;
                }
                f.sort_by(|x, y| x.total_cmp(y));
                let at = |q: f64| f[((f.len() - 1) as f64 * q).round() as usize];
                let mean = f.iter().sum::<f64>() / f.len() as f64;
                out.push(ChargeOption { slot: k, weapon: c.weapon, charge: *charge, carried: *carried, target_ehp: ehp_against(&target.computed.tank, &w.volley), mean, low: at(0.05), high: at(0.95), group: w });
            }
        }
    }
    out
}

/// One ship's applied volley by damage type: each of its alpha weapon groups' volley times the
/// share landing, with `swap` putting another charge's group in place of the one it names.
pub fn ship_volley(apps: &[Application], slot: usize, swap: Option<(crate::ids::TypeId, &WeaponGroup, f64)>) -> crate::stats::Damage {
    let mut d = crate::stats::Damage::default();
    for x in apps.iter().filter(|x| x.slot == slot && x.weapon.alpha()) {
        let (w, share) = match swap {
            Some((weapon, g, m)) if weapon == x.weapon.type_id => (g, m),
            _ => (&x.weapon, x.mean),
        };
        d.em += w.volley.em * share;
        d.thermal += w.volley.thermal * share;
        d.kinetic += w.volley.kinetic * share;
        d.explosive += w.volley.explosive * share;
    }
    d
}

/// Ships of `a`'s slot `k` that must volley together to kill `target`: its hit points against
/// their applied damage mix over one ship's applied volley times the volley efficiency.
pub fn ships_per_kill(target: &SlotStats, volley: &crate::stats::Damage, efficiency: f64) -> Option<u32> {
    let v = volley.total() * efficiency;
    (v > 0.0).then(|| (ehp_against(&target.computed.tank, volley) / v).ceil() as u32)
}

/// The summary of `a` on one ship of `b`, the way the sweep and the race see it: every weapon
/// with its best carried charge, its share averaged over both formations' pairs, ewar both ways;
/// critical mass by `a`'s main slot against the ship's hit points for the damage mix landing.
pub fn summary(a: &Side, b: &Side, target: &SlotStats, distance: f64) -> super::Against {
    summary_from(a, b, target, distance, &application(a, b, target, distance))
}

/// `summary` from `application`'s figures already at hand.
pub fn summary_from(a: &Side, b: &Side, target: &SlotStats, distance: f64, apps: &[Application]) -> super::Against {
    let eff = f64::from(a.fleet.volley_efficiency.clamp(0.01, 1.0));
    let count = |k: usize| f64::from(a.stats.slots.get(k).map_or(0, |s| s.count));
    let applied_dps: f64 = apps.iter().map(|x| x.weapon.dps * x.mean * count(x.slot)).sum();
    let mut fleet = crate::stats::Damage::default();
    for k in 0..a.stats.slots.len() {
        let v = ship_volley(apps, k, None);
        let n = count(k);
        fleet.em += v.em * n;
        fleet.thermal += v.thermal * n;
        fleet.kinetic += v.kinetic * n;
        fleet.explosive += v.explosive * n;
    }
    let main = a.stats.slots.iter().position(|s| s.role == super::Role::Main).unwrap_or(0);
    let on_target = planned(&a.ships(), distance, &a.fleet.ewar_plan, a.fleet.split_fire, b.stats.ships, Some(&target.computed));
    let ehp = ehp_against(&target.computed.tank, &fleet);
    super::Against {
        applied_dps,
        applied_volley: fleet.total(),
        critical_mass: ships_per_kill(target, &ship_volley(apps, main, None), eff),
        kills_per_volley: if ehp > 0.0 { fleet.total() * eff / ehp } else { 0.0 },
        target_velocity: b.speed() * on_target.speed,
        target_signature: target.computed.mobility.signature * on_target.signature,
        ..Default::default()
    }
}
