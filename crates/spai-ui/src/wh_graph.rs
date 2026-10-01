//! The wormhole map's layout and drawing, free of the app: where systems go, how hole lines are
//! routed between them, and the colours and marks they are drawn with.

use std::collections::{HashMap, HashSet, VecDeque};

use egui_phosphor::regular as icon;

use spai_core::whdata::{self, Class};
use spai_core::wormholes::{time_left, Mass, TimeLeft, Wormhole};
// Where each hole's line was drawn in the last frame, by hole id, for tests that hover one.
#[cfg(any(test, feature = "test-support"))]
thread_local! {
    pub static EDGE_PROBE: std::cell::RefCell<Vec<(i64, Vec<egui::Pos2>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Most gate jumps a pinned system may be from a chain and still join it.
pub const WH_PIN_JUMPS_MAX: u32 = 100;

pub const NODE: egui::Vec2 = egui::vec2(290.0, 56.0);
/// Drifter systems: their name, J-code and badge need more room.
/// A pinned system's copy beside a cluster.
pub const PILL: egui::Vec2 = egui::vec2(230.0, 40.0);


/// A line's details beside the pointer. Drawn here rather than as the map's own tooltip: that
/// belongs to the whole map, and egui places and gates it for the map, not for the line.
pub fn line_tip(ui: &egui::Ui, pointer: Option<egui::Pos2>, text: String) {
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
/// Who added a hole and when, and who last changed it, for tooltips and lists: "Added by X 3h ago"
/// and, when someone changed it since, "edited by Y 20m ago".
pub fn who_lines(w: &spai_core::wormholes::Wormhole, now: i64) -> (String, Option<String>) {
    use spai_core::wormholes::Source;
    let ago = |t: i64| crate::widgets::human_ago(now - t);
    let added = match w.created_by.as_ref().or(w.detected_by.as_ref()) {
        Some(who) => format!("Added by {who} {} ago", ago(w.reported_at)),
        None => match w.source {
            Source::Manual => format!("Added by hand {} ago", ago(w.reported_at)),
            Source::Auto => format!("Detected {} ago", ago(w.reported_at)),
            s => format!("Added from {} {} ago", s.label(), ago(w.reported_at)),
        },
    };
    // A change within a minute of adding it is the adding itself.
    let edited = w.edited_by.as_ref().filter(|(_, at)| *at > w.reported_at + 60).map(|(who, at)| format!("Edited by {who} {} ago", ago(*at)));
    (added, edited)
}

/// The same as [`who_lines`] for a list cell: how long ago and who, the edit marked with a pencil,
/// with the full sentences on hover.
pub fn who_cell(ui: &mut egui::Ui, w: &spai_core::wormholes::Wormhole, now: i64) {
    let (added, edited) = who_lines(w, now);
    let ago = |t: i64| crate::widgets::human_ago(now - t);
    let by = w.created_by.as_ref().or(w.detected_by.as_ref()).cloned().unwrap_or_else(|| w.source.label().to_owned());
    let hover = match &edited {
        Some(e) => format!("{added}\n{e}"),
        None => added,
    };
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        // A fixed width the names wrap in by word: a long name must neither widen the panel nor
        // break letter by letter in a squeezed column.
        ui.set_min_width(56.0);
        ui.set_max_width(56.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(egui::RichText::new(format!("{} ago", ago(w.reported_at))).weak()).on_hover_text(&hover);
        ui.label(egui::RichText::new(by).weak()).on_hover_text(&hover);
        if let (Some(_), Some((who, at))) = (&edited, &w.edited_by) {
            ui.label(egui::RichText::new(format!("{} {} ago", egui_phosphor::regular::PENCIL_SIMPLE, ago(*at))).weak()).on_hover_text(&hover);
            ui.label(egui::RichText::new(who).weak()).on_hover_text(&hover);
        }
    });
}

pub fn tip_pos(pointer: egui::Pos2, size: egui::Vec2, screen: egui::Rect) -> egui::Pos2 {
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
pub fn unidentified_type(system: i64, name: &str) -> Option<&'static str> {
    name.to_lowercase().contains("unidentified").then(|| whdata::drifter_code(system)).flatten()
}

pub fn node_size(id: i64) -> egui::Vec2 {
    if id < 0 {
        return PILL;
    }
    NODE
}

/// A system's name as people say it: the drifter systems by their own names, not J-codes.
pub fn display_name(id: i64, name: &str) -> String {
    match whdata::DRIFTERS.iter().find(|d| d.2 == id) {
        Some(d) => {
            let mut c = d.1.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
        None => name.to_owned(),
    }
}
#[cfg(test)]
use crate::wh_layout::COL;
pub const GRID: f32 = 10.0;
/// How far out of a box an edge runs before it turns.
pub const STUB: f32 = 20.0;
pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideTab {
    #[default]
    Info,
    Routes,
    Sigs,
}

/// A hole's type, whichever side it was read on; between a drifter system and k-space, that
/// drifter's own type when none was entered.
pub fn hole_code(w: &Wormhole) -> Option<String> {
    let wspace = |id: i64| (31_000_000..32_000_000).contains(&id);
    let drifter = || match (spai_core::whdata::drifter_code(w.system_id), w.dest_system_id) {
        (Some(c), Some(d)) if !wspace(d) => Some(c.to_owned()),
        (None, Some(d)) if !wspace(w.system_id) => spai_core::whdata::drifter_code(d).map(str::to_owned),
        _ => None,
    };
    w.wh_type.clone().or_else(|| w.dest_wh_type.clone()).or_else(drifter)
}

pub fn letters(sig: &str) -> String {
    sig.trim().chars().take(3).collect::<String>().to_uppercase()
}

/// What a probe scan of `system` says about the saved holes. First, those whose signature there it
/// no longer lists: only from a paste that replaces the list and has signatures in it. Second, a
/// signature for the one hole there without one, when exactly one scanned wormhole is nobody's:
/// the hole's id, the signature, and whether it goes on the hole's own side.
pub fn probe_effects(holes: &[Wormhole], system: i64, scan: &[spai_core::wormholes::ScanSig], full: bool) -> (Vec<i64>, Option<(i64, String, bool)>) {
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
    let free: Vec<&spai_core::wormholes::ScanSig> = scan
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

pub const SHATTERED_COLOR: egui::Color32 = egui::Color32::from_rgb(0x9A, 0xD8, 0xF0);

/// Drops from `holes` those whose k-space end is not in `keep`, unless both ends are k-space (so
/// k-space to Pochven, which is k-space too). Returns how many were dropped at each end kept.
pub fn overview(holes: &mut Vec<Wormhole>, kspace: impl Fn(i64) -> bool, keep: &HashSet<i64>) -> HashMap<i64, usize> {
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
pub fn pill_id(pin: i64, exit: i64) -> i64 {
    -(pin * 100_000 + exit.rem_euclid(100_000))
}


/// The known hole behind signature `sig` in `system`, matched on its first three letters.
pub fn sig_hole<'a>(holes: &'a [Wormhole], system: i64, sig: &str) -> Option<&'a Wormhole> {
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
pub fn short_group(group: &str) -> &str {
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
pub struct RouteCache {
    pub key: u64,
    pub boxes: HashMap<i64, egui::Rect>,
    pub links: Vec<(i64, i64, bool)>,
    pub paths: Vec<Option<Vec<egui::Pos2>>>,
    pub partial: bool,
}

#[derive(Default)]
pub struct WhGraphView {
    pub table: bool,
    /// The signature browser instead of the map or the table.
    pub sig_browser: bool,
    pub pan: egui::Vec2,
    pub selected: Option<i64>,
    /// Loaded once; written through on each drop.
    pub dragged: Option<HashMap<i64, egui::Pos2>>,
    pub drag: Option<(i64, egui::Pos2)>,
    pub pin_query: String,
    pub zoom: f32,
    pub fit_pending: bool,
    /// Only this system and those a few holes from it.
    pub focus: Option<i64>,
    pub focus_depth: u8,
    pub focus_dragged: HashMap<i64, egui::Pos2>,
    pub side_tab: SideTab,
    pub sigs: Option<(i64, Vec<spai_core::wormholes::SystemSig>)>,
    pub sigs_pruned: bool,
    pub sig_note: Option<String>,
    pub keep_missing: bool,
    /// The last routes worked out, and what they were worked out for.
    pub route_cache: Option<RouteCache>,
    /// Gate jumps from each pinned system, for joining it to the focused chain.
    pub gate_dist: HashMap<i64, HashMap<i64, u32>>,
    /// The last auto layout and what it was worked out from: the layered one is too slow to
    /// redo every frame.
    pub layout_cache: Option<(u64, Vec<(i64, Option<i64>, egui::Pos2)>)>,
    /// The canvas as last drawn, whose shape the chains are packed to.
    pub canvas: Option<egui::Rect>,
    /// Holes whose signature a probe scan no longer lists, waiting on the user: the system, and
    /// each hole with whether it is ticked to go.
    pub gone: Option<(i64, Vec<(i64, bool)>)>,
}

impl WhGraphView {
    pub fn depth(&self) -> u8 {
        if self.focus_depth == 0 { 2 } else { self.focus_depth }
    }

    pub fn set_focus(&mut self, focus: Option<i64>) {
        self.focus = focus;
        self.focus_dragged.clear();
        self.gate_dist.clear();
        self.fit_pending = true;
    }

    pub fn fit(&mut self, pos: &HashMap<i64, egui::Pos2>, rect: egui::Rect) {
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
    pub fn forget_sigs(&mut self) {
        self.sigs = None;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn show_sigs(&mut self, system: i64, sigs: Vec<spai_core::wormholes::SystemSig>) {
        self.side_tab = SideTab::Sigs;
        self.sigs = Some((system, sigs));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    /// The top left at `zoom`, whatever a fit asked for.
    #[cfg(any(test, feature = "test-support"))]
    pub fn hold_view(&mut self, zoom: f32) {
        self.zoom = zoom;
        self.pan = egui::vec2(24.0, 24.0);
        self.fit_pending = false;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn pan_to(&mut self, pan: egui::Vec2) {
        self.pan = pan;
    }

    /// Routes for `links`, worked out again only when a box moves or the links change.
    pub fn routes(
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

    pub fn zoom_by(&mut self, factor: f32, rect: egui::Rect) {
        let old = self.zoom;
        let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let rel = rect.center() - (rect.min + self.pan);
        self.pan += rel * (1.0 - new / old);
        self.zoom = new;
    }
}

/// Around the systems on the map, as far as the view may scroll.
pub const CANVAS_MARGIN: f32 = 400.0;

/// The map's extent: every box, plus [`CANVAS_MARGIN`] around them. Fixed while the map is, so the
/// minimap keeps its scale as the view moves.
pub fn canvas_of(pos: &HashMap<i64, egui::Pos2>) -> Option<egui::Rect> {
    let content = pos.iter().fold(egui::Rect::NOTHING, |b, (id, p)| b.union(egui::Rect::from_min_size(*p, node_size(*id))));
    content.is_positive().then(|| content.expand(CANVAS_MARGIN))
}

/// Centres for a label of `size` along the straight leg from `from` towards `to`, nearest `from`
/// first, each keeping the whole label on the leg.
pub fn along(from: egui::Pos2, to: egui::Pos2, size: egui::Vec2) -> Vec<egui::Pos2> {
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
pub fn first_free(
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
pub fn components(edges: &[(i64, i64)]) -> Vec<Vec<i64>> {
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
pub fn within(holes: &[Wormhole], from: i64, depth: u8) -> HashSet<i64> {
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
pub fn route_all(
    boxes: &HashMap<i64, egui::Rect>,
    links: &[(i64, i64, bool)],
    parent: &HashMap<i64, i64>,
) -> Vec<Option<Vec<egui::Pos2>>> {
    route_some(boxes, links, parent, &vec![None; links.len()])
}

/// [`route_all`] with the links that have a line in `keep` left on it: only the others are routed,
/// around the kept ones.
pub fn route_some(
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
pub const LANE: f32 = 7.0;

/// One straight piece of a routed line: path `path`, points `at` and `at + 1`.
pub struct Piece {
    pub path: usize,
    pub at: usize,
    pub upright: bool,
    /// x for an upright piece, y for a flat one.
    pub coord: f32,
    pub lo: f32,
    pub hi: f32,
}

/// Spreads lines that run on top of each other into parallel lanes. Where they part, the order of
/// the lanes follows where each turns, so they part without crossing.
pub fn nudge(paths: &mut [Option<Vec<egui::Pos2>>], boxes: &HashMap<i64, egui::Rect>) {
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
pub fn fanned_paths(boxes: &HashMap<i64, egui::Rect>, links: &[(i64, i64, bool)]) -> HashMap<usize, Vec<egui::Pos2>> {
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

pub fn candidates(a: egui::Rect, b: egui::Rect, a_hub: bool) -> Vec<Vec<egui::Pos2>> {
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
pub struct Routed {
    pub path: Vec<egui::Pos2>,
    pub a: i64,
    pub b: i64,
    pub hole: bool,
    pub bbox: egui::Rect,
}

/// What `path` costs, or infinity once it is sure to be over `bound`.
#[allow(clippy::too_many_arguments)]
pub fn route_cost(
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
pub fn crosses(a0: egui::Pos2, a1: egui::Pos2, b0: egui::Pos2, b1: egui::Pos2) -> bool {
    let inside = |v: f32, p: f32, q: f32| v > p.min(q) + 0.5 && v < p.max(q) - 0.5;
    let cross = |u0: egui::Pos2, u1: egui::Pos2, f0: egui::Pos2, f1: egui::Pos2| {
        (u0.x - u1.x).abs() < 0.5 && (f0.y - f1.y).abs() < 0.5 && inside(u0.x, f0.x, f1.x) && inside(f0.y, u0.y, u1.y)
    };
    cross(a0, a1, b0, b1) || cross(b0, b1, a0, a1)
}

/// How long two axis-aligned segments run along each other.
pub fn overlap(a0: egui::Pos2, a1: egui::Pos2, b0: egui::Pos2, b1: egui::Pos2) -> f32 {
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
pub fn walk_all(path: &[egui::Pos2], from_start: bool, size: egui::Vec2) -> Vec<egui::Pos2> {
    let pts: Vec<egui::Pos2> = if from_start { path.to_vec() } else { path.iter().rev().copied().collect() };
    pts.windows(2).flat_map(|s| along(s[0], s[1], size)).collect()
}

/// [`first_free`] with no fallback: only spots off every other line.
pub fn first_free_strict(
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
pub fn simplify(path: Vec<egui::Pos2>) -> Vec<egui::Pos2> {
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
pub fn rounded(path: &[egui::Pos2], radius: f32) -> Vec<egui::Pos2> {
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
#[cfg(any(test, feature = "test-support"))]
pub fn auto_layout(edges: &[(i64, i64)], score: impl Fn(i64) -> i64) -> Vec<(i64, Option<i64>, egui::Pos2)> {
    crate::wh_layout::layout(edges, &[], score, crate::wh_layout::Opts::default())
}

/// The auto layout with systems already on the map (placed before, or dragged) kept where they
/// are. A new system goes where the layout would put it relative to its parent, or to the nearest
/// free spot from there: above or below first, then a column further out.
#[cfg(any(test, feature = "test-support"))]
pub fn place(auto: &[(i64, Option<i64>, egui::Pos2)], dragged: &HashMap<i64, egui::Pos2>) -> HashMap<i64, egui::Pos2> {
    place_with(auto, dragged, crate::wh_layout::Opts::default())
}

/// [`place`] for a layout growing the way `opts` says: a new system looks beside its spot first,
/// then a level further out.
pub fn place_with(auto: &[(i64, Option<i64>, egui::Pos2)], dragged: &HashMap<i64, egui::Pos2>, opts: crate::wh_layout::Opts) -> HashMap<i64, egui::Pos2> {
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
                egui::pos2(0.0, bottom + crate::wh_layout::CHAIN_GAP)
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

pub fn snap(p: egui::Pos2) -> egui::Pos2 {
    egui::pos2((p.x / GRID).round() * GRID, (p.y / GRID).round() * GRID)
}

pub fn class_color(class: Class, security: f64) -> egui::Color32 {
    use egui::Color32 as C;
    match class {
        Class::W(1..=3) => C::from_rgb(0x4f, 0x9c, 0xff),
        Class::W(4 | 5) => C::from_rgb(0xff, 0x9b, 0x3d),
        Class::W(6) => C::from_rgb(0xff, 0x55, 0x55),
        Class::W(_) => C::from_rgb(0xc0, 0x7c, 0xff),
        Class::Thera => C::from_rgb(0xe6, 0xd0, 0x4a),
        Class::Drifter(_) => C::from_rgb(0xff, 0x6e, 0xc7),
        Class::Pochven => C::from_rgb(0xd0, 0x30, 0x30),
        _ => crate::colors::security_color(security),
    }
}

pub fn effect_color(effect: &str) -> egui::Color32 {
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
pub fn life_badge(w: &Wormhole, now: i64, visuals: &egui::Visuals) -> Option<(String, egui::Color32)> {
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
pub fn time_color(t: TimeLeft) -> egui::Color32 {
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
pub fn stroke_hole(painter: &egui::Painter, line: &[egui::Pos2], stroke: egui::Stroke, mass: Option<Mass>) {
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
pub fn wh_route_toggle(ui: &mut egui::Ui, off: bool) -> bool {
    let text = if off { egui::RichText::new(icon::PROHIBIT).color(crate::colors::HOSTILE) } else { egui::RichText::new(icon::PROHIBIT) };
    ui.small_button(text)
        .on_hover_text(if off { "Switched off for routes: click to let routes use it" } else { "Do not use this hole in routes" })
        .clicked()
}

/// `c` with most of its colour taken out, for what routes may not use.
pub fn desaturate(c: egui::Color32) -> egui::Color32 {
    let grey = 0.3 * c.r() as f32 + 0.59 * c.g() as f32 + 0.11 * c.b() as f32;
    let mix = |v: u8| (v as f32 * 0.15 + grey * 0.85 * 0.8).round() as u8;
    egui::Color32::from_rgba_unmultiplied(mix(c.r()), mix(c.g()), mix(c.b()), c.a())
}

pub fn desaturate_stroke(s: egui::Stroke) -> egui::Stroke {
    egui::Stroke::new(s.width, desaturate(s.color))
}

/// Gap in a switched-off line, in screen pixels, that its slash stands in.
pub const SLASH_GAP: f32 = 10.0;

/// `line` cut with a gap for its slash, and the cut's centre and direction. The cut goes in the
/// middle of the last leg, which a line fanning out of a shared trunk has to itself, or in the
/// middle of the whole line when that leg is short. `None` for a line too short to cut.
pub fn split_middle(line: &[egui::Pos2], gap: f32) -> Option<(Vec<egui::Pos2>, Vec<egui::Pos2>, egui::Pos2, egui::Vec2)> {
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
pub fn slashed(painter: &egui::Painter, line: &[egui::Pos2], stroke: egui::Stroke, draw: impl Fn(&egui::Painter, &[egui::Pos2], egui::Stroke)) {
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
pub fn mass_rank(m: Option<Mass>) -> u8 {
    match m {
        None | Some(Mass::Fresh) => 0,
        Some(Mass::Reduced) => 1,
        Some(Mass::Critical) => 2,
    }
}

/// `line` redrawn as a zigzag: a point every `step` pixels along it, pushed `amp` to either side
/// in turn. The ends stay put so it still meets its boxes.
pub fn zigzag(line: &[egui::Pos2], step: f32, amp: f32) -> Vec<egui::Pos2> {
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
pub fn system_tag(c: Class, security: f64) -> String {
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

pub fn drifter_color() -> egui::Color32 {
    class_color(Class::Drifter(14), -1.0)
}

/// A box's fill tinted with the drifter colour, enough to pick out at a glance.
pub fn drifter_fill(base: egui::Color32) -> egui::Color32 {
    let d = drifter_color();
    let mix = |a: u8, b: u8| (a as f32 * 0.78 + b as f32 * 0.22).round() as u8;
    egui::Color32::from_rgb(mix(base.r(), d.r()), mix(base.g(), d.g()), mix(base.b(), d.b()))
}

/// A pinned system this few gate jumps from a chain is close enough to stand out: green under
/// five, yellow under ten.
pub fn close_color(jumps: u32) -> Option<egui::Color32> {
    match jumps {
        0..5 => Some(egui::Color32::from_rgb(0x5F, 0xD0, 0x6E)),
        5..10 => Some(egui::Color32::from_rgb(0xF2, 0xD0, 0x4A)),
        _ => None,
    }
}

pub fn mass_color(m: Option<Mass>) -> egui::Color32 {
    use egui::Color32 as C;
    match m {
        None => C::from_rgb(0x7d, 0x8a, 0x99),
        Some(Mass::Fresh) => C::from_rgb(0x6f, 0xc2, 0x76),
        Some(Mass::Reduced) => C::from_rgb(0xf0, 0xa0, 0x30),
        Some(Mass::Critical) => C::from_rgb(0xff, 0x4a, 0x4a),
    }
}

pub fn dist_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-3)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use spai_core::wormholes::Life;

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
        let auto = crate::wh_layout::layout(&edges, &[], |_| 0, crate::wh_layout::Opts::default());
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
    fn a_probe_scan_finds_gone_holes_and_names_a_lone_bare_one() {
        use spai_core::wormholes::ScanSig;
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

