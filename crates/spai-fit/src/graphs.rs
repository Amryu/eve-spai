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
    /// Damage per second from opening fire: spooling weapons build up cycle by cycle, and against
    /// a fit, a reactive hardener shifts from its even split.
    DamageOverTime,
}

impl GraphKind {
    /// A damage graph of either kind: per second or per shot.
    pub fn is_damage(self) -> bool {
        matches!(self, GraphKind::DamageVsDistance | GraphKind::DamageVsTargetSpeed | GraphKind::DamageVsTargetSignature | GraphKind::DamageOverTime) || self.is_volley()
    }

    pub fn is_volley(self) -> bool {
        matches!(self, GraphKind::VolleyVsDistance | GraphKind::VolleyVsTargetSpeed | GraphKind::VolleyVsTargetSignature)
    }

    pub const ALL: [GraphKind; 14] = [
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
        GraphKind::DamageOverTime,
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
            K::DamageOverTime => (Y::Dps, X::Time),
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
    pub projection: Projection,
}

impl GraphSpec {
    pub fn new(kind: GraphKind, target: TargetProfile) -> Self {
        GraphSpec { kind, x_to: None, points: 400, target, include_drones: true, projection: Projection::default() }
    }
}

/// How the attacker's running webs and painters land on the target of a damage graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Projection {
    /// The target fit, for its ewar resistances and its own signature modifiers.
    pub target: Option<std::sync::Arc<Computed>>,
    /// The shooter's speed across the line of sight and the sine of the target's heading, so only
    /// the target's share of the transversal shrinks with its webbed speed; `None` scales all of it.
    pub across: Option<(f64, f64)>,
}

/// The target as it is at `distance` under `c`'s own ewar, each module by how far into its
/// falloff the target is.
fn projected_on(c: &Computed, p: &Projection, across: Option<(f64, f64)>, distance: f64, t: TargetProfile) -> TargetProfile {
    let e = crate::fleet::ewar::ewar_on(&[(1, c)], distance, 1.0, p.target.as_deref());
    if e.speed == 1.0 && e.signature == 1.0 {
        return t;
    }
    let velocity = f64::from(t.velocity) * e.speed;
    let transversal = match across {
        Some((shooter, sin)) => (shooter - velocity * sin).abs(),
        None => f64::from(t.transversal) * e.speed,
    };
    TargetProfile { velocity: velocity as f32, transversal: transversal as f32, signature: (f64::from(t.signature) * e.signature) as f32, ..t }
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
            (WeaponKind::Missile | WeaponKind::Breacher, Some(m)) => vec![m.short_range(), m.long_range()],
            (WeaponKind::Bomb, Some(m)) => vec![m.range - w.splash, m.range + w.splash],
            _ => Vec::new(),
        })
        .chain(c.weapons.iter().filter(|w| include_drones || w.kind != WeaponKind::Drone).filter_map(|w| match w.kind {
            WeaponKind::Missile | WeaponKind::Bomb | WeaponKind::Breacher => None,
            WeaponKind::Drone => Some(c.drones.control_range),
            WeaponKind::Smartbomb | WeaponKind::Vorton | WeaponKind::Superweapon => Some(w.optimal),
            _ => (w.falloff > 0.0).then_some(w.optimal + 3.0 * w.falloff),
        }))
        .collect();
    edges.retain(|e| e.is_finite() && *e > 0.0);
    edges
}

