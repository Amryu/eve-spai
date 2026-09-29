//! Scan routes: where one or more scouts should fly to scan every system of interest around a
//! centre, nearest first and in as few jumps as can be managed.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::geo::{is_no_transit, Systems};

pub struct Scout {
    pub name: String,
    pub start: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoutPlan {
    pub name: String,
    /// The systems to scan, in order. One the scout starts in comes first.
    pub stops: Vec<i64>,
    /// Every system flown through, from the start to the last stop.
    pub path: Vec<i64>,
    pub jumps: u32,
    /// Stops from outside the area, taken in because the route passes close by.
    pub detours: Vec<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub scouts: Vec<ScoutPlan>,
    /// Targets no scout can reach within the search depth.
    pub unreached: Vec<i64>,
}

/// Reordering is cubic in a route's length; past this a route keeps the order it was built in.
const IMPROVE_MAX: usize = 120;
const IMPROVE_PASSES: usize = 20;

/// Shortest paths out of one system: jumps and the system each was reached from.
struct Tree {
    dist: HashMap<i64, u32>,
    parent: HashMap<i64, i64>,
}

impl Tree {
    fn grow(graph: &Systems, from: i64, depth: u32, allowed: &impl Fn(i64) -> bool) -> Tree {
        let mut dist = HashMap::from([(from, 0)]);
        let mut parent = HashMap::new();
        let mut queue = VecDeque::from([from]);
        while let Some(sys) = queue.pop_front() {
            let d = dist[&sys];
            if d >= depth || (sys != from && (is_no_transit(sys) || !allowed(sys))) {
                continue;
            }
            for &n in graph.neighbors(sys) {
                if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(n) {
                    e.insert(d + 1);
                    parent.insert(n, sys);
                    queue.push_back(n);
                }
            }
        }
        Tree { dist, parent }
    }

    /// The systems after the root up to `to`, or `None` when it is out of reach.
    fn path_to(&self, to: i64) -> Option<Vec<i64>> {
        self.dist.get(&to)?;
        let mut out = vec![to];
        let mut at = to;
        while let Some(&p) = self.parent.get(&at) {
            out.push(p);
            at = p;
        }
        out.pop();
        out.reverse();
        Some(out)
    }
}

/// Routes for `scouts` over `targets`, within `depth` jumps of each other. `centre` breaks ties
/// towards the nearer system, so scanning works outwards. Systems where `allowed` is false are
/// never flown through. `extras` lie outside the area: one joins a route when the route comes
/// within `detour` jumps of it and taking it in costs no more than there and back.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    graph: &Systems,
    centre: i64,
    targets: &HashSet<i64>,
    extras: &HashSet<i64>,
    detour: u32,
    scouts: &[Scout],
    depth: u32,
    allowed: impl Fn(i64) -> bool,
) -> Plan {
    let mut trees: HashMap<i64, Tree> = HashMap::new();
    let centre_dist = Tree::grow(graph, centre, depth.saturating_mul(2), &allowed).dist;
    let mut pending: HashSet<i64> = targets.clone();
    let mut routes: Vec<ScoutPlan> = scouts
        .iter()
        .map(|s| ScoutPlan { name: s.name.clone(), path: vec![s.start], ..Default::default() })
        .collect();
    for r in &mut routes {
        let start = r.path[0];
        if pending.remove(&start) {
            r.stops.push(start);
        }
    }
    let mut stuck: HashSet<usize> = HashSet::new();
    loop {
        if pending.is_empty() {
            break;
        }
        // The scout with the least flying so far takes its nearest target next.
        let Some(k) = (0..routes.len()).filter(|k| !stuck.contains(k)).min_by_key(|k| (routes[*k].jumps, *k)) else { break };
        let at = *routes[k].path.last().unwrap();
        let t = trees.entry(at).or_insert_with(|| Tree::grow(graph, at, depth, &allowed));
        let next = pending
            .iter()
            .filter_map(|p| Some((*p, *t.dist.get(p)?)))
            .min_by_key(|(p, d)| (*d, centre_dist.get(p).copied().unwrap_or(u32::MAX), *p));
        let Some((to, _)) = next else {
            stuck.insert(k);
            continue;
        };
        let leg = t.path_to(to).unwrap_or_default();
        let r = &mut routes[k];
        for sys in leg {
            r.path.push(sys);
            r.jumps += 1;
            // Flown through on the way: scanned there and then, not come back for.
            if pending.remove(&sys) {
                r.stops.push(sys);
            }
        }
    }
    for r in &mut routes {
        improve(graph, r, depth, &allowed, &mut trees);
    }
    if detour > 0 {
        take_detours(graph, &mut routes, extras, detour, depth, &allowed, &mut trees);
    }
    let mut unreached: Vec<i64> = pending.into_iter().collect();
    unreached.sort_unstable();
    Plan { scouts: routes, unreached }
}

