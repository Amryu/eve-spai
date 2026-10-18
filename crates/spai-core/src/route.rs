//! Routes for the map's drag gesture: by gates, by capital jumps, or legs of both, and the titan
//! bridge the rescue planner uses. All share one shape so the page draws them with one piece of code.

use serde::Serialize;

use crate::map::MapSystem;

/// One system on a route, and how it was reached from the one before it.
#[derive(Serialize, Clone)]
pub struct Hop {
    pub id: i64,
    pub name: String,
    pub security: f64,
    /// Edge into this hop: 0 gate, 1 jump bridge, 2 capital jump. The first hop is 0.
    pub kind: u8,
    /// A kind 0 edge through a known wormhole rather than a stargate.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hole: bool,
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
    /// Every equally short way on from here, when there is more than one. The route takes the next
    /// hop; the others are the branches the user can swap to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fork: Vec<Branch>,
}

/// One way on from a system that forks. Named, because the branch the route does not take is not on
/// the path and the page has nowhere else to read its name from.
#[derive(Serialize, Clone)]
pub struct Branch {
    pub id: i64,
    pub name: String,
}

/// Which way to leave a system that forks, by system id. Empty means "whichever the search found".
pub type Picks = std::collections::HashMap<i64, i64>;

/// One leg of a route with its alternatives: same jump count, sorted by distance.
#[derive(Serialize)]
pub struct LegChoice {
    pub from: i64,
    pub to: i64,
    pub from_name: String,
    pub to_name: String,
    pub options: Vec<RouteOption>,
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

/// Walks `base` again, leaving each system the user picked a way out of by that way and taking the
/// shortest way on from there. Falls back to `base` when a pick strands the walk.
fn take_picks(
    graph: &crate::geo::Systems,
    base: Vec<i64>,
    to: i64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    picks: &Picks,
) -> Vec<i64> {
    let Some(&start) = base.first() else { return base };
    take_picks_with(graph, base, to, bridges, holes, |id| id == start || id == to || !avoid.blocked(id), picks)
}

fn take_picks_with(
    graph: &crate::geo::Systems,
    base: Vec<i64>,
    to: i64,
    bridges: bool,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    allowed: impl Fn(i64) -> bool,
    picks: &Picks,
) -> Vec<i64> {
    let Some(&start) = base.first() else { return base };
    let dist = graph.distances_to(to, true, bridges, holes, &allowed);
    let mut out = vec![start];
    let mut tail: std::collections::VecDeque<i64> = base.iter().skip(1).copied().collect();
    let mut cur = start;
    while cur != to {
        // The route can only get shorter, so a walk that does not is a bug, not a longer way round.
        if out.len() > base.len() * 4 + 8 {
            return base;
        }
        let d = dist.get(&cur).copied().unwrap_or_default();
        let pick = picks.get(&cur).copied().filter(|&n| {
            d > 0
                && dist.get(&n) == Some(&(d - 1))
                && graph.steps_from(cur, to, true, bridges, holes, &allowed).contains(&n)
        });
        let next = match pick {
            Some(p) if tail.front() != Some(&p) => {
                let Some(rest) = graph.route_with(p, to, true, bridges, holes, &allowed) else {
                    return base;
                };
                tail = rest.iter().skip(1).copied().collect();
                p
            }
            _ => match tail.pop_front() {
                Some(n) => n,
                None => return base,
            },
        };
        out.push(next);
        cur = next;
    }
    out
}

/// Marks every system a route leaves by one of several equally short ways, so the panel can offer
/// the choice and the in-game route can be pinned to the branch that was taken.
///
/// Worked out per leg: between two waypoints "equally short" means equally short to the next
/// waypoint, not to the far end of the route.
pub fn mark_forks(
    options: &mut [RouteOption],
    graph: &crate::geo::Systems,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) {
    for o in options.iter_mut() {
        for h in o.hops.iter_mut() {
            h.fork.clear();
        }
        let mut seg_start = 0usize;
        for i in 1..o.path.len() {
            // A leg ends at a waypoint, and a jump is not a fork: nothing branches off a bridge.
            let jumped = o.hops.get(i).is_some_and(|h| h.kind == 2);
            let ends = jumped || o.hops.get(i).is_some_and(|h| h.anchor) || i + 1 == o.path.len();
            if !ends {
                continue;
            }
            let leg_end = if jumped { i - 1 } else { i };
            if leg_end > seg_start {
                mark_leg(o, seg_start, leg_end, graph, bridges, avoid, holes);
            }
            seg_start = i;
        }
    }
}

fn mark_leg(
    o: &mut RouteOption,
    start: usize,
    end: usize,
    graph: &crate::geo::Systems,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) {
    let (from, to) = (o.path[start], o.path[end]);
    let allowed = |id: i64| id == from || id == to || !avoid.blocked(id);
    let dist = graph.distances_to(to, true, bridges, holes, &allowed);
    for i in start..end {
        let id = o.path[i];
        let Some(&d) = dist.get(&id) else { continue };
        if d == 0 {
            continue;
        }
        let mut choices: Vec<Branch> = graph
            .steps_from(id, to, true, bridges, holes, &allowed)
            .into_iter()
            .filter(|n| dist.get(n) == Some(&(d - 1)))
            .map(|n| Branch {
                id: n,
                name: graph.info_of(n).map(|i| i.name.clone()).unwrap_or_else(|| n.to_string()),
            })
            .collect();
        if choices.len() < 2 {
            continue;
        }
        choices.sort_by(|a, b| a.name.cmp(&b.name));
        o.hops[i].fork = choices;
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

/// The waypoints to push into the game for a planned route.
///
/// A gate route is flown by the autopilot, so every system on it becomes a waypoint and the in-game
/// route matches the planned one. A route with a bridge or a capital jump cannot be: those legs are
/// flown by hand, so only the systems on either side of them are set, which is where the pilot needs
/// to be, along with the waypoints the user named and the branches they picked. The start is included
/// when the character is somewhere else, since the route only makes sense from there.
pub fn ingame_waypoints(opt: &RouteOption, player: Option<i64>) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    let Some(&start) = opt.path.first() else { return out };
    if player != Some(start) {
        out.push(start);
    }
    let flown = opt.hops.iter().all(|h| h.kind == 0);
    if flown {
        out.extend(opt.path.iter().skip(1).copied());
    } else {
        for (i, h) in opt.hops.iter().enumerate() {
            // A system the user named is a waypoint in the game too, or a route planned through one
            // arrives there by whatever way the game likes, or not at all.
            if h.anchor && i > 0 && out.last() != Some(&h.id) {
                out.push(h.id);
            }
            // A fork the autopilot would take the other way round needs the branch pinned, or the
            // game flies its own equally short route instead of the one on screen.
            if !h.fork.is_empty() {
                if let Some(next) = opt.path.get(i + 1) {
                    out.push(*next);
                }
            }
            if h.kind != 0 {
                if let Some(prev) = opt.hops.get(i.wrapping_sub(1)) {
                    if out.last() != Some(&prev.id) {
                        out.push(prev.id);
                    }
                }
                out.push(h.id);
            }
        }
        if let Some(&last) = opt.path.last() {
            out.push(last);
        }
    }
    out.dedup();
    // A waypoint where the character already sits is completed the moment it is set.
    if out.first() == player.as_ref() {
        out.remove(0);
    }
    out
}

#[derive(Serialize, Clone)]
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
    /// A jump-ahead plan whose capital gates before it jumps, offered beside the ones that do not.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gated: bool,
    /// The capital's own gates to where it jumps from, one run of systems per time it gates: before
    /// the first jump and between later ones. The hops are the fleet's way, which may take
    /// Ansiblexes and holes the capital cannot.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub capital_gates: Vec<Vec<Branch>>,
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
        hole: false,
        ly,
        fuel: None,
        fatigue_min: None,
        reactivation_min: None,
        warn: None,
        anchor: false,
        fork: Vec::new(),
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
    gate_via(graph, from, to, bridges, avoid, holes, &Picks::new())
}

/// A gate route that leaves the systems in `picks` the way the user asked, staying as short as the
/// search's own answer: a pick is only honoured when it is one of the equally short ways on.
pub fn gate_via(
    graph: &crate::geo::Systems,
    from: i64,
    to: i64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    picks: &Picks,
) -> Option<RouteOption> {
    // Endpoints are exempt, or avoiding the system you stand in would yield no route at all.
    // Wormholes arrive as extra edges because that is how the app's own planner takes them.
    let path = graph.route_with(from, to, true, bridges, holes, |id| {
        id == from || id == to || !avoid.blocked(id)
    })?;
    let path = if picks.is_empty() {
        path
    } else {
        take_picks(graph, path, to, bridges, avoid, holes, picks)
    };
    Some(gate_option(graph, path))
}

fn gate_option(graph: &crate::geo::Systems, path: Vec<i64>) -> RouteOption {
    let hops: Vec<Hop> = path
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let kind = if i > 0 && graph.is_bridge(path[i - 1], id) { 1 } else { 0 };
            let mut h = named(graph, id, kind, None);
            h.hole = kind == 0 && i > 0 && graph.is_hole_step(path[i - 1], id);
            h
        })
        .collect();
    let gates = hops.iter().skip(1).filter(|h| h.kind == 0 && !h.hole).count();
    // A step that is not in the graph's own adjacency is one the hole map let through.
    let uses_wormhole = path.windows(2).any(|w| graph.is_hole_step(w[0], w[1]));
    RouteOption {
        uses_wormhole,
        gated: false,
        capital_gates: Vec::new(),
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
    }
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
    Some(jump_option(graph, coords, path, class, jfc))
}

