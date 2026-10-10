//! The star map's shared pieces: the wormhole overlay drawn over New Eden, and the line styles
//! bridges and routes are drawn with, and the layers both apps draw.

use std::collections::{HashMap, HashSet};

use spai_core::ansiblex::JumpBridge;
use spai_core::geo::Systems;
use spai_core::map::MapSystem;

pub fn is_kspace(id: i64) -> bool {
    (30_000_000..31_000_000).contains(&id)
}

pub fn is_jspace(id: i64) -> bool {
    (31_000_000..32_000_000).contains(&id)
}

/// What kinds of hole a k-space system has, for its icon on the map.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct HoleMark {
    pub regular: bool,
    pub thera: bool,
    /// The drifter systems its drifter holes lead to, by letter; '?' for one not known yet.
    pub drifters: std::collections::BTreeSet<char>,
}

impl HoleMark {
    /// The kind of `w`, seen from its k-space end.
    pub fn add(&mut self, w: &spai_core::wormholes::Wormhole) {
        const THERA: i64 = 31_000_005;
        let ends = [Some(w.system_id), w.dest_system_id];
        let drifter = spai_core::whdata::DRIFTERS
            .iter()
            .find(|d| ends.contains(&Some(d.2)))
            .map(|d| d.2)
            .or_else(|| [&w.wh_type, &w.dest_wh_type].into_iter().flatten().find_map(|t| spai_core::whdata::drifter_for_code(t)));
        if let Some(id) = drifter {
            self.drifters.insert(drifter_letter(id));
        } else if w.is_drifter {
            self.drifters.insert('?');
        } else if w.dest == spai_core::wormholes::DestClass::Thera || ends.contains(&Some(THERA)) {
            self.thera = true;
        } else {
            self.regular = true;
        }
    }
}

/// A drifter system's letter: S, B, V, C, R.
pub fn drifter_letter(id: i64) -> char {
    spai_core::whdata::DRIFTERS.iter().find(|d| d.2 == id).and_then(|d| d.1.chars().next()).map_or('?', |c| c.to_ascii_uppercase())
}

#[derive(Default, Clone)]
pub struct WhOverlay {
    pub direct: Vec<(i64, i64)>,
    pub chains: Vec<(i64, i64, usize)>,
    pub jspace_holes: std::collections::HashSet<i64>,
    /// The kinds of hole behind each of `jspace_holes`.
    pub marks: std::collections::HashMap<i64, HoleMark>,
    pub thera_conns: Vec<i64>,
    /// Links drawn above that no hole routes may use still makes: a pair from `direct` or
    /// `chains`, smaller id first, or a system of `thera_conns` paired with Thera.
    pub blocked: std::collections::HashSet<(i64, i64)>,
    /// Systems of `jspace_holes` whose every hole into J-space is switched off.
    pub jspace_blocked: std::collections::HashSet<i64>,
}

impl WhOverlay {
    /// `usable` says which holes routes may go through; the rest are drawn as switched off.
    pub fn build(whs: &[spai_core::wormholes::Wormhole], usable: impl Fn(&spai_core::wormholes::Wormhole) -> bool) -> WhOverlay {
        const THERA: i64 = 31_000_005;
        const MAX_CHAINS: usize = 60;
        let mut all = Self::links(whs);
        let open: Vec<spai_core::wormholes::Wormhole> = whs.iter().filter(|w| usable(w)).cloned().collect();
        let open = Self::links(&open);
        let reach: std::collections::HashSet<(i64, i64)> = open.direct.iter().copied().chain(open.chains.iter().map(|c| (c.0, c.1))).collect();
        all.blocked = all.direct.iter().copied().chain(all.chains.iter().map(|c| (c.0, c.1))).filter(|k| !reach.contains(k)).collect();
        all.blocked.extend(all.thera_conns.iter().filter(|id| !open.thera_conns.contains(id)).map(|id| (*id, THERA)));
        all.jspace_blocked = all.jspace_holes.difference(&open.jspace_holes).copied().collect();
        all.chains.truncate(MAX_CHAINS);
        all
    }

