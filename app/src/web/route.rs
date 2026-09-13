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
    /// Why not to fly through here, if there is a reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<HopWarning>,
    /// A system the user named: the start, a waypoint, or the destination. Everything else on a
    /// route is just somewhere it passes through.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anchor: bool,
}

/// One leg of a route, and the ways of flying it that are no worse.
///
/// Alternatives are the same number of jumps and sorted by total distance, so the list is "these all
/// cost you the same; here they are from shortest to longest".
#[derive(Serialize)]
pub struct LegChoice {
    pub from: i64,
    pub to: i64,
    pub from_name: String,
    pub to_name: String,
    pub options: Vec<RouteOption>,
}

/// Where a route may not go.
#[derive(Default, Clone)]
pub struct Avoid {
    /// Kept for good, from settings.
    pub always: std::collections::HashSet<i64>,
    /// Kept for this route only, from the client.
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

/// How many ways of flying one leg are worth offering.
///
/// Past a handful they stop being a choice and start being a list, and every one of them costs a
/// search.
const LEG_OPTIONS: usize = 4;

/// A reason to look twice at a system on the route.
///
/// Intel below Danger is not carried: a route through nullsec passes through dozens of systems that
/// someone has said something about, and a warning on all of them is a warning on none.
#[derive(Serialize, Clone, Copy, Default, PartialEq)]
pub struct HopWarning {
    /// Worst severity reported inside the intel TTL. 2 is Danger, 3 Critical.
    pub sev: u8,
    /// When that report came in, so the page can say how old it is.
    pub at: i64,
    /// Ship and pod kills in the last hour, from ESI.
    pub kills: u32,
    pub pods: u32,
}

/// Danger and above. Below that a nullsec route would be warnings end to end.
pub const WARN_SEVERITY: u8 = 2;

/// Mark the systems the user named, so the map and the list can pick them out of the ones the route
/// merely passes through.
pub fn mark_anchors(options: &mut [RouteOption], anchors: &[i64]) {
    let set: std::collections::HashSet<i64> = anchors.iter().copied().collect();
    for o in options.iter_mut() {
        for h in o.hops.iter_mut() {
            h.anchor = set.contains(&h.id);
        }
    }
}

/// Attach the warnings to every hop of every option.
///
/// A pass over the finished routes rather than an argument to each builder: what counts as dangerous
/// is a property of the moment, not of the path, and threading it through three route functions
/// would put the same lookup in three places.
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

/// The warning map from what the page already has: the map pane's per-system intel and the status
/// pane's kill counts.
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

/// The same map from the app's own state: raw reports plus the severity rules, because the desktop
/// has no published snapshot to read when the web view is switched off.
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
    /// What avoidance cost, when it cost anything. Absent when the route is the one you would have
    /// flown anyway, which is most of the time even with a long avoid list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detour: Option<String>,
    /// The titan's own jump, when it moves itself before bridging: where from, where to, and how far.
    /// Not a hop, because the fleet does not fly it; the fleet gates to where the titan lands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub titan_jump: Option<TitanJump>,
    /// Gates this saves over flying it without the titan. The whole point of the question, and the
    /// number that says whether the answer is worth the fuel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<usize>,
}

#[derive(Serialize, Clone)]
pub struct TitanJump {
    pub from: i64,
    pub to: i64,
    pub from_name: String,
    pub to_name: String,
    pub ly: f64,
}

/// The hulls a jump route can be planned for, so the page's picker is the app's own list rather than
/// a second copy of it.
#[derive(Serialize)]
pub struct Avoided {
    pub id: i64,
    pub name: String,
    pub always: bool,
}

