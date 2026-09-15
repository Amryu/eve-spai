//! Range/fuel/fatigue match the live game mechanics (verified against the EVE University wiki
//! and the official Jump Activation Cooldown article):
//!   range  = base × (1 + 0.20 × JDC)
//!   fuel   = Σ ly × isotopes/ly × (1 − 0.10 × JFC) × (1 − role_fuel)
//!   d'     = ly × (1 − role_reduction)                 (black ops 0.75, JF/rorqual 0.90)
//!   fatigue(blue)    = max(prev, 10) × (1 + d'),  capped at 300 min (5 h)
//!   cooldown(red)    = max(prev_fatigue / 10, 1 + d'), capped at 30 min
//! Per-hull fuel is the standard class value (a specific Titan can differ).
//! Base range and fuel come from the live SDE `jumpDriveRange` / `jumpDriveConsumptionAmount`
//! attributes, NOT the Phoebe-era 2014 values: those were restored in later patches.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::map::{ly_distance, LY_METERS};
use crate::store::MapSystem;

#[derive(Clone, Copy)]
pub struct ShipClass {
    pub name: &'static str,
    pub base_ly: f64,
    pub fuel_per_ly: f64,
    /// Hull skill bonus on top of JFC (jump freighters 10%/level = 50% at V).
    pub fuel_role_reduction: f64,
    pub fatigue_role_reduction: f64,
}

// base_ly / fuel_per_ly are the hull attributes before skills; JDC V doubles the range, JFC V
// halves the fuel. Fatigue role bonus reduces effective distance: black ops 75%, jump freighters
// / rorquals 90%, other capitals none. Saved routes store the picker index, so a new class is
// APPENDED here, never sorted into range order.
pub const SHIP_CLASSES: &[ShipClass] = &[
    ShipClass { name: "Capital (Dread / Carrier / FAX)", base_ly: 3.5, fuel_per_ly: 3000.0, fuel_role_reduction: 0.0, fatigue_role_reduction: 0.0 },
    ShipClass { name: "Supercarrier / Titan", base_ly: 3.0, fuel_per_ly: 3000.0, fuel_role_reduction: 0.0, fatigue_role_reduction: 0.0 },
    ShipClass { name: "Black Ops", base_ly: 4.0, fuel_per_ly: 700.0, fuel_role_reduction: 0.0, fatigue_role_reduction: 0.75 },
    ShipClass { name: "Jump Freighter", base_ly: 5.0, fuel_per_ly: 9400.0, fuel_role_reduction: 0.5, fatigue_role_reduction: 0.9 },
    ShipClass { name: "Rorqual", base_ly: 5.0, fuel_per_ly: 4000.0, fuel_role_reduction: 0.0, fatigue_role_reduction: 0.9 },
    ShipClass { name: "Command Carrier", base_ly: 3.75, fuel_per_ly: 3000.0, fuel_role_reduction: 0.0, fatigue_role_reduction: 0.0 },
];

pub fn max_range_ly(class: &ShipClass, jdc: u32) -> f64 {
    class.base_ly * (1.0 + 0.20 * jdc as f64)
}

/// A system is a valid jump destination only in low or null sec (sec < 0.5, as EVE rounds it).
pub fn cyno_able(security: f64) -> bool {
    (security * 10.0).round() / 10.0 < 0.5
}

/// Whether a capital can light a cyno here at all.
///
/// Zarzakh is null sec and passes the security test, and nothing jumps into or out of it: no cynos,
/// and the only ways through are the drifter gates. A jump route through it is not a route.
pub fn jumpable(s: &MapSystem) -> bool {
    cyno_able(s.security) && !crate::geo::is_no_transit(s.id) && s.region_id != POCHVEN
}

/// Pochven. Null sec by security, and no capital jumps into or out of it: the only ways in are the
/// Triglavian gates, so a jump route through it is a route nobody can fly.
pub const POCHVEN: i64 = 10_000_070;

struct Grid {
    cell: f64,
    map: HashMap<(i64, i64, i64), Vec<usize>>,
}

