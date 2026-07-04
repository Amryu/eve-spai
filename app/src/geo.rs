use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug)]
pub struct SystemInfo {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub constellation: String,
    pub region: String,
    pub faction: String,
}

pub fn is_wormhole_system(id: i64) -> bool {
    (31_000_000..32_000_000).contains(&id)
}

pub struct Systems {
    by_name: HashMap<String, SystemInfo>,
    by_id: HashMap<i64, SystemInfo>,
    adjacency: HashMap<i64, Vec<i64>>,
    gate_adjacency: HashMap<i64, Vec<i64>>,
    stargates: HashMap<i64, Vec<[f64; 3]>>,
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
            stargates: HashMap::new(),
        }
    }

    pub fn set_stargates(&mut self, stargates: HashMap<i64, Vec<[f64; 3]>>) {
        self.stargates = stargates;
    }

    pub fn on_gate(&self, system: i64, pos: [f64; 3]) -> bool {
        const ON_GATE_M: f64 = 150_000.0;
        self.stargates.get(&system).is_some_and(|gates| {
            gates.iter().any(|g| {
                let d2 = (g[0] - pos[0]).powi(2)
                    + (g[1] - pos[1]).powi(2)
                    + (g[2] - pos[2]).powi(2);
                d2 <= ON_GATE_M * ON_GATE_M
            })
        })
    }

    pub fn info_of(&self, id: i64) -> Option<&SystemInfo> {
        self.by_id.get(&id)
    }

    pub fn neighbors(&self, id: i64) -> &[i64] {
        self.adjacency.get(&id).map_or(&[], |v| v.as_slice())
    }

    pub fn neighbors_gates_only(&self, id: i64) -> &[i64] {
        self.gate_adjacency.get(&id).map_or(&[], |v| v.as_slice())
    }

    pub fn is_bridge(&self, a: i64, b: i64) -> bool {
        self.adjacency.get(&a).is_some_and(|v| v.contains(&b))
            && !self.gate_adjacency.get(&a).is_some_and(|v| v.contains(&b))
    }

    pub fn jump_bridge_dest(&self, id: i64) -> Option<&SystemInfo> {
        let gates = self.gate_adjacency.get(&id);
        self.adjacency
            .get(&id)?
            .iter()
            .find(|n| !gates.is_some_and(|g| g.contains(n)))
            .and_then(|n| self.by_id.get(n))
    }

    pub fn add_bridges(&mut self, pairs: &[(i64, i64)]) {
        for &(a, b) in pairs {
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

    pub fn lookup(&self, token: &str) -> Option<&SystemInfo> {
        self.by_name.get(&token.to_lowercase())
    }

    pub fn lookup_prefix(&self, token: &str) -> Option<&SystemInfo> {
        let t = token.to_lowercase();
        let mut found: Option<&SystemInfo> = None;
        for (name, info) in &self.by_name {
            if name.starts_with(&t) {
                if found.is_some() {
                    return None;
                }
                found = Some(info);
            }
        }
        found
    }

    pub fn region_names(&self) -> std::collections::HashSet<String> {
        self.by_name
            .values()
            .map(|i| i.region.to_lowercase())
            .filter(|r| !r.is_empty())
            .collect()
    }

    pub fn lookup_prefix_in_regions(&self, token: &str, regions: &[String]) -> Option<&SystemInfo> {
        if regions.is_empty() {
            return None;
        }
        let t = token.to_lowercase();
        let regset: std::collections::HashSet<String> =
            regions.iter().map(|r| r.to_lowercase()).collect();
        let mut found: Option<&SystemInfo> = None;
        for info in self.by_name.values() {
            if info.name.to_lowercase().starts_with(&t)
                && regset.contains(&info.region.to_lowercase())
            {
                if found.is_some() {
                    return None;
                }
                found = Some(info);
            }
        }
        found
    }

    pub fn jumps(&self, from: i64, to: i64, max_jumps: u32) -> Option<u32> {
        Self::bfs_jumps(&self.adjacency, from, to, max_jumps)
    }

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

    pub fn path(&self, from: i64, to: i64) -> Option<Vec<i64>> {
        self.route(from, to, true, true, |_| true)
    }

    pub fn route(
        &self,
        from: i64,
        to: i64,
        allow_regional_gates: bool,
        allow_jump_bridges: bool,
        allowed: impl Fn(i64) -> bool,
    ) -> Option<Vec<i64>> {
        if from == to {
            return Some(vec![from]);
        }
        let mut prev: HashMap<i64, i64> = HashMap::new();
        let mut visited: HashSet<i64> = HashSet::from([from]);
        let mut queue: VecDeque<i64> = VecDeque::from([from]);
        while let Some(sys) = queue.pop_front() {
            for &n in self.adjacency.get(&sys).into_iter().flatten() {
                let is_gate = self.gate_adjacency.get(&sys).is_some_and(|g| g.contains(&n));
                if is_gate {
                    let cross_region =
                        self.info_of(sys).map(|i| &i.region) != self.info_of(n).map(|i| &i.region);
                    if cross_region && !allow_regional_gates {
                        continue;
                    }
                } else if !allow_jump_bridges {
                    continue;
                }
                if n != to && !allowed(n) {
                    continue;
                }
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

    #[test]
    fn wormhole_system_ids() {
        assert!(is_wormhole_system(31_000_005));
        assert!(is_wormhole_system(31_002_238));
        assert!(!is_wormhole_system(30_000_142));
        assert!(!is_wormhole_system(30_002_659));
        assert!(!is_wormhole_system(0));
    }

    fn line_graph() -> Systems {
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
        assert_eq!(g.jumps(1, 4, 2), None);
        assert_eq!(g.jumps(1, 99, 10), None);
    }

    #[test]
    fn constrained_route() {
        let mk = |id: i64, region: &str| SystemInfo {
            id,
            name: format!("S{id}"),
            security: 0.0,
            constellation: String::new(),
            region: region.to_string(),
            faction: String::new(),
        };
        let by_name: HashMap<String, SystemInfo> = [
            ("a", mk(1, "R1")),
            ("b", mk(2, "R1")),
            ("c", mk(3, "R2")),
            ("d", mk(4, "R2")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in [(1, 2), (2, 3), (3, 4)] {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        let g = Systems::new(by_name, adj);
        assert_eq!(g.route(1, 4, true, true, |_| true), Some(vec![1, 2, 3, 4]));
        assert_eq!(g.route(1, 4, false, true, |_| true), None);
        assert_eq!(g.route(1, 4, true, true, |s| s != 3), None);
        assert_eq!(g.route(2, 2, true, true, |_| true), Some(vec![2]));
    }

    #[test]
    fn path_matches_unconstrained_route() {
        let g = line_graph();
        assert_eq!(g.path(1, 4), g.route(1, 4, true, true, |_| true));
        assert_eq!(g.path(1, 4), Some(vec![1, 2, 3, 4]));
        assert_eq!(g.path(2, 2), Some(vec![2]));
        assert_eq!(g.path(1, 99), None);
    }
}
