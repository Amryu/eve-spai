//! The capacitor over time: modules draw at their cycle times, boosters inject when there is
//! room, and between events the capacitor recharges along EVE's curve. Reports where it settles
//! or when it runs dry.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Something that cycles against the capacitor.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub cycle_ms: f64,
    /// Drawn at each activation.
    pub need: f64,
    /// Given at each activation (a booster charge).
    pub give: f64,
    /// Activations per load, then `reload_ms` pass; `None` runs forever.
    pub shots: Option<u32>,
    pub reload_ms: f64,
    /// A nosferatu, `give` its drain: true when it takes from a target emptier than the ship.
    pub nos: Option<bool>,
}

impl User {
    /// Net GJ per second over a long run, reloads included.
    pub fn average_per_s(&self) -> f64 {
        let per_cycle = self.need - self.give;
        match self.shots {
            Some(0) => 0.0,
            Some(n) => per_cycle * f64::from(n) / ((f64::from(n) * self.cycle_ms + self.reload_ms) / 1000.0),
            None => per_cycle / (self.cycle_ms / 1000.0),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Fraction of capacity the capacitor settles at; the lowest point of the run's last fifth.
    pub stable_at: Option<f64>,
    /// Seconds until a module found no capacitor to run.
    pub lasts_s: Option<f64>,
}

pub const DEFAULT_HORIZON_S: f64 = 3600.0;

/// The capacitor after `dt` seconds of recharging from `c` with capacity `cap` and time constant
/// `tau` seconds: `cap × (1 + (√(c / cap) − 1) × e^(−5 dt / τ))²`.
pub fn recharge(c: f64, cap: f64, tau: f64, dt: f64) -> f64 {
    if cap <= 0.0 || tau <= 0.0 {
        return c.min(cap);
    }
    let x = (c.max(0.0) / cap).sqrt() - 1.0;
    cap * (1.0 + x * (-5.0 * dt / tau).exp()).powi(2)
}

/// The capacitor level over time on `n` points up to `horizon_s`: a run of the simulation, the
/// recharge curve filling in between activations.
pub fn trace(capacity: f64, tau_s: f64, users: &[User], horizon_s: f64, n: usize) -> Vec<[f64; 2]> {
    trace_against(capacity, tau_s, users, None, horizon_s, n)
}

/// The ship a nosferatu takes from, running its own modules.
pub struct Opponent<'a> {
    pub capacity: f64,
    pub tau_s: f64,
    pub users: &'a [User],
    /// The share of a drain that lands: its capacitor warfare resistance.
    pub resist: f64,
}

/// The trace with each nosferatu taking what the game lets it from `opponent`'s capacitor: only
/// while that is fuller than the ship's, unless the hull overrides it, and never more than it
/// holds. Without one the target is unknown and only overriding nosferatu count.
pub fn trace_against(capacity: f64, tau_s: f64, users: &[User], opponent: Option<&Opponent>, horizon_s: f64, n: usize) -> Vec<[f64; 2]> {
    let n = n.max(2);
    let mut events: Vec<(f64, f64)> = Vec::new();
    let unknown: Vec<User>;
    let users = if opponent.is_none() && users.iter().any(|u| u.nos == Some(false)) {
        unknown = users.iter().filter(|u| u.nos != Some(false)).cloned().collect();
        &unknown[..]
    } else {
        users
    };
    simulate_impl(capacity, tau_s, users, horizon_s, true, Some(&mut events), opponent);
    let mut out = Vec::with_capacity(n);
    let mut last = (0.0, capacity);
    let mut next = 0;
    for i in 0..n {
        let t = horizon_s * i as f64 / (n - 1) as f64;
        while next < events.len() && events[next].0 <= t {
            last = events[next];
            next += 1;
        }
        let dried = events.last().is_some_and(|e| e.1 < 0.0 && e.0 <= t);
        out.push([t, if dried { 0.0 } else { recharge(last.1, capacity, tau_s, t - last.0) }]);
    }
    out
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Time(f64);

impl Eq for Time {}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Where a steady drain settles, from the recharge curve: the rate at fraction `p` is
/// `10 × capacity / τ × (√p − p)`, which equals the drain at
/// `p = ¼ (1 + √(1 − 0.4 × drain × τ / capacity))²`; `None` when the drain exceeds the peak.
pub fn stable_estimate(capacity: f64, tau_s: f64, drain_per_s: f64) -> Option<f64> {
    if capacity <= 0.0 || tau_s <= 0.0 {
        return None;
    }
    let x = 1.0 - 0.4 * drain_per_s * tau_s / capacity;
    (x >= 0.0).then(|| (0.25 * (1.0 + x.sqrt()).powi(2)).min(1.0))
}

/// Runs until the horizon or until a module cannot activate. Identical users start staggered
/// over one cycle, as pilots cycle their modules.
pub fn simulate(capacity: f64, tau_s: f64, users: &[User], horizon_s: f64) -> Outcome {
    simulate_with(capacity, tau_s, users, horizon_s, true)
}

pub fn simulate_with(capacity: f64, tau_s: f64, users: &[User], horizon_s: f64, stagger: bool) -> Outcome {
    simulate_impl(capacity, tau_s, users, horizon_s, stagger, None, None)
}

/// `events`, when given, receives `(time, capacitor after)` at every activation; running dry adds
/// a final `(time, -1)`.
fn simulate_impl(capacity: f64, tau_s: f64, users: &[User], horizon_s: f64, stagger: bool, mut events: Option<&mut Vec<(f64, f64)>>, opponent: Option<&Opponent>) -> Outcome {
    if capacity <= 0.0 {
        return Outcome { stable_at: None, lasts_s: Some(0.0) };
    }
    if users.is_empty() {
        return Outcome { stable_at: Some(1.0), lasts_s: None };
    }
    // Against a known opponent a nosferatu cycles like any module and takes what it finds.
    let mut heap_seed: Vec<(f64, usize)> = Vec::new();
    let is_booster = |u: &User| u.give > 0.0 && (opponent.is_none() || u.nos.is_none());
    let theirs = opponent.map_or(&[][..], |o| o.users);
    let mut their_cap = opponent.map_or(0.0, |o| o.capacity);
    let mut their_t = 0.0;
    let mut their_fired = vec![0u32; theirs.len()];
    for (j, u) in theirs.iter().enumerate() {
        let twins_before = theirs[..j].iter().filter(|o| *o == u).count();
        let twins = theirs.iter().filter(|o| *o == u).count();
        heap_seed.push((u.cycle_ms / 1000.0 * twins_before as f64 / twins as f64, users.len() + j));
    }
    let mut heap: BinaryHeap<Reverse<(Time, usize)>> = heap_seed.into_iter().map(|(t, i)| Reverse((Time(t), i))).collect();
    let mut fired = vec![0u32; users.len()];
    // Boosters wait until there is room, or until a module would otherwise find no capacitor.
    let mut ready_at: Vec<f64> = vec![0.0; users.len()];
    for (i, u) in users.iter().enumerate() {
        let twins_before = users[..i].iter().filter(|o| *o == u).count();
        let twins = users.iter().filter(|o| *o == u).count();
        let offset = if stagger { u.cycle_ms / 1000.0 * twins_before as f64 / twins as f64 } else { 0.0 };
        ready_at[i] = offset;
        heap.push(Reverse((Time(offset), i)));
    }
    let mut cap = capacity;
    let mut t = 0.0;
    let mut low_tail = capacity;
    let tail_from = horizon_s * 0.8;
    let next_cycle = |u: &User, fired: u32| match u.shots {
        Some(0) => None,
        Some(n) if fired % n == 0 => Some(u.cycle_ms + u.reload_ms),
        _ => Some(u.cycle_ms),
    };
    while let Some(Reverse((Time(at), i))) = heap.pop() {
        if at > horizon_s {
            break;
        }
        if let (Some(o), Some(j)) = (opponent, i.checked_sub(users.len())) {
            // Their modules run when they can, boosters when there is room; nothing of theirs
            // ends the run.
            their_cap = recharge(their_cap, o.capacity, o.tau_s, at - their_t);
            their_t = at;
            let u = &theirs[j];
            let give = if u.nos.is_some() { 0.0 } else { u.give };
            if give > 0.0 && their_cap + give > o.capacity {
                heap.push(Reverse((Time(at + 1.0), i)));
                continue;
            }
            if u.need <= their_cap {
                their_cap = (their_cap - u.need + give).min(o.capacity);
                their_fired[j] += 1;
            }
            if let Some(ms) = next_cycle(u, their_fired[j].max(1)) {
                heap.push(Reverse((Time(at + ms.max(1.0) / 1000.0), i)));
            }
            continue;
        }
        cap = recharge(cap, capacity, tau_s, at - t);
        t = at;
        let u = &users[i];
        if let (Some(o), Some(any_level)) = (opponent, u.nos) {
            their_cap = recharge(their_cap, o.capacity, o.tau_s, at - their_t);
            their_t = at;
            let fuller = o.capacity > 0.0 && their_cap / o.capacity > cap / capacity;
            let took = if any_level || fuller { (u.give * o.resist).min(their_cap) } else { 0.0 };
            their_cap -= took;
            cap = (cap + took).min(capacity);
            if let Some(ev) = events.as_deref_mut() {
                ev.push((t, cap));
            }
            if t >= tail_from {
                low_tail = low_tail.min(cap);
            }
            fired[i] += 1;
            if let Some(ms) = next_cycle(u, fired[i]) {
                heap.push(Reverse((Time(t + ms / 1000.0), i)));
            }
            continue;
        }
        if is_booster(u) {
            if ready_at[i] > t + 1e-9 {
                continue;
            }
            if cap + u.give > capacity {
                heap.push(Reverse((Time(t + 1.0), i)));
                continue;
            }
        } else {
            while u.need > cap {
                // The waiting booster that wastes the least, if any is ready.
                let pick = (0..users.len())
                    .filter(|&b| is_booster(&users[b]) && ready_at[b] <= t + 1e-9 && users[b].need <= cap)
                    .min_by(|&a, &b| (cap + users[a].give - capacity).abs().partial_cmp(&(cap + users[b].give - capacity).abs()).unwrap_or(std::cmp::Ordering::Equal));
                let Some(b) = pick else {
                    if let Some(ev) = events.as_deref_mut() {
                        ev.push((t, -1.0));
                    }
                    return Outcome { stable_at: None, lasts_s: Some(t) };
                };
                cap = (cap - users[b].need + users[b].give).min(capacity);
                fired[b] += 1;
                match next_cycle(&users[b], fired[b]) {
                    Some(ms) => {
                        ready_at[b] = t + ms / 1000.0;
                        heap.push(Reverse((Time(ready_at[b]), b)));
                    }
                    None => ready_at[b] = f64::INFINITY,
                }
            }
        }
        cap = (cap - u.need + u.give).min(capacity);
        if let Some(ev) = events.as_deref_mut() {
            ev.push((t, cap));
        }
        if t >= tail_from {
            low_tail = low_tail.min(cap);
        }
        fired[i] += 1;
        match next_cycle(u, fired[i]) {
            Some(ms) => {
                ready_at[i] = t + ms / 1000.0;
                heap.push(Reverse((Time(ready_at[i]), i)));
            }
            None => ready_at[i] = f64::INFINITY,
        }
    }
    let end = recharge(cap, capacity, tau_s, horizon_s - t);
    Outcome { stable_at: Some((low_tail.min(end) / capacity).clamp(0.0, 1.0)), lasts_s: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn the_recharge_curve_peaks_at_a_quarter_and_tops_out() {
        let (cap, tau) = (1000.0, 200.0);
        assert!(close(recharge(0.0, cap, tau, 1e9), cap, 1e-6));
        assert!(close(recharge(cap, cap, tau, 100.0), cap, 1e-9));
        // The rate at 25% is 2.5 × cap / tau.
        let c = 250.0;
        let rate = (recharge(c, cap, tau, 0.001) - c) / 0.001;
        assert!(close(rate, 2.5 * cap / tau, 0.05), "{rate}");
        // Lower at 10% and at 60%.
        assert!((recharge(100.0, cap, tau, 0.001) - 100.0) / 0.001 < rate);
        assert!((recharge(600.0, cap, tau, 0.001) - 600.0) / 0.001 < rate);
    }

    #[test]
    fn a_light_draw_settles_and_a_heavy_one_runs_dry() {
        let light = vec![User { cycle_ms: 5000.0, need: 10.0, give: 0.0, shots: None, reload_ms: 0.0, nos: None }];
        let out = simulate(1000.0, 200.0, &light, 3600.0);
        assert!(out.lasts_s.is_none());
        let at = out.stable_at.unwrap();
        let estimate = stable_estimate(1000.0, 200.0, 2.0).unwrap();
        assert!(close(at, estimate, 0.02), "{at} vs the closed-form {estimate}");
        assert!(close(estimate, 0.918, 0.001));
        assert_eq!(stable_estimate(1000.0, 200.0, 12.5), Some(0.25), "the peak rate holds at exactly a quarter");
        assert_eq!(stable_estimate(1000.0, 200.0, 13.0), None);
        let heavy = vec![User { cycle_ms: 1000.0, need: 50.0, give: 0.0, shots: None, reload_ms: 0.0, nos: None }];
        let out = simulate(1000.0, 200.0, &heavy, 3600.0);
        assert!(out.stable_at.is_none());
        let lasts = out.lasts_s.unwrap();
        assert!(lasts > 15.0 && lasts < 60.0, "{lasts}");
    }

    #[test]
    fn boosters_hold_back_until_there_is_room_and_then_keep_a_fit_up() {
        let users = vec![
            User { cycle_ms: 1000.0, need: 40.0, give: 0.0, shots: None, reload_ms: 0.0, nos: None },
            User { cycle_ms: 12000.0, need: 0.0, give: 800.0, shots: Some(3), reload_ms: 10000.0, nos: None },
        ];
        let without = simulate(1000.0, 200.0, &users[..1], 600.0);
        assert!(without.lasts_s.is_some());
        let with = simulate(1000.0, 200.0, &users, 600.0);
        assert!(with.lasts_s.is_none_or(|t| t > without.lasts_s.unwrap()), "{with:?} vs {without:?}");
        assert!(close(users[1].average_per_s(), -800.0 * 3.0 / 46.0, 1e-9));
    }

    #[test]
    fn twins_start_staggered_which_keeps_the_low_point_higher() {
        let u = User { cycle_ms: 10000.0, need: 2.5, give: 0.0, shots: None, reload_ms: 0.0, nos: None };
        let together = simulate_with(100.0, 400.0, &[u.clone(), u.clone()], 600.0, false);
        let staggered = simulate_with(100.0, 400.0, &[u.clone(), u.clone()], 600.0, true);
        assert!(together.lasts_s.is_none() && staggered.lasts_s.is_none(), "{together:?} {staggered:?}");
        assert!(staggered.stable_at.unwrap() > together.stable_at.unwrap() + 0.01, "{staggered:?} vs {together:?}");
    }

    #[test]
    fn a_trace_follows_the_run_and_hits_zero_when_dry() {
        let users = vec![User { cycle_ms: 1000.0, need: 50.0, give: 0.0, shots: None, reload_ms: 0.0, nos: None }];
        let pts = trace(1000.0, 200.0, &users, 120.0, 25);
        assert_eq!(pts.len(), 25);
        assert_eq!(pts[0][0], 0.0);
        assert!((pts[0][1] - 950.0).abs() < 1e-6, "{}: the first activation lands at time zero", pts[0][1]);
        assert!(pts[1][1] < 950.0 && pts[1][1] > 0.0);
        assert_eq!(pts.last().unwrap()[1], 0.0);
        let idle = trace(1000.0, 200.0, &[], 10.0, 3);
        assert!(idle.iter().all(|p| (p[1] - 1000.0).abs() < 1e-9));
    }

    #[test]
    fn empty_and_nothing_cases() {
        assert_eq!(simulate(0.0, 200.0, &[], 10.0).lasts_s, Some(0.0));
        assert_eq!(simulate(100.0, 200.0, &[], 10.0).stable_at, Some(1.0));
        assert_eq!(User { cycle_ms: 1000.0, need: 5.0, give: 0.0, shots: Some(0), reload_ms: 0.0, nos: None }.average_per_s(), 0.0);
    }

    #[test]
    fn a_nosferatu_drains_only_a_fuller_capacitor_unless_the_hull_overrides_it() {
        let drain = User { cycle_ms: 2000.0, need: 10.0, ..User::default() };
        let nos = |any| User { cycle_ms: 5000.0, give: 40.0, nos: Some(any), ..User::default() };
        let end = |users: &[User], o: Option<&Opponent>| trace_against(1000.0, 300.0, users, o, 120.0, 200).last().unwrap()[1];
        let alone = end(&[drain.clone()], None);
        assert!((end(&[drain.clone(), nos(false)], None) - alone).abs() < 1e-9, "an unknown target gives a plain nosferatu nothing");
        assert!(end(&[drain.clone(), nos(true)], None) > alone + 1.0, "one that drains any ship counts");
        let full = Opponent { capacity: 5000.0, tau_s: 300.0, users: &[], resist: 1.0 };
        assert!(end(&[drain.clone(), nos(false)], Some(&full)) > alone + 1.0, "a fuller target feeds it");
        let flat = [User { cycle_ms: 1000.0, need: 200.0, ..User::default() }];
        let dry = Opponent { capacity: 1000.0, tau_s: 300.0, users: &flat, resist: 1.0 };
        assert!((end(&[drain.clone(), nos(false)], Some(&dry)) - alone).abs() < 5.0, "an emptier target gives nothing");
        assert!(end(&[drain.clone(), nos(true)], Some(&dry)) > alone, "the override takes what little there is");
        let resistant = Opponent { resist: 0.0, ..full };
        assert!((end(&[drain, nos(false)], Some(&resistant)) - alone).abs() < 1e-9, "resistance cuts the drain");
    }
}
