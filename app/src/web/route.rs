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
    /// Isotopes this jump burns, and the two timers it leaves behind, in minutes. Absent for a gate,
    /// which costs neither.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fatigue_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactivation_min: Option<f64>,
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

/// The hulls a jump route can be planned for, so the page's picker is the app's own list rather than
/// a second copy of it.
#[derive(Serialize)]
pub struct Hull {
    pub name: &'static str,
    pub base_ly: f64,
}

pub fn hulls() -> Vec<Hull> {
    crate::jumproute::SHIP_CLASSES
        .iter()
        .map(|c| Hull { name: c.name, base_ly: c.base_ly })
        .collect()
}

#[derive(Serialize, Default)]
pub struct RouteOut {
    pub kind: String,
    pub from: i64,
    pub to: i64,
    pub options: Vec<RouteOption>,
    /// What the jump figures were worked out with, echoed back so the page's controls and the
    /// numbers beside them cannot drift apart.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hulls: Vec<Hull>,
    pub hull: usize,
    pub jdc: u32,
    pub jfc: u32,
    pub max_ly: f64,
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
        fuel: None,
        fatigue_min: None,
        reactivation_min: None,
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
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
) -> Option<RouteOption> {
    let max_ly = crate::jumproute::max_range_ly(class, jdc);
    let path =
        crate::jumproute::shortest_path_pref(coords, max_ly, from, to, &Default::default())?;
    // The fuel and both timers come from the app's own model, per jump, so the page reports what the
    // planner would and nothing has its own idea of how fatigue compounds.
    let costs = crate::jumproute::hop_costs(coords, &path, class, jfc);
    let total_ly: f64 = costs.iter().map(|c| c.ly).sum();
    let fuel: f64 = costs.iter().map(|c| c.fuel).sum();
    let mut hops = Vec::with_capacity(path.len());
    for (i, &id) in path.iter().enumerate() {
        let mut h = named(graph, id, if i == 0 { 0 } else { 2 }, None);
        if let Some(c) = i.checked_sub(1).and_then(|k| costs.get(k)) {
            h.ly = Some(c.ly);
            h.fuel = Some(c.fuel);
            h.fatigue_min = Some(c.fatigue_min);
            h.reactivation_min = Some(c.reactivation_min);
        }
        hops.push(h);
    }
    Some(RouteOption {
        label: format!("{total_ly:.1} ly"),
        gates: 0,
        jumps: path.len().saturating_sub(1),
        total_ly,
        note: Some(format!(
            "{} isotopes · {:.0} min fatigue at the end",
            (fuel.round() as i64).to_string(),
            costs.last().map(|c| c.fatigue_min).unwrap_or_default()
        )),
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

/// Two legs end to end. The second's first hop is the first's last, so it is dropped.
fn join(head: RouteOption, tail: RouteOption) -> RouteOption {
    let mut hops = head.hops;
    hops.extend(tail.hops.into_iter().skip(1));
    let mut path = head.path;
    path.extend(tail.path.into_iter().skip(1));
    RouteOption {
        label: tail.label,
        gates: head.gates + tail.gates,
        jumps: head.jumps + tail.jumps,
        total_ly: head.total_ly + tail.total_ly,
        note: tail.note,
        path,
        hops,
    }
}

/// A route through waypoints.
///
/// The anchors are the systems the drags named, in order. Every leg but the last is a plain gate or
/// jump leg; only the last one can have alternatives, which is what the option list is for. A titan
/// route chains as gates up to the last leg, because a titan route *is* one jump and then gates, and
/// chaining several jumps is what the jump route already does.
#[allow(clippy::too_many_arguments)]
pub fn chain(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    anchors: &[i64],
    kind: &str,
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
    titan_ly: f64,
    bridges: bool,
) -> Vec<RouteOption> {
    if anchors.len() < 2 {
        return Vec::new();
    }
    let leg = |a: i64, b: i64| -> Option<RouteOption> {
        match kind {
            "jump" => jump(graph, coords, a, b, class, jdc, jfc),
            _ => gate(graph, a, b, bridges),
        }
    };
    let split = anchors.len() - 2;
    let mut head: Option<RouteOption> = None;
    for w in anchors[..=split].windows(2) {
        let Some(next) = leg(w[0], w[1]) else { return Vec::new() };
        head = Some(match head {
            Some(h) => join(h, next),
            None => next,
        });
    }
    let (a, b) = (anchors[split], anchors[split + 1]);
    let last: Vec<RouteOption> = match kind {
        "titan" => titan(graph, coords, a, b, titan_ly, bridges),
        _ => leg(a, b).into_iter().collect(),
    };
    match head {
        Some(h) => last
            .into_iter()
            .map(|t| join(clone_option(&h), t))
            .collect(),
        None => last,
    }
}

/// `RouteOption` is not `Clone` by derive because `Hop` is not; this is the one place that needs it.
fn clone_option(o: &RouteOption) -> RouteOption {
    RouteOption {
        label: o.label.clone(),
        path: o.path.clone(),
        hops: o
            .hops
            .iter()
            .map(|h| Hop {
                id: h.id,
                name: h.name.clone(),
                security: h.security,
                kind: h.kind,
                ly: h.ly,
                fuel: h.fuel,
                fatigue_min: h.fatigue_min,
                reactivation_min: h.reactivation_min,
            })
            .collect(),
        gates: o.gates,
        jumps: o.jumps,
        total_ly: o.total_ly,
        note: o.note.clone(),
    }
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

    /// A waypoint makes one route, not two: the leg boundary is where the first leg's last hop is,
    /// and repeating it would draw a doubled system and count an extra jump.
    #[test]
    fn a_chain_joins_at_the_waypoint_without_repeating_it() {
        let g = graph();
        let (a, b, c) = (30_004_759_i64, 30_004_608, 30_003_704);
        let direct = gate(&g, a, c, false).expect("connected");
        let via = chain(&g, &[], &[a, b, c], "gate", &crate::jumproute::SHIP_CLASSES[1], 5, 5, 6.0, false);
        let via = via.first().expect("a chained route");
        assert_eq!(via.path.iter().filter(|&&id| id == b).count(), 1, "the waypoint appears once");
        assert_eq!(via.hops.len(), via.path.len());
        assert!(via.jumps >= direct.jumps, "a detour is never shorter than the direct route");
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
