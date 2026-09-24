//! The tracked fleet's Map tab: where the pilots are, where they were at any recorded moment, and
//! for a capital save the titan route out of staging to the capital being worked.

use super::*;
use crate::fleets::movement::{Kind, MoveEvent, Seen, Via};

/// What the fleet page knows that the map needs, taken while the fleet lock is held.
pub(crate) struct FleetMapInput {
    pub(crate) fleet_id: String,
    /// Everyone in the fleet right now, by character id.
    pub(crate) pilots: std::collections::BTreeMap<i64, Seen>,
    pub(crate) fc_character: Option<i64>,
    pub(crate) cap_save: bool,
    /// Still running. A closed fleet is only its record.
    pub(crate) live: bool,
}

impl FleetMapInput {
    pub(crate) fn of(fleet: &crate::fleets::model::Fleet, comp: &crate::fleets::model::Composition) -> Self {
        let pilots = comp
            .members()
            .map(|m| {
                (
                    m.character_id,
                    Seen {
                        name: m.name.clone(),
                        system_id: m.solar_system_id,
                        ship_type_id: m.ship_type_id,
                        ship_name: m.ship_type_name.clone(),
                    },
                )
            })
            .collect();
        FleetMapInput {
            fleet_id: fleet.id.0.clone(),
            pilots,
            fc_character: comp.commander.as_ref().map(|m| m.character_id),
            cap_save: fleet.tag_ids.iter().any(|t| t.0 == crate::settings::CAPITAL_SAVE_TAG),
            live: fleet.closed_at.is_none(),
        }
    }
}

/// The map tab's own view, the recorded movement it replays and the route it last computed.
#[derive(Default)]
pub(crate) struct FleetMapView {
    zoom: f32,
    pan: egui::Vec2,
    /// The systems the view was fitted to; a new set refits it.
    fitted: Vec<i64>,
    route: Option<((i64, i64, crate::ansiblex::BridgeKey), Option<crate::web::route::RouteOption>)>,
    /// The fleet's recorded movement, and when it was read.
    events: Option<(String, std::time::Instant, std::sync::Arc<Vec<MoveEvent>>)>,
    /// The moment on the slider. None follows the live fleet.
    at: Option<i64>,
    /// The one pilot being followed.
    focus: Option<i64>,
    search: String,
    /// Systems expanded in the side list.
    open_systems: std::collections::HashSet<String>,
}

#[cfg(test)]
impl FleetMapView {
    /// A recorded history, a moment on the slider and a pilot to follow, as a scene wants them.
    pub(crate) fn seed(&mut self, fleet_id: &str, events: Vec<MoveEvent>, at: Option<i64>, focus: Option<i64>) {
        self.events = Some((fleet_id.to_owned(), std::time::Instant::now(), std::sync::Arc::new(events)));
        self.at = at;
        self.focus = focus;
    }
}

/// A capital asking for rescue, as the map needs it.
struct Ping {
    seq: u64,
    system: i64,
    label: String,
}

const FC_COL: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xC1, 0x07);
const MEMBER_COL: egui::Color32 = egui::Color32::from_rgb(0x4F, 0xC3, 0xF7);
const PING_COL: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x40, 0x40);
const STAGING_COL: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x7B, 0xE0);
const GATE_COL: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xB1, 0x34);
const JUMP_COL: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x7B, 0xE0);
const BRIDGE_COL: egui::Color32 = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);
const HOLE_COL: egui::Color32 = egui::Color32::from_rgb(0xB0, 0x7C, 0xE8);

/// How often the recorded movement is read again while the fleet is live.
const EVENTS_REREAD: std::time::Duration = std::time::Duration::from_secs(3);

/// EVE time, with the date once a record spans more than a day.
fn clock(t: i64, with_date: bool) -> String {
    let Some(d) = chrono::DateTime::from_timestamp(t, 0) else { return String::new() };
    d.format(if with_date { "%m-%d %H:%M:%S" } else { "%H:%M:%S" }).to_string()
}

/// One line of a timeline.
fn describe(e: &MoveEvent, name: &dyn Fn(i64) -> String, whose: bool) -> String {
    let who = if whose { format!("{} ", e.name) } else { String::new() };
    let via = e.via.map(|v| format!(" by {}", v.label())).unwrap_or_default();
    match e.kind {
        Kind::Join => format!("{who}joined in {} ({})", name(e.system_id), e.ship_name),
        Kind::Leave => format!("{who}left, last in {} ({})", name(e.system_id), e.ship_name),
        Kind::Move => format!("{who}{} \u{2192} {}{via}", name(e.from_system), name(e.system_id)),
        Kind::Ship => format!("{who}now in a {}", e.ship_name),
        Kind::Bulk if e.from_system == 0 => format!("Fleet in {} ({})", name(e.system_id), e.count),
        Kind::Bulk => format!("Fleet {} \u{2192} {} ({}){via}", name(e.from_system), name(e.system_id), e.count),
        Kind::Resume => "Recording resumed; what happened before is unknown".to_owned(),
    }
}

