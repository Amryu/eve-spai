//! The wormhole map: known holes as a graph of J-space and the k-space systems they open into.
//! Laid out as a tree per chain, from the side our characters are on; a system the user dragged
//! keeps its place, and whatever grows off it later is laid out relative to it.

use std::collections::{HashMap, HashSet, VecDeque};

use egui_phosphor::regular as icon;

use super::SpaiApp;
use crate::whdata::{self, Class};
use crate::wormholes::{time_left, Mass, TimeLeft, Wormhole};

/// Wide enough that at [`MIN_ZOOM`] a name still fits its scaled box, so no box grows past its
/// place when zoomed out.
pub(crate) const NODE: egui::Vec2 = egui::vec2(290.0, 56.0);
/// Drifter systems: their name, J-code and badge need more room.
/// A pinned system's copy beside a cluster.
const PILL: egui::Vec2 = egui::vec2(230.0, 40.0);

// Where each hole's line was drawn in the last frame, by hole id, for tests that hover one.
#[cfg(test)]
thread_local! {
    pub(crate) static EDGE_PROBE: std::cell::RefCell<Vec<(i64, Vec<egui::Pos2>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A line's details beside the pointer. Drawn here rather than as the map's own tooltip: that
/// belongs to the whole map, and egui places and gates it for the map, not for the line.
fn line_tip(ui: &egui::Ui, pointer: Option<egui::Pos2>, text: String) {
    let Some(at) = pointer else { return };
    // Laid out at full width first: wrapping to whatever room is left beside the pointer is what
    // squeezed it near the right edge.
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_size = ui.painter().layout_no_wrap(text.clone(), font, ui.visuals().text_color()).size();
    let margin = egui::Frame::popup(ui.style()).total_margin().sum();
    let size = text_size + margin + egui::vec2(2.0, 2.0);
    let pos = tip_pos(at, size, ui.ctx().content_rect());
    egui::Area::new(ui.id().with("wh_line_tip"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.add(egui::Label::new(text).extend());
            });
        });
}

/// Below and right of the pointer, flipped to the other side on whichever axis would run off
/// `screen`, and kept on it as a last resort.
fn tip_pos(pointer: egui::Pos2, size: egui::Vec2, screen: egui::Rect) -> egui::Pos2 {
    const GAP: f32 = 14.0;
    let mut p = pointer + egui::vec2(GAP, GAP);
    if p.x + size.x > screen.right() {
        p.x = pointer.x - GAP - size.x;
    }
    if p.y + size.y > screen.bottom() {
        p.y = pointer.y - GAP - size.y;
    }
    egui::pos2(p.x.clamp(screen.left(), (screen.right() - size.x).max(screen.left())), p.y.clamp(screen.top(), (screen.bottom() - size.y).max(screen.top())))
}

/// An "Unidentified Wormhole" in a drifter system can only be that system's own hole type.
fn unidentified_type(system: i64, name: &str) -> Option<&'static str> {
    name.to_lowercase().contains("unidentified").then(|| whdata::drifter_code(system)).flatten()
}

pub(crate) fn node_size(id: i64) -> egui::Vec2 {
    if id < 0 {
        return PILL;
    }
    NODE
}

/// A system's name as people say it: the drifter systems by their own names, not J-codes.
fn display_name(id: i64, name: &str) -> String {
    match whdata::DRIFTERS.iter().find(|d| d.2 == id) {
        Some(d) => {
            let mut c = d.1.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
        None => name.to_owned(),
    }
}
use super::wh_layout::COL;
const GRID: f32 = 10.0;
/// How far out of a box an edge runs before it turns.
pub(crate) const STUB: f32 = 20.0;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SideTab {
    #[default]
    Info,
    Routes,
    Sigs,
}

/// A hole's type, whichever side it was read on; between a drifter system and k-space, that
/// drifter's own type when none was entered.
fn hole_code(w: &Wormhole) -> Option<String> {
    let wspace = |id: i64| (31_000_000..32_000_000).contains(&id);
    let drifter = || match (crate::whdata::drifter_code(w.system_id), w.dest_system_id) {
        (Some(c), Some(d)) if !wspace(d) => Some(c.to_owned()),
        (None, Some(d)) if !wspace(w.system_id) => crate::whdata::drifter_code(d).map(str::to_owned),
        _ => None,
    };
    w.wh_type.clone().or_else(|| w.dest_wh_type.clone()).or_else(drifter)
}

fn letters(sig: &str) -> String {
    sig.trim().chars().take(3).collect::<String>().to_uppercase()
}

/// What a probe scan of `system` says about the saved holes. First, those whose signature there it
/// no longer lists: only from a paste that replaces the list and has signatures in it. Second, a
/// signature for the one hole there without one, when exactly one scanned wormhole is nobody's:
/// the hole's id, the signature, and whether it goes on the hole's own side.
pub(crate) fn probe_effects(holes: &[Wormhole], system: i64, scan: &[crate::wormholes::ScanSig], full: bool) -> (Vec<i64>, Option<(i64, String, bool)>) {
    let side = |w: &Wormhole| -> Option<(bool, Option<String>)> {
        if w.system_id == system {
            Some((true, w.signature.clone()))
        } else if w.dest_system_id == Some(system) {
            Some((false, w.dest_signature.clone()))
        } else {
            None
        }
    };
    let here: Vec<(&Wormhole, bool, Option<String>)> = holes.iter().filter_map(|w| side(w).map(|(near, sig)| (w, near, sig.map(|s| letters(&s)).filter(|s| s.len() == 3)))).collect();
    let scanned: HashSet<String> = scan.iter().map(|s| letters(&s.id)).collect();
    let lists_sigs = scan.iter().any(|s| s.kind.to_lowercase().contains("signature"));
    let gone: Vec<i64> = if full && lists_sigs {
        here.iter().filter(|(_, _, sig)| sig.as_ref().is_some_and(|s| !scanned.contains(s))).map(|(w, _, _)| w.id).collect()
    } else {
        Vec::new()
    };
    let taken: HashSet<&String> = here.iter().filter_map(|(_, _, sig)| sig.as_ref()).collect();
    let free: Vec<&crate::wormholes::ScanSig> = scan
        .iter()
        .filter(|s| s.group.to_lowercase().contains("wormhole") && !taken.contains(&letters(&s.id)))
        .collect();
    let bare: Vec<&(&Wormhole, bool, Option<String>)> = here.iter().filter(|(_, _, sig)| sig.is_none()).collect();
    let fill = match (bare.as_slice(), free.as_slice()) {
        ([(w, near, _)], [sig]) => Some((w.id, sig.id.clone(), *near)),
        _ => None,
    };
    (gone, fill)
}

const SHATTERED_COLOR: egui::Color32 = egui::Color32::from_rgb(0x9A, 0xD8, 0xF0);

/// Drops from `holes` those whose k-space end is not in `keep`, unless both ends are k-space (so
/// k-space to Pochven, which is k-space too). Returns how many were dropped at each end kept.
fn overview(holes: &mut Vec<Wormhole>, kspace: impl Fn(i64) -> bool, keep: &HashSet<i64>) -> HashMap<i64, usize> {
    // A k-space system on the map anyway, as one end of a k-space to k-space hole, keeps its
    // other holes too.
    let mut keep = keep.clone();
    for w in holes.iter() {
        if let Some(b) = w.dest_system_id.filter(|b| kspace(w.system_id) && kspace(*b)) {
            keep.extend([w.system_id, b]);
        }
    }
    let mut hidden: HashMap<i64, usize> = HashMap::new();
    holes.retain(|w| {
        let Some(b) = w.dest_system_id else { return true };
        let a = w.system_id;
        let (ka, kb) = (kspace(a), kspace(b));
        if ka && kb {
            return true;
        }
        let (gone_a, gone_b) = (ka && !keep.contains(&a), kb && !keep.contains(&b));
        if !gone_a && !gone_b {
            return true;
        }
        for (end, gone) in [(a, gone_a), (b, gone_b)] {
            if !gone {
                *hidden.entry(end).or_default() += 1;
            }
        }
        false
    });
    hidden
}

/// The map id of a pinned system's copy beside a cluster's exit: negative, so no system has it,
/// and the same for as long as the exit is.
fn pill_id(pin: i64, exit: i64) -> i64 {
    -(pin * 100_000 + exit.rem_euclid(100_000))
}

/// Gate jumps under which a pinned system counts as near a cluster and joins it. The colours
/// ([`close_color`]) still only mark the nearest, under 10.
const NEAR_JUMPS: u32 = 30;

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

/// The lines last routed and what for. `partial` when some were kept from before during a drag.
struct RouteCache {
    key: u64,
    boxes: HashMap<i64, egui::Rect>,
    links: Vec<(i64, i64, bool)>,
    paths: Vec<Option<Vec<egui::Pos2>>>,
    partial: bool,
}

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
    keep_missing: bool,
    /// The last routes worked out, and what they were worked out for.
    route_cache: Option<RouteCache>,
    /// Gate jumps from each pinned system, for joining it to the focused chain.
    gate_dist: HashMap<i64, HashMap<i64, u32>>,
    /// The last auto layout and what it was worked out from: the layered one is too slow to
    /// redo every frame.
    layout_cache: Option<(u64, Vec<(i64, Option<i64>, egui::Pos2)>)>,
    /// The canvas as last drawn, whose shape the chains are packed to.
    canvas: Option<egui::Rect>,
    /// Holes whose signature a probe scan no longer lists, waiting on the user: the system, and
    /// each hole with whether it is ticked to go.
    pub(crate) gone: Option<(i64, Vec<(i64, bool)>)>,
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
        let Some(bounds) = pos.iter().fold(None::<egui::Rect>, |b, (id, p)| {
            let r = egui::Rect::from_min_size(*p, node_size(*id));
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
    }

    #[cfg(test)]
    pub(crate) fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    /// The top left at `zoom`, whatever a fit asked for.
    #[cfg(test)]
    pub(crate) fn hold_view(&mut self, zoom: f32) {
        self.zoom = zoom;
        self.pan = egui::vec2(24.0, 24.0);
        self.fit_pending = false;
    }

    #[cfg(test)]
    pub(crate) fn pan_to(&mut self, pan: egui::Vec2) {
        self.pan = pan;
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
        let dragging = self.drag.is_some();
        if let Some(c) = &self.route_cache {
            if c.key == key && (dragging || !c.partial) {
                return c.paths.clone();
            }
        }
        // While a box is dragged only its own lines, and those it now lies across, are routed
        // again; the rest stay as they were. Letting go routes everything once more.
        let (paths, partial) = match &self.route_cache {
            Some(c) if dragging && c.links == links => {
                let moved: Vec<egui::Rect> = boxes
                    .iter()
                    .filter(|(id, r)| c.boxes.get(id) != Some(r))
                    .flat_map(|(id, r)| [Some(*r), c.boxes.get(id).copied()])
                    .flatten()
                    .collect();
                let moved_ids: HashSet<i64> = boxes.iter().filter(|(id, r)| c.boxes.get(id) != Some(r)).map(|(id, _)| *id).collect();
                let keep: Vec<Option<Vec<egui::Pos2>>> = links
                    .iter()
                    .zip(&c.paths)
                    .map(|(&(a, b, _), p)| {
                        let p = p.as_ref()?;
                        let touches = moved_ids.contains(&a) || moved_ids.contains(&b);
                        let crosses = p.windows(2).any(|s| {
                            let seg = egui::Rect::from_two_pos(s[0], s[1]).expand(0.5);
                            moved.iter().any(|r| r.shrink(1.0).intersects(seg))
                        });
                        (!touches && !crosses).then(|| p.clone())
                    })
                    .collect();
                (route_some(boxes, links, parent, &keep), true)
            }
            _ => (route_all(boxes, links, parent), false),
        };
        self.route_cache = Some(RouteCache { key, boxes: boxes.clone(), links: links.to_vec(), paths: paths.clone(), partial });
        paths
    }

    fn zoom_by(&mut self, factor: f32, rect: egui::Rect) {
        let old = self.zoom;
        let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let rel = rect.center() - (rect.min + self.pan);
        self.pan += rel * (1.0 - new / old);
        self.zoom = new;
    }
}

/// Around the systems on the map, as far as the view may scroll.
const CANVAS_MARGIN: f32 = 400.0;

/// The map's extent: every box, plus [`CANVAS_MARGIN`] around them. Fixed while the map is, so the
/// minimap keeps its scale as the view moves.
fn canvas_of(pos: &HashMap<i64, egui::Pos2>) -> Option<egui::Rect> {
    let content = pos.iter().fold(egui::Rect::NOTHING, |b, (id, p)| b.union(egui::Rect::from_min_size(*p, node_size(*id))));
    content.is_positive().then(|| content.expand(CANVAS_MARGIN))
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
    route_some(boxes, links, parent, &vec![None; links.len()])
}

/// [`route_all`] with the links that have a line in `keep` left on it: only the others are routed,
/// around the kept ones.
pub(crate) fn route_some(
    boxes: &HashMap<i64, egui::Rect>,
    links: &[(i64, i64, bool)],
    parent: &HashMap<i64, i64>,
    keep: &[Option<Vec<egui::Pos2>>],
) -> Vec<Option<Vec<egui::Pos2>>> {
    let bbox_of = |p: &[egui::Pos2]| p.iter().fold(egui::Rect::NOTHING, |r, q| r.union(egui::Rect::from_min_max(*q, *q)));
    let mut done: Vec<Routed> = keep
        .iter()
        .zip(links)
        .filter_map(|(k, &(a, b, hole))| k.as_ref().map(|p| Routed { path: p.clone(), a, b, hole, bbox: bbox_of(p) }))
        .collect();
    let mut out: Vec<Option<Vec<egui::Pos2>>> = keep.to_vec();
    let box_list: Vec<(i64, egui::Rect)> = boxes.iter().map(|(id, r)| (*id, *r)).collect();
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
            (Some(ra), Some(rb)) => {
                let d = ra.center() - rb.center();
                d.x.abs().min(d.y.abs()) as i64
            }
            _ => i64::MAX,
        }
    };
    order.sort_by_key(|&i| (!links[i].2, rise(i), i));
    let fanned = fanned_paths(boxes, links);
    for i in order {
        if keep[i].is_some() {
            continue;
        }
        let (a, b, hole) = links[i];
        let (Some(ra), Some(rb)) = (boxes.get(&a), boxes.get(&b)) else { continue };
        if let Some(p) = fanned.get(&i).filter(|p| route_cost(p, a, b, hole, boxes, &box_list, &[], f32::INFINITY) < 1_000_000.0) {
            done.push(Routed { path: p.clone(), a, b, hole, bbox: bbox_of(p) });
            out[i] = Some(p.clone());
            continue;
        }
        // On a tie the bend sits by the system the branch grows from (else the busier one), so
        // its links fan out from one trunk there.
        let a_hub = match (parent.get(&b) == Some(&a), parent.get(&a) == Some(&b)) {
            (true, _) => true,
            (_, true) => false,
            _ => degree.get(&a) >= degree.get(&b),
        };
        // The first of the cheapest, as a plain minimum would pick; a candidate stops being costed
        // once it is past the best so far.
        let mut best: Option<(Vec<egui::Pos2>, f32)> = None;
        for p in candidates(*ra, *rb, a_hub) {
            let bound = best.as_ref().map_or(f32::INFINITY, |(_, c)| *c);
            let cost = route_cost(&p, a, b, hole, boxes, &box_list, &done, bound);
            if cost < bound {
                best = Some((p, cost));
            }
        }
        let best = best.map(|(p, _)| p);
        if let Some(p) = &best {
            done.push(Routed { path: p.clone(), a, b, hole, bbox: bbox_of(p) });
        }
        out[i] = best;
    }
    nudge(&mut out, boxes);
    out
}

