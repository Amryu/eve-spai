//! Routes for the map's drag gesture: by gates, by capital jumps, and by titan bridge plus gates.
//! All three share one shape so the page draws them with one piece of code.

use serde::Serialize;

use crate::store::MapSystem;

/// One system on a route, and how it was reached from the one before it.
#[derive(Serialize)]
pub struct Hop {
    pub id: i64,
    pub name: String,
    pub security: f64,
    /// Edge into this hop: 0 gate, 1 jump bridge, 2 capital jump. The first hop is 0.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ly: Option<f64>,
    /// Isotopes burned and the fatigue and reactivation timers in minutes. Absent for a gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fatigue_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactivation_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<HopWarning>,
    /// The start, a waypoint, or the destination, as opposed to a system the route passes through.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anchor: bool,
}

/// One leg of a route with its alternatives: same jump count, sorted by distance.
#[derive(Serialize)]
pub struct LegChoice {
    pub from: i64,
    pub to: i64,
    pub from_name: String,
    pub to_name: String,
    pub options: Vec<RouteOption>,
    /// Set on a titan route's last leg, whose options are the whole route's options, so the window
    /// must not also show them as a per-leg switcher.
    pub whole_route: bool,
}

#[derive(Default, Clone)]
pub struct Avoid {
    /// Persistent, from settings.
    pub always: std::collections::HashSet<i64>,
    /// This route only, from the client.
    pub once: std::collections::HashSet<i64>,
}

impl Avoid {
    pub fn blocked(&self, id: i64) -> bool {
        self.always.contains(&id) || self.once.contains(&id)
    }
    pub fn any(&self) -> bool {
        !self.always.is_empty() || !self.once.is_empty()
    }
}

/// Each alternative costs a search, and past a handful they stop being a useful choice.
const LEG_OPTIONS: usize = 4;

/// Intel below Danger is not carried: a nullsec route passes dozens of reported systems, and warning
/// on all of them would be noise.
#[derive(Serialize, Clone, Copy, Default, PartialEq)]
pub struct HopWarning {
    /// Worst severity reported inside the intel TTL. 2 is Danger, 3 Critical.
    pub sev: u8,
    /// When that report came in.
    pub at: i64,
    /// Ship and pod kills in the last hour, from ESI.
    pub kills: u32,
    pub pods: u32,
}

pub const WARN_SEVERITY: u8 = 2;

pub fn mark_anchors(options: &mut [RouteOption], anchors: &[i64]) {
    let set: std::collections::HashSet<i64> = anchors.iter().copied().collect();
    for o in options.iter_mut() {
        for h in o.hops.iter_mut() {
            h.anchor = set.contains(&h.id);
        }
    }
}

/// A pass over finished routes rather than an argument to each builder, so the danger lookup lives
/// in one place instead of three.
pub fn annotate(
    options: &mut [RouteOption],
    danger: &std::collections::HashMap<i64, HopWarning>,
) {
    for o in options.iter_mut() {
        for h in o.hops.iter_mut() {
            h.warn = danger.get(&h.id).copied();
        }
    }
}

/// Warnings from the published snapshot: the map pane's intel and the status pane's kill counts.
pub fn danger_from_marks(
    intel: &[(i64, u8, i64)],
    kills: &[(i64, u32, u32)],
) -> std::collections::HashMap<i64, HopWarning> {
    let mut out: std::collections::HashMap<i64, HopWarning> = std::collections::HashMap::new();
    for &(id, sev, at) in intel {
        if sev >= WARN_SEVERITY {
            let e = out.entry(id).or_default();
            e.sev = e.sev.max(sev);
            e.at = e.at.max(at);
        }
    }
    for &(id, k, p) in kills {
        if k > 0 || p > 0 {
            let e = out.entry(id).or_default();
            e.kills = k;
            e.pods = p;
        }
    }
    out
}