impl Grid {
    fn key(x: f64, y: f64, z: f64, cell: f64) -> (i64, i64, i64) {
        ((x / cell).floor() as i64, (y / cell).floor() as i64, (z / cell).floor() as i64)
    }
    fn new(systems: &[MapSystem], cell: f64) -> Self {
        let mut map: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        for (i, s) in systems.iter().enumerate() {
            map.entry(Self::key(s.x, s.y, s.z, cell)).or_default().push(i);
        }
        Grid { cell, map }
    }
    fn near(&self, s: &MapSystem) -> Vec<usize> {
        let (kx, ky, kz) = Self::key(s.x, s.y, s.z, self.cell);
        let mut out = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.map.get(&(kx + dx, ky + dy, kz + dz)) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out
    }
}

pub fn shortest_path_pref(
    systems: &[MapSystem],
    max_ly: f64,
    from: i64,
    to: i64,
    prefer: &HashSet<i64>,
) -> Option<Vec<i64>> {
    let idx: HashMap<i64, usize> = systems.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let fi = *idx.get(&from)?;
    let ti = *idx.get(&to)?;
    if fi == ti {
        return Some(vec![from]);
    }
    let cell = (max_ly * LY_METERS).max(1.0);
    let grid = Grid::new(systems, cell);
    let max_m2 = (max_ly * LY_METERS).powi(2);
    let dist2 = |a: &MapSystem, b: &MapSystem| {
        (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
    };
    // Dijkstra on (jumps, systems you cannot dock in, light years), in that order.
    //
    // A plain breadth-first search minimises jumps and nothing else, so among the many paths of the
    // same length it returned whichever the queue reached first: on a dense map that is routinely
    // several light years and a lot of fuel worse than the best one. Jumps still come first, because
    // one fewer jump is worth any amount of distance; docking preference keeps the priority it had
    // over distance; light years decide what used to be arbitrary.
    let mut best: HashMap<usize, Key> = HashMap::new();
    let mut prev: HashMap<usize, usize> = HashMap::new();
    let mut heap: BinaryHeap<std::cmp::Reverse<(Key, usize)>> = BinaryHeap::new();
    let start = Key { jumps: 0, off_pref: 0, ly: 0.0 };
    best.insert(fi, start);
    heap.push(std::cmp::Reverse((start, fi)));
    while let Some(std::cmp::Reverse((k, cur))) = heap.pop() {
        if best.get(&cur).is_some_and(|b| *b < k) {
            continue;
        }
        if cur == ti {
            let mut path = vec![systems[ti].id];
            let mut c = ti;
            while let Some(&p) = prev.get(&c) {
                path.push(systems[p].id);
                c = p;
            }
            path.reverse();
            return Some(path);
        }
        let s = &systems[cur];
        for n in grid.near(s) {
            if n == cur {
                continue;
            }
            let t = &systems[n];
            if !jumpable(t) {
                continue;
            }
            let d2 = dist2(s, t);
            if d2 > max_m2 {
                continue;
            }
            let next = Key {
                jumps: k.jumps + 1,
                off_pref: k.off_pref + u32::from(!prefer.is_empty() && !prefer.contains(&t.id)),
                ly: k.ly + d2.sqrt() / LY_METERS,
            };
            if best.get(&n).is_some_and(|b| *b <= next) {
                continue;
            }
            best.insert(n, next);
            prev.insert(n, cur);
            heap.push(std::cmp::Reverse((next, n)));
        }
    }
    None
}

/// What a path has cost so far, ordered the way a jump route is judged.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    jumps: u32,
    off_pref: u32,
    ly: f64,
}

impl Eq for Key {}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        self.jumps
            .cmp(&other.jumps)
            .then(self.off_pref.cmp(&other.off_pref))
            // Distances are sums of square roots and never NaN here, so an unordered pair can only
            // come from a corrupt coordinate; calling it equal keeps the heap well-formed.
            .then(self.ly.partial_cmp(&other.ly).unwrap_or(Ordering::Equal))
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn alternatives(systems: &[MapSystem], max_ly: f64, a: i64, b: i64) -> Vec<i64> {
    let max_m2 = (max_ly * LY_METERS).powi(2);
    let find = |id: i64| systems.iter().find(|s| s.id == id);
    let (Some(sa), Some(sb)) = (find(a), find(b)) else { return Vec::new() };
    let d2 = |p: &MapSystem, q: &MapSystem| (p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2);
    systems
        .iter()
        .filter(|s| s.id != a && s.id != b && jumpable(s))
        .filter(|s| d2(s, sa) <= max_m2 && d2(s, sb) <= max_m2)
        .map(|s| s.id)
        .collect()
}