    fn links(whs: &[spai_core::wormholes::Wormhole]) -> WhOverlay {
        use std::collections::{HashMap, HashSet, VecDeque};
        const MAX_J_HOPS: usize = 4;
        const MAX_HUB_DEGREE: usize = 6;

        use spai_core::wormholes::DestClass;
        // Turnur is itself K-space, so a hole to it is a K→K edge the is_jspace test below misses.
        let notable_dest =
            |d: DestClass| matches!(d, DestClass::Wspace | DestClass::Thera | DestClass::Turnur);
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut jspace_holes: HashSet<i64> = HashSet::new();
        let mut marks: HashMap<i64, HoleMark> = HashMap::new();
        let mut mark = |k: i64, w: &spai_core::wormholes::Wormhole| {
            jspace_holes.insert(k);
            marks.entry(k).or_default().add(w);
        };
        for w in whs {
            let a = w.system_id;
            let b = w.dest_system_id;
            if is_kspace(a) && (notable_dest(w.dest) || b.is_some_and(is_jspace)) {
                mark(a, w);
            }
            if let Some(b) = b {
                adj.entry(a).or_default().push(b);
                adj.entry(b).or_default().push(a);
                if is_kspace(b) && is_jspace(a) {
                    mark(b, w);
                }
            }
        }
        let degree: HashMap<i64, usize> =
            adj.iter().map(|(k, v)| (*k, v.len())).collect();

        let mut direct: Vec<(i64, i64)> = Vec::new();
        let mut chains: Vec<(i64, i64, usize)> = Vec::new();
        let mut seen: HashSet<(i64, i64)> = HashSet::new();
        let mut starts: Vec<i64> = adj.keys().copied().filter(|id| is_kspace(*id)).collect();
        starts.sort_unstable();
        for &start in &starts {
            let mut visited: HashSet<i64> = HashSet::from([start]);
            let mut q: VecDeque<(i64, usize)> = VecDeque::from([(start, 0usize)]);
            while let Some((node, jhops)) = q.pop_front() {
                for &nb in adj.get(&node).into_iter().flatten() {
                    if is_kspace(nb) {
                        if nb == start {
                            continue;
                        }
                        let key = (start.min(nb), start.max(nb));
                        if seen.insert(key) {
                            if jhops == 0 {
                                direct.push(key);
                            } else {
                                chains.push((key.0, key.1, jhops));
                            }
                        }
                    } else if is_jspace(nb)
                        && !visited.contains(&nb)
                        && jhops < MAX_J_HOPS
                        && degree.get(&nb).copied().unwrap_or(0) <= MAX_HUB_DEGREE
                    {
                        visited.insert(nb);
                        q.push_back((nb, jhops + 1));
                    }
                }
            }
        }
        chains.sort_by_key(|c| c.2);
        const THERA: i64 = 31_000_005;
        let thera_conns: Vec<i64> = adj
            .get(&THERA)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| is_kspace(*id))
            .collect();
        WhOverlay { direct, chains, jspace_holes, marks, thera_conns, ..Default::default() }
    }
}

/// `dashed_flow` along a polyline, so an arc crawls the same way a straight leg does.
///
/// Per segment with the phase carried forward, rather than per segment from zero: restarting the
/// pattern at every sample of a fourteen-point arc turns a crawl into a shimmer.
pub fn polyline_flow(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    color: egui::Color32,
    phase: f32,
) {
    polyline_flow_gradient(painter, pts, color, color, phase);
}

/// [`polyline_flow`] shading from `from` to `to`, for a bridge coloured by the zones at its ends.
pub fn polyline_flow_gradient(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    from: egui::Color32,
    to: egui::Color32,
    phase: f32,
) {
    let total: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let len = (w[1] - w[0]).length();
        let t = if total > 0.0 { (walked + len * 0.5) / total } else { 0.0 };
        dashed_flow(painter, w[0], w[1], lerp_color(from, to, t), phase - walked);
        walked += len;
    }
}

pub fn dashed_flow(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32, phase: f32) {
    let dir = p2 - p1;
    let len = dir.length();
    if len < 1.0 {
        return;
    }
    let unit = dir / len;
    let (dash, period) = (6.0f32, 12.0f32);
    let mut d = (phase % period) - period;
    let stroke = egui::Stroke::new(2.0, color);
    while d < len {
        let s = d.max(0.0);
        let e = (d + dash).min(len);
        if e > s {
            painter.line_segment([p1 + unit * s, p1 + unit * e], stroke);
        }
        d += period;
    }
}

