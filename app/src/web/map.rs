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
    /// Holds a Jove Observatory. Static, so it rides with the geometry rather than the live layer.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub j: bool,
}

#[derive(Serialize)]
pub struct Geometry {
    pub extent: f64,
    pub nodes: Vec<Node>,
    /// Index pairs into `nodes` with a style, deduped `a < b`. Ids would be eight bytes each and
    /// there are thousands of them; indices roughly halve the payload.
    ///
    /// The third value is 0 inside a constellation, 1 across constellations, 2 across regions. A
    /// gate says where a boundary runs as much as where you can go, and on a map of identical lines
    /// none of those boundaries were visible.
    pub edges: Vec<(usize, usize, u8)>,
    /// Real positions in hundredths of a light year, parallel to `nodes`.
    ///
    /// The drawn coordinates are a projection quantised into a 0..4096 box, which cannot answer "is
    /// this within 6 ly", so jump range needs the real thing. Three small integers per system rather
    /// than three floats: at 0.01 ly the whole galaxy fits in about a million units and the error is
    /// far below anything a range band cares about.
    pub pos3: Vec<[i32; 3]>,
    /// Map units per light year, for drawing a range band at the right size. Exact for a geographic
    /// layout, and the same approximation the app makes for any other.
    pub units_per_ly: f64,
    /// Region id to name, for the labels the map draws when it is zoomed out too far for system
    /// names to be readable. About a hundred entries, so it rides with the geometry.
    pub regions: Vec<(i64, String)>,
    /// Jump bridges, same index-pair shape. Separate from `edges` because they are drawn
    /// differently and can be switched off on their own.
    pub bridges: Vec<(usize, usize)>,
}

/// Whether a system belongs on the star map at all.
///
/// The SDE carries far more than the map shows: wormhole space, abyssal pockets and the Jove
/// regions are all in `sde_systems`, and none of them are places you fly to through a gate. The
/// desktop map drops them with exactly these two rules (`app.rs`, the `map_systems` build), so this
/// uses the same ones rather than inventing a third answer.
///
/// - Nothing connects to it, so it cannot be reached or drawn as part of the graph.
/// - Its region name contains a digit, which is how J-space (`A-R00001`) and the abyssal and Jove
///   regions are named and normal space is not.
pub fn on_the_map(id: i64, graph: &crate::geo::Systems) -> bool {
    !graph.neighbors(id).is_empty()
        && graph.info_of(id).is_none_or(|i| !i.region.chars().any(|c| c.is_ascii_digit()))
}

pub fn build(systems: &[crate::store::MapSystem], graph: &crate::geo::Systems) -> Geometry {
    let mut nodes: Vec<&crate::store::MapSystem> =
        systems.iter().filter(|s| on_the_map(s.id, graph)).collect();
    nodes.sort_unstable_by_key(|s| s.id);

    let (min_x, max_x, min_z, max_z) = bounds(&nodes);
    let span = ((max_x - min_x).max(max_z - min_z)).max(1.0);
    let project = |v: f64, min: f64| (((v - min) / span) * EXTENT).round() as i64;
    // North is up. `map::project` draws with `center.y - (z - mid)`, so screen y runs opposite to
    // z; emitting z unflipped rendered the whole map upside down against the app's.
    let project_z = |v: f64| ((((max_z - v) / span)) * EXTENT).round() as i64;

    let index: std::collections::HashMap<i64, usize> =
        nodes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();

    let mut edges: Vec<(usize, usize, u8)> = Vec::new();
    let mut bridges: Vec<(usize, usize)> = Vec::new();
    for s in &nodes {
        let Some(&a) = index.get(&s.id) else { continue };
        for &nb in graph.neighbors_gates_only(s.id) {
            let Some(&b) = index.get(&nb) else { continue };
            if a < b {
                edges.push((a, b, edge_style(graph, s.id, nb)));
            }
        }
        for &nb in graph.neighbors(s.id) {
            if !graph.is_bridge(s.id, nb) {
                continue;
            }
            let Some(&b) = index.get(&nb) else { continue };
            if a < b {
                bridges.push((a, b));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    bridges.sort_unstable();
    bridges.dedup();

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
                z: project_z(s.z2d),
                j: crate::jove::has(s.id),
            })
            .collect(),
        pos3: nodes
            .iter()
            .map(|s| {
                let ly = |v: f64| (v / crate::map::LY_METERS * 100.0).round() as i32;
                [ly(s.x), ly(s.y), ly(s.z)]
            })
            .collect(),
        units_per_ly: EXTENT * crate::map::LY_METERS / span,
        edges,
        bridges,
        regions: {
            let mut r: Vec<(i64, String)> = nodes
                .iter()
                .filter_map(|s| {
                    graph.info_of(s.id).map(|i| (s.region_id, i.region.clone()))
                })
                .collect();
            r.sort_unstable();
            r.dedup_by_key(|(id, _)| *id);
            r
        },
    }
}

