//! The wormhole map: known holes as a graph of J-space and the k-space systems they open into.
//! Laid out as a tree per chain, from the side our characters are on; a system the user dragged
//! keeps its place, and whatever grows off it later is laid out relative to it.

use std::collections::{HashMap, HashSet, VecDeque};

use egui_phosphor::regular as icon;

use super::SpaiApp;
use crate::whdata::{self, Class};
use crate::wormholes::{Life, Mass, ShipSize, Wormhole};

/// Wide enough that at [`MIN_ZOOM`] a name still fits its scaled box, so no box grows past its
/// place when zoomed out.
const NODE: egui::Vec2 = egui::vec2(240.0, 50.0);
const COL: f32 = 360.0;
const ROW: f32 = 70.0;
const CHAIN_GAP: f32 = 50.0;
const GRID: f32 = 10.0;
/// How far out of a box an edge runs before it turns.
const STUB: f32 = 20.0;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SideTab {
    #[default]
    Info,
    Routes,
    Sigs,
}

/// The known hole behind signature `sig` in `system`, matched on its first three letters.
fn sig_hole<'a>(holes: &'a [Wormhole], system: i64, sig: &str) -> Option<&'a Wormhole> {
    holes.iter().find(|w| {
        let here = if w.system_id == system {
            w.signature.as_deref()
        } else if w.dest_system_id == Some(system) {
            w.dest_signature.as_deref()
        } else {
            None
        };
        here.is_some_and(|s| s.get(..3).is_some() && s.get(..3) == sig.get(..3))
    })
}

/// The probe scanner's group, short enough for a narrow column.
fn short_group(group: &str) -> &str {
    match group {
        "" => "?",
        "Combat Site" => "Combat",
        "Data Site" => "Data",
        "Relic Site" => "Relic",
        "Gas Site" => "Gas",
        "Ore Site" => "Ore",
        "Wormhole" => "WH",
        g => g,
    }
}

const LEGEND: &str = "Line colour: mass. Grey unknown, green over 50%, orange under 50%, red under 10%, yellow a drifter hole.\n\
Dashed: under 4 hours left.\n\
Dotted with a jump count: gates and bridges to a pinned system (focus view).\n\
Box border: the system effect's colour.\n\
Chains start from a system one of your characters is in, else from the system with the most holes.";

#[derive(Default)]
pub(crate) struct WhGraphView {
    pub(crate) table: bool,
    pan: egui::Vec2,
    pub(crate) selected: Option<i64>,
    /// Loaded once; written through on each drop.
    dragged: Option<HashMap<i64, egui::Pos2>>,
    drag: Option<(i64, egui::Pos2)>,
    pin_query: String,
    zoom: f32,
    fit_pending: bool,
    /// Only this system and those a few holes from it.
    pub(crate) focus: Option<i64>,
    focus_depth: u8,
    focus_dragged: HashMap<i64, egui::Pos2>,
    side_tab: SideTab,
    sigs: Option<(i64, Vec<crate::store::SystemSig>)>,
    sigs_pruned: bool,
    sig_note: Option<String>,
    /// The signature whose hole is open for changes.
    sig_edit: Option<String>,
    keep_missing: bool,
    /// The last routes worked out, and what they were worked out for.
    route_cache: Option<(u64, Vec<Option<Vec<egui::Pos2>>>)>,
    /// Gate jumps from each pinned system, for joining it to the focused chain.
    gate_dist: HashMap<i64, HashMap<i64, u32>>,
}

impl WhGraphView {
    fn depth(&self) -> u8 {
        if self.focus_depth == 0 { 2 } else { self.focus_depth }
    }

    pub(crate) fn set_focus(&mut self, focus: Option<i64>) {
        self.focus = focus;
        self.focus_dragged.clear();
        self.gate_dist.clear();
        self.fit_pending = true;
    }

    fn fit(&mut self, pos: &HashMap<i64, egui::Pos2>, rect: egui::Rect) {
        let Some(bounds) = pos.values().fold(None::<egui::Rect>, |b, p| {
            let r = egui::Rect::from_min_size(*p, NODE);
            Some(b.map_or(r, |b| b.union(r)))
        }) else {
            self.zoom = 1.0;
            self.pan = egui::vec2(24.0, 24.0);
            return;
        };
        let room = rect.shrink2(egui::vec2(24.0, 24.0));
        self.zoom = (room.width() / bounds.width()).min(room.height() / bounds.height()).clamp(MIN_ZOOM, 1.0);
        self.pan = (room.center() - rect.min) - bounds.center().to_vec2() * self.zoom;
    }

    /// Drops the cached signature list, after a group changed it.
    pub(crate) fn forget_sigs(&mut self) {
        self.sigs = None;
    }

    #[cfg(test)]
    pub(crate) fn show_sigs(&mut self, system: i64, sigs: Vec<crate::store::SystemSig>) {
        self.side_tab = SideTab::Sigs;
        self.sigs = Some((system, sigs));
        self.sig_edit = self.sigs.as_ref().and_then(|(_, l)| l.iter().find(|s| s.group == "Wormhole")).map(|s| s.sig.clone());
    }

    #[cfg(test)]
    pub(crate) fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    /// Routes for `links`, worked out again only when a box moves or the links change.
    fn routes(
        &mut self,
        boxes: &HashMap<i64, egui::Rect>,
        links: &[(i64, i64, bool)],
        parent: &HashMap<i64, i64>,
    ) -> Vec<Option<Vec<egui::Pos2>>> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut placed: Vec<(i64, i32, i32)> = boxes.iter().map(|(id, r)| (*id, r.min.x as i32, r.min.y as i32)).collect();
        placed.sort_unstable();
        placed.hash(&mut h);
        links.hash(&mut h);
        let key = h.finish();
        if let Some((k, r)) = &self.route_cache {
            if *k == key {
                return r.clone();
            }
        }
        let r = route_all(boxes, links, parent);
        self.route_cache = Some((key, r.clone()));
        r
    }

    fn zoom_by(&mut self, factor: f32, rect: egui::Rect) {
        let old = self.zoom;
        let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let rel = rect.center() - (rect.min + self.pan);
        self.pan += rel * (1.0 - new / old);
        self.zoom = new;
    }
}

/// Centres for a label of `size` along the straight leg from `from` towards `to`, nearest `from`
/// first, each keeping the whole label on the leg.
fn along(from: egui::Pos2, to: egui::Pos2, size: egui::Vec2) -> Vec<egui::Pos2> {
    let v = to - from;
    let len = v.length();
    if len < 1.0 {
        return Vec::new();
    }
    let dir = v / len;
    let half = if dir.x.abs() > dir.y.abs() { size.x / 2.0 } else { size.y / 2.0 };
    let mut out = Vec::new();
    let mut d = half + 4.0;
    while d <= len + half {
        out.push(from + dir * d);
        d += 6.0;
    }
    out
}

/// The first of `spots` where a label of `size` touches no label in `taken` and no system box,
/// preferring spots off every line in `others`: a label on a stretch two lines share could belong
/// to either.
fn first_free(
    spots: impl IntoIterator<Item = egui::Pos2>,
    size: egui::Vec2,
    taken: &[egui::Rect],
    boxes: &[egui::Rect],
    others: &[&[egui::Pos2]],
) -> Option<egui::Rect> {
    let rects: Vec<egui::Rect> = spots.into_iter().map(|c| egui::Rect::from_center_size(c, size)).collect();
    let free = |r: &egui::Rect| !taken.iter().any(|t| t.expand(2.0).intersects(*r)) && !boxes.iter().any(|b| b.expand(4.0).intersects(*r));
    let on_other = |r: &egui::Rect| {
        others.iter().any(|l| l.windows(2).any(|s| egui::Rect::from_two_pos(s[0], s[1]).expand(1.0).intersects(*r)))
    };
    rects.iter().find(|r| free(r) && !on_other(r)).or_else(|| rects.iter().find(|r| free(r))).copied()
}

/// The connected groups of systems in `edges`.
fn components(edges: &[(i64, i64)]) -> Vec<Vec<i64>> {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for &(a, b) in edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    let mut keys: Vec<i64> = adj.keys().copied().collect();
    keys.sort_unstable();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for k in keys {
        if !seen.insert(k) {
            continue;
        }
        let mut comp = vec![k];
        let mut q = VecDeque::from([k]);
        while let Some(u) = q.pop_front() {
            for v in &adj[&u] {
                if seen.insert(*v) {
                    comp.push(*v);
                    q.push_back(*v);
                }
            }
        }
        out.push(comp);
    }
    out
}

/// The systems at most `depth` known holes from `from`.
fn within(holes: &[Wormhole], from: i64, depth: u8) -> HashSet<i64> {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for w in holes {
        if let Some(b) = w.dest_system_id {
            adj.entry(w.system_id).or_default().push(b);
            adj.entry(b).or_default().push(w.system_id);
        }
    }
    let mut seen = HashSet::from([from]);
    let mut ring = vec![from];
    for _ in 0..depth {
        let next: Vec<i64> = ring.iter().flat_map(|n| adj.get(n).into_iter().flatten()).copied().filter(|n| seen.insert(*n)).collect();
        ring = next;
    }
    seen
}