/// An arch between two points, sampled as a polyline.
///
/// Jump bridges are drawn as arches rather than straight lines: a bridge and a gate between the same
/// pair of systems are otherwise the same stroke in a different colour, and on a busy map colour
/// alone is not enough to tell a route you can fly from one you need a bridge for.
///
/// The apex rises straight up the screen rather than perpendicular to the segment. The map is a
/// top-down projection of a plane, so "above the plane" is up, whatever direction the bridge runs;
/// a perpendicular bow makes a north-south bridge bulge sideways, which reads as a detour rather
/// than as height.
pub fn arc_polyline(a: egui::Pos2, b: egui::Pos2, bow: f32) -> Vec<egui::Pos2> {
    let d = b - a;
    let len = d.length();
    if len < 0.5 {
        return vec![a, b];
    }
    let mid = a + d * 0.5;
    let ctrl = mid - egui::vec2(0.0, len * bow * 2.0);
    (0..=14)
        .map(|i| {
            let t = i as f32 / 14.0;
            let u = 1.0 - t;
            egui::pos2(
                u * u * a.x + 2.0 * u * t * ctrl.x + t * t * b.x,
                u * u * a.y + 2.0 * u * t * ctrl.y + t * t * b.y,
            )
        })
        .collect()
}

/// Marks the only direction a route may take a bridge, `inset` back from the end of `arc` so the
/// destination's dot does not cover it.
pub fn bridge_arrowhead(painter: &egui::Painter, arc: &[egui::Pos2], color: egui::Color32, inset: f32) {
    let mut left = inset;
    let mut at = None;
    for w in arc.windows(2).rev() {
        let (a, b) = (w[0], w[1]);
        let len = (b - a).length();
        if len >= left && len > 0.0 {
            let dir = (b - a) / len;
            at = Some((b - dir * left, dir));
            break;
        }
        left -= len;
    }
    let Some((tip, dir)) = at else { return };
    let back = tip - dir * 10.0;
    let side = egui::vec2(-dir.y, dir.x) * 5.5;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, back + side, back - side],
        color,
        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(160)),
    ));
}

pub fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// `pts` as a solid line shading from `from` to `to` along its length.
pub fn gradient_polyline(painter: &egui::Painter, pts: &[egui::Pos2], from: egui::Color32, to: egui::Color32, width: f32) {
    let total: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let len = (w[1] - w[0]).length();
        let t = if total > 0.0 { (walked + len * 0.5) / total } else { 0.0 };
        painter.line_segment([w[0], w[1]], egui::Stroke::new(width, lerp_color(from, to, t)));
        walked += len;
    }
}

/// How high a bridge arch rises, as a fraction of its own length.
pub const BRIDGE_BOW: f32 = 0.12;

/// Each bridge, low system id first, with whether the zone limit lets a route take it from the
/// low end and from the high end.
pub fn bridge_directions(list: &[JumpBridge], graph: &Systems, capital: &str, max_zone: u8) -> HashMap<(i64, i64), (bool, bool)> {
    spai_core::ansiblex::bridges(list, graph, capital)
        .into_iter()
        .map(|br| {
            let (fwd, back) = (br.forward(max_zone), br.back(max_zone));
            if br.a < br.b {
                ((br.a, br.b), (fwd, back))
            } else {
                ((br.b, br.a), (back, fwd))
            }
        })
        .collect()
}

/// A bridge's colours at each end: the zone each end sits in.
pub fn bridge_colors(graph: &Systems, capital: &str, a: i64, b: i64, fallback: egui::Color32) -> (egui::Color32, egui::Color32) {
    let col = |sys| spai_core::ansiblex::zone_at(graph, capital, sys).map_or(fallback, spai_core::ansiblex::zone_color);
    (col(a), col(b))
}

fn seg_visible(cull: egui::Rect, a: egui::Pos2, b: egui::Pos2) -> bool {
    egui::Rect::from_two_pos(a, b).intersects(cull)
}