/// The avoid list, named and ordered, for showing back to the user.
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
    /// The ways of flying each leg, so the window can offer them per waypoint.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub legs: Vec<LegChoice>,
    /// What the jump figures were worked out with, echoed back so the page's controls and the
    /// numbers beside them cannot drift apart.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hulls: Vec<Hull>,
    pub hull: usize,
    pub jdc: u32,
    pub jfc: u32,
    pub max_ly: f64,
    /// Whether the app is routing through scanned wormholes, echoed so the page can show the switch
    /// rather than keep its own copy of a setting that lives in the app.
    pub via_wormholes: bool,
    /// Every system this route was planned around, named, and whether it is on the permanent list.
    /// Sent because the ids alone are not something anyone can check: "avoiding 3 systems" is only
    /// useful if you can see which three.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub avoided: Vec<Avoided>,
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
        warn: None,
        anchor: false,
    }
}

/// The gate route, with jump bridges when the user counts them.
pub fn gate(
    graph: &crate::geo::Systems,
    from: i64,
    to: i64,
    bridges: bool,
    avoid: &Avoid,
    holes: &std::collections::HashMap<i64, Vec<i64>>,
) -> Option<RouteOption> {
    // The endpoints are exempt. Avoiding the system you are standing in, or the one you asked to go
    // to, would just mean no route at all, which is a worse answer than an honest one.
    //
    // `holes` is the scanned wormhole chain, empty unless the user routes through them. They are
    // passed as extra edges rather than as a flag because that is how the app's own planner takes
    // them, and a second way of expressing "there is a hole here" is a second way to be wrong.
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
    Some(RouteOption {
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

/// The capital jump route: cyno-able systems only, one hop per jump.
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
    // Avoidance is applied by taking the systems out of the graph rather than by filtering the
    // result: a path that goes through a banned system is not a worse path, it is not a path.
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
        detour: None,
        titan_jump: None,
        saved: None,
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

/// The titan answer: one jump, and gates for the rest.
///
/// This is the rescue mode's calculation, which asks the question the other way round and for the
/// same reason: the fewest *gate* jumps from the far end that a titan can still reach, not the system
/// that happens to be nearest on the map. A system four light years away and twelve gates out is a
/// worse answer than one six light years away and two gates out, and picking by distance gets that
/// backwards every time.
///
/// `at_start` says which end the titan is at. With it set, which is the default, the titan is in the
/// system the route starts from: you jump out as far as range allows and gate the rest. Cleared, the
/// titan is waiting at the far end: you gate out to the best system it can reach and get bridged in.
/// The search is the same either way, run from the other end.
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
    // The titan moves itself first: it is sitting with the fleet, so it can jump somewhere that
    // bridges better and have the fleet gate out to meet it. Worth offering because a titan's range
    // is often the difference between two gates and twenty.
    if self_jump && at_start && titans.is_empty() {
        let opts = titan_self_jump(graph, coords, from, to, max_ly, bridges, avoid, holes);
        if !opts.is_empty() {
            return opts;
        }
    }
    // Named titans win over the two guesses: if the user has said where the ships actually are, that
    // is the answer, and "at the start" is only a guess about where one might be.
    if !titans.is_empty() {
        return titan_via(graph, coords, from, to, max_ly, bridges, avoid, holes, titans);
    }
    if !at_start {
        // The mirror image: plan it backwards and turn the result around. One implementation of
        // "one jump, gates for the rest" rather than two that can disagree.
        return titan(graph, coords, to, from, max_ly, bridges, true, avoid, holes, titans, false)
            .into_iter()
            .map(reverse)
            .collect();
    }
    let (Some(start), Some(target)) = (pos(coords, from), pos(coords, to)) else {
        return Vec::new();
    };
    // Everything the titan can reach in one jump, the target itself excluded: if it were in range
    // this would not be a titan route, it would be one jump.
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
            let rest = gate(graph, hop, to, bridges, avoid, holes)?;
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
        path,
        hops,
    }
}

/// Every way of flying one leg that is no worse than the best one.
///
/// The best path, then the best path with each of its intermediate systems banned in turn, keeping
/// only the ones that take the same number of jumps. That is the cheap half of Yen's algorithm and it
/// is enough here: the question is "what else costs the same", not "rank every path there is".
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
    // What the avoid list cost, if it cost anything. Worth one more search: a route that is three
    // jumps longer than it needs to be is worth knowing about, and "why is this going the long way
    // round" is otherwise unanswerable from the list.
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
    // Same cost, so the tie goes to the shorter flight. For a gate route that is the one with less
    // grid to cross; for a jump route it is less fuel and less fatigue.
    out.sort_by(|p, q| p.total_ly.partial_cmp(&q.total_ly).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// A route through waypoints, with the ways of flying each leg.
///
/// The anchors are the systems the drags and the menu named, in order. `pick` is which alternative to
/// use for each leg, and the assembled route comes back alongside the choices so the client never has
/// to join legs itself.
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
        legs.push(LegChoice { from: a, to: b, from_name: name(a), to_name: name(b), options });
    }
    if legs.iter().any(|l| l.options.is_empty()) {
        return (legs, Vec::new());
    }
    // Assembled here, not in the client: joining legs is where the duplicated hop lives, and one
    // implementation of that is enough.
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
    // The titan search's alternatives are alternatives for the whole route, not for one leg of it,
    // so they stay in the option list the window already has.
    if kind == "titan" && legs[last].options.len() > 1 {
        out = (0..legs[last].options.len())
            .map(|k| assemble(&move |i: usize| if i == last { k } else { pick.get(i).copied().unwrap_or(0) }))
            .collect();
    }
    (legs, out)
}