fn jump_option(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    path: Vec<i64>,
    class: &crate::jumproute::ShipClass,
    jfc: u32,
) -> RouteOption {
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
    RouteOption {
        label: format!("{total_ly:.1} ly"),
        gates: 0,
        jumps: path.len().saturating_sub(1),
        total_ly,
        detour: None,
        titan_jump: None,
        saved: None,
        uses_wormhole: false,
        gated: false,
        capital_gates: Vec::new(),
        note: Some(format!(
            "{} isotopes · {:.0} min fatigue at the end",
            (fuel.round() as i64).to_string(),
            costs.last().map(|c| c.fatigue_min).unwrap_or_default()
        )),
        path,
        hops,
    }
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
    let (Some(start), Some(end)) = (pos(coords, from), pos(coords, to)) else {
        return Vec::new();
    };
    // In range, one jump is the whole route: nothing to gate, and no reason to move the titan.
    let ly = crate::map::ly_distance(start, end);
    if ly <= max_ly && crate::jumproute::jumpable(end) && !avoid.blocked(to) {
        let name = graph.info_of(to).map(|i| i.name.clone()).unwrap_or_default();
        return vec![RouteOption {
            label: format!("direct to {name}"),
            gates: 0,
            jumps: 1,
            total_ly: ly,
            note: Some(format!("{ly:.1} ly direct jump")),
            detour: None,
            uses_wormhole: false,
            gated: false,
            capital_gates: Vec::new(),
            titan_jump: None,
            saved: None,
            path: vec![from, to],
            hops: vec![named(graph, from, 0, None), named(graph, to, 2, Some(ly))],
        }];
    }
    let stay = titan_stationary(graph, coords, from, to, start, max_ly, bridges, avoid, holes);
    // A titan with the fleet may reposition first, but only for a real gain: its drive, fuel and
    // fatigue are not free, so a gate or so saved is not worth it.
    if self_jump {
        let plain = gate(graph, from, to, bridges, avoid, holes).map_or(usize::MAX, |o| o.gates);
        let beat = stay.iter().map(|o| o.gates).chain([plain]).min().unwrap_or(usize::MAX);
        let mut opts = titan_self_jump(graph, coords, from, to, max_ly, bridges, avoid, holes, beat);
        if !opts.is_empty() {
            opts.extend(stay);
            return opts;
        }
    }
    stay
}

/// The titan bridges from where it sits: the reachable system fewest gates from the target.
#[allow(clippy::too_many_arguments)]
fn titan_stationary(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    start: &MapSystem,
    max_ly: f64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) -> Vec<RouteOption> {
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
                gated: false,
                capital_gates: Vec::new(),
                titan_jump: None,
                saved: None,
                path,
                hops,
            })
        })
        .collect()
}

#[derive(Default, Clone)]
pub struct Ahead {
    pub pre_gates: u32,
    /// Zero is read as one.
    pub max_jumps: u32,
    /// Cyno beacons the landings are limited to, the destination aside. `None`: anywhere.
    pub beacons: Option<std::collections::HashSet<i64>>,
    /// Docking systems, preferred as landings. Empty for a hull that docks anywhere.
    pub prefer: std::collections::HashSet<i64>,
    /// Black ops may gate through high sec; capitals cannot.
    pub highsec: bool,
    /// Black ops may take Ansiblexes before the jump; capitals cannot.
    pub pre_bridges: bool,
    /// The holes the hull fits through before the jump: XL for a carrier, none for a supercapital.
    pub pre_holes: std::collections::HashMap<i64, Vec<i64>>,
}

pub const AHEAD_OPTIONS: usize = 5;

/// How far a jump-ahead capital gates before jumping in the plans offered beside the ones it does
/// not, when the user asked for no gating.
pub const GATED_PLAN_GATES: u32 = 5;

/// A gate, an Ansiblex and a hole are one step each for the fleet; the capital's jumps are not.
fn fleet_steps(o: &RouteOption) -> usize {
    o.hops.iter().skip(1).filter(|h| h.kind != 2).count()
}

/// [`jump_ahead_plans`], and with no gating asked for, also the plans that gate the capital up to
/// [`GATED_PLAN_GATES`] first and save the fleet steps for it, marked `gated`.
#[allow(clippy::too_many_arguments)]
pub fn jump_ahead(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
    bridges: bool,
    avoid_gate: &Avoid,
    avoid_jump: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    ahead: &Ahead,
    picks: &Picks,
) -> Vec<RouteOption> {
    let plan = |a: &Ahead| jump_ahead_plans(graph, coords, from, to, class, jdc, jfc, bridges, avoid_gate, avoid_jump, holes, a, picks);
    let mut out = plan(ahead);
    if ahead.pre_gates > 0 {
        return out;
    }
    let jumps = |o: &RouteOption| o.hops.iter().any(|h| h.kind == 2);
    let best = out.iter().filter(|o| jumps(o)).map(fleet_steps).min();
    let wider = Ahead { pre_gates: GATED_PLAN_GATES, ..ahead.clone() };
    out.extend(
        plan(&wider)
            .into_iter()
            .filter(|o| jumps(o) && !o.capital_gates.is_empty() && best.is_none_or(|b| fleet_steps(o) < b))
            .map(|mut o| {
                o.gated = true;
                o
            }),
    );
    out
}