/// What the fit's weapons land with perfect application, the ceiling of its damage graphs: turrets
/// and drones hit every shot, wrecking shots included, and spooling weapons are fully spooled on
/// the damage over time graph.
pub fn max_damage(c: &Computed, spec: &GraphSpec) -> f64 {
    use crate::stats::WeaponKind;
    c.weapons
        .iter()
        .filter(|w| spec.include_drones || w.kind != WeaponKind::Drone)
        .map(|w| {
            let full = if spec.kind.is_volley() {
                w.volley.total()
            } else if spec.kind == GraphKind::DamageOverTime {
                w.cold_dps() * (1.0 + w.spool_max)
            } else {
                w.dps
            };
            let perfect = match w.kind {
                WeaponKind::Turret | WeaponKind::Drone => crate::stats::application::turret_damage_factor(1.0),
                _ => 1.0,
            };
            full * perfect
        })
        .sum()
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

/// The curves for the graph.
pub fn sample(c: &Computed, spec: &GraphSpec) -> Vec<Series> {
    let n = spec.points.max(2);
    match spec.kind {
        GraphKind::DamageVsDistance | GraphKind::VolleyVsDistance => {
            let reach = c.weapons.iter().map(|w| w.reach()).fold(10_000.0, f64::max);
            let to = spec.x_to.unwrap_or(reach * 1.1);
            let t = spec.target.clone();
            damage_series(c, spec, with_edges(grid(0.0, to, n), &damage_edges(c, spec.include_drones)).into_iter(), |x| (x, projected_on(c, &spec.projection, spec.projection.across, x, t.clone())))
        }
        GraphKind::DamageVsTargetSpeed | GraphKind::VolleyVsTargetSpeed => {
            let to = spec.x_to.unwrap_or(3000.0);
            let base = spec.target.clone();
            // The axis is the target's own speed with its transversal; the webs cut both.
            damage_series(c, spec, grid(0.0, to, n), |x| (f64::from(base.distance), projected_on(c, &spec.projection, None, f64::from(base.distance), TargetProfile { velocity: x as f32, transversal: x as f32, ..base.clone() })))
        }
        GraphKind::DamageVsTargetSignature | GraphKind::VolleyVsTargetSignature => {
            let to = spec.x_to.unwrap_or(2000.0);
            let base = spec.target.clone();
            damage_series(c, spec, grid(1.0, to, n), |x| (f64::from(base.distance), projected_on(c, &spec.projection, spec.projection.across, f64::from(base.distance), TargetProfile { signature: x as f32, ..base.clone() })))
        }
        GraphKind::CapacitorOverTime => {
            let horizon = spec.x_to.unwrap_or_else(|| c.cap.lasts_s.map_or(600.0, |t| (t * 1.2).max(60.0)));
            // A target fit feeds the nosferatu as far as the game lets it.
            let t = spec.projection.target.as_deref();
            let opponent = t.map(|t| cap_sim::Opponent { capacity: t.cap.capacity, tau_s: t.cap.recharge_s, users: &t.cap.users, resist: t.resist.neut });
            let trace = cap_sim::trace_against(c.cap.capacity, c.cap.recharge_s, &c.cap.users, opponent.as_ref(), horizon, n);
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
        GraphKind::DamageOverTime => damage_over_time(c, spec, None),
    }
}

/// Remote modules by type and kind against distance: repairs (with their total) or ewar.
fn remote_series(c: &Computed, spec: &GraphSpec, n: usize, reps: bool) -> Vec<Series> {
    let mods: Vec<&crate::stats::RemoteModule> = c.remote.modules.iter().filter(|m| if reps { m.kind.is_rep() } else { !m.kind.is_assist() }).collect();
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
    fn the_capacitor_graph_feeds_a_nosferatu_only_from_a_fuller_target() {
        let d = dataset();
        let mut f = fit();
        f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 0).with_state(State::Active));
        f.modules.push(Module::new(ids::NOS, Slot::High, 2).with_state(State::Active));
        let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
        f.modules.pop();
        let bare = compute(&d, &f, &SkillSet::all_v(), &Env::default());
        let low = |c: &Computed, target: Option<std::sync::Arc<Computed>>| {
            let spec = GraphSpec { x_to: Some(300.0), projection: Projection { target, across: None }, ..GraphSpec::new(GraphKind::CapacitorOverTime, TargetProfile::default()) };
            sample(c, &spec)[0].points.iter().skip(1).map(|p| p[1]).fold(f64::MAX, f64::min)
        };
        assert!((low(&c, None) - low(&bare, None)).abs() < 1e-9, "an unknown target feeds it nothing");
        let idle = std::sync::Arc::new(compute(&d, &Fit::new(ids::SHIP, "idle"), &SkillSet::all_v(), &Env::default()));
        assert!(low(&c, Some(idle)) > low(&bare, None), "a target sitting full does");
    }

    #[test]
    fn damage_against_distance_falls_off_per_weapon_and_sums() {
        let d = dataset();
        let f = fit();
        let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
        let spec = GraphSpec::new(GraphKind::DamageVsDistance, TargetProfile { signature: 400.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0 });
        let series = sample(&c, &spec);
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
        let volley = sample(&c, &GraphSpec::new(GraphKind::VolleyVsDistance, spec.target.clone()));
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
    fn the_max_line_is_the_damage_before_application() {
        let d = dataset();
        let c = compute(&d, &fit(), &SkillSet::all_v(), &Env::default());
        let target = TargetProfile { signature: 40.0, velocity: 2000.0, transversal: 2000.0, distance: 5000.0, radius: 0.0 };
        for kind in [GraphKind::DamageVsDistance, GraphKind::DamageVsTargetSpeed, GraphKind::VolleyVsTargetSignature, GraphKind::DamageOverTime] {
            let spec = GraphSpec::new(kind, target.clone());
            let max = max_damage(&c, &spec);
            let top = sample(&c, &spec).iter().flat_map(|s| s.points.iter().map(|p| p[1])).fold(0.0, f64::max);
            assert!(top <= max + 1e-9, "{kind:?}: the curve stays under the line, {top} > {max}");
            assert!(top < max * 0.9, "{kind:?}: a small fast target takes less than the line, {top} vs {max}");
        }
        let gun = c.weapons.iter().find(|w| w.type_id == ids::GUN).unwrap();
        let ideal = GraphSpec::new(GraphKind::DamageVsDistance, TargetProfile { signature: 10_000.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0 });
        let gun_line = sample(&c, &ideal).into_iter().find(|s| s.label == SeriesLabel::Weapon(ids::GUN, Some(ids::AMMO))).unwrap();
        assert!((gun_line.points[0][1] - gun.dps * crate::stats::application::turret_damage_factor(1.0)).abs() < 1e-6, "a still target at point blank touches it");
        let without = max_damage(&c, &GraphSpec { include_drones: false, ..ideal.clone() });
        assert!(without < max_damage(&c, &ideal));
    }

    #[test]
    fn faster_and_smaller_targets_take_less() {
        let d = dataset();
        let c = compute(&d, &fit(), &SkillSet::all_v(), &Env::default());
        let target = TargetProfile { signature: 100.0, velocity: 0.0, transversal: 0.0, distance: 1000.0, radius: 0.0 };
        let speed = sample(&c, &GraphSpec::new(GraphKind::DamageVsTargetSpeed, target.clone()));
        let total = speed.iter().find(|s| s.label == SeriesLabel::Total).unwrap();
        assert!(total.points[0][1] > total.points.last().unwrap()[1] * 2.0);
        let sig = sample(&c, &GraphSpec::new(GraphKind::DamageVsTargetSignature, target));
        let total = sig.iter().find(|s| s.label == SeriesLabel::Total).unwrap();
        assert!(total.points[0][1] < total.points.last().unwrap()[1]);
        let no_drones = sample(&c, &GraphSpec { include_drones: false, ..GraphSpec::new(GraphKind::DamageVsDistance, TargetProfile::default()) });
        assert!(no_drones.iter().all(|s| !matches!(s.label, SeriesLabel::Weapon(id, _) if id == ids::DRONE)));
    }

    #[test]
    fn time_curves_start_where_they_should() {
        let d = dataset();
        let c = compute(&d, &fit(), &SkillSet::all_v(), &Env::default());
        let v = sample(&c, &GraphSpec::new(GraphKind::VelocityOverTime, TargetProfile::default()));
        assert_eq!(v[0].points[0], [0.0, 0.0]);
        assert!(v[0].points.last().unwrap()[1] > c.mobility.max_velocity * 0.9);
        let cap = sample(&c, &GraphSpec::new(GraphKind::CapacitorOverTime, TargetProfile::default()));
        assert_eq!(cap[0].points[0][1], c.cap.capacity);
        let shield = sample(&c, &GraphSpec::new(GraphKind::ShieldOverTime, TargetProfile::default()));
        assert_eq!(shield[0].points[0][1], c.tank.shield.hp);
        let lock = sample(&c, &GraphSpec::new(GraphKind::LockTime, TargetProfile::default()));
        assert!(lock[0].points[0][1] > lock[0].points.last().unwrap()[1], "big targets lock faster");
        let warp = sample(&c, &GraphSpec::new(GraphKind::WarpTime, TargetProfile::default()));
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
        let reps = sample(&c, &GraphSpec::new(GraphKind::RemoteRepVsDistance, TargetProfile::default()));
        assert_eq!(reps.len(), 1, "two repairers of one type are one curve, with no total for one kind: {reps:?}");
        let SeriesLabel::Remote(t, kind) = reps[0].label else { panic!() };
        assert_eq!((t, kind), (ids::REMOTE_REP, crate::stats::RemoteKind::ArmorRep));
        assert!((reps[0].points[0][1] - 20.0).abs() < 1e-9);
        assert_eq!(reps[0].points.last().unwrap()[1], 0.0);
        let at = |x: f64| reps[0].points.iter().min_by(|a, b| (a[0] - x).abs().partial_cmp(&(b[0] - x).abs()).unwrap()).unwrap();
        assert!(at(9000.0)[1] < 12.0 && at(9000.0)[1] > 8.0, "{:?}: half at one falloff", at(9000.0));
        let ewar = sample(&c, &GraphSpec::new(GraphKind::EwarVsDistance, TargetProfile::default()));
        assert_eq!(ewar.len(), 1);
        assert_eq!(ewar[0].points[0][1], 60.0);
        assert!(ewar[0].points.last().unwrap()[0] > 25_000.0, "the axis reaches past three falloffs");
        let none = compute(&d, &Fit::new(ids::SHIP, "bare"), &SkillSet::all_v(), &Env::default());
        assert!(sample(&none, &GraphSpec::new(GraphKind::EwarVsDistance, TargetProfile::default())).is_empty());
    }

    #[test]
    fn the_attackers_running_web_slows_the_target_within_its_reach() {
        let d = dataset();
        let with_web = |state: State| {
            let mut f = Fit::new(ids::SHIP, "x");
            f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
            f.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(state));
            compute(&d, &f, &SkillSet::all_v(), &Env::default())
        };
        let (active, online) = (with_web(State::Active), with_web(State::Online));
        let bare = {
            let mut f = Fit::new(ids::SHIP, "x");
            f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
            compute(&d, &f, &SkillSet::all_v(), &Env::default())
        };
        let total = |c: &Computed, spec: &GraphSpec| sample(c, spec).into_iter().next().unwrap().points;
        let target = TargetProfile { signature: 400.0, velocity: 1500.0, transversal: 1500.0, distance: 5000.0, radius: 0.0 };
        let speed = GraphSpec::new(GraphKind::DamageVsTargetSpeed, target.clone());
        let (a, o, b) = (total(&active, &speed), total(&online, &speed), total(&bare, &speed));
        assert_eq!(o, b, "an online web projects nothing");
        let fast = a.len() - 1;
        assert!(a[fast][1] > b[fast][1] * 1.01, "webbed, the fast target is hit harder: {:?} vs {:?}", a[fast], b[fast]);
        let at = |ps: &[[f64; 2]], x: f64| ps.iter().min_by(|p, q| (p[0] - x).abs().total_cmp(&(q[0] - x).abs())).unwrap()[1];
        let dist = GraphSpec { x_to: Some(40_000.0), ..GraphSpec::new(GraphKind::DamageVsDistance, target.clone()) };
        let (a, b) = (total(&active, &dist), total(&bare, &dist));
        assert!(at(&a, 2000.0) > at(&b, 2000.0) * 1.01, "inside the web's optimal");
        assert_eq!(at(&a, 30_000.0), at(&b, 30_000.0), "past three falloffs the web lands nothing");
        // Across the line of sight a still shooter leaves only the target's own share, cut by the web.
        let across = GraphSpec { projection: Projection { target: None, across: Some((0.0, 1.0)) }, ..GraphSpec::new(GraphKind::DamageVsTargetSignature, target) };
        let projected = projected_on(&active, &across.projection, across.projection.across, 5000.0, across.target.clone());
        assert!((projected.velocity - 600.0).abs() < 1e-3 && (projected.transversal - 600.0).abs() < 1e-3, "{projected:?}");
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
        assert_eq!(GraphY::Dps.xs(), vec![GraphX::Distance, GraphX::TargetSpeed, GraphX::TargetSignature, GraphX::Time]);
        assert_eq!(GraphY::LockTime.xs(), vec![GraphX::TargetSignature]);
        for y in GraphY::ALL {
            assert!(!y.xs().is_empty(), "{y:?} has a bottom");
        }
    }
}