/// Stargates between the drawn systems, except where a bridge is drawn instead. A gate says where
/// you are as much as where you can go: inside a constellation, out of it, or out of the region
/// entirely. On a map of identical solid lines none of those boundaries would be visible.
pub fn paint_gates(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    graph: &Systems,
    draw: &[MapSystem],
    pos: &HashMap<i64, egui::Pos2>,
    bridges: &HashMap<(i64, i64), (bool, bool)>,
    cull: egui::Rect,
) {
    let line_col = visuals.weak_text_color().gamma_multiply(0.5);
    let region_of: HashMap<i64, i64> = draw.iter().map(|s| (s.id, s.region_id)).collect();
    let constel_of: HashMap<i64, &str> = draw.iter().filter_map(|s| graph.info_of(s.id).map(|i| (s.id, i.constellation.as_str()))).collect();
    for s in draw {
        let p1 = pos[&s.id];
        for &n in graph.neighbors(s.id) {
            if s.id < n && !bridges.contains_key(&(s.id, n)) {
                if let Some(p2) = pos.get(&n) {
                    if seg_visible(cull, p1, *p2) {
                        let stroke = egui::Stroke::new(1.0, line_col);
                        let other_region = region_of.get(&n).is_some_and(|r| *r != s.region_id);
                        let other_constel = constel_of.get(&s.id).zip(constel_of.get(&n)).is_some_and(|(a, b)| a != b);
                        if other_region {
                            painter.extend(egui::Shape::dashed_line(&[p1, *p2], stroke, 4.0, 4.0));
                        } else if other_constel {
                            // Dotted: a short dash with a wide gap reads as dots without needing a
                            // separate shape. 1 px dots shimmered out of sight as their sub-pixel
                            // position moved with pan and zoom.
                            painter.extend(egui::Shape::dashed_line(&[p1, *p2], egui::Stroke::new(1.3, line_col), 2.0, 3.0));
                        } else {
                            painter.line_segment([p1, *p2], stroke);
                        }
                    }
                }
            }
        }
    }
}

/// How far to pan this frame while a route is dragged near the edge of `view`: faster the deeper
/// the pointer is into the edge band, up to a steady top speed, and not once the map's systems
/// (`content`) no longer reach past the middle of the view on that side, so it cannot run away.
pub fn edge_pan(view: egui::Rect, pointer: egui::Pos2, content: egui::Rect, dt: f32) -> egui::Vec2 {
    const BAND: f32 = 56.0;
    const TOP_SPEED: f32 = 900.0;
    let depth = |into: f32| (into / BAND).clamp(0.0, 1.0);
    let speed = |t: f32| TOP_SPEED * t * t * dt.min(0.05);
    let mut d = egui::Vec2::ZERO;
    let c = view.center();
    let right = depth(pointer.x - (view.right() - BAND));
    let left = depth(view.left() + BAND - pointer.x);
    let down = depth(pointer.y - (view.bottom() - BAND));
    let up = depth(view.top() + BAND - pointer.y);
    if right > 0.0 && content.right() > c.x {
        d.x -= speed(right);
    }
    if left > 0.0 && content.left() < c.x {
        d.x += speed(left);
    }
    if down > 0.0 && content.bottom() > c.y {
        d.y -= speed(down);
    }
    if up > 0.0 && content.top() < c.y {
        d.y += speed(up);
    }
    d
}

/// The pairs of systems a route runs between, low id first: each step, and each stretch through
/// wormhole space between two k-space systems, which the map draws as one chain.
pub fn route_pairs(path: &[i64], out: &mut HashSet<(i64, i64)>) {
    let mut last_k: Option<i64> = None;
    for (i, &id) in path.iter().enumerate() {
        if i > 0 {
            let a = path[i - 1];
            out.insert((a.min(id), a.max(id)));
        }
        if is_kspace(id) {
            if let Some(k) = last_k.filter(|k| *k != id && path.get(i.wrapping_sub(1)) != Some(k)) {
                out.insert((k.min(id), k.max(id)));
            }
            last_k = Some(id);
        }
    }
}

