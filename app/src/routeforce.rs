//! Waypoints that make the client plan the route the app planned.
//!
//! The client's route planner knows nothing of Ansiblex zones or of a gate's capacitor cost: it
//! takes the fewest jumps over every bridge the character can use. A route that avoids the costly
//! ones therefore has to be pinned with waypoints, which is what the planner routes between,
//! whether or not the player flies it on autopilot. Each waypoint is a system the player has to
//! pass through, so this uses as few as it can: a leg is left to the client whenever every route
//! it could pick for that leg is one the app would have planned itself.

use crate::geo::Systems;
use std::collections::HashMap;

/// How far apart two waypoints may sit before the search gives up on the leg.
const MAX_LEG: u32 = 120;

/// The waypoints to set, in order, ending at the destination. `path` is the planned route with its
/// start system first. `ok_edge` says whether a step is one the plan allows.
pub fn waypoints(path: &[i64], game: &Systems, ok_edge: &dyn Fn(i64, i64) -> bool) -> Vec<i64> {
    let Some(&last) = path.last() else { return Vec::new() };
    if path.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < path.len() - 1 {
        // The furthest system the client can be trusted to reach on its own. `at + 1` always
        // qualifies: one step of the plan is a step the plan allows.
        let mut reach = at + 1;
        while reach + 1 < path.len() && leg_is_safe(game, path[at], path[reach + 1], ok_edge) {
            reach += 1;
        }
        out.push(path[reach]);
        at = reach;
    }
    if out.last() != Some(&last) {
        out.push(last);
    }
    out
}

/// True when no route the client could pick from `a` to `b` uses a step the plan disallows.
///
/// The client picks *a* shortest route, and ties are its own business, so every shortest route has
/// to be acceptable. A step `u -> v` lies on one exactly when `dist(a, u) + 1 + dist(v, b)` is the
/// distance from `a` to `b`, which is what this walks.
fn leg_is_safe(game: &Systems, a: i64, b: i64, ok_edge: &dyn Fn(i64, i64) -> bool) -> bool {
    let from_a = game.distances_from(a, MAX_LEG);
    let Some(&total) = from_a.get(&b) else { return false };
    let to_b = game.distances_to(b, true, true, &HashMap::new(), |_| true);
    for (u, du) in &from_a {
        if du + 1 > total {
            continue;
        }
        for v in game.neighbors(*u) {
            let Some(dv) = to_b.get(v) else { continue };
            if du + 1 + dv == total && !ok_edge(*u, *v) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::SystemInfo;

    /// A ring of `n` systems, gated in sequence, plus whatever bridges are laid over it.
    fn ring(n: i64, bridges: &[(i64, i64)]) -> Systems {
        let by_name = (0..n)
            .map(|i| {
                let info = SystemInfo {
                    id: i,
                    name: format!("s{i}"),
                    security: -0.5,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                };
                (info.name.clone(), info)
            })
            .collect();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        for i in 0..n - 1 {
            adjacency.entry(i).or_default().push(i + 1);
            adjacency.entry(i + 1).or_default().push(i);
        }
        let mut s = Systems::new(by_name, adjacency);
        s.add_bridges(bridges);
        s
    }

    #[test]
    fn a_plain_route_needs_only_the_destination() {
        let game = ring(6, &[]);
        let path: Vec<i64> = (0..6).collect();
        assert_eq!(waypoints(&path, &game, &|_, _| true), vec![5]);
    }

    #[test]
    fn a_forbidden_shortcut_is_pinned_past() {
        // The client would take the bridge 1 -> 4; the plan gates around it.
        let game = ring(6, &[(1, 4)]);
        let path: Vec<i64> = (0..6).collect();
        let ok = |a: i64, b: i64| !((a, b) == (1, 4) || (a, b) == (4, 1));
        let wp = waypoints(&path, &game, &ok);
        assert_eq!(wp, vec![2, 3, 5], "pinned past the bridge, then left to the client");
        // Each leg the client is trusted with is one it cannot spoil.
        for w in [(0, 2), (2, 3), (3, 5)] {
            assert!(leg_is_safe(&game, w.0, w.1, &ok), "leg {w:?} is not safe after all");
        }
    }

    #[test]
    fn a_permitted_bridge_is_left_to_the_client() {
        let game = ring(6, &[(1, 4)]);
        // The plan takes the bridge too, so the client cannot get it wrong.
        let path = vec![0, 1, 4, 5];
        assert_eq!(waypoints(&path, &game, &|_, _| true), vec![5]);
    }

    #[test]
    fn a_tie_the_client_could_lose_is_pinned() {
        // Two equal-length routes from 0 to 4: the gates, or the bridge 0 -> 3.
        let mut game = ring(5, &[]);
        game.add_bridges(&[(0, 3)]);
        let path = vec![0, 1, 2, 3, 4];
        let ok = |a: i64, b: i64| !((a, b) == (0, 3) || (a, b) == (3, 0));
        let wp = waypoints(&path, &game, &ok);
        assert!(wp.len() > 1, "a leg with a forbidden tie must be split: {wp:?}");
        assert_eq!(wp.last(), Some(&4));
    }

    #[test]
    fn an_empty_or_single_system_path_needs_nothing() {
        let game = ring(3, &[]);
        assert!(waypoints(&[], &game, &|_, _| true).is_empty());
        assert!(waypoints(&[1], &game, &|_, _| true).is_empty());
    }
}