#[cfg(test)]
mod speed_bump {
    use super::*;
    use crate::test_support::{dataset, ids};
    use crate::fit::{DroneStack, Fit, Module, Slot, State};

    #[test]
    fn spooling_damage_climbs_a_step_a_cycle_and_a_hardener_eats_into_it() {
        let d = dataset();
        let mut f = Fit::new(ids::SHIP, "x");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
        let mut c = crate::stats::compute(&d, &f, &crate::skills::SkillSet::all_v(), &crate::calc::Env::default());
        let target = TargetProfile { signature: 400.0, distance: 0.0, ..Default::default() };
        // A disintegrator's spool on the gun: +100% at most, 10% a cycle, figures set full.
        let w = &mut c.weapons[0];
        let cold = w.dps;
        w.spool_max = 1.0;
        w.spool_per_cycle = 0.1;
        w.spool_frac = 1.0;
        w.dps = cold * 2.0;
        let spec = GraphSpec { x_to: Some(120.0), ..GraphSpec::new(GraphKind::DamageOverTime, target.clone()) };
        let line = &damage_over_time(&c, &spec, None)[0].points;
        let first = line[0][1];
        let last = line.last().unwrap()[1];
        assert!(line.windows(2).all(|p| p[1][1] >= p[0][1] - 1e-9), "never falls while spooling");
        assert!((last / first - 2.0).abs() < 1e-6, "from cold to twice that: {first} to {last}");
        // Against a reactive hardener that starts even, the damage taken falls as it adapts.
        let rah = crate::calc::Rah { armor_before: [1.0; 4], start: [0.15; 4], shift: 0.06, cycle_s: 10.0 };
        c.weapons[0].spool_max = 0.0;
        c.weapons[0].dps = cold;
        let tank = TargetTank { resonance: [1.0; 4], rah: Some(rah) };
        let taken = &damage_over_time(&c, &spec, Some(&tank))[0].points;
        let (start, end) = (taken[0][1], taken.last().unwrap()[1]);
        let bare = damage_over_time(&c, &spec, None)[0].points[0][1];
        assert!((start - bare * 0.85).abs() < 1e-6, "even at first: 15% off everything");
        assert!(end < start * 0.95, "adapted, it lets less through: {start} to {end}");
    }