/// Warnings from raw reports, because the desktop has no published snapshot when the web view is
/// off.
pub fn danger_from_reports(
    reports: &[crate::intel::IntelReport],
    rules: &crate::settings::SeverityRules,
    ttl: i64,
    now: i64,
    kills: &[(i64, u32, u32)],
) -> std::collections::HashMap<i64, HopWarning> {
    let marks: Vec<(i64, u8, i64)> = reports
        .iter()
        .filter(|r| ttl <= 0 || now - r.received <= ttl)
        .filter(|r| !r.clear)
        .flat_map(|r| {
            let sev = crate::app::severity_of(r, rules) as u8;
            r.systems.iter().map(move |s| (s.id, sev, r.received))
        })
        .collect();
    danger_from_marks(&marks, kills)
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
    /// What avoidance cost, if it changed the route.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detour: Option<String>,
    /// The titan's own jump when it moves before bridging. Not a hop, because the fleet does not fly
    /// it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub titan_jump: Option<TitanJump>,
    /// Gates saved over the plain gate route.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<usize>,
    /// Set only when the route crosses a scanned wormhole, not when the setting merely allows it.
    /// This is what makes a saved route expire.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub uses_wormhole: bool,
}

#[derive(Serialize, Clone)]
pub struct TitanJump {
    pub from: i64,
    pub to: i64,
    pub from_name: String,
    pub to_name: String,
    pub ly: f64,
}

#[derive(Serialize)]
pub struct Avoided {
    pub id: i64,
    pub name: String,
    pub always: bool,
}

pub fn avoided(graph: &crate::geo::Systems, avoid: &Avoid) -> Vec<Avoided> {
    let mut out: Vec<Avoided> = avoid
        .always
        .iter()
        .map(|id| (id, true))
        .chain(avoid.once.iter().map(|id| (id, false)))
        .map(|(&id, always)| Avoided {
            id,
            name: graph.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string()),
            always,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

#[derive(Serialize)]
pub struct Hull {
    pub name: &'static str,
    pub base_ly: f64,
}

/// Served from the app's own list so the page's picker cannot drift from it.
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
    /// Per-leg alternatives, so the window can offer them per waypoint.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub legs: Vec<LegChoice>,
    /// Echoed so the page's controls match the numbers computed with them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hulls: Vec<Hull>,
    pub hull: usize,
    pub jdc: u32,
    pub jfc: u32,
    pub max_ly: f64,
    /// Echoed so the page shows the app's setting instead of keeping its own copy.
    pub via_wormholes: bool,
    /// Named, because "avoiding 3 systems" is only useful if the user can see which three.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub avoided: Vec<Avoided>,
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
        warn: None,
        anchor: false,
    }
}

pub fn gate(
    graph: &crate::geo::Systems,
    from: i64,
    to: i64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) -> Option<RouteOption> {
    // Endpoints are exempt, or avoiding the system you stand in would yield no route at all.
    // Wormholes arrive as extra edges because that is how the app's own planner takes them.
    let path = graph.route_with(from, to, true, bridges, holes, |id| {
        id == from || id == to || !avoid.blocked(id)
    })?;
    let hops: Vec<Hop> = path
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let kind = if i > 0 && graph.is_bridge(path[i - 1], id) { 1 } else { 0 };
            named(graph, id, kind, None)
        })
        .collect();
    let gates = hops.iter().skip(1).filter(|h| h.kind == 0).count();
    // A step that is not in the graph's own adjacency is one the hole map let through.
    let uses_wormhole = path.windows(2).any(|w| graph.is_hole_step(w[0], w[1]));
    Some(RouteOption {
        uses_wormhole,
        label: "Gates".to_owned(),
        gates,
        jumps: path.len().saturating_sub(1),
        total_ly: 0.0,
        note: None,
        detour: None,
        titan_jump: None,
        saved: None,
        path,
        hops,
    })
}

fn pos<'a>(coords: &'a [MapSystem], id: i64) -> Option<&'a MapSystem> {
    coords.iter().find(|s| s.id == id)
}