impl SpaiApp {
    /// The recorded movement of `fleet_id`, re-read every few seconds while it is being recorded.
    fn fleet_map_events(&mut self, fleet_id: &str) -> std::sync::Arc<Vec<MoveEvent>> {
        if let Some((id, read, ev)) = &self.fleet_map.events {
            // A render keeps what the scene seeded: its store is a scratch one, empty or shared.
            if id == fleet_id && (read.elapsed() < EVENTS_REREAD || self.headless) {
                return ev.clone();
            }
        }
        let ev = std::sync::Arc::new(self.store.as_ref().map(|s| s.fleet_moves(fleet_id)).unwrap_or_default());
        self.fleet_map.events = Some((fleet_id.to_owned(), std::time::Instant::now(), ev.clone()));
        ev
    }

    pub(crate) fn fleet_map_ui(&mut self, ui: &mut egui::Ui, input: &FleetMapInput) {
        let (Some(graph), Some(coords)) = (self.systems.clone(), self.map_coords.clone()) else {
            ui.label(egui::RichText::new("The map data is still loading.").weak());
            return;
        };
        let name = |id: i64| graph.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| "unknown".into());
        let events = self.fleet_map_events(&input.fleet_id);
        let now = chrono::Utc::now().timestamp();
        let first = events.first().map(|e| e.at);
        let last = match events.last() {
            Some(e) if input.live => e.at.max(now),
            Some(e) => e.at,
            None => now,
        };
        // A closed fleet opens on its last moment with anyone in it: at the close itself everyone
        // has left.
        let end = if input.live {
            last
        } else {
            events.iter().filter(|e| e.kind != Kind::Leave).map(|e| e.at).max().unwrap_or(last)
        };
        let with_date = first.is_some_and(|f| last - f > 86_400);

        // The time bar, when there is a record to move through. Without one, a note on the canvas.
        let mut notes: Vec<String> = Vec::new();
        match first {
            Some(first) if last > first => {
                ui.horizontal(|ui| {
                    let mut t = self.fleet_map.at.unwrap_or(end);
                    let following = self.fleet_map.at.is_none();
                    if input.live
                        && ui.add_enabled(!following, egui::Button::new("Live")).on_hover_text("Follow the fleet as it is now").clicked()
                    {
                        self.fleet_map.at = None;
                    }
                    ui.label(egui::RichText::new(clock(t, with_date)).monospace());
                    ui.spacing_mut().slider_width = (ui.available_width() - 20.0).max(120.0);
                    let s = egui::Slider::new(&mut t, first..=last).show_value(false);
                    if ui.add(s).changed() {
                        self.fleet_map.at = (t != end).then_some(t);
                    }
                });
            }
            _ if input.live => notes.push("Movement is recorded from when the fleet is first opened here.".into()),
            _ => notes.push("This fleet's movement was not recorded: it was never tracked here while it ran.".into()),
        }
        let at = self.fleet_map.at;
        // Live, the tree itself is the freshest answer; anywhere on the slider, the record is.
        let until = at.unwrap_or(if input.live { i64::MAX } else { end });
        let pilots: std::collections::BTreeMap<i64, Seen> = match at {
            None if input.live && !input.pilots.is_empty() => input.pilots.clone(),
            _ => crate::fleets::movement::state_at(&events, until),
        };

        let (pings, selected) = if input.cap_save { self.fleet_map_pings() } else { (Vec::new(), None) };
        if pings.len() > 1 {
            let mut pick = None;
            ui.horizontal_wrapped(|ui| {
                for p in &pings {
                    if selectable_chip(ui, selected == Some(p.seq), p.label.as_str()).clicked() {
                        pick = Some(p.seq);
                    }
                }
            });
            if let Some(seq) = pick {
                self.rescue.lock().unwrap().select_ping(seq);
            }
        }
        let target = selected
            .and_then(|s| pings.iter().find(|p| p.seq == s))
            .or(pings.first())
            .map(|p| p.system);
        let staging = input.cap_save.then(|| self.rescue_staging_id(&graph)).flatten();
        let route = match (staging, target) {
            (Some(from), Some(to)) => self.fleet_map_route(&graph, &coords, from, to),
            _ => None,
        };
        // Only once a capital is pinged: most fleets are not saves, and a note about one that is not
        // happening is noise.
        if let Some(t) = target {
            notes.push(match (staging, &route) {
                (None, _) => "No rescue staging system set.".to_owned(),
                (Some(s), None) => format!("No titan route from {} to {}.", name(s), name(t)),
                (Some(s), Some(o)) => {
                    format!("Titan at {}: {}", name(s), o.note.clone().unwrap_or_else(|| o.label.clone()))
                }
            });
        }