/// Reorders a route's stops to reach them sooner on the whole (the sum of the jumps at which each
/// is reached), then lays the path again through the new order.
fn improve(graph: &Systems, r: &mut ScoutPlan, depth: u32, allowed: &impl Fn(i64) -> bool, trees: &mut HashMap<i64, Tree>) {
    let start = r.path[0];
    // The start stays first; a stop in the start system costs nothing and stays with it.
    let fixed = r.stops.first() == Some(&start);
    let mut order: Vec<i64> = r.stops.iter().copied().filter(|s| *s != start).collect();
    if order.len() >= 3 && order.len() <= IMPROVE_MAX {
        let mut nodes = vec![start];
        nodes.extend(&order);
        for n in &nodes {
            trees.entry(*n).or_insert_with(|| Tree::grow(graph, *n, depth, allowed));
        }
        let d = |a: i64, b: i64| trees[&a].dist.get(&b).copied().unwrap_or(depth * 4);
        let cost = |o: &[i64]| -> u64 {
            let (mut at, mut t, mut sum) = (start, 0u64, 0u64);
            for s in o {
                t += d(at, *s) as u64;
                sum += t;
                at = *s;
            }
            sum
        };
        let mut best = cost(&order);
        for _ in 0..IMPROVE_PASSES {
            let mut better = false;
            // 2-opt: a stretch flown the other way round.
            for i in 0..order.len() {
                for j in i + 1..order.len() {
                    order[i..=j].reverse();
                    let c = cost(&order);
                    if c < best {
                        best = c;
                        better = true;
                    } else {
                        order[i..=j].reverse();
                    }
                }
            }
            // Or-opt: one stop moved elsewhere.
            for i in 0..order.len() {
                for j in 0..order.len() {
                    if i == j {
                        continue;
                    }
                    let s = order.remove(i);
                    order.insert(j, s);
                    let c = cost(&order);
                    if c < best {
                        best = c;
                        better = true;
                    } else {
                        let s = order.remove(j);
                        order.insert(i, s);
                    }
                }
            }
            if !better {
                break;
            }
        }
    }
    relay(graph, r, order, fixed, depth, allowed, trees, &mut HashSet::new());
}

/// Lays `r`'s path again through `order`, the start first and kept as a stop when `fixed`.
/// Targets flown past on the way count where they are passed, and so does anything in `bonus`,
/// which is taken out of it and kept as a detour.
#[allow(clippy::too_many_arguments)]
fn relay(
    graph: &Systems,
    r: &mut ScoutPlan,
    order: Vec<i64>,
    fixed: bool,
    depth: u32,
    allowed: &impl Fn(i64) -> bool,
    trees: &mut HashMap<i64, Tree>,
    bonus: &mut HashSet<i64>,
) {
    let start = r.path[0];
    let wanted: HashSet<i64> = order.iter().copied().collect();
    let mut stops = if fixed { vec![start] } else { Vec::new() };
    let mut path = vec![start];
    let mut done: HashSet<i64> = HashSet::new();
    let mut at = start;
    for s in order {
        if done.contains(&s) {
            continue;
        }
        let t = trees.entry(at).or_insert_with(|| Tree::grow(graph, at, depth, allowed));
        let Some(leg) = t.path_to(s) else { continue };
        for sys in leg {
            path.push(sys);
            if wanted.contains(&sys) && done.insert(sys) {
                stops.push(sys);
            } else if bonus.remove(&sys) {
                done.insert(sys);
                stops.push(sys);
                r.detours.push(sys);
            }
        }
        at = s;
    }
    r.jumps = path.len() as u32 - 1;
    r.stops = stops;
    r.path = path;
}

