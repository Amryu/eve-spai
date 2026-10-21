//! A fleet's ewar on the other at a given distance, from its ships' own modules: each module's
//! strength scaled by how far into its falloff the distance is, stacked with the others.

use crate::dogma::Op;
use crate::stats::{range_factor, Computed, RemoteKind};

/// What the other side's ewar does to a ship (or the ships of a fleet on average): multipliers
/// of its figures, the capacitor drained per second, and whether it is held in place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EwarOn {
    pub speed: f64,
    pub signature: f64,
    pub lock_range: f64,
    pub scan_resolution: f64,
    pub tracking: f64,
    pub optimal: f64,
    pub falloff: f64,
    pub explosion_radius: f64,
    pub explosion_velocity: f64,
    pub missile_velocity: f64,
    pub flight_time: f64,
    /// Capacitor drained per second, all neutralisers together.
    pub neut: f64,
    /// Warp scramblers or disruptors in reach: held.
    pub scrammed: bool,
    pub pointed: bool,
    /// Any web in reach.
    pub webbed: bool,
}

impl Default for EwarOn {
    fn default() -> Self {
        EwarOn { speed: 1.0, signature: 1.0, lock_range: 1.0, scan_resolution: 1.0, tracking: 1.0, optimal: 1.0, falloff: 1.0, explosion_radius: 1.0, explosion_velocity: 1.0, missile_velocity: 1.0, flight_time: 1.0, neut: 0.0, scrammed: false, pointed: false, webbed: false }
    }
}

/// One module of the other side, with how many of it fly.
struct Source {
    kind: RemoteKind,
    parts: [f64; 4],
    amount: f64,
    optimal: f64,
    falloff: f64,
    count: f64,
}

/// Penalised percent changes on one figure, for `k` modules (fractional: the last counts in part).
fn stacked(percents: &[f64], k: f64) -> f64 {
    let mut sorted: Vec<f64> = percents.to_vec();
    sorted.sort_by(|a, b| b.abs().total_cmp(&a.abs()));
    let at = |n: usize| -> f64 {
        let mods: Vec<(Op, f64, bool)> = sorted.iter().take(n).map(|p| (Op::PostPercent, *p, true)).collect();
        crate::calc::combine(1.0, true, &mods)
    };
    let whole = k.floor() as usize;
    let part = k - k.floor();
    let lo = at(whole.min(sorted.len()));
    let hi = at((whole + 1).min(sorted.len()));
    lo + (hi - lo) * part
}

/// The ewar `ships` (each a fit with how many fly) put on the other side at `distance`, spread
/// over `targets` ships: 1 for everything on the primary, the enemy's count to share it out.
pub fn ewar_at(ships: &[(u32, &Computed)], distance: f64, targets: f64) -> EwarOn {
    let sources: Vec<Source> = ships
        .iter()
        .flat_map(|(n, c)| c.remote.modules.iter().map(move |m| Source { kind: m.kind, parts: m.parts, amount: m.amount, optimal: m.optimal, falloff: m.falloff, count: f64::from(*n) }))
        .collect();
    let targets = targets.max(1.0);
    let mut out = EwarOn::default();
    // Per figure, the percent each module gives at this distance, as many times as it flies.
    let figure = |kind: RemoteKind, part: usize| -> (Vec<f64>, f64) {
        let mut list = Vec::new();
        let mut total = 0.0;
        for s in sources.iter().filter(|s| s.kind == kind && s.parts[part] != 0.0) {
            let f = range_factor(distance, s.optimal, s.falloff);
            if f <= 0.0 {
                continue;
            }
            for _ in 0..(s.count.ceil() as usize).max(1) {
                list.push(s.parts[part] * f);
            }
            total += s.count;
        }
        (list, total / targets)
    };
    let apply = |kind: RemoteKind, part: usize| -> f64 {
        let (list, k) = figure(kind, part);
        if list.is_empty() {
            1.0
        } else {
            stacked(&list, k.min(list.len() as f64))
        }
    };
    out.speed = apply(RemoteKind::Web, 0);
    out.signature = apply(RemoteKind::Painter, 0);
    out.lock_range = apply(RemoteKind::Damp, 0);
    out.scan_resolution = apply(RemoteKind::Damp, 1);
    out.tracking = apply(RemoteKind::TrackingDisruptor, 0);
    out.optimal = apply(RemoteKind::TrackingDisruptor, 1);
    out.falloff = apply(RemoteKind::TrackingDisruptor, 2);
    out.explosion_radius = apply(RemoteKind::GuidanceDisruptor, 0);
    out.explosion_velocity = apply(RemoteKind::GuidanceDisruptor, 1);
    out.missile_velocity = apply(RemoteKind::GuidanceDisruptor, 2);
    out.flight_time = apply(RemoteKind::GuidanceDisruptor, 3);
    out.neut = sources.iter().filter(|s| s.kind == RemoteKind::Neut).map(|s| s.amount * s.count * range_factor(distance, s.optimal, s.falloff)).sum::<f64>() / targets;
    // Tackle holds or not: within its optimal, it has no falloff.
    let reaches = |kind: RemoteKind| sources.iter().any(|s| s.kind == kind && distance <= s.optimal);
    out.scrammed = reaches(RemoteKind::Scram);
    out.pointed = out.scrammed || reaches(RemoteKind::Point);
    out.webbed = sources.iter().any(|s| s.kind == RemoteKind::Web && range_factor(distance, s.optimal, s.falloff) > 0.0);
    out
}

