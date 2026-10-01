//! The wormhole map tab as both apps draw it. Everything app-specific (where holes and settings
//! live, what opening a system means, the side panel) goes through [`WhHost`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui_phosphor::regular as icon;
use spai_core::geo::Systems;
use spai_core::whdata::{self, Class};
use spai_core::wormholes::{time_left, Mass, TimeLeft, Wormhole};

use crate::wh_graph::*;

/// The map's settings, as the host keeps them.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WhPrefs {
    /// Systems joined to the chains by gates, by name.
    pub route_pins: Vec<String>,
    pub pin_jumps: u32,
    pub layout_style: String,
    pub layout_pack: bool,
    pub minimap: bool,
    pub legend_open: bool,
}

/// What the map needs from the app around it.
pub trait WhHost {
    fn systems(&self) -> Option<Arc<Systems>>;
    /// The holes to draw, after the user's filters.
    fn holes(&self, now: i64) -> Vec<Wormhole>;
    /// The user's characters: name to (system, online).
    fn characters(&self) -> HashMap<String, (i64, bool)>;
    fn prefs(&self) -> WhPrefs;
    /// Called when the map changed a setting.
    fn set_prefs(&mut self, prefs: WhPrefs);
    /// Where the user left systems, in map units.
    fn saved_layout(&self) -> HashMap<i64, egui::Pos2>;
    fn save_layout(&mut self, id: i64, at: egui::Pos2);
    fn clear_layout(&mut self);
    /// Whether the route filter keeps this hole off routes.
    fn blocked(&self, w: &Wormhole, now: i64) -> bool;
    /// Whether the user switched this hole off for routes.
    fn disabled(&self, w: &Wormhole) -> bool;
    fn disabled_count(&self) -> usize;
    /// Systems whose holes routes leave out.
    fn disabled_systems(&self) -> Vec<i64>;
    fn clear_disabled(&mut self);
    /// The sharing group a hole came from, by name.
    fn group_name(&self, uid: &str) -> Option<String>;
    fn open_system(&mut self, id: i64);
    /// More entries for a system's right-click menu.
    fn system_menu(&mut self, ui: &mut egui::Ui, id: i64);
    /// More controls at the end of the toolbar.
    fn toolbar(&mut self, view: &mut WhGraphView, ui: &mut egui::Ui);
    /// Controls beside Tidy.
    fn after_tidy(&mut self, _view: &mut WhGraphView, _ui: &mut egui::Ui) {}
    /// The selected system's panel on the right.
    fn side_panel(
        &mut self,
        view: &mut WhGraphView,
        ui: &mut egui::Ui,
        geo: &Arc<Systems>,
        holes: &[Wormhole],
        chars: &HashMap<String, (i64, bool)>,
        now: i64,
    );
}

fn layout_opts(view: &WhGraphView, p: &WhPrefs) -> crate::wh_layout::Opts {
    // The aspect in steps of a tenth, so resizing the window by a few pixels keeps the layout.
    let aspect = view.canvas.map(|r| (r.aspect_ratio() * 10.0).round() / 10.0).filter(|a| a.is_finite() && *a > 0.0).unwrap_or(1.6);
    crate::wh_layout::Opts { style: crate::wh_layout::Style::from_code(&p.layout_style), aspect: p.layout_pack.then_some(aspect) }
}

/// Forgets where systems were dragged and lays the whole map out afresh.
pub fn reset_layout(view: &mut WhGraphView, host: &mut impl WhHost) {
    host.clear_layout();
    view.dragged = Some(HashMap::new());
    view.pan = egui::Vec2::ZERO;
}

/// [`reset_layout`], and shows all of it.
pub fn tidy_layout(view: &mut WhGraphView, host: &mut impl WhHost) {
    reset_layout(view, host);
    view.focus_dragged.clear();
    view.layout_cache = None;
    view.fit_pending = true;
}