/// Takes in targets outside the area, the cheapest first: each goes where it adds the fewest
/// jumps to any route that comes within `detour` jumps of it, while that is at most there and
/// back.
fn take_detours(
    graph: &Systems,
    routes: &mut [ScoutPlan],
    extras: &HashSet<i64>,
    detour: u32,
    depth: u32,
    allowed: &impl Fn(i64) -> bool,
    trees: &mut HashMap<i64, Tree>,
) {
    let mut left: HashSet<i64> = extras.clone();
    // Anything a route already flies through costs nothing.
    for r in routes.iter_mut() {
        let start = r.path[0];
        let fixed = r.stops.first() == Some(&start);
        let order: Vec<i64> = r.stops.iter().copied().filter(|s| *s != start).collect();
        relay(graph, r, order, fixed, depth, allowed, trees, &mut left);
    }
    let far = depth * 4;
    loop {
        let mut cands: Vec<i64> = left.iter().copied().collect();
        cands.sort_unstable();
        // (extra jumps, route, position in its order, target)
        let mut best: Option<(u32, usize, usize, i64)> = None;
        for &e in &cands {
            let near: Vec<i64> = {
                let te = trees.entry(e).or_insert_with(|| Tree::grow(graph, e, depth, allowed));
                routes.iter().flat_map(|r| r.path.iter().copied()).filter(|p| te.dist.get(p).is_some_and(|d| *d <= detour)).collect()
            };
            if near.is_empty() {
                continue;
            }
            for (k, r) in routes.iter().enumerate() {
                if !r.path.iter().any(|p| near.contains(p)) {
                    continue;
                }
                let start = r.path[0];
                let mut nodes = vec![start];
                nodes.extend(r.stops.iter().copied().filter(|s| *s != start));
                for n in nodes.iter().chain([&e]) {
                    trees.entry(*n).or_insert_with(|| Tree::grow(graph, *n, depth, allowed));
                }
                let d = |a: i64, b: i64| trees[&a].dist.get(&b).copied().unwrap_or(far);
                for pos in 0..nodes.len() {
                    let a = nodes[pos];
                    let cost = match nodes.get(pos + 1) {
                        Some(&b) => (d(a, e) + d(e, b)).saturating_sub(d(a, b)),
                        None => d(a, e),
                    };
                    if best.is_none_or(|(c, ..)| cost < c) {
                        best = Some((cost, k, pos, e));
                    }
                }
            }
        }
        let Some((_, k, pos, e)) = best.filter(|(c, ..)| *c <= 2 * detour) else { break };
        left.remove(&e);
        let r = &mut routes[k];
        let start = r.path[0];
        let fixed = r.stops.first() == Some(&start);
        let mut order: Vec<i64> = r.stops.iter().copied().filter(|s| *s != start).collect();
        order.insert(pos, e);
        r.detours.push(e);
        relay(graph, r, order, fixed, depth, allowed, trees, &mut left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::SystemInfo;

    /// A `w` by `h` grid of gate-linked systems, id `x * 100 + y + 1`.
    fn grid(w: i64, h: i64) -> Systems {
        let id = |x: i64, y: i64| x * 100 + y + 1;
        let mut by_name = HashMap::new();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for x in 0..w {
            for y in 0..h {
                let name = format!("G{x}-{y}");
                by_name.insert(
                    name.to_lowercase(),
                    SystemInfo { id: id(x, y), name, security: -0.5, constellation: String::new(), region: "Grid".into(), faction: String::new() },
                );
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if (0..w).contains(&nx) && (0..h).contains(&ny) {
                        adj.entry(id(x, y)).or_default().push(id(nx, ny));
                    }
                }
            }
        }
        Systems::new(by_name, adj)
    }

    fn all(w: i64, h: i64) -> HashSet<i64> {
        (0..w).flat_map(|x| (0..h).map(move |y| x * 100 + y + 1)).collect()
    }

    fn scout(name: &str, start: i64) -> Scout {
        Scout { name: name.into(), start }
    }

    fn walks(g: &Systems, p: &ScoutPlan) -> bool {
        p.path.windows(2).all(|s| g.neighbors(s[0]).contains(&s[1]))
    }

    #[test]
    fn every_target_is_scanned_along_a_real_path() {
        let g = grid(5, 5);
        let targets = all(5, 5);
        let p = plan(&g, 1, &targets, &HashSet::new(), 0, &[scout("A", 1)], 20, |_| true);
        assert!(p.unreached.is_empty());
        let got: HashSet<i64> = p.scouts[0].stops.iter().copied().collect();
        assert_eq!(got, targets);
        assert_eq!(p.scouts[0].stops.len(), targets.len(), "no stop twice");
        assert!(walks(&g, &p.scouts[0]));
        assert_eq!(p.scouts[0].jumps as usize, p.scouts[0].path.len() - 1);
        assert!(p.scouts[0].jumps <= 30, "a snake through 25 systems is 24 jumps: {}", p.scouts[0].jumps);
    }

    #[test]
    fn two_scouts_split_the_work_and_finish_sooner() {
        let g = grid(6, 6);
        let targets = all(6, 6);
        let one = plan(&g, 1, &targets, &HashSet::new(), 0, &[scout("A", 1)], 30, |_| true);
        let two = plan(&g, 1, &targets, &HashSet::new(), 0, &[scout("A", 1), scout("B", 506)], 30, |_| true);
        let (a, b): (HashSet<i64>, HashSet<i64>) =
            (two.scouts[0].stops.iter().copied().collect(), two.scouts[1].stops.iter().copied().collect());
        assert!(a.is_disjoint(&b), "a system scanned twice");
        assert_eq!(a.len() + b.len(), targets.len());
        let longest = two.scouts.iter().map(|s| s.jumps).max().unwrap();
        assert!(longest < one.scouts[0].jumps, "{longest} vs {}", one.scouts[0].jumps);
    }

    #[test]
    fn near_systems_come_first() {
        let g = grid(7, 1);
        // From the middle of a line: the three on either side, the near ones early.
        let targets: HashSet<i64> = (0..7).map(|x| x * 100 + 1).collect();
        let p = plan(&g, 301, &targets, &HashSet::new(), 0, &[scout("A", 301)], 20, |_| true);
        let s = &p.scouts[0].stops;
        assert_eq!(s[0], 301, "where it starts");
        let first_two: HashSet<i64> = s[1..3].iter().copied().collect();
        assert!(first_two.contains(&201) || first_two.contains(&401), "{s:?}");
    }

    #[test]
    fn a_system_flown_through_is_not_flown_back_to() {
        let g = grid(5, 1);
        let targets: HashSet<i64> = [101, 201, 301, 401].into();
        let p = plan(&g, 1, &targets, &HashSet::new(), 0, &[scout("A", 1)], 20, |_| true);
        assert_eq!(p.scouts[0].stops, vec![101, 201, 301, 401]);
        assert_eq!(p.scouts[0].jumps, 4);
    }

    #[test]
    fn avoided_systems_are_never_flown_through() {
        let g = grid(3, 3);
        // The middle column's top two are avoided: the way round is by the bottom row.
        let targets: HashSet<i64> = [201].into();
        let p = plan(&g, 1, &targets, &HashSet::new(), 0, &[scout("A", 1)], 20, |s| s != 101 && s != 102);
        assert!(!p.scouts[0].path.iter().any(|s| *s == 101 || *s == 102), "{:?}", p.scouts[0].path);
        assert_eq!(p.scouts[0].stops, vec![201]);
    }

    #[test]
    fn a_target_just_outside_is_taken_in_when_the_route_passes_close() {
        // Along the bottom row of a 5 by 3 grid; 202 is one jump off it, 203 two.
        let g = grid(5, 3);
        let core: HashSet<i64> = [101, 201, 301, 401].into();
        let off = |extras: &[i64], detour: u32| {
            let p = plan(&g, 1, &core, &extras.iter().copied().collect(), detour, &[scout("A", 1)], 20, |_| true);
            let r = p.scouts[0].clone();
            assert!(walks(&g, &r));
            r
        };
        let r = off(&[202], 1);
        assert!(r.stops.contains(&202) && r.detours == vec![202], "{r:?}");
        assert_eq!(r.jumps, 6, "four along, one out and back");
        assert!(!off(&[203], 1).stops.contains(&203), "two jumps off is past a detour of one");
        let r = off(&[203], 2);
        assert!(r.stops.contains(&203), "{r:?}");
        assert!(off(&[202], 0).detours.is_empty(), "no detours asked for");
    }

    #[test]
    fn a_target_outside_that_the_route_flies_through_costs_nothing() {
        let g = grid(5, 1);
        let p = plan(&g, 1, &[401].into(), &[201].into(), 1, &[scout("A", 1)], 20, |_| true);
        assert_eq!(p.scouts[0].stops, vec![201, 401]);
        assert_eq!(p.scouts[0].detours, vec![201]);
        assert_eq!(p.scouts[0].jumps, 4);
    }

    #[test]
    #[ignore]
    fn bench_a_big_scan() {
        let g = grid(20, 20);
        let targets = all(20, 20);
        let at = std::time::Instant::now();
        let p = plan(&g, 1010, &targets, &HashSet::new(), 0, &[scout("A", 1), scout("B", 1901), scout("C", 1020)], 24, |_| true);
        let jumps: Vec<u32> = p.scouts.iter().map(|s| s.jumps).collect();
        println!("400 systems, 3 scouts: {:?}, jumps {jumps:?}", at.elapsed());
        assert!(p.unreached.is_empty());
    }

    #[test]
    fn nothing_to_scan_plans_nothing_and_out_of_reach_is_reported() {
        let g = grid(4, 1);
        let p = plan(&g, 1, &HashSet::new(), &HashSet::new(), 0, &[scout("A", 1)], 10, |_| true);
        assert!(p.scouts[0].stops.is_empty() && p.scouts[0].jumps == 0);
        let p = plan(&g, 1, &[301].into(), &HashSet::new(), 0, &[scout("A", 1)], 2, |_| true);
        assert_eq!(p.unreached, vec![301]);
    }
}