/// Ansiblex bridges as arcs coloured by zone, skipping those in `routed` (a route draws its own).
/// A bridge the zone limit closes both ways is faint and dashed; one open only one way gets an
/// arrowhead.
pub fn paint_bridges(
    painter: &egui::Painter,
    bridges: &HashMap<(i64, i64), (bool, bool)>,
    routed: &HashSet<(i64, i64)>,
    pos: &HashMap<i64, egui::Pos2>,
    cull: egui::Rect,
    dot: f32,
    colors: impl Fn(i64, i64) -> (egui::Color32, egui::Color32),
) {
    for (&(a, c), &(up, down)) in bridges {
        if routed.contains(&(a, c)) {
            continue;
        }
        let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&c)) else { continue };
        if !seg_visible(cull, *p1, *p2) {
            continue;
        }
        let arc = arc_polyline(*p1, *p2, BRIDGE_BOW);
        let (ca, cc) = colors(a, c);
        if !up && !down {
            polyline_flow_gradient(painter, &arc, ca.gamma_multiply(0.5), cc.gamma_multiply(0.5), 0.0);
            continue;
        }
        gradient_polyline(painter, &arc, ca, cc, 1.8);
        if up != down {
            let (tip, col): (Vec<egui::Pos2>, _) = if up { (arc, cc) } else { (arc.into_iter().rev().collect(), ca) };
            bridge_arrowhead(painter, &tip, col, dot + 5.0);
        }
    }
}

/// Which of the wormhole layer's parts to draw.
#[derive(Clone, Copy, Debug)]
pub struct HoleLayer {
    pub turnur: bool,
    pub thera: bool,
    /// The map is laid out by region (Thera then sits between Solitude and Derelik).
    pub spaced: bool,
}

