//! Solar-system geography: name lookup + jump-distance over the SDE graph.
//!
//! Used by the intel parser (system detection, movement distance) and — later —
//! by the battle-report clustering ("within N jumps"). Jump bridges will extend
//! the adjacency once that feature lands (docs/DESIGN.md §7.2 A1).

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug)]
pub struct SystemInfo {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub constellation: String,
    pub region: String,
    /// NPC faction (sovereignty / rats), empty for unclaimed systems.
    pub faction: String,
}

pub struct Systems {
    by_name: HashMap<String, SystemInfo>,
    by_id: HashMap<i64, SystemInfo>,
    /// Gate + jump-bridge edges.
    adjacency: HashMap<i64, Vec<i64>>,
    /// Gate-only edges (no jump bridges) — for distances enemies can actually travel.
    gate_adjacency: HashMap<i64, Vec<i64>>,
}

impl Systems {
    pub fn new(by_name: HashMap<String, SystemInfo>, adjacency: HashMap<i64, Vec<i64>>) -> Self {
        let by_id = by_name.values().map(|s| (s.id, s.clone())).collect();
        let gate_adjacency = adjacency.clone();
        Self {
            by_name,
            by_id,
            adjacency,
            gate_adjacency,
        }
    }

    /// Look up a system by id.
    pub fn info_of(&self, id: i64) -> Option<&SystemInfo> {
        self.by_id.get(&id)
    }

    /// Adjacent systems (gate + bridge neighbours).
    pub fn neighbors(&self, id: i64) -> &[i64] {
        self.adjacency.get(&id).map_or(&[], |v| v.as_slice())
    }

    /// Whether the edge a↔b is a jump bridge (an adjacency that isn't a gate).
    pub fn is_bridge(&self, a: i64, b: i64) -> bool {
        self.adjacency.get(&a).is_some_and(|v| v.contains(&b))
            && !self.gate_adjacency.get(&a).is_some_and(|v| v.contains(&b))
    }

    /// Add bidirectional jump-bridge edges (configured by the user) so distance and
    /// battle clustering can travel them like gates.
    pub fn add_bridges(&mut self, pairs: &[(i64, i64)]) {
        for &(a, b) in pairs {
            // Don't duplicate an edge that already exists as a gate (or another
            // bridge) — that would list the neighbour twice.
            let av = self.adjacency.entry(a).or_default();
            if !av.contains(&b) {
                av.push(b);
            }
            let bv = self.adjacency.entry(b).or_default();
            if !bv.contains(&a) {
                bv.push(a);
            }
        }
    }

    /// Look up a system by (case-insensitive) name token.
    pub fn lookup(&self, token: &str) -> Option<&SystemInfo> {
        self.by_name.get(&token.to_lowercase())
    }

    /// Resolve an abbreviated null-sec code (e.g. "78-", "C-J") to a system, but
    /// only when the prefix is unambiguous.
    pub fn lookup_prefix(&self, token: &str) -> Option<&SystemInfo> {
        let t = token.to_lowercase();
        let mut found: Option<&SystemInfo> = None;
        for (name, info) in &self.by_name {
            if name.starts_with(&t) {
                if found.is_some() {
                    return None; // ambiguous
                }
                found = Some(info);
            }
        }
        found
    }

    /// Shortest jump distance between two systems (0 if equal), or None if
    /// unreachable within `max_jumps`. Includes jump bridges.
    pub fn jumps(&self, from: i64, to: i64, max_jumps: u32) -> Option<u32> {
        Self::bfs_jumps(&self.adjacency, from, to, max_jumps)
    }

    /// As [`Self::jumps`], but over gate edges only (ignoring jump bridges) — the
    /// distance a hostile (who can't use your bridges) would actually have to travel.
    pub fn jumps_gates_only(&self, from: i64, to: i64, max_jumps: u32) -> Option<u32> {
        Self::bfs_jumps(&self.gate_adjacency, from, to, max_jumps)
    }

    fn bfs_jumps(adj: &HashMap<i64, Vec<i64>>, from: i64, to: i64, max_jumps: u32) -> Option<u32> {
        if from == to {
            return Some(0);
        }
        let mut visited: HashSet<i64> = HashSet::from([from]);
        let mut queue: VecDeque<(i64, u32)> = VecDeque::from([(from, 0)]);
        while let Some((sys, dist)) = queue.pop_front() {
            if dist >= max_jumps {
                continue;
            }
            for &n in adj.get(&sys).into_iter().flatten() {
                if n == to {
                    return Some(dist + 1);
                }
                if visited.insert(n) {
                    queue.push_back((n, dist + 1));
                }
            }
        }
        None
    }

    /// Shortest gate path from `from` to `to` (inclusive of both), via BFS.
    pub fn path(&self, from: i64, to: i64) -> Option<Vec<i64>> {
        if from == to {
            return Some(vec![from]);
        }
        let mut prev: HashMap<i64, i64> = HashMap::new();
        let mut visited: HashSet<i64> = HashSet::from([from]);
        let mut queue: VecDeque<i64> = VecDeque::from([from]);
        while let Some(sys) = queue.pop_front() {
            for &n in self.adjacency.get(&sys).into_iter().flatten() {
                if visited.insert(n) {
                    prev.insert(n, sys);
                    if n == to {
                        let mut route = vec![to];
                        let mut cur = to;
                        while let Some(&p) = prev.get(&cur) {
                            route.push(p);
                            cur = p;
                        }
                        route.reverse();
                        return Some(route);
                    }
                    queue.push_back(n);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_graph() -> Systems {
        // A - B - C - D  (ids 1..4)
        let by_name = [("a", 1), ("b", 2), ("c", 3), ("d", 4)]
            .into_iter()
            .map(|(n, id)| {
                (
                    n.to_string(),
                    SystemInfo {
                        id,
                        name: n.to_uppercase(),
                        security: 0.0,
                        constellation: String::new(),
                        region: String::new(),
                        faction: String::new(),
                    },
                )
            })
            .collect();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in [(1, 2), (2, 3), (3, 4)] {
            adjacency.entry(a).or_default().push(b);
            adjacency.entry(b).or_default().push(a);
        }
        Systems::new(by_name, adjacency)
    }

    #[test]
    fn bfs_distances() {
        let g = line_graph();
        assert_eq!(g.jumps(1, 1, 10), Some(0));
        assert_eq!(g.jumps(1, 2, 10), Some(1));
        assert_eq!(g.jumps(1, 4, 10), Some(3));
        assert_eq!(g.jumps(1, 4, 2), None); // beyond max
        assert_eq!(g.jumps(1, 99, 10), None); // unknown system
    }
}