/// A capital gates up to `pre_gates`, then jumps up to `max_jumps` times with the fleet riding
/// each jump by bridge or conduit; the fleet gates the rest. Ranked by fleet gates, then jumps,
/// then docking, then light years, one plan per landing.
#[allow(clippy::too_many_arguments)]
fn jump_ahead_plans(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    from: i64,
    to: i64,
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
    bridges: bool,
    avoid_gate: &Avoid,
    avoid_jump: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    ahead: &Ahead,
    picks: &Picks,
) -> Vec<RouteOption> {
    let max_ly = crate::jumproute::max_range_ly(class, jdc);
    let name = |id: i64| graph.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string());
    let (Some(start), Some(end)) = (pos(coords, from), pos(coords, to)) else {
        return Vec::new();
    };
    if from == to {
        return Vec::new();
    }
    let plain = gate(graph, from, to, bridges, avoid_gate, holes).map(|o| fleet_steps(&o));
    if crate::jumproute::jumpable(start)
        && crate::jumproute::jumpable(end)
        && crate::map::ly_distance(start, end) <= max_ly
    {
        let mut o = jump_option(graph, coords, vec![from, to], class, jfc);
        o.label = format!("direct to {}", name(to));
        o.saved = plain;
        return vec![o];
    }

    let (pre_bridges, pre_holes) = (ahead.pre_bridges && bridges, &ahead.pre_holes);
    let cap_ok = |id: i64| {
        !avoid_gate.blocked(id)
            && (ahead.highsec || graph.info_of(id).is_some_and(|i| crate::jumproute::cyno_able(i.security)))
    };
    let mut launches: std::collections::HashMap<i64, u32> = std::collections::HashMap::from([(from, 0)]);
    let mut ring = vec![from];
    for d in 1..=ahead.pre_gates {
        let mut next = Vec::new();
        for &sys in &ring {
            for n in graph.steps_from(sys, from, true, pre_bridges, pre_holes, cap_ok) {
                if let std::collections::hash_map::Entry::Vacant(e) = launches.entry(n) {
                    e.insert(d);
                    next.push(n);
                }
            }
        }
        ring = next;
    }
    let land = |s: &MapSystem| {
        s.id == to || (!avoid_jump.blocked(s.id) && ahead.beacons.as_ref().is_none_or(|b| b.contains(&s.id)))
    };
    // Where the capital waits for the fleet: a system it gated to, or one it jumped to alone from
    // there. Per system the fewest gates, then no jump of its own, then the shortest one.
    let mut stands: std::collections::HashMap<i64, (u32, Option<(i64, f64)>)> =
        launches.iter().map(|(&id, &d)| (id, (d, None))).collect();
    let better = |a: &(u32, Option<(i64, f64)>), b: &(u32, Option<(i64, f64)>)| {
        let key = |s: &(u32, Option<(i64, f64)>)| (s.0, s.1.is_some(), s.1.map_or(0.0, |v| v.1));
        let (ka, kb) = (key(a), key(b));
        (ka.0, ka.1).cmp(&(kb.0, kb.1)).then(ka.2.total_cmp(&kb.2)).is_lt()
    };
    for (&l, &d) in &launches {
        let Some(lp) = pos(coords, l).filter(|p| crate::jumproute::jumpable(p)) else { continue };
        for x in coords {
            if x.id == l || x.id == to || !crate::jumproute::jumpable(x) || !land(x) {
                continue;
            }
            let ly = crate::map::ly_distance(lp, x);
            if ly > max_ly {
                continue;
            }
            let cand = (d, Some((l, ly)));
            if stands.get(&x.id).is_none_or(|cur| better(&cand, cur)) {
                stands.insert(x.id, cand);
            }
        }
    }
    let gate_ok = |id: i64| !avoid_gate.blocked(id);
    // The fleet goes to the capital its own way, and that is what a plan costs it.
    let mut fleet_to: std::collections::HashMap<i64, u32> = std::collections::HashMap::from([(from, 0)]);
    let mut ring = vec![from];
    let mut d = 0;
    while !ring.is_empty() && stands.keys().any(|l| !fleet_to.contains_key(l)) {
        d += 1;
        let mut next = Vec::new();
        for &sys in &ring {
            for n in graph.steps_from(sys, from, true, bridges, holes, gate_ok) {
                if let std::collections::hash_map::Entry::Vacant(e) = fleet_to.entry(n) {
                    e.insert(d);
                    next.push(n);
                }
            }
        }
        ring = next;
    }
    let sources: Vec<(i64, u32)> = stands.keys().filter_map(|&id| Some((id, *fleet_to.get(&id)?))).collect();
    let mut prefer = ahead.prefer.clone();
    if !prefer.is_empty() {
        prefer.insert(to);
    }
    // Between jumps the capital may gate again, up to as many as before the first, the fleet with it
    // its own way.
    let between = |x: i64| -> Vec<(i64, u32)> {
        let ball = |rules_bridges: bool, rules_holes: &std::collections::HashMap<i64, Vec<i64>>, ok: &dyn Fn(i64) -> bool| {
            let mut seen: std::collections::HashMap<i64, u32> = std::collections::HashMap::from([(x, 0)]);
            let mut ring = vec![x];
            for d in 1..=ahead.pre_gates {
                let mut next = Vec::new();
                for &sys in &ring {
                    for n in graph.steps_from(sys, x, true, rules_bridges, rules_holes, ok) {
                        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(n) {
                            e.insert(d);
                            next.push(n);
                        }
                    }
                }
                ring = next;
            }
            seen
        };
        let capital = ball(pre_bridges, pre_holes, &cap_ok);
        let fleet = ball(bridges, holes, &gate_ok);
        capital.keys().filter_map(|l| Some((*l, *fleet.get(l)?))).collect()
    };
    let reached = crate::jumproute::reach(coords, max_ly, ahead.max_jumps.max(1), &sources, land, &prefer, between);
    let left = graph.distances_to(to, true, bridges, holes, gate_ok);
    // A landing is exempt from the gate avoid list, like a gate route's start.
    let tail_of = |id: i64| -> Option<u32> {
        if id == to {
            return Some(0);
        }
        graph.steps_from(id, to, true, bridges, holes, gate_ok).into_iter().filter_map(|n| left.get(&n)).min().map(|d| d + 1)
    };

    let mut ranked: Vec<(u32, u32, u32, f64, i64)> = reached
        .iter()
        // The capital could have waited there, for the fleet no more than the launch.
        .filter(|(id, r)| !stands.contains_key(id) || fleet_to.get(id).is_none_or(|&p| p > r.pre))
        .filter_map(|(&id, r)| Some((r.pre + tail_of(id)?, r.jumps, r.off_pref, r.ly, id)))
        .collect();
    let order = |a: &(u32, u32, u32, f64, i64), b: &(u32, u32, u32, f64, i64)| {
        (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)).then(a.3.total_cmp(&b.3)).then(a.4.cmp(&b.4))
    };
    ranked.sort_by(order);
    // Spares for the re-rank on the built routes, whose shortest paths can differ from the search's.
    ranked.truncate(AHEAD_OPTIONS * 3);

    let mut out: Vec<(RouteOption, u32, f64)> = ranked
        .into_iter()
        .filter_map(|(_, _, off_pref, _, landing)| {
            let legs = reached.get(&landing)?.legs.clone();
            let launch = legs[0].0;
            let (_, solo) = *stands.get(&launch)?;
            let gated_to = solo.map_or(launch, |(l, _)| l);
            let capital = graph.route_with(from, gated_to, true, pre_bridges, pre_holes, cap_ok)?;
            // The fleet's way from `a` to the capital at `b`, as the capital's own way if it gates.
            let fleet_leg = |a: i64, b: i64| -> Option<RouteOption> {
                let ok = |id: i64| id == a || id == b || gate_ok(id);
                let p = graph.route_with(a, b, true, bridges, holes, ok)?;
                let p = if picks.is_empty() { p } else { take_picks_with(graph, p, b, bridges, holes, ok, picks) };
                Some(gate_option(graph, p))
            };
            let mut o = fleet_leg(from, launch)?;
            let pre = fleet_steps(&o);
            let mut note = String::new();
            let mut capital_gates: Vec<Vec<Branch>> = Vec::new();
            let run = |p: &[i64]| p.iter().map(|&id| Branch { id, name: name(id) }).collect::<Vec<_>>();
            if capital.len() > 1 {
                note.push_str(&format!("Capital gates {} to {}. ", capital.len() - 1, name(gated_to)));
                capital_gates.push(run(&capital));
            }
            if let Some((_, ly)) = solo {
                note.push_str(&format!("Capital jumps alone {ly:.1} ly to {}. ", name(launch)));
            }
            if launch != from {
                note.push_str(&format!("Fleet {pre} to {}. ", name(launch)));
            }
            let mut between_steps = 0;
            let mut ly = 0.0;
            for (k, &(l, y)) in legs.iter().enumerate() {
                if k > 0 {
                    let at = legs[k - 1].1;
                    if l != at {
                        let cap = graph.route_with(at, l, true, pre_bridges, pre_holes, |id| id == l || cap_ok(id))?;
                        let leg = fleet_leg(at, l)?;
                        between_steps += fleet_steps(&leg);
                        note.push_str(&format!("Capital gates {} to {}, fleet {}. ", cap.len() - 1, name(l), fleet_steps(&leg)));
                        capital_gates.push(run(&cap));
                        o = join(o, leg);
                    }
                }
                let j = jump_option(graph, coords, vec![l, y], class, jfc);
                ly += j.total_ly;
                note.push_str(&format!("Jump {:.1} ly to {}. ", j.total_ly, name(y)));
                o = join(o, j);
            }
            let tail = if landing == to { None } else { Some(gate_via(graph, landing, to, bridges, avoid_gate, holes, picks)?) };
            let post = tail.as_ref().map_or(0, fleet_steps);
            if let Some(t) = tail {
                o = join(o, t);
                note.push_str(&format!("Then {post} gates."));
            }
            let pre = pre + between_steps;
            o.note = Some(note.trim_end().to_owned());
            o.capital_gates = capital_gates;
            o.titan_jump = solo.map(|(l, sly)| TitanJump { from: l, to: launch, from_name: name(l), to_name: name(launch), ly: sly });
            o.label = if launch != from { format!("via {} from {}", name(landing), name(launch)) } else { format!("via {}", name(landing)) };
            o.saved = plain.and_then(|p| p.checked_sub(pre + post));
            if plain.is_some_and(|p| pre + post >= p) {
                return None;
            }
            Some((o, off_pref, ly))
        })
        .collect();
    let capital = |o: &RouteOption| o.hops.iter().filter(|h| h.kind == 2).count();
    // On equal fleet steps, the plan where the capital moves less on its own.
    let alone = |o: &RouteOption| o.capital_gates.iter().map(|g| g.len().saturating_sub(1)).sum::<usize>() + usize::from(o.titan_jump.is_some());
    out.sort_by(|(a, ap, al), (b, bp, bl)| {
        (fleet_steps(a), capital(a), alone(a), ap).cmp(&(fleet_steps(b), capital(b), alone(b), bp)).then(al.total_cmp(bl))
    });
    out.truncate(AHEAD_OPTIONS);
    if out.is_empty() {
        if let Some(mut g) = gate(graph, from, to, bridges, avoid_gate, holes) {
            g.note = Some("No jump saves a gate. This is the gate route.".to_owned());
            return vec![g];
        }
    }
    out.into_iter().map(|(o, _, _)| o).collect()
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
        gated: head.gated || tail.gated,
        capital_gates: head.capital_gates.into_iter().chain(tail.capital_gates).collect(),
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
    picks: &Picks,
) -> Vec<RouteOption> {
    let one = |extra: &Avoid| -> Option<RouteOption> {
        match kind {
            "jump" => jump(graph, coords, a, b, class, jdc, jfc, extra),
            _ => gate_via(graph, a, b, bridges, extra, holes, picks),
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

/// The kind of leg `i` of a route whose legs are `kinds`: "gate" or "jump". Legs past the end of
/// the list take the last one's kind, so one entry describes a whole plain route.
pub fn leg_kind<'a>(kinds: &[&'a str], i: usize) -> &'a str {
    kinds.get(i).or(kinds.last()).copied().unwrap_or("gate")
}