/// Known holes over New Eden: direct k-space links, chains through J-space with their jump
/// counts, and Thera and Turnur with their connections. `place` projects map coordinates.
#[allow(clippy::too_many_arguments)]
pub fn paint_wormholes(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    overlay: &WhOverlay,
    draw: &[MapSystem],
    pos: &HashMap<i64, egui::Pos2>,
    layer: HoleLayer,
    dot: f32,
    place: impl Fn(f64, f64) -> egui::Pos2,
    routed: &HashSet<(i64, i64)>,
) {
        let wh_col = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
        // A hole or chain a route takes is drawn by the route, animated: the plain line under it would
        // put two lines on one hop. See [`route_pairs`].
        let taken = |a: i64, b: i64| routed.contains(&(a.min(b), a.max(b)));
        let chain_col = egui::Color32::from_rgb(0xB0, 0x7C, 0xE8);
        const TURNUR: i64 = 30_002_086;
        let off = |a: i64, b: i64| overlay.blocked.contains(&(a.min(b), a.max(b)));
        for &(a, b) in &overlay.direct {
            if (!layer.turnur && (a == TURNUR || b == TURNUR)) || taken(a, b) {
                continue;
            }
            if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&b)) {
                let stroke = egui::Stroke::new(1.6, wh_col);
                if off(a, b) {
                    crate::wh_graph::slashed(&painter, &[*p1, *p2], crate::wh_graph::desaturate_stroke(stroke), |p, l, s| {
                        p.add(egui::Shape::line(l.to_vec(), s));
                    });
                } else {
                    painter.line_segment([*p1, *p2], stroke);
                }
            }
        }
        for &(a, b, hops) in &overlay.chains {
            if (!layer.turnur && (a == TURNUR || b == TURNUR)) || taken(a, b) {
                continue;
            }
            if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&b)) {
                let stroke = egui::Stroke::new(1.8, chain_col);
                if off(a, b) {
                    crate::wh_graph::slashed(&painter, &[*p1, *p2], crate::wh_graph::desaturate_stroke(stroke), |p, l, s| {
                        p.extend(egui::Shape::dashed_line(l, s, 6.0, 4.0));
                    });
                } else {
                    painter.extend(egui::Shape::dashed_line(&[*p1, *p2], stroke, 6.0, 4.0));
                }
                let mid = egui::pos2((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
                let txt = format!("{hops}J");
                let r = painter.text(
                    mid,
                    egui::Align2::CENTER_CENTER,
                    &txt,
                    egui::FontId::proportional(11.0),
                    chain_col,
                );
                painter.rect_filled(r.expand(2.0), 3.0, visuals.extreme_bg_color.gamma_multiply(0.7));
                painter.text(
                    mid,
                    egui::Align2::CENTER_CENTER,
                    &txt,
                    egui::FontId::proportional(11.0),
                    chain_col,
                );
            }
        }
        if layer.thera {
            let conns: Vec<&MapSystem> = overlay
                .thera_conns
                .iter()
                .filter_map(|id| draw.iter().find(|s| s.id == *id))
                .collect();
            let conn_screen: Vec<egui::Pos2> =
                conns.iter().filter_map(|s| pos.get(&s.id).copied()).collect();
            if !conns.is_empty() && !conn_screen.is_empty() {
                let mut cx = conns.iter().map(|s| s.x).sum::<f64>() / conns.len() as f64;
                let min_z = conns.iter().map(|s| s.z).fold(f64::INFINITY, f64::min);
                let max_z = conns.iter().map(|s| s.z).fold(f64::NEG_INFINITY, f64::max);
                let mut tz = min_z - (max_z - min_z).max(1.0) * 0.25;
                if layer.spaced {
                    let rc = |rid: i64| -> Option<(f64, f64)> {
                        let sys: Vec<&MapSystem> =
                            draw.iter().filter(|s| s.region_id == rid).collect();
                        if sys.is_empty() {
                            return None;
                        }
                        let n = sys.len() as f64;
                        Some((
                            sys.iter().map(|s| s.x).sum::<f64>() / n,
                            sys.iter().map(|s| s.z).sum::<f64>() / n,
                        ))
                    };
                    if let (Some(sl), Some(dm)) = (rc(10_000_053), rc(10_000_045)) {
                        cx = (sl.0 + dm.0) / 2.0;
                        tz = (sl.1 + dm.1) / 2.0;
                    }
                }
                let tp = place(cx, tz);
                let line_col = egui::Color32::from_rgb(0x6E, 0xC8, 0xF0);
                let tcol = egui::Color32::from_rgb(0xB0, 0x70, 0xE0);
                for (s, p) in conns.iter().filter_map(|s| Some((s, pos.get(&s.id)?))) {
                    let stroke = egui::Stroke::new(1.6, line_col);
                    if overlay.blocked.contains(&(s.id, 31_000_005)) {
                        crate::wh_graph::slashed(&painter, &[tp, *p], crate::wh_graph::desaturate_stroke(stroke), |p, l, s| {
                            p.add(egui::Shape::line(l.to_vec(), s));
                        });
                    } else {
                        painter.line_segment([tp, *p], stroke);
                    }
                }
                painter.circle_filled(tp, dot + 3.0, tcol);
                painter.circle_stroke(tp, dot + 6.0, egui::Stroke::new(2.0, tcol));
                let lp = tp + egui::vec2(0.0, -dot - 11.0);
                let r = painter.text(lp, egui::Align2::CENTER_CENTER, "Thera",
                    egui::FontId::proportional(12.0), tcol);
                painter.rect_filled(r.expand(2.0), 3.0,
                    visuals.extreme_bg_color.gamma_multiply(0.7));
                painter.text(lp, egui::Align2::CENTER_CENTER, "Thera",
                    egui::FontId::proportional(12.0), tcol);
            }
        }
        if layer.turnur {
            if let Some(tp) = pos.get(&TURNUR).copied() {
                let col = egui::Color32::from_rgb(0xE0, 0xA8, 0x4C);
                painter.circle_stroke(tp, dot + 6.0, egui::Stroke::new(2.0, col));
                let lp = tp + egui::vec2(0.0, -dot - 11.0);
                let r = painter.text(lp, egui::Align2::CENTER_CENTER, "Turnur",
                    egui::FontId::proportional(12.0), col);
                painter.rect_filled(r.expand(2.0), 3.0,
                    visuals.extreme_bg_color.gamma_multiply(0.7));
                painter.text(lp, egui::Align2::CENTER_CENTER, "Turnur",
                    egui::FontId::proportional(12.0), col);
            }
        }
}