/// Cyno-able systems only, one hop per jump.
#[allow(clippy::too_many_arguments)]
pub fn jump(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
    avoid: &Avoid,
) -> Option<RouteOption> {
    let max_ly = crate::jumproute::max_range_ly(class, jdc);
    // Avoided systems are removed from the search set so the search routes around them.
    let filtered: Vec<MapSystem>;
    let search: &[MapSystem] = if avoid.any() {
        filtered = coords
            .iter()
            .filter(|s| s.id == from || s.id == to || !avoid.blocked(s.id))
            .cloned()
            .collect();
        &filtered
    } else {
        coords
    };
    let path =
        crate::jumproute::shortest_path_pref(search, max_ly, from, to, &Default::default())?;
    // Costs come from the planner's own model so the page and the desktop agree on fatigue.
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
        detour: None,
        titan_jump: None,
        saved: None,
        uses_wormhole: false,
        note: Some(format!(
            "{} isotopes · {:.0} min fatigue at the end",
            (fuel.round() as i64).to_string(),
            costs.last().map(|c| c.fatigue_min).unwrap_or_default()
        )),
        path,
        hops,
    })
}

/// The ring can hold a dozen systems equally far by gates; past a handful they stop being a choice.
const TITAN_OPTIONS: usize = 5;
/// How far the gate search will look for a way in from a jump-off point.
const TITAN_MAX_JUMPS: u32 = 40;