/// Right-angled routes for every link, from its first system to its second, in map units.
///
/// Each link tries a handful of shapes (a bend near either end or midway, a detour round the right
/// or left, down-across-down) with entry points a little off the box centre, and takes the
/// cheapest: through another system's box is ruled out; along another link's line is costly unless
/// both are holes out of the same system (their shared trunk is the fan-out); then short and few
/// bends. Links are routed in order, each seeing the ones before it.
pub(crate) fn route_all(
    boxes: &HashMap<i64, egui::Rect>,
    links: &[(i64, i64, bool)],
    parent: &HashMap<i64, i64>,
) -> Vec<Option<Vec<egui::Pos2>>> {
    let mut done: Vec<(Vec<egui::Pos2>, i64, i64, bool)> = Vec::new();
    let mut out: Vec<Option<Vec<egui::Pos2>>> = vec![None; links.len()];
    let mut degree: HashMap<i64, usize> = HashMap::new();
    for &(a, b, _) in links {
        *degree.entry(a).or_default() += 1;
        *degree.entry(b).or_default() += 1;
    }
    // Holes before gate links, and within each the straightest first: a level link keeps the
    // middle of its box and the others fan out above and below it.
    let mut order: Vec<usize> = (0..links.len()).collect();
    let rise = |i: usize| {
        let (a, b, _) = links[i];
        match (boxes.get(&a), boxes.get(&b)) {
            (Some(ra), Some(rb)) => (ra.center().y - rb.center().y).abs() as i64,
            _ => i64::MAX,
        }
    };
    order.sort_by_key(|&i| (!links[i].2, rise(i), i));
    for i in order {
        let (a, b, hole) = links[i];
        let (Some(ra), Some(rb)) = (boxes.get(&a), boxes.get(&b)) else { continue };
        // On a tie the bend sits by the system the branch grows from (else the busier one), so
        // its links fan out from one trunk there.
        let a_hub = match (parent.get(&b) == Some(&a), parent.get(&a) == Some(&b)) {
            (true, _) => true,
            (_, true) => false,
            _ => degree.get(&a) >= degree.get(&b),
        };
        let best = candidates(*ra, *rb, a_hub)
            .into_iter()
            .map(|p| {
                let cost = route_cost(&p, a, b, hole, boxes, &done);
                (p, cost)
            })
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(p, _)| p);
        if let Some(p) = &best {
            done.push((p.clone(), a, b, hole));
        }
        out[i] = best;
    }
    out
}

fn candidates(a: egui::Rect, b: egui::Rect, a_hub: bool) -> Vec<Vec<egui::Pos2>> {
    use egui::pos2;
    const OFFSETS: [f32; 5] = [0.0, 8.0, -8.0, 16.0, -16.0];
    let mut out = Vec::new();
    let (ya, yb) = (a.center().y, b.center().y);
    let (xa, xb) = (a.center().x, b.center().x);
    for oa in OFFSETS {
        for ob in OFFSETS {
            let (pa, pb) = (ya + oa, yb + ob);
            // Side to side, with the bend near either end or midway.
            if b.left() - a.right() >= 2.0 * STUB {
                let (near_a, near_b) = (a.right() + STUB, b.left() - STUB);
                let order = if a_hub { [near_a, near_b] } else { [near_b, near_a] };
                for x in [order[0], order[1], (a.right() + b.left()) / 2.0] {
                    out.push(vec![pos2(a.right(), pa), pos2(x, pa), pos2(x, pb), pos2(b.left(), pb)]);
                }
            }
            if a.left() - b.right() >= 2.0 * STUB {
                let (near_a, near_b) = (a.left() - STUB, b.right() + STUB);
                let order = if a_hub { [near_a, near_b] } else { [near_b, near_a] };
                for x in [order[0], order[1], (a.left() + b.right()) / 2.0] {
                    out.push(vec![pos2(a.left(), pa), pos2(x, pa), pos2(x, pb), pos2(b.right(), pb)]);
                }
            }
            // Round the right or the left, for boxes in one column.
            for k in 1..=3 {
                let x = a.right().max(b.right()) + STUB * k as f32;
                out.push(vec![pos2(a.right(), pa), pos2(x, pa), pos2(x, pb), pos2(b.right(), pb)]);
                let x = a.left().min(b.left()) - STUB * k as f32;
                out.push(vec![pos2(a.left(), pa), pos2(x, pa), pos2(x, pb), pos2(b.left(), pb)]);
            }
            // Down, across and down.
            let (qa, qb) = (xa + oa, xb + ob);
            if b.top() - a.bottom() >= 2.0 * STUB {
                for y in [a.bottom() + STUB, b.top() - STUB] {
                    out.push(vec![pos2(qa, a.bottom()), pos2(qa, y), pos2(qb, y), pos2(qb, b.top())]);
                }
            }
            if a.top() - b.bottom() >= 2.0 * STUB {
                for y in [a.top() - STUB, b.bottom() + STUB] {
                    out.push(vec![pos2(qa, a.top()), pos2(qa, y), pos2(qb, y), pos2(qb, b.bottom())]);
                }
            }
        }
    }
    out.into_iter().map(simplify).collect()
}

fn route_cost(path: &[egui::Pos2], a: i64, b: i64, hole: bool, boxes: &HashMap<i64, egui::Rect>, done: &[(Vec<egui::Pos2>, i64, i64, bool)]) -> f32 {
    let mut cost = 0.0;
    for s in path.windows(2) {
        let seg = egui::Rect::from_two_pos(s[0], s[1]).expand(0.5);
        for (id, r) in boxes {
            if *id != a && *id != b && r.shrink(1.0).intersects(seg) {
                cost += 1_000_000.0;
            }
        }
        cost += (s[1] - s[0]).length();
        for (other, oa, ob, other_hole) in done {
            // Holes out of one system share their trunk; any other shared stretch is ambiguous.
            let fan_out = hole && *other_hole && (*oa == a || *ob == a || *oa == b || *ob == b);
            let per_px = if fan_out { 0.0 } else { 60.0 };
            for t in other.windows(2) {
                cost += per_px * overlap(s[0], s[1], t[0], t[1]);
                if !fan_out && crosses(s[0], s[1], t[0], t[1]) {
                    cost += 40.0;
                }
            }
        }
    }
    // Off-centre ports are for keeping off other lines, not for shaving a few pixels.
    let off = |p: egui::Pos2, r: &egui::Rect| {
        if (p.x - r.left()).abs() < 0.5 || (p.x - r.right()).abs() < 0.5 { (p.y - r.center().y).abs() } else { (p.x - r.center().x).abs() }
    };
    let ports = boxes.get(&a).map_or(0.0, |r| off(path[0], r)) + boxes.get(&b).map_or(0.0, |r| off(path[path.len() - 1], r));
    cost + 25.0 * path.len().saturating_sub(2) as f32 + 4.0 * ports
}

/// Whether an upright and a flat segment cross inside both, not just touch at an end.
fn crosses(a0: egui::Pos2, a1: egui::Pos2, b0: egui::Pos2, b1: egui::Pos2) -> bool {
    let inside = |v: f32, p: f32, q: f32| v > p.min(q) + 0.5 && v < p.max(q) - 0.5;
    let cross = |u0: egui::Pos2, u1: egui::Pos2, f0: egui::Pos2, f1: egui::Pos2| {
        (u0.x - u1.x).abs() < 0.5 && (f0.y - f1.y).abs() < 0.5 && inside(u0.x, f0.x, f1.x) && inside(f0.y, u0.y, u1.y)
    };
    cross(a0, a1, b0, b1) || cross(b0, b1, a0, a1)
}

/// How long two axis-aligned segments run along each other.
fn overlap(a0: egui::Pos2, a1: egui::Pos2, b0: egui::Pos2, b1: egui::Pos2) -> f32 {
    let run = |p0: f32, p1: f32, q0: f32, q1: f32| (p0.max(p1).min(q0.max(q1)) - p0.min(p1).max(q0.min(q1))).max(0.0);
    let flat = |p: egui::Pos2, q: egui::Pos2| (p.y - q.y).abs() < 0.5;
    let upright = |p: egui::Pos2, q: egui::Pos2| (p.x - q.x).abs() < 0.5;
    if flat(a0, a1) && flat(b0, b1) && (a0.y - b0.y).abs() < 3.0 {
        run(a0.x, a1.x, b0.x, b1.x)
    } else if upright(a0, a1) && upright(b0, b1) && (a0.x - b0.x).abs() < 3.0 {
        run(a0.y, a1.y, b0.y, b1.y)
    } else {
        0.0
    }
}

/// Label centres along the whole of `path` from one end, keeping the label on a leg.
fn walk_all(path: &[egui::Pos2], from_start: bool, size: egui::Vec2) -> Vec<egui::Pos2> {
    let pts: Vec<egui::Pos2> = if from_start { path.to_vec() } else { path.iter().rev().copied().collect() };
    pts.windows(2).flat_map(|s| along(s[0], s[1], size)).collect()
}

/// [`first_free`] with no fallback: only spots off every other line.
fn first_free_strict(
    spots: impl IntoIterator<Item = egui::Pos2>,
    size: egui::Vec2,
    taken: &[egui::Rect],
    boxes: &[egui::Rect],
    others: &[&[egui::Pos2]],
) -> Option<egui::Rect> {
    let on_other = |r: &egui::Rect| {
        others.iter().any(|l| l.windows(2).any(|s| egui::Rect::from_two_pos(s[0], s[1]).expand(1.0).intersects(*r)))
    };
    spots
        .into_iter()
        .map(|c| egui::Rect::from_center_size(c, size))
        .find(|r| !taken.iter().any(|t| t.expand(2.0).intersects(*r)) && !boxes.iter().any(|b| b.expand(4.0).intersects(*r)) && !on_other(r))
}

/// Label centres along `path` from one end towards its middle, keeping the label on a leg.
fn walk(path: &[egui::Pos2], from_start: bool, size: egui::Vec2) -> Vec<egui::Pos2> {
    let pts: Vec<egui::Pos2> = if from_start { path.to_vec() } else { path.iter().rev().copied().collect() };
    let total: f32 = pts.windows(2).map(|s| (s[1] - s[0]).length()).sum();
    let mut out = Vec::new();
    let mut gone = 0.0;
    for s in pts.windows(2) {
        let len = (s[1] - s[0]).length();
        for c in along(s[0], s[1], size) {
            if gone + (c - s[0]).length() <= total / 2.0 {
                out.push(c);
            }
        }
        gone += len;
    }
    out
}