/// The wormhole map: holes as a graph of systems, laid out, routed and drawn, with its toolbar,
/// system list and minimap. `host` is the app around it.
pub fn show(view: &mut WhGraphView, host: &mut impl WhHost, ui: &mut egui::Ui) {
    let Some(geo) = host.systems() else {
        ui.label(egui::RichText::new("The system map is still loading.").weak());
        return;
    };
    let now = spai_core::clock::utc().timestamp();
    let mut holes: Vec<Wormhole> = host.holes(now);
    // The side panel lists every hole of the selected system, drawn on the map or not.
    let all_holes = holes.clone();
    list(view, host, ui, &geo, &holes);
    let focus = view.focus;
    if let Some(f) = focus {
        let near = within(&holes, f, view.depth());
        holes.retain(|w| near.contains(&w.system_id) && w.dest_system_id.is_none_or(|b| near.contains(&b)));
    }
    let mut edges: Vec<(i64, i64)> = holes.iter().filter_map(|w| Some((w.system_id, w.dest_system_id?))).collect();
    // Pinned systems join the holes by gates, so they tie the chains together.
    // (exit, where the pin is met by gates, jumps, the pin)
    let mut gate_links: Vec<(i64, i64, u32, i64)> = Vec::new();
    // The drifter systems are hubs of their own: on the map and joined to the chains like
    // pinned systems, though the Routes panel lists only what the user pins.
    let drifters: Vec<i64> = whdata::DRIFTERS.iter().map(|d| d.2).filter(|id| geo.info_of(*id).is_some()).collect();
    let chars: HashMap<String, (i64, bool)> = host.characters();
    let p0 = host.prefs();
    let mut pins: Vec<i64> = p0.route_pins.iter().filter_map(|p| geo.lookup(p).map(|i| i.id)).collect();
    let user_pins = pins.clone();
    for d in &drifters {
        if !pins.contains(d) {
            pins.push(*d);
        }
    }
    let kspace = |id: &i64| geo.info_of(*id).is_some_and(|i| whdata::class_of(*id, i.security, &i.region).is_kspace());
    // Gate jumps under which a pinned system counts as near a cluster and joins it. The
    // colours ([`close_color`]) still only mark the nearest, under 10.
    let near_jumps = p0.pin_jumps.clamp(1, WH_PIN_JUMPS_MAX);
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
        view.gate_dist.entry(*a).or_insert_with(|| geo.distances_from(*a, WH_PIN_JUMPS_MAX));
    }
    let dist = |from: i64, exit: i64| view.gate_dist.get(&from).and_then(|d| d.get(&exit).copied());
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
    gate_links.retain(|(_, _, n, _)| *n < near_jumps);
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
                .filter(|(_, n)| *n < near_jumps);
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
    let opts = layout_opts(view, &p0);
    let auto = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut ids: Vec<i64> = edges.iter().flat_map(|(a, b)| [*a, *b]).chain(alone.iter().copied()).collect();
        ids.sort_unstable();
        ids.dedup();
        let scored: Vec<(i64, i64)> = ids.iter().map(|id| (*id, score(*id))).collect();
        (&edges, &alone, &scored, opts.style, opts.aspect.map(f32::to_bits)).hash(&mut h);
        let key = h.finish();
        match &view.layout_cache {
            Some((k, a)) if *k == key => a.clone(),
            _ => {
                let a = crate::wh_layout::layout(&edges, &alone, &score, opts);
                view.layout_cache = Some((key, a.clone()));
                a
            }
        }
    };
    if view.dragged.is_none() {
        view.dragged = Some(host.saved_layout());
    }
    // A focused view is laid out around its system; moving things there is only for the moment.
    let mut pos = if focus.is_some() {
        place_with(&auto, &view.focus_dragged, opts)
    } else {
        place_with(&auto, view.dragged.as_ref().unwrap(), opts)
    };
    // Everything placed stays put from now on, so a system turning up (synced, detected or
    // added) never shuffles the ones already there. Remembered across restarts outside focus.
    {
        let known = if focus.is_some() { &mut view.focus_dragged } else { view.dragged.get_or_insert_default() };
        let fresh: Vec<(i64, egui::Pos2)> = pos.iter().filter(|(id, _)| !known.contains_key(id)).map(|(id, p)| (*id, *p)).collect();
        for (id, p) in &fresh {
            known.insert(*id, *p);
        }
        if focus.is_none() {
            for (id, p) in &fresh {
                host.save_layout(*id, *p);
            }
        }
    }
    if let Some((id, p)) = view.drag {
        pos.insert(id, p);
    }

    // Holes whose far side is unknown, by the system they are in.
    let mut open: HashMap<i64, usize> = HashMap::new();
    for w in holes.iter().filter(|w| w.dest_system_id.is_none()) {
        *open.entry(w.system_id).or_default() += 1;
    }

    host.side_panel(view, ui, &geo, &all_holes, &chars, now);
    // Read again: the side panel may have changed them.
    let mut prefs = host.prefs();
    let prefs_was = prefs.clone();

    let mut focus_on: Option<Option<i64>> = None;
    // The view's own controls, a row above the canvas.
    let focus_name = focus.and_then(|f| geo.info_of(f)).map(|i| i.name.clone());
    let mut zoom_step: Option<f32> = None;
    let mut tidy = false;
    {
        {
            ui.horizontal_wrapped(|ui| {
                    use crate::widgets::SteadySelect as _;
                    ui.add_space(8.0);
                    if ui.button(icon::MINUS).on_hover_text("Zoom out").clicked() {
                        zoom_step = Some(1.0 / 1.25);
                    }
                    ui.label(format!("{:.0}%", view.zoom * 100.0));
                    if ui.button(icon::PLUS).on_hover_text("Zoom in").clicked() {
                        zoom_step = Some(1.25);
                    }
                    if ui.button(format!("{}  Fit", icon::CORNERS_OUT)).on_hover_text("Show everything").clicked() {
                        view.fit_pending = true;
                    }
                    if ui
                        .button(format!("{}  Tidy", icon::TREE_STRUCTURE))
                        .on_hover_text("Lay the whole map out afresh, forgetting where systems were dragged")
                        .clicked()
                    {
                        tidy = true;
                    }
                    host.after_tidy(view, ui);
                    ui.menu_button(format!("{}  Layout", icon::CARET_DOWN), |ui| {
                        use crate::wh_layout::Style;
                        let style = Style::from_code(&prefs.layout_style);
                        let pick = |ui: &mut egui::Ui, on: bool, label: &str, hint: &str| ui.menu_label(on, label).on_hover_text(hint).clicked();
                        ui.label(egui::RichText::new("Style").weak());
                        if pick(ui, style == Style::Tree, "Tree", "Each chain as a compact tree from its most important system") {
                            prefs.layout_style = Style::Tree.code().to_owned();
                            tidy = true;
                        }
                        if pick(ui, style == Style::Layered, "Layered", "Fewer crossing lines where holes close loops") {
                            prefs.layout_style = Style::Layered.code().to_owned();
                            tidy = true;
                        }
                        ui.separator();
                        ui.label(egui::RichText::new("Separate chains").weak());
                        if pick(ui, prefs.layout_pack, "Packed to the window", "Side by side in rows, to fill the window's shape") {
                            prefs.layout_pack = true;
                            tidy = true;
                        }
                        if pick(ui, !prefs.layout_pack, "In one line", "One after another") {
                            prefs.layout_pack = false;
                            tidy = true;
                        }
                        ui.separator();
                        ui.checkbox(&mut prefs.minimap, "Minimap");
                        if tidy {
                            ui.close();
                        }
                    });
                    if ui.menu_label(prefs.legend_open, format!("{}  Legend", icon::BOOK_OPEN)).clicked() {
                        prefs.legend_open = !prefs.legend_open;
                    }
                    host.toolbar(view, ui);
                    if let Some(name) = &focus_name {
                        ui.separator();
                        ui.label(format!("{}  {name}", icon::CROSSHAIR));
                        for d in 1..=3u8 {
                            if ui
                                .menu_label(view.depth() == d, format!("{d}"))
                                .on_hover_text(format!("Systems up to {d} hole{} away", if d == 1 { "" } else { "s" }))
                                .clicked()
                            {
                                view.focus_depth = d;
                                view.fit_pending = true;
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
        tidy_layout(view, host);
    }
    if prefs.legend_open {
        legend(ui);
    }

    let rect = ui.available_rect_before_wrap();
    view.canvas = Some(rect);
    if let Some(f) = zoom_step {
        view.zoom_by(f, rect);
    }
    let bg = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    if view.zoom <= 0.0 {
        // First look: everything if it stays readable, else the top left at full size.
        view.fit(&pos, rect);
        if view.zoom < 0.6 {
            view.zoom = 1.0;
            view.pan = egui::vec2(24.0, 24.0);
        }
    }
    if std::mem::take(&mut view.fit_pending) {
        view.fit(&pos, rect);
    }
    if bg.dragged() && view.drag.is_none() {
        view.pan += bg.drag_delta();
    }
    if bg.clicked() {
        view.selected = None;
    }
    if bg.hovered() || ui.rect_contains_pointer(rect) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let old = view.zoom;
            let new = (old * (scroll * 0.003).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
            if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                let rel = m - (rect.min + view.pan);
                view.pan += rel * (1.0 - new / old);
            }
            view.zoom = new;
        }
    }
    // The view's centre stays over the systems, so the map cannot be scrolled off into nothing.
    if let Some(canvas) = canvas_of(&pos) {
        let content = canvas.shrink(CANVAS_MARGIN);
        let centre = ((rect.center() - rect.min - view.pan) / view.zoom).to_pos2();
        let c = centre.clamp(content.min, content.max);
        view.pan = rect.center() - rect.min - c.to_vec2() * view.zoom;
    }
    let zoom = view.zoom;
    let origin = rect.min + view.pan;
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
    let off_systems: HashSet<i64> = host.disabled_systems().into_iter().collect();
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
    let routes = view.routes(&world, &links, &parent);
    let hole_paths: Vec<Option<Vec<egui::Pos2>>> = routes[..holes.len()].to_vec();
    let screen = |path: &[egui::Pos2]| rounded(&path.iter().map(|p| to_screen(*p)).collect::<Vec<_>>(), 10.0 * zoom);
    let hit = |line: &[egui::Pos2]| pointer.is_some_and(|p| line.windows(2).any(|s| dist_to_segment(p, s[0], s[1]) < 6.0));

    #[cfg(any(test, feature = "test-support"))]
    EDGE_PROBE.with(|p| p.borrow_mut().clear());
    // Every line first, so no line is ever drawn over a label. Solid ones go down first and
    // each line on a thin band of the background: where lines share a stretch, a dashed one on
    // top keeps its gaps instead of a solid one beneath showing through them.
    let blocked: HashSet<i64> = holes.iter().filter(|w| host.blocked(w, now)).map(|w| w.id).collect();
    let mut order: Vec<usize> = (0..holes.len()).collect();
    order.sort_by_key(|&i| mass_rank(holes[i].mass));
    for wi in order {
        let w = &holes[wi];
        let Some(path) = &hole_paths[wi] else { continue };
        let line = screen(path);
        #[cfg(any(test, feature = "test-support"))]
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
                view.drag = Some((id, p));
            }
            if resp.dragged() {
                if let Some((_, at)) = &mut view.drag {
                    *at += resp.drag_delta() / zoom;
                }
            }
            if resp.drag_stopped() {
                if let Some((did, at)) = view.drag.take() {
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
                format!("{}: pinned, no cluster within {} jumps", info.name, prefs.pin_jumps)
            } else {
                format!("{}: pinned, {n} jumps by gate and bridge from this cluster's nearest exit", info.name)
            });
            continue;
        }
        let Some(info) = geo.info_of(id) else { continue };
        let c = whdata::class_of(id, info.security, &info.region);
        let resp = ui.interact(r.intersect(rect), ui.id().with(("wh_node", id)), egui::Sense::click_and_drag());
        if resp.drag_started() {
            view.drag = Some((id, p));
        }
        if resp.dragged() {
            if let Some((_, at)) = &mut view.drag {
                *at += resp.drag_delta() / zoom;
            }
        }
        if resp.drag_stopped() {
            if let Some((did, at)) = view.drag.take() {
                drop = Some((did, snap(at)));
            }
        }
        if resp.clicked() {
            clicked = Some(id);
        }
        if resp.double_clicked() {
            focus_on = Some(Some(id));
        }
        let pinned = prefs.route_pins.iter().any(|n| n.eq_ignore_ascii_case(&info.name));
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
            host.system_menu(ui, id);
        });

        let jsys = whdata::jsystem(id);
        let effect = jsys.and_then(|j| j.effect.as_deref());
        let selected = view.selected == Some(id);
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
            "{} {} {} {} {}",
            name(w.system_id),
            w.signature.as_deref().unwrap_or(""),
            icon::ARROW_RIGHT,
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
        let (added, edited) = crate::wh_graph::who_lines(w, now);
        tip.push_str(&format!("\n{added}"));
        if let Some(e) = edited {
            tip.push_str(&format!("\n{e}"));
        }
        tip.push_str(&format!("\nSource: {}", w.source.label()));
        if let Some(name) = host.group_name(&w.uid) {
            tip.push_str(&format!("\nShared in {name}"));
        }
        if host.disabled(w) {
            tip.push_str("\nSwitched off for routes");
        } else if blocked.contains(&w.id) {
            tip.push_str("\nOff routes: the filter hides it");
        }
        line_tip(ui, pointer, tip);
    }

    if let Some((id, at)) = drop {
        if focus.is_some() {
            view.focus_dragged.insert(id, at);
        } else {
            host.save_layout(id, at);
            view.dragged.get_or_insert_default().insert(id, at);
        }
    }
    if let Some(id) = clicked {
        view.selected = Some(id);
    }
    if let Some(f) = focus_on {
        view.set_focus(f);
        if f.is_some() {
            view.selected = f;
        }
    }
    if prefs.minimap && !pos.is_empty() {
        minimap(view, ui, rect, &pos, &geo);
    }
    if let Some(id) = opened {
        host.open_system(id);
    }
    if let Some(name) = pin {
        match prefs.route_pins.iter().position(|n| n.eq_ignore_ascii_case(&name)) {
            Some(i) => {
                prefs.route_pins.remove(i);
            }
            None => prefs.route_pins.push(name),
        }
    }
    if prefs != prefs_was {
        host.set_prefs(prefs);
    }
}