/// The titan jumps itself, the fleet gates out to it, and it bridges them from there.
///
/// The objective is the **fleet's** gate count, not the titan's jump. A titan that hops one system
/// over has moved and helped nobody; the one worth taking is the landing from which the bridge lands
/// the fleet as close to the destination as it can, counting what it costs the fleet to get there.
///
/// Two distance balls do most of the work: how far every system is from the start by gates, and how
/// far every system is from the destination by gates. After that a landing is scored by adding two
/// numbers, and only the inner "which system does the bridge land on" needs a scan.
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
    /// How far the fleet will gate to meet the titan. Past this it is not a shortcut any more.
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

    // Every system the titan could jump to that the fleet can also reach by gates.
    let landings: Vec<&MapSystem> = coords
        .iter()
        .filter(|s| s.id != from && crate::jumproute::jumpable(s))
        .filter(|s| !avoid.blocked(s.id))
        .filter(|s| out_from_start.contains_key(&s.id))
        .filter(|s| crate::map::ly_distance(start, s) <= max_m)
        .collect();

    // Scored: what the fleet gates to reach the titan, plus what it gates after being thrown.
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
        // Strictly better, which is what "saves at least one jump" means. A reposition that ties is
        // a titan cycling its drive for nothing.
        if total >= baseline {
            continue;
        }
        scored.push((total, crate::map::ly_distance(start, land), land.id, hop));
    }
    // Fewest gates for the fleet first; ties to the shorter titan jump, which is less fatigue on the
    // ship that has to make it.
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

/// Gate to a titan, jump as far in as its range allows, gate the rest.
///
/// One option per titan that helps, best first. A titan that does not help is left out rather than
/// listed as a worse choice: the point of asking is to find the one that does, and the caller says so
/// when none of them do.
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
        // Getting to the titan is gates, and it can be none of them if you are already there.
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
        // No titan shortens it, so the honest answer is the route you would fly anyway, saying why.
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

/// A route flown the other way.
///
/// The hops reverse, but a hop's `kind`, distance and fuel describe the edge *into* it, so those have
/// to shift one place as well or the jump would be reported on the wrong system.
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
        let r = gate(&g, id, id, false, &Avoid::default(), &Default::default()).expect("a system can reach itself");
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

    /// A system on the avoid list is not routed through, and the endpoints are exempt: avoiding the
    /// system you are standing in would mean no route at all, which is a worse answer than an honest
    /// one.
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

    /// Every hop names a real system. An id that fell out of the graph would render as a number.
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