/// 0 within a constellation, 1 across constellations, 2 across regions.
fn edge_style(graph: &crate::geo::Systems, a: i64, b: i64) -> u8 {
    let (Some(x), Some(y)) = (graph.info_of(a), graph.info_of(b)) else { return 0 };
    if x.region != y.region {
        2
    } else if x.constellation != y.constellation {
        1
    } else {
        0
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
        // 7-K5EL is in Fountain in the fixture graph, so it gets its own region id here too:
        // a row whose `region_id` disagrees with the graph's region name is not a real row.
        let region_id = if id == 30_003_704 { 10_000_058 } else { 10_000_060 };
        crate::store::MapSystem {
            id,
            name: name.to_owned(),
            security: -0.36,
            region_id,
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
        for &(a, b, _) in &geo.edges {
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
            let pair = e.as_array().unwrap();
            for side in &pair[..2] {
                let x = side.as_u64().unwrap();
                assert!(x < n, "{x} is not an index into {n} nodes, it looks like an id");
            }
            assert!(pair[2].as_u64().unwrap() <= 2, "style is one of three");
        }
    }

    /// North is up, as in the app. Emitting z unflipped rendered the whole map upside down.
    #[test]
    fn greater_z_is_higher_on_screen() {
        let (_, g) = fixture();
        let s = vec![
            sys(30_004_759, "south", 0.0, 0.0),
            sys(30_004_608, "north", 0.0, 100.0),
        ];
        let geo = build(&s, &g);
        let by = |n: &str| geo.nodes.iter().find(|x| x.n == n).unwrap().z;
        assert!(by("north") < by("south"), "north {} should sit above south {}", by("north"), by("south"));
    }

    /// The fixtures straddle a real boundary: 1DQ1-A and 319-3D are both in Delve, 7-K5EL is in
    /// Fountain, and all three share a constellation name. So one gate is interior and one leaves
    /// the region, which is exactly the distinction being drawn.
    #[test]
    fn gate_style_says_which_boundary_it_crosses() {
        let (_, g) = fixture();
        assert_eq!(edge_style(&g, 30_004_759, 30_004_608), 0, "1DQ1-A to 319-3D, both Delve");
        assert_eq!(edge_style(&g, 30_004_608, 30_003_704), 2, "319-3D to 7-K5EL, Delve to Fountain");
        // Symmetric: which end you ask from cannot change the answer.
        assert_eq!(edge_style(&g, 30_003_704, 30_004_608), 2);
        // A system the graph has never heard of falls back to interior rather than guessing.
        assert_eq!(edge_style(&g, 30_009_999, 30_004_608), 0);
    }

    /// A range band is only meaningful if the positions behind it are the real ones. The drawn
    /// coordinates are projected and quantised, so they cannot answer a distance question.
    #[test]
    fn real_positions_ride_alongside_the_drawn_ones() {
        let (s, g) = fixture();
        let geo = build(&s, &g);
        assert_eq!(geo.pos3.len(), geo.nodes.len(), "one per node, in the same order");
        assert!(geo.units_per_ly > 0.0);
        // The fixture systems are metres apart, which is a rounding error in light years, so every
        // real position lands on the origin. That is correct: it is the same number the app would
        // compute, not a placeholder.
        assert!(geo.pos3.iter().all(|p| p == &[0, 0, 0]), "{:?}", geo.pos3);
    }

    #[test]
    fn regions_are_named_once_each() {
        let (s, g) = fixture();
        let geo = build(&s, &g);
        let names: Vec<&str> = geo.regions.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.contains(&"Delve") && names.contains(&"Fountain"), "{names:?}");
        let ids: Vec<i64> = geo.regions.iter().map(|(i, _)| *i).collect();
        let mut uniq = ids.clone();
        uniq.dedup();
        assert_eq!(ids, uniq, "one entry per region");
    }

    #[test]
    fn security_is_rounded_to_one_place() {
        let mut s = vec![sys(30_004_759, "1DQ1-A", 0.0, 0.0)];
        s[0].security = -0.3649;
        let (_, g) = fixture();
        assert_eq!(build(&s, &g).nodes[0].s, -0.4);
    }

    /// The reported bug: the payload carried 2604 wormhole systems and 401 abyssal ones, none of
    /// which are on the star map, plus 3222 systems with no gate at all.
    #[test]
    fn wormhole_abyssal_and_unconnected_systems_are_not_on_the_map() {
        let (mut s, g) = fixture();
        // A J-space system: in the SDE, in no region the map draws, connected to nothing.
        s.push(sys(31_000_123, "J123456", 500.0, 500.0));
        // An abyssal pocket.
        s.push(sys(32_000_042, "ADR01", 600.0, 600.0));
        // A k-space id that the graph has no edges for.
        s.push(sys(30_009_999, "Orphan", 700.0, 700.0));

        let geo = build(&s, &g);
        let names: Vec<&str> = geo.nodes.iter().map(|n| n.n.as_str()).collect();
        assert_eq!(names, vec!["7-K5EL", "319-3D", "1DQ1-A"], "only connected k-space, {names:?}");
    }

    /// Every node has to keep indexing correctly after the filter removes rows. Filtering before
    /// the index is built is the whole reason this is safe; doing it after would shift every edge.
    #[test]
    fn edges_still_index_the_filtered_list() {
        let (mut s, g) = fixture();
        s.insert(0, sys(31_000_123, "J123456", 500.0, 500.0));
        let geo = build(&s, &g);
        for &(a, b, _) in &geo.edges {
            assert!(a < geo.nodes.len() && b < geo.nodes.len(), "{a},{b} out of range");
        }
        assert_eq!(geo.edges.len(), 2);
    }

    #[test]
    fn an_empty_universe_does_not_divide_by_zero() {
        let (_, g) = fixture();
        let geo = build(&[], &g);
        assert!(geo.nodes.is_empty() && geo.edges.is_empty());
    }
}