/// A planned route's legs: capital jumps and bridges as arcs, gates as crawling dashes, holes in the
/// hole colour. `by_hole` says whether a gate-kind hop is really a wormhole.
#[allow(clippy::too_many_arguments)]
pub fn paint_route_legs(
    painter: &egui::Painter,
    pos: &HashMap<i64, egui::Pos2>,
    hops: &[spai_core::route::Hop],
    phase: f32,
    hole_color: egui::Color32,
    by_hole: impl Fn(i64, i64) -> bool,
    bridge_colors: impl Fn(i64, i64, egui::Color32) -> (egui::Color32, egui::Color32),
) {
    const PICK_GATE: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xB1, 0x34);
    const PICK_JUMP: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x7B, 0xE0);
    const PICK_BRIDGE: egui::Color32 = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);
    // A J-space system has no place on this map: the hop is drawn between the k-space systems
    // on either side of it, as a hole, rather than dropped.
    let mut last: Option<(i64, egui::Pos2)> = None;
    let mut through_jspace = false;
    for h in hops {
        let Some(&b) = pos.get(&h.id) else {
            through_jspace |= last.is_some();
            continue;
        };
        let Some((prev_id, a)) = last.replace((h.id, b)) else { continue };
        let hole = std::mem::take(&mut through_jspace) || (h.kind == 0 && by_hole(prev_id, h.id));
        if hole {
            dashed_flow(painter, a, b, hole_color, phase);
            continue;
        }
        match h.kind {
            2 | 1 => {
                let (ca, cb) = if h.kind == 2 {
                    (PICK_JUMP, PICK_JUMP)
                } else {
                    bridge_colors(prev_id, h.id, PICK_BRIDGE)
                };
                // Dashed and crawling like the gates and like the browser's: an arc drawn
                // solid while the rest of the route moves reads as a different kind of thing.
                polyline_flow_gradient(painter, &arc_polyline(a, b, BRIDGE_BOW), ca, cb, phase);
            }
            // Crawling dashes, the same as the browser's and the same as this map's own
            // travel route: a static line is hard to pick out of a map already full of them.
            _ => dashed_flow(painter, a, b, PICK_GATE, phase),
        }
    }
}

/// Region names over the middle of each region's drawn systems, for a map zoomed out too far for
/// system names.
pub fn paint_region_labels(painter: &egui::Painter, draw: &[MapSystem], pos: &HashMap<i64, egui::Pos2>, regions: &[(i64, String)], rect: egui::Rect) {
        let mut acc: std::collections::HashMap<i64, (egui::Vec2, u32)> =
            std::collections::HashMap::new();
        for s in draw {
            let e = acc.entry(s.region_id).or_insert((egui::Vec2::ZERO, 0));
            e.0 += pos[&s.id].to_vec2();
            e.1 += 1;
        }
        let mut labels: Vec<(i64, egui::Pos2)> =
            acc.into_iter().map(|(rid, (sum, n))| (rid, (sum / n as f32).to_pos2())).collect();
        labels.sort_by_key(|(rid, _)| *rid);
        let font = egui::FontId::proportional(16.0);
        for (rid, c) in labels {
            if !rect.contains(c) {
                continue;
            }
            let Some((_, name)) = regions.iter().find(|(id, _)| *id == rid) else {
                continue;
            };
            let col = painter.ctx().global_style().visuals.strong_text_color();
            painter.text(c + egui::vec2(1.0, 1.0), egui::Align2::CENTER_CENTER, name, font.clone(), crate::theme::halo(col));
            painter.text(c, egui::Align2::CENTER_CENTER, name, font.clone(), col);
        }
}

/// Jove Observatory systems' marker colour.
/// How a route list names the way into a hop, and its colour: the map's own colours for each kind
/// of leg, so a jump, a bridge or a hole stands out from the gates around it. `None` is a gate.
pub fn hop_tail(h: &spai_core::route::Hop, first: bool) -> (String, Option<egui::Color32>) {
    if first {
        return (crate::tr!("start").to_owned(), None);
    }
    match h.kind {
        2 => (crate::trf!("jump {ly} ly", ly = format!("{:.1}", h.ly.unwrap_or_default())), Some(egui::Color32::from_rgb(0xE0, 0x7B, 0xE0))),
        1 => (crate::tr!("ansiblex").to_owned(), Some(egui::Color32::from_rgb(0x3A, 0xD0, 0x6A))),
        _ if h.hole => (crate::tr!("wormhole").to_owned(), Some(egui::Color32::from_rgb(0x4D, 0xD0, 0xC4))),
        _ => (crate::tr!("gate").to_owned(), None),
    }
}

pub const JOVE_COLOR: egui::Color32 = egui::Color32::from_rgb(0xB8, 0x8C, 0xF0);

/// How many icon widths a system's hole mark takes.
pub fn hole_mark_slots(m: &HoleMark) -> f32 {
    let blue = m.regular || m.thera;
    if blue && !m.drifters.is_empty() { 2.0 } else { 1.35 }
}