/// One titan jump, gates for the rest.
///
/// Like rescue mode, picks the reachable system fewest *gates* from the far end, not the nearest by
/// distance: 4 ly and twelve gates out is worse than 6 ly and two gates out.
///
/// `at_start` set: the titan is in the start system. Cleared: it waits at the far end and bridges the
/// fleet in, which is the same search run from the other end.
#[allow(clippy::too_many_arguments)]
pub fn titan(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    max_ly: f64,
    bridges: bool,
    at_start: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    titans: &[i64],
    self_jump: bool,
) -> Vec<RouteOption> {
    // A titan sitting with the fleet can reposition first and bridge from somewhere better.
    if self_jump && at_start && titans.is_empty() {
        let opts = titan_self_jump(graph, coords, from, to, max_ly, bridges, avoid, holes);
        if !opts.is_empty() {
            return opts;
        }
    }
    // Named titans are known positions, so they win over the `at_start` guess.
    if !titans.is_empty() {
        return titan_via(graph, coords, from, to, max_ly, bridges, avoid, holes, titans);
    }
    if !at_start {
        // Plan it backwards and reverse, so there is one implementation that cannot disagree.
        return titan(graph, coords, to, from, max_ly, bridges, true, avoid, holes, titans, false)
            .into_iter()
            .map(reverse)
            .collect();
    }
    // The target's position is unused, the ring is found by gate distance.
    let (Some(start), Some(_)) = (pos(coords, from), pos(coords, to)) else {
        return Vec::new();
    };
    // The target is excluded: if it were in range this would be a single jump.
    let in_range: std::collections::HashSet<i64> = coords
        .iter()
        .filter(|s| s.id != to && crate::jumproute::jumpable(s))
        .filter(|s| !avoid.blocked(s.id))
        .filter(|s| crate::map::ly_distance(start, s) <= max_ly)
        .map(|s| s.id)
        .collect();
    if in_range.is_empty() {
        return Vec::new();
    }
    let Some((_, mut ring)) = graph.nearest_matching(to, TITAN_MAX_JUMPS, |id| {
        in_range.contains(&id) && !avoid.blocked(id)
    })
    else {
        return Vec::new();
    };
    // Equal gates, so ties go to the shorter jump for less fatigue and fuel.
    ring.sort_by(|a, b| {
        let d = |id: i64| pos(coords, id).map(|s| crate::map::ly_distance(start, s)).unwrap_or(f64::MAX);
        d(*a).partial_cmp(&d(*b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    ring.truncate(TITAN_OPTIONS);

    ring.iter()
        .filter_map(|&hop| {
            let ly = crate::map::ly_distance(start, pos(coords, hop)?);
            let rest = gate(graph, hop, to, bridges, avoid, holes)?;
            let rest_uses_hole = rest.uses_wormhole;
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
                detour: rest.detour,
                uses_wormhole: rest_uses_hole,
                titan_jump: None,
                saved: None,
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
        detour: head.detour.or(tail.detour),
        titan_jump: head.titan_jump.or(tail.titan_jump),
        saved: head.saved.or(tail.saved),
        uses_wormhole: head.uses_wormhole || tail.uses_wormhole,
        path,
        hops,
    }
}

/// Alternatives for one leg with the same jump count as the best path.
///
/// Bans each intermediate system of the best path in turn, the cheap half of Yen's algorithm. Enough
/// to answer "what else costs the same" without ranking every path.
#[allow(clippy::too_many_arguments)]
fn leg_options(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    a: i64,
    b: i64,
    kind: &str,
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) -> Vec<RouteOption> {
    let one = |extra: &Avoid| -> Option<RouteOption> {
        match kind {
            "jump" => jump(graph, coords, a, b, class, jdc, jfc, extra),
            _ => gate(graph, a, b, bridges, extra, holes),
        }
    };
    let Some(mut best) = one(avoid) else { return Vec::new() };
    let want = best.jumps;
    // One extra search, so a detour caused by the avoid list is explained.
    if avoid.any() {
        if let Some(free) = one(&Avoid::default()) {
            if free.path != best.path {
                let extra = best.jumps as i64 - free.jumps as i64;
                best.detour = Some(match extra {
                    0 => "avoiding systems, same length".to_owned(),
                    1 => "1 jump longer, avoiding systems".to_owned(),
                    n if n > 0 => format!("{n} jumps longer, avoiding systems"),
                    _ => "avoiding systems".to_owned(),
                });
            }
        }
    }
    let mut out = vec![best];
    let mut seen: std::collections::HashSet<Vec<i64>> =
        std::collections::HashSet::from([out[0].path.clone()]);
    let via: Vec<i64> = out[0].path.iter().copied().filter(|&id| id != a && id != b).collect();
    for ban in via {
        if out.len() >= LEG_OPTIONS {
            break;
        }
        let mut extra = avoid.clone();
        extra.once.insert(ban);
        let Some(alt) = one(&extra) else { continue };
        if alt.jumps != want || !seen.insert(alt.path.clone()) {
            continue;
        }
        out.push(alt);
    }
    // Ties go to the shorter distance. Gate routes carry no distance, so they keep search order.
    out.sort_by(|p, q| p.total_ly.partial_cmp(&q.total_ly).unwrap_or(std::cmp::Ordering::Equal));
    // Label by the system that differs, or gate alternatives would all read the same.
    let common: std::collections::HashSet<i64> = out
        .iter()
        .skip(1)
        .fold(out[0].path.iter().copied().collect(), |acc: std::collections::HashSet<i64>, o| {
            acc.intersection(&o.path.iter().copied().collect()).copied().collect()
        });
    for o in out.iter_mut() {
        if let Some(&via) = o.path.iter().find(|id| !common.contains(id)) {
            let name = graph.info_of(via).map(|i| i.name.clone()).unwrap_or_default();
            if !name.is_empty() {
                o.label = format!("via {name}");
            }
        }
    }
    out
}

/// A route through waypoints. `pick` selects each leg's alternative, and the assembled route comes
/// back with the choices so the client never joins legs itself.
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
    titan_at_start: bool,
    titans: &[i64],
    titan_self_jump: bool,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    pick: &[usize],
) -> (Vec<LegChoice>, Vec<RouteOption>) {
    if anchors.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let name = |id: i64| {
        graph.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string())
    };
    let last = anchors.len() - 2;
    let mut legs: Vec<LegChoice> = Vec::new();
    for (i, w) in anchors.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let options = if i == last && kind == "titan" {
            titan(graph, coords, a, b, titan_ly, bridges, titan_at_start, avoid, holes, titans, titan_self_jump)
        } else {
            leg_options(graph, coords, a, b, kind, class, jdc, jfc, bridges, avoid, holes)
        };
        legs.push(LegChoice {
            from: a,
            to: b,
            from_name: name(a),
            to_name: name(b),
            options,
            whole_route: false,
        });
    }
    if legs.iter().any(|l| l.options.is_empty()) {
        return (legs, Vec::new());
    }
    let assemble = |choice: &dyn Fn(usize) -> usize| -> RouteOption {
        let mut acc: Option<RouteOption> = None;
        for (i, l) in legs.iter().enumerate() {
            let o = clone_option(&l.options[choice(i).min(l.options.len() - 1)]);
            acc = Some(match acc {
                Some(h) => join(h, o),
                None => o,
            });
        }
        acc.expect("at least one leg")
    };
    let chosen = assemble(&|i: usize| pick.get(i).copied().unwrap_or(0));
    let mut out = vec![chosen];
    // Titan alternatives cover the whole route, so they become the route's options.
    if kind == "titan" && legs[last].options.len() > 1 {
        out = (0..legs[last].options.len())
            .map(|k| assemble(&move |i: usize| if i == last { k } else { pick.get(i).copied().unwrap_or(0) }))
            .collect();
        // The option tabs choose this leg and `pick` does not reach it, so a switcher would do nothing.
        legs[last].whole_route = true;
    }
    (legs, out)
}

/// The titan jumps itself, the fleet gates out to it, and it bridges them from there.
///
/// Minimizes the fleet's total gates. Gate distances from the start and to the destination are
/// computed once, so a landing's score is a sum and only the bridge target needs a scan.
#[allow(clippy::too_many_arguments)]
fn titan_self_jump(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    max_ly: f64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) -> Vec<RouteOption> {
    /// Past this many gates to meet the titan it is no longer a shortcut.
    const MAX_GATE_TO_TITAN: u32 = 8;

    let (Some(start), Some(plain)) =
        (pos(coords, from), gate(graph, from, to, bridges, avoid, holes))
    else {
        return Vec::new();
    };
    let baseline = plain.gates;
    let out_from_start = graph.gate_distances_from(from, MAX_GATE_TO_TITAN);
    let in_to_target = graph.gate_distances_from(to, TITAN_MAX_JUMPS);
    let max_m = max_ly;

    let landings: Vec<&MapSystem> = coords
        .iter()
        .filter(|s| s.id != from && crate::jumproute::jumpable(s))
        .filter(|s| !avoid.blocked(s.id))
        .filter(|s| out_from_start.contains_key(&s.id))
        .filter(|s| crate::map::ly_distance(start, s) <= max_m)
        .collect();

    // Score: fleet gates to reach the titan plus gates after the bridge.
    let mut scored: Vec<(usize, f64, i64, i64)> = Vec::new();
    for land in landings {
        let fleet_out = *out_from_start.get(&land.id).unwrap_or(&u32::MAX) as usize;
        let mut best: Option<(usize, i64)> = None;
        for h in coords {
            if h.id == to || !crate::jumproute::jumpable(h) || avoid.blocked(h.id) {
                continue;
            }
            let Some(&d) = in_to_target.get(&h.id) else { continue };
            if best.is_some_and(|(b, _)| d as usize >= b) {
                continue;
            }
            if crate::map::ly_distance(land, h) <= max_m {
                best = Some((d as usize, h.id));
            }
        }
        let Some((gates_in, hop)) = best else { continue };
        let total = fleet_out + gates_in;
        // Strictly better only, a tie cycles the titan's drive for nothing.
        if total >= baseline {
            continue;
        }
        scored.push((total, crate::map::ly_distance(start, land), land.id, hop));
    }
    // Ties go to the shorter titan jump, for less fatigue on the titan.
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0).then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    scored.dedup_by_key(|s| s.2);
    scored.truncate(4);

    let name = |id: i64| graph.info_of(id).map(|i| i.name.clone()).unwrap_or_default();
    scored
        .into_iter()
        .filter_map(|(_, jump_ly, land, hop)| {
            let to_titan = gate(graph, from, land, bridges, avoid, holes)?;
            let tail = gate(graph, hop, to, bridges, avoid, holes)?;
            let lpos = pos(coords, land)?;
            let bridge_ly = pos(coords, hop).map(|h| crate::map::ly_distance(lpos, h))?;
            let mut hops = to_titan.hops;
            hops.push(named(graph, hop, 2, Some(bridge_ly)));
            hops.extend(tail.hops.into_iter().skip(1));
            let mut path = to_titan.path;
            path.push(hop);
            path.extend(tail.path.into_iter().skip(1));
            Some(RouteOption {
                label: format!("titan to {}", name(land)),
                gates: to_titan.gates + tail.gates,
                jumps: 1,
                total_ly: bridge_ly,
                note: Some(format!(
                    "Titan jumps {jump_ly:.1} ly to {}. Fleet gates {} to meet it, {bridge_ly:.1} ly to {}, then {} gates. {} instead of {baseline}.",
                    name(land),
                    to_titan.gates,
                    name(hop),
                    tail.gates,
                    to_titan.gates + tail.gates
                )),
                detour: None,
                saved: baseline.checked_sub(to_titan.gates + tail.gates),
                uses_wormhole: to_titan.uses_wormhole || tail.uses_wormhole,
                titan_jump: Some(TitanJump {
                    from,
                    to: land,
                    from_name: name(from),
                    to_name: name(land),
                    ly: jump_ly,
                }),
                path,
                hops,
            })
        })
        .collect()
}

/// Gate to a named titan, bridge as far in as range allows, gate the rest.
///
/// One option per titan that beats the plain gate route, best first. With none, the gate route comes
/// back with a note saying so.
#[allow(clippy::too_many_arguments)]
fn titan_via(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    max_ly: f64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    titans: &[i64],
) -> Vec<RouteOption> {
    let plain = gate(graph, from, to, bridges, avoid, holes);
    let baseline = plain.as_ref().map(|o| o.gates).unwrap_or(usize::MAX);
    let mut out: Vec<RouteOption> = Vec::new();
    for &t in titans {
        let Some(tpos) = pos(coords, t) else { continue };
        let Some(head) = gate(graph, from, t, bridges, avoid, holes) else { continue };
        let in_range: std::collections::HashSet<i64> = coords
            .iter()
            .filter(|s| s.id != to && crate::jumproute::jumpable(s))
            .filter(|s| !avoid.blocked(s.id))
            .filter(|s| crate::map::ly_distance(tpos, s) <= max_ly)
            .map(|s| s.id)
            .collect();
        if in_range.is_empty() {
            continue;
        }
        let Some((_, mut ring)) = graph.nearest_matching(to, TITAN_MAX_JUMPS, |id| {
            in_range.contains(&id) && !avoid.blocked(id)
        }) else {
            continue;
        };
        ring.sort_by(|a, b| {
            let d = |id: i64| {
                pos(coords, id).map(|s| crate::map::ly_distance(tpos, s)).unwrap_or(f64::MAX)
            };
            d(*a).partial_cmp(&d(*b)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(&hop) = ring.first() else { continue };
        let Some(tail) = gate(graph, hop, to, bridges, avoid, holes) else { continue };
        let ly = pos(coords, hop).map(|h| crate::map::ly_distance(tpos, h)).unwrap_or_default();
        let tname = graph.info_of(t).map(|i| i.name.clone()).unwrap_or_default();
        let hname = graph.info_of(hop).map(|i| i.name.clone()).unwrap_or_default();

        let (head_hole, tail_hole) = (head.uses_wormhole, tail.uses_wormhole);
        let mut hops = head.hops;
        let mut jump_hop = named(graph, hop, 2, Some(ly));
        jump_hop.anchor = false;
        hops.push(jump_hop);
        hops.extend(tail.hops.into_iter().skip(1));
        let mut path = head.path;
        path.push(hop);
        path.extend(tail.path.into_iter().skip(1));
        let gates = head.gates + tail.gates;
        if gates >= baseline {
            continue;
        }
        out.push(RouteOption {
            label: format!("via {tname}"),
            gates,
            jumps: 1,
            total_ly: ly,
            note: Some(format!("{} gates to {tname}, {ly:.1} ly to {hname}, {} gates in", head.gates, tail.gates)),
            detour: None,
            titan_jump: None,
            saved: baseline.checked_sub(gates),
            uses_wormhole: head_hole || tail_hole,
            path,
            hops,
        });
    }
    out.sort_by(|a, b| {
        a.gates.cmp(&b.gates).then(
            a.total_ly.partial_cmp(&b.total_ly).unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    if out.is_empty() {
        return plain
            .into_iter()
            .map(|mut o| {
                o.note = Some(
                    "No titan on the list gets you there in fewer gates. This is the gate route."
                        .to_owned(),
                );
                o
            })
            .collect();
    }
    out
}

/// A hop's `kind`, distance and fuel describe the edge *into* it, so they shift one place when the
/// hops reverse.
fn reverse(o: RouteOption) -> RouteOption {
    let n = o.hops.len();
    let mut hops: Vec<Hop> = o.hops.into_iter().rev().collect();
    let edges: Vec<(u8, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> = hops
        .iter()
        .map(|h| (h.kind, h.ly, h.fuel, h.fatigue_min, h.reactivation_min))
        .collect();
    for i in 0..n {
        let e = if i == 0 { None } else { edges.get(i - 1) };
        let (kind, ly, fuel, fat, react) = e.copied().unwrap_or((0, None, None, None, None));
        hops[i].kind = kind;
        hops[i].ly = ly;
        hops[i].fuel = fuel;
        hops[i].fatigue_min = fat;
        hops[i].reactivation_min = react;
    }
    RouteOption {
        label: o.label,
        path: o.path.into_iter().rev().collect(),
        hops,
        gates: o.gates,
        jumps: o.jumps,
        total_ly: o.total_ly,
        note: o.note,
        detour: o.detour,
        titan_jump: o.titan_jump,
        saved: o.saved,
        uses_wormhole: o.uses_wormhole,
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
                warn: h.warn,
                anchor: h.anchor,
            })
            .collect(),
        gates: o.gates,
        jumps: o.jumps,
        total_ly: o.total_ly,
        note: o.note.clone(),
        detour: o.detour.clone(),
        titan_jump: o.titan_jump.clone(),
        saved: o.saved,
        uses_wormhole: o.uses_wormhole,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> std::sync::Arc<crate::geo::Systems> {
        crate::uitest::fixtures::systems()
    }

    /// The caller draws `path`, and an empty one would look like a failure.
    #[test]
    fn a_route_to_the_same_system_is_a_single_hop() {
        let g = graph();
        let id = 30_004_759;
        let r = gate(&g, id, id, false, &Avoid::default(), &Default::default()).expect("a system can reach itself");
        assert_eq!(r.path, vec![id]);
        assert_eq!(r.jumps, 0);
        assert_eq!(r.hops.len(), 1);
    }

    /// A titan leg's options belong to the whole route, and `pick` does not reach that leg, so a
    /// per-leg switcher for it would do nothing.
    #[test]
    fn a_titan_leg_is_marked_so_its_options_are_not_offered_twice() {
        use crate::store::MapSystem;
        // A line of seven with two titans near the start reaching different systems. The shared
        // fixture is three systems wide, too small for two titan options.
        let ly = crate::map::LY_METERS;
        let ids: Vec<i64> = (0..7).map(|i| 30_100_000 + i).collect();
        let mut by_name = std::collections::HashMap::new();
        let mut adjacency = std::collections::HashMap::new();
        let mut coords: Vec<MapSystem> = Vec::new();
        for (i, &id) in ids.iter().enumerate() {
            let name = format!("S{i}");
            by_name.insert(name.clone(), crate::geo::SystemInfo {
                id,
                name: name.clone(),
                security: -0.4,
                constellation: "C".into(),
                region: "R".into(),
                faction: String::new(),
            });
            let mut near = Vec::new();
            if i > 0 {
                near.push(ids[i - 1]);
            }
            if i + 1 < ids.len() {
                near.push(ids[i + 1]);
            }
            adjacency.insert(id, near);
            coords.push(MapSystem {
                id,
                name,
                security: -0.4,
                region_id: 10_000_060,
                // S0 and S1 sit together; S4 is 4 ly out and S5 is 6 ly out, so a titan in S0
                // reaches both and one in S1 reaches only S4.
                x: match i {
                    4 => 4.0 * ly,
                    5 => 6.0 * ly,
                    6 => 20.0 * ly,
                    _ => 0.0,
                },
                y: 0.0,
                z: if i == 1 { 1.0 * ly } else { 0.0 },
                x2d: 0.0,
                z2d: 0.0,
            });
        }
        let g = crate::geo::Systems::new(by_name, adjacency);
        let (legs, out) = chain(
            &g,
            &coords,
            &[ids[0], ids[6]],
            "titan",
            &crate::jumproute::SHIP_CLASSES[1],
            5,
            5,
            6.5,
            true,
            &[ids[0], ids[1]],
            false,
            false,
            &Avoid::default(),
            &Default::default(),
            &[],
        );
        assert_eq!(out.len(), 2, "both titans beat the plain gate route, so there are two options");
        assert!(legs.last().expect("a leg").whole_route, "the titan leg is the route's own choice");

        // A gate route's legs are per-leg choices and stay switchable.
        let (gates, _) = chain(
            &g,
            &coords,
            &[ids[0], ids[3], ids[6]],
            "gate",
            &crate::jumproute::SHIP_CLASSES[1],
            5,
            5,
            6.5,
            true,
            &[],
            false,
            false,
            &Avoid::default(),
            &Default::default(),
            &[],
        );
        assert!(gates.iter().all(|l| !l.whole_route), "no gate leg is the whole route");
    }

    /// Repeating the waypoint would draw a doubled system and count an extra jump.
    #[test]
    fn a_chain_joins_at_the_waypoint_without_repeating_it() {
        let g = graph();
        let (a, b, c) = (30_004_759_i64, 30_004_608, 30_003_704);
        let direct = gate(&g, a, c, false, &Avoid::default(), &Default::default()).expect("connected");
        let (_, opts) = chain(
            &g,
            &[],
            &[a, b, c],
            "gate",
            &crate::jumproute::SHIP_CLASSES[1],
            5,
            5,
            6.0,
            true,
            &[],
            false,
            false,
            &Avoid::default(),
            &Default::default(),
            &[],
        );
        let via = opts.first().expect("a chained route");
        assert_eq!(via.path.iter().filter(|&&id| id == b).count(), 1, "the waypoint appears once");
        assert_eq!(via.hops.len(), via.path.len());
        assert!(via.jumps >= direct.jumps, "a detour is never shorter than the direct route");
    }

    /// Avoided systems are not routed through, but the endpoints are exempt.
    #[test]
    fn an_avoided_system_is_not_routed_through() {
        let g = graph();
        let (a, mid, b) = (30_004_759_i64, 30_004_608, 30_003_704);
        let direct = gate(&g, a, b, false, &Avoid::default(), &Default::default()).expect("connected");
        assert!(direct.path.contains(&mid), "the fixture route goes through the middle");
        let mut avoid = Avoid::default();
        avoid.once.insert(mid);
        assert!(gate(&g, a, b, false, &avoid, &Default::default()).is_none(), "no other way round in this fixture");
        // The endpoints stay reachable however they are listed.
        avoid.once.insert(a);
        avoid.once.insert(b);
        avoid.once.remove(&mid);
        let still = gate(&g, a, b, false, &avoid, &Default::default()).expect("endpoints are exempt");
        assert_eq!(still.path, direct.path);
    }

    /// An id missing from the graph would render as a number.
    #[test]
    fn every_hop_carries_a_name() {
        let g = graph();
        let (a, b) = (30_004_759, 30_004_608);
        let r = gate(&g, a, b, false, &Avoid::default(), &Default::default()).expect("the fixture systems are connected");
        assert!(r.hops.iter().all(|h| !h.name.is_empty() && h.name.parse::<i64>().is_err()));
        assert_eq!(r.hops.first().map(|h| h.id), Some(a));
        assert_eq!(r.hops.last().map(|h| h.id), Some(b));
        assert_eq!(r.gates, r.jumps, "no bridges in the fixture, so every hop is a gate");
    }
}