/// The whole map in small in the canvas corner, with the part on screen outlined. Clicking or
/// dragging in it moves the view there.
fn minimap(wh: &mut WhGraphView, ui: &mut egui::Ui, rect: egui::Rect, pos: &HashMap<i64, egui::Pos2>, geo: &Systems) {
    const MAX: egui::Vec2 = egui::vec2(200.0, 130.0);
    let zoom = wh.zoom;
    let seen = egui::Rect::from_min_size((-wh.pan / zoom).to_pos2(), rect.size() / zoom);
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
        wh.pan = rect.center() - rect.min - target.to_vec2() * zoom;
        ui.ctx().request_repaint();
    }
    resp.on_hover_text("Click or drag to move the view");
}

/// Every J-space system with a known connection, to jump the map to it.
fn list(view: &mut WhGraphView, host: &mut impl WhHost, ui: &mut egui::Ui, geo: &Systems, holes: &[Wormhole]) {
    use crate::widgets::SteadySelect as _;
    let mut links: HashMap<i64, usize> = HashMap::new();
    for w in holes {
        let Some(b) = w.dest_system_id else { continue };
        for id in [w.system_id, b] {
            *links.entry(id).or_default() += 1;
        }
    }
    let mut rows: Vec<(Class, &spai_core::geo::SystemInfo, usize)> = links
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
    let mut chars: Vec<(String, i64)> = host.characters().into_iter().map(|(n, (s, _))| (n, s)).collect();
    chars.sort();
    let disabled = host.disabled_count();
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
                    let on = view.selected == Some(*sys);
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
                    let on = view.focus == Some(info.id);
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
        view.set_focus(Some(id));
        view.selected = Some(id);
    }
    if let Some(id) = open_sys {
        view.selected = Some(id);
    }
    if clear {
        host.clear_disabled();
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