/// Space between two lines that would otherwise share a stretch, in map units.
pub(crate) const LANE: f32 = 7.0;

/// One straight piece of a routed line: path `path`, points `at` and `at + 1`.
struct Piece {
    path: usize,
    at: usize,
    upright: bool,
    /// x for an upright piece, y for a flat one.
    coord: f32,
    lo: f32,
    hi: f32,
}

/// Spreads lines that run on top of each other into parallel lanes. Where they part, the order of
/// the lanes follows where each turns, so they part without crossing.
fn nudge(paths: &mut [Option<Vec<egui::Pos2>>], boxes: &HashMap<i64, egui::Rect>) {
    let mut pieces: Vec<Piece> = Vec::new();
    for (pi, p) in paths.iter().enumerate() {
        let Some(p) = p else { continue };
        for (k, s) in p.windows(2).enumerate() {
            let upright = (s[0].x - s[1].x).abs() < 0.5;
            let flat = (s[0].y - s[1].y).abs() < 0.5;
            if upright == flat {
                continue;
            }
            let (coord, a, b) = if upright { (s[0].x, s[0].y, s[1].y) } else { (s[0].y, s[0].x, s[1].x) };
            pieces.push(Piece { path: pi, at: k, upright, coord, lo: a.min(b), hi: a.max(b) });
        }
    }
    // Pieces of different lines on one line and overlapping, joined transitively.
    let mut group: Vec<usize> = (0..pieces.len()).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            let (a, b) = (&pieces[i], &pieces[j]);
            if a.path != b.path && a.upright == b.upright && (a.coord - b.coord).abs() < 1.0 && a.hi.min(b.hi) - a.lo.max(b.lo) > 1.0 {
                let (ra, rb) = (root(&mut group, i), root(&mut group, j));
                group[ra] = rb;
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..pieces.len() {
        let r = root(&mut group, i);
        groups.entry(r).or_default().push(i);
    }
    let orig: Vec<Option<Vec<egui::Pos2>>> = paths.to_vec();
    let mut moves: Vec<(usize, usize, bool, f32)> = Vec::new();
    for members in groups.values().filter(|m| m.len() > 1) {
        let upright = pieces[members[0]].upright;
        // Along the lanes' axis `u`, across them `v`: an upright piece is flat with the axes swapped.
        let (u, v): (fn(egui::Pos2) -> f32, fn(egui::Pos2) -> f32) = if upright { (|p: egui::Pos2| p.y, |p: egui::Pos2| p.x) } else { (|p: egui::Pos2| p.x, |p: egui::Pos2| p.y) };
        let key = |pc: &Piece| -> (f32, f32, f32, f32) {
            let p = orig[pc.path].as_ref().unwrap();
            let (s0, s1) = (p[pc.at], p[pc.at + 1]);
            let (low_end, low_next, high_end, high_next) = if u(s0) <= u(s1) {
                (s0, pc.at.checked_sub(1).map(|i| p[i]), s1, p.get(pc.at + 2).copied())
            } else {
                (s1, p.get(pc.at + 2).copied(), s0, pc.at.checked_sub(1).map(|i| p[i]))
            };
            // Turning off towards lower `v` at the low end: the later it turns, the lower its lane.
            let low = match low_next {
                Some(n) if v(n) < v(low_end) => (0.0, -u(low_end)),
                Some(_) => (2.0, u(low_end)),
                None => (1.0, 0.0),
            };
            let high = match high_next {
                Some(n) if v(n) < v(high_end) => (0.0, u(high_end)),
                Some(_) => (2.0, -u(high_end)),
                None => (1.0, 0.0),
            };
            let far = high_next.or(low_next).map_or(0.0, v);
            (low.0 * 1e7 + low.1, high.0 * 1e7 + high.1, far, pc.path as f32)
        };
        let mut order: Vec<usize> = members.clone();
        order.sort_by(|a, b| key(&pieces[*a]).partial_cmp(&key(&pieces[*b])).unwrap_or(std::cmp::Ordering::Equal));
        // One lane per line, even where a line has two pieces in the group.
        let mut lanes: Vec<usize> = Vec::new();
        for i in &order {
            if !lanes.iter().any(|l| pieces[*l].path == pieces[*i].path) {
                lanes.push(*i);
            }
        }
        if lanes.len() < 2 {
            continue;
        }
        let coord = pieces[lanes[0]].coord;
        let lo = members.iter().map(|i| pieces[*i].lo).fold(f32::INFINITY, f32::min);
        let hi = members.iter().map(|i| pieces[*i].hi).fold(f32::NEG_INFINITY, f32::max);
        // Room across: off the boxes beside the stretch, and on the box a line ends at.
        let (mut min, mut max) = (f32::NEG_INFINITY, f32::INFINITY);
        for r in boxes.values() {
            let (ulo, uhi, vlo, vhi) = if upright { (r.top(), r.bottom(), r.left(), r.right()) } else { (r.left(), r.right(), r.top(), r.bottom()) };
            let touches = (ulo - hi).abs() < 0.5 || (uhi - lo).abs() < 0.5;
            if touches && coord > vlo && coord < vhi {
                min = min.max(vlo + 4.0);
                max = max.min(vhi - 4.0);
            } else if uhi > lo + 0.5 && ulo < hi - 0.5 {
                if vhi <= coord {
                    min = min.max(vhi + 4.0);
                } else if vlo >= coord {
                    max = max.min(vlo - 4.0);
                }
            }
        }
        let n = lanes.len() as f32;
        let room = (max - min).max(0.0);
        let step = if room.is_finite() { LANE.min(room / (n - 1.0)) } else { LANE };
        let half = step * (n - 1.0) / 2.0;
        let centre = if min.is_finite() && max.is_finite() && max - min >= 2.0 * half {
            coord.clamp(min + half, max - half)
        } else if min.is_finite() && max.is_finite() {
            (min + max) / 2.0
        } else {
            coord
        };
        for (k, first) in lanes.iter().enumerate() {
            let at = centre - half + k as f32 * step;
            let line = pieces[*first].path;
            for i in members.iter().filter(|i| pieces[**i].path == line) {
                moves.push((line, pieces[*i].at, upright, at));
            }
        }
    }
    for (line, at, upright, to) in moves {
        let Some(p) = paths[line].as_mut() else { continue };
        for q in [at, at + 1] {
            if upright {
                p[q].x = to;
            } else {
                p[q].y = to;
            }
        }
    }
}

/// Lines out of one side of a box that has pinned systems hanging off it, laid out together: each
/// leaves at its own port, in the top-to-bottom order of the boxes they go to, and those going the
/// same way turn in nested order, so none crosses or runs along another. By link index.
fn fanned_paths(boxes: &HashMap<i64, egui::Rect>, links: &[(i64, i64, bool)]) -> HashMap<usize, Vec<egui::Pos2>> {
    use egui::pos2;
    let mut out = HashMap::new();
    let with_pills: HashSet<i64> = links.iter().filter(|l| !l.2).map(|l| l.0).collect();
    for a in with_pills {
        let Some(ra) = boxes.get(&a) else { continue };
        for right in [true, false] {
            // A hole stored from its far end still leaves this box; its path is turned round after.
            let mut side: Vec<(usize, egui::Rect, bool)> = links
                .iter()
                .enumerate()
                .filter_map(|(i, l)| match (l.0 == a, l.1 == a) {
                    (true, _) => Some((i, *boxes.get(&l.1)?, false)),
                    (_, true) => Some((i, *boxes.get(&l.0)?, true)),
                    _ => None,
                })
                .filter(|(_, rb, _)| if right { rb.left() - ra.right() >= 2.0 * STUB } else { ra.left() - rb.right() >= 2.0 * STUB })
                .collect();
            if side.len() < 2 {
                continue;
            }
            side.sort_by(|x, y| x.1.center().y.total_cmp(&y.1.center().y));
            let n = side.len();
            let gap = ((ra.height() - 6.0) / (n - 1) as f32).min(10.0);
            let port = |k: usize| ra.center().y + (k as f32 - (n - 1) as f32 / 2.0) * gap;
            let (edge, dir) = if right { (ra.right(), 1.0) } else { (ra.left(), -1.0) };
            let far = side.iter().map(|(_, rb, _)| if right { rb.left() } else { -rb.right() }).fold(f32::INFINITY, f32::min);
            let room = (far * dir - STUB) - (edge + dir * STUB);
            let step = if n > 1 { (room.abs() / (n - 1) as f32).min(8.0) } else { 0.0 };
            // Going up, the topmost turns first; going down, the bottommost.
            let (mut up, mut down): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
            for (k, (_, rb, _)) in side.iter().enumerate() {
                if rb.center().y < port(k) - 0.5 {
                    up.push(k);
                } else if rb.center().y > port(k) + 0.5 {
                    down.push(k);
                }
            }
            down.reverse();
            let mut turn: HashMap<usize, usize> = HashMap::new();
            for (rank, k) in up.iter().enumerate() {
                turn.insert(*k, rank);
            }
            for (rank, k) in down.iter().enumerate() {
                turn.insert(*k, rank);
            }
            for (k, (i, rb, turned)) in side.iter().enumerate() {
                let (pa, pb) = (port(k), rb.center().y);
                let end = if right { rb.left() } else { rb.right() };
                let path = match turn.get(&k) {
                    None => vec![pos2(edge, pa), pos2(end, pa)],
                    Some(rank) => {
                        let x = edge + dir * (STUB + *rank as f32 * step);
                        vec![pos2(edge, pa), pos2(x, pa), pos2(x, pb), pos2(end, pb)]
                    }
                };
                let mut path = simplify(path);
                if *turned {
                    path.reverse();
                }
                out.insert(*i, path);
            }
        }
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
            // Down, across and down, for boxes one above the other. Side by side, a line leaves
            // from the side facing the other box, never from the top or bottom.
            let (qa, qb) = (xa + oa, xb + ob);
            let beside = b.left() - a.right() >= 2.0 * STUB || a.left() - b.right() >= 2.0 * STUB;
            if !beside && b.top() - a.bottom() >= 2.0 * STUB {
                for y in [a.bottom() + STUB, b.top() - STUB, (a.bottom() + b.top()) / 2.0] {
                    out.push(vec![pos2(qa, a.bottom()), pos2(qa, y), pos2(qb, y), pos2(qb, b.top())]);
                }
            }
            if !beside && a.top() - b.bottom() >= 2.0 * STUB {
                for y in [a.top() - STUB, b.bottom() + STUB, (a.top() + b.bottom()) / 2.0] {
                    out.push(vec![pos2(qa, a.top()), pos2(qa, y), pos2(qb, y), pos2(qb, b.bottom())]);
                }
            }
        }
    }
    out.into_iter().map(simplify).collect()
}

/// A link already routed, with the box around its line for skipping it where it is far away.
struct Routed {
    path: Vec<egui::Pos2>,
    a: i64,
    b: i64,
    hole: bool,
    bbox: egui::Rect,
}