/// A whole route's totals, for the tests that pin the fatigue and fuel rules against the wiki figures.
#[cfg(test)]
pub struct RouteCost {
    pub jumps: usize,
    pub total_ly: f64,
    pub fuel: f64,
    pub final_fatigue_min: f64,
    pub total_delay_min: f64,
}

/// One jump's bill: how far, how much fuel, and the two timers it leaves behind.
#[derive(Clone, Copy, Default)]
pub struct HopCost {
    pub ly: f64,
    pub fuel: f64,
    /// The blue bar after this jump, in minutes.
    pub fatigue_min: f64,
    /// The red bar after this jump: how long before the drive can be used again.
    pub reactivation_min: f64,
}

/// Per jump, so the planner can show where the fatigue actually comes from.
///
/// The totals are folded out of this rather than computed a second time: two implementations of the
/// fatigue rules would be two chances to get them wrong, and the numbers are read side by side.
pub fn hop_costs(
    systems: &[MapSystem],
    path: &[i64],
    class: &ShipClass,
    jfc: u32,
) -> Vec<HopCost> {
    let idx: HashMap<i64, &MapSystem> = systems.iter().map(|s| (s.id, s)).collect();
    let fuel_mult = 1.0 - 0.10 * jfc as f64;
    let mut fatigue = 0.0_f64;
    let mut out = Vec::with_capacity(path.len().saturating_sub(1));
    for w in path.windows(2) {
        let (Some(a), Some(b)) = (idx.get(&w[0]), idx.get(&w[1])) else { continue };
        let ly = ly_distance(a, b);
        let d_eff = ly * (1.0 - class.fatigue_role_reduction);
        let reactivation = (fatigue / 10.0).max(1.0 + d_eff).min(30.0);
        fatigue = (fatigue.max(10.0) * (1.0 + d_eff)).min(300.0);
        out.push(HopCost {
            ly,
            fuel: ly * class.fuel_per_ly * fuel_mult * (1.0 - class.fuel_role_reduction),
            fatigue_min: fatigue,
            reactivation_min: reactivation,
        });
    }
    out
}