    #[test]
    fn damage_never_rises_with_target_speed() {
        let d = dataset();
        let mut f = Fit::new(ids::SHIP, "x");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
        f.modules.push(Module::new(ids::LAUNCHER, Slot::High, 1).with_charge(ids::MISSILE).with_state(State::Active));
        f.drones.push(DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None });
        let c = crate::stats::compute(&d, &f, &crate::skills::SkillSet::all_v(), &crate::calc::Env::default());
        for kind in [GraphKind::VolleyVsTargetSpeed, GraphKind::DamageVsTargetSpeed] {
            let spec = GraphSpec::new(kind, TargetProfile { signature: 120.0, distance: 5000.0, ..Default::default() });
            for s in sample(&c, &spec) {
                for w in s.points.windows(2) {
                    assert!(w[1][1] <= w[0][1] + 1e-9, "{kind:?} {:?} rises from {:?} to {:?}", s.label, w[0], w[1]);
                }
            }
        }
    }
}

/// The series' value at `x`, read between its points.
#[cfg(test)]
pub(crate) fn value_at_for_test(points: &[[f64; 2]], x: f64) -> f64 {
    let i = points.iter().position(|p| p[0] >= x).unwrap_or(points.len() - 1);
    if i == 0 {
        return points[0][1];
    }
    let (a, b) = (points[i - 1], points[i]);
    if b[0] == a[0] { b[1] } else { a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]) }
}