/// A system's hole icon, left edge at `at.x`, centred on `at.y`: blue for regular holes with a T
/// for Thera, pink for drifter holes with each drifter's letter, the two offset when both apply.
pub fn paint_hole_mark(painter: &egui::Painter, at: egui::Pos2, icon_h: f32, icon_w: f32, m: &HoleMark, blocked: bool) {
    let tint = |c: egui::Color32| if blocked { crate::wh_graph::desaturate(c) } else { c };
    let blue = tint(egui::Color32::from_rgb(0x4D, 0xD0, 0xC4));
    let pink = tint(crate::wh_graph::drifter_color());
    // A size up from the other icons, so the letters on it stay readable at a small dot.
    let icon_h = icon_h * 1.2;
    let font = egui::FontId::proportional(icon_h);
    let letter_font = |n: usize| egui::FontId::new(icon_h * if n > 2 { 0.5 } else if n == 2 { 0.58 } else { 0.72 }, egui::FontFamily::Proportional);
    let glyph = |x: f32, col: egui::Color32| {
        painter.text(egui::pos2(x, at.y), egui::Align2::LEFT_CENTER, egui_phosphor::regular::SPIRAL, font.clone(), col);
    };
    // Letters sit on the spiral's middle, outlined so they read over its lines.
    let letters = |x: f32, text: &str| {
        let c = egui::pos2(x + icon_h / 2.0, at.y);
        let f = letter_font(text.chars().count());
        for off in [egui::vec2(-1.0, 0.0), egui::vec2(1.0, 0.0), egui::vec2(0.0, -1.0), egui::vec2(0.0, 1.0)] {
            painter.text(c + off, egui::Align2::CENTER_CENTER, text, f.clone(), egui::Color32::BLACK);
        }
        painter.text(c, egui::Align2::CENTER_CENTER, text, f, egui::Color32::WHITE);
    };
    let has_blue = m.regular || m.thera;
    let mut x = at.x;
    if has_blue {
        glyph(x, blue);
        if !m.drifters.is_empty() {
            x += icon_w * 0.55;
        }
    }
    if !m.drifters.is_empty() {
        glyph(x, pink);
        letters(x, &m.drifters.iter().collect::<String>());
    }
    if m.thera {
        letters(at.x, "T");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each step is a pair, and so is a stretch through wormhole space between two k-space
    /// systems, which the map draws as one chain; k-space systems a gate apart add nothing more.
    /// Faster towards the edge, capped, and only while the map still reaches past the middle.
    #[test]
    fn dragging_near_an_edge_pans_towards_it_within_limits() {
        let view = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0));
        let wide = egui::Rect::from_min_max(egui::pos2(-2000.0, -2000.0), egui::pos2(3000.0, 3000.0));
        let dt = 1.0 / 60.0;
        assert_eq!(edge_pan(view, egui::pos2(500.0, 400.0), wide, dt), egui::Vec2::ZERO, "nothing mid-screen");
        let near = edge_pan(view, egui::pos2(970.0, 400.0), wide, dt).x;
        let at = edge_pan(view, egui::pos2(1000.0, 400.0), wide, dt).x;
        assert!(near < 0.0 && at < near, "the map moves left, faster at the edge: {near} {at}");
        assert!(edge_pan(view, egui::pos2(1400.0, 400.0), wide, dt).x >= at, "no faster past the edge");
        let gone = egui::Rect::from_min_max(egui::pos2(-900.0, 0.0), egui::pos2(400.0, 800.0));
        assert_eq!(edge_pan(view, egui::pos2(995.0, 400.0), gone, dt).x, 0.0, "nothing more to the right");
    }

    #[test]
    fn a_route_covers_its_steps_and_its_wormhole_stretches() {
        let (jita, j1, j2, amarr, dodixie) = (30_000_142, 31_000_200, 31_000_300, 30_002_187, 30_002_659);
        let mut out = HashSet::new();
        route_pairs(&[jita, j1, j2, amarr, dodixie], &mut out);
        assert!(out.contains(&(jita.min(j1), jita.max(j1))));
        assert!(out.contains(&(jita.min(amarr), jita.max(amarr))), "the chain from Jita to Amarr");
        assert!(out.contains(&(amarr.min(dodixie), amarr.max(dodixie))));
        assert!(!out.contains(&(jita.min(dodixie), jita.max(dodixie))), "not across a gate step");
        assert_eq!(out.len(), 5);
    }
}