        self.fleet_map_side(ui, &events, &pilots, until, with_date, &name);

        let focus = self.fleet_map.focus;
        let shown_pilots: Vec<(&i64, &Seen)> =
            pilots.iter().filter(|(id, s)| s.system_id > 0 && focus.is_none_or(|f| f == **id)).collect();
        let mut counts: Vec<(i64, u32)> = Vec::new();
        for (_, s) in &shown_pilots {
            match counts.iter_mut().find(|(sys, _)| *sys == s.system_id) {
                Some(c) => c.1 += 1,
                None => counts.push((s.system_id, 1)),
            }
        }
        let fc = input.fc_character.and_then(|c| pilots.get(&c)).map(|s| s.system_id).filter(|s| *s > 0);
        // The legs drawn under the dots: one pilot's own path, or where the bulk of the fleet went.
        let legs: Vec<(i64, i64, Option<Via>)> = events
            .iter()
            .filter(|e| e.at <= until && e.from_system > 0 && e.system_id > 0)
            .filter(|e| match focus {
                Some(f) => e.kind == Kind::Move && e.character_id == f,
                None => e.kind == Kind::Bulk,
            })
            .map(|e| (e.from_system, e.system_id, e.via))
            .collect();

        // Every system the record touches, so scrubbing does not refit the view at every step.
        let mut ids: Vec<i64> = counts.iter().map(|(s, _)| *s).collect();
        ids.extend(input.pilots.values().map(|s| s.system_id).filter(|s| *s > 0));
        ids.extend(events.iter().flat_map(|e| [e.system_id, e.from_system]).filter(|s| *s > 0));
        ids.extend(staging);
        ids.extend(pings.iter().map(|p| p.system));
        if let Some(o) = &route {
            ids.extend(o.hops.iter().map(|h| h.id));
        }
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("No pilot locations yet.").weak());
            for n in &notes {
                ui.label(egui::RichText::new(n).weak());
            }
            return;
        }
        // A handful of dots on an empty canvas reads as nothing; their gate neighbours give it shape.
        let mut shown = ids.clone();
        if ids.len() < 6 {
            for id in &ids {
                shown.extend_from_slice(graph.neighbors_gates_only(*id));
            }
            shown.sort_unstable();
            shown.dedup();
        }
        let subset: Vec<crate::store::MapSystem> = coords
            .iter()
            .filter(|s| shown.binary_search(&s.id).is_ok())
            .map(|s| crate::store::MapSystem { x: s.x2d, z: s.z2d, ..s.clone() })
            .collect();
        let Some(bounds) = crate::map::Bounds::of(&subset) else { return };

        let view = &mut self.fleet_map;
        if view.fitted != ids {
            view.fitted = ids.clone();
            view.zoom = 1.0;
            view.pan = egui::Vec2::ZERO;
        }
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        if resp.dragged() {
            view.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = view.zoom;
                let new = (old * (scroll * 0.003).exp()).clamp(0.3, 8.0);
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let rel = m - (rect.center() + view.pan);
                    view.pan += rel * (1.0 - new / old);
                }
                view.zoom = new;
            }
        }
        let (zoom, pan) = (view.zoom, view.pan);
        // Inset past the projection's own margin: names hang either side of an edge dot.
        let fit = rect.shrink2(egui::vec2(60.0, 20.0));
        let pos: std::collections::HashMap<i64, egui::Pos2> = subset
            .iter()
            .map(|s| (s.id, crate::map::project(s.x, s.z, &bounds, fit, zoom, pan)))
            .collect();
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();
        let dot = 5.0;

        for s in &subset {
            let Some(&a) = pos.get(&s.id) else { continue };
            for n in graph.neighbors_gates_only(s.id) {
                if *n > s.id {
                    if let Some(&b) = pos.get(n) {
                        painter.line_segment([a, b], egui::Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.6)));
                    }
                }
            }
        }
        for b in crate::ansiblex::bridges(&self.settings.jump_bridges, &graph, &self.settings.ansiblex_capital) {
            if let (Some(&p1), Some(&p2)) = (pos.get(&b.a), pos.get(&b.b)) {
                let (ca, cb) = self.bridge_colors(b.a, b.b, egui::Color32::from_rgb(0x5A, 0xC8, 0x6A));
                gradient_polyline(&painter, &arc_polyline(p1, p2, BRIDGE_BOW), ca, cb, 1.4);
            }
        }
        let wh_col = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
        for &(a, b) in &self.wh_overlay.direct {
            if let (Some(&p1), Some(&p2)) = (pos.get(&a), pos.get(&b)) {
                painter.line_segment([p1, p2], egui::Stroke::new(1.6, wh_col));
            }
        }
        for &(a, b, hops) in &self.wh_overlay.chains {
            if let (Some(&p1), Some(&p2)) = (pos.get(&a), pos.get(&b)) {
                painter.extend(egui::Shape::dashed_line(&[p1, p2], egui::Stroke::new(1.8, HOLE_COL), 6.0, 4.0));
                painter.text(
                    p1.lerp(p2, 0.5),
                    egui::Align2::CENTER_CENTER,
                    format!("{hops}J"),
                    egui::FontId::proportional(13.0),
                    HOLE_COL,
                );
            }
        }

        let time = ui.input(|i| i.time);
        let phase = (time * 28.0) as f32;
        let mut animate = false;
        for (a, b, via) in &legs {
            let (Some(&p1), Some(&p2)) = (pos.get(a), pos.get(b)) else { continue };
            animate = true;
            match via {
                Some(Via::Jump) => polyline_flow(&painter, &arc_polyline(p1, p2, BRIDGE_BOW), JUMP_COL, phase),
                Some(Via::Ansiblex) => polyline_flow(&painter, &arc_polyline(p1, p2, BRIDGE_BOW), BRIDGE_COL, phase),
                Some(Via::Wormhole | Via::Unknown) => {
                    painter.extend(egui::Shape::dashed_line(&[p1, p2], egui::Stroke::new(2.4, HOLE_COL), 4.0, 4.0));
                }
                _ => dashed_flow(&painter, p1, p2, GATE_COL, phase),
            }
        }
        if let Some(o) = &route {
            self.draw_route_legs(&painter, &pos, o, phase);
            animate = true;
        }

        let intel = self.intel_highlights();
        let blink = (time as f32 * 6.0).sin().abs();
        let count_of = |id: i64| counts.iter().find(|(s, _)| *s == id).map(|(_, n)| *n);
        let font = egui::TextStyle::Body.resolve(ui.style());
        animate |= !pings.is_empty();
        for s in &subset {
            let p = pos[&s.id];
            if !rect.expand(20.0).contains(p) {
                continue;
            }
            if let Some((sev, received)) = intel.get(&s.id) {
                let base = severity_color(*sev);
                let fresh = now - received < 15;
                animate |= fresh;
                let fill = if fresh { 0.45 + 0.45 * blink } else { 0.40 };
                painter.circle_filled(p, dot + 5.0, base.gamma_multiply(fill));
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(2.5, base));
            }
            let related = ids.binary_search(&s.id).is_ok();
            let col = security_color(s.security);
            painter.circle_filled(p, if related { dot } else { dot * 0.6 }, if related { col } else { col.gamma_multiply(0.5) });
            for pg in pings.iter().filter(|pg| pg.system == s.id) {
                let strong = Some(pg.seq) == selected || selected.is_none();
                let a = if strong { (0.4 + 0.5 * blink).min(1.0) } else { 0.25 };
                painter.circle_filled(p, dot + 7.0, PING_COL.gamma_multiply(a));
                painter.circle_stroke(p, dot + 9.0, egui::Stroke::new(if strong { 3.0 } else { 1.5 }, PING_COL));
            }
            if Some(s.id) == staging {
                painter.circle_stroke(p, dot + 11.0, egui::Stroke::new(2.0, STAGING_COL));
            }
            if let Some(n) = count_of(s.id) {
                painter.circle_stroke(p, dot + 5.0, egui::Stroke::new(2.5, MEMBER_COL));
                let label = match focus.and_then(|f| pilots.get(&f)) {
                    Some(seen) => format!("{} ({})", seen.name, seen.ship_name),
                    None => n.to_string(),
                };
                painter.text(p + egui::vec2(dot + 6.0, -(dot + 6.0)), egui::Align2::LEFT_BOTTOM, label, font.clone(), MEMBER_COL);
            }
            if focus.is_none() && Some(s.id) == fc {
                painter.circle_stroke(p, dot + 8.5, egui::Stroke::new(3.0, FC_COL));
                painter.text(p + egui::vec2(-(dot + 6.0), -(dot + 6.0)), egui::Align2::RIGHT_BOTTOM, "FC", font.clone(), FC_COL);
            }
            // A neighbour is named once its dot has room for a name, which on a big canvas with few
            // systems is from the start rather than after zooming in.
            let roomy = || {
                pos.iter()
                    .filter(|(id, _)| **id != s.id)
                    .map(|(_, q)| q.distance(p))
                    .fold(f32::INFINITY, f32::min)
                    >= NAME_ROOM
            };
            if related || roomy() {
                let mut label = s.name.clone();
                if self.wh_overlay.jspace_holes.contains(&s.id) {
                    label = format!("{} {label}", egui_phosphor::regular::SPIRAL);
                }
                let at = p + egui::vec2(0.0, dot + 12.0);
                for off in OUTLINE {
                    painter.text(at + off, egui::Align2::CENTER_TOP, &label, font.clone(), visuals.extreme_bg_color);
                }
                let col = if related { visuals.strong_text_color() } else { visuals.weak_text_color() };
                painter.text(at, egui::Align2::CENTER_TOP, &label, font.clone(), col);
            }
        }
        // Over the canvas, not above it: a line of text in the layout pushed the map and the side
        // panel down by different amounts.
        if !notes.is_empty() {
            let text = notes.join("\n");
            // Painted text is invisible to AccessKit, so the canvas carries it as its own label.
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &text));
            let galley = painter.layout(text, font.clone(), visuals.text_color(), rect.width() - 16.0);
            let at = rect.left_top() + egui::vec2(8.0, 6.0);
            let back = egui::Rect::from_min_size(at, galley.size()).expand(4.0);
            painter.rect_filled(back, 4.0, visuals.extreme_bg_color.gamma_multiply(0.85));
            painter.galley(at, galley, visuals.text_color());
        }
        if animate {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }

        // Who was in a system, in what, at the moment on the slider.
        let hovered = resp.hover_pos().and_then(|m| nearest_system(m, &pos, 12.0));
        if let Some(id) = hovered {
            let mut here: Vec<&Seen> = shown_pilots.iter().filter(|(_, s)| s.system_id == id).map(|(_, s)| *s).collect();
            here.sort_by_key(|s| s.name.to_lowercase());
            let mut tip = name(id);
            if !here.is_empty() {
                tip.push_str(&format!(": {} pilots", here.len()));
                for s in here.iter().take(25) {
                    tip.push_str(&format!("\n{} ({})", s.name, s.ship_name));
                }
                if here.len() > 25 {
                    tip.push_str(&format!("\n{} more", here.len() - 25));
                }
            }
            resp.clone().on_hover_text_at_pointer(tip);
            if resp.clicked() {
                self.open_system(id);
            }
        }
    }

    /// The side panel: who to follow, where everyone was at the moment on the slider, and the
    /// timeline behind it. A time in the timeline moves the slider there.
    ///
    /// Both lists draw only the rows in view: rows scrolled away still count as widgets, and one
    /// sitting under the action bar would be a second button in the same place.
    fn fleet_map_side(
        &mut self,
        ui: &mut egui::Ui,
        events: &[MoveEvent],
        pilots: &std::collections::BTreeMap<i64, Seen>,
        until: i64,
        with_date: bool,
        name: &dyn Fn(i64) -> String,
    ) {
        let mut jump_to: Option<i64> = None;
        let mut focus = self.fleet_map.focus;
        let mut toggle: Option<String> = None;
        egui::Panel::right("fleet_map_side").resizable(true).default_size(320.0).size_range(240.0..=520.0).show_inside(ui, |ui| {
            // Everyone ever in the fleet, so someone who has left can still be followed.
            let mut everyone: Vec<(i64, String)> = pilots.iter().map(|(id, s)| (*id, s.name.clone())).collect();
            for e in events.iter().filter(|e| e.character_id > 0) {
                if !everyone.iter().any(|(id, _)| *id == e.character_id) {
                    everyone.push((e.character_id, e.name.clone()));
                }
            }
            everyone.sort_by_key(|p| p.1.to_lowercase());
            let current = focus.and_then(|f| everyone.iter().find(|(id, _)| *id == f)).map(|(_, n)| n.clone());
            ui.horizontal(|ui| {
                // Fixed: sized from the panel, the panel grows to fit it and it grows again.
                egui::ComboBox::from_id_salt("fleet_map_focus")
                    .width(220.0)
                    .selected_text(current.clone().unwrap_or_else(|| "Whole fleet".to_owned()))
                    .show_ui(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.fleet_map.search).hint_text("Search"));
                        if ui.menu_label(focus.is_none(), "Whole fleet").clicked() {
                            focus = None;
                        }
                        let q = self.fleet_map.search.to_lowercase();
                        for (id, n) in everyone.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)) {
                            if ui.menu_label(focus == Some(*id), n.as_str()).clicked() {
                                focus = Some(*id);
                            }
                        }
                    });
                if focus.is_some() && ui.button(egui_phosphor::regular::X).on_hover_text("Back to the whole fleet").clicked() {
                    focus = None;
                }
            });
            ui.separator();
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
            let body = (ui.available_height() - LEGEND_H).max(120.0);

            let rows: Vec<TimelineRow> = match focus {
                Some(f) => {
                    ui.label(egui::RichText::new(match pilots.get(&f) {
                        Some(s) => format!("In {}, {}", name(s.system_id), s.ship_name),
                        None => "Not in the fleet at this moment".to_owned(),
                    }).strong());
                    events.iter().filter(|e| e.character_id == f).map(|e| TimelineRow::one(e, describe(e, name, false))).collect()
                }
                None => {
                    ui.label(egui::RichText::new(format!("{} pilots", pilots.len())).strong());
                    let mut by_system: std::collections::BTreeMap<String, Vec<(i64, &Seen)>> = Default::default();
                    for (id, s) in pilots {
                        by_system.entry(name(s.system_id)).or_default().push((*id, s));
                    }
                    let mut groups: Vec<_> = by_system.into_iter().collect();
                    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
                    let mut where_rows: Vec<(Option<i64>, String, String)> = Vec::new();
                    for (sys, mut who) in groups {
                        let open = self.fleet_map.open_systems.contains(&sys);
                        let caret = if open { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT };
                        where_rows.push((None, format!("{caret} {sys} ({})", who.len()), sys.clone()));
                        if open {
                            who.sort_by_key(|p| p.1.name.to_lowercase());
                            where_rows.extend(who.into_iter().map(|(id, s)| (Some(id), format!("    {} ({})", s.name, s.ship_name), sys.clone())));
                        }
                    }
                    let list_h = (row_h * where_rows.len() as f32).min(body * 0.4);
                    egui::ScrollArea::vertical().id_salt("fleet_map_where").max_height(list_h).auto_shrink([false, true]).show_rows(
                        ui,
                        row_h,
                        where_rows.len(),
                        |ui, range| {
                            for (who, text, sys) in &where_rows[range] {
                                let r = ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), row_h),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| ui.link(text.as_str()),
                                    )
                                    .inner;
                                if r.clicked() {
                                    match who {
                                        Some(id) => focus = Some(*id),
                                        None => toggle = Some(sys.clone()),
                                    }
                                }
                            }
                        },
                    );
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Timeline").strong());
                    timeline_rows(events, name)
                }
            };
            let left = (ui.available_height() - LEGEND_H).max(row_h * 3.0);
            egui::ScrollArea::vertical().id_salt("fleet_map_timeline").max_height(left).auto_shrink([false, false]).show_rows(
                ui,
                row_h,
                rows.len(),
                |ui, range| {
                    for r in &rows[range] {
                        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if ui.link(egui::RichText::new(clock(r.at, with_date)).monospace()).on_hover_text("Show the map at this moment").clicked() {
                                jump_to = Some(r.at);
                            }
                            let t = egui::RichText::new(&r.text);
                            let label = ui.add(egui::Label::new(if r.at > until { t.weak() } else { t }).truncate());
                            if let Some(tip) = &r.tip {
                                label.on_hover_text(tip);
                            }
                        });
                    }
                },
            );
            let (strip, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), LEGEND_H), egui::Sense::hover());
            paint_legend(ui, strip);
        });
        if let Some(sys) = toggle {
            if !self.fleet_map.open_systems.remove(&sys) {
                self.fleet_map.open_systems.insert(sys);
            }
        }
        if focus != self.fleet_map.focus {
            self.fleet_map.focus = focus;
        }
        if let Some(t) = jump_to {
            self.fleet_map.at = Some(t);
        }
    }

    /// Starts recording the movement of the live fleet on screen, and resumes recordings the app
    /// was running when it last closed. Recording then runs whatever the app shows, until the fleet
    /// closes.
    pub(crate) fn fleet_track_poll(&mut self) {
        /// Past this many at once it is somebody leaving pages open, not fleets being run.
        const MAX_TRACKED: usize = 5;
        /// A recording not refreshed for this long belongs to a fleet long gone.
        const RESUME_WITHIN: i64 = 12 * 3600;

        if self.headless || !self.settings.fleet_enabled {
            return;
        }
        let Some(systems) = self.systems.clone() else { return };
        self.fleet_trackers.retain(|_, (_, h)| !h.is_finished());
        let mut want: Vec<(crate::fleets::model::FleetId, String)> = Vec::new();
        if !self.fleet_tracks_resumed {
            self.fleet_tracks_resumed = true;
            if let Some(store) = &self.store {
                let since = chrono::Utc::now().timestamp() - RESUME_WITHIN;
                want.extend(
                    store
                        .fleet_tracks_open(since)
                        .into_iter()
                        .map(|(id, name)| (crate::fleets::model::FleetId(id), name)),
                );
            }
        }
        {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            if let (crate::fleets::state::Page::Tracking(id), Some(open)) = (&st.page, st.open.value.as_ref()) {
                if open.fleet.id == *id && open.fleet.closed_at.is_none() {
                    want.push((id.clone(), open.fleet.name.clone()));
                }
            }
        }
        for (id, name) in want {
            if self.fleet_trackers.contains_key(&id) || self.fleet_trackers.len() >= MAX_TRACKED {
                continue;
            }
            let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let handle = crate::fleets::tracker::spawn(
                id.clone(),
                name,
                crate::fleets::tracker::Tracking {
                    backend: self.fleet_backend.clone(),
                    systems: systems.clone(),
                    ships: self.fleet_ship_types(),
                    alive: alive.clone(),
                    ctx: self.ui_ctx.clone(),
                },
            );
            self.fleet_trackers.insert(id, (alive, handle));
        }
    }

    /// Open capital pings with a known system, newest first, and the one being worked.
    fn fleet_map_pings(&self) -> (Vec<Ping>, Option<u64>) {
        let r = self.rescue.lock().unwrap();
        let pings = r
            .recent_pings(8)
            .into_iter()
            .filter_map(|e| {
                let system = e.system_id?;
                let where_ = e.system_name.clone().unwrap_or_default();
                let label = match e.cap_class {
                    Some(c) => format!("{} \u{b7} {where_}", c.label()),
                    None => where_,
                };
                Some(Ping { seq: e.seq, system, label })
            })
            .collect();
        (pings, r.selected_ping)
    }

    /// The cap-save preset's formup, else the configured staging system.
    pub(crate) fn rescue_staging_id(&self, graph: &crate::geo::Systems) -> Option<i64> {
        let want = self.rescue.lock().unwrap().doctrine.clone();
        let all = self.rescue_presets();
        let preset = crate::settings::find_preset(&all, &want).or_else(|| all.first());
        preset
            .and_then(|p| p.formup_location.as_ref().map(|(id, _)| *id))
            .or_else(|| graph.lookup(self.settings.rescue_staging_system.trim()).map(|i| i.id))
    }

    /// The titan route from staging to the capital, over zone 1 bridges only. Cached: the search
    /// walks every system in range.
    fn fleet_map_route(
        &mut self,
        graph: &crate::geo::Systems,
        coords: &[crate::store::MapSystem],
        from: i64,
        to: i64,
    ) -> Option<crate::web::route::RouteOption> {
        let key = (from, to, crate::ansiblex::BridgeKey::of(&self.settings));
        if let Some((k, r)) = &self.fleet_map.route {
            if *k == key {
                return r.clone();
            }
        }
        let zone1 = crate::ansiblex::with_max_zone(graph, &self.settings, 1);
        let route = titan_rescue_route(&zone1, coords, from, to);
        self.fleet_map.route = Some((key, route.clone()));
        route
    }
}

