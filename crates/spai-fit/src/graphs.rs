//! The curves the simulator draws: damage against distance, target speed and signature, the
//! capacitor and shield over time, speed over time, warp time and lock time. The engine gives the
//! numbers; the screens draw and label them.

use crate::fit::TargetProfile;
use crate::ids::TypeId;
use crate::stats::application::applied_factor;
use crate::stats::{RemoteKind, range_factor, cap_sim, Computed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphKind {
    DamageVsDistance,
    DamageVsTargetSpeed,
    DamageVsTargetSignature,
    /// One round of every weapon, as the damage graphs but per shot rather than per second.
    VolleyVsDistance,
    VolleyVsTargetSpeed,
    VolleyVsTargetSignature,
    CapacitorOverTime,
    ShieldOverTime,
    VelocityOverTime,
    WarpTime,
    LockTime,
    RemoteRepVsDistance,
    EwarVsDistance,
}

impl GraphKind {
    /// A damage graph of either kind: per second or per shot.
    pub fn is_damage(self) -> bool {
        matches!(self, GraphKind::DamageVsDistance | GraphKind::DamageVsTargetSpeed | GraphKind::DamageVsTargetSignature) || self.is_volley()
    }

    pub fn is_volley(self) -> bool {
        matches!(self, GraphKind::VolleyVsDistance | GraphKind::VolleyVsTargetSpeed | GraphKind::VolleyVsTargetSignature)
    }

    pub const ALL: [GraphKind; 13] = [
        GraphKind::DamageVsDistance,
        GraphKind::DamageVsTargetSpeed,
        GraphKind::DamageVsTargetSignature,
        GraphKind::VolleyVsDistance,
        GraphKind::VolleyVsTargetSpeed,
        GraphKind::VolleyVsTargetSignature,
        GraphKind::CapacitorOverTime,
        GraphKind::ShieldOverTime,
        GraphKind::VelocityOverTime,
        GraphKind::WarpTime,
        GraphKind::LockTime,
        GraphKind::RemoteRepVsDistance,
        GraphKind::EwarVsDistance,
    ];

    /// What the graph plots, up the side and along the bottom.
    pub fn axes(self) -> (GraphY, GraphX) {
        use {GraphKind as K, GraphX as X, GraphY as Y};
        match self {
            K::DamageVsDistance => (Y::Dps, X::Distance),
            K::DamageVsTargetSpeed => (Y::Dps, X::TargetSpeed),
            K::DamageVsTargetSignature => (Y::Dps, X::TargetSignature),
            K::VolleyVsDistance => (Y::Volley, X::Distance),
            K::VolleyVsTargetSpeed => (Y::Volley, X::TargetSpeed),
            K::VolleyVsTargetSignature => (Y::Volley, X::TargetSignature),
            K::CapacitorOverTime => (Y::Capacitor, X::Time),
            K::ShieldOverTime => (Y::Shield, X::Time),
            K::VelocityOverTime => (Y::Speed, X::Time),
            K::WarpTime => (Y::WarpTime, X::WarpDistance),
            K::LockTime => (Y::LockTime, X::TargetSignature),
            K::RemoteRepVsDistance => (Y::RemoteRep, X::Distance),
            K::EwarVsDistance => (Y::Ewar, X::Distance),
        }
    }

    /// The graph of that pair, if the two go together.
    pub fn from_axes(y: GraphY, x: GraphX) -> Option<GraphKind> {
        GraphKind::ALL.into_iter().find(|k| k.axes() == (y, x))
    }
}

/// What a graph can plot up its side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraphY {
    Dps,
    Volley,
    Capacitor,
    Shield,
    Speed,
    WarpTime,
    LockTime,
    RemoteRep,
    Ewar,
}

/// What a graph can run along its bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraphX {
    Distance,
    TargetSpeed,
    TargetSignature,
    Time,
    /// Warp distance, in AU rather than metres.
    WarpDistance,
}

impl GraphY {
    pub const ALL: [GraphY; 9] = [GraphY::Dps, GraphY::Volley, GraphY::Capacitor, GraphY::Shield, GraphY::Speed, GraphY::WarpTime, GraphY::LockTime, GraphY::RemoteRep, GraphY::Ewar];