#[cfg(test)]
pub fn route_cost(systems: &[MapSystem], path: &[i64], class: &ShipClass, jfc: u32) -> RouteCost {
    let hops = hop_costs(systems, path, class, jfc);
    RouteCost {
        jumps: path.len().saturating_sub(1),
        total_ly: hops.iter().map(|h| h.ly).sum(),
        fuel: hops.iter().map(|h| h.fuel).sum(),
        final_fatigue_min: hops.last().map(|h| h.fatigue_min).unwrap_or_default(),
        total_delay_min: hops.iter().map(|h| h.reactivation_min).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sys(id: i64, x: f64, sec: f64) -> MapSystem {
        MapSystem { id, name: format!("S{id}"), security: sec, region_id: 0, x: x * LY_METERS, y: 0.0, z: 0.0, x2d: 0.0, z2d: 0.0 }
    }

    fn at(id: i64, x: f64, y: f64) -> MapSystem {
        MapSystem {
            id,
            name: format!("S{id}"),
            security: -0.4,
            region_id: 0,
            x: x * LY_METERS,
            y: y * LY_METERS,
            z: 0.0,
            x2d: 0.0,
            z2d: 0.0,
        }
    }

    /// Same number of jumps, so the shorter one wins.
    ///
    /// A breadth-first search minimises jumps and stops thinking: among equal-length paths it took
    /// whichever the queue reached first. `B` is listed before `A` here precisely so that the first
    /// one reached is the longer one, which is what a plain BFS returned.
    #[test]
    fn equal_jumps_go_the_short_way() {
        let s = vec![
            at(1, 0.0, 0.0),   // start
            at(2, 4.5, 3.0),   // B: two jumps, 10.8 ly
            at(3, 4.5, 0.0),   // A: two jumps, 9.0 ly
            at(4, 9.0, 0.0),   // target
        ];
        let path = shortest_path_pref(&s, 6.0, 1, 4, &HashSet::new()).unwrap();
        assert_eq!(path, vec![1, 3, 4], "the straight line, not the detour");
        let cost = route_cost(&s, &path, &SHIP_CLASSES[0], 5);
        assert!((cost.total_ly - 9.0).abs() < 0.01, "9 ly, not 10.8");
    }

    /// One fewer jump beats any amount of distance: fatigue is per jump, and a shorter route that
    /// takes an extra one is the wrong trade.
    #[test]
    fn fewer_jumps_beat_shorter_distance() {
        let s = vec![
            at(1, 0.0, 0.0),
            at(2, 3.0, 0.0),
            at(3, 6.0, 0.0),
        ];
        // 6 ly in one jump, or 3 + 3 in two. The one-jump answer is longer in neither, so widen the
        // direct hop to make the point: the two-hop path is strictly shorter in ly.
        let mut s2 = s.clone();
        s2[2] = at(3, 5.9, 1.5);
        let path = shortest_path_pref(&s2, 6.2, 1, 3, &HashSet::new()).unwrap();
        assert_eq!(path, vec![1, 3], "one jump, even though two would be a shorter flight");
    }

    /// Zarzakh is null sec, so the security test lets it through, and nothing jumps into or out of
    /// it. A route that went through it would be one nobody can fly.
    #[test]
    fn zarzakh_is_never_jumped_through() {
        let mut mid = at(crate::geo::ZARZAKH, 4.0, 0.0);
        mid.security = -0.9;
        let s = vec![at(1, 0.0, 0.0), mid, at(3, 8.0, 0.0)];
        assert!(jumpable(&s[0]), "an ordinary null system is fine");
        assert!(!jumpable(&s[1]), "Zarzakh is not");
        // 8 ly needs the middle hop, and the middle hop is Zarzakh, so there is no route at all.
        assert!(shortest_path_pref(&s, 5.0, 1, 3, &HashSet::new()).is_none());
        assert!(alternatives(&s, 5.0, 1, 3).is_empty());
    }

    /// Pochven is null sec by security and has no capital jumps: the only ways in are the Triglavian
    /// gates, so a jump route through it is one nobody can fly.
    #[test]
    fn pochven_is_never_jumped_through() {
        let mut mid = at(9, 4.0, 0.0);
        mid.region_id = POCHVEN;
        let s = vec![at(1, 0.0, 0.0), mid, at(3, 8.0, 0.0)];
        assert!(!jumpable(&s[1]));
        assert!(shortest_path_pref(&s, 5.0, 1, 3, &HashSet::new()).is_none());
    }

    /// Per-hop fatigue is what the totals are made of, so they cannot disagree.
    #[test]
    fn hop_costs_add_up_to_the_route_cost() {
        let s = vec![at(1, 0.0, 0.0), at(2, 4.0, 0.0), at(3, 8.0, 0.0), at(4, 12.0, 0.0)];
        let path = vec![1, 2, 3, 4];
        let hops = hop_costs(&s, &path, &SHIP_CLASSES[0], 5);
        let total = route_cost(&s, &path, &SHIP_CLASSES[0], 5);
        assert_eq!(hops.len(), 3);
        assert!((hops.iter().map(|h| h.ly).sum::<f64>() - total.total_ly).abs() < 1e-9);
        assert!((hops.iter().map(|h| h.fuel).sum::<f64>() - total.fuel).abs() < 1e-6);
        assert!((hops.last().unwrap().fatigue_min - total.final_fatigue_min).abs() < 1e-9);
        // Fatigue compounds, so each jump leaves more of it than the one before.
        assert!(hops[0].fatigue_min < hops[1].fatigue_min);
        assert!(hops[1].fatigue_min < hops[2].fatigue_min);
    }

    #[test]
    fn straight_line_route() {
        let s = vec![sys(1, 0.0, -0.4), sys(2, 4.0, -0.4), sys(3, 8.0, -0.4), sys(4, 12.0, -0.4)];
        let path = shortest_path_pref(&s, 5.0, 1, 4, &HashSet::new()).unwrap();
        assert_eq!(path, vec![1, 2, 3, 4]);
        let cost = route_cost(&s, &path, &SHIP_CLASSES[0], 5);
        assert_eq!(cost.jumps, 3);
        assert!((cost.total_ly - 12.0).abs() < 0.01);
    }

    #[test]
    fn mechanics_match_game() {
        let s = vec![sys(1, 0.0, -0.4), sys(2, 5.0, -0.4)];
        let c = route_cost(&s, &[1, 2], &SHIP_CLASSES[0], 5);
        assert!((c.final_fatigue_min - 60.0).abs() < 0.01, "fatigue {}", c.final_fatigue_min);
        assert!((c.total_delay_min - 6.0).abs() < 0.01, "delay {}", c.total_delay_min);
        assert!((c.fuel - 7500.0).abs() < 0.5, "fuel {}", c.fuel);

        let bo = route_cost(&s, &[1, 2], &SHIP_CLASSES[2], 5);
        assert!((bo.final_fatigue_min - 22.5).abs() < 0.01, "bo fatigue {}", bo.final_fatigue_min);
        assert!((bo.fuel - 1750.0).abs() < 0.5, "bo fuel {}", bo.fuel);

        let jf = route_cost(&s, &[1, 2], &SHIP_CLASSES[3], 5);
        assert!((jf.fuel - 11750.0).abs() < 0.5, "jf fuel {}", jf.fuel);
    }

    #[test]
    fn maxed_ranges_match_game() {
        let maxed: Vec<f64> = SHIP_CLASSES.iter().map(|c| max_range_ly(c, 5)).collect();
        assert_eq!(maxed, vec![7.0, 6.0, 8.0, 10.0, 10.0, 7.5]);
    }

    #[test]
    fn command_carrier_reaches_where_a_capital_cannot() {
        let cc = SHIP_CLASSES.iter().find(|c| c.name == "Command Carrier").unwrap();
        let cap = &SHIP_CLASSES[0];
        assert_eq!(cc.base_ly, 3.75);
        assert_eq!(max_range_ly(cc, 5), 7.5);
        assert_eq!(max_range_ly(cc, 0), 3.75);

        // 7.2 ly is inside a maxed command carrier and outside a maxed capital, so the class is
        // the difference between a route and no route, not just a different fuel number.
        let s = vec![sys(1, 0.0, -0.4), sys(2, 7.2, -0.4)];
        let cap_ly = max_range_ly(cap, 5);
        let cc_ly = max_range_ly(cc, 5);
        assert!(shortest_path_pref(&s, cap_ly, 1, 2, &HashSet::new()).is_none());
        assert_eq!(shortest_path_pref(&s, cc_ly, 1, 2, &HashSet::new()).unwrap(), vec![1, 2]);

        // Carrier fuel and no role bonus: same cost as a capital over the same distance.
        let path = vec![1, 2];
        let a = route_cost(&s, &path, cc, 5);
        let b = route_cost(&s, &path, cap, 5);
        assert!((a.fuel - b.fuel).abs() < 0.01);
        assert!((a.final_fatigue_min - b.final_fatigue_min).abs() < 0.01);
    }

    /// Saved routes store the picker index, so appending is the only safe way to add a class.
    #[test]
    fn existing_class_indices_are_stable() {
        assert_eq!(SHIP_CLASSES[0].name, "Capital (Dread / Carrier / FAX)");
        assert_eq!(SHIP_CLASSES[1].name, "Supercarrier / Titan");
        assert_eq!(SHIP_CLASSES[2].name, "Black Ops");
        assert_eq!(SHIP_CLASSES[3].name, "Jump Freighter");
        assert_eq!(SHIP_CLASSES[4].name, "Rorqual");
    }

    #[test]
    fn skips_when_in_range() {
        let s = vec![sys(1, 0.0, -0.4), sys(2, 3.0, -0.4), sys(3, 6.0, -0.4)];
        assert_eq!(shortest_path_pref(&s, 5.0, 1, 3, &HashSet::new()).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn hisec_destination_unreachable() {
        let s = vec![sys(1, 0.0, -0.4), sys(2, 3.0, 0.9)];
        assert!(shortest_path_pref(&s, 5.0, 1, 2, &HashSet::new()).is_none());
    }

    #[test]
    fn out_of_range_unreachable() {
        let s = vec![sys(1, 0.0, -0.4), sys(2, 9.0, -0.4)];
        assert!(shortest_path_pref(&s, 5.0, 1, 2, &HashSet::new()).is_none());
    }
}