/// What `path` costs, or infinity once it is sure to be over `bound`.
#[allow(clippy::too_many_arguments)]
fn route_cost(
    path: &[egui::Pos2],
    a: i64,
    b: i64,
    hole: bool,
    boxes: &HashMap<i64, egui::Rect>,
    box_list: &[(i64, egui::Rect)],
    done: &[Routed],
    bound: f32,
) -> f32 {
    // Off-centre ports are for keeping off other lines, not for shaving a few pixels.
    let off = |p: egui::Pos2, r: &egui::Rect| {
        if (p.x - r.left()).abs() < 0.5 || (p.x - r.right()).abs() < 0.5 { (p.y - r.center().y).abs() } else { (p.x - r.center().x).abs() }
    };
    let ports = boxes.get(&a).map_or(0.0, |r| off(path[0], r)) + boxes.get(&b).map_or(0.0, |r| off(path[path.len() - 1], r));
    let bends = 25.0 * path.len().saturating_sub(2) as f32;
    let tail = bends + 4.0 * ports;
    let mut cost = 0.0;
    for s in path.windows(2) {
        let seg = egui::Rect::from_two_pos(s[0], s[1]).expand(0.5);
        for (id, r) in box_list {
            if *id != a && *id != b && r.shrink(1.0).intersects(seg) {
                cost += 1_000_000.0;
            }
        }
        cost += (s[1] - s[0]).length();
        // Lines further than this can neither run along nor cross the segment.
        let near = seg.expand(3.5);
        for other in done {
            if !near.intersects(other.bbox) {
                continue;
            }
            // Holes out of one system share their trunk; any other shared stretch is ambiguous.
            let fan_out = hole && other.hole && (other.a == a || other.b == a || other.a == b || other.b == b);
            let per_px = if fan_out { 0.0 } else { 60.0 };
            for t in other.path.windows(2) {
                cost += per_px * overlap(s[0], s[1], t[0], t[1]);
                if !fan_out && crosses(s[0], s[1], t[0], t[1]) {
                    cost += 40.0;
                }
            }
        }
        if cost + tail > bound {
            return f32::INFINITY;
        }
    }
    cost + bends + 4.0 * ports
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
#[cfg(test)]
pub(crate) fn auto_layout(edges: &[(i64, i64)], score: impl Fn(i64) -> i64) -> Vec<(i64, Option<i64>, egui::Pos2)> {
    super::wh_layout::layout(edges, &[], score, super::wh_layout::Opts::default())
}

/// The auto layout with systems already on the map (placed before, or dragged) kept where they
/// are. A new system goes where the layout would put it relative to its parent, or to the nearest
/// free spot from there: above or below first, then a column further out.
#[cfg(test)]
pub(crate) fn place(auto: &[(i64, Option<i64>, egui::Pos2)], dragged: &HashMap<i64, egui::Pos2>) -> HashMap<i64, egui::Pos2> {
    place_with(auto, dragged, super::wh_layout::Opts::default())
}

/// [`place`] for a layout growing the way `opts` says: a new system looks beside its spot first,
/// then a level further out.
pub(crate) fn place_with(auto: &[(i64, Option<i64>, egui::Pos2)], dragged: &HashMap<i64, egui::Pos2>, opts: super::wh_layout::Opts) -> HashMap<i64, egui::Pos2> {
    let (level, beside) = opts.steps();
    // Clusters, by their root, that already have a system where it was left.
    let mut root_of: HashMap<i64, i64> = HashMap::new();
    for (n, parent, _) in auto {
        let r = parent.and_then(|p| root_of.get(&p).copied()).unwrap_or(*n);
        root_of.insert(*n, r);
    }
    let settled: HashSet<i64> = auto.iter().filter(|(n, _, _)| dragged.contains_key(n)).map(|(n, _, _)| root_of[n]).collect();
    let auto_at: HashMap<i64, egui::Pos2> = auto.iter().map(|(n, _, p)| (*n, *p)).collect();
    let mut at: HashMap<i64, egui::Pos2> = HashMap::new();
    // Boxes differ in width, so each pair is checked with its own.
    let clear = |at: &HashMap<i64, egui::Pos2>, n: i64, p: egui::Pos2| {
        // Under the layout's own gap between neighbours, or every packed sibling would count as
        // in the way.
        let me = egui::Rect::from_min_size(p, node_size(n)).expand(2.0);
        at.iter().all(|(id, q)| !me.intersects(egui::Rect::from_min_size(*q, node_size(*id)).expand(2.0)))
    };
    // A remembered spot another system has since taken (this one was gone a while, say) counts
    // as no spot: it is placed afresh rather than on top of the other.
    for (n, _, _) in auto {
        if let Some(p) = dragged.get(n) {
            if clear(&at, *n, *p) {
                at.insert(*n, *p);
            }
        }
    }
    for (n, parent, p) in auto {
        if at.contains_key(n) {
            continue;
        }
        let want = match parent {
            Some(par) => at[par] + (*p - auto_at[par]),
            // A cluster turning up on a map already laid out goes below all of it: its spot in a
            // fresh layout would sit among clusters that have since stayed where they were.
            None if !dragged.is_empty() && !at.is_empty() && !settled.contains(n) => {
                let bottom = at.iter().map(|(id, q)| q.y + node_size(*id).y).fold(f32::MIN, f32::max);
                egui::pos2(0.0, bottom + super::wh_layout::CHAIN_GAP)
            }
            None => *p,
        };
        let spot = (0..4)
            .flat_map(|col| {
                (0..60).map(move |k: i32| {
                    let step = if k % 2 == 0 { k / 2 } else { -(k + 1) / 2 };
                    want + level * col as f32 + beside * step as f32
                })
            })
            .find(|c| clear(&at, *n, *c))
            .unwrap_or(want);
        at.insert(*n, spot);
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

/// What is left of a hole, short, in its warning colour: the reading while it holds, then the
/// clock's worse figure. `None` when nothing is known and nothing is near.
fn life_badge(w: &Wormhole, now: i64, visuals: &egui::Visuals) -> Option<(String, egui::Color32)> {
    let t = time_left(w, now);
    Some(match t {
        TimeLeft::Plenty => (w.life?.short().to_owned(), visuals.text_color()),
        TimeLeft::Under12h => ("<12h".to_owned(), time_color(t)),
        TimeLeft::Under4h => ("<4h".to_owned(), time_color(t)),
        TimeLeft::Under1h => ("<1h".to_owned(), time_color(t)),
        TimeLeft::Expiring => ("Expired".to_owned(), time_color(t)),
    })
}

/// A hole's line colour: how long it has left, from green to red, and purple once it could close
/// at any moment.
fn time_color(t: TimeLeft) -> egui::Color32 {
    use egui::Color32 as C;
    match t {
        TimeLeft::Plenty => C::from_rgb(0x6f, 0xc2, 0x76),
        TimeLeft::Under12h => C::from_rgb(0xF2, 0xD0, 0x4A),
        TimeLeft::Under4h => C::from_rgb(0xFF, 0x8F, 0x2A),
        TimeLeft::Under1h => C::from_rgb(0xff, 0x4a, 0x4a),
        TimeLeft::Expiring => C::from_rgb(0xC0, 0x5C, 0xE0),
    }
}

/// A line that breaks up as the hole's mass goes: solid, long dashes under half, a jagged line
/// under a tenth.
fn stroke_hole(painter: &egui::Painter, line: &[egui::Pos2], stroke: egui::Stroke, mass: Option<Mass>) {
    match mass {
        None | Some(Mass::Fresh) => {
            painter.add(egui::Shape::line(line.to_vec(), stroke));
        }
        Some(Mass::Reduced) => painter.extend(egui::Shape::dashed_line(line, stroke, 12.0, 6.0)),
        Some(Mass::Critical) => {
            painter.add(egui::Shape::line(zigzag(line, 7.0, 1.6), egui::Stroke::new(stroke.width * 0.85, stroke.color)));
        }
    }
}

/// A hole's switch for routes, lit while it is off. Returns whether it was clicked.
pub(crate) fn wh_route_toggle(ui: &mut egui::Ui, off: bool) -> bool {
    let text = if off { egui::RichText::new(icon::PROHIBIT).color(crate::theme::standing::HOSTILE) } else { egui::RichText::new(icon::PROHIBIT) };
    ui.small_button(text)
        .on_hover_text(if off { "Switched off for routes: click to let routes use it" } else { "Do not use this hole in routes" })
        .clicked()
}

/// `c` with most of its colour taken out, for what routes may not use.
pub(crate) fn desaturate(c: egui::Color32) -> egui::Color32 {
    let grey = 0.3 * c.r() as f32 + 0.59 * c.g() as f32 + 0.11 * c.b() as f32;
    let mix = |v: u8| (v as f32 * 0.15 + grey * 0.85 * 0.8).round() as u8;
    egui::Color32::from_rgba_unmultiplied(mix(c.r()), mix(c.g()), mix(c.b()), c.a())
}

pub(crate) fn desaturate_stroke(s: egui::Stroke) -> egui::Stroke {
    egui::Stroke::new(s.width, desaturate(s.color))
}

/// Gap in a switched-off line, in screen pixels, that its slash stands in.
const SLASH_GAP: f32 = 10.0;

/// `line` cut with a gap for its slash, and the cut's centre and direction. The cut goes in the
/// middle of the last leg, which a line fanning out of a shared trunk has to itself, or in the
/// middle of the whole line when that leg is short. `None` for a line too short to cut.
fn split_middle(line: &[egui::Pos2], gap: f32) -> Option<(Vec<egui::Pos2>, Vec<egui::Pos2>, egui::Pos2, egui::Vec2)> {
    let total: f32 = line.windows(2).map(|s| (s[1] - s[0]).length()).sum();
    if line.len() < 2 || total < gap * 3.0 {
        return None;
    }
    let last = (line[line.len() - 1] - line[line.len() - 2]).length();
    let half = if last >= gap * 3.0 { total - last / 2.0 } else { total / 2.0 };
    let (cut_a, cut_b) = (half - gap / 2.0, half + gap / 2.0);
    let (mut first, mut second) = (vec![line[0]], Vec::new());
    let (mut mid, mut dir) = (line[0], egui::Vec2::X);
    let mut walked = 0.0;
    for s in line.windows(2) {
        let len = (s[1] - s[0]).length();
        let d = if len > 0.0 { (s[1] - s[0]) / len } else { egui::Vec2::X };
        let at = |t: f32| s[0] + d * (t - walked);
        if walked + len <= cut_a {
            first.push(s[1]);
        } else if walked < cut_a {
            first.push(at(cut_a));
        }
        if walked <= half && half < walked + len {
            (mid, dir) = (at(half), d);
        }
        if walked <= cut_b && cut_b < walked + len {
            second.push(at(cut_b));
            second.push(s[1]);
        } else if walked > cut_b {
            second.push(s[1]);
        }
        walked += len;
    }
    Some((first, second, mid, dir))
}

/// A switched-off line: drawn by `draw` as usual but broken at the middle, a slash across the break.
pub(crate) fn slashed(painter: &egui::Painter, line: &[egui::Pos2], stroke: egui::Stroke, draw: impl Fn(&egui::Painter, &[egui::Pos2], egui::Stroke)) {
    let Some((a, b, mid, dir)) = split_middle(line, SLASH_GAP) else {
        draw(painter, line, stroke);
        return;
    };
    draw(painter, &a, stroke);
    draw(painter, &b, stroke);
    let across = egui::vec2(-dir.y, dir.x) * 7.0 + dir * 4.0;
    painter.line_segment([mid - across, mid + across], egui::Stroke::new(stroke.width.max(1.5), stroke.color));
}

/// Drawing order: solid first, so a broken line on a shared stretch is not hidden beneath one.
fn mass_rank(m: Option<Mass>) -> u8 {
    match m {
        None | Some(Mass::Fresh) => 0,
        Some(Mass::Reduced) => 1,
        Some(Mass::Critical) => 2,
    }
}

/// `line` redrawn as a zigzag: a point every `step` pixels along it, pushed `amp` to either side
/// in turn. The ends stay put so it still meets its boxes.
fn zigzag(line: &[egui::Pos2], step: f32, amp: f32) -> Vec<egui::Pos2> {
    let total: f32 = line.windows(2).map(|s| (s[1] - s[0]).length()).sum();
    let Some(&first) = line.first() else { return Vec::new() };
    let mut out = vec![first];
    let (mut seg, mut into, mut k) = (0usize, 0.0f32, 1usize);
    let mut d = step;
    while d < total - step * 0.5 {
        // Walk to the segment holding distance `d`.
        while seg + 1 < line.len() && into + (line[seg + 1] - line[seg]).length() < d {
            into += (line[seg + 1] - line[seg]).length();
            seg += 1;
        }
        if seg + 1 >= line.len() {
            break;
        }
        let (a, b) = (line[seg], line[seg + 1]);
        let dir = (b - a).normalized();
        let at = a + dir * (d - into);
        let side = if k % 2 == 0 { amp } else { -amp };
        out.push(at + egui::vec2(-dir.y, dir.x) * side);
        d += step;
        k += 1;
    }
    out.push(*line.last().unwrap());
    out
}

/// What goes in front of a system's name: its security in k-space, its class in wormhole space,
/// and a single letter for the named ones, which are unique.
fn system_tag(c: Class, security: f64) -> String {
    match c {
        Class::W(n) => format!("C{n}"),
        Class::Thera => "T".into(),
        Class::Drifter(14) => "S".into(),
        Class::Drifter(15) => "B".into(),
        Class::Drifter(16) => "V".into(),
        Class::Drifter(17) => "C".into(),
        Class::Drifter(18) => "R".into(),
        Class::Drifter(_) => "D".into(),
        Class::Pochven => "Poch".into(),
        _ => format!("{security:.1}"),
    }
}

fn drifter_color() -> egui::Color32 {
    class_color(Class::Drifter(14), -1.0)
}

/// A box's fill tinted with the drifter colour, enough to pick out at a glance.
fn drifter_fill(base: egui::Color32) -> egui::Color32 {
    let d = drifter_color();
    let mix = |a: u8, b: u8| (a as f32 * 0.78 + b as f32 * 0.22).round() as u8;
    egui::Color32::from_rgb(mix(base.r(), d.r()), mix(base.g(), d.g()), mix(base.b(), d.b()))
}

/// A pinned system this few gate jumps from a chain is close enough to stand out: green under
/// five, yellow under ten.
fn close_color(jumps: u32) -> Option<egui::Color32> {
    match jumps {
        0..5 => Some(egui::Color32::from_rgb(0x5F, 0xD0, 0x6E)),
        5..10 => Some(egui::Color32::from_rgb(0xF2, 0xD0, 0x4A)),
        _ => None,
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
        self.wh_cache.iter().filter(|w| self.wh_shown(w, now)).collect()
    }

    /// Whether a hole joins a kind of space at either end: it goes both ways, so a hole found
    /// in highsec leads to highsec as much as one leading there.
    pub(crate) fn wh_touches(&self, w: &Wormhole, d: crate::wormholes::DestClass) -> bool {
        w.dest == d || self.systems.as_ref().is_some_and(|g| crate::app::wormholes_ui::dest_class(g, w.system_id) == d)
    }

    pub(crate) fn wh_graph_view(&mut self, ui: &mut egui::Ui) {
        let Some(geo) = self.systems.clone() else {
            ui.label(egui::RichText::new("The system map is still loading.").weak());
            return;
        };
        let now = chrono::Utc::now().timestamp();
        let mut holes: Vec<Wormhole> = self.wh_graph_visible(now).into_iter().cloned().collect();
        // The side panel lists every hole of the selected system, drawn on the map or not.
        let all_holes = holes.clone();
        self.wh_graph_list(ui, &geo, &holes);
        let focus = self.wh_graph.focus;
        if let Some(f) = focus {
            let near = within(&holes, f, self.wh_graph.depth());
            holes.retain(|w| near.contains(&w.system_id) && w.dest_system_id.is_none_or(|b| near.contains(&b)));
        }
        let mut edges: Vec<(i64, i64)> = holes.iter().filter_map(|w| Some((w.system_id, w.dest_system_id?))).collect();
        // Pinned systems join the holes by gates, so they tie the chains together.
        // (exit, where the pin is met by gates, jumps, the pin)
        let mut gate_links: Vec<(i64, i64, u32, i64)> = Vec::new();
        // The drifter systems are hubs of their own: on the map and joined to the chains like
        // pinned systems, though the Routes panel lists only what the user pins.
        let drifters: Vec<i64> = whdata::DRIFTERS.iter().map(|d| d.2).filter(|id| geo.info_of(*id).is_some()).collect();
        let chars: HashMap<String, (i64, bool)> = self.player.lock().unwrap().locations.clone();
        let mut pins: Vec<i64> = self.settings.wh_route_pins.iter().filter_map(|p| geo.lookup(p).map(|i| i.id)).collect();
        let user_pins = pins.clone();
        for d in &drifters {
            if !pins.contains(d) {
                pins.push(*d);
            }
        }
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
                    if !gate_links.iter().any(|(x, y, _, _)| (*x, *y) == (exit, anchor) || (*x, *y) == (anchor, exit)) {
                        gate_links.push((exit, anchor, n, *pid));
                    }
                }
            }
        }
        // A gate link is a way somewhere only when it is short: further out it says nothing.
        gate_links.retain(|(_, _, n, _)| *n < NEAR_JUMPS);
        // The overview keeps every wormhole system but only the k-space exits that lead somewhere:
        // a pinned system, a character, or the short way to either. The rest are counted on the
        // box they hang from. k-space to k-space and k-space to Pochven holes always stay.
        let mut hidden: HashMap<i64, usize> = HashMap::new();
        let mut only_counted: Vec<i64> = Vec::new();
        if focus.is_none() {
            let keep: HashSet<i64> = pins.iter().copied().chain(gate_links.iter().flat_map(|(e, a, _, _)| [*e, *a])).collect();
            hidden = overview(&mut holes, |id| kspace(&id), &keep);
            // A wormhole system whose every hole went into the count still shows, with its count.
            let drawn: HashSet<i64> = holes.iter().flat_map(|w| [Some(w.system_id), w.dest_system_id]).flatten().collect();
            only_counted = hidden.keys().copied().filter(|id| !drawn.contains(id)).collect();
            edges = holes.iter().filter_map(|w| Some((w.system_id, w.dest_system_id?))).collect();
        }
        // A pinned system joins every cluster near it as a rounded element beside the cluster's
        // nearest exit, so the way to it reads at a glance without lines across the map. A pin
        // near nothing still shows, once, on its own.
        let mut pills: HashMap<i64, (i64, u32)> = HashMap::new();
        let mut pill_alone: Vec<i64> = Vec::new();
        let clusters = components(&edges);
        for pid in user_pins.iter().filter(|p| !drifters.contains(p)) {
            let mut shown = false;
            for cl in &clusters {
                if cl.contains(pid) {
                    shown = true;
                    continue;
                }
                let best = anchors[pid]
                    .iter()
                    .flat_map(|a| cl.iter().filter(|e| kspace(e)).filter_map(move |e| Some((*e, dist(*a, *e)?))))
                    .min_by_key(|(_, n)| *n)
                    .filter(|(_, n)| *n < NEAR_JUMPS);
                if let Some((exit, n)) = best {
                    let id = pill_id(*pid, exit);
                    pills.insert(id, (*pid, n));
                    edges.push((exit, id));
                    shown = true;
                }
            }
            if !shown {
                let id = pill_id(*pid, 0);
                pills.insert(id, (*pid, u32::MAX));
                pill_alone.push(id);
            }
        }
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
        let mut alone: Vec<i64> = if focus.is_none() { drifters.clone() } else { Vec::new() };
        alone.extend(pill_alone);
        alone.extend(only_counted);
        alone.sort_unstable();
        alone.dedup();
        let opts = self.wh_layout_opts();
        let auto = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            let mut ids: Vec<i64> = edges.iter().flat_map(|(a, b)| [*a, *b]).chain(alone.iter().copied()).collect();
            ids.sort_unstable();
            ids.dedup();
            let scored: Vec<(i64, i64)> = ids.iter().map(|id| (*id, score(*id))).collect();
            (&edges, &alone, &scored, opts.style, opts.aspect.map(f32::to_bits)).hash(&mut h);
            let key = h.finish();
            match &self.wh_graph.layout_cache {
                Some((k, a)) if *k == key => a.clone(),
                _ => {
                    let a = super::wh_layout::layout(&edges, &alone, &score, opts);
                    self.wh_graph.layout_cache = Some((key, a.clone()));
                    a
                }
            }
        };
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
            place_with(&auto, &self.wh_graph.focus_dragged, opts)
        } else {
            place_with(&auto, self.wh_graph.dragged.as_ref().unwrap(), opts)
        };
        // Everything placed stays put from now on, so a system turning up (synced, detected or
        // added) never shuffles the ones already there. Remembered across restarts outside focus.
        {
            let known = if focus.is_some() { &mut self.wh_graph.focus_dragged } else { self.wh_graph.dragged.get_or_insert_default() };
            let fresh: Vec<(i64, egui::Pos2)> = pos.iter().filter(|(id, _)| !known.contains_key(id)).map(|(id, p)| (*id, *p)).collect();
            for (id, p) in &fresh {
                known.insert(*id, *p);
            }
            if focus.is_none() {
                if let Some(store) = self.store.as_ref() {
                    for (id, p) in &fresh {
                        store.set_wh_layout(*id, p.x, p.y);
                    }
                }
            }
        }
        if let Some((id, p)) = self.wh_graph.drag {
            pos.insert(id, p);
        }

        // Holes whose far side is unknown, by the system they are in.
        let mut open: HashMap<i64, usize> = HashMap::new();
        for w in holes.iter().filter(|w| w.dest_system_id.is_none()) {
            *open.entry(w.system_id).or_default() += 1;
        }

        self.wh_graph_side(ui, &geo, &all_holes, &chars, now);

        let mut focus_on: Option<Option<i64>> = None;
        // The view's own controls, a row above the canvas.
        let focus_name = focus.and_then(|f| geo.info_of(f)).map(|i| i.name.clone());
        let mut zoom_step: Option<f32> = None;
        let mut tidy = false;
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
                        if ui
                            .button(format!("{}  Tidy", icon::TREE_STRUCTURE))
                            .on_hover_text("Lay the whole map out afresh, forgetting where systems were dragged")
                            .clicked()
                        {
                            tidy = true;
                        }
                        ui.menu_button(format!("{}  Layout", icon::CARET_DOWN), |ui| {
                            use super::wh_layout::Style;
                            let style = Style::from_code(&self.settings.wh_layout_style);
                            let pick = |ui: &mut egui::Ui, on: bool, label: &str, hint: &str| ui.menu_label(on, label).on_hover_text(hint).clicked();
                            ui.label(egui::RichText::new("Style").weak());
                            if pick(ui, style == Style::Tree, "Tree", "Each chain as a compact tree from its most important system") {
                                self.settings.wh_layout_style = Style::Tree.code().to_owned();
                                tidy = true;
                            }
                            if pick(ui, style == Style::Layered, "Layered", "Fewer crossing lines where holes close loops") {
                                self.settings.wh_layout_style = Style::Layered.code().to_owned();
                                tidy = true;
                            }
                            ui.separator();
                            ui.label(egui::RichText::new("Separate chains").weak());
                            if pick(ui, self.settings.wh_layout_pack, "Packed to the window", "Side by side in rows, to fill the window's shape") {
                                self.settings.wh_layout_pack = true;
                                tidy = true;
                            }
                            if pick(ui, !self.settings.wh_layout_pack, "In one line", "One after another") {
                                self.settings.wh_layout_pack = false;
                                tidy = true;
                            }
                            ui.separator();
                            if ui.checkbox(&mut self.settings.wh_minimap, "Minimap").changed() {
                                self.needs_save = true;
                            }
                            if tidy {
                                ui.close();
                            }
                        });
                        if ui.menu_label(self.settings.wh_legend_open, format!("{}  Legend", icon::BOOK_OPEN)).clicked() {
                            self.settings.wh_legend_open = !self.settings.wh_legend_open;
                            self.needs_save = true;
                        }
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

        if tidy {
            self.needs_save = true;
            self.wh_graph_tidy();
        }
        if self.settings.wh_legend_open {
            legend(ui);
        }

        let rect = ui.available_rect_before_wrap();
        self.wh_graph.canvas = Some(rect);
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
        // The view's centre stays over the systems, so the map cannot be scrolled off into nothing.
        if let Some(canvas) = canvas_of(&pos) {
            let content = canvas.shrink(CANVAS_MARGIN);
            let centre = ((rect.center() - rect.min - self.wh_graph.pan) / self.wh_graph.zoom).to_pos2();
            let c = centre.clamp(content.min, content.max);
            self.wh_graph.pan = rect.center() - rect.min - c.to_vec2() * self.wh_graph.zoom;
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
        // Far out, a box is too short for 12px text: just the name, as big as the box allows.
        let tiny = zoom < 0.5;
        let name_font = if tiny {
            egui::FontId::new((NODE.y * zoom - 3.0).clamp(8.0, 12.0), body.family.clone())
        } else {
            font.clone()
        };
        // Never below readable when the boxes are small; growing with them once they are big.
        let chip_font = egui::FontId::proportional(if detail { (12.0 * zoom).clamp(12.0, 22.0) } else { 11.0 });
        let chip_pad = if detail { egui::vec2(8.0, 2.0) * zoom.max(1.0) } else { egui::vec2(4.0, 1.0) };
        let chip_gap = if detail { 3.0 } else { 2.0 };
        // What must be seen at any zoom sits in chips on the box's right: shattered, our
        // characters, holes not drawn, far sides unknown.
        let off_systems: HashSet<i64> = self.settings.wh_disabled_systems.iter().copied().collect();
        let chips_of = |id: i64| -> Vec<(String, egui::Color32)> {
            let mut chips = Vec::new();
            let Some(info) = geo.info_of(id) else { return chips };
            if off_systems.contains(&id) {
                chips.push((icon::PROHIBIT.to_owned(), visuals.weak_text_color()));
            }
            if matches!(whdata::class_of(id, info.security, &info.region), Class::Drifter(_)) {
                chips.push((icon::SKULL.to_owned(), drifter_color()));
            } else if whdata::jsystem(id).is_some_and(|j| j.shattered()) {
                chips.push((icon::DIAMONDS_FOUR.to_owned(), SHATTERED_COLOR));
            }
            if let Some(who) = here.get(&id) {
                chips.push((format!("{} {}", icon::USER, who.len()), visuals.hyperlink_color));
            }
            if let Some(n) = hidden.get(&id) {
                chips.push((format!("+{n}"), visuals.text_color()));
            }
            if let Some(n) = open.get(&id) {
                chips.push((format!("{n} ?"), visuals.warn_fg_color));
            }
            chips
        };
        let chips_width = |chips: &[(String, egui::Color32)]| -> f32 {
            chips
                .iter()
                .map(|(t, c)| painter.layout_no_wrap(t.clone(), chip_font.clone(), *c).size().x + chip_pad.x + chip_gap)
                .sum()
        };
        // The name gets what the chips leave, and ends in an ellipsis when that is not enough.
        let line1_of = |id: i64, room: f32| {
            let info = geo.info_of(id)?;
            let c = whdata::class_of(id, info.security, &info.region);
            let mut job = egui::text::LayoutJob::default();
            job.wrap = egui::text::TextWrapping {
                max_width: room.max(1.0),
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some('\u{2026}'),
            };
            job.append(&system_tag(c, info.security), 0.0, egui::TextFormat::simple(name_font.clone(), class_color(c, info.security)));
            let lead = if tiny { 4.0 } else { 6.0 };
            job.append(&display_name(id, &info.name), lead, egui::TextFormat::simple(name_font.clone(), visuals.strong_text_color()));
            Some(painter.layout_job(job))
        };
        #[allow(clippy::type_complexity)]
        let rects: HashMap<i64, (egui::Rect, Option<std::sync::Arc<egui::Galley>>, Vec<(String, egui::Color32)>)> = pos
            .iter()
            .map(|(id, p)| {
                let r = egui::Rect::from_min_size(to_screen(*p), node_size(*id) * zoom);
                let chips = chips_of(*id);
                let lead = if detail { 8.0 * zoom } else { 6.0 };
                let room = r.width() - lead - 4.0 - chips_width(&chips) - 4.0;
                (*id, (r, line1_of(*id, room), chips))
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
        let world: HashMap<i64, egui::Rect> = pos.iter().map(|(id, p)| (*id, egui::Rect::from_min_size(*p, node_size(*id)))).collect();
        let pill_links: Vec<(i64, i64)> = edges.iter().copied().filter(|(_, b)| pills.contains_key(b)).collect();
        let links: Vec<(i64, i64, bool)> = holes
            .iter()
            .map(|w| (w.system_id, w.dest_system_id.unwrap_or(0), true))
            .chain(pill_links.iter().map(|(a, b)| (*a, *b, false)))
            .collect();
        let parent: HashMap<i64, i64> = auto.iter().filter_map(|(n, p, _)| Some((*n, (*p)?))).collect();
        let routes = self.wh_graph.routes(&world, &links, &parent);
        let hole_paths: Vec<Option<Vec<egui::Pos2>>> = routes[..holes.len()].to_vec();
        let screen = |path: &[egui::Pos2]| rounded(&path.iter().map(|p| to_screen(*p)).collect::<Vec<_>>(), 10.0 * zoom);
        let hit = |line: &[egui::Pos2]| pointer.is_some_and(|p| line.windows(2).any(|s| dist_to_segment(p, s[0], s[1]) < 6.0));

        #[cfg(test)]
        EDGE_PROBE.with(|p| p.borrow_mut().clear());
        // Every line first, so no line is ever drawn over a label. Solid ones go down first and
        // each line on a thin band of the background: where lines share a stretch, a dashed one on
        // top keeps its gaps instead of a solid one beneath showing through them.
        let blocked: HashSet<i64> = holes.iter().filter(|w| self.wh_blocked(w, now)).map(|w| w.id).collect();
        let mut order: Vec<usize> = (0..holes.len()).collect();
        order.sort_by_key(|&i| mass_rank(holes[i].mass));
        for wi in order {
            let w = &holes[wi];
            let Some(path) = &hole_paths[wi] else { continue };
            let line = screen(path);
            #[cfg(test)]
            EDGE_PROBE.with(|p| p.borrow_mut().push((w.id, line.clone())));
            // Colour is how long it has left; the pattern is how much mass.
            let hot = hovered_edge.is_none() && hit(&line);
            let width = if hot { 4.0 } else { 2.5 };
            if mass_rank(w.mass) > 0 {
                painter.add(egui::Shape::line(line.clone(), egui::Stroke::new(width + 3.0, visuals.panel_fill)));
            }
            let stroke = egui::Stroke::new(width, time_color(time_left(w, now)));
            if blocked.contains(&w.id) {
                slashed(&painter, &line, desaturate_stroke(stroke), |p, l, s| stroke_hole(p, l, s, w.mass));
            } else {
                stroke_hole(&painter, &line, stroke, w.mass);
            }
            if hot {
                hovered_edge = Some(w);
            }
        }
        // A pinned system's copy hangs off its exit by a short dotted line, in its distance colour.
        for (li, &(_, pill)) in pill_links.iter().enumerate() {
            let Some(path) = &routes[holes.len() + li] else { continue };
            let n = pills.get(&pill).map_or(u32::MAX, |(_, n)| *n);
            painter.extend(egui::Shape::dotted_line(&screen(path), close_color(n).unwrap_or(visuals.weak_text_color()), 6.0, 1.8));
        }

        // Then the labels, in map units, each off every other label, every box and, where it can
        // be, every line that is not its own: a label on a shared stretch could belong to either.
        // The pinned systems' lines follow the holes', so a hole's index is its own here too.
        let lines: Vec<&[egui::Pos2]> = routes.iter().map(|p| p.as_deref().unwrap_or(&[])).collect();
        let others = |own: usize| lines.iter().enumerate().filter(move |(i, _)| *i != own).map(|(_, l)| *l).collect::<Vec<_>>();
        let boxes: Vec<egui::Rect> = pos.iter().map(|(id, p)| egui::Rect::from_min_size(*p, node_size(*id))).collect();
        let mut taken: Vec<egui::Rect> = Vec::new();
        let draw_label = |r: egui::Rect, g: std::sync::Arc<egui::Galley>, border: Option<egui::Color32>| {
            let sr = egui::Rect::from_min_max(to_screen(r.min), to_screen(r.max));
            match border {
                Some(c) => painter.rect(sr, 3.0, visuals.extreme_bg_color, egui::Stroke::new(1.0, c), egui::StrokeKind::Outside),
                None => painter.rect_filled(sr, 3.0, visuals.extreme_bg_color),
            };
            painter.galley(sr.center() - g.size() / 2.0, g, visuals.text_color());
        };
        let pad = egui::vec2(6.0, 1.0);
        let sig_font = egui::FontId::new(font.size * 0.8, font.family.clone());
        if detail {
            for (wi, w) in holes.iter().enumerate() {
                let Some(path) = &hole_paths[wi] else { continue };
                // One tag per hole, both signatures in the line's own direction. Only on a stretch
                // this hole has to itself: on a shared one it could belong to any of the holes on
                // it. No such spot, no tag (hovering the line says it).
                let short = |s: &Option<String>| s.as_deref().map(|s| s.chars().take(3).collect::<String>());
                let text = match (short(&w.signature), short(&w.dest_signature)) {
                    (None, None) => continue,
                    (a, b) => format!("{}>{}", a.as_deref().unwrap_or("?"), b.as_deref().unwrap_or("?")),
                };
                let g = painter.layout_no_wrap(text, sig_font.clone(), visuals.text_color());
                let size = (g.size() + pad) / zoom;
                let Some(r) = first_free_strict(walk_all(path, true, size), size, &taken, &boxes, &others(wi)) else { continue };
                taken.push(r);
                draw_label(r, g, None);
            }
        }

        let mut clicked: Option<i64> = None;
        let mut opened: Option<i64> = None;
        let mut pin: Option<String> = None;
        let mut drop: Option<(i64, egui::Pos2)> = None;
        let mut ids: Vec<i64> = pos.keys().copied().collect();
        ids.sort_unstable();
        let mut chip_rows: Vec<(egui::Rect, Vec<(String, egui::Color32)>)> = Vec::new();
        for id in ids {
            let p = pos[&id];
            let (r, line1, chips) = rects[&id].clone();
            if !rect.intersects(r) {
                continue;
            }
            if let Some(&(pin, n)) = pills.get(&id) {
                let Some(info) = geo.info_of(pin) else { continue };
                // Only the part on the canvas: the rest is under a side panel, whose clicks are its own.
                let resp = ui.interact(r.intersect(rect), ui.id().with(("wh_pill", id)), egui::Sense::click_and_drag());
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
                    clicked = Some(pin);
                }
                let color = close_color(n).unwrap_or(visuals.widgets.noninteractive.bg_stroke.color);
                painter.rect(r, r.height() / 2.0, visuals.panel_fill, egui::Stroke::new(2.0, color), egui::StrokeKind::Inside);
                let text = if n == u32::MAX { display_name(pin, &info.name) } else { format!("{}  {n}j", display_name(pin, &info.name)) };
                let pill_font = egui::FontId::proportional((13.0 * zoom).clamp(10.0, 22.0));
                let mut job = egui::text::LayoutJob::simple_singleline(format!("{} {text}", icon::PUSH_PIN), pill_font, visuals.text_color());
                job.wrap = egui::text::TextWrapping {
                    max_width: (r.width() - r.height()).max(1.0),
                    max_rows: 1,
                    break_anywhere: true,
                    overflow_character: Some('\u{2026}'),
                };
                let g = painter.layout_job(job);
                painter.with_clip_rect(r.shrink(2.0).intersect(rect)).galley(r.center() - g.size() / 2.0, g, visuals.text_color());
                resp.on_hover_text(if n == u32::MAX {
                    format!("{}: pinned, no cluster within {NEAR_JUMPS} jumps", info.name)
                } else {
                    format!("{}: pinned, {n} jumps by gate and bridge from this cluster's nearest exit", info.name)
                });
                continue;
            }
            let Some(info) = geo.info_of(id) else { continue };
            let c = whdata::class_of(id, info.security, &info.region);
            let resp = ui.interact(r.intersect(rect), ui.id().with(("wh_node", id)), egui::Sense::click_and_drag());
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
                self.wh_disable_menu(ui, id);
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
            // A drifter system is washed in the drifter colour; its border still shows its effect.
            let fill = if matches!(c, Class::Drifter(_)) { drifter_fill(visuals.panel_fill) } else { visuals.panel_fill };
            painter.rect(r, 5.0 * zoom, fill, border, egui::StrokeKind::Inside);
            if !chips.is_empty() {
                chip_rows.push((r, chips));
            }
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
                    // The class is already the tag in front of the name.
                    if statics.is_empty() { effect.unwrap_or("").to_string() } else { statics.join(" ") }
                }
                _ => info.region.clone(),
            };
            // A drifter system's J-code, since its box shows it by name.
            let line2 = if whdata::DRIFTERS.iter().any(|d| d.2 == id) {
                if line2.is_empty() { info.name.clone() } else { format!("{} \u{b7} {line2}", info.name) }
            } else {
                line2
            };
            let g2 = painter.layout(line2, font.clone(), visuals.weak_text_color(), (node_size(id).x - 16.0) * zoom);
            painter.galley(r.min + egui::vec2(8.0 * zoom, (27.0 * zoom).max(5.0 * zoom + line1_h + 1.0)), g2, visuals.weak_text_color());
            if resp.hovered() && !resp.dragged() {
                let mut tip = format!("{} ({})", info.name, c.label());
                if let Some(e) = effect {
                    tip.push_str(&format!("\n{e}"));
                }
                if let Some(who) = here.get(&id) {
                    tip.push_str(&format!("\nHere: {}", who.join(", ")));
                }
                if jsys.is_some_and(|j| j.shattered()) {
                    tip.push_str("\nShattered");
                }
                if let Some(n) = hidden.get(&id) {
                    tip.push_str(&format!("\n{n} more hole{} to k-space leading nowhere pinned: double-click to see them", if *n == 1 { "" } else { "s" }));
                }
                if off_systems.contains(&id) {
                    tip.push_str("\nIts holes are switched off for routes");
                }
                tip.push_str("\nClick to select, double-click to focus, drag to move");
                resp.on_hover_text(tip);
            }
        }
        // Over every box, so a neighbour drawn later never covers them.
        let chip_h = painter.layout_no_wrap("+0".into(), chip_font.clone(), visuals.text_color()).size().y + 2.0;
        for (r, chips) in &chip_rows {
            // Inside the box on its right: centred when the box is small, else on its first line.
            let y = if detail { r.top() + 4.0 * zoom.max(1.0) + chip_h / 2.0 } else { r.center().y };
            {
                let mut x = r.right() - 4.0;
                for (text, color) in chips {
                    let g = painter.layout_no_wrap(text.clone(), chip_font.clone(), *color);
                    let size = g.size() + chip_pad;
                    let cr = egui::Rect::from_min_size(egui::pos2(x - size.x, y - size.y / 2.0), size);
                    painter.rect(cr, 4.0, visuals.extreme_bg_color, egui::Stroke::new(1.0, *color), egui::StrokeKind::Inside);
                    painter.galley(cr.center() - g.size() / 2.0, g, *color);
                    x = cr.left() - chip_gap;
                }
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
                tip.push_str(&format!("\nMass: {}", m.short()));
            }
            if let Some((life, _)) = life_badge(w, now, ui.visuals()) {
                tip.push_str(&format!("\nLife: {life}"));
            }
            tip.push_str(&format!("\nAdded {} ago", super::human_ago(now - w.reported_at)));
            tip.push_str(&format!("\nSource: {}", w.source.label()));
            if let Some(name) = self.wh_group_of.get(&w.uid).and_then(|g| self.share_group_name(g)) {
                tip.push_str(&format!("\nShared in {name}"));
            }
            if self.wh_disabled(w) {
                tip.push_str("\nSwitched off for routes");
            } else if blocked.contains(&w.id) {
                tip.push_str("\nOff routes: the filter hides it");
            }
            line_tip(ui, pointer, tip);
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
        if self.settings.wh_minimap && !pos.is_empty() {
            self.wh_graph_minimap(ui, rect, &pos, &geo);
        }
        if let Some(id) = opened {
            self.open_system(id);
        }
        if let Some(name) = pin {
            self.toggle_wh_pin(&name);
        }
    }

    /// The whole map in small in the canvas corner, with the part on screen outlined. Clicking or
    /// dragging in it moves the view there.
    fn wh_graph_minimap(&mut self, ui: &mut egui::Ui, rect: egui::Rect, pos: &HashMap<i64, egui::Pos2>, geo: &crate::geo::Systems) {
        const MAX: egui::Vec2 = egui::vec2(200.0, 130.0);
        let zoom = self.wh_graph.zoom;
        let seen = egui::Rect::from_min_size((-self.wh_graph.pan / zoom).to_pos2(), rect.size() / zoom);
        let Some(world) = canvas_of(pos) else { return };
        // Nothing off screen to find: it would only cover boxes.
        if seen.expand(4.0 / zoom).contains_rect(world.shrink(CANVAS_MARGIN)) {
            return;
        }
        let scale = (MAX.x / world.width()).min(MAX.y / world.height());
        let size = world.size() * scale;
        let mini = egui::Rect::from_min_size(rect.right_bottom() - size - egui::vec2(12.0, 12.0), size);
        if size.x > rect.width() * 0.5 || size.y > rect.height() * 0.5 {
            return;
        }
        let resp = ui.interact(mini, ui.id().with("wh_minimap"), egui::Sense::click_and_drag());
        let to_mini = |p: egui::Pos2| mini.min + (p - world.min) * scale;
        let painter = ui.painter_at(mini.expand(2.0));
        let visuals = ui.visuals();
        painter.rect(mini, 4.0, visuals.extreme_bg_color.gamma_multiply(0.92), visuals.widgets.noninteractive.bg_stroke, egui::StrokeKind::Inside);
        for (id, p) in pos {
            let r = egui::Rect::from_min_max(to_mini(*p), to_mini(*p + node_size(*id)));
            let col = geo
                .info_of(*id)
                .map(|i| class_color(whdata::class_of(*id, i.security, &i.region), i.security))
                .unwrap_or(visuals.weak_text_color());
            painter.rect_filled(r, 1.0, col.gamma_multiply(0.8));
        }
        let view = egui::Rect::from_min_max(to_mini(seen.min), to_mini(seen.max)).intersect(mini);
        painter.rect(view, 2.0, visuals.selection.bg_fill.gamma_multiply(0.2), egui::Stroke::new(1.5, visuals.selection.stroke.color), egui::StrokeKind::Inside);
        if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on()) {
            let target = world.min + (p - mini.min) / scale;
            self.wh_graph.pan = rect.center() - rect.min - target.to_vec2() * zoom;
            ui.ctx().request_repaint();
        }
        resp.on_hover_text("Click or drag to move the view");
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
        let mut open_sys: Option<i64> = None;
        // Where our online characters are, to open their system and fill in its holes and
        // signatures, whether the map draws it or not.
        let mut chars: Vec<(String, i64)> = self.player.lock().unwrap().locations.iter().map(|(n, (s, _))| (n.clone(), *s)).collect();
        chars.sort();
        let disabled = self.wh_disabled_count();
        let mut clear = false;
        egui::Panel::left("wh_graph_list").resizable(true).default_size(150.0).show_inside(ui, |ui| {
            if disabled > 0 {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("{} {disabled} off for routes", icon::PROHIBIT)).weak());
                    clear = ui.button("Allow all").on_hover_text("Let routes use every hole and system switched off").clicked();
                });
                ui.separator();
            }
            if !chars.is_empty() {
                ui.label(egui::RichText::new("Characters").weak());
                egui::Grid::new("wh_graph_chars_grid").spacing([6.0, 2.0]).show(ui, |ui| {
                    for (name, sys) in &chars {
                        let Some(i) = geo.info_of(*sys) else { continue };
                        let c = whdata::class_of(*sys, i.security, &i.region);
                        ui.label(egui::RichText::new(system_tag(c, i.security)).color(class_color(c, i.security)));
                        let on = self.wh_graph.selected == Some(*sys);
                        if ui.menu_label(on, display_name(*sys, &i.name)).on_hover_text(format!("{name} is here: open its holes and signatures")).clicked() {
                            open_sys = Some(*sys);
                        }
                        ui.label(egui::RichText::new(name).weak());
                        ui.end_row();
                    }
                });
                ui.separator();
            }
            ui.label(egui::RichText::new(format!("{} system{}", rows.len(), if rows.len() == 1 { "" } else { "s" })).weak());
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("wh_graph_list_grid").spacing([6.0, 2.0]).show(ui, |ui| {
                    for (c, info, n) in &rows {
                        let tag = system_tag(*c, info.security);
                        ui.label(egui::RichText::new(tag).color(class_color(*c, info.security)));
                        let on = self.wh_graph.focus == Some(info.id);
                        let effect = whdata::jsystem(info.id).and_then(|j| j.effect.clone());
                        let r = ui.menu_label(on, display_name(info.id, &info.name));
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
        if let Some(id) = open_sys {
            self.wh_graph.selected = Some(id);
        }
        if clear {
            self.clear_wh_disabled();
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
        let mut new_hole: Option<String> = None;
        let mut toggle: Option<String> = None;
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
                        let (sig, far, far_sig) = if w.system_id == sel {
                            (&w.signature, w.dest_system_id, &w.dest_signature)
                        } else {
                            (&w.dest_signature, Some(w.system_id), &w.signature)
                        };
                        let ty = hole_code(w);
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
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if let Some((text, color)) = life_badge(w, now, ui.visuals()) {
                                let read = w.observed_at.map(|t| format!(", read {} ago", super::human_ago(now - t))).unwrap_or_default();
                                ui.label(egui::RichText::new(text).color(color))
                                    .on_hover_text(format!("Time left{read}"));
                            }
                            if let Some(m) = w.mass {
                                ui.label(egui::RichText::new(m.short()).color(mass_color(Some(m)))).on_hover_text(format!("Mass: {}", m.label()));
                            }
                        });
                        ui.label(egui::RichText::new(format!("{} ago", super::human_ago(now - w.reported_at))).weak())
                            .on_hover_text(format!("Added {} ago, from {}", super::human_ago(now - w.reported_at), w.source.label()));
                        ui.horizontal(|ui| {
                            if ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Edit this hole").clicked() {
                                edit = Some(w.id);
                            }
                            if ui.small_button(icon::X).on_hover_text("Mark this hole dead").clicked() {
                                kill = Some(w.id);
                            }
                            if wh_route_toggle(ui, self.settings.wh_disabled_holes.contains(&w.uid)) {
                                toggle = Some(w.uid.clone());
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
                    let mut q = std::mem::take(&mut self.wh_graph.pin_query);
                    let picked = self.system_input(ui, "wh_route_pin", &mut q, "Add a system", 160.0);
                    self.wh_graph.pin_query = q;
                    let add = picked.is_some() || ui.button(icon::PLUS).on_hover_text("Measure routes to this system").clicked();
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
                // A table, not a grid: the Info column takes whatever width the panel has left.
                let row_h = ui.spacing().interact_size.y + 4.0;
                egui_extras::TableBuilder::new(ui)
                    .id_salt("wh_graph_scan")
                    .striped(true)
                    .vscroll(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(egui_extras::Column::auto())
                    .column(egui_extras::Column::auto())
                    .column(egui_extras::Column::remainder().clip(true))
                    .column(egui_extras::Column::auto())
                    .column(egui_extras::Column::auto())
                    .header(row_h, |mut header| {
                        for h in ["Id", "Group", "Info", "Added", ""] {
                            header.col(|ui| {
                                ui.label(egui::RichText::new(h).strong());
                            });
                        }
                    })
                    .body(|mut body| {
                    for sg in &sigs {
                        body.row(row_h, |mut row| {
                        let anomaly = sg.kind.to_lowercase().contains("anomal");
                        row.col(|ui| {
                            let id = ui.label(if anomaly { egui::RichText::new(&sg.sig).weak() } else { egui::RichText::new(&sg.sig) });
                            id.on_hover_text(&sg.kind);
                        });
                        row.col(|ui| {
                            ui.label(short_group(&sg.group));
                        });
                        // A wormhole signature we know the far side of says where it goes.
                        let hole = sig_hole(holes, sel, &sg.sig);
                        row.col(|ui| {
                        match hole.map(|w| if w.system_id == sel { w.dest_system_id } else { Some(w.system_id) }) {
                            Some(Some(f)) => {
                                let code = hole.and_then(hole_code).map(|c| format!("{c} ")).unwrap_or_default();
                                if ui.link(format!("{code}{} {}", icon::ARROW_RIGHT, name(f))).clicked() {
                                    select = Some(f);
                                }
                            }
                            _ => {
                                let text = match unidentified_type(sel, &sg.name) {
                                    Some(code) => format!("{code} \u{b7} {}", sg.name),
                                    None if sg.name.is_empty() => "\u{2014}".to_owned(),
                                    None => sg.name.clone(),
                                };
                                ui.add(egui::Label::new(&text).truncate());
                            }
                        }
                        });
                        row.col(|ui| {
                            ui.label(super::human_ago(now - sg.added_at)).on_hover_text(format!(
                                "Added by {}, last seen in a paste {} ago",
                                sg.who,
                                super::human_ago(now - sg.updated_at)
                            ));
                        });
                        row.col(|ui| {
                            // Remove first, so those line up whether or not an edit button follows.
                            if ui.small_button(icon::X).on_hover_text("Remove").clicked() {
                                drop_sig = Some(sg.sig.clone());
                            }
                            let is_hole = hole.is_some() || sg.group == "Wormhole";
                            if is_hole && ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Edit this wormhole").clicked() {
                                match hole {
                                    Some(w) => edit = Some(w.id),
                                    None => new_hole = Some(sg.sig.clone()),
                                }
                            }
                        });
                        });
                    }
                });
                    }
                }
            });
        });
        if let Some(text) = paste {
            self.wh_graph_paste(sel, text, now);
        }
        if let Some(sig) = new_hole {
            let wh_type = self
                .wh_graph
                .sigs
                .as_ref()
                .and_then(|(_, l)| l.iter().find(|s| s.sig == sig))
                .and_then(|s| unidentified_type(sel, &s.name));
            self.wh_form = Some(crate::app::wormholes_ui::WhForm::at(info.name.clone(), sig, wh_type));
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
        if let Some(uid) = toggle {
            self.toggle_wh_hole(&uid);
        }
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
        let full = !self.wh_graph.keep_missing;
        let (added, updated, removed) = store.merge_system_sigs(system, &scan, &who, now, full);
        let linked = self.wh_probe_followup(system, &scan, full, &who);
        self.wh_graph.sigs = None;
        self.wh_graph_sigs(system);
        self.wh_graph.sig_note = Some(format!("{added} new, {updated} updated, {removed} removed{linked}"));
    }

    /// Carries a saved probe scan over to the holes: a lone unclaimed wormhole signature goes on
    /// the lone hole there without one, and holes whose signature is gone wait on the user.
    /// Returns what was linked, for the note.
    fn wh_probe_followup(&mut self, system: i64, scan: &[crate::wormholes::ScanSig], full: bool, who: &str) -> String {
        let Some(store) = self.store.as_ref() else { return String::new() };
        let now = chrono::Utc::now().timestamp();
        let holes: Vec<Wormhole> = store.wormholes().into_iter().filter(|w| !w.is_expired(now)).collect();
        let (gone, fill) = probe_effects(&holes, system, scan, full);
        let mut note = String::new();
        if let Some((id, sig, near)) = fill {
            if let Some(mut w) = store.wormhole_by_id(id) {
                if near {
                    w.signature = Some(sig.clone());
                } else {
                    w.dest_signature = Some(sig.clone());
                }
                w.updated_at = now;
                store.write_wormhole(&w);
                store.audit_wormhole(&w.uid, who, crate::wormholes::Source::Manual, &[("signature", sig.clone())]);
                note = format!(", {sig} put on its hole");
                self.wh_reloaded = None;
            }
        }
        if !gone.is_empty() {
            self.wh_graph.gone = Some((system, gone.into_iter().map(|id| (id, true)).collect()));
        }
        note
    }

    /// `holes` saved, and the ones listed after `system` waiting on the gone-from-the-scan prompt.
    #[cfg(test)]
    pub(crate) fn seed_gone(&mut self, system: i64, holes: Vec<Wormhole>) {
        let Some(store) = self.store.as_ref() else { return };
        let ids: Vec<(i64, bool)> = holes.iter().map(|w| (store.upsert_wormhole(w), true)).collect();
        self.wh_graph.gone = Some((system, ids));
    }

    /// Asks before marking dead the holes a probe scan no longer lists.
    pub(crate) fn wh_gone_window(&mut self, ctx: &egui::Context) {
        let Some((system, mut list)) = self.wh_graph.gone.take() else { return };
        let geo = self.systems.clone();
        let name = |id: i64| geo.as_ref().and_then(|g| g.info_of(id)).map_or(format!("#{id}"), |i| display_name(id, &i.name));
        let holes: HashMap<i64, Wormhole> = self.store.as_ref().map(|s| list.iter().filter_map(|(id, _)| Some((*id, s.wormhole_by_id(*id)?))).collect()).unwrap_or_default();
        list.retain(|(id, _)| holes.contains_key(id));
        if list.is_empty() {
            return;
        }
        let mut act: Option<bool> = None;
        egui::Window::new("Holes gone from the scan")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "The probe scan of {} no longer lists the signature of {}. A hole's signature goes when it collapses.",
                    name(system),
                    if list.len() == 1 { "this hole" } else { "these holes" }
                ));
                ui.add_space(4.0);
                for (id, keep) in list.iter_mut() {
                    let w = &holes[id];
                    let (sig, far) = if w.system_id == system { (&w.signature, w.dest_system_id) } else { (&w.dest_signature, Some(w.system_id)) };
                    let far = far.map_or_else(|| w.dest.label().to_owned(), name);
                    let ty = hole_code(w).map(|t| format!(" ({t})")).unwrap_or_default();
                    ui.checkbox(keep, format!("{} \u{2192} {far}{ty}", sig.as_deref().unwrap_or("?")));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Mark dead").clicked() {
                        act = Some(true);
                    }
                    if ui.button("Keep them").clicked() {
                        act = Some(false);
                    }
                });
            });
        match act {
            Some(true) => {
                let who = {
                    let p = self.player.lock().unwrap();
                    if p.active_name.is_empty() { "me".to_owned() } else { p.active_name.clone() }
                };
                if let Some(store) = self.store.as_ref() {
                    for (id, _) in list.iter().filter(|(_, go)| *go) {
                        if let Some(w) = holes.get(id) {
                            store.kill_wormhole(*id);
                            store.audit_wormhole(&w.uid, &who, crate::wormholes::Source::Manual, &[("dead", "signature gone from a probe scan".to_owned())]);
                        }
                    }
                }
                self.wh_reloaded = None;
            }
            Some(false) => {}
            None => self.wh_graph.gone = Some((system, list)),
        }
    }

    fn wh_layout_opts(&self) -> super::wh_layout::Opts {
        // The aspect in steps of a tenth, so resizing the window by a few pixels keeps the layout.
        let aspect = self.wh_graph.canvas.map(|r| (r.aspect_ratio() * 10.0).round() / 10.0).filter(|a| a.is_finite() && *a > 0.0).unwrap_or(1.6);
        super::wh_layout::Opts {
            style: super::wh_layout::Style::from_code(&self.settings.wh_layout_style),
            aspect: self.settings.wh_layout_pack.then_some(aspect),
        }
    }

    /// Lays the whole map out afresh and shows all of it.
    pub(crate) fn wh_graph_tidy(&mut self) {
        self.wh_graph_reset_layout();
        self.wh_graph.focus_dragged.clear();
        self.wh_graph.layout_cache = None;
        self.wh_graph.fit_pending = true;
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
    use crate::wormholes::Life;

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
    fn a_cluster_that_turns_up_later_goes_below_the_map_not_into_it() {
        let first = auto_layout(&[(1, 2), (1, 3)], |n| if n == 1 { 100 } else { 0 });
        let placed = place(&first, &HashMap::new());
        let later = auto_layout(&[(1, 2), (1, 3), (7, 8)], |n| if n == 1 { 100 } else { 0 });
        let now = place(&later, &placed);
        let bottom = [1, 2, 3].iter().map(|n| placed[n].y + NODE.y).fold(f32::MIN, f32::max);
        assert!(now[&7].y >= bottom && now[&8].y >= bottom, "{:?} {:?} above {bottom}", now[&7], now[&8]);
        assert_eq!(now[&8] - now[&7], later.iter().find(|x| x.0 == 8).unwrap().2 - later.iter().find(|x| x.0 == 7).unwrap().2, "its own shape kept");
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
    fn routes_run_at_right_angles_and_fan_out_side_by_side() {
        let parent = egui::Rect::from_min_size(egui::pos2(0.0, 100.0), NODE);
        let up = egui::Rect::from_min_size(egui::pos2(COL, 0.0), NODE);
        let down = egui::Rect::from_min_size(egui::pos2(COL, 200.0), NODE);
        let boxes = HashMap::from([(1, parent), (2, up), (3, down)]);
        let r = route_all(&boxes, &[(1, 2, true), (1, 3, true)], &HashMap::new());
        for p in r.iter().flatten() {
            assert!(p.windows(2).all(|s| (s[0].x - s[1].x).abs() < 0.5 || (s[0].y - s[1].y).abs() < 0.5), "{p:?}");
            assert_eq!(p[0].x, NODE.x, "out of the parent's side");
        }
        let (a, b) = (r[0].clone().unwrap(), r[1].clone().unwrap());
        let mid = 100.0 + NODE.y / 2.0;
        assert_eq!((a[0].y, b[0].y), (mid - LANE / 2.0, mid + LANE / 2.0), "a lane each, the one going up on top");
        let shared: f32 = a.windows(2).flat_map(|s| b.windows(2).map(move |t| overlap(s[0], s[1], t[0], t[1]))).sum();
        assert!(shared < 1.0, "{a:?} {b:?}");
        assert!(!a.windows(2).any(|s| b.windows(2).any(|t| crosses(s[0], s[1], t[0], t[1]))), "{a:?} {b:?}");
    }

    #[test]
    fn many_holes_out_of_one_system_keep_apart_and_never_cross() {
        let parent = egui::Rect::from_min_size(egui::pos2(0.0, 300.0), NODE);
        let mut boxes = HashMap::from([(0, parent)]);
        let mut links = Vec::new();
        for k in 1..=6i64 {
            boxes.insert(k, egui::Rect::from_min_size(egui::pos2(COL, (k - 1) as f32 * 120.0), NODE));
            links.push((0, k, true));
        }
        let r: Vec<Vec<egui::Pos2>> = route_all(&boxes, &links, &HashMap::new()).into_iter().map(Option::unwrap).collect();
        for (i, a) in r.iter().enumerate() {
            for b in &r[i + 1..] {
                let shared: f32 = a.windows(2).flat_map(|s| b.windows(2).map(move |t| overlap(s[0], s[1], t[0], t[1]))).sum();
                assert!(shared < 1.0, "{a:?} {b:?}");
                assert!(!a.windows(2).any(|s| b.windows(2).any(|t| crosses(s[0], s[1], t[0], t[1]))), "{a:?} {b:?}");
            }
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
    fn a_new_system_leaves_the_placed_ones_where_they_are() {
        let first = auto_layout(&[(1, 2), (1, 3)], |n| if n == 1 { 100 } else { 0 });
        let placed = place(&first, &HashMap::new());
        // A fourth system joins under 1: the auto layout alone would shuffle 2 and 3 to fit it.
        let second = auto_layout(&[(1, 2), (1, 3), (1, 4)], |n| if n == 1 { 100 } else { 0 });
        let now = place(&second, &placed);
        for id in [1, 2, 3] {
            assert_eq!(now[&id], placed[&id], "system {id} moved");
        }
        let clash = [1, 2, 3].iter().any(|id| {
            let q = now[id];
            (q.x - now[&4].x).abs() < NODE.x + 10.0 && (q.y - now[&4].y).abs() < NODE.y + 10.0
        });
        assert!(!clash, "the new one found its own room: {:?}", now[&4]);
        assert_eq!(now[&4].x, COL, "beside its parent's other holes");
    }

    #[test]
    fn a_drag_reroutes_only_what_it_moves_and_letting_go_routes_everything() {
        let auto = auto_layout(&[(1, 2), (1, 3), (3, 4), (5, 6)], |n| if n == 1 { 100 } else { 0 });
        let boxes: HashMap<i64, egui::Rect> = auto.iter().map(|(n, _, p)| (*n, egui::Rect::from_min_size(*p, NODE))).collect();
        let links = vec![(1, 2, true), (1, 3, true), (3, 4, true), (5, 6, true)];
        let parent: HashMap<i64, i64> = auto.iter().filter_map(|(n, p, _)| Some((*n, (*p)?))).collect();
        let mut view = WhGraphView::default();
        let before = view.routes(&boxes, &links, &parent);
        let mut moved = boxes.clone();
        moved.insert(4, boxes[&4].translate(egui::vec2(0.0, 300.0)));
        view.drag = Some((4, egui::Pos2::ZERO));
        let during = view.routes(&moved, &links, &parent);
        assert_eq!(during[3], before[3], "a line far from the drag is left alone");
        assert_ne!(during[2], before[2], "the dragged system's own line follows it");
        view.drag = None;
        assert_eq!(view.routes(&moved, &links, &parent), route_all(&moved, &links, &parent), "letting go routes everything afresh");
    }

    /// How long routing takes on a map the size of a busy one: ~110 systems, ~100 holes.
    #[test]
    #[ignore]
    fn bench_route_all() {
        let mut edges = Vec::new();
        for c in 0..10i64 {
            let base = 31_000_100 + c * 20;
            for k in 1..11 {
                edges.push((base + (k - 1) / 2, base + k));
            }
        }
        let auto = super::super::wh_layout::layout(&edges, &[], |_| 0, super::super::wh_layout::Opts::default());
        let boxes: HashMap<i64, egui::Rect> = auto.iter().map(|(n, _, p)| (*n, egui::Rect::from_min_size(*p, node_size(*n)))).collect();
        let links: Vec<(i64, i64, bool)> = edges.iter().map(|(a, b)| (*a, *b, true)).collect();
        let parent: HashMap<i64, i64> = auto.iter().filter_map(|(n, p, _)| Some((*n, (*p)?))).collect();
        let t = std::time::Instant::now();
        let r = route_all(&boxes, &links, &parent);
        eprintln!("BENCH route_all {} links, {} boxes: {:?}", links.len(), boxes.len(), t.elapsed());
        assert_eq!(r.len(), links.len());
        // A drag frame: one box moved, its lines routed again around the rest.
        let mut view = WhGraphView::default();
        view.routes(&boxes, &links, &parent);
        view.drag = Some((31_000_105, egui::Pos2::ZERO));
        let mut moved = boxes.clone();
        let r5 = moved[&31_000_105];
        moved.insert(31_000_105, r5.translate(egui::vec2(40.0, 30.0)));
        let t = std::time::Instant::now();
        let d = view.routes(&moved, &links, &parent);
        eprintln!("BENCH drag frame: {:?}", t.elapsed());
        assert!(links.iter().zip(&d).filter(|((a, b, _), _)| *a == 31_000_105 || *b == 31_000_105).all(|(_, p)| p.is_some()));
    }

    #[test]
    fn the_overview_counts_exits_that_lead_nowhere_instead_of_drawing_them() {
        let hole = |a: i64, b: i64| Wormhole { system_id: a, dest_system_id: Some(b), ..Default::default() };
        let (thera, j, jita, amamake, pochven) = (31_000_005, 31_000_100, 30_000_142, 30_002_537, 30_000_021);
        let kspace = |id: i64| (30_000_000..31_000_000).contains(&id);
        let mut holes = vec![
            hole(thera, jita),
            hole(thera, amamake),
            hole(j, jita),
            hole(jita, amamake),
            hole(pochven, jita),
            Wormhole { system_id: j, dest_system_id: None, ..Default::default() },
        ];
        let keep = HashSet::from([amamake]);
        let hidden = overview(&mut holes, kspace, &keep);
        let left: Vec<(i64, Option<i64>)> = holes.iter().map(|w| (w.system_id, w.dest_system_id)).collect();
        // Jita is drawn anyway, for its k-space holes, so its holes into wormhole space stay too.
        assert_eq!(left.len(), 6, "{left:?}");
        assert!(hidden.is_empty());
        // Without those, Jita leads nowhere pinned: counted on the wormhole side instead.
        let mut holes = vec![hole(thera, jita), hole(thera, amamake), hole(j, jita)];
        let hidden = overview(&mut holes, kspace, &keep);
        let left: Vec<(i64, Option<i64>)> = holes.iter().map(|w| (w.system_id, w.dest_system_id)).collect();
        assert_eq!(left, vec![(thera, Some(amamake))]);
        assert_eq!(hidden, HashMap::from([(thera, 1), (j, 1)]));
        let _ = pochven;
    }

    #[test]
    fn the_dialogs_offer_only_free_wormhole_signatures() {
        use crate::app::wormholes_ui::offerable;
        let sig = |id: &str, kind: &str, group: &str| crate::store::SystemSig {
            sig: id.into(),
            kind: kind.into(),
            group: group.into(),
            name: String::new(),
            added_at: 0,
            updated_at: 0,
            who: String::new(),
        };
        let here = 31_000_004;
        let holes = [
            Wormhole { id: 1, system_id: here, signature: Some("ABC-123".into()), ..Default::default() },
            Wormhole { id: 2, system_id: 30_000_224, dest_system_id: Some(here), dest_signature: Some("XYZ".into()), ..Default::default() },
        ];
        let sig_kind = "Cosmic Signature";
        assert!(offerable(&sig("NEW-001", sig_kind, "Wormhole"), here, &holes, None));
        assert!(offerable(&sig("UNS-002", sig_kind, ""), here, &holes, None), "not scanned yet: could be one");
        assert!(!offerable(&sig("DAT-003", sig_kind, "Data Site"), here, &holes, None), "scanned as something else");
        assert!(!offerable(&sig("ANO-004", "Cosmic Anomaly", "Combat Site"), here, &holes, None));
        assert!(!offerable(&sig("ABC-123", sig_kind, "Wormhole"), here, &holes, None), "another hole's, on its own side");
        assert!(!offerable(&sig("XYZ-999", sig_kind, "Wormhole"), here, &holes, None), "another hole's, on the far side");
        assert!(offerable(&sig("ABC-123", sig_kind, "Wormhole"), here, &holes, Some(1)), "the hole being edited keeps its own");
    }

    #[test]
    fn a_probe_scan_finds_gone_holes_and_names_a_lone_bare_one() {
        use crate::wormholes::ScanSig;
        let sig = |id: &str, group: &str| ScanSig { id: id.into(), kind: "Cosmic Signature".into(), group: group.into(), name: String::new() };
        let hole = |id: i64, from: i64, sig: Option<&str>, to: i64, far: Option<&str>| Wormhole {
            id,
            system_id: from,
            signature: sig.map(Into::into),
            dest_system_id: Some(to),
            dest_signature: far.map(Into::into),
            ..Default::default()
        };
        let here = 31_000_004;
        let holes = [
            hole(1, here, Some("ABC-123"), 30_000_142, None),
            hole(2, 30_004_759, Some("QQQ"), here, Some("XYZ-999")),
            hole(3, here, None, 30_003_704, None),
            hole(4, 30_000_001, Some("NOT"), 30_000_002, None),
        ];
        let scan = [sig("ABC-123", "Wormhole"), sig("NEW-001", "Wormhole"), sig("DAT-555", "Data Site")];
        let (gone, fill) = probe_effects(&holes, here, &scan, true);
        assert_eq!(gone, vec![2], "XYZ is gone from here; a hole elsewhere is not this scan's business");
        assert_eq!(fill, Some((3, "NEW-001".into(), true)));
        // A paste that keeps what it does not list says nothing about what is gone.
        assert!(probe_effects(&holes, here, &scan, false).0.is_empty());
        // Two unclaimed wormholes: which one is the hole's is a guess, so neither.
        let two = [sig("ABC-123", "Wormhole"), sig("NEW-001", "Wormhole"), sig("NEW-002", "Wormhole")];
        assert_eq!(probe_effects(&holes, here, &two, true).1, None);
        // Anomalies only: nothing about signatures, so nothing gone.
        let anoms = [ScanSig { id: "ANO-100".into(), kind: "Cosmic Anomaly".into(), group: "Combat Site".into(), name: String::new() }];
        assert!(probe_effects(&holes, here, &anoms, true).0.is_empty());
    }

    #[test]
    fn a_holes_line_pattern_follows_its_time_left() {
        let now = 1_000_000;
        let hole = |life: Option<Life>, expiry: i64| Wormhole { life, explicit_expiry: Some(now + expiry), ..Default::default() };
        assert_eq!(time_left(&hole(None, 13 * 3600), now), TimeLeft::Plenty);
        assert_eq!(time_left(&hole(None, 10 * 3600), now), TimeLeft::Under12h);
        assert_eq!(time_left(&hole(None, 3 * 3600), now), TimeLeft::Under4h);
        assert_eq!(time_left(&hole(None, 1800), now), TimeLeft::Under1h);
        assert_eq!(time_left(&hole(None, -60), now), TimeLeft::Expiring);
        assert_eq!(time_left(&hole(Some(Life::Expired), 10 * 3600), now), TimeLeft::Expiring, "the scout's word wins");
        assert_eq!(time_left(&hole(Some(Life::Under1h), 10 * 3600), now), TimeLeft::Under1h);
        // Read as less than a day: half of that gone, as likely closed as not, then on down.
        let day = |ago: i64| Wormhole { life: Some(Life::UnderDay), explicit_expiry: Some(now - ago + 86_400), ..Default::default() };
        assert_eq!(time_left(&day(6 * 3600), now), TimeLeft::Plenty);
        assert_eq!(time_left(&day(13 * 3600), now), TimeLeft::Under12h);
        assert_eq!(time_left(&day(21 * 3600), now), TimeLeft::Under4h);
        assert_eq!(time_left(&day(25 * 3600), now), TimeLeft::Expiring);
        // A reading of under four hours runs out too.
        let four = Wormhole { life: Some(Life::Under4h), explicit_expiry: Some(now - 60), ..Default::default() };
        assert_eq!(time_left(&four, now), TimeLeft::Expiring);
    }

    #[test]
    fn wormhole_systems_show_their_class_not_minus_one() {
        assert_eq!(system_tag(Class::W(4), -1.0), "C4");
        assert_eq!(system_tag(Class::Thera, -1.0), "T");
        assert_eq!(system_tag(Class::Drifter(15), -1.0), "B");
        assert_eq!(display_name(31_000_002, "J110145"), "Barbican");
        assert_eq!(system_tag(Class::Pochven, -1.0), "Poch");
        assert_eq!(system_tag(Class::Ls, 0.3), "0.3");
    }

    #[test]
    fn a_returning_system_does_not_land_on_the_one_that_took_its_spot() {
        let auto = auto_layout(&[(1, 2), (1, 3)], |n| if n == 1 { 100 } else { 0 });
        let first = place(&auto, &HashMap::new());
        // 3 remembered where 2 now is: one of them has to move.
        let remembered = HashMap::from([(1, first[&1]), (2, first[&2]), (3, first[&2])]);
        let at = place(&auto, &remembered);
        assert_ne!(at[&2], at[&3]);
    }

    #[test]
    fn a_switched_off_line_breaks_on_its_own_last_leg() {
        let line = [egui::pos2(0.0, 0.0), egui::pos2(40.0, 0.0), egui::pos2(40.0, 60.0)];
        let (a, b, mid, dir) = split_middle(&line, 10.0).unwrap();
        assert_eq!(a, vec![egui::pos2(0.0, 0.0), egui::pos2(40.0, 0.0), egui::pos2(40.0, 25.0)]);
        assert_eq!(b, vec![egui::pos2(40.0, 35.0), egui::pos2(40.0, 60.0)]);
        assert_eq!((mid, dir), (egui::pos2(40.0, 30.0), egui::Vec2::Y));
        let short_end = [egui::pos2(0.0, 0.0), egui::pos2(60.0, 0.0), egui::pos2(60.0, 10.0)];
        assert_eq!(split_middle(&short_end, 10.0).unwrap().2, egui::pos2(35.0, 0.0), "the middle of the whole line");
        assert!(split_middle(&[egui::pos2(0.0, 0.0), egui::pos2(20.0, 0.0)], 10.0).is_none());
    }

    #[test]
    fn a_zigzag_keeps_its_ends_and_swings_to_both_sides() {
        let line = [egui::pos2(0.0, 0.0), egui::pos2(100.0, 0.0)];
        let z = zigzag(&line, 5.0, 3.0);
        assert_eq!((z[0], *z.last().unwrap()), (line[0], line[1]));
        assert!(z.iter().any(|p| p.y > 2.0) && z.iter().any(|p| p.y < -2.0));
    }

    #[test]
    fn a_tooltip_flips_away_from_the_screen_edges() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let size = egui::vec2(300.0, 120.0);
        assert_eq!(tip_pos(egui::pos2(100.0, 100.0), size, screen), egui::pos2(114.0, 114.0), "room: below right");
        let p = tip_pos(egui::pos2(900.0, 100.0), size, screen);
        assert!(p.x + size.x <= 900.0, "near the right edge it goes left of the pointer: {p:?}");
        let p = tip_pos(egui::pos2(100.0, 750.0), size, screen);
        assert!(p.y + size.y <= 750.0, "near the bottom it goes above: {p:?}");
    }

    #[test]
    fn drops_snap_to_the_grid() {
        assert_eq!(snap(egui::pos2(14.0, 26.0)), egui::pos2(10.0, 30.0));
    }
}

/// What the map's colours, lines and marks mean, each next to a small drawn example.
fn legend(ui: &mut egui::Ui) {
    use egui::Color32 as C;
    let v = ui.visuals().clone();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let row_h = ui.spacing().interact_size.y;
    enum Line {
        Solid(C),
        Mass(Option<Mass>),
        Off(C),
    }
    // A wrapping row decides where to break before it knows how wide a grouped item is, so each
    // item measures itself first and starts a new row when it would not fit.
    let room_for = |ui: &mut egui::Ui, sample: f32, text: &str| {
        let w = sample
            + ui.spacing().item_spacing.x * 2.0
            + ui.painter().layout_no_wrap(text.to_owned(), font.clone(), v.text_color()).size().x;
        if ui.max_rect().right() - ui.cursor().left() < w {
            ui.end_row();
        }
    };
    let line = |ui: &mut egui::Ui, kind: Line, text: &str| {
        room_for(ui, 40.0, text);
        ui.horizontal(|ui| {
            let (r, p) = ui.allocate_painter(egui::vec2(40.0, row_h), egui::Sense::hover());
            let (a, b) = (r.rect.left_center() + egui::vec2(2.0, 0.0), r.rect.right_center() - egui::vec2(2.0, 0.0));
            match kind {
                Line::Solid(c) => {
                    p.line_segment([a, b], egui::Stroke::new(2.5, c));
                }
                Line::Mass(m) => stroke_hole(&p, &[a, b], egui::Stroke::new(2.5, v.weak_text_color()), m),
                Line::Off(c) => slashed(&p, &[a, b], desaturate_stroke(egui::Stroke::new(2.5, c)), |p, l, s| stroke_hole(p, l, s, None)),
            }
            ui.label(text);
        });
    };
    let chip = |ui: &mut egui::Ui, text: &str, border: Option<C>, what: &str| {
        let g = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), v.text_color());
        room_for(ui, g.size().x + 10.0, what);
        ui.horizontal(|ui| {
            let (r, p) = ui.allocate_painter(g.size() + egui::vec2(10.0, 4.0), egui::Sense::hover());
            let rect = r.rect;
            match border {
                Some(c) => p.rect(rect, 3.0, v.extreme_bg_color, egui::Stroke::new(1.0, c), egui::StrokeKind::Inside),
                None => p.rect_filled(rect, 3.0, v.extreme_bg_color),
            };
            p.galley(rect.center() - g.size() / 2.0, g, v.text_color());
            ui.label(what);
        });
    };
    let boxed_fill = |ui: &mut egui::Ui, fill: C, stroke: egui::Stroke, what: &str| {
        room_for(ui, 34.0, what);
        ui.horizontal(|ui| {
            let (r, p) = ui.allocate_painter(egui::vec2(34.0, row_h), egui::Sense::hover());
            p.rect(r.rect.shrink2(egui::vec2(1.0, 3.0)), 4.0, fill, stroke, egui::StrokeKind::Inside);
            ui.label(what);
        });
    };
    let boxed = |ui: &mut egui::Ui, stroke: egui::Stroke, what: &str| boxed_fill(ui, v.panel_fill, stroke, what);
    let tag = |ui: &mut egui::Ui, text: &str, color: C| {
        room_for(ui, 0.0, text);
        ui.label(egui::RichText::new(text).color(color).strong());
    };
    let heading = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).strong());
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        heading(ui, "Holes");
        ui.horizontal_wrapped(|ui| {
            line(ui, Line::Solid(time_color(TimeLeft::Plenty)), "over 12 hours left");
            line(ui, Line::Solid(time_color(TimeLeft::Under12h)), "under 12 hours");
            line(ui, Line::Solid(time_color(TimeLeft::Under4h)), "under 4 hours");
            line(ui, Line::Solid(time_color(TimeLeft::Under1h)), "under 1 hour");
            line(ui, Line::Solid(time_color(TimeLeft::Expiring)), "could close any moment");
        });
        ui.horizontal_wrapped(|ui| {
            line(ui, Line::Mass(Some(Mass::Fresh)), "over 50% mass, or unknown");
            line(ui, Line::Mass(Some(Mass::Reduced)), "under 50%");
            line(ui, Line::Mass(Some(Mass::Critical)), "under 10%");
        });
        ui.horizontal_wrapped(|ui| {
            line(ui, Line::Off(time_color(TimeLeft::Plenty)), "off for routes (right-click a system)");
            chip(ui, "ABC>XYZ", None, "signatures, from the hole's own system to the far one");
        });
        // The chips on a box, drawn as the map draws them.
        let mark = |ui: &mut egui::Ui, text: &str, color: C, what: &str| {
            let g = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color);
            room_for(ui, g.size().x + 10.0, what);
            ui.horizontal(|ui| {
                let (r, p) = ui.allocate_painter(g.size() + egui::vec2(8.0, 2.0), egui::Sense::hover());
                p.rect(r.rect, 4.0, v.extreme_bg_color, egui::Stroke::new(1.0, color), egui::StrokeKind::Inside);
                p.galley(r.rect.center() - g.size() / 2.0, g, color);
                ui.label(what);
            });
        };
        ui.horizontal_wrapped(|ui| {
            mark(ui, "Amamake 7j", close_color(7).unwrap_or(v.text_color()), "a pinned system this many gate jumps away (green under 5)");
        });
        ui.horizontal_wrapped(|ui| {
            mark(ui, "+8", v.text_color(), "holes to k-space leading to nothing pinned, not drawn");
            mark(ui, icon::DIAMONDS_FOUR, SHATTERED_COLOR, "shattered");
            mark(ui, icon::PROHIBIT, v.weak_text_color(), "every hole here off for routes");
        });
        ui.add_space(4.0);
        heading(ui, "Systems");
        ui.horizontal_wrapped(|ui| {
            for (name, class) in [
                ("C1-C3", Class::W(1)),
                ("C4-C5", Class::W(4)),
                ("C6", Class::W(6)),
                ("C13", Class::W(13)),
                ("Thera", Class::Thera),
                ("Drifter", Class::Drifter(14)),
                ("Pochven", Class::Pochven),
            ] {
                tag(ui, name, class_color(class, -1.0));
            }
            ui.separator();
            for sec in [1.0, 0.5, 0.3, 0.0, -0.5] {
                tag(ui, &format!("{sec:.1}"), class_color(Class::Ns, sec));
            }
            room_for(ui, 0.0, "security");
            ui.label(egui::RichText::new("security").weak());
        });
        ui.horizontal_wrapped(|ui| {
            for effect in ["Magnetar", "Red Giant", "Pulsar", "Wolf-Rayet Star", "Cataclysmic Variable", "Black Hole"] {
                boxed(ui, egui::Stroke::new(1.5, effect_color(effect)), effect);
            }
        });
        ui.horizontal_wrapped(|ui| {
            boxed(ui, egui::Stroke::new(2.5, v.selection.stroke.color), "selected");
            boxed(ui, egui::Stroke::new(2.5, v.hyperlink_color), "focused");
            boxed_fill(ui, drifter_fill(v.panel_fill), egui::Stroke::new(1.5, v.widgets.noninteractive.bg_stroke.color), &format!("{} drifter system", icon::SKULL));
            room_for(ui, 30.0, "your characters there");
            ui.label(egui::RichText::new(format!("{} 2", icon::USER)).color(v.hyperlink_color));
            ui.label("your characters there");
            room_for(ui, 30.0, "holes whose far side is unknown");
            ui.label(egui::RichText::new("1 ?").color(v.warn_fg_color));
            ui.label("holes whose far side is unknown");
        });
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(
                "Chains grow from a system one of your characters is in, else a pinned system, else the one with the most holes.",
            )
            .weak(),
        );
    });
    ui.add_space(4.0);
}