/// Where each kind of the side's ewar stops working: (kind, optimal, the outer edge where the
/// strongest module's strength reaches nothing).
pub fn cutoffs(ships: &[(u32, &Computed)]) -> Vec<(RemoteKind, f64, f64)> {
    let mut out: Vec<(RemoteKind, f64, f64)> = Vec::new();
    for (_, c) in ships {
        for m in &c.remote.modules {
            if m.kind.is_rep() || m.kind == RemoteKind::CapTransfer {
                continue;
            }
            let edge = if matches!(m.kind, RemoteKind::Scram | RemoteKind::Point) || m.falloff <= 0.0 { m.optimal } else { m.optimal + 3.0 * m.falloff };
            match out.iter_mut().find(|(k, _, _)| *k == m.kind) {
                Some(e) => {
                    e.1 = e.1.max(m.optimal);
                    e.2 = e.2.max(edge);
                }
                None => out.push((m.kind, m.optimal, edge)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::{Fit, Module, Slot, State};
    use crate::test_support::{dataset, ids};

    #[test]
    fn webs_bite_in_range_fade_in_falloff_and_spread_thin() {
        let d = dataset();
        let mut f = Fit::new(ids::SHIP, "webber");
        f.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
        let c = crate::stats::compute(&d, &f, &crate::skills::SkillSet::all_v(), &crate::calc::Env::default());
        let web = c.remote.modules.iter().find(|m| m.kind == RemoteKind::Web).expect("a web");
        let strength = web.parts[0];
        assert!(strength < 0.0, "a web slows: {strength}");
        let near = ewar_at(&[(1, &c)], web.optimal * 0.5, 1.0);
        assert!((near.speed - (1.0 + strength / 100.0)).abs() < 1e-9 && near.webbed);
        let far = ewar_at(&[(1, &c)], web.optimal + 50_000.0 + 3.0 * web.falloff, 1.0);
        assert_eq!(far.speed, 1.0);
        assert!(!far.webbed);
        if web.falloff > 0.0 {
            let fading = ewar_at(&[(1, &c)], web.optimal + web.falloff, 1.0);
            assert!(fading.speed > near.speed && fading.speed < 1.0, "half strength in falloff");
        }
        // Two webs on one ship stack, penalised; one web over two ships is half a web each.
        let two = ewar_at(&[(2, &c)], web.optimal * 0.5, 1.0);
        assert!(two.speed < near.speed);
        let shared = ewar_at(&[(1, &c)], web.optimal * 0.5, 2.0);
        assert!((shared.speed - (1.0 + near.speed) / 2.0).abs() < 1e-9, "{} against {}", shared.speed, near.speed);
        let cut = cutoffs(&[(1, &c)]);
        assert!(cut.iter().any(|(k, o, _)| *k == RemoteKind::Web && (*o - web.optimal).abs() < 1e-9));
    }
}