const LEGEND_H: f32 = 30.0;
/// Screen distance to the nearest other dot at which a neighbouring system gets its name.
const NAME_ROOM: f32 = 70.0;

/// What the colours of a path mean. Painted, not laid out, so it is not a widget.
fn paint_legend(ui: &egui::Ui, strip: egui::Rect) {
    let painter = ui.painter_at(strip);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut x = strip.left() + 2.0;
    let y = strip.center().y;
    for (col, text) in [(GATE_COL, "gate"), (JUMP_COL, "jump"), (BRIDGE_COL, "ansiblex"), (HOLE_COL, "wormhole")] {
        painter.line_segment([egui::pos2(x, y), egui::pos2(x + 16.0, y)], egui::Stroke::new(3.0, col));
        let r = painter.text(egui::pos2(x + 20.0, y), egui::Align2::LEFT_CENTER, text, font.clone(), ui.visuals().text_color());
        x = r.right() + 12.0;
    }
}

/// One line of a timeline list.
struct TimelineRow {
    at: i64,
    text: String,
    tip: Option<String>,
}

impl TimelineRow {
    fn one(e: &MoveEvent, text: String) -> Self {
        TimelineRow { at: e.at, text, tip: None }
    }
}

/// The whole fleet's timeline, newest first: where the bulk went, who came and went. Everyone in the
/// fleet when recording starts joins in the same second, and a close leaves the same way, so a run
/// like that is one line with the names on hover.
fn timeline_rows(events: &[MoveEvent], name: &dyn Fn(i64) -> String) -> Vec<TimelineRow> {
    let shown: Vec<&MoveEvent> = events
        .iter()
        .rev()
        .filter(|e| matches!(e.kind, Kind::Bulk | Kind::Join | Kind::Leave | Kind::Resume))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < shown.len() {
        let e = shown[i];
        let run = shown[i..]
            .iter()
            .take_while(|x| x.kind == e.kind && x.at == e.at && matches!(e.kind, Kind::Join | Kind::Leave))
            .count()
            .max(1);
        if run > 3 {
            let verb = if e.kind == Kind::Join { "joined" } else { "left" };
            let names: Vec<String> = shown[i..i + run].iter().map(|x| format!("{} ({})", x.name, x.ship_name)).collect();
            out.push(TimelineRow { at: e.at, text: format!("{run} pilots {verb}"), tip: Some(names.join("\n")) });
        } else {
            out.extend(shown[i..i + run].iter().map(|x| TimelineRow::one(x, describe(x, name, true))));
        }
        i += run;
    }
    out
}