    /// The bottoms this can be plotted against, in the order offered.
    pub fn xs(self) -> Vec<GraphX> {
        GraphX::ALL.into_iter().filter(|x| GraphKind::from_axes(self, *x).is_some()).collect()
    }
}

impl GraphX {
    pub const ALL: [GraphX; 5] = [GraphX::Distance, GraphX::TargetSpeed, GraphX::TargetSignature, GraphX::Time, GraphX::WarpDistance];
}

#[derive(Clone, Debug, PartialEq)]
pub enum SeriesLabel {
    /// One weapon group: its type and charge.
    Weapon(TypeId, Option<TypeId>),
    Total,
    Capacitor,
    Shield,
    Velocity,
    Warp,
    Lock,
    /// One kind of remote module, all of its type together.
    Remote(TypeId, RemoteKind),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub label: SeriesLabel,
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphSpec {
    pub kind: GraphKind,
    /// `None` picks a range that shows the whole curve.
    pub x_to: Option<f64>,
    pub points: usize,
    pub target: TargetProfile,
    pub include_drones: bool,
}

impl GraphSpec {
    pub fn new(kind: GraphKind, target: TargetProfile) -> Self {
        GraphSpec { kind, x_to: None, points: 400, target, include_drones: true }
    }
}

/// What the capacitor simulation needs that `Computed` does not keep.
pub struct CapInputs {
    pub users: Vec<cap_sim::User>,
}

pub const AU: f64 = 149_597_870_700.0;

/// Seconds to warp `distance` metres at `warp_speed` AU/s from a ship doing `sub_speed` m/s.
pub fn warp_time(distance: f64, warp_speed_au: f64, sub_speed: f64) -> f64 {
    if warp_speed_au <= 0.0 || distance <= 0.0 {
        return 0.0;
    }
    let k_a = warp_speed_au;
    let k_d = (k_a / 3.0).min(2.0);
    let full = k_a * AU;
    let v_drop = (sub_speed / 2.0).clamp(1.0, 100.0);
    let d_min = AU + full / k_d;
    let (v, cruise) = if distance < d_min { (distance * k_a * k_d / (k_a + k_d), 0.0) } else { (full, (distance - d_min) / full) };
    (v / k_a).ln() / k_a + cruise + (v / v_drop).max(1.0).ln() / k_d
}

/// Speed `t` seconds after starting from rest: `vmax × (1 − e^(−t / τ))`, τ = agility × mass / 10⁶.
pub fn velocity_at(t: f64, max_velocity: f64, agility: f64, mass: f64) -> f64 {
    let tau = agility * mass / 1.0e6;
    if tau <= 0.0 {
        return max_velocity;
    }
    max_velocity * (1.0 - (-t / tau).exp())
}

fn grid(from: f64, to: f64, n: usize) -> impl Iterator<Item = f64> {
    let n = n.max(2);
    (0..n).map(move |i| from + (to - from) * i as f64 / (n - 1) as f64)
}

/// How far either side of a cutoff the curve is sampled, so the drop draws as a wall.
const EDGE: f64 = 0.5;

/// The grid plus a sample just before, at and just after each `edge` inside it: where a curve
/// ends outright (a missile's flight, a smartbomb's reach, a module's last falloff) the line is
/// then vertical rather than a slope between two grid points.
fn with_edges(xs: impl Iterator<Item = f64>, edges: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = xs.collect();
    let (Some(&lo), Some(&hi)) = (v.first(), v.last()) else { return v };
    for &e in edges {
        for x in [e - EDGE, e, e + EDGE] {
            if x > lo && x < hi {
                v.push(x);
            }
        }
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup();
    v
}

/// Where each weapon's damage ends outright along the distance axis.
fn damage_edges(c: &Computed, include_drones: bool) -> Vec<f64> {
    use crate::stats::WeaponKind;
    let mut edges: Vec<f64> = c
        .weapons
        .iter()
        .filter(|w| include_drones || w.kind != WeaponKind::Drone)
        .flat_map(|w| match (w.kind, w.missile.as_ref()) {
            (WeaponKind::Missile, Some(m)) => vec![m.short_range(), m.long_range()],
            _ => Vec::new(),
        })
        .chain(c.weapons.iter().filter(|w| include_drones || w.kind != WeaponKind::Drone).filter_map(|w| match w.kind {
            WeaponKind::Missile => None,
            WeaponKind::Drone => Some(c.drones.control_range),
            WeaponKind::Smartbomb => Some(w.optimal),
            _ => (w.falloff > 0.0).then_some(w.optimal + 3.0 * w.falloff),
        }))
        .collect();
    edges.retain(|e| e.is_finite() && *e > 0.0);
    edges
}

/// The damage curves of a fit at each `x`, `x` being distance, target speed or signature.
fn damage_series(c: &Computed, spec: &GraphSpec, xs: impl Iterator<Item = f64>, at: impl Fn(f64) -> (f64, TargetProfile)) -> Vec<Series> {
    let xs: Vec<f64> = xs.collect();
    let drone_speed = c.weapons.iter().find(|w| w.kind == crate::stats::WeaponKind::Drone).map_or(0.0, |_| spec.target.velocity as f64);
    let mut out: Vec<Series> = Vec::new();
    let mut total = vec![0.0; xs.len()];
    for w in &c.weapons {
        if w.kind == crate::stats::WeaponKind::Drone && !spec.include_drones {
            continue;
        }
        let drone_v = if w.kind == crate::stats::WeaponKind::Drone { c.drone_speed.unwrap_or(drone_speed) } else { 0.0 };
        let full = if spec.kind.is_volley() { w.volley.total() } else { w.dps };
        let points: Vec<[f64; 2]> = xs
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                let (distance, target) = at(x);
                let y = full * applied_factor(w, distance, &target, drone_v, c.drones.control_range);
                total[i] += y;
                [x, y]
            })
            .collect();
        out.push(Series { label: SeriesLabel::Weapon(w.type_id, w.charge), points });
    }
    if out.len() > 1 {
        out.push(Series { label: SeriesLabel::Total, points: xs.iter().zip(total).map(|(x, y)| [*x, y]).collect() });
    }
    out
}

