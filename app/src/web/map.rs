//! Map geometry for the page.
//!
//! Served separately from the live layer and cached hard: the SDE does not change between releases,
//! while intel changes every few seconds. Re-sending 8000 nodes twice a second would be absurd, so
//! the rule is that the live payload in `snapshot::MapLive` carries system ids and never coordinates.

use serde::Serialize;

/// The projected box nodes are quantised into. Integers in a fixed range keep the payload small and
/// give the browser a `viewBox` it can pan and zoom without rescaling anything.
pub const EXTENT: f64 = 4096.0;

#[derive(Serialize)]
pub struct Node {
    pub i: i64,
    pub n: String,
    /// Security, one decimal. The page indexes the same eleven-stop ramp the app does.
    pub s: f64,
    pub r: i64,
    pub x: i64,
    pub z: i64,
}

#[derive(Serialize)]
pub struct Geometry {
    pub extent: f64,
    pub nodes: Vec<Node>,
    /// Index pairs into `nodes`, deduped `a < b`. Ids would be eight bytes each and there are
    /// thousands of them; indices roughly halve the payload.
    pub edges: Vec<(usize, usize)>,
}

pub fn build(systems: &[crate::store::MapSystem], graph: &crate::geo::Systems) -> Geometry {
    let mut nodes: Vec<&crate::store::MapSystem> = systems.iter().collect();
    nodes.sort_unstable_by_key(|s| s.id);

    let (min_x, max_x, min_z, max_z) = bounds(&nodes);
    let span = ((max_x - min_x).max(max_z - min_z)).max(1.0);
    let project = |v: f64, min: f64| (((v - min) / span) * EXTENT).round() as i64;

    let index: std::collections::HashMap<i64, usize> =
        nodes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();

    let mut edges: Vec<(usize, usize)> = Vec::new();
    for s in &nodes {
        let Some(&a) = index.get(&s.id) else { continue };
        for &nb in graph.neighbors_gates_only(s.id) {
            let Some(&b) = index.get(&nb) else { continue };
            if a < b {
                edges.push((a, b));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();

    Geometry {
        extent: EXTENT,
        nodes: nodes
            .iter()
            .map(|s| Node {
                i: s.id,
                n: s.name.clone(),
                s: (s.security * 10.0).round() / 10.0,
                r: s.region_id,
                x: project(s.x2d, min_x),
                z: project(s.z2d, min_z),
            })
            .collect(),
        edges,
    }
}

fn bounds(nodes: &[&crate::store::MapSystem]) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for s in nodes {
        b.0 = b.0.min(s.x2d);
        b.1 = b.1.max(s.x2d);
        b.2 = b.2.min(s.z2d);
        b.3 = b.3.max(s.z2d);
    }
    if nodes.is_empty() {
        (0.0, 1.0, 0.0, 1.0)
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(id: i64, name: &str, x: f64, z: f64) -> crate::store::MapSystem {
        crate::store::MapSystem {
            id,
            name: name.to_owned(),
            security: -0.36,
            region_id: 10_000_060,
            x,
            y: 0.0,
            z,
            x2d: x,
            z2d: z,
        }
    }

    fn fixture() -> (Vec<crate::store::MapSystem>, std::sync::Arc<crate::geo::Systems>) {
        (
            vec![
                sys(30_004_759, "1DQ1-A", 0.0, 0.0),
                sys(30_004_608, "319-3D", 100.0, 50.0),
                sys(30_003_704, "7-K5EL", 200.0, 0.0),
            ],
            crate::uitest::fixtures::systems(),
        )
    }

    #[test]
    fn every_coordinate_lands_inside_the_box() {
        let (s, g) = fixture();
        let geo = build(&s, &g);
        assert_eq!(geo.nodes.len(), 3);
        for n in &geo.nodes {
            assert!((0..=EXTENT as i64).contains(&n.x), "{} x={}", n.n, n.x);
            assert!((0..=EXTENT as i64).contains(&n.z), "{} z={}", n.n, n.z);
        }
    }

    #[test]
    fn edges_are_index_pairs_deduped_low_to_high() {
        let (s, g) = fixture();
        let geo = build(&s, &g);
        // The fixture graph is a line: 1DQ1-A to 319-3D to 7-K5EL.
        assert_eq!(geo.edges.len(), 2, "{:?}", geo.edges);
        for &(a, b) in &geo.edges {
            assert!(a < b, "an edge must be stored once, low index first: {a},{b}");
            assert!(b < geo.nodes.len(), "an edge must index a node that exists");
        }
    }

    /// A system id is eight digits and there are thousands of edges. This is what stops someone
    /// "simplifying" the wire format back to ids without noticing the cost.
    #[test]
    fn edges_are_indices_on_the_wire_not_ids() {
        let (s, g) = fixture();
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&build(&s, &g)).unwrap())
            .unwrap();
        let n = v["nodes"].as_array().unwrap().len() as u64;
        for e in v["edges"].as_array().unwrap() {
            for side in e.as_array().unwrap() {
                let x = side.as_u64().unwrap();
                assert!(x < n, "{x} is not an index into {n} nodes, it looks like an id");
            }
        }
    }

    #[test]
    fn security_is_rounded_to_one_place() {
        let mut s = vec![sys(30_004_759, "1DQ1-A", 0.0, 0.0)];
        s[0].security = -0.3649;
        let (_, g) = fixture();
        assert_eq!(build(&s, &g).nodes[0].s, -0.4);
    }

    #[test]
    fn an_empty_universe_does_not_divide_by_zero() {
        let (_, g) = fixture();
        let geo = build(&[], &g);
        assert!(geo.nodes.is_empty() && geo.edges.is_empty());
    }
}