/// Direct when the capital is in range, otherwise a bridge as close as range allows and gates.
/// The titan stays at staging: it is the one bridging the fleet out.
pub(crate) fn titan_rescue_route(
    graph: &crate::geo::Systems,
    coords: &[crate::store::MapSystem],
    from: i64,
    to: i64,
) -> Option<crate::web::route::RouteOption> {
    crate::web::route::titan(
        graph,
        coords,
        from,
        to,
        DELVE911_RANGE_LY,
        true,
        true,
        &crate::web::route::Avoid::default(),
        &std::collections::HashMap::new(),
        &[],
        false,
    )
    .into_iter()
    .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only zone 1 bridges carry the fleet after the titan jump.
    #[test]
    fn the_rescue_route_takes_zone_one_bridges_only() {
        let (mut base, coords) = crate::uitest::fixtures::fleet_map_world();
        base.set_positions(coords.iter().map(|s| (s.id, [s.x, s.y, s.z])).collect());
        let (staging, gate_a, target) = (30_000_772, 30_900_001, 30_004_759);
        let route = |capital: &str| {
            let settings = crate::settings::Settings {
                jump_bridges: vec![crate::settings::JumpBridge { from: "Fixture Gate A".into(), to: "319-3D".into() }],
                ansiblex_capital: capital.into(),
                ..Default::default()
            };
            let g = crate::ansiblex::with_max_zone(&base, &settings, 1);
            titan_rescue_route(&g, &coords, staging, target).expect("a route")
        };
        let kinds = |o: &crate::web::route::RouteOption| o.hops.iter().map(|h| (h.id, h.kind)).collect::<Vec<_>>();

        // 319-3D is 10 ly from a capital at staging: zone 2, so the fleet gates.
        let far = route("Placeholder Staging");
        assert_eq!(kinds(&far)[1], (gate_a, 2), "the titan jumps to Gate A: {:?}", kinds(&far));
        assert!(far.hops.iter().all(|h| h.kind != 1), "a zone 2 bridge was used: {:?}", kinds(&far));
        assert_eq!(far.gates, 3);

        // With the capital at 319-3D the same bridge lands in zone 1.
        let near = route("319-3D");
        assert!(near.hops.iter().any(|h| h.kind == 1 && h.id == 30_004_608), "{:?}", kinds(&near));
    }

    /// A capital in range is one titan jump from staging.
    #[test]
    fn a_capital_in_range_is_bridged_directly() {
        let (base, coords) = crate::uitest::fixtures::fleet_map_world();
        let pocket = 30_900_003;
        let o = titan_rescue_route(&base, &coords, 30_000_772, pocket).expect("a route");
        assert_eq!(o.hops.iter().map(|h| (h.id, h.kind)).collect::<Vec<_>>(), vec![(30_000_772, 0), (pocket, 2)]);
    }
}