/// The curves for the graph. `cap` is needed for the capacitor graph only.
pub fn sample(c: &Computed, spec: &GraphSpec, cap: Option<&CapInputs>) -> Vec<Series> {
    let n = spec.points.max(2);
    match spec.kind {
        GraphKind::DamageVsDistance | GraphKind::VolleyVsDistance => {
            let reach = c.weapons.iter().map(|w| w.missile.as_ref().map_or(w.optimal + 3.0 * w.falloff, |m| m.long_range())).fold(10_000.0, f64::max);
            let to = spec.x_to.unwrap_or(reach * 1.1);
            let t = spec.target.clone();
            damage_series(c, spec, with_edges(grid(0.0, to, n), &damage_edges(c, spec.include_drones)).into_iter(), move |x| (x, t.clone()))
        }
        GraphKind::DamageVsTargetSpeed | GraphKind::VolleyVsTargetSpeed => {
            let to = spec.x_to.unwrap_or(3000.0);
            let base = spec.target.clone();
            damage_series(c, spec, grid(0.0, to, n), move |x| (f64::from(base.distance), TargetProfile { velocity: x as f32, transversal: x as f32, ..base.clone() }))
        }
        GraphKind::DamageVsTargetSignature | GraphKind::VolleyVsTargetSignature => {
            let to = spec.x_to.unwrap_or(2000.0);
            let base = spec.target.clone();
            damage_series(c, spec, grid(1.0, to, n), move |x| (f64::from(base.distance), TargetProfile { signature: x as f32, ..base.clone() }))
        }
        GraphKind::CapacitorOverTime => {
            let horizon = spec.x_to.unwrap_or_else(|| c.cap.lasts_s.map_or(600.0, |t| (t * 1.2).max(60.0)));
            let users = cap.map(|c| c.users.as_slice()).unwrap_or(&[]);
            let trace = cap_sim::trace(c.cap.capacity, c.cap.recharge_s, users, horizon, n);
            vec![Series { label: SeriesLabel::Capacitor, points: trace }]
        }
        GraphKind::ShieldOverTime => {
            let horizon = spec.x_to.unwrap_or(c.tank.shield_recharge_s.max(1.0) * 1.5);
            let trace = cap_sim::trace(c.tank.shield.hp, c.tank.shield_recharge_s, &[], horizon, n).into_iter().map(|[t, v]| [t, v]).collect();
            vec![Series { label: SeriesLabel::Shield, points: trace }]
        }
        GraphKind::VelocityOverTime => {
            let m = &c.mobility;
            let horizon = spec.x_to.unwrap_or((m.align_s * 4.0).max(5.0));
            vec![Series { label: SeriesLabel::Velocity, points: grid(0.0, horizon, n).map(|t| [t, velocity_at(t, m.max_velocity, m.agility, m.mass)]).collect() }]
        }
        GraphKind::WarpTime => {
            let to = spec.x_to.unwrap_or(100.0);
            vec![Series { label: SeriesLabel::Warp, points: grid(0.0, to, n).map(|au| [au, warp_time(au * AU, c.mobility.warp_speed_au, c.mobility.max_velocity)]).collect() }]
        }
        GraphKind::LockTime => {
            let to = spec.x_to.unwrap_or(2000.0);
            vec![Series { label: SeriesLabel::Lock, points: grid(10.0, to, n).map(|sig| [sig, c.sensors.lock_time_s(sig)]).collect() }]
        }
        GraphKind::RemoteRepVsDistance => remote_series(c, spec, n, true),
        GraphKind::EwarVsDistance => remote_series(c, spec, n, false),
    }
}