/// A target's tank for damage taken over time: the resonances of the layer being shot, and its
/// reactive hardener if it runs one (the resonances then are the armor's without it).
#[derive(Clone, Copy, Debug)]
pub struct TargetTank {
    pub resonance: [f64; 4],
    pub rah: Option<crate::calc::Rah>,
}

/// Damage per second from the moment fire opens, at the spec's target and distance. Each weapon's
/// spool builds up a step a cycle from cold. With a target tank, one line of the damage it takes,
/// its reactive hardener starting even and shifting each of its cycles towards what hits it.
pub fn damage_over_time(c: &Computed, spec: &GraphSpec, tank: Option<&TargetTank>) -> Vec<Series> {
    use crate::stats::WeaponKind;
    let weapons: Vec<&crate::stats::WeaponGroup> = c.weapons.iter().filter(|w| spec.include_drones || w.kind != WeaponKind::Drone).collect();
    if weapons.is_empty() {
        return Vec::new();
    }
    let spool_end = weapons.iter().filter_map(|w| w.spool_cycles().map(|n| f64::from(n) * w.cycle_s)).fold(0.0, f64::max);
    let rah_end = tank.and_then(|t| t.rah).filter(|r| r.shift > 0.0).map_or(0.0, |r| r.cycle_s * 12.0);
    let horizon = spec.x_to.unwrap_or_else(|| (spool_end.max(rah_end) * 1.25).max(60.0));
    // The steps: every cycle boundary of a spooling weapon until full, every hardener cycle.
    let mut steps: Vec<f64> = Vec::new();
    for w in &weapons {
        if let Some(n) = w.spool_cycles() {
            steps.extend((1..=n).map(|k| f64::from(k) * w.cycle_s));
        }
    }
    if let Some(r) = tank.and_then(|t| t.rah).filter(|r| r.shift > 0.0 && r.cycle_s > 0.0) {
        let mut t = r.cycle_s;
        while t < horizon {
            steps.push(t);
            t += r.cycle_s;
        }
    }
    let mut ts: Vec<f64> = grid(0.0, horizon, spec.points.max(2)).collect();
    for s in steps.iter().filter(|s| **s < horizon) {
        ts.push(s - 1e-3);
        ts.push(*s);
    }
    ts.sort_by(|a, b| a.total_cmp(b));
    ts.dedup();
    let distance = f64::from(spec.target.distance);
    let target = &projected_on(c, &spec.projection, spec.projection.across, distance, spec.target.clone());
    let drone_v = c.drone_speed.unwrap_or(0.0);
    // Each weapon's damage per second by type, cold, as it lands on the target.
    let landed: Vec<[f64; 4]> = weapons
        .iter()
        .map(|w| {
            let v = &w.volley;
            let total = v.total().max(1e-9);
            let f = w.cold_dps() * applied_factor(w, distance, target, drone_v, c.drones.control_range) / total;
            [v.em * f, v.thermal * f, v.kinetic * f, v.explosive * f]
        })
        .collect();
    let at = |t: f64| -> Vec<[f64; 4]> { weapons.iter().zip(&landed).map(|(w, d)| d.map(|x| x * w.spool_at(t))).collect() };
    let Some(tank) = tank else {
        let mut out: Vec<Series> = weapons.iter().map(|w| Series { label: SeriesLabel::Weapon(w.type_id, w.charge), points: Vec::new() }).collect();
        let mut total = Vec::new();
        for &t in &ts {
            let per = at(t);
            let mut sum = 0.0;
            for (k, d) in per.iter().enumerate() {
                let y: f64 = d.iter().sum();
                out[k].points.push([t, y]);
                sum += y;
            }
            total.push([t, sum]);
        }
        if out.len() > 1 {
            out.push(Series { label: SeriesLabel::Total, points: total });
        }
        return out;
    };
    // Against a tank: the hardener's split for each of its cycles, from what hit it in the last.
    let resonance_at = |t: f64| -> [f64; 4] {
        let Some(r) = tank.rah else { return tank.resonance };
        let cycles = if r.cycle_s > 0.0 && r.shift > 0.0 { (t / r.cycle_s).floor() as usize } else { 0 };
        let mut res = r.start;
        for k in 0..cycles {
            let incoming = at(k as f64 * r.cycle_s).iter().fold([0.0; 4], |a, d| std::array::from_fn(|i| a[i] + d[i]));
            let sum: f64 = incoming.iter().sum();
            if sum <= 0.0 {
                break;
            }
            res = crate::calc::patch::adapt_step(res, r.armor_before, incoming.map(|x| x / sum), r.shift);
        }
        std::array::from_fn(|i| r.armor_before[i] * (1.0 - res[i]))
    };
    let points = ts
        .iter()
        .map(|&t| {
            let res = resonance_at(t);
            let y: f64 = at(t).iter().map(|d| (0..4).map(|i| d[i] * res[i]).sum::<f64>()).sum();
            [t, y]
        })
        .collect();
    vec![Series { label: SeriesLabel::Total, points }]
}