/// A route through waypoints, each leg flown by gate or by jump as `kinds` says (a mixed route has
/// both). `pick` selects each leg's alternative, and the assembled route comes back with the choices
/// so the client never joins legs itself. Gate legs avoid `avoid_gate`, jump legs `avoid_jump`.
#[allow(clippy::too_many_arguments)]
pub fn chain(
    graph: &crate::geo::Systems,
    coords: &[MapSystem],
    anchors: &[i64],
    kinds: &[&str],
    class: &crate::jumproute::ShipClass,
    jdc: u32,
    jfc: u32,
    bridges: bool,
    avoid_gate: &Avoid,
    avoid_jump: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
    pick: &[usize],
    picks: &Picks,
    ahead: &Ahead,
) -> (Vec<LegChoice>, Vec<RouteOption>) {
    if anchors.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let name = |id: i64| {
        graph.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string())
    };
    let mut legs: Vec<LegChoice> = Vec::new();
    for (i, w) in anchors.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let kind = leg_kind(kinds, i);
        let options = if kind == "ahead" {
            let first = !kinds.iter().take(i).any(|k| *k == "ahead");
            let leg = Ahead { pre_gates: if first { ahead.pre_gates } else { 0 }, ..ahead.clone() };
            jump_ahead(graph, coords, a, b, class, jdc, jfc, bridges, avoid_gate, avoid_jump, holes, &leg, picks)
        } else {
            let avoid = if kind == "jump" { avoid_jump } else { avoid_gate };
            leg_options(graph, coords, a, b, kind, class, jdc, jfc, bridges, avoid, holes, picks)
        };
        legs.push(LegChoice { from: a, to: b, from_name: name(a), to_name: name(b), options });
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
        let mut o = acc.expect("at least one leg");
        recost_fatigue(&mut o, class);
        o
    };
    let picked = |i: usize| pick.get(i).copied().unwrap_or(0);
    // Jump-ahead plans differ in cost, so they are the route's options, not forks.
    let ahead_leg = (0..legs.len()).find(|&i| leg_kind(kinds, i) == "ahead" && legs[i].options.len() > 1);
    let out = match ahead_leg {
        // Named and marked by the jump-ahead leg's plan: joined, the route would take the last leg's.
        Some(at) => (0..legs[at].options.len())
            .map(|k| {
                let mut o = assemble(&|i| if i == at { k } else { picked(i) });
                o.label = legs[at].options[k].label.clone();
                o.gated = legs[at].options[k].gated;
                o
            })
            .collect(),
        None => vec![assemble(&picked)],
    };
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
    baseline: usize,
) -> Vec<RouteOption> {
    /// Past this many gates to meet the titan it is no longer a shortcut.
    const MAX_GATE_TO_TITAN: u32 = 8;
    /// Gates a move has to save over not moving at all.
    const MIN_GAIN: usize = 2;

    let Some(start) = pos(coords, from) else {
        return Vec::new();
    };
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
        if total + MIN_GAIN > baseline {
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
                gated: false,
                capital_gates: Vec::new(),
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
            gated: false,
            capital_gates: Vec::new(),
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

/// Fatigue carries across the whole route, not across one leg. Each leg is planned on its own and
/// starts from nothing, so an assembled route has to be re-costed or every waypoint reads as a
/// fresh capital with no fatigue.
fn recost_fatigue(o: &mut RouteOption, class: &crate::jumproute::ShipClass) {
    let mut fatigue = 0.0;
    for h in &mut o.hops {
        // Jump hops only: gates and bridges carry no fatigue.
        let (Some(ly), 2) = (h.ly, h.kind) else { continue };
        let reactivation;
        (fatigue, reactivation) = crate::jumproute::next_fatigue(fatigue, ly, class);
        h.fatigue_min = Some(fatigue);
        h.reactivation_min = Some(reactivation);
    }
    // The note came from the last leg, so it quotes that leg's fuel and fatigue, not the route's.
    if fatigue > 0.0 && o.note.as_ref().is_some_and(|n| n.contains("min fatigue at the end")) {
        let fuel: f64 = o.hops.iter().filter_map(|h| h.fuel).sum();
        o.note = Some(format!("{} isotopes · {fatigue:.0} min fatigue at the end", fuel.round() as i64));
    }
}

/// A hop's `kind`, distance and fuel describe the edge *into* it, so they shift one place when the
/// hops reverse.
fn reverse(o: RouteOption) -> RouteOption {
    let n = o.hops.len();
    let mut hops: Vec<Hop> = o.hops.into_iter().rev().collect();
    let edges: Vec<(u8, bool, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> = hops
        .iter()
        .map(|h| (h.kind, h.hole, h.ly, h.fuel, h.fatigue_min, h.reactivation_min))
        .collect();
    for i in 0..n {
        let e = if i == 0 { None } else { edges.get(i - 1) };
        let (kind, hole, ly, fuel, fat, react) = e.copied().unwrap_or((0, false, None, None, None, None));
        hops[i].kind = kind;
        hops[i].hole = hole;
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
        gated: o.gated,
        capital_gates: o.capital_gates.clone(),
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
                hole: h.hole,
                ly: h.ly,
                fuel: h.fuel,
                fatigue_min: h.fatigue_min,
                reactivation_min: h.reactivation_min,
                warn: h.warn,
                anchor: h.anchor,
                fork: h.fork.clone(),
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
        gated: o.gated,
        capital_gates: o.capital_gates.clone(),
    }
}

#[cfg(test)]
mod tests {

    fn opt(path: &[i64], kinds: &[u8]) -> RouteOption {
        RouteOption {
            label: String::new(),
            path: path.to_vec(),
            hops: path
                .iter()
                .zip(kinds)
                .map(|(id, kind)| Hop {
                    id: *id,
                    name: format!("S{id}"),
                    security: 0.0,
                    kind: *kind,
                    hole: false,
                    ly: None,
                    fuel: None,
                    fatigue_min: None,
                    reactivation_min: None,
                    warn: None,
                    anchor: false,
                    fork: Vec::new(),
                })
                .collect(),
            gates: 0,
            jumps: 0,
            total_ly: 0.0,
            note: None,
            detour: None,
            titan_jump: None,
            saved: None,
            uses_wormhole: false,
            gated: false,
            capital_gates: Vec::new(),
        }
    }

    /// The autopilot can fly every leg of a gate route, so it gets the whole path.
    #[test]
    fn a_gate_route_sets_every_system() {
        let o = opt(&[1, 2, 3, 4], &[0, 0, 0, 0]);
        assert_eq!(ingame_waypoints(&o, Some(1)), vec![2, 3, 4]);
    }

    /// A route planned from somewhere else is only flyable from its start, so that is the first stop.
    #[test]
    fn a_route_that_starts_elsewhere_leads_with_its_start() {
        let o = opt(&[1, 2, 3], &[0, 0, 0]);
        assert_eq!(ingame_waypoints(&o, Some(9)), vec![1, 2, 3]);
        assert_eq!(ingame_waypoints(&o, None), vec![1, 2, 3]);
    }

    /// The bridge is flown by hand: the waypoints are where to be before it and where to carry on.
    #[test]
    fn a_bridged_route_sets_only_the_ends_of_the_legs_it_cannot_fly() {
        let o = opt(&[1, 2, 3, 4, 5], &[0, 0, 2, 0, 0]);
        assert_eq!(ingame_waypoints(&o, Some(1)), vec![2, 3, 5]);
        assert_eq!(ingame_waypoints(&o, Some(7)), vec![1, 2, 3, 5]);
    }

    /// The waypoint the user named is a waypoint in the game too, even on a route the autopilot
    /// cannot fly end to end.
    #[test]
    fn a_named_waypoint_is_pinned_on_a_bridged_route() {
        // The waypoint sits well away from the bridge, which is the case that tells a pinned
        // waypoint apart from a system that is pinned for being one end of a leg.
        let mut o = opt(&[1, 2, 3, 4, 5, 6, 7], &[0, 0, 0, 0, 0, 1, 0]);
        o.hops[0].anchor = true;
        o.hops[2].anchor = true;
        o.hops[6].anchor = true;
        assert_eq!(ingame_waypoints(&o, Some(1)), vec![3, 5, 6, 7]);
        assert_eq!(ingame_waypoints(&o, Some(9)), vec![1, 3, 5, 6, 7], "planned from elsewhere");
    }

    /// A bridge straight off the start still names the start, since the jump begins there.
    #[test]
    fn a_bridge_on_the_first_leg_keeps_both_of_its_ends() {
        let o = opt(&[1, 2, 3], &[0, 1, 0]);
        assert_eq!(ingame_waypoints(&o, Some(1)), vec![2, 3], "no waypoint on the system it starts in");
        assert_eq!(ingame_waypoints(&o, Some(9)), vec![1, 2, 3]);
    }

    #[test]
    fn an_empty_route_sets_nothing() {
        assert!(ingame_waypoints(&opt(&[], &[]), Some(1)).is_empty());
    }
    use super::*;

    fn graph() -> std::sync::Arc<crate::geo::Systems> {
        std::sync::Arc::new(crate::test_support::small_universe(&[]))
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

    /// Two ways of the same length from 1 to 4, and one way on to 5.
    fn diamond() -> crate::geo::Systems {
        let mut by_name = std::collections::HashMap::new();
        let mut adjacency: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        for id in 1..=5i64 {
            by_name.insert(format!("S{id}"), crate::geo::SystemInfo {
                id,
                name: format!("S{id}"),
                security: -0.4,
                constellation: "C".into(),
                region: "R".into(),
                faction: String::new(),
            });
        }
        for (a, b) in [(1, 2), (1, 3), (2, 4), (3, 4), (4, 5)] {
            adjacency.entry(a).or_default().push(b);
            adjacency.entry(b).or_default().push(a);
        }
        crate::geo::Systems::new(by_name, adjacency)
    }

    /// Both ways out of the start are three jumps, so the start is a fork and nothing else is.
    #[test]
    fn a_system_with_two_equally_short_ways_on_is_a_fork() {
        let g = diamond();
        let mut opts =
            vec![gate(&g, 1, 5, false, &Avoid::default(), &Default::default()).expect("connected")];
        mark_forks(&mut opts, &g, false, &Avoid::default(), &Default::default());
        let ids: Vec<i64> = opts[0].hops[0].fork.iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![2, 3], "the fork is the system the ways part at");
        assert!(opts[0].hops[1].fork.is_empty(), "one way on is not a choice");
        assert!(opts[0].hops.last().expect("hops").fork.is_empty(), "the end goes nowhere");
    }

    /// The pick is honoured, and a pick that is not a way on is ignored rather than obeyed.
    #[test]
    fn a_picked_branch_is_the_one_the_route_takes() {
        let g = diamond();
        let route = |picks: &Picks| {
            gate_via(&g, 1, 5, false, &Avoid::default(), &Default::default(), picks)
                .expect("connected")
                .path
        };
        assert_eq!(route(&Picks::from([(1, 3)])), vec![1, 3, 4, 5]);
        assert_eq!(route(&Picks::from([(1, 2)])), vec![1, 2, 4, 5]);
        let long_way = route(&Picks::from([(1, 5)]));
        assert_eq!(long_way.len(), 4, "a pick may not make the route longer");
        assert_eq!(route(&Picks::from([(9, 9)])), route(&Picks::new()), "unknown systems do nothing");
    }

    /// The autopilot would pick its own way through a fork, so the branch that was taken is a
    /// waypoint even on a route that is otherwise only pinned at the ends of its legs.
    #[test]
    fn a_fork_pins_its_branch_in_the_game() {
        let mut o = opt(&[1, 2, 3, 4, 5], &[0, 0, 0, 2, 0]);
        o.hops[0].fork =
            vec![Branch { id: 2, name: "S2".into() }, Branch { id: 7, name: "S7".into() }];
        assert_eq!(ingame_waypoints(&o, Some(1)), vec![2, 3, 4, 5]);
    }

    /// The page has its own copy of the waypoint rule, because it pushes the route itself. This runs
    /// both on the same routes and fails when they disagree: a page that pins a different set wipes
    /// the route in the game and says nothing.
    #[test]
    fn the_page_pins_the_same_waypoints_as_the_app() {
        if std::process::Command::new("node").arg("--version").output().is_err() {
            eprintln!("node is not installed, skipping the page's half of the waypoint rule");
            return;
        }
        let fork = |a: i64, b: i64| vec![
            Branch { id: a, name: format!("S{a}") },
            Branch { id: b, name: format!("S{b}") },
        ];
        let mut bridged = opt(&[1, 2, 3, 4, 5, 6, 7], &[0, 0, 0, 0, 2, 0, 0]);
        bridged.hops[0].anchor = true;
        bridged.hops[3].anchor = true;
        bridged.hops[6].anchor = true;
        bridged.hops[1].fork = fork(3, 8);
        let mut gates = opt(&[1, 2, 3, 4], &[0, 0, 0, 0]);
        gates.hops[2].anchor = true;
        let mut ansiblex = opt(&[1, 2, 3, 4, 5], &[0, 1, 0, 1, 0]);
        ansiblex.hops[2].anchor = true;
        // A waypoint nowhere near the bridge: the case that separates a pinned waypoint from a
        // system pinned for being the end of a leg.
        let mut far = opt(&[1, 2, 3, 4, 5, 6, 7], &[0, 0, 0, 0, 0, 1, 0]);
        far.hops[0].anchor = true;
        far.hops[2].anchor = true;
        far.hops[6].anchor = true;
        let cases: Vec<(RouteOption, Option<i64>)> = vec![
            (bridged, Some(1)),
            (gates, Some(1)),
            (ansiblex, Some(9)),
            (far, Some(1)),
            (opt(&[], &[]), Some(1)),
            (opt(&[1], &[0]), Some(1)),
        ];
        let mine: Vec<Vec<i64>> =
            cases.iter().map(|(o, p)| ingame_waypoints(o, *p)).collect();

        let dir = std::env::temp_dir().join(format!("eve-spai-waypoints-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        std::fs::write(dir.join("waypoints.js"), include_str!("../../../app/src/web/assets/waypoints.js")).expect("module");
        std::fs::write(
            dir.join("run.mjs"),
            r#"import { ingameWaypoints } from "./waypoints.js";
const cases = JSON.parse(process.argv[2]);
console.log(JSON.stringify(cases.map(([o, p]) => ingameWaypoints(o, p))));
"#,
        )
        .expect("runner");
        let payload = serde_json::to_string(&cases).expect("cases");
        let out = std::process::Command::new("node")
            .arg(dir.join("run.mjs"))
            .arg(&payload)
            .output()
            .expect("node ran");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "node failed: {}", String::from_utf8_lossy(&out.stderr));
        let theirs: Vec<Vec<i64>> =
            serde_json::from_slice(&out.stdout).expect("the page returned waypoint lists");
        assert_eq!(theirs, mine, "the page and the app pin different waypoints");
    }

    /// A line of ten systems with a jump bridge from 2 to 9, which makes the way back through the
    /// bridge much shorter than the way along the line.
    fn line_with_a_bridge() -> crate::geo::Systems {
        let mut by_name = std::collections::HashMap::new();
        let mut adjacency: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        for id in 1..=10i64 {
            by_name.insert(format!("S{id}"), crate::geo::SystemInfo {
                id,
                name: format!("S{id}"),
                security: -0.4,
                constellation: "C".into(),
                region: "R".into(),
                faction: String::new(),
            });
            if id > 1 {
                adjacency.entry(id - 1).or_default().push(id);
                adjacency.entry(id).or_default().push(id - 1);
            }
        }
        let mut g = crate::geo::Systems::new(by_name, adjacency);
        g.add_bridges(&[(2, 9)]);
        g
    }

    /// The whole flow behind the bug: a route planned through a waypoint, on a path the autopilot
    /// cannot fly end to end. The waypoint has to reach the game, or it is not a waypoint at all.
    #[test]
    fn a_waypoint_survives_planning_a_bridged_route() {
        let g = line_with_a_bridge();
        let anchors = [1i64, 4, 10];
        let (_, mut options) = chain(
            &g,
            &[],
            &anchors,
            &["gate"],
            &crate::jumproute::SHIP_CLASSES[0],
            5,
            5,
            true,
            &Avoid::default(),
            &Avoid::default(),
            &Default::default(),
            &[],
            &Picks::new(),
            &Ahead::default(),
        );
        mark_anchors(&mut options, &anchors);
        let o = options.first().expect("a route");
        assert!(o.hops.iter().any(|h| h.kind == 1), "the bridge is the point of this route");
        let wp = ingame_waypoints(o, Some(1));
        assert!(wp.contains(&4), "the waypoint the user named was dropped: {wp:?}");
        assert_eq!(wp.last(), Some(&10), "the destination is the last stop");
    }

    /// Waypoints, forks and bridge ends all reach the game, in the order they are flown.
    #[test]
    fn pinned_systems_keep_their_travel_order() {
        let mut o = opt(&[1, 2, 3, 4, 5, 6, 7], &[0, 0, 0, 0, 2, 0, 0]);
        o.hops[0].anchor = true;
        o.hops[3].anchor = true;
        o.hops[6].anchor = true;
        o.hops[1].fork =
            vec![Branch { id: 3, name: "S3".into() }, Branch { id: 8, name: "S8".into() }];
        assert_eq!(ingame_waypoints(&o, Some(1)), vec![3, 4, 5, 7]);
    }

    /// A line of seven systems with two titans near the start reaching different systems. The shared
    /// fixture is three systems wide, too small for two titan options.
    fn line_of_seven() -> (crate::geo::Systems, Vec<crate::map::MapSystem>, Vec<i64>) {
        use crate::map::MapSystem;
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
        (g, coords, ids)
    }

    /// Fatigue is the pilot's, not the leg's: a waypoint in the middle of a jump route must not
    /// hand the capital a clean slate.
    #[test]
    fn fatigue_carries_across_a_waypoint() {
        // Five systems in a line, 4 ly apart: every hop is one jump for a capital at JDC 5.
        use crate::map::MapSystem;
        let ly = crate::map::LY_METERS;
        let ids: Vec<i64> = (0..5).map(|i| 30_200_000 + i).collect();
        let mut by_name = std::collections::HashMap::new();
        let mut adjacency = std::collections::HashMap::new();
        let mut coords: Vec<MapSystem> = Vec::new();
        for (i, &id) in ids.iter().enumerate() {
            let name = format!("J{i}");
            by_name.insert(name.clone(), crate::geo::SystemInfo {
                id,
                name: name.clone(),
                security: -0.4,
                constellation: "C".into(),
                region: "R".into(),
                faction: String::new(),
            });
            adjacency.insert(id, Vec::new());
            coords.push(MapSystem {
                id,
                name,
                security: -0.4,
                region_id: 10_000_060,
                x: i as f64 * 4.0 * ly,
                y: 0.0,
                z: 0.0,
                x2d: 0.0,
                z2d: 0.0,
            });
        }
        let g = crate::geo::Systems::new(by_name, adjacency);
        let class = &crate::jumproute::SHIP_CLASSES[0];
        let run = |anchors: &[i64]| {
            let (_, out) = chain(
                &g,
                &coords,
                anchors,
                &["jump"],
                class,
                5,
                5,
                false,
                &Avoid::default(),
                &Avoid::default(),
                &Default::default(),
                &[],
                &Picks::new(),
                &Ahead::default(),
            );
            let o = out.into_iter().next().expect("a route");
            let fatigue: Vec<f64> = o.hops.iter().filter_map(|h| h.fatigue_min).collect();
            (fatigue, o.note.unwrap_or_default())
        };
        let (straight, _) = run(&[ids[0], ids[4]]);
        let (split, note) = run(&[ids[0], ids[2], ids[4]]);
        assert_eq!(straight.len(), split.len(), "the same jumps either way");
        assert_eq!(straight, split, "the waypoint reset the fatigue");
        assert!(split.windows(2).all(|w| w[1] >= w[0]), "fatigue only climbs: {split:?}");
        let last = split.last().copied().expect("a jump");
        assert!(
            note.contains(&format!("{last:.0} min fatigue at the end")),
            "the note quotes a leg's fatigue, not the route's: {note}"
        );
    }

    /// Ten systems in a gate line, 1.4 ly apart: a titan without skills (3 ly) reaches two along.
    /// S0 is high sec when `hisec_start`, where no jump drive lights.
    fn ahead_line(hisec_start: bool) -> (crate::geo::Systems, Vec<crate::map::MapSystem>, Vec<i64>) {
        use crate::map::MapSystem;
        let ids: Vec<i64> = (0..10).map(|i| 30_200_000 + i).collect();
        let mut by_name = std::collections::HashMap::new();
        let mut adjacency = std::collections::HashMap::new();
        let mut coords = Vec::new();
        for (i, &id) in ids.iter().enumerate() {
            let security = if i == 0 && hisec_start { 0.9 } else { -0.4 };
            let name = format!("A{i}");
            by_name.insert(name.clone(), crate::geo::SystemInfo {
                id,
                name: name.clone(),
                security,
                constellation: "C".into(),
                region: "R".into(),
                faction: String::new(),
            });
            let near: Vec<i64> = [i.checked_sub(1), Some(i + 1).filter(|&n| n < ids.len())].into_iter().flatten().map(|n| ids[n]).collect();
            adjacency.insert(id, near);
            let x = i as f64 * 1.4 * crate::map::LY_METERS;
            coords.push(MapSystem { id, name, security, region_id: 10_000_060, x, y: 0.0, z: 0.0, x2d: 0.0, z2d: 0.0 });
        }
        (crate::geo::Systems::new(by_name, adjacency), coords, ids)
    }

    fn ahead(g: &crate::geo::Systems, coords: &[crate::map::MapSystem], from: i64, to: i64, avoid_jump: &Avoid, a: &Ahead) -> Vec<RouteOption> {
        let titan = &crate::jumproute::SHIP_CLASSES[1];
        jump_ahead(g, coords, from, to, titan, 0, 5, false, &Avoid::default(), avoid_jump, &Default::default(), a, &Picks::new())
            .into_iter()
            .filter(|o| !o.gated)
            .collect()
    }

    /// With no gating asked for, the plans that gate the capital first come along when they save
    /// the fleet steps, marked, and named by where the capital jumps from.
    #[test]
    fn plans_that_gate_first_are_offered_beside_when_they_save_steps() {
        let (g, coords, ids) = ahead_line(true);
        let titan = &crate::jumproute::SHIP_CLASSES[1];
        let all = jump_ahead(&g, &coords, ids[0], ids[9], titan, 0, 5, false, &Avoid::default(), &Avoid::default(), &Default::default(), &Ahead::default(), &Picks::new());
        let gated: Vec<&RouteOption> = all.iter().filter(|o| o.gated).collect();
        assert!(all.iter().any(|o| !o.gated && jumped(o).is_empty()), "nothing jumps out of high sec, so the gate route");
        assert!(!gated.is_empty(), "gating out of high sec first is offered");
        assert!(gated.iter().all(|o| o.hops[1].kind == 0 && !jumped(o).is_empty()));
        assert!(gated[0].label.contains(" from "), "{}", gated[0].label);
        assert!(gated.iter().all(|o| o.gates < 9), "each saves on the 9 gates");
        let asked = jump_ahead(&g, &coords, ids[0], ids[9], titan, 0, 5, false, &Avoid::default(), &Avoid::default(), &Default::default(), &Ahead { pre_gates: 1, ..Default::default() }, &Picks::new());
        assert!(asked.iter().all(|o| !o.gated), "gating asked for: one list");
    }

    fn jumped(o: &RouteOption) -> Vec<i64> {
        o.hops.iter().filter(|h| h.kind == 2).map(|h| h.id).collect()
    }

    #[test]
    fn jump_ahead_in_range_is_one_direct_jump() {
        let (g, coords, ids) = ahead_line(false);
        let out = ahead(&g, &coords, ids[0], ids[2], &Avoid::default(), &Ahead::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].hops.iter().map(|h| (h.id, h.kind)).collect::<Vec<_>>(), vec![(ids[0], 0), (ids[2], 2)]);
    }

    /// The fleet rides every jump to the end; the next plan lands a gate short.
    #[test]
    fn jump_ahead_chains_jumps_and_ranks_plans_by_fleet_gates() {
        let (g, coords, ids) = ahead_line(false);
        let out = ahead(&g, &coords, ids[0], ids[9], &Avoid::default(), &Ahead { max_jumps: 5, ..Default::default() });
        let shape: Vec<(usize, usize)> = out.iter().map(|o| (o.jumps - o.gates, o.gates)).collect();
        assert_eq!(&shape[..2], &[(5, 0), (4, 1)], "fewest gates first");
        assert_eq!(out.len(), AHEAD_OPTIONS);
        assert!(out.windows(2).all(|w| w[0].gates <= w[1].gates), "ranked by gates");
        assert_eq!(jumped(&out[0]).last(), Some(&ids[9]), "the last jump lands on the destination");
        assert_eq!(out[1].hops.last().map(|h| (h.id, h.kind)), Some((ids[9], 0)), "gated in");
        assert_eq!(out[0].saved, Some(9));
    }

    /// With beacons, the capital lands only on one, whatever a hard cyno would reach.
    #[test]
    fn jump_ahead_on_beacons_lands_only_on_beacons() {
        let (g, coords, ids) = ahead_line(false);
        let a = Ahead { beacons: Some([ids[2]].into()), max_jumps: 5, ..Default::default() };
        let out = ahead(&g, &coords, ids[0], ids[9], &Avoid::default(), &a);
        assert_eq!(out.len(), 1);
        assert_eq!(jumped(&out[0]), vec![ids[2]]);
        assert_eq!(out[0].gates, 7);
    }

    #[test]
    fn jump_ahead_never_lands_in_an_avoided_system() {
        let (g, coords, ids) = ahead_line(false);
        let avoid = Avoid { once: [ids[2], ids[6]].into(), ..Default::default() };
        let out = ahead(&g, &coords, ids[0], ids[9], &avoid, &Ahead { max_jumps: 5, ..Default::default() });
        assert!(!out.is_empty());
        for o in &out {
            assert!(!jumped(o).iter().any(|id| *id == ids[2] || *id == ids[6]), "{:?}", o.note);
        }
    }

    /// Nothing jumps out of high sec, so the capital has to gate out first, and only when allowed to.
    #[test]
    fn jump_ahead_gates_out_first_when_allowed() {
        let (g, coords, ids) = ahead_line(true);
        let stuck = ahead(&g, &coords, ids[0], ids[9], &Avoid::default(), &Ahead::default());
        assert!(stuck.iter().all(|o| jumped(o).is_empty()), "no jump, only the gate route");
        let a = Ahead { pre_gates: 1, max_jumps: 5, ..Default::default() };
        let out = ahead(&g, &coords, ids[0], ids[9], &Avoid::default(), &a);
        let o = out.first().expect("a plan");
        assert_eq!((o.hops[1].id, o.hops[1].kind), (ids[1], 0), "one gate to the jump-off point");
        assert_eq!(o.capital_gates.concat().iter().map(|b| b.id).collect::<Vec<_>>(), vec![ids[0], ids[1]]);
        assert!(o.note.as_deref().is_some_and(|n| n.starts_with("Capital gates 1 to A1")), "{:?}", o.note);
    }

    /// Black ops take an Ansiblex to the jump-off point; a titan cannot and has to gate there.
    #[test]
    fn black_ops_may_bridge_before_jumping() {
        let (mut g, coords, ids) = ahead_line(false);
        g.add_bridges(&[(ids[0], ids[5])]);
        let titan = &crate::jumproute::SHIP_CLASSES[1];
        let plan = |pre_bridges: bool| {
            let a = Ahead { pre_gates: 1, pre_bridges, ..Default::default() };
            jump_ahead(&g, &coords, ids[0], ids[9], titan, 0, 5, true, &Avoid::default(), &Avoid::default(), &Default::default(), &a, &Picks::new())
        };
        let blops = plan(true);
        assert_eq!((blops[0].hops[1].id, blops[0].hops[1].kind), (ids[5], 1), "over the Ansiblex first");
        assert_eq!(jumped(&blops[0]), vec![ids[7]]);
        let titan = plan(false);
        assert!(titan.iter().all(|o| jumped(o).is_empty()), "gating to a jump-off point costs the titan more than the Ansiblex route");
    }

    /// The titan gates to the jump-off point and the fleet takes the Ansiblex there: the plan costs
    /// the fleet one jump, and the capital's two gates are kept apart.
    #[test]
    fn the_fleet_and_the_capital_reach_the_jump_off_point_each_their_own_way() {
        let (mut g, coords, ids) = ahead_line(true);
        g.add_bridges(&[(ids[0], ids[2])]);
        let titan = &crate::jumproute::SHIP_CLASSES[1];
        let a = Ahead { pre_gates: 2, ..Default::default() };
        let out = jump_ahead(&g, &coords, ids[0], ids[9], titan, 0, 5, true, &Avoid::default(), &Avoid::default(), &Default::default(), &a, &Picks::new());
        let o = out.iter().find(|o| o.capital_gates.first().and_then(|r| r.last()).is_some_and(|b| b.id == ids[2])).expect("a plan from A2");
        assert_eq!((o.hops[1].id, o.hops[1].kind), (ids[2], 1), "the fleet over the Ansiblex");
        assert_eq!(o.capital_gates[0].iter().map(|b| b.id).collect::<Vec<_>>(), vec![ids[0], ids[1], ids[2]], "the capital by gates");
    }

    /// The titan jumps ahead alone, the fleet catches up over an Ansiblex the titan cannot take, and
    /// the titan bridges it on from there: four fleet steps, where riding the titan's first jump
    /// would leave six.
    #[test]
    fn the_capital_jumps_ahead_alone_and_the_fleet_catches_up() {
        let (mut g, mut coords, ids) = ahead_line(false);
        for (i, c) in coords.iter_mut().enumerate() {
            c.x = i as f64 * crate::map::LY_METERS;
        }
        g.add_bridges(&[(ids[0], ids[3])]);
        let titan = &crate::jumproute::SHIP_CLASSES[1];
        let out = jump_ahead(&g, &coords, ids[0], ids[9], titan, 0, 5, true, &Avoid::default(), &Avoid::default(), &Default::default(), &Ahead::default(), &Picks::new());
        let o = &out[0];
        let tj = o.titan_jump.as_ref().expect("the capital moves first");
        assert_eq!((tj.from, tj.to), (ids[0], ids[3]), "alone, three light years ahead");
        assert_eq!((o.hops[1].id, o.hops[1].kind), (ids[3], 1), "the fleet over the Ansiblex");
        assert_eq!(jumped(o), vec![ids[6]], "bridged on from where the capital waits");
        assert_eq!(o.hops.iter().skip(1).filter(|h| h.kind != 2).count(), 4);
        assert!(o.capital_gates.is_empty(), "no gates of its own");
    }

    /// A carrier goes through an XL hole to its jump-off point, which the planner is handed.
    #[test]
    fn a_carrier_may_take_a_hole_before_jumping() {
        let (g, coords, ids) = ahead_line(false);
        let hole: std::collections::HashMap<i64, Vec<i64>> = [(ids[0], vec![ids[5]]), (ids[5], vec![ids[0]])].into();
        let a = Ahead { pre_gates: 1, pre_holes: hole.clone(), ..Default::default() };
        let titan = &crate::jumproute::SHIP_CLASSES[1];
        let out = jump_ahead(&g, &coords, ids[0], ids[9], titan, 0, 5, false, &Avoid::default(), &Avoid::default(), &hole, &a, &Picks::new());
        let h = &out[0].hops[1];
        assert_eq!((h.id, h.kind, h.hole), (ids[5], 0, true), "through the hole first");
        assert_eq!(jumped(&out[0]), vec![ids[7]]);
    }

    /// Among equally long chains, the one that lands where the capital can dock.
    #[test]
    fn jump_ahead_prefers_systems_the_capital_can_dock_in() {
        let (g, coords, ids) = ahead_line(false);
        let odd = |a: &Ahead| ahead(&g, &coords, ids[0], ids[9], &Avoid::default(), a).remove(0);
        let a = Ahead { prefer: [ids[1], ids[3], ids[5], ids[7]].into(), max_jumps: 5, ..Default::default() };
        assert_eq!(jumped(&odd(&a)), vec![ids[1], ids[3], ids[5], ids[7], ids[9]]);
        let a = Ahead { prefer: [ids[2], ids[4], ids[6], ids[8]].into(), max_jumps: 5, ..Default::default() };
        assert_eq!(jumped(&odd(&a)), vec![ids[2], ids[4], ids[6], ids[8], ids[9]]);
    }

    /// The jump limit holds: one jump lands two along and gates the rest, three jumps get further.
    #[test]
    fn jump_ahead_keeps_to_the_jump_limit() {
        let (g, coords, ids) = ahead_line(false);
        for (limit, gates) in [(1, 7), (3, 3)] {
            let out = ahead(&g, &coords, ids[0], ids[9], &Avoid::default(), &Ahead { max_jumps: limit, ..Default::default() });
            assert!(out.iter().all(|o| jumped(o).len() <= limit as usize), "limit {limit}");
            assert_eq!(out[0].gates, gates, "limit {limit}");
        }
    }

    /// With a waypoint after it, the jump-ahead leg still names each plan: the joined route would
    /// otherwise take the gate leg's name, and every plan would read "Gates".
    #[test]
    fn plans_keep_their_names_with_a_gate_leg_after_them() {
        let (g, coords, ids) = ahead_line(false);
        let (_, out) = chain(
            &g,
            &coords,
            &[ids[0], ids[8], ids[9]],
            &["ahead", "gate"],
            &crate::jumproute::SHIP_CLASSES[1],
            0,
            5,
            false,
            &Avoid::default(),
            &Avoid::default(),
            &Default::default(),
            &[],
            &Picks::new(),
            &Ahead { max_jumps: 2, ..Default::default() },
        );
        assert!(out.len() > 1);
        assert!(out.iter().all(|o| o.label.starts_with("via ")), "{:?}", out.iter().map(|o| &o.label).collect::<Vec<_>>());
    }

    /// The plans are the route's options; the gating before them belongs to the first leg only.
    #[test]
    fn a_jump_ahead_route_offers_its_plans_as_options() {
        let (g, coords, ids) = ahead_line(true);
        let a = Ahead { pre_gates: 1, max_jumps: 3, ..Default::default() };
        let (legs, out) = chain(
            &g,
            &coords,
            &[ids[0], ids[9]],
            &["ahead"],
            &crate::jumproute::SHIP_CLASSES[1],
            0,
            5,
            false,
            &Avoid::default(),
            &Avoid::default(),
            &Default::default(),
            &[],
            &Picks::new(),
            &a,
        );
        assert_eq!(legs.len(), 1);
        assert!(out.len() > 1, "each plan is an option");
        assert!(out[0].hops.iter().filter_map(|h| h.fatigue_min).last().is_some_and(|f| f > 0.0), "fatigue carried");
    }

    /// xorshift64*: deterministic, so a failing seed can be replayed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn chance(&mut self, pct: u64) -> bool {
            self.below(100) < pct
        }
    }

    /// One random universe and one random question about it.
    struct Case {
        g: crate::geo::Systems,
        coords: Vec<crate::map::MapSystem>,
        adj: std::collections::HashMap<i64, Vec<i64>>,
        from: i64,
        to: i64,
        jdc: u32,
        avoid_gate: Avoid,
        avoid_jump: Avoid,
        ahead: Ahead,
    }

    /// Systems scattered over a box several titan ranges wide, a gate tree between near neighbours
    /// plus extra gates, some high sec, and random beacons, avoid lists, docking and gating rules.
    fn random_case(seed: u64) -> Case {
        use crate::map::MapSystem;
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let n = 40 + rng.below(40) as usize;
        let ly = crate::map::LY_METERS;
        let ids: Vec<i64> = (0..n as i64).map(|i| 30_300_000 + i).collect();
        let mut coords = Vec::new();
        let mut by_name = std::collections::HashMap::new();
        for (i, &id) in ids.iter().enumerate() {
            let security = if rng.chance(15) { 0.7 } else { -0.3 };
            let (x, z) = (rng.below(2500) as f64 / 100.0 * ly, rng.below(2500) as f64 / 100.0 * ly);
            let name = format!("R{i}");
            by_name.insert(name.clone(), crate::geo::SystemInfo {
                id,
                name: name.clone(),
                security,
                constellation: "C".into(),
                region: "R".into(),
                faction: String::new(),
            });
            coords.push(MapSystem { id, name, security, region_id: 10_000_060, x, y: 0.0, z, x2d: 0.0, z2d: 0.0 });
        }
        let d = |a: usize, b: usize| crate::map::ly_distance(&coords[a], &coords[b]);
        let mut adj: std::collections::HashMap<i64, Vec<i64>> = ids.iter().map(|&id| (id, Vec::new())).collect();
        let mut link = |a: usize, b: usize, adj: &mut std::collections::HashMap<i64, Vec<i64>>| {
            if a != b && !adj[&ids[a]].contains(&ids[b]) {
                adj.get_mut(&ids[a]).expect("id").push(ids[b]);
                adj.get_mut(&ids[b]).expect("id").push(ids[a]);
            }
        };
        // Each system gates to its nearest earlier one, so the universe is one piece, as null is.
        for i in 1..n {
            let j = (0..i).min_by(|&a, &b| d(i, a).total_cmp(&d(i, b))).expect("an earlier system");
            link(i, j, &mut adj);
        }
        for _ in 0..n / 3 {
            let a = rng.below(n as u64) as usize;
            let b = (0..n).filter(|&b| b != a).min_by(|&p, &q| (d(a, p) + rng.below(3) as f64).total_cmp(&(d(a, q) + rng.below(3) as f64))).expect("another system");
            link(a, b, &mut adj);
        }
        let g = crate::geo::Systems::new(by_name, adj.clone());
        let from = ids[rng.below(n as u64) as usize];
        let to = ids[rng.below(n as u64) as usize];
        let pick = |rng: &mut Rng, pct: u64| -> std::collections::HashSet<i64> {
            ids.iter().copied().filter(|_| rng.chance(pct)).collect()
        };
        let mut avoid_gate = Avoid::default();
        avoid_gate.once = pick(&mut rng, 6);
        let mut avoid_jump = Avoid::default();
        avoid_jump.once = pick(&mut rng, 10);
        let ahead = Ahead {
            pre_gates: rng.below(4) as u32,
            max_jumps: 1 + rng.below(4) as u32,
            beacons: rng.chance(40).then(|| pick(&mut rng, 45)),
            prefer: if rng.chance(50) { pick(&mut rng, 30) } else { Default::default() },
            highsec: rng.chance(25),
            ..Default::default()
        };
        Case { g, coords, adj, from, to, jdc: rng.below(6) as u32, avoid_gate, avoid_jump, ahead }
    }

    impl Case {
        fn plan(&self) -> Vec<RouteOption> {
            let titan = &crate::jumproute::SHIP_CLASSES[1];
            jump_ahead(&self.g, &self.coords, self.from, self.to, titan, self.jdc, 5, false, &self.avoid_gate, &self.avoid_jump, &Default::default(), &self.ahead, &Picks::new())
                .into_iter()
                .filter(|o| !o.gated)
                .collect()
        }
        fn max_ly(&self) -> f64 {
            crate::jumproute::max_range_ly(&crate::jumproute::SHIP_CLASSES[1], self.jdc)
        }
        fn at(&self, id: i64) -> &crate::map::MapSystem {
            self.coords.iter().find(|s| s.id == id).expect("a system of the case")
        }
        fn lowsec(&self, id: i64) -> bool {
            crate::jumproute::cyno_able(self.at(id).security)
        }
        fn may_land(&self, id: i64) -> bool {
            self.lowsec(id)
                && (id == self.to
                    || (!self.avoid_jump.blocked(id) && self.ahead.beacons.as_ref().is_none_or(|b| b.contains(&id))))
        }
        fn cap_gates(&self, id: i64) -> bool {
            !self.avoid_gate.blocked(id) && (self.ahead.highsec || self.lowsec(id))
        }
        /// Plain breadth-first gate distances from `start` through systems `ok` lets through.
        fn bfs(&self, start: i64, ok: impl Fn(i64) -> bool) -> std::collections::HashMap<i64, u32> {
            let mut dist = std::collections::HashMap::from([(start, 0)]);
            let mut queue = std::collections::VecDeque::from([start]);
            while let Some(s) = queue.pop_front() {
                for &n in &self.adj[&s] {
                    if !dist.contains_key(&n) && ok(n) {
                        dist.insert(n, dist[&s] + 1);
                        queue.push_back(n);
                    }
                }
            }
            dist
        }

        /// Every landing's fewest fleet gates, found the slow way: each launch point, everything its
        /// jumps reach, and the gates from there to the target.
        fn oracle(&self) -> Vec<u32> {
            let gated: std::collections::HashMap<i64, u32> =
                self.bfs(self.from, |id| self.cap_gates(id)).into_iter().filter(|&(_, d)| d <= self.ahead.pre_gates).collect();
            // What a plan costs is the fleet's way to the launch point, which may differ from the capital's.
            let fleet = self.bfs(self.from, |id| !self.avoid_gate.blocked(id));
            // Where the capital can wait: a system it gated to, or one it jumped to alone from one.
            let mut stands: std::collections::HashSet<i64> = gated.keys().copied().collect();
            for &l in gated.keys().filter(|&&id| self.lowsec(id)) {
                for t in &self.coords {
                    if t.id != self.to && self.may_land(t.id) && crate::map::ly_distance(self.at(l), t) <= self.max_ly() {
                        stands.insert(t.id);
                    }
                }
            }
            let launches: Vec<(i64, u32)> =
                stands.iter().filter(|&&id| self.lowsec(id)).filter_map(|&id| Some((id, *fleet.get(&id)?))).collect();
            // Cheapest fleet cost to land in each system, one layer per jump: the first from where the
            // capital waits, each later one after the capital gates on up to its limit.
            let in_range = |l: i64| -> Vec<i64> {
                self.coords
                    .iter()
                    .filter(|t| t.id != l && self.may_land(t.id) && crate::map::ly_distance(self.at(l), t) <= self.max_ly())
                    .map(|t| t.id)
                    .collect()
            };
            let mut layer: std::collections::HashMap<i64, u32> = Default::default();
            for &(l, pre) in &launches {
                for t in in_range(l) {
                    let e = layer.entry(t).or_insert(u32::MAX);
                    *e = (*e).min(pre);
                }
            }
            let mut best = layer.clone();
            for _ in 1..self.ahead.max_jumps {
                let mut next: std::collections::HashMap<i64, u32> = Default::default();
                for (&x, &cost) in &layer {
                    let cap = self.bfs(x, |id| self.cap_gates(id));
                    let fleet = self.bfs(x, |id| !self.avoid_gate.blocked(id));
                    for (&l, &d) in &cap {
                        if d > self.ahead.pre_gates || !self.lowsec(l) {
                            continue;
                        }
                        let Some(&c) = fleet.get(&l) else { continue };
                        for t in in_range(l) {
                            let e = next.entry(t).or_insert(u32::MAX);
                            *e = (*e).min(cost + c);
                        }
                    }
                }
                for (&t, &c) in &next {
                    let e = best.entry(t).or_insert(u32::MAX);
                    *e = (*e).min(c);
                }
                layer = next;
            }
            let mut out: Vec<u32> = best
                .into_iter()
                .filter(|(f, pre)| !stands.contains(f) || fleet.get(f).is_none_or(|d| d > pre))
                .filter_map(|(f, pre)| {
                    let tail = if f == self.to {
                        0
                    } else {
                        *self.bfs(f, |id| id == self.to || !self.avoid_gate.blocked(id)).get(&self.to)?
                    };
                    Some(pre + tail)
                })
                .collect();
            let plain = self.bfs(self.from, |id| id == self.to || !self.avoid_gate.blocked(id)).get(&self.to).copied();
            out.retain(|&g| plain.is_none_or(|p| g < p));
            out.sort_unstable();
            out
        }

        /// Whether a plan is one a fleet can fly under the case's rules, step by step.
        fn check(&self, o: &RouteOption) -> Result<(), String> {
            self.check_within(o, self.ahead.pre_gates)
        }

        fn check_within(&self, o: &RouteOption, pre_gates: u32) -> Result<(), String> {
            let h = &o.hops;
            if h.first().map(|x| x.id) != Some(self.from) || h.last().map(|x| x.id) != Some(self.to) {
                return Err("does not run from start to target".into());
            }
            let jumps: Vec<usize> = h.iter().enumerate().filter(|(_, x)| x.kind == 2).map(|(i, _)| i).collect();
            let first = *jumps.first().ok_or("no capital jump")?;
            if jumps.len() > self.ahead.max_jumps as usize {
                return Err(format!("{} jumps, {} allowed", jumps.len(), self.ahead.max_jumps));
            }
            let launches: Vec<i64> = jumps.iter().map(|&i| h[i - 1].id).collect();
            for w in h.windows(2) {
                let (a, b) = (w[0].id, w[1].id);
                match w[1].kind {
                    0 if !self.adj[&a].contains(&b) => return Err(format!("{} to {} is no gate", w[0].name, w[1].name)),
                    0 if b != self.to && !launches.contains(&b) && self.avoid_gate.blocked(b) => {
                        return Err(format!("fleet gates through avoided {}", w[1].name))
                    }
                    2 => {
                        let ly = crate::map::ly_distance(self.at(a), self.at(b));
                        if ly > self.max_ly() + 1e-9 {
                            return Err(format!("{ly:.2} ly jump beyond {:.2}", self.max_ly()));
                        }
                        if !self.may_land(b) {
                            return Err(format!("lands in {}, which it may not", w[1].name));
                        }
                        if !self.lowsec(a) {
                            return Err(format!("jumps out of high sec {}", w[0].name));
                        }
                    }
                    _ => {}
                }
            }
            // Where the capital gates on its own: from the start (or from where it jumps alone) to
            // the first launch, and from each landing to the next launch where they differ.
            let mut want: Vec<(i64, i64)> = Vec::new();
            let mut first_launch = launches[0];
            if let Some(tj) = &o.titan_jump {
                if tj.to != first_launch {
                    return Err("the capital jumps alone somewhere the fleet does not jump from".into());
                }
                if tj.to == self.to || !self.may_land(tj.to) || !self.lowsec(tj.from) {
                    return Err(format!("the capital may not jump alone from {} to {}", tj.from_name, tj.to_name));
                }
                if crate::map::ly_distance(self.at(tj.from), self.at(tj.to)) > self.max_ly() + 1e-9 {
                    return Err("the capital's own jump is out of range".into());
                }
                first_launch = tj.from;
            }
            if first_launch != self.from {
                want.push((self.from, first_launch));
            }
            for k in 1..jumps.len() {
                let (landed, next) = (h[jumps[k - 1]].id, launches[k]);
                if landed != next {
                    want.push((landed, next));
                }
            }
            if want.len() != o.capital_gates.len() {
                return Err(format!("{} capital runs, {} expected", o.capital_gates.len(), want.len()));
            }
            for ((a, b), run) in want.iter().zip(&o.capital_gates) {
                let ids: Vec<i64> = run.iter().map(|x| x.id).collect();
                if ids.first() != Some(a) || ids.last() != Some(b) {
                    return Err("a capital run does not join its jumps".into());
                }
                if ids.len() - 1 > pre_gates as usize {
                    return Err(format!("capital gates {}, {} allowed", ids.len() - 1, pre_gates));
                }
                for w in ids.windows(2) {
                    if !self.adj[&w[0]].contains(&w[1]) || !self.cap_gates(w[1]) {
                        return Err(format!("the capital cannot gate to {}", self.at(w[1]).name));
                    }
                }
            }
            if o.gates != h.iter().filter(|x| x.kind == 0).count() - 1 {
                return Err("gate count does not match the hops".into());
            }
            Ok(())
        }
    }

    /// Hundreds of random universes, each answered both by the planner and by brute force: every plan
    /// must be flyable, and the five offered must be the five fewest-gate landings there are.
    #[test]
    fn jump_ahead_finds_the_five_fewest_gate_plans_in_random_universes() {
        let (mut planned, mut empty, mut gated_first, mut on_beacons, mut gated_extra, mut solo) = (0, 0, 0, 0, 0, 0);
        for seed in 0..400 {
            let c = random_case(seed);
            let out = c.plan();
            if c.from == c.to {
                assert!(out.is_empty(), "seed {seed}: nowhere to go");
                continue;
            }
            let best = c.oracle();
            let direct = c.lowsec(c.from)
                && c.lowsec(c.to)
                && c.from != c.to
                && crate::map::ly_distance(c.at(c.from), c.at(c.to)) <= c.max_ly();
            if direct {
                assert_eq!(out.len(), 1, "seed {seed}: in range is one direct jump");
                assert_eq!((out[0].gates, out[0].hops.len()), (0, 2), "seed {seed}");
                continue;
            }
            if best.is_empty() {
                empty += 1;
                assert!(out.len() <= 1 && out.iter().all(|o| o.hops.iter().all(|h| h.kind == 0)), "seed {seed}: nothing beats gating, so the gate route");
                continue;
            }
            let got: Vec<u32> = out.iter().map(|o| o.gates as u32).collect();
            let want: Vec<u32> = best.iter().copied().take(AHEAD_OPTIONS).collect();
            assert_eq!(got, want, "seed {seed}: fleet gates of the plans against brute force");
            for o in &out {
                if let Err(e) = c.check(o) {
                    panic!("seed {seed}: {e}: {:?}", o.hops.iter().map(|h| (&h.name, h.kind)).collect::<Vec<_>>());
                }
                gated_first += usize::from(o.hops[1].kind == 0 && o.hops.iter().any(|h| h.kind == 2));
            }
            if c.ahead.pre_gates == 0 {
                let titan = &crate::jumproute::SHIP_CLASSES[1];
                let all = jump_ahead(&c.g, &c.coords, c.from, c.to, titan, c.jdc, 5, false, &c.avoid_gate, &c.avoid_jump, &Default::default(), &c.ahead, &Picks::new());
                let best = out.iter().filter(|o| !jumped(o).is_empty()).map(|o| o.gates).min();
                for o in all.iter().filter(|o| o.gated) {
                    if let Err(e) = c.check_within(o, GATED_PLAN_GATES) {
                        panic!("seed {seed}: gated plan: {e}");
                    }
                    assert!(best.is_none_or(|b| o.gates < b), "seed {seed}: a gated plan has to save steps");
                    gated_extra += 1;
                }
            }
            let landings: std::collections::HashSet<i64> =
                out.iter().map(|o| o.hops.iter().rev().find(|h| h.kind == 2).expect("a jump").id).collect();
            assert_eq!(landings.len(), out.len(), "seed {seed}: one plan per landing");
            planned += usize::from(!out.is_empty());
            solo += out.iter().filter(|o| o.titan_jump.is_some()).count();
            on_beacons += usize::from(!out.is_empty() && c.ahead.beacons.is_some());
        }
        assert!(planned > 100 && empty > 5, "planned {planned}, empty {empty}");
        assert!(gated_first > 20 && on_beacons > 30 && gated_extra > 20 && solo > 50, "gated first {gated_first}, on beacons {on_beacons}, gated extra {gated_extra}, solo {solo}");
    }

    /// A target in range is one jump, not a bridge to its neighbour and a gate.
    #[test]
    fn a_target_in_titan_range_is_one_direct_jump() {
        let (g, coords, ids) = line_of_seven();
        let none = std::collections::HashMap::new();
        let out = titan(&g, &coords, ids[0], ids[4], 6.0, true, true, &Avoid::default(), &none, &[], true);
        assert_eq!(out.len(), 1, "one option: {:?}", out.iter().map(|o| &o.label).collect::<Vec<_>>());
        let o = &out[0];
        assert_eq!((o.gates, o.jumps, o.titan_jump.is_some()), (0, 1, false));
        assert_eq!(o.hops.iter().map(|h| (h.id, h.kind)).collect::<Vec<_>>(), vec![(ids[0], 0), (ids[4], 2)]);
    }

    /// "Titan can move" allows a move, it does not demand one. From S0 the titan bridges to S5 and
    /// the fleet gates once; moving it first cannot beat that, so it stays.
    #[test]
    fn a_titan_that_may_move_stays_when_moving_gains_nothing() {
        let (g, coords, ids) = line_of_seven();
        let none = std::collections::HashMap::new();
        let out = titan(&g, &coords, ids[0], ids[6], 6.0, true, true, &Avoid::default(), &none, &[], true);
        let best = out.first().expect("a route");
        assert!(best.titan_jump.is_none(), "the titan moved for nothing: {:?}", best.note);
        assert_eq!(best.path.first(), Some(&ids[0]));
        assert_eq!(best.gates, 1, "bridge to S5, one gate to S6: {:?}", best.note);
        assert!(out.iter().all(|o| o.titan_jump.is_none()), "no move offered at all");
    }

    /// A mixed route flies each leg its own way: jumps out to the waypoint, gates on from it, and the
    /// hops say which is which.
    #[test]
    fn a_mixed_route_jumps_one_leg_and_gates_the_next() {
        let (g, coords, ids) = line_of_seven();
        let (legs, out) = chain(
            &g,
            &coords,
            &[ids[0], ids[4], ids[6]],
            &["jump", "gate"],
            &crate::jumproute::SHIP_CLASSES[1],
            5,
            5,
            false,
            &Avoid::default(),
            &Avoid::default(),
            &Default::default(),
            &[],
            &Picks::new(),
            &Ahead::default(),
        );
        assert_eq!(legs.len(), 2);
        let o = out.first().expect("a route");
        let at = |id: i64| o.hops.iter().find(|h| h.id == id).map(|h| h.kind);
        assert_eq!(at(ids[4]), Some(2), "the first leg ends in a jump");
        assert_eq!((at(ids[5]), at(ids[6])), (Some(0), Some(0)), "the second leg is gates");
        assert_eq!(leg_kind(&["jump"], 3), "jump", "one kind covers every leg");
        assert_eq!(leg_kind(&[], 0), "gate");
    }

    /// A hop through a known hole says so, and is not counted as a gate.
    #[test]
    fn a_hop_through_a_hole_is_marked_and_not_a_gate() {
        let g = crate::test_support::small_universe(&[]);
        let holes = std::collections::HashMap::from([(30_004_759, vec![30_000_142]), (30_000_142, vec![30_004_759])]);
        let o = gate(&g, 30_004_759, 30_000_142, false, &Avoid::default(), &holes).expect("through the hole");
        assert_eq!(o.hops.iter().map(|h| h.hole).collect::<Vec<_>>(), vec![false, true]);
        assert_eq!(o.gates, 0);
        assert!(o.uses_wormhole);
        assert!(reverse(o).hops[1].hole, "the hole stays on the hop it leads into, reversed");
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
            &["gate"],
            &crate::jumproute::SHIP_CLASSES[1],
            5,
            5,
            false,
            &Avoid::default(),
            &Avoid::default(),
            &Default::default(),
            &[],
            &Picks::new(),
            &Ahead::default(),
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
