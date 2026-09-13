//! Routes between two systems, as the map's drag gesture asks for them.
//!
//! Three questions, one endpoint: by gates, by capital jumps, and the titan answer, which is one jump
//! out of range followed by gates. All three come back in the same shape so the page draws and lists
//! them with one piece of code.

use serde::Serialize;

use crate::store::MapSystem;

/// One system on a route, and how it was reached from the one before it.
#[derive(Serialize)]
pub struct Hop {
    pub id: i64,
    pub name: String,
    pub security: f64,
    /// 0 gate, 1 jump bridge, 2 capital jump. The first hop of a route has no edge before it and is
    /// always 0.
    pub kind: u8,
    /// Light years covered by a capital jump. Absent for a gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ly: Option<f64>,
}

#[derive(Serialize)]
pub struct RouteOption {
    pub label: String,
    pub path: Vec<i64>,
    pub hops: Vec<Hop>,
    pub gates: usize,
    pub jumps: usize,
    pub total_ly: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Serialize, Default)]
pub struct RouteOut {
    pub kind: String,
    pub from: i64,
    pub to: i64,
    pub options: Vec<RouteOption>,
    /// Why there is nothing to show, when there is nothing to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn named(graph: &crate::geo::Systems, id: i64, kind: u8, ly: Option<f64>) -> Hop {
    let info = graph.info_of(id);
    Hop {
        id,
        name: info.map(|i| i.name.clone()).unwrap_or_else(|| id.to_string()),
        security: info.map(|i| i.security).unwrap_or_default(),
        kind,
        ly,
    }
}

/// The gate route, with jump bridges when the user counts them.
pub fn gate(
    graph: &crate::geo::Systems,
    from: i64,
    to: i64,
    bridges: bool,
) -> Option<RouteOption> {
    let path = graph.route(from, to, true, bridges, |_| true)?;
    let hops: Vec<Hop> = path
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let kind = if i > 0 && graph.is_bridge(path[i - 1], id) { 1 } else { 0 };
            named(graph, id, kind, None)
        })
        .collect();
    let gates = hops.iter().skip(1).filter(|h| h.kind == 0).count();
    Some(RouteOption {
        label: "Gates".to_owned(),
        gates,
        jumps: path.len().saturating_sub(1),
        total_ly: 0.0,
        note: None,
        path,
        hops,
    })
}

fn pos<'a>(coords: &'a [MapSystem], id: i64) -> Option<&'a MapSystem> {
    coords.iter().find(|s| s.id == id)
}

/// The capital jump route: cyno-able systems only, one hop per jump.
pub fn jump(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    max_ly: f64,
) -> Option<RouteOption> {
    let path =
        crate::jumproute::shortest_path_pref(coords, max_ly, from, to, &Default::default())?;
    let mut total_ly = 0.0;
    let mut hops = Vec::with_capacity(path.len());
    for (i, &id) in path.iter().enumerate() {
        let ly = if i == 0 {
            None
        } else {
            let d = pos(coords, path[i - 1])
                .zip(pos(coords, id))
                .map(|(a, b)| crate::map::ly_distance(a, b));
            total_ly += d.unwrap_or_default();
            d
        };
        hops.push(named(graph, id, if i == 0 { 0 } else { 2 }, ly));
    }
    Some(RouteOption {
        label: format!("{:.1} ly", total_ly),
        gates: 0,
        jumps: path.len().saturating_sub(1),
        total_ly,
        note: None,
        path,
        hops,
    })
}

/// How many jump-off candidates are worth offering.
///
/// The ring the search lands on can hold a dozen systems that are all the same number of gates from
/// the target. Past a handful they stop being a choice and start being a list.
const TITAN_OPTIONS: usize = 5;
/// How far the gate search will look for a way in from a jump-off point.
const TITAN_MAX_JUMPS: u32 = 40;

/// The titan answer: one jump as far in as range allows, then gates.
///
/// This is the rescue mode's calculation, which asks the question the other way round and for the
/// same reason: the fewest *gate* jumps from the target that a titan can still reach, not the system
/// that happens to be nearest on the map. A system four light years away and twelve gates out is a
/// worse answer than one six light years away and two gates out, and picking by distance gets that
/// backwards every time.
pub fn titan(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    max_ly: f64,
    bridges: bool,
) -> Vec<RouteOption> {
    let (Some(start), Some(target)) = (pos(coords, from), pos(coords, to)) else {
        return Vec::new();
    };
    // Everything the titan can reach in one jump, the target itself excluded: if it were in range
    // this would not be a titan route, it would be one jump.
    let in_range: std::collections::HashSet<i64> = coords
        .iter()
        .filter(|s| s.id != to && crate::jumproute::cyno_able(s.security))
        .filter(|s| crate::map::ly_distance(start, s) <= max_ly)
        .map(|s| s.id)
        .collect();
    if in_range.is_empty() {
        return Vec::new();
    }
    let Some((_, mut ring)) =
        graph.nearest_matching(to, TITAN_MAX_JUMPS, |id| in_range.contains(&id))
    else {
        return Vec::new();
    };
    // Same number of gates for all of them, so the tie goes to the shorter jump, which is less
    // fatigue and less fuel for an identical arrival.
    ring.sort_by(|a, b| {
        let d = |id: i64| pos(coords, id).map(|s| crate::map::ly_distance(start, s)).unwrap_or(f64::MAX);
        d(*a).partial_cmp(&d(*b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    ring.truncate(TITAN_OPTIONS);

    ring.iter()
        .filter_map(|&hop| {
            let ly = crate::map::ly_distance(start, pos(coords, hop)?);
            let rest = gate(graph, hop, to, bridges)?;
            let mut hops = vec![named(graph, from, 0, None), named(graph, hop, 2, Some(ly))];
            hops.extend(rest.hops.into_iter().skip(1));
            let mut path = vec![from];
            path.extend(rest.path.iter().copied());
            let name = graph.info_of(hop).map(|i| i.name.clone()).unwrap_or_default();
            Some(RouteOption {
                label: format!("via {name}"),
                gates: rest.gates,
                jumps: 1,
                total_ly: ly,
                note: Some(format!("{ly:.1} ly jump, then {} gates", rest.gates)),
                path,
                hops,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> std::sync::Arc<crate::geo::Systems> {
        crate::uitest::fixtures::systems()
    }

    /// A route to yourself is one system, not nothing: the caller draws `path` and an empty one
    /// would look like a failure.
    #[test]
    fn a_route_to_the_same_system_is_a_single_hop() {
        let g = graph();
        let id = 30_004_759;
        let r = gate(&g, id, id, false).expect("a system can reach itself");
        assert_eq!(r.path, vec![id]);
        assert_eq!(r.jumps, 0);
        assert_eq!(r.hops.len(), 1);
    }

    /// Every hop names a real system. An id that fell out of the graph would render as a number.
    #[test]
    fn every_hop_carries_a_name() {
        let g = graph();
        let (a, b) = (30_004_759, 30_004_608);
        let r = gate(&g, a, b, false).expect("the fixture systems are connected");
        assert!(r.hops.iter().all(|h| !h.name.is_empty() && h.name.parse::<i64>().is_err()));
        assert_eq!(r.hops.first().map(|h| h.id), Some(a));
        assert_eq!(r.hops.last().map(|h| h.id), Some(b));
        assert_eq!(r.gates, r.jumps, "no bridges in the fixture, so every hop is a gate");
    }
}