/// Remote modules by type and kind against distance: repairs (with their total) or ewar.
fn remote_series(c: &Computed, spec: &GraphSpec, n: usize, reps: bool) -> Vec<Series> {
    let mods: Vec<&crate::stats::RemoteModule> = c.remote.modules.iter().filter(|m| m.kind.is_rep() == reps).collect();
    if mods.is_empty() {
        return Vec::new();
    }
    let reach = mods.iter().map(|m| m.optimal + 3.0 * m.falloff).fold(1000.0, f64::max);
    // A module without falloff stops dead at its optimal; one with falloff at three of them.
    let edges: Vec<f64> = mods.iter().map(|m| if m.falloff > 0.0 { m.optimal + 3.0 * m.falloff } else { m.optimal }).collect();
    let xs: Vec<f64> = with_edges(grid(0.0, spec.x_to.unwrap_or(reach * 1.1), n), &edges);
    let mut out: Vec<Series> = Vec::new();
    let mut total = vec![0.0; xs.len()];
    for m in mods {
        let label = SeriesLabel::Remote(m.type_id, m.kind);
        let points: Vec<f64> = xs.iter().map(|&x| m.amount * range_factor(x, m.optimal, m.falloff)).collect();
        if m.kind != RemoteKind::CapTransfer {
            for (t, y) in total.iter_mut().zip(&points) {
                *t += y;
            }
        }
        match out.iter_mut().find(|s| s.label == label) {
            Some(s) => s.points.iter_mut().zip(points).for_each(|(p, y)| p[1] += y),
            None => out.push(Series { label, points: xs.iter().zip(points).map(|(x, y)| [*x, y]).collect() }),
        }
    }
    if reps && out.iter().filter(|s| !matches!(s.label, SeriesLabel::Remote(_, RemoteKind::CapTransfer))).count() > 1 {
        out.push(Series { label: SeriesLabel::Total, points: xs.iter().zip(total).map(|(x, y)| [*x, y]).collect() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Env;
    use crate::fit::{DroneStack, Fit, Module, Slot, State};
    use crate::skills::SkillSet;
    use crate::stats::compute;
    use crate::test_support::{dataset, ids};

    fn fit() -> Fit {
        let mut f = Fit::new(ids::SHIP, "t");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
        f.modules.push(Module::new(ids::LAUNCHER, Slot::High, 1).with_charge(ids::MISSILE).with_state(State::Active));
        f.drones.push(DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None });
        f
    }

    #[test]
    fn damage_against_distance_falls_off_per_weapon_and_sums() {
        let d = dataset();
        let f = fit();
        let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
        let spec = GraphSpec::new(GraphKind::DamageVsDistance, TargetProfile { signature: 400.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0 });
        let series = sample(&c, &spec, None);
        assert_eq!(series.len(), 4, "gun, launcher, drones and the total");
        let gun = series.iter().find(|s| s.label == SeriesLabel::Weapon(ids::GUN, Some(ids::AMMO))).unwrap();
        let gun_group = c.weapons.iter().find(|w| w.type_id == ids::GUN).unwrap();
        assert!((gun.points[0][1] - gun_group.dps * 0.5 * (1.0 + 0.98 + 0.0501)).abs() < 1e-6, "at zero range, every shot hits");
        assert!(gun.points.last().unwrap()[1] < gun.points[0][1] * 0.01, "nothing lands past three falloffs");
        let missiles = series.iter().find(|s| matches!(s.label, SeriesLabel::Weapon(id, _) if id == ids::LAUNCHER)).unwrap();
        let missile_group = c.weapons.iter().find(|w| w.type_id == ids::LAUNCHER).unwrap();
        let within = missiles.points.iter().filter(|p| p[0] < missile_group.missile.as_ref().unwrap().range).last().unwrap();
        assert!((within[1] - missile_group.dps).abs() < 1e-6, "a still 400 m target takes full missile damage up to the range");
        assert_eq!(missiles.points.last().unwrap()[1], 0.0);
        let volley = sample(&c, &GraphSpec::new(GraphKind::VolleyVsDistance, spec.target.clone()), None);
        let gun_v = volley.iter().find(|s| matches!(s.label, SeriesLabel::Weapon(id, _) if id == ids::GUN)).unwrap();
        assert!((gun_v.points[0][1] / gun.points[0][1] - gun_group.volley.total() / gun_group.dps).abs() < 1e-9, "a volley graph is the damage graph scaled to one shot");
        let range = missile_group.missile.as_ref().unwrap().range;
        let wall: Vec<&[f64; 2]> = missiles.points.iter().filter(|p| (p[0] - range).abs() <= EDGE + 1e-9).collect();
        assert_eq!(wall.len(), 3, "sampled just before, at and just after the flight range: {wall:?}");
        assert!(wall[0][1] > 0.0 && wall[1][1] > 0.0 && wall[2][1] == 0.0, "the drop is a wall half a metre wide");
        let total = series.iter().find(|s| s.label == SeriesLabel::Total).unwrap();
        let sum: f64 = series.iter().filter(|s| s.label != SeriesLabel::Total).map(|s| s.points[0][1]).sum();
        assert!((total.points[0][1] - sum).abs() < 1e-9);
    }

    #[test]
    fn faster_and_smaller_targets_take_less() {
        let d = dataset();
        let c = compute(&d, &fit(), &SkillSet::all_v(), &Env::default());
        let target = TargetProfile { signature: 100.0, velocity: 0.0, transversal: 0.0, distance: 1000.0, radius: 0.0 };
        let speed = sample(&c, &GraphSpec::new(GraphKind::DamageVsTargetSpeed, target.clone()), None);
        let total = speed.iter().find(|s| s.label == SeriesLabel::Total).unwrap();
        assert!(total.points[0][1] > total.points.last().unwrap()[1] * 2.0);
        let sig = sample(&c, &GraphSpec::new(GraphKind::DamageVsTargetSignature, target), None);
        let total = sig.iter().find(|s| s.label == SeriesLabel::Total).unwrap();
        assert!(total.points[0][1] < total.points.last().unwrap()[1]);
        let no_drones = sample(&c, &GraphSpec { include_drones: false, ..GraphSpec::new(GraphKind::DamageVsDistance, TargetProfile::default()) }, None);
        assert!(no_drones.iter().all(|s| !matches!(s.label, SeriesLabel::Weapon(id, _) if id == ids::DRONE)));
    }

    #[test]
    fn time_curves_start_where_they_should() {
        let d = dataset();
        let c = compute(&d, &fit(), &SkillSet::all_v(), &Env::default());
        let v = sample(&c, &GraphSpec::new(GraphKind::VelocityOverTime, TargetProfile::default()), None);
        assert_eq!(v[0].points[0], [0.0, 0.0]);
        assert!(v[0].points.last().unwrap()[1] > c.mobility.max_velocity * 0.9);
        let cap = sample(&c, &GraphSpec::new(GraphKind::CapacitorOverTime, TargetProfile::default()), Some(&CapInputs { users: vec![] }));
        assert_eq!(cap[0].points[0][1], c.cap.capacity);
        let shield = sample(&c, &GraphSpec::new(GraphKind::ShieldOverTime, TargetProfile::default()), None);
        assert_eq!(shield[0].points[0][1], c.tank.shield.hp);
        let lock = sample(&c, &GraphSpec::new(GraphKind::LockTime, TargetProfile::default()), None);
        assert!(lock[0].points[0][1] > lock[0].points.last().unwrap()[1], "big targets lock faster");
        let warp = sample(&c, &GraphSpec::new(GraphKind::WarpTime, TargetProfile::default()), None);
        assert_eq!(warp[0].points[0][1], 0.0);
        assert!(warp[0].points.last().unwrap()[1] > warp[0].points[1][1]);
    }

    #[test]
    fn warp_time_matches_the_published_formula() {
        // 5 AU/s, a 10 AU warp: 5.1 s to speed, 1.2 s cruising, 13.6 s slowing down.
        let t = warp_time(10.0 * AU, 5.0, 365.0);
        assert!((t - 20.0).abs() < 1.0, "{t}");
        assert!(warp_time(0.5 * AU, 5.0, 365.0) < t);
        assert_eq!(warp_time(AU, 0.0, 365.0), 0.0);
        assert!((velocity_at(1.0e9, 365.0, 3.0, 1.0e6) - 365.0).abs() < 1e-6);
    }

    #[test]
    fn remote_and_ewar_curves_follow_the_modules_reach() {
        let d = dataset();
        let mut f = Fit::new(ids::SHIP, "logi");
        f.modules.push(Module::new(ids::REMOTE_REP, Slot::High, 0).with_state(State::Active));
        f.modules.push(Module::new(ids::REMOTE_REP, Slot::High, 1).with_state(State::Active));
        f.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
        let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
        let reps = sample(&c, &GraphSpec::new(GraphKind::RemoteRepVsDistance, TargetProfile::default()), None);
        assert_eq!(reps.len(), 1, "two repairers of one type are one curve, with no total for one kind: {reps:?}");
        let SeriesLabel::Remote(t, kind) = reps[0].label else { panic!() };
        assert_eq!((t, kind), (ids::REMOTE_REP, crate::stats::RemoteKind::ArmorRep));
        assert!((reps[0].points[0][1] - 20.0).abs() < 1e-9);
        assert_eq!(reps[0].points.last().unwrap()[1], 0.0);
        let at = |x: f64| reps[0].points.iter().min_by(|a, b| (a[0] - x).abs().partial_cmp(&(b[0] - x).abs()).unwrap()).unwrap();
        assert!(at(9000.0)[1] < 12.0 && at(9000.0)[1] > 8.0, "{:?}: half at one falloff", at(9000.0));
        let ewar = sample(&c, &GraphSpec::new(GraphKind::EwarVsDistance, TargetProfile::default()), None);
        assert_eq!(ewar.len(), 1);
        assert_eq!(ewar[0].points[0][1], 60.0);
        assert!(ewar[0].points.last().unwrap()[0] > 25_000.0, "the axis reaches past three falloffs");
        let none = compute(&d, &Fit::new(ids::SHIP, "bare"), &SkillSet::all_v(), &Env::default());
        assert!(sample(&none, &GraphSpec::new(GraphKind::EwarVsDistance, TargetProfile::default()), None).is_empty());
    }
}

#[cfg(test)]
mod axes_tests {
    use super::*;

    #[test]
    fn every_graph_is_one_pair_of_axes_and_back() {
        for k in GraphKind::ALL {
            let (y, x) = k.axes();
            assert_eq!(GraphKind::from_axes(y, x), Some(k));
        }
        assert_eq!(GraphKind::from_axes(GraphY::Capacitor, GraphX::Distance), None, "capacitor runs over time only");
        assert_eq!(GraphY::Dps.xs(), vec![GraphX::Distance, GraphX::TargetSpeed, GraphX::TargetSignature]);
        assert_eq!(GraphY::LockTime.xs(), vec![GraphX::TargetSignature]);
        for y in GraphY::ALL {
            assert!(!y.xs().is_empty(), "{y:?} has a bottom");
        }
    }
}
