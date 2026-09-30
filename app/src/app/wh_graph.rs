//! The wormhole map: known holes as a graph of J-space and the k-space systems they open into.
//! Laid out as a tree per chain, from the side our characters are on; a system the user dragged
//! keeps its place, and whatever grows off it later is laid out relative to it.

use std::collections::{HashMap, HashSet};

use egui_phosphor::regular as icon;

use super::SpaiApp;
use crate::whdata::{self, Class};
use crate::wormholes::{time_left, Mass, TimeLeft, Wormhole};
pub(crate) use spai_ui::wh_graph::*;

/// Wide enough that at [`MIN_ZOOM`] a name still fits its scaled box, so no box grows past its
/// place when zoomed out.

// Where each hole's line was drawn in the last frame, by hole id, for tests that hover one.
#[cfg(test)]
thread_local! {
    pub(crate) static EDGE_PROBE: std::cell::RefCell<Vec<(i64, Vec<egui::Pos2>)>> = const { std::cell::RefCell::new(Vec::new()) };
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
        let now = crate::clock::utc().timestamp();
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
        // Gate jumps under which a pinned system counts as near a cluster and joins it. The
        // colours ([`close_color`]) still only mark the nearest, under 10.
        let near_jumps = self.settings.wh_pin_jumps.clamp(1, crate::settings::WH_PIN_JUMPS_MAX);
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
            self.wh_graph.gate_dist.entry(*a).or_insert_with(|| geo.distances_from(*a, crate::settings::WH_PIN_JUMPS_MAX));
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
                        self.track_scanner_toggle(ui);
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
                    format!("{}: pinned, no cluster within {} jumps", info.name, self.settings.wh_pin_jumps)
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
        let mut drop_sig: Option<crate::store::SystemSig> = None;
        let mut new_hole: Option<String> = None;
        let mut toggle: Option<String> = None;
        let mut clear_filter = false;
        // The filter narrows the map and the Info list, never what a signature is known to lead to.
        let every: Vec<Wormhole> = self.wh_cache.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).cloned().collect();
        let hidden = every.len().saturating_sub(holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).count());
        egui::Panel::right("wh_graph_side").resizable(true).default_size(320.0).show_inside(ui, |ui| {
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
                // Equal thirds while every label fits in one; a label wider than its third would
                // widen the panel, which widens the thirds, frame after frame.
                let w = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
                let font = egui::TextStyle::Button.resolve(ui.style());
                let pad = 2.0 * ui.spacing().button_padding.x;
                let fits = tabs.iter().all(|(_, l)| ui.painter().layout_no_wrap(l.clone(), font.clone(), egui::Color32::WHITE).size().x + pad <= w);
                ui.horizontal(|ui| {
                    use crate::app::SteadySelect as _;
                    for (t, label) in tabs {
                        let on = self.wh_graph.side_tab == t;
                        let r = if fits { ui.menu_label_sized([w, 24.0], on, label) } else { ui.menu_label(on, label) };
                        if r.clicked() {
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
                    ui.label(egui::RichText::new(if hidden > 0 { "None shown" } else { "None known" }).weak());
                }
                if hidden > 0 {
                    ui.horizontal(|ui| {
                        let text = if hidden == 1 { "1 connection hidden by the filter".to_owned() } else { format!("{hidden} connections hidden by the filter") };
                        ui.label(egui::RichText::new(text).color(crate::theme::standing::WARNING));
                        if ui.button(format!("{}  Clear filter", icon::FUNNEL_X)).clicked() {
                            clear_filter = true;
                        }
                    });
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
                    self.sig_undo_button(ui, icon::ARROW_COUNTER_CLOCKWISE);
                });
                if let Some(t) = ui.input(|i| {
                    i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None })
                }) {
                    if ui.ui_contains_pointer() && ui.memory(|m| m.focused().is_none()) {
                        paste = Some(Some(t));
                    }
                }
                // Always one line, so a paste never pushes the list down: the count, then what the
                // last paste did.
                let summary = match (&self.wh_graph.sig_note, sigs.len()) {
                    (_, 0) => "No signatures pasted for this system".to_owned(),
                    (Some(note), n) => format!("{n} signatures \u{b7} {note}"),
                    (None, n) => format!("{n} signatures"),
                };
                ui.add(egui::Label::new(egui::RichText::new(summary).weak()).truncate());
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                // A table, not a grid: the Info column takes whatever width the panel has left.
                let row_h = ui.spacing().interact_size.y + 4.0;
                let text_w = |t: &str| ui.painter().layout_no_wrap(t.to_owned(), egui::TextStyle::Body.resolve(ui.style()), egui::Color32::WHITE).size().x;
                let id_w = text_w("MMM-888");
                let eve = self.settings.use_eve_time;
                // As wide as the times shown: a weekday only for the ones not from today.
                let found_head = egui::WidgetText::from(egui::RichText::new("Found").strong()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x;
                let found_w = sigs.iter().map(|sg| text_w(&super::sig_browser::found_at(sg.added_at, now, eve))).fold(found_head, f32::max);
                // The table takes one column gap more than it is given; in a resizable panel that
                // grows the panel a little every frame until it settles. Give it one gap less.
                let room = egui::vec2(ui.available_width() - ui.spacing().item_spacing.x, 0.0);
                let gutter = super::sig_browser::scrollbar_gutter(ui);
                let visuals = ui.visuals().clone();
                ui.allocate_ui(room, |ui| {
                egui_extras::TableBuilder::new(ui)
                    .id_salt("wh_graph_scan")
                    .striped(true)
                    .vscroll(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(egui_extras::Column::exact(id_w))
                    .column(egui_extras::Column::exact(found_w))
                    // Narrow enough for the panel as it is: pasting must never widen it.
                    .column(egui_extras::Column::remainder().at_least(40.0).clip(true))
                    // Room for remove and edit, whichever rows have them: measured, it drifts. Past
                    // them, room for the panel's scrollbar.
                    .column(egui_extras::Column::exact(76.0 + gutter))
                    .header(row_h, |mut header| {
                        for h in ["Id", "Found", "Info", ""] {
                            header.col(|ui| {
                                ui.label(egui::RichText::new(h).strong());
                            });
                        }
                    })
                    .body(|mut body| {
                    for sg in &sigs {
                        body.row(row_h, |mut row| {
                        let anomaly = super::sig_browser::is_anomaly(sg);
                        let aged = super::sig_browser::age_color(&visuals, now, sg.updated_at);
                        row.col(|ui| {
                            let id = ui.label(match aged {
                                Some(c) => egui::RichText::new(&sg.sig).color(c),
                                None if anomaly => egui::RichText::new(&sg.sig).weak(),
                                None => egui::RichText::new(&sg.sig),
                            });
                            id.on_hover_text(format!(
                                "{}\nAdded {} ago by {}, last seen in a paste {} ago",
                                sg.kind,
                                super::human_ago(now - sg.added_at),
                                sg.who,
                                super::human_ago(now - sg.updated_at)
                            ));
                        });
                        row.col(|ui| {
                            let t = egui::RichText::new(super::sig_browser::found_at(sg.added_at, now, eve));
                            ui.label(match aged {
                                Some(c) => t.color(c),
                                None => t,
                            })
                            .on_hover_text(super::sig_browser::found_hover(sg.added_at, now, eve));
                        });
                        // A wormhole signature we know the far side of says where it goes.
                        let hole = sig_hole(&every, sel, &sg.sig);
                        row.col(|ui| {
                        // The group in front, short and grey, so the site's name gets the room.
                        ui.label(egui::RichText::new(short_group(&sg.group)).weak());
                        match hole.map(|w| if w.system_id == sel { w.dest_system_id } else { Some(w.system_id) }) {
                            Some(Some(f)) => {
                                let code = hole.and_then(hole_code).map(|c| format!("{c} ")).unwrap_or_default();
                                let text = egui::RichText::new(format!("{code}{} {}", icon::ARROW_RIGHT, name(f))).color(ui.visuals().hyperlink_color);
                                if ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    select = Some(f);
                                }
                            }
                            // Known to be a hole, its far side only a kind of space so far.
                            Some(None) => {
                                let code = hole.and_then(hole_code).map(|c| format!("{c} ")).unwrap_or_default();
                                let dest = hole.map_or("?", |w| w.dest.label());
                                ui.add(egui::Label::new(format!("{code}{} {dest}", icon::ARROW_RIGHT)).truncate());
                            }
                            _ => {
                                let text = match unidentified_type(sel, &sg.name) {
                                    Some(code) => format!("{code} \u{b7} {}", sg.name),
                                    None if sg.name.is_empty() => "\u{2014}".to_owned(),
                                    None => sg.name.clone(),
                                };
                                let text = egui::RichText::new(text);
                                ui.add(egui::Label::new(if let Some(c) = aged { text.color(c) } else { text }).truncate());
                            }
                        }
                        });
                        row.col(|ui| {
                            // Remove first, so those line up whether or not an edit button follows.
                            if ui.small_button(icon::X).on_hover_text("Remove").clicked() {
                                drop_sig = Some(sg.clone());
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
            self.sig_delete(vec![(sel, sig)]);
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
        if clear_filter {
            self.settings.wh_filter = Default::default();
            if self.settings.wh_route_filtered {
                self.wh_routing_changed();
            } else {
                self.needs_save = true;
            }
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
                        s.prune_system_sigs(crate::clock::utc().timestamp() - 3 * 86_400);
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
        let (added, updated, removed) = store.merge_system_sigs(system, &scan, &who, now, full, None);
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
        let now = crate::clock::utc().timestamp();
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
        // A partial code typed before this scan: made whole now that one listed id fits it.
        let claimed = |full: &str, id: i64| {
            holes.iter().any(|o| {
                o.id != id
                    && ((o.system_id == system && o.signature.as_deref() == Some(full))
                        || (o.dest_system_id == Some(system) && o.dest_signature.as_deref() == Some(full)))
            })
        };
        for w in holes.iter().filter(|w| w.system_id == system || w.dest_system_id == Some(system)) {
            let near = w.system_id == system;
            let Some(typed) = (if near { &w.signature } else { &w.dest_signature }) else { continue };
            if crate::wormholes::is_sig_id(typed) {
                continue;
            }
            let Ok(Some(full)) = self.wh_complete_sig(Some(system), typed, Some(w.id)) else { continue };
            if claimed(&full, w.id) {
                continue;
            }
            if let Some(mut row) = store.wormhole_by_id(w.id) {
                if near {
                    row.signature = Some(full.clone());
                } else {
                    row.dest_signature = Some(full.clone());
                }
                row.updated_at = now;
                store.write_wormhole(&row);
                store.audit_wormhole(&row.uid, who, crate::wormholes::Source::Manual, &[("signature", full.clone())]);
                note.push_str(&format!(", {typed} is {full}"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
            origin: None,
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
}