/// Drops points that do not turn the path.
fn simplify(path: Vec<egui::Pos2>) -> Vec<egui::Pos2> {
    let mut out: Vec<egui::Pos2> = Vec::with_capacity(path.len());
    for p in path {
        if out.last().is_some_and(|q| (*q - p).length() < 0.5) {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            if ((b - a).x * (p - b).y - (b - a).y * (p - b).x).abs() < 0.5 {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// The path with each corner replaced by a quarter curve of up to `radius`.
fn rounded(path: &[egui::Pos2], radius: f32) -> Vec<egui::Pos2> {
    if path.len() < 3 {
        return path.to_vec();
    }
    let mut out = vec![path[0]];
    for w in path.windows(3) {
        let (a, p, c) = (w[0], w[1], w[2]);
        let r = radius.min((a - p).length() / 2.0).min((c - p).length() / 2.0);
        let (s, e) = (p + (a - p).normalized() * r, p + (c - p).normalized() * r);
        for i in 0..=6 {
            let t = i as f32 / 6.0;
            let q = s.lerp(p, t).lerp(p.lerp(e, t), t);
            out.push(q);
        }
    }
    out.push(path[path.len() - 1]);
    out
}

/// A position for every node, in BFS order per chain so a node comes after its parent.
pub(crate) fn auto_layout(edges: &[(i64, i64)], score: impl Fn(i64) -> i64) -> Vec<(i64, Option<i64>, egui::Pos2)> {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for &(a, b) in edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    for v in adj.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let mut nodes: Vec<i64> = adj.keys().copied().collect();
    nodes.sort_unstable();
    let mut seen = HashSet::new();
    let mut chains: Vec<Vec<i64>> = Vec::new();
    for &n in &nodes {
        if seen.insert(n) {
            let mut comp = vec![n];
            let mut q = VecDeque::from([n]);
            while let Some(u) = q.pop_front() {
                for &v in &adj[&u] {
                    if seen.insert(v) {
                        comp.push(v);
                        q.push_back(v);
                    }
                }
            }
            chains.push(comp);
        }
    }
    let rank = |c: &Vec<i64>| (c.iter().map(|n| score(*n)).max().unwrap_or(0), c.len());
    chains.sort_by_key(|c| std::cmp::Reverse(rank(c)));

    let mut out = Vec::new();
    let mut top = 0.0;
    for comp in chains {
        let root = *comp.iter().max_by_key(|n| (score(**n), adj[*n].len(), -**n)).unwrap();
        let mut parent: HashMap<i64, Option<i64>> = HashMap::from([(root, None)]);
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut order = vec![root];
        let mut q = VecDeque::from([root]);
        while let Some(u) = q.pop_front() {
            for &v in &adj[&u] {
                if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(v) {
                    e.insert(Some(u));
                    children.entry(u).or_default().push(v);
                    order.push(v);
                    q.push_back(v);
                }
            }
        }
        // Leaves take one row each; a parent sits level with the middle of its children.
        let mut y: HashMap<i64, f32> = HashMap::new();
        let mut next_row = 0.0f32;
        fn place(n: i64, children: &HashMap<i64, Vec<i64>>, y: &mut HashMap<i64, f32>, next_row: &mut f32) -> f32 {
            let kids = children.get(&n).cloned().unwrap_or_default();
            let at = if kids.is_empty() {
                let r = *next_row;
                *next_row += 1.0;
                r
            } else {
                let rows: Vec<f32> = kids.iter().map(|k| place(*k, children, y, next_row)).collect();
                (rows[0] + rows[rows.len() - 1]) / 2.0
            };
            y.insert(n, at);
            at
        }
        place(root, &children, &mut y, &mut next_row);
        let mut depth: HashMap<i64, u32> = HashMap::from([(root, 0)]);
        for &n in &order {
            if let Some(Some(p)) = parent.get(&n) {
                depth.insert(n, depth[p] + 1);
            }
        }
        for &n in &order {
            out.push((n, parent[&n], egui::pos2(depth[&n] as f32 * COL, top + y[&n] * ROW)));
        }
        top += next_row * ROW + CHAIN_GAP;
    }
    out
}

/// The auto layout with dragged systems kept where they were put, and a system that was not
/// dragged moved along with its parent, then down until it has room.
pub(crate) fn place(auto: &[(i64, Option<i64>, egui::Pos2)], dragged: &HashMap<i64, egui::Pos2>) -> HashMap<i64, egui::Pos2> {
    let auto_at: HashMap<i64, egui::Pos2> = auto.iter().map(|(n, _, p)| (*n, *p)).collect();
    let mut at: HashMap<i64, egui::Pos2> = HashMap::new();
    let clear = |at: &HashMap<i64, egui::Pos2>, p: egui::Pos2| {
        at.values().all(|q| (q.x - p.x).abs() >= NODE.x + 10.0 || (q.y - p.y).abs() >= NODE.y + 10.0)
    };
    for (n, _, _) in auto {
        if let Some(p) = dragged.get(n) {
            at.insert(*n, *p);
        }
    }
    for (n, parent, p) in auto {
        if at.contains_key(n) {
            continue;
        }
        let mut p = match parent {
            Some(par) => at[par] + (*p - auto_at[par]),
            None => *p,
        };
        let moved = parent.is_some_and(|par| at[&par] != auto_at[&par]) || !dragged.is_empty();
        if moved {
            let mut tries = 0;
            while !clear(&at, p) && tries < 200 {
                p.y += ROW;
                tries += 1;
            }
        }
        at.insert(*n, p);
    }
    at
}

fn snap(p: egui::Pos2) -> egui::Pos2 {
    egui::pos2((p.x / GRID).round() * GRID, (p.y / GRID).round() * GRID)
}

pub(crate) fn class_color(class: Class, security: f64) -> egui::Color32 {
    use egui::Color32 as C;
    match class {
        Class::W(1..=3) => C::from_rgb(0x4f, 0x9c, 0xff),
        Class::W(4 | 5) => C::from_rgb(0xff, 0x9b, 0x3d),
        Class::W(6) => C::from_rgb(0xff, 0x55, 0x55),
        Class::W(_) => C::from_rgb(0xc0, 0x7c, 0xff),
        Class::Thera => C::from_rgb(0xe6, 0xd0, 0x4a),
        Class::Drifter(_) => C::from_rgb(0xff, 0x6e, 0xc7),
        Class::Pochven => C::from_rgb(0xd0, 0x30, 0x30),
        _ => super::security_color(security),
    }
}

fn effect_color(effect: &str) -> egui::Color32 {
    use egui::Color32 as C;
    match effect {
        "Magnetar" => C::from_rgb(0xe0, 0x6f, 0xdf),
        "Red Giant" => C::from_rgb(0xd9, 0x44, 0x44),
        "Pulsar" => C::from_rgb(0x42, 0x8b, 0xf5),
        "Wolf-Rayet Star" => C::from_rgb(0xe8, 0x8a, 0x2e),
        "Cataclysmic Variable" => C::from_rgb(0xe8, 0xe0, 0x6a),
        _ => C::from_rgb(0x88, 0x88, 0x88),
    }
}

fn mass_color(m: Option<Mass>) -> egui::Color32 {
    use egui::Color32 as C;
    match m {
        None => C::from_rgb(0x7d, 0x8a, 0x99),
        Some(Mass::Fresh) => C::from_rgb(0x6f, 0xc2, 0x76),
        Some(Mass::Reduced) => C::from_rgb(0xf0, 0xa0, 0x30),
        Some(Mass::Critical) => C::from_rgb(0xff, 0x4a, 0x4a),
    }
}

fn dist_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-3)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

impl SpaiApp {
    fn wh_graph_visible(&self, now: i64) -> Vec<&Wormhole> {
        let (fd, fs, fe) = (self.wh_filter_dest, self.wh_filter_source, self.wh_filter_expiring);
        self.wh_cache
            .iter()
            .filter(|w| {
                fd.is_none_or(|d| w.dest == d)
                    && fs.is_none_or(|s| w.source == s)
                    && (!fe || w.hours_left(now).is_some_and(|h| h <= 4))
            })
            .collect()
    }

    pub(crate) fn wh_graph_view(&mut self, ui: &mut egui::Ui) {
        let Some(geo) = self.systems.clone() else {
            ui.label(egui::RichText::new("The system map is still loading.").weak());
            return;
        };
        let now = chrono::Utc::now().timestamp();
        let mut holes: Vec<Wormhole> = self.wh_graph_visible(now).into_iter().cloned().collect();
        self.wh_graph_list(ui, &geo, &holes);
        let focus = self.wh_graph.focus;
        if let Some(f) = focus {
            let near = within(&holes, f, self.wh_graph.depth());
            holes.retain(|w| near.contains(&w.system_id) && w.dest_system_id.is_none_or(|b| near.contains(&b)));
        }
        let mut edges: Vec<(i64, i64)> = holes.iter().filter_map(|w| Some((w.system_id, w.dest_system_id?))).collect();
        // Pinned systems join the holes by gates, so they tie the chains together.
        let mut gate_links: Vec<(i64, i64, u32)> = Vec::new();
        let pins: Vec<i64> = self.settings.wh_route_pins.iter().filter_map(|p| geo.lookup(p).map(|i| i.id)).collect();
        let kspace = |id: &i64| geo.info_of(*id).is_some_and(|i| whdata::class_of(*id, i.security, &i.region).is_kspace());
        let chains: Vec<Vec<i64>> = match focus {
            Some(f) => {
                let mut all: Vec<i64> = edges.iter().flat_map(|(a, b)| [*a, *b]).chain([f]).collect();
                all.sort_unstable();
                all.dedup();
                vec![all]
            }
            None => components(&edges),
        };
        // Where a pinned system is reached by gates: itself in k-space; a pinned wormhole system
        // (Thera, a J-system) has no gates, so the k-space exits of its own chain stand in for it.
        let anchors: HashMap<i64, Vec<i64>> = pins
            .iter()
            .map(|p| {
                let own = if kspace(p) {
                    vec![*p]
                } else {
                    chains.iter().find(|c| c.contains(p)).map(|c| c.iter().copied().filter(|e| kspace(e)).collect()).unwrap_or_default()
                };
                (*p, own)
            })
            .collect();
        for a in anchors.values().flatten() {
            self.wh_graph.gate_dist.entry(*a).or_insert_with(|| geo.distances_from(*a, 100));
        }
        let dist = |from: i64, exit: i64| self.wh_graph.gate_dist.get(&from).and_then(|d| d.get(&exit).copied());
        // Every chain reaches every pinned system through its own closest exit; a pinned system is
        // one box however many chains lead to it. A wormhole pin is met at its chain's closest exit.
        for chain in &chains {
            for pid in pins.iter().filter(|p| !chain.contains(p)) {
                let best = anchors[pid]
                    .iter()
                    .flat_map(|a| chain.iter().filter(|e| kspace(e)).filter_map(move |e| Some((*e, *a, dist(*a, *e)?))))
                    .min_by_key(|(_, _, n)| *n);
                if let Some((exit, anchor, n)) = best {
                    if !gate_links.iter().any(|(x, y, _)| (*x, *y) == (exit, anchor) || (*x, *y) == (anchor, exit)) {
                        gate_links.push((exit, anchor, n));
                    }
                }
            }
        }
        edges.extend(gate_links.iter().map(|(e, p, _)| (*e, *p)));
        let chars: HashMap<String, (i64, bool)> = self.player.lock().unwrap().locations.clone();
        let mut here: HashMap<i64, Vec<String>> = HashMap::new();
        for (name, (sys, _)) in &chars {
            here.entry(*sys).or_default().push(name.clone());
        }
        let score = |id: i64| {
            if focus == Some(id) {
                return i64::MAX;
            }
            // Pinned systems are where the chains hang from, unless a character is somewhere.
            let pinned = if pins.contains(&id) { 50 } else { 0 };
            here.get(&id).map_or(pinned, |v| 100 + v.len() as i64)
        };
        let auto = auto_layout(&edges, score);
        if self.wh_graph.dragged.is_none() {
            self.wh_graph.dragged = Some(
                self.store
                    .as_ref()
                    .map(|s| s.wh_layout().into_iter().map(|(id, (x, y))| (id, egui::pos2(x, y))).collect())
                    .unwrap_or_default(),
            );
        }
        // A focused view is laid out around its system; moving things there is only for the moment.
        let mut pos = if focus.is_some() {
            place(&auto, &self.wh_graph.focus_dragged)
        } else {
            place(&auto, self.wh_graph.dragged.as_ref().unwrap())
        };
        if let Some((id, p)) = self.wh_graph.drag {
            pos.insert(id, p);
        }

        // Holes whose far side is unknown, by the system they are in.
        let mut open: HashMap<i64, usize> = HashMap::new();
        for w in holes.iter().filter(|w| w.dest_system_id.is_none()) {
            *open.entry(w.system_id).or_default() += 1;
        }

        self.wh_graph_side(ui, &geo, &holes, &chars, now);

        let mut focus_on: Option<Option<i64>> = None;
        // The view's own controls, a row above the canvas.
        let focus_name = focus.and_then(|f| geo.info_of(f)).map(|i| i.name.clone());
        let mut zoom_step: Option<f32> = None;
        {
            {
                ui.horizontal_wrapped(|ui| {
                        use crate::app::SteadySelect as _;
                        ui.add_space(8.0);
                        if ui.button(icon::MINUS).on_hover_text("Zoom out").clicked() {
                            zoom_step = Some(1.0 / 1.25);
                        }
                        ui.label(format!("{:.0}%", self.wh_graph.zoom * 100.0));
                        if ui.button(icon::PLUS).on_hover_text("Zoom in").clicked() {
                            zoom_step = Some(1.25);
                        }
                        if ui.button(format!("{}  Fit", icon::CORNERS_OUT)).on_hover_text("Show everything").clicked() {
                            self.wh_graph.fit_pending = true;
                        }
                        ui.label(egui::RichText::new(icon::QUESTION).weak()).on_hover_text(LEGEND);
                        if let Some(name) = &focus_name {
                            ui.separator();
                            ui.label(format!("{}  {name}", icon::CROSSHAIR));
                            for d in 1..=3u8 {
                                if ui
                                    .menu_label(self.wh_graph.depth() == d, format!("{d}"))
                                    .on_hover_text(format!("Systems up to {d} hole{} away", if d == 1 { "" } else { "s" }))
                                    .clicked()
                                {
                                    self.wh_graph.focus_depth = d;
                                    self.wh_graph.fit_pending = true;
                                }
                            }
                            if ui.button(format!("{}  Show all", icon::X)).clicked() {
                                focus_on = Some(None);
                            }
                        }
                });
            }
        }

        let rect = ui.available_rect_before_wrap();
        if let Some(f) = zoom_step {
            self.wh_graph.zoom_by(f, rect);
        }
        let bg = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        if self.wh_graph.zoom <= 0.0 {
            // First look: everything if it stays readable, else the top left at full size.
            self.wh_graph.fit(&pos, rect);
            if self.wh_graph.zoom < 0.6 {
                self.wh_graph.zoom = 1.0;
                self.wh_graph.pan = egui::vec2(24.0, 24.0);
            }
        }
        if std::mem::take(&mut self.wh_graph.fit_pending) {
            self.wh_graph.fit(&pos, rect);
        }
        if bg.dragged() && self.wh_graph.drag.is_none() {
            self.wh_graph.pan += bg.drag_delta();
        }
        if bg.clicked() {
            self.wh_graph.selected = None;
        }
        if bg.hovered() || ui.rect_contains_pointer(rect) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = self.wh_graph.zoom;
                let new = (old * (scroll * 0.003).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let rel = m - (rect.min + self.wh_graph.pan);
                    self.wh_graph.pan += rel * (1.0 - new / old);
                }
                self.wh_graph.zoom = new;
            }
        }
        let zoom = self.wh_graph.zoom;
        let origin = rect.min + self.wh_graph.pan;
        let to_screen = |p: egui::Pos2| origin + p.to_vec2() * zoom;
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();
        let body = egui::TextStyle::Body.resolve(ui.style());
        // Names never shrink below a readable size; the boxes are wide enough to hold them at any zoom.
        let font = egui::FontId::new((body.size * zoom).clamp(12.0, body.size * 1.5), body.family.clone());
        let detail = zoom >= 0.65;
        let line1_of = |id: i64| {
            let info = geo.info_of(id)?;
            let c = whdata::class_of(id, info.security, &info.region);
            let mut job = egui::text::LayoutJob::default();
            job.append(&format!("{:.1}", info.security), 0.0, egui::TextFormat::simple(font.clone(), class_color(c, info.security)));
            job.append(&info.name, 6.0, egui::TextFormat::simple(font.clone(), visuals.strong_text_color()));
            Some(painter.layout_job(job))
        };
        let rects: HashMap<i64, (egui::Rect, Option<std::sync::Arc<egui::Galley>>)> = pos
            .iter()
            .map(|(id, p)| {
                let g = line1_of(*id);
                let r = egui::Rect::from_min_size(to_screen(*p), NODE * zoom);
                (*id, (r, g))
            })
            .collect();

        if edges.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No hole with both sides known yet. The table has the rest.",
                body.clone(),
                visuals.weak_text_color(),
            );
        }

        let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let mut hovered_edge: Option<&Wormhole> = None;
        // Routes and label spots are worked out on the map's own boxes, in map units, and only
        // then scaled: neither how an edge runs nor where its labels sit depends on the zoom.
        let world: HashMap<i64, egui::Rect> = pos.iter().map(|(id, p)| (*id, egui::Rect::from_min_size(*p, NODE))).collect();
        let links: Vec<(i64, i64, bool)> = holes
            .iter()
            .map(|w| (w.system_id, w.dest_system_id.unwrap_or(0), true))
            .chain(gate_links.iter().map(|(e, p, _)| (*e, *p, false)))
            .collect();
        let parent: HashMap<i64, i64> = auto.iter().filter_map(|(n, p, _)| Some((*n, (*p)?))).collect();
        let routes = self.wh_graph.routes(&world, &links, &parent);
        let hole_paths: Vec<Option<Vec<egui::Pos2>>> = routes[..holes.len()].to_vec();
        let link_paths: Vec<Option<Vec<egui::Pos2>>> = routes[holes.len()..].to_vec();
        let screen = |path: &[egui::Pos2]| rounded(&path.iter().map(|p| to_screen(*p)).collect::<Vec<_>>(), 10.0 * zoom);
        let hit = |line: &[egui::Pos2]| pointer.is_some_and(|p| line.windows(2).any(|s| dist_to_segment(p, s[0], s[1]) < 6.0));

        // Every line first, so no line is ever drawn over a label.
        for (wi, w) in holes.iter().enumerate() {
            let Some(path) = &hole_paths[wi] else { continue };
            let line = screen(path);
            let color = if w.is_drifter { crate::theme::standing::WARNING } else { mass_color(w.mass) };
            let hot = hovered_edge.is_none() && hit(&line);
            let width = if hot { 4.0 } else { 2.5 };
            let short = matches!(w.life, Some(Life::Under4h | Life::Under1h | Life::Expired))
                || w.hours_left(now).is_some_and(|h| h < 4);
            if short {
                painter.extend(egui::Shape::dashed_line(&line, egui::Stroke::new(width, color), 8.0, 6.0));
            } else {
                painter.add(egui::Shape::line(line, egui::Stroke::new(width, color)));
            }
            if hot {
                hovered_edge = Some(w);
            }
        }
        let mut hovered_link: Option<(i64, i64, u32)> = None;
        let link_color = visuals.weak_text_color();
        for (li, &(exit, pin, n)) in gate_links.iter().enumerate() {
            let Some(path) = &link_paths[li] else { continue };
            let line = screen(path);
            let hot = hovered_edge.is_none() && hovered_link.is_none() && hit(&line);
            painter.extend(egui::Shape::dotted_line(&line, link_color, 6.0, if hot { 2.0 } else { 1.3 }));
            if hot {
                hovered_link = Some((exit, pin, n));
            }
        }

        // Then the labels, in map units, each off every other label, every box and, where it can
        // be, every line that is not its own: a label on a shared stretch could belong to either.
        let lines: Vec<&[egui::Pos2]> = hole_paths.iter().chain(link_paths.iter()).map(|p| p.as_deref().unwrap_or(&[])).collect();
        let others = |own: usize| lines.iter().enumerate().filter(move |(i, _)| *i != own).map(|(_, l)| *l).collect::<Vec<_>>();
        let boxes: Vec<egui::Rect> = pos.values().map(|p| egui::Rect::from_min_size(*p, NODE)).collect();
        let mut taken: Vec<egui::Rect> = Vec::new();
        let draw_label = |r: egui::Rect, g: std::sync::Arc<egui::Galley>, border: Option<egui::Color32>| {
            let sr = egui::Rect::from_min_max(to_screen(r.min), to_screen(r.max));
            match border {
                Some(c) => painter.rect(sr, 3.0, visuals.extreme_bg_color, egui::Stroke::new(1.0, c), egui::StrokeKind::Outside),
                None => painter.rect_filled(sr, 3.0, visuals.extreme_bg_color),
            };
            painter.galley(sr.center() - g.size() / 2.0, g, visuals.text_color());
        };
        let pad = egui::vec2(8.0, 2.0);
        if detail {
            for (wi, w) in holes.iter().enumerate() {
                let Some(path) = &hole_paths[wi] else { continue };
                // Routes run from the hole's own system to the far one; each side's tag stays on
                // its own half, as near its own box as it can be off the other lines.
                // Only on a stretch this hole has to itself: on a shared trunk a tag could belong
                // to any of the holes on it. No such spot, no tag (hovering the line says it).
                let own = others(wi);
                for (sig, near) in [(&w.dest_signature, false), (&w.signature, true)] {
                    let Some(sig) = sig else { continue };
                    let g = painter.layout_no_wrap(sig.chars().take(3).collect(), font.clone(), visuals.text_color());
                    let size = (g.size() + pad) / zoom;
                    let spots = walk_all(path, near, size);
                    let Some(r) = first_free_strict(spots, size, &taken, &boxes, &own) else { continue };
                    taken.push(r);
                    draw_label(r, g, None);
                }
            }
        }
        for (li, &(_, _, n)) in gate_links.iter().enumerate() {
            let Some(path) = &link_paths[li] else { continue };
            let g = painter.layout_no_wrap(format!("{n}j"), font.clone(), visuals.text_color());
            let size = (g.size() + pad) / zoom;
            // Anywhere along its own route, the legs nearest the pinned system first; a label with
            // nowhere free is left off (hovering the line still says it) rather than drawn over another.
            let mut spots = walk(path, false, size);
            spots.extend(walk(path, true, size));
            let Some(r) = first_free(spots, size, &taken, &boxes, &others(holes.len() + li)) else { continue };
            taken.push(r);
            draw_label(r, g, Some(link_color));
        }

        let mut clicked: Option<i64> = None;
        let mut opened: Option<i64> = None;
        let mut pin: Option<String> = None;
        let mut drop: Option<(i64, egui::Pos2)> = None;
        let mut ids: Vec<i64> = pos.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let p = pos[&id];
            let (r, line1) = rects[&id].clone();
            if !rect.intersects(r) {
                continue;
            }
            let Some(info) = geo.info_of(id) else { continue };
            let c = whdata::class_of(id, info.security, &info.region);
            let resp = ui.interact(r, ui.id().with(("wh_node", id)), egui::Sense::click_and_drag());
            if resp.drag_started() {
                self.wh_graph.drag = Some((id, p));
            }
            if resp.dragged() {
                if let Some((_, at)) = &mut self.wh_graph.drag {
                    *at += resp.drag_delta() / zoom;
                }
            }
            if resp.drag_stopped() {
                if let Some((did, at)) = self.wh_graph.drag.take() {
                    drop = Some((did, snap(at)));
                }
            }
            if resp.clicked() {
                clicked = Some(id);
            }
            if resp.double_clicked() {
                focus_on = Some(Some(id));
            }
            let pinned = self.settings.wh_route_pins.iter().any(|n| n.eq_ignore_ascii_case(&info.name));
            resp.context_menu(|ui| {
                if ui.button("Focus on this system").clicked() {
                    focus_on = Some(Some(id));
                    ui.close();
                }
                if ui.button("Open system").clicked() {
                    opened = Some(id);
                    ui.close();
                }
                if ui.button(if pinned { "Remove from routes" } else { "Add to routes" }).clicked() {
                    pin = Some(info.name.clone());
                    ui.close();
                }
            });

            let jsys = whdata::jsystem(id);
            let effect = jsys.and_then(|j| j.effect.as_deref());
            let selected = self.wh_graph.selected == Some(id);
            let border = if selected {
                egui::Stroke::new(2.5, visuals.selection.stroke.color)
            } else if focus == Some(id) {
                egui::Stroke::new(2.5, visuals.hyperlink_color)
            } else {
                egui::Stroke::new(1.5, effect.map_or(visuals.widgets.noninteractive.bg_stroke.color, effect_color))
            };
            painter.rect(r, 5.0 * zoom, visuals.panel_fill, border, egui::StrokeKind::Inside);
            let Some(line1) = line1 else { continue };
            let line1_h = line1.size().y;
            if !detail {
                let clip = painter.with_clip_rect(r.shrink(2.0).intersect(rect));
                let at = egui::pos2(r.left() + 6.0, r.center().y - line1.size().y / 2.0);
                clip.galley(at, line1, visuals.text_color());
                continue;
            }
            let painter = painter.with_clip_rect(r.shrink(2.0).intersect(rect));
            painter.galley(r.min + egui::vec2(8.0, 5.0) * zoom, line1, visuals.text_color());
            let line2 = match jsys {
                Some(j) if !c.is_kspace() => {
                    let statics: Vec<String> = j
                        .statics
                        .iter()
                        .map(|s| match whdata::hole_type(s).map(|t| t.dest) {
                            Some(whdata::Dest::Class(d)) => match d {
                                Class::W(n) => format!("C{n}"),
                                Class::Hs => "HS".into(),
                                Class::Ls => "LS".into(),
                                Class::Ns => "NS".into(),
                                other => other.label(),
                            },
                            _ => s.clone(),
                        })
                        .collect();
                    let class = match c {
                        Class::W(n) => format!("C{n}"),
                        _ => c.label(),
                    };
                    if statics.is_empty() { class } else { format!("{class} \u{b7} {}", statics.join(" ")) }
                }
                _ => info.region.clone(),
            };
            let g2 = painter.layout(line2, font.clone(), visuals.weak_text_color(), (NODE.x - 16.0) * zoom);
            painter.galley(r.min + egui::vec2(8.0 * zoom, (27.0 * zoom).max(5.0 * zoom + line1_h + 1.0)), g2, visuals.weak_text_color());
            // Our characters here, and holes out of here that lead somewhere unknown.
            let mut badges: Vec<(String, egui::Color32)> = Vec::new();
            if let Some(who) = here.get(&id) {
                badges.push((format!("{} {}", icon::USER, who.len()), visuals.hyperlink_color));
            }
            if let Some(n) = open.get(&id) {
                badges.push((format!("{n} ?"), visuals.warn_fg_color));
            }
            let mut x = r.right() - 6.0 * zoom;
            for (text, color) in badges {
                let g = painter.layout_no_wrap(text, font.clone(), color);
                x -= g.size().x;
                painter.galley(egui::pos2(x, r.top() + 5.0 * zoom), g, color);
                x -= 8.0 * zoom;
            }
            if resp.hovered() && !resp.dragged() {
                let mut tip = format!("{} ({})", info.name, c.label());
                if let Some(e) = effect {
                    tip.push_str(&format!("\n{e}"));
                }
                if let Some(who) = here.get(&id) {
                    tip.push_str(&format!("\nHere: {}", who.join(", ")));
                }
                tip.push_str("\nClick to select, double-click to focus, drag to move");
                resp.on_hover_text(tip);
            }
        }
        if let Some(w) = hovered_edge {
            let name = |id: i64| geo.info_of(id).map_or(format!("#{id}"), |i| i.name.clone());
            let mut tip = format!(
                "{} {} \u{2192} {} {}",
                name(w.system_id),
                w.signature.as_deref().unwrap_or(""),
                w.dest_system_id.map(name).unwrap_or_default(),
                w.dest_signature.as_deref().unwrap_or("")
            );
            let types: Vec<&str> = [w.wh_type.as_deref(), w.dest_wh_type.as_deref()].into_iter().flatten().collect();
            if !types.is_empty() {
                tip.push_str(&format!("\nType: {}", types.join(" / ")));
            }
            if let Some(s) = w.effective_size() {
                tip.push_str(&format!("\nSize: {}", s.label()));
            }
            if let Some(m) = w.mass {
                tip.push_str(&format!("\nMass: {}", m.label()));
            }
            if let Some(l) = w.life {
                tip.push_str(&format!("\nLife: {}", l.label()));
            }
            tip.push_str(&format!("\nSource: {}", w.source.label()));
            if let Some(name) = self.wh_group_of.get(&w.uid).and_then(|g| self.share_group_name(g)) {
                tip.push_str(&format!("\nShared in {name}"));
            }
            bg.on_hover_text(tip);
        } else if let Some((exit, pin, n)) = hovered_link {
            let name = |id: i64| geo.info_of(id).map_or(format!("#{id}"), |i| i.name.clone());
            bg.on_hover_text(format!("{} to {}: {n} jumps by gate and bridge", name(exit), name(pin)));
        }

        if let Some((id, at)) = drop {
            if focus.is_some() {
                self.wh_graph.focus_dragged.insert(id, at);
            } else {
                if let Some(s) = self.store.as_ref() {
                    s.set_wh_layout(id, at.x, at.y);
                }
                self.wh_graph.dragged.get_or_insert_default().insert(id, at);
            }
        }
        if let Some(id) = clicked {
            self.wh_graph.selected = Some(id);
        }
        if let Some(f) = focus_on {
            self.wh_graph.set_focus(f);
            if f.is_some() {
                self.wh_graph.selected = f;
            }
        }
        if let Some(id) = opened {
            self.open_system(id);
        }
        if let Some(name) = pin {
            self.toggle_wh_pin(&name);
        }
    }

    /// Every J-space system with a known connection, to jump the map to it.
    fn wh_graph_list(&mut self, ui: &mut egui::Ui, geo: &crate::geo::Systems, holes: &[Wormhole]) {
        use crate::app::SteadySelect as _;
        let mut links: HashMap<i64, usize> = HashMap::new();
        for w in holes {
            let Some(b) = w.dest_system_id else { continue };
            for id in [w.system_id, b] {
                *links.entry(id).or_default() += 1;
            }
        }
        let mut rows: Vec<(Class, &crate::geo::SystemInfo, usize)> = links
            .iter()
            .filter_map(|(id, n)| {
                let i = geo.info_of(*id)?;
                let c = whdata::class_of(*id, i.security, &i.region);
                (!c.is_kspace()).then_some((c, i, *n))
            })
            .collect();
        let order = |c: Class| match c {
            Class::W(n) => n as i32,
            Class::Thera => 20,
            Class::Drifter(_) => 30,
            _ => 40,
        };
        rows.sort_by(|a, b| order(a.0).cmp(&order(b.0)).then(a.1.name.cmp(&b.1.name)));
        let mut focus: Option<i64> = None;
        egui::Panel::left("wh_graph_list").resizable(true).default_size(150.0).show_inside(ui, |ui| {
            ui.label(egui::RichText::new(format!("{} system{}", rows.len(), if rows.len() == 1 { "" } else { "s" })).weak());
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("wh_graph_list_grid").spacing([6.0, 2.0]).show(ui, |ui| {
                    for (c, info, n) in &rows {
                        let tag = match c {
                            Class::W(n) => format!("C{n}"),
                            Class::Drifter(_) => "Dr".into(),
                            _ => String::new(),
                        };
                        ui.label(egui::RichText::new(tag).color(class_color(*c, info.security)));
                        let on = self.wh_graph.focus == Some(info.id);
                        let effect = whdata::jsystem(info.id).and_then(|j| j.effect.clone());
                        let r = ui.menu_label(on, info.name.as_str());
                        let r = match effect {
                            Some(e) => r.on_hover_text(format!(
                                "{e}: {}\n{n} known connection{}",
                                whdata::effect_summary(&e),
                                if *n == 1 { "" } else { "s" }
                            )),
                            None => r.on_hover_text(format!("{n} known connection{}", if *n == 1 { "" } else { "s" })),
                        };
                        if r.clicked() {
                            focus = Some(info.id);
                        }
                        ui.label(egui::RichText::new(n.to_string()).weak());
                        ui.end_row();
                    }
                });
                if rows.is_empty() {
                    ui.label(egui::RichText::new("No J-space system with a known connection").weak());
                }
            });
        });
        if let Some(id) = focus {
            self.wh_graph.set_focus(Some(id));
            self.wh_graph.selected = Some(id);
        }
    }

    fn toggle_wh_pin(&mut self, name: &str) {
        let pins = &mut self.settings.wh_route_pins;
        if let Some(i) = pins.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            pins.remove(i);
        } else {
            pins.push(name.to_owned());
        }
        self.needs_save = true;
    }

    fn wh_graph_side(
        &mut self,
        ui: &mut egui::Ui,
        geo: &std::sync::Arc<crate::geo::Systems>,
        holes: &[Wormhole],
        chars: &HashMap<String, (i64, bool)>,
        now: i64,
    ) {
        let Some(sel) = self.wh_graph.selected else { return };
        let Some(info) = geo.info_of(sel).cloned() else { return };
        let mut edit: Option<i64> = None;
        let mut kill: Option<i64> = None;
        let mut select: Option<i64> = None;
        let mut facts = false;
        let mut focus = false;
        let mut unpin: Option<String> = None;
        let mut paste: Option<Option<String>> = None;
        let mut drop_sig: Option<String> = None;
        let mut edit_sig: Option<String> = None;
        let mut quick: Option<(Wormhole, Wormhole)> = None;
        let mut new_hole: Option<String> = None;
        egui::Panel::right("wh_graph_side").resizable(true).default_size(260.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(&info.name);
                    if self.wh_graph.focus != Some(sel)
                        && ui.button(icon::CROSSHAIR).on_hover_text("Show only this system and those near it").clicked()
                    {
                        focus = true;
                    }
                    if ui.button(icon::INFO).on_hover_text("Wormhole facts about this system").clicked() {
                        facts = true;
                    }
                    if ui.button(icon::X).on_hover_text("Deselect").clicked() {
                        select = Some(0);
                    }
                });
                let c = whdata::class_of(sel, info.security, &info.region);
                ui.label(format!("{} \u{b7} {}", c.label(), info.region));
                let name = |id: i64| geo.info_of(id).map_or(format!("#{id}"), |i| i.name.clone());
                ui.add_space(4.0);
                let n_sigs = self.wh_graph_sigs(sel).len();
                let tabs = [
                    (SideTab::Info, "Info".to_owned()),
                    (SideTab::Routes, "Routes".to_owned()),
                    (SideTab::Sigs, if n_sigs == 0 { "Signatures".to_owned() } else { format!("Signatures ({n_sigs})") }),
                ];
                let w = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
                ui.horizontal(|ui| {
                    use crate::app::SteadySelect as _;
                    for (t, label) in tabs {
                        if ui.menu_label_sized([w, 24.0], self.wh_graph.side_tab == t, label).clicked() {
                            self.wh_graph.side_tab = t;
                        }
                    }
                });
                ui.separator();
                match self.wh_graph.side_tab {
                    SideTab::Info => {
                ui.label(egui::RichText::new("Connections").strong());
                let mut any = false;
                egui::Grid::new("wh_graph_sigs").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for w in holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)) {
                        any = true;
                        // Seen from the selected side.
                        let (sig, ty, far, far_sig) = if w.system_id == sel {
                            (&w.signature, &w.wh_type, w.dest_system_id, &w.dest_signature)
                        } else {
                            (&w.dest_signature, &w.dest_wh_type, Some(w.system_id), &w.signature)
                        };
                        ui.label(sig.as_deref().unwrap_or("—"));
                        ui.label(ty.as_deref().unwrap_or("—"));
                        match far {
                            Some(f) => {
                                let text = match far_sig {
                                    Some(s) => format!("{} {}", name(f), s),
                                    None => name(f),
                                };
                                if ui.link(format!("{} {text}", icon::ARROW_RIGHT)).clicked() {
                                    select = Some(f);
                                }
                            }
                            None => {
                                ui.label(format!("{} {}", icon::ARROW_RIGHT, w.dest.label()));
                            }
                        }
                        let state = [w.life.map(|l| l.label()), w.mass.map(|m| m.label())]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(", ");
                        ui.label(if state.is_empty() {
                            format!("{} ago", super::human_ago(now - w.reported_at))
                        } else {
                            state
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Edit this hole").clicked() {
                                edit = Some(w.id);
                            }
                            if ui.small_button(icon::X).on_hover_text("Mark this hole dead").clicked() {
                                kill = Some(w.id);
                            }
                        });
                        ui.end_row();
                    }
                });
                if !any {
                    ui.label(egui::RichText::new("None known").weak());
                }
                if !c.is_kspace() {
                    ui.add_space(10.0);
                    crate::app::wormholes_ui::wh_system_facts(ui, sel, &info, false);
                }

                    }
                    SideTab::Routes => {
                ui.label(egui::RichText::new("Jumps from here, through the holes allowed below").weak());
                let adj = self.wh_adjacency();
                let mut targets: Vec<(String, i64, bool)> = Vec::new();
                for p in &self.settings.wh_route_pins {
                    if let Some(i) = geo.lookup(p) {
                        targets.push((i.name.clone(), i.id, true));
                    }
                }
                let mut names: Vec<&String> = chars.keys().collect();
                names.sort();
                for n in names {
                    targets.push((n.clone(), chars[n].0, false));
                }
                if targets.is_empty() {
                    ui.label(egui::RichText::new("Pin a system to see how far it is.").weak());
                }
                for (label, dest, is_pin) in &targets {
                    let route = geo.route_with(sel, *dest, true, true, &adj, |_| true);
                    ui.horizontal(|ui| {
                        if *is_pin && ui.small_button(icon::X).on_hover_text("Remove").clicked() {
                            unpin = Some(label.clone());
                        }
                        if !*is_pin {
                            ui.label(egui::RichText::new(icon::USER).weak());
                        }
                        let dest_name = name(*dest);
                        let text = if *is_pin || dest_name == *label { label.clone() } else { format!("{label} ({dest_name})") };
                        if ui.link(text).clicked() {
                            select = Some(*dest);
                        }
                        match &route {
                            Some(r) => ui.label(format!("{}j", r.len() - 1)),
                            None => ui.label(egui::RichText::new("no route").weak()),
                        };
                    });
                    if let Some(r) = &route {
                        // One square per jump, wrapped to the panel's width, at least ten a row.
                        const STEP: f32 = 10.0;
                        const ROW: f32 = 13.0;
                        let hops = r.len().saturating_sub(1);
                        // The visible width: wider content above can stretch the layout past it.
                        let visible = ui.clip_rect().right().min(ui.max_rect().right()) - ui.cursor().left();
                        let per_row = (((visible + 2.0) / STEP) as usize).max(10);
                        let rows = hops.div_ceil(per_row).max(1);
                        let (resp, painter) =
                            ui.allocate_painter(egui::vec2(hops.min(per_row) as f32 * STEP, rows as f32 * ROW), egui::Sense::hover());
                        for (i, s) in r.iter().skip(1).enumerate() {
                            let color = geo
                                .info_of(*s)
                                .map(|i| class_color(whdata::class_of(*s, i.security, &i.region), i.security))
                                .unwrap_or(egui::Color32::GRAY);
                            let at = resp.rect.min + egui::vec2((i % per_row) as f32 * STEP, (i / per_row) as f32 * ROW + 1.0);
                            painter.rect_filled(egui::Rect::from_min_size(at, egui::vec2(8.0, 10.0)), 1.0, color);
                        }
                        resp.on_hover_text(r.iter().skip(1).map(|s| name(*s)).collect::<Vec<_>>().join(" \u{2192} "));
                    }
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.wh_graph.pin_query).hint_text("Add a system").desired_width(160.0),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let add = enter || ui.button(icon::PLUS).on_hover_text("Measure routes to this system").clicked();
                    if let Some(i) = geo.lookup(self.wh_graph.pin_query.trim()).filter(|_| add) {
                        if !self.settings.wh_route_pins.iter().any(|n| n.eq_ignore_ascii_case(&i.name)) {
                            self.settings.wh_route_pins.push(i.name.clone());
                            self.needs_save = true;
                        }
                        self.wh_graph.pin_query.clear();
                    }
                });
                if self.wh_route_kinds_ui(ui) {
                    self.needs_save = true;
                    self.replan_routes();
                }
                    }
                    SideTab::Sigs => {
                let sigs = self.wh_graph_sigs(sel);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(format!("{}  Paste probe scan", icon::CLIPBOARD_TEXT))
                        .on_hover_text("In EVE, select everything in the probe scanner and copy it, then click here or press Ctrl+V over this panel")
                        .clicked()
                    {
                        paste = Some(None);
                    }
                    ui.checkbox(&mut self.wh_graph.keep_missing, "Keep missing")
                        .on_hover_text("Keep signatures the paste does not list. Off, a full paste replaces the list: what is missing is gone from space.");
                });
                if let Some(t) = ui.input(|i| {
                    i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None })
                }) {
                    if ui.ui_contains_pointer() && ui.memory(|m| m.focused().is_none()) {
                        paste = Some(Some(t));
                    }
                }
                if let Some(note) = &self.wh_graph.sig_note {
                    ui.label(egui::RichText::new(note).weak());
                }
                if sigs.is_empty() {
                    ui.label(egui::RichText::new("No signatures pasted for this system").weak());
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                egui::Grid::new("wh_graph_scan").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for h in ["Id", "Group", "Info", "Added"] {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for sg in &sigs {
                        let anomaly = sg.kind.to_lowercase().contains("anomal");
                        let id = ui.label(if anomaly { egui::RichText::new(&sg.sig).weak() } else { egui::RichText::new(&sg.sig) });
                        id.on_hover_text(&sg.kind);
                        ui.label(short_group(&sg.group));
                        // A wormhole signature we know the far side of says where it goes.
                        let hole = sig_hole(holes, sel, &sg.sig);
                        match hole.map(|w| if w.system_id == sel { w.dest_system_id } else { Some(w.system_id) }) {
                            Some(Some(f)) => {
                                if ui.link(format!("{} {}", icon::ARROW_RIGHT, name(f))).clicked() {
                                    select = Some(f);
                                }
                            }
                            _ => {
                                ui.label(if sg.name.is_empty() { "—" } else { sg.name.as_str() });
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.label(super::human_ago(now - sg.added_at)).on_hover_text(format!(
                                "Added by {}, last seen in a paste {} ago",
                                sg.who,
                                super::human_ago(now - sg.updated_at)
                            ));
                            let is_hole = hole.is_some() || sg.group == "Wormhole";
                            if is_hole && ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Update this wormhole").clicked() {
                                edit_sig = Some(sg.sig.clone());
                            }
                            if ui.small_button(icon::X).on_hover_text("Remove").clicked() {
                                drop_sig = Some(sg.sig.clone());
                            }
                        });
                        ui.end_row();
                    }
                });
                // The chosen hole's state, changed in place as it degrades.
                let open_sig = self.wh_graph.sig_edit.clone();
                if let Some(w) = open_sig.as_deref().and_then(|sig| sig_hole(holes, sel, sig)) {
                    let mut w = w.clone();
                    let was = w.clone();
                    let here = w.system_id == sel;
                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        let far = if here { w.dest_system_id } else { Some(w.system_id) };
                        ui.label(
                            egui::RichText::new(format!(
                                "{} {} {}",
                                open_sig.as_deref().unwrap_or_default(),
                                icon::ARROW_RIGHT,
                                far.map(name).unwrap_or_else(|| w.dest.label().to_owned())
                            ))
                            .strong(),
                        );
                        if ui.small_button(icon::X).on_hover_text("Close").clicked() {
                            self.wh_graph.sig_edit = None;
                        }
                    });
                    egui::Grid::new("wh_graph_sig_edit").spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("Type");
                        let codes: Vec<&str> = whdata::types().iter().map(|t| t.code.as_str()).collect();
                        let ty = if here { &mut w.wh_type } else { &mut w.dest_wh_type };
                        let mut code = ty.clone().unwrap_or_default();
                        if crate::app::wormholes_ui::wh_type_picker(ui, "wh_sig_type", 150.0, &mut code, &codes) {
                            *ty = (!code.is_empty()).then_some(code.clone());
                            if let Some(s) = crate::wormholes::sizes_for(&[code.as_str()]).first().filter(|_| !code.is_empty() && code != "K162") {
                                w.size = Some(*s);
                            }
                        }
                        ui.end_row();
                        ui.label("Size");
                        let sizes: Vec<_> = [ShipSize::Frigate, ShipSize::Medium, ShipSize::Large, ShipSize::XLarge]
                            .into_iter()
                            .map(|s| (s, s.short(), s.label()))
                            .collect();
                        crate::app::wormholes_ui::choice_row(ui, &mut w.size, &sizes);
                        ui.end_row();
                        ui.label("Time left");
                        let lives: Vec<_> = Life::ALL.into_iter().map(|l| (l, l.short(), l.label())).collect();
                        crate::app::wormholes_ui::choice_row(ui, &mut w.life, &lives);
                        ui.end_row();
                        ui.label("Mass left");
                        let masses: Vec<_> = Mass::ALL.into_iter().map(|m| (m, m.short(), m.label())).collect();
                        crate::app::wormholes_ui::choice_row(ui, &mut w.mass, &masses);
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if ui.button(format!("{}  All fields", icon::PENCIL_SIMPLE)).clicked() {
                            edit = Some(w.id);
                        }
                        if ui.button(format!("{}  Collapsed", icon::X)).on_hover_text("Mark this hole dead").clicked() {
                            kill = Some(w.id);
                        }
                    });
                    if w != was {
                        quick = Some((was, w));
                    }
                } else if let Some(sig) = open_sig {
                    // A wormhole signature not tied to a known hole yet: enter it.
                    self.wh_graph.sig_edit = None;
                    new_hole = Some(sig);
                }
                    }
                }
            });
        });
        if let Some(text) = paste {
            self.wh_graph_paste(sel, text, now);
        }
        if let Some(sig) = edit_sig {
            self.wh_graph.sig_edit = (self.wh_graph.sig_edit.as_deref() != Some(sig.as_str())).then_some(sig);
        }
        if let Some((was, w)) = quick {
            self.wh_quick_save(&was, w, now);
        }
        if let Some(sig) = new_hole {
            self.wh_form = Some(crate::app::wormholes_ui::WhForm::at(info.name.clone(), sig));
        }
        if let Some(sig) = drop_sig {
            if let Some(s) = self.store.as_ref() {
                s.delete_system_sig(sel, &sig);
            }
            self.wh_graph.sigs = None;
        }
        if let Some(id) = select {
            self.wh_graph.selected = (id != 0).then_some(id);
        }
        if facts {
            self.wh_info = Some(sel);
        }
        if focus {
            self.wh_graph.set_focus(Some(sel));
        }
        if let Some(n) = unpin {
            self.toggle_wh_pin(&n);
        }
        if let Some(id) = kill {
            self.kill_wormhole(id);
        }
        if let Some(id) = edit {
            self.wh_edit(id);
        }
    }

    /// Writes a change made in the signatures tab, with who made it.
    fn wh_quick_save(&mut self, was: &Wormhole, mut w: Wormhole, now: i64) {
        let who = if self.settings.active_character.is_empty() { "me".to_owned() } else { self.settings.active_character.clone() };
        let mut changes: Vec<(&str, String)> = Vec::new();
        if w.wh_type != was.wh_type || w.dest_wh_type != was.dest_wh_type {
            changes.push(("type", w.wh_type.clone().or(w.dest_wh_type.clone()).unwrap_or_default()));
        }
        if w.size != was.size {
            changes.push(("size", w.size.map_or("unknown", |s| s.label()).to_owned()));
        }
        if w.life != was.life {
            changes.push(("time left", w.life.map_or("unknown", |l| l.label()).to_owned()));
            w.explicit_expiry = w.life.and_then(|l| l.closes_by(now)).or(was.explicit_expiry);
        }
        if w.mass != was.mass {
            changes.push(("mass left", w.mass.map_or("unknown", |m| m.label()).to_owned()));
        }
        if w.life != was.life || w.mass != was.mass {
            w.observed_at = Some(now);
        }
        w.updated_at = now;
        w.seen_by |= crate::wormholes::Source::Manual.bit();
        if let Some(store) = &self.store {
            store.write_wormhole(&w);
            store.audit_wormhole(&w.uid, &who, crate::wormholes::Source::Manual, &changes);
        }
        if let Some(c) = self.wh_cache.iter_mut().find(|c| c.id == w.id) {
            *c = w;
        }
        self.wh_reloaded = None;
    }

    /// The selected system's pasted signatures, read once per system.
    fn wh_graph_sigs(&mut self, system: i64) -> Vec<crate::store::SystemSig> {
        if self.wh_graph.sigs.as_ref().is_none_or(|(s, _)| *s != system) {
            let list = self
                .store
                .as_ref()
                .map(|s| {
                    if !std::mem::replace(&mut self.wh_graph.sigs_pruned, true) {
                        s.prune_system_sigs(chrono::Utc::now().timestamp() - 3 * 86_400);
                    }
                    s.system_sigs(system)
                })
                .unwrap_or_default();
            self.wh_graph.sigs = Some((system, list));
            self.wh_graph.sig_note = None;
        }
        self.wh_graph.sigs.as_ref().map(|(_, l)| l.clone()).unwrap_or_default()
    }

    /// Folds a probe scanner copy into `system`'s list: `text` when given, else the clipboard.
    fn wh_graph_paste(&mut self, system: i64, text: Option<String>, now: i64) {
        let text = text.or_else(|| {
            if self.dscan_clip.is_none() {
                self.dscan_clip = arboard::Clipboard::new().ok();
            }
            self.dscan_clip.as_mut().and_then(|c| c.get_text().ok())
        });
        let scan = crate::wormholes::probe_scan(text.as_deref().unwrap_or(""));
        if scan.is_empty() {
            self.wh_graph.sig_note = Some("The clipboard holds no probe scanner rows.".into());
            return;
        }
        let who = {
            let p = self.player.lock().unwrap();
            if p.active_name.is_empty() { "me".to_owned() } else { p.active_name.clone() }
        };
        let Some(store) = self.store.as_ref() else { return };
        let (added, updated, removed) = store.merge_system_sigs(system, &scan, &who, now, !self.wh_graph.keep_missing);
        self.wh_graph.sigs = None;
        self.wh_graph_sigs(system);
        self.wh_graph.sig_note = Some(format!("{added} new, {updated} updated, {removed} removed"));
    }

    pub(crate) fn wh_graph_reset_layout(&mut self) {
        if let Some(s) = self.store.as_ref() {
            s.clear_wh_layout();
        }
        self.wh_graph.dragged = Some(HashMap::new());
        self.wh_graph.pan = egui::Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_grows_rightwards_from_the_highest_score() {
        // 1 (our side) - 2 - 3, and 2 - 4.
        let auto = auto_layout(&[(1, 2), (2, 3), (2, 4)], |n| if n == 1 { 100 } else { 0 });
        let at: HashMap<i64, egui::Pos2> = auto.iter().map(|(n, _, p)| (*n, *p)).collect();
        assert_eq!(auto[0].0, 1);
        assert_eq!(at[&1].x, 0.0);
        assert_eq!(at[&2].x, COL);
        assert_eq!(at[&3].x, 2.0 * COL);
        assert_eq!(at[&4].x, 2.0 * COL);
        assert_ne!(at[&3].y, at[&4].y);
        assert_eq!(at[&2].y, (at[&3].y + at[&4].y) / 2.0);
    }

    #[test]
    fn separate_chains_do_not_overlap() {
        let auto = auto_layout(&[(1, 2), (3, 4), (3, 5)], |_| 0);
        let at: HashMap<i64, egui::Pos2> = auto.iter().map(|(n, _, p)| (*n, *p)).collect();
        let rows = |ns: &[i64]| ns.iter().map(|n| at[n].y).fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
        let (a, b) = (rows(&[1, 2]), rows(&[3, 4, 5]));
        assert!(a.1 + NODE.y < b.0 || b.1 + NODE.y < a.0, "{a:?} {b:?}");
    }

    #[test]
    fn a_dragged_system_takes_its_subtree_along() {
        let auto = auto_layout(&[(1, 2), (2, 3)], |n| if n == 1 { 100 } else { 0 });
        let dragged = HashMap::from([(2, egui::pos2(500.0, 400.0))]);
        let at = place(&auto, &dragged);
        assert_eq!(at[&2], egui::pos2(500.0, 400.0));
        assert_eq!(at[&3], egui::pos2(500.0 + COL, 400.0));
        assert_eq!(at[&1], egui::pos2(0.0, 0.0));
    }

    #[test]
    fn a_new_system_makes_room_for_itself() {
        let auto = auto_layout(&[(1, 2), (1, 3)], |n| if n == 1 { 100 } else { 0 });
        let first = place(&auto, &HashMap::new());
        // Drag 3 onto where 2 would go: 2 must move off it.
        let dragged = HashMap::from([(3, first[&2])]);
        let at = place(&auto, &dragged);
        assert_ne!(at[&2], at[&3]);
    }

    #[test]
    fn routes_run_at_right_angles_and_fan_out_on_one_trunk() {
        let parent = egui::Rect::from_min_size(egui::pos2(0.0, 100.0), NODE);
        let up = egui::Rect::from_min_size(egui::pos2(COL, 0.0), NODE);
        let down = egui::Rect::from_min_size(egui::pos2(COL, 200.0), NODE);
        let boxes = HashMap::from([(1, parent), (2, up), (3, down)]);
        let r = route_all(&boxes, &[(1, 2, true), (1, 3, true)], &HashMap::new());
        for p in r.iter().flatten() {
            assert!(p.windows(2).all(|s| (s[0].x - s[1].x).abs() < 0.5 || (s[0].y - s[1].y).abs() < 0.5), "{p:?}");
            assert_eq!(p[0], egui::pos2(NODE.x, 125.0), "out of the parent's side");
        }
    }

    #[test]
    fn a_route_never_crosses_another_box() {
        // Three in one column: the top to the bottom must go round the middle one.
        let at = |y: f32| egui::Rect::from_min_size(egui::pos2(0.0, y), NODE);
        let boxes = HashMap::from([(1, at(0.0)), (2, at(100.0)), (3, at(200.0))]);
        let p = route_all(&boxes, &[(1, 3, true)], &HashMap::new())[0].clone().unwrap();
        for s in p.windows(2) {
            assert!(!boxes[&2].shrink(1.0).intersects(egui::Rect::from_two_pos(s[0], s[1])), "{p:?}");
        }
    }

    #[test]
    fn two_links_into_one_box_do_not_share_a_line() {
        let at = |x: f32, y: f32| egui::Rect::from_min_size(egui::pos2(x, y), NODE);
        let boxes = HashMap::from([(1, at(0.0, 0.0)), (2, at(0.0, 100.0)), (9, at(COL, 0.0))]);
        let r = route_all(&boxes, &[(1, 9, false), (2, 9, false)], &HashMap::new());
        let (a, b) = (r[0].clone().unwrap(), r[1].clone().unwrap());
        let shared: f32 = a.windows(2).flat_map(|s| b.windows(2).map(move |t| overlap(s[0], s[1], t[0], t[1]))).sum();
        assert!(shared < 1.0, "{a:?} {b:?}");
    }

    #[test]
    fn focus_keeps_systems_within_its_depth() {
        let hole = |a, b| Wormhole { system_id: a, dest_system_id: Some(b), ..Default::default() };
        let holes = [hole(1, 2), hole(2, 3), hole(3, 4), hole(9, 8)];
        assert_eq!(within(&holes, 2, 1), HashSet::from([1, 2, 3]));
        assert_eq!(within(&holes, 1, 2), HashSet::from([1, 2, 3]));
    }

    #[test]
    fn labels_never_land_on_each_other() {
        // Two routes ending on the same leg into one box, as in a staging system two chains reach.
        let size = egui::vec2(30.0, 16.0);
        let target = egui::Rect::from_min_size(egui::pos2(300.0, 0.0), egui::vec2(100.0, 40.0));
        let leg = |from: egui::Pos2| {
            let mid = from.lerp(egui::pos2(300.0, 20.0), 0.5);
            let mut spots = along(mid, egui::pos2(300.0, 20.0), size);
            spots.extend(along(mid, from, size));
            spots
        };
        let mut taken = Vec::new();
        for from in [egui::pos2(100.0, 20.0), egui::pos2(160.0, 20.0), egui::pos2(220.0, 20.0)] {
            if let Some(r) = first_free(leg(from), size, &taken, &[target], &[]) {
                assert!(!r.intersects(target));
                taken.push(r);
            }
        }
        assert!(taken.len() >= 2);
        for (i, a) in taken.iter().enumerate() {
            for b in &taken[i + 1..] {
                assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn a_label_keeps_off_a_line_it_shares() {
        // Ours comes up and joins theirs; ours must be labelled on its own vertical stretch.
        let size = egui::vec2(30.0, 16.0);
        let theirs = [egui::pos2(0.0, 0.0), egui::pos2(300.0, 0.0)];
        let ours = [egui::pos2(150.0, 200.0), egui::pos2(150.0, 0.0), egui::pos2(300.0, 0.0)];
        let mut spots = Vec::new();
        for seg in ours.windows(2).rev() {
            let mid = seg[0].lerp(seg[1], 0.5);
            spots.extend(along(mid, seg[1], size));
            spots.extend(along(mid, seg[0], size));
        }
        let r = first_free(spots, size, &[], &[], &[&theirs]).unwrap();
        assert!((r.center().x - 150.0).abs() < 1.0 && r.center().y > 10.0, "{r:?}");
    }

    #[test]
    fn a_fan_of_three_keeps_the_level_one_in_the_middle_and_never_overlaps() {
        let at = |x: f32, y: f32| egui::Rect::from_min_size(egui::pos2(x, y), NODE);
        let boxes = HashMap::from([(1, at(0.0, 100.0)), (2, at(COL, 0.0)), (3, at(COL, 100.0)), (4, at(COL, 200.0))]);
        let r = route_all(&boxes, &[(1, 2, false), (1, 3, false), (1, 4, false)], &HashMap::new());
        let paths: Vec<Vec<egui::Pos2>> = r.into_iter().map(Option::unwrap).collect();
        assert_eq!(paths[1].len(), 2, "the level one runs straight: {:?}", paths[1]);
        assert_eq!(paths[1][0].y, boxes[&1].center().y, "out of the middle");
        for i in 0..3 {
            for j in i + 1..3 {
                let shared: f32 = paths[i].windows(2).flat_map(|s| paths[j].windows(2).map(move |t| overlap(s[0], s[1], t[0], t[1]))).sum();
                let crossing = paths[i].windows(2).any(|s| paths[j].windows(2).any(|t| crosses(s[0], s[1], t[0], t[1])));
                assert!(shared < 1.0 && !crossing, "{:?} and {:?}", paths[i], paths[j]);
            }
        }
    }

    #[test]
    fn chains_are_found_apart() {
        let mut c = components(&[(1, 2), (2, 3), (7, 8)]);
        for x in &mut c {
            x.sort_unstable();
        }
        c.sort();
        assert_eq!(c, vec![vec![1, 2, 3], vec![7, 8]]);
    }

    #[test]
    fn drops_snap_to_the_grid() {
        assert_eq!(snap(egui::pos2(14.0, 26.0)), egui::pos2(10.0, 30.0));
    }
}
