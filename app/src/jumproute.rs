//! Range/fuel/fatigue match the live game mechanics (verified against the EVE University wiki
//! and the official Jump Activation Cooldown article):
//!   range  = base × (1 + 0.20 × JDC)
//!   fuel   = Σ ly × isotopes/ly × (1 − 0.10 × JFC)     (caps 1000, JF 3100, black ops 400)
//!   d'     = ly × (1 − role_reduction)                 (black ops 0.75, JF/rorqual 0.90)
//!   fatigue(blue)    = max(prev, 10) × (1 + d'),  capped at 300 min (5 h)
//!   cooldown(red)    = max(prev_fatigue / 10, 1 + d'), capped at 30 min
//! Per-hull fuel is the standard class value (a specific Titan can differ).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::map::{ly_distance, LY_METERS};
use crate::store::MapSystem;

#[derive(Clone, Copy)]
pub struct ShipClass {
    pub name: &'static str,
    pub base_ly: f64,
    pub fuel_per_ly: f64,
    pub fatigue_role_reduction: f64,
}

// Fuel = base isotopes/ly (capitals 1000, jump freighters 3100, black ops 400); range base is
// half the JDC-V max (×2 at level V). Fatigue role bonus reduces effective distance: black ops
// 75%, jump freighters / rorquals 90%, other capitals none.
pub const SHIP_CLASSES: &[ShipClass] = &[
    ShipClass { name: "Capital (Dread / Carrier / FAX / Rorqual)", base_ly: 2.5, fuel_per_ly: 1000.0, fatigue_role_reduction: 0.0 },
    ShipClass { name: "Supercarrier / Titan", base_ly: 1.75, fuel_per_ly: 1000.0, fatigue_role_reduction: 0.0 },
    ShipClass { name: "Black Ops", base_ly: 4.0, fuel_per_ly: 400.0, fatigue_role_reduction: 0.75 },
    ShipClass { name: "Jump Freighter", base_ly: 5.0, fuel_per_ly: 3100.0, fatigue_role_reduction: 0.9 },
];

pub fn max_range_ly(class: &ShipClass, jdc: u32) -> f64 {
    class.base_ly * (1.0 + 0.20 * jdc as f64)
}

/// A system is a valid jump destination only in low or null sec (sec < 0.5, as EVE rounds it).
pub fn cyno_able(security: f64) -> bool {
    (security * 10.0).round() / 10.0 < 0.5
}

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
    let mut prev: HashMap<usize, usize> = HashMap::new();
    let mut seen: HashSet<usize> = HashSet::new();
    let mut q = VecDeque::new();
    q.push_back(fi);
    seen.insert(fi);
    while let Some(cur) = q.pop_front() {
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
        let mut neighbours: Vec<usize> = grid
            .near(s)
            .into_iter()
            .filter(|&n| n != cur && !seen.contains(&n))
            .filter(|&n| cyno_able(systems[n].security) && dist2(s, &systems[n]) <= max_m2)
            .collect();
        // Expand favourited systems first so they win equal-length ties.
        if !prefer.is_empty() {
            neighbours.sort_by_key(|&n| !prefer.contains(&systems[n].id));
        }
        for n in neighbours {
            if seen.insert(n) {
                prev.insert(n, cur);
                q.push_back(n);
            }
        }
    }
    None
}

#[derive(Clone)]
pub struct Leg {
    pub from: i64,
    pub to: i64,
    pub path: Vec<i64>,
    pub valid: bool,
}

pub fn plan(systems: &[MapSystem], max_ly: f64, anchors: &[i64], prefer: &HashSet<i64>) -> Vec<Leg> {
    let mut legs = Vec::new();
    for w in anchors.windows(2) {
        let (a, b) = (w[0], w[1]);
        match shortest_path_pref(systems, max_ly, a, b, prefer) {
            Some(path) => legs.push(Leg { from: a, to: b, path, valid: true }),
            None => legs.push(Leg { from: a, to: b, path: vec![a, b], valid: false }),
        }
    }
    legs
}

pub fn flatten(legs: &[Leg]) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for leg in legs {
        for &s in &leg.path {
            if out.last() != Some(&s) {
                out.push(s);
            }
        }
    }
    out
}

pub fn alternatives(systems: &[MapSystem], max_ly: f64, a: i64, b: i64) -> Vec<i64> {
    let max_m2 = (max_ly * LY_METERS).powi(2);
    let find = |id: i64| systems.iter().find(|s| s.id == id);
    let (Some(sa), Some(sb)) = (find(a), find(b)) else { return Vec::new() };
    let d2 = |p: &MapSystem, q: &MapSystem| (p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2);
    systems
        .iter()
        .filter(|s| s.id != a && s.id != b && cyno_able(s.security))
        .filter(|s| d2(s, sa) <= max_m2 && d2(s, sb) <= max_m2)
        .map(|s| s.id)
        .collect()
}

pub struct RouteCost {
    pub jumps: usize,
    pub total_ly: f64,
    pub fuel: f64,
    pub final_fatigue_min: f64,
    pub total_delay_min: f64,
}

pub fn route_cost(systems: &[MapSystem], path: &[i64], class: &ShipClass, jfc: u32) -> RouteCost {
    let idx: HashMap<i64, &MapSystem> = systems.iter().map(|s| (s.id, s)).collect();
    let fuel_mult = 1.0 - 0.10 * jfc as f64;
    let mut total_ly = 0.0;
    let mut fuel = 0.0;
    let mut fatigue = 0.0_f64;
    let mut total_delay = 0.0_f64;
    for w in path.windows(2) {
        let (Some(a), Some(b)) = (idx.get(&w[0]), idx.get(&w[1])) else { continue };
        let ly = ly_distance(a, b);
        total_ly += ly;
        fuel += ly * class.fuel_per_ly * fuel_mult;
        let d_eff = ly * (1.0 - class.fatigue_role_reduction);
        total_delay += (fatigue / 10.0).max(1.0 + d_eff).min(30.0);
        fatigue = fatigue.max(10.0) * (1.0 + d_eff);
        fatigue = fatigue.min(300.0);
    }
    RouteCost {
        jumps: path.len().saturating_sub(1),
        total_ly,
        fuel,
        final_fatigue_min: fatigue,
        total_delay_min: total_delay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sys(id: i64, x: f64, sec: f64) -> MapSystem {
        MapSystem { id, name: format!("S{id}"), security: sec, region_id: 0, x: x * LY_METERS, y: 0.0, z: 0.0, x2d: 0.0, z2d: 0.0 }
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
        assert!((c.fuel - 2500.0).abs() < 0.5, "fuel {}", c.fuel);

        let bo = route_cost(&s, &[1, 2], &SHIP_CLASSES[2], 5);
        assert!((bo.final_fatigue_min - 22.5).abs() < 0.01, "bo fatigue {}", bo.final_fatigue_min);
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
