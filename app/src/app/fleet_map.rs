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
    /// Everyone the dashboard lists as having taken part, for naming them on kills.
    pub(crate) roster: Vec<(i64, String)>,
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
            roster: Vec::new(),
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
    /// Its kills and losses, read with the movement.
    kills: std::sync::Arc<Vec<crate::store::FleetKill>>,
    /// The moment on the slider. None follows the live fleet.
    at: Option<i64>,
    /// The one pilot being followed.
    focus: Option<i64>,
    search: String,
    side_tab: SideTab,
    /// The system whose pilots are listed in a dialog.
    system_dialog: Option<i64>,
    /// The slider moved: the timeline scrolls to its moment.
    scroll_to: bool,
    playing: bool,
    /// Record seconds per real second while playing.
    speed: i64,
    last_tick: Option<std::time::Instant>,
    /// The fleet all of the above belongs to.
    fleet: String,
}

#[cfg(test)]
impl FleetMapView {
    /// A recorded history, a moment on the slider and a pilot to follow, as a scene wants them.
    pub(crate) fn seed(
        &mut self,
        fleet_id: &str,
        events: Vec<MoveEvent>,
        kills: Vec<crate::store::FleetKill>,
        at: Option<i64>,
        focus: Option<i64>,
    ) {
        self.events = Some((fleet_id.to_owned(), std::time::Instant::now(), std::sync::Arc::new(events)));
        self.fleet = fleet_id.to_owned();
        self.kills = std::sync::Arc::new(kills);
        self.at = at;
        self.focus = focus;
    }
}

/// The fleet's battle report and the kills it is made from, kept for the fleet on screen.
#[derive(Default)]
pub(crate) struct FleetBr {
    fleet: String,
    url: Option<String>,
    /// How many kills the report was made from; more since means it is out of date.
    made_from: usize,
    kills: std::sync::Arc<Vec<crate::store::FleetKill>>,
    counted_at: Option<std::time::Instant>,
    /// A report being created, and its answer once it is in.
    job: std::sync::Arc<std::sync::Mutex<Option<Result<(String, String), String>>>>,
    busy: bool,
    /// The kill count of the report being made.
    pending_from: usize,
    error: Option<String>,
    /// Not the fleet on screen's alone, so it survives moving between fleets.
    backfill: Backfill,
}

/// zKillboard backfills for closed fleets: asked for, running, and finished but not yet noticed.
#[derive(Default)]
struct Backfill {
    tried: std::collections::HashSet<String>,
    running: std::collections::HashSet<String>,
    done: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

/// What the fleet header shows of its battle report.
pub(crate) struct BrView {
    pub(crate) url: Option<String>,
    pub(crate) busy: bool,
    /// Its kills are still being fetched from zKillboard.
    pub(crate) backfilling: bool,
    pub(crate) has_kills: bool,
    pub(crate) stale: bool,
    pub(crate) error: Option<String>,
}

pub(crate) enum BrClick {
    Create,
    Open,
    Copy,
    InApp,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SideTab {
    #[default]
    Timeline,
    Locations,
}

/// Playback speeds: record seconds per real second.
const SPEEDS: [i64; 3] = [10, 60, 300];

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
const KILL_COL: egui::Color32 = egui::Color32::from_rgb(0x66, 0xBB, 0x6A);
const LOSS_COL: egui::Color32 = egui::Color32::from_rgb(0xEF, 0x53, 0x50);
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
    /// Hull names by type id, for kill lines.
    fn ship_names(&self) -> impl Fn(i64) -> String + 'static {
        let index = self.ship_index.clone();
        move |id: i64| {
            if crate::fleets::movement::is_pod(id) {
                return "Capsule".to_owned();
            }
            index
                .as_ref()
                .and_then(|i| i.values().find(|(t, _)| *t == id).map(|(_, n)| n.clone()))
                .unwrap_or_else(|| format!("ship {id}"))
        }
    }

    /// The recorded movement of `fleet_id`, re-read every few seconds while it is being recorded.
    fn fleet_map_events(&mut self, fleet_id: &str) -> std::sync::Arc<Vec<MoveEvent>> {
        if let Some((id, read, ev)) = &self.fleet_map.events {
            // A render keeps what the scene seeded: its store is a scratch one, empty or shared.
            if id == fleet_id && (read.elapsed() < EVENTS_REREAD || self.headless) {
                return ev.clone();
            }
        }
        let ev = std::sync::Arc::new(self.store.as_ref().map(|s| s.fleet_moves(fleet_id)).unwrap_or_default());
        let kills = self.store.as_ref().map(|s| s.fleet_kills(fleet_id)).unwrap_or_default();
        self.fleet_map.kills = std::sync::Arc::new(crate::fleets::movement::in_fleet_kills(&kills, &ev));
        self.fleet_map.events = Some((fleet_id.to_owned(), std::time::Instant::now(), ev.clone()));
        ev
    }

    pub(crate) fn fleet_map_ui(&mut self, ui: &mut egui::Ui, input: &FleetMapInput) {
        let (Some(graph), Some(coords)) = (self.systems.clone(), self.map_coords.clone()) else {
            ui.label(egui::RichText::new("The map data is still loading.").weak());
            return;
        };
        let name = |id: i64| graph.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| "unknown".into());
        // The slider, the followed pilot and the view belong to one fleet. Kept across fleets, a
        // pilot followed in the last one filtered this one down to nothing while the picker read
        // "Whole fleet".
        if self.fleet_map.fleet != input.fleet_id {
            let route = self.fleet_map.route.take();
            self.fleet_map = FleetMapView { fleet: input.fleet_id.clone(), route, ..Default::default() };
        }
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

        // Playback moves the slider on by wall-clock time, and stops at the end of the record.
        if self.fleet_map.playing {
            let now_i = std::time::Instant::now();
            let dt = self.fleet_map.last_tick.map_or(0.0, |t| now_i.duration_since(t).as_secs_f64());
            self.fleet_map.last_tick = Some(now_i);
            let from = self.fleet_map.at.unwrap_or(first.unwrap_or(end));
            let t = from + (dt * self.fleet_map.speed.max(1) as f64).round() as i64;
            if t >= end {
                self.fleet_map.playing = false;
                self.fleet_map.at = None;
            } else {
                self.fleet_map.at = Some(t);
            }
            self.fleet_map.scroll_to = true;
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let state_now = |at: Option<i64>| {
            let until = at.unwrap_or(if input.live { i64::MAX } else { end });
            // Live, the tree itself is the freshest answer; anywhere on the slider, the record is.
            let pilots: std::collections::BTreeMap<i64, Seen> = match at {
                None if input.live && !input.pilots.is_empty() => input.pilots.clone(),
                _ => crate::fleets::movement::state_at(&events, until),
            };
            (until, pilots)
        };
        let (until, pilots) = state_now(self.fleet_map.at);

        // The side panel first, so the time bar and everything under it is the map's width only.
        let kills = self.fleet_map.kills.clone();
        let ship = self.ship_names();
        // Fleet pilots by id, from wherever they were seen, to name them on the kills they made.
        let mut known: std::collections::HashMap<i64, String> = input.roster.iter().cloned().collect();
        known.extend(input.pilots.iter().map(|(id, s)| (*id, s.name.clone())));
        known.extend(events.iter().filter(|e| e.character_id > 0).map(|e| (e.character_id, e.name.clone())));
        let who = |ids: &[i64]| by_names(ids, &known);
        self.fleet_map_side(ui, &events, &kills, &ship, &who, &pilots, until, with_date, &name);
        self.fleet_map_system_dialog(&ui.ctx().clone(), &pilots, until, with_date, &name);

        // The time bar, when there is a record to move through. Without one, a note on the canvas.
        let mut notes: Vec<String> = Vec::new();
        match first {
            Some(first) if last > first => self.fleet_map_time_bar(ui, &events, first, last, end, input.live, with_date),
            _ if input.live => notes.push("Movement is recorded from when the fleet is first opened here.".into()),
            _ => notes.push("This fleet's movement was not recorded: it was never tracked here while it ran.".into()),
        }
        let at = self.fleet_map.at;
        let (until, pilots) = state_now(at);

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



        let focus = self.fleet_map.focus;
        let shown_pilots: Vec<(&i64, &Seen)> =
            pilots.iter().filter(|(id, s)| s.system_id > 0 && focus.is_none_or(|f| f == **id)).collect();
        // Kills and losses up to the slider, the followed pilot's only when there is one. A pod
        // rides on its ship loss and is not counted again.
        let shown_kills: Vec<&crate::store::FleetKill> = kills
            .iter()
            .filter(|k| k.at <= until && focus.is_none_or(|f| k.victim_char == f || k.members.contains(&f)))
            .collect();
        let tally = |sys: i64| {
            let here = shown_kills.iter().filter(|k| k.system_id == sys && k.pod_of == 0);
            here.fold((0u32, 0u32), |(w, l), k| if k.loss { (w, l + 1) } else { (w + 1, l) })
        };
        let mut counts: Vec<(i64, u32)> = Vec::new();
        for (_, s) in &shown_pilots {
            match counts.iter_mut().find(|(sys, _)| *sys == s.system_id) {
                Some(c) => c.1 += 1,
                None => counts.push((s.system_id, 1)),
            }
        }
        let fc = input.fc_character.and_then(|c| pilots.get(&c)).map(|s| s.system_id).filter(|s| *s > 0);
        // The legs drawn under the dots: one pilot's own path, or where the bulk of the fleet went.
        // Off the map like on the main one: wormhole space and the other hidden regions sit far from
        // New Eden, and one of them in the set shrank everything else to a dot.
        let on_map = |id: i64| {
            is_kspace(id) && graph.info_of(id).is_none_or(|i| !is_hidden_region(&i.region))
        };
        let (legs, last_on) = paths(&events, until, focus, &on_map);
        let mut in_hole: Vec<(i64, Vec<&Seen>)> = Vec::new();
        for (id, s) in &shown_pilots {
            if on_map(s.system_id) {
                continue;
            }
            let Some(&entry) = last_on.get(id) else { continue };
            match in_hole.iter_mut().find(|(e, _)| *e == entry) {
                Some((_, v)) => v.push(*s),
                None => in_hole.push((entry, vec![*s])),
            }
        }

        // Every system the record touches, so scrubbing does not refit the view at every step.
        let mut ids: Vec<i64> = counts.iter().map(|(s, _)| *s).collect();
        ids.extend(input.pilots.values().map(|s| s.system_id).filter(|s| *s > 0));
        ids.extend(events.iter().flat_map(|e| [e.system_id, e.from_system]).filter(|s| *s > 0));
        ids.extend(staging);
        ids.extend(kills.iter().map(|k| k.system_id));
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
        // Every region the fleet entered, whole, so its moves read against the map they happened on.
        let regions: std::collections::HashSet<i64> = coords
            .iter()
            .filter(|s| ids.binary_search(&s.id).is_ok() && on_map(s.id))
            .map(|s| s.region_id)
            .collect();
        let subset: Vec<crate::store::MapSystem> = coords
            .iter()
            .filter(|s| regions.contains(&s.region_id) && on_map(s.id))
            .map(|s| crate::store::MapSystem { x: s.x2d, z: s.z2d, ..s.clone() })
            .collect();
        // What was used up to the moment on the slider is drawn in full; the rest of the regions
        // is only there for orientation and stays pale.
        let mut used_sys: std::collections::HashSet<i64> = events
            .iter()
            .filter(|e| e.at <= until)
            .flat_map(|e| [e.system_id, e.from_system])
            .filter(|s| *s > 0)
            .collect();
        used_sys.extend(pilots.values().map(|s| s.system_id));
        used_sys.extend(staging);
        used_sys.extend(shown_kills.iter().map(|k| k.system_id));
        used_sys.extend(pings.iter().map(|p| p.system));
        let mut used_link: std::collections::HashSet<(i64, i64)> = events
            .iter()
            .filter(|e| e.at <= until && matches!(e.kind, Kind::Move | Kind::Bulk) && e.from_system > 0)
            .map(|e| (e.from_system.min(e.system_id), e.from_system.max(e.system_id)))
            .collect();
        if let Some(o) = &route {
            used_sys.extend(o.hops.iter().map(|h| h.id));
            used_link.extend(o.hops.windows(2).map(|w| (w[0].id.min(w[1].id), w[0].id.max(w[1].id))));
        }
        let Some(bounds) = crate::map::Bounds::of(&subset) else { return };

        let view = &mut self.fleet_map;
        if view.fitted != ids {
            view.fitted = ids.clone();
            view.zoom = 1.0;
            view.pan = egui::Vec2::ZERO;
        }
        let rect = ui.available_rect_before_wrap();
        // Inset past the projection's own margin: names hang either side of an edge dot.
        let fit = rect.shrink2(egui::vec2(60.0, 20.0));
        // Zoom in until a light year spans a good part of the canvas, however far apart the
        // systems are: a fixed factor ran out long before that on a spread-out fleet.
        let ly_px = crate::map::ly_to_pixels(1.0, &bounds, fit, 1.0).max(0.001);
        let max_zoom = (MAX_LY_PX / ly_px).max(8.0);
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        if resp.dragged() {
            view.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = view.zoom;
                let new = (old * (scroll * 0.003).exp()).clamp(0.3, max_zoom);
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let rel = m - (rect.center() + view.pan);
                    view.pan += rel * (1.0 - new / old);
                }
                view.zoom = new;
            }
        }
        let (zoom, pan) = (view.zoom, view.pan);
        let pos: std::collections::HashMap<i64, egui::Pos2> = subset
            .iter()
            .map(|s| (s.id, crate::map::project(s.x, s.z, &bounds, fit, zoom, pan)))
            .collect();
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();
        // Dots and their rings shrink as the systems crowd together, down to a couple of pixels,
        // so a zoomed-out map still shows the route between them rather than a row of discs.
        let spacing = {
            let on_screen: Vec<egui::Pos2> = pos.values().copied().filter(|p| rect.contains(*p)).collect();
            let mut min = f32::INFINITY;
            for (i, a) in on_screen.iter().enumerate() {
                for b in &on_screen[i + 1..] {
                    min = min.min(a.distance(*b));
                }
            }
            min
        };
        let k = (spacing / FULL_SIZE_SPACING).clamp(MIN_DOT_R / 5.0, 1.0);
        let dot = 5.0 * k;
        let ring = |w: f32| (w * k).max(1.0);

        let pale = visuals.weak_text_color().gamma_multiply(0.25);
        let worn = visuals.text_color().gamma_multiply(0.7);
        for s in &subset {
            let Some(&a) = pos.get(&s.id) else { continue };
            for n in graph.neighbors_gates_only(s.id) {
                if *n > s.id {
                    if let Some(&b) = pos.get(n) {
                        let used = used_link.contains(&(s.id, *n));
                        painter.line_segment([a, b], egui::Stroke::new(if used { 1.6 } else { 1.0 }, if used { worn } else { pale }));
                    }
                }
            }
        }
        for b in crate::ansiblex::bridges(&self.settings.jump_bridges, &graph, &self.settings.ansiblex_capital) {
            if let (Some(&p1), Some(&p2)) = (pos.get(&b.a), pos.get(&b.b)) {
                let (ca, cb) = self.bridge_colors(b.a, b.b, egui::Color32::from_rgb(0x5A, 0xC8, 0x6A));
                let (ca, cb) = if used_link.contains(&(b.a.min(b.b), b.a.max(b.b))) {
                    (ca, cb)
                } else {
                    (desaturate(ca).gamma_multiply(0.35), desaturate(cb).gamma_multiply(0.35))
                };
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
                    polyline_flow(&painter, &arc_polyline(p1, p2, BRIDGE_BOW), HOLE_COL, phase);
                }
                _ => dashed_flow(&painter, p1, p2, GATE_COL, phase),
            }
        }
        let hole_font = egui::TextStyle::Body.resolve(ui.style());
        for (entry, who) in &in_hole {
            let Some(&p) = pos.get(entry) else { continue };
            // A short arc up and away from the dot, the way a bridge leaves it, ending in the count.
            let end = p + egui::vec2(38.0, -38.0);
            polyline_flow(&painter, &arc_polyline(p, end, BRIDGE_BOW), HOLE_COL, phase);
            painter.circle_filled(end, 11.0, visuals.extreme_bg_color);
            painter.circle_stroke(end, 11.0, egui::Stroke::new(1.5, HOLE_COL));
            painter.text(end, egui::Align2::CENTER_CENTER, who.len().to_string(), hole_font.clone(), HOLE_COL);
            animate = true;
        }
        if let Some(o) = &route {
            self.draw_route_legs(&painter, &pos, o, phase);
            animate = true;
        }

        // Intel is about now. On a moment from the past it would light systems for what happened
        // since, not then.
        let intel = if input.live && at.is_none() { self.intel_highlights() } else { Default::default() };
        let (mut on_screen, mut named) = (0usize, 0usize);
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
                painter.circle_filled(p, dot + 5.0 * k, base.gamma_multiply(fill));
                painter.circle_stroke(p, dot + 3.0 * k, egui::Stroke::new(ring(2.5), base));
            }
            let related = used_sys.contains(&s.id);
            let col = security_color(s.security);
            painter.circle_filled(p, if related { dot } else { (dot * 0.6).max(MIN_DOT_R) }, if related { col } else { desaturate(col).gamma_multiply(0.5) });
            for pg in pings.iter().filter(|pg| pg.system == s.id) {
                let strong = Some(pg.seq) == selected || selected.is_none();
                let a = if strong { (0.4 + 0.5 * blink).min(1.0) } else { 0.25 };
                painter.circle_filled(p, dot + 7.0 * k, PING_COL.gamma_multiply(a));
                painter.circle_stroke(p, dot + 9.0 * k, egui::Stroke::new(ring(if strong { 3.0 } else { 1.5 }), PING_COL));
            }
            if Some(s.id) == staging {
                painter.circle_stroke(p, dot + 11.0 * k, egui::Stroke::new(ring(2.0), STAGING_COL));
            }
            if let Some(n) = count_of(s.id) {
                painter.circle_stroke(p, dot + 5.0 * k, egui::Stroke::new(ring(2.5), MEMBER_COL));
                let label = match focus.and_then(|f| pilots.get(&f)) {
                    Some(seen) => format!("{} ({})", seen.name, seen.ship_name),
                    None => n.to_string(),
                };
                painter.text(p + egui::vec2(dot + 6.0 * k, -(dot + 6.0 * k)), egui::Align2::LEFT_BOTTOM, label, font.clone(), MEMBER_COL);
            }
            let (won, lost) = tally(s.id);
            if won + lost > 0 {
                // Under the dot, left: kills in green, losses in red.
                let mut x = p.x - (dot + 4.0 * k);
                let y = p.y + dot + 4.0 * k;
                for (n, glyph, col) in [(lost, egui_phosphor::regular::SKULL, LOSS_COL), (won, egui_phosphor::regular::CROSSHAIR, KILL_COL)] {
                    if n == 0 {
                        continue;
                    }
                    let r = painter.text(egui::pos2(x, y), egui::Align2::RIGHT_TOP, format!("{glyph}{n}"), font.clone(), col);
                    x = r.left() - 4.0;
                }
            }
            if focus.is_none() && Some(s.id) == fc {
                painter.circle_stroke(p, dot + 8.5 * k, egui::Stroke::new(ring(3.0), FC_COL));
                painter.text(p + egui::vec2(-(dot + 6.0 * k), -(dot + 6.0 * k)), egui::Align2::RIGHT_BOTTOM, "FC", font.clone(), FC_COL);
            }
            // A system is named once its dot has room for a name, fading in as the room opens up.
            let room = pos
                .iter()
                .filter(|(id, _)| **id != s.id)
                .map(|(_, q)| q.distance(p))
                .fold(f32::INFINITY, f32::min);
            let fade = ((room - NAME_FADE_FROM) / (NAME_FADE_TO - NAME_FADE_FROM)).clamp(0.0, 1.0);
            if rect.contains(p) {
                on_screen += 1;
                if fade > 0.5 {
                    named += 1;
                }
            }
            if fade > 0.0 {
                let mut label = s.name.clone();
                if self.wh_overlay.jspace_holes.contains(&s.id) {
                    label = format!("{} {label}", egui_phosphor::regular::SPIRAL);
                }
                let at = p + egui::vec2(0.0, dot + 12.0 * k);
                for off in OUTLINE {
                    painter.text(at + off, egui::Align2::CENTER_TOP, &label, font.clone(), visuals.extreme_bg_color.gamma_multiply(fade));
                }
                let col = if related { visuals.strong_text_color() } else { visuals.weak_text_color() };
                painter.text(at, egui::Align2::CENTER_TOP, &label, font.clone(), col.gamma_multiply(fade));
            }
        }
        // Zoomed out, where systems go unnamed, the regions are named instead, on top, the way the
        // main map does it.
        if named * 2 < on_screen {
            let mut acc: std::collections::HashMap<i64, (egui::Vec2, u32, i64)> = Default::default();
            for s in &subset {
                let Some(&p) = pos.get(&s.id) else { continue };
                let e = acc.entry(s.region_id).or_insert((egui::Vec2::ZERO, 0, s.id));
                e.0 += p.to_vec2();
                e.1 += 1;
            }
            let region_font = egui::FontId::proportional(16.0);
            for (sum, n, any) in acc.into_values() {
                let c = (sum / n as f32).to_pos2();
                if !rect.contains(c) {
                    continue;
                }
                let Some(name) = graph.info_of(any).map(|i| i.region.clone()) else { continue };
                painter.text(c + egui::vec2(1.0, 1.0), egui::Align2::CENTER_CENTER, &name, region_font.clone(), egui::Color32::from_black_alpha(180));
                painter.text(c, egui::Align2::CENTER_CENTER, &name, region_font.clone(), egui::Color32::from_gray(220));
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
            let ship = self.ship_names();
            for kl in shown_kills.iter().filter(|kl| kl.system_id == id && kl.pod_of == 0) {
                let pod: f64 = shown_kills.iter().filter(|p| p.pod_of == kl.kill_id).map(|p| p.value).sum();
                let pod = if pod > 0.0 { format!(" + pod {}", fmt_isk(pod)) } else { String::new() };
                let by = if kl.loss { String::new() } else { by_names(&kl.members, &known) };
                tip.push_str(&format!(
                    "\n{} {} {} {} ({}{pod}){by}",
                    clock(kl.at, with_date),
                    if kl.loss { "Lost" } else { "Killed" },
                    kl.victim_name,
                    ship(kl.ship_type_id),
                    fmt_isk(kl.value)
                ));
            }
            if let Some((_, who)) = in_hole.iter().find(|(e, _)| *e == id) {
                tip.push_str(&format!("\n{} in wormhole space from here:", who.len()));
                for s in who.iter().take(25) {
                    tip.push_str(&format!("\n{} ({}) in {}", s.name, s.ship_name, name(s.system_id)));
                }
            }
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

    /// Live, step back, play, step forward, speed, the moment, and the slider over the record.
    #[allow(clippy::too_many_arguments)]
    fn fleet_map_time_bar(
        &mut self,
        ui: &mut egui::Ui,
        events: &[MoveEvent],
        first: i64,
        last: i64,
        end: i64,
        live: bool,
        with_date: bool,
    ) {
        use egui_phosphor::regular as ic;
        let v = &mut self.fleet_map;
        let mut t = v.at.unwrap_or(end);
        let before = v.at;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if live && ui.add_enabled(v.at.is_some(), egui::Button::new("Live")).on_hover_text("Follow the fleet as it is now").clicked() {
                v.at = None;
                v.playing = false;
            }
            // A step is to the next moment anything was recorded, not a fixed stretch of time.
            let prev = events.iter().rev().map(|e| e.at).find(|a| *a < t);
            let next = events.iter().map(|e| e.at).find(|a| *a > t);
            if ui.add_enabled(prev.is_some(), egui::Button::new(ic::SKIP_BACK)).on_hover_text("Back to the previous change").clicked() {
                v.at = prev;
                v.playing = false;
            }
            let play = if v.playing { ic::PAUSE } else { ic::PLAY };
            if ui.button(play).on_hover_text(if v.playing { "Pause" } else { "Play the record from here" }).clicked() {
                v.playing = !v.playing;
                v.last_tick = None;
                // Played from the end, it starts over.
                if v.playing && v.at.is_none() {
                    v.at = Some(first);
                }
            }
            if ui.add_enabled(v.at.is_some(), egui::Button::new(ic::SKIP_FORWARD)).on_hover_text("On to the next change").clicked() {
                v.at = next.filter(|n| *n < end);
                v.playing = false;
            }
            if v.speed == 0 {
                v.speed = SPEEDS[1];
            }
            egui::ComboBox::from_id_salt("fleet_map_speed")
                .width(60.0)
                .selected_text(format!("\u{d7}{}", v.speed))
                .show_ui(ui, |ui| {
                    for s in SPEEDS {
                        ui.menu_value(&mut v.speed, s, format!("\u{d7}{s}"));
                    }
                })
                .response
                .on_hover_text("Playback speed: record seconds per second");
            ui.label(egui::RichText::new(clock(t, with_date)).monospace());
            ui.spacing_mut().slider_width = (ui.available_width() - 20.0).max(120.0);
            if ui.add(egui::Slider::new(&mut t, first..=last).show_value(false)).changed() {
                v.at = (t != end).then_some(t);
                v.playing = false;
            }
        });
        if v.at != before {
            v.scroll_to = true;
        }
    }

    /// The side panel: who to follow, then two tabs: the timeline behind the slider, and where
    /// everyone was at its moment. A time in the timeline moves the slider there, a system in the
    /// locations opens a dialog of who was in it.
    ///
    /// Both lists draw only the rows in view: rows scrolled away still count as widgets, and one
    /// sitting under the action bar would be a second button in the same place.
    #[allow(clippy::too_many_arguments)]
    fn fleet_map_side(
        &mut self,
        ui: &mut egui::Ui,
        events: &[MoveEvent],
        kills: &[crate::store::FleetKill],
        ship: &dyn Fn(i64) -> String,
        who: &dyn Fn(&[i64]) -> String,
        pilots: &std::collections::BTreeMap<i64, Seen>,
        until: i64,
        with_date: bool,
        name: &dyn Fn(i64) -> String,
    ) {
        let mut jump_to: Option<i64> = None;
        let mut focus = self.fleet_map.focus;
        let mut dialog: Option<i64> = None;
        let mut tab = self.fleet_map.side_tab;
        let scroll_to = std::mem::take(&mut self.fleet_map.scroll_to);
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
            // Someone this fleet never had cannot be followed in it.
            if current.is_none() {
                focus = None;
            }
            ui.add_space(6.0);
            // Right to left: the ✕ first, then the picker gets exactly what is left. Guessing the
            // ✕'s width short made the row wider than the panel, and the panel grew to fit it every
            // frame until it hit its maximum.
            let row_h = ui.spacing().interact_size.y;
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), row_h),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if focus.is_some() && ui.button(egui_phosphor::regular::X).on_hover_text("Back to the whole fleet").clicked() {
                        focus = None;
                    }
                    egui::ComboBox::from_id_salt("fleet_map_focus")
                        .width(ui.available_width().max(80.0))
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
                },
            );
            if let Some(f) = focus {
                ui.label(egui::RichText::new(match pilots.get(&f) {
                    Some(s) => format!("In {}, {}", name(s.system_id), s.ship_name),
                    None => "Not in the fleet at this moment".to_owned(),
                }).strong());
            }
            if !kills.is_empty() {
                self.fleet_map_battle(ui, kills, with_date);
            }
            ui.add_space(2.0);
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let half = egui::vec2(ui.available_width() / 2.0, row_h + 4.0);
                if ui.menu_label_sized(half, tab == SideTab::Timeline, "Timeline").clicked() {
                    tab = SideTab::Timeline;
                }
                if ui.menu_label_sized(half, tab == SideTab::Locations, format!("Locations ({})", pilots.len())).clicked() {
                    tab = SideTab::Locations;
                }
            });
            ui.separator();
            let height = (ui.available_height() - LEGEND_H).max(row_h * 3.0);
            match tab {
                SideTab::Timeline => {
                    let rows: Vec<TimelineRow> = match focus {
                        Some(f) => {
                            let mut rows: Vec<TimelineRow> =
                                events.iter().filter(|e| e.character_id == f).map(|e| TimelineRow::one(e, describe(e, name, false))).collect();
                            rows.extend(kill_rows(kills, Some(f), ship, who, name));
                            rows.sort_by_key(|r| r.at);
                            rows
                        }
                        None => {
                            let mut rows = timeline_rows(events, name);
                            rows.extend(kill_rows(kills, None, ship, who, name));
                            rows.sort_by_key(|r| std::cmp::Reverse(r.at));
                            rows
                        }
                    };
                    // Newest first for the fleet, oldest first for one pilot: either way, the row
                    // for the moment on the slider is the latest one not after it.
                    let at_row = match focus {
                        Some(_) => rows.iter().rposition(|r| r.at <= until),
                        None => rows.iter().position(|r| r.at <= until),
                    };
                    let mut area = egui::ScrollArea::vertical().id_salt("fleet_map_timeline").max_height(height).auto_shrink([false, false]);
                    if let (true, Some(i)) = (scroll_to, at_row) {
                        let step = row_h + ui.spacing().item_spacing.y;
                        area = area.vertical_scroll_offset((i as f32 * step - height / 2.0).max(0.0));
                    }
                    area.show_rows(ui, row_h, rows.len(), |ui, range| {
                        for (i, r) in rows[range.clone()].iter().enumerate() {
                            let here = at_row == Some(range.start + i);
                            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let time = egui::RichText::new(clock(r.at, with_date)).monospace();
                                if ui.link(if here { time.strong() } else { time }).on_hover_text("Show the map at this moment").clicked() {
                                    jump_to = Some(r.at);
                                }
                                let t = egui::RichText::new(&r.text);
                                let t = if r.at > until { t.weak() } else if here { t.strong() } else { t };
                                let label = ui.add(egui::Label::new(t).truncate());
                                if let Some(tip) = &r.tip {
                                    label.on_hover_text(tip);
                                }
                            });
                        }
                    });
                }
                SideTab::Locations => {
                    let mut by_system: std::collections::BTreeMap<i64, u32> = Default::default();
                    for s in pilots.values() {
                        *by_system.entry(s.system_id).or_default() += 1;
                    }
                    let mut groups: Vec<(i64, u32)> = by_system.into_iter().collect();
                    groups.sort_by(|a, b| b.1.cmp(&a.1).then(name(a.0).cmp(&name(b.0))));
                    egui::ScrollArea::vertical().id_salt("fleet_map_where").max_height(height).auto_shrink([false, false]).show_rows(
                        ui,
                        row_h,
                        groups.len(),
                        |ui, range| {
                            for (sys, n) in &groups[range] {
                                let r = ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), row_h),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| ui.link(format!("{} ({n})", name(*sys))).on_hover_text("Who was here, in what"),
                                    )
                                    .inner;
                                if r.clicked() {
                                    dialog = Some(*sys);
                                }
                            }
                        },
                    );
                }
            }
            let (strip, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), LEGEND_H), egui::Sense::hover());
            paint_legend(ui, strip);
        });
        self.fleet_map.side_tab = tab;
        if dialog.is_some() {
            self.fleet_map.system_dialog = dialog;
        }
        if focus != self.fleet_map.focus {
            self.fleet_map.focus = focus;
        }
        if let Some(t) = jump_to {
            self.fleet_map.at = Some(t);
            self.fleet_map.playing = false;
        }
    }

    /// When the fleet fought and what it cost. The report made from it is in the fleet's header.
    fn fleet_map_battle(&mut self, ui: &mut egui::Ui, kills: &[crate::store::FleetKill], with_date: bool) {
        use egui_phosphor::regular as ic;
        let counted = kills.iter().filter(|k| k.pod_of == 0);
        let (won, lost) = counted.fold((0, 0), |(w, l), k| if k.loss { (w, l + 1) } else { (w + 1, l) });
        let first = kills.iter().map(|k| k.at).min().unwrap_or(0);
        let last = kills.iter().map(|k| k.at).max().unwrap_or(0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{} {won}", ic::CROSSHAIR)).color(KILL_COL));
            ui.label(egui::RichText::new(format!("{} {lost}", ic::SKULL)).color(LOSS_COL));
            ui.label(format!("{} to {}", clock(first, with_date), clock(last, with_date)));
        });
    }

    /// The fleet's battle report as its header shows it: the saved link, a report being made, or
    /// whether there are kills to make one from, and the same fight in the Battles tab.
    pub(crate) fn fleet_br_view(&mut self, fleet_id: &str) -> BrView {
        let b = &mut self.fleet_br;
        if b.fleet != fleet_id {
            let backfill = std::mem::take(&mut b.backfill);
            *b = FleetBr { fleet: fleet_id.to_owned(), backfill, ..Default::default() };
            b.url = self.store.as_ref().and_then(|s| s.fleet_br(fleet_id));
            b.made_from = self
                .store
                .as_ref()
                .and_then(|s| s.kv_get(&format!("fleet_br_kills:{fleet_id}")))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
        if b.counted_at.is_none_or(|t| t.elapsed() > EVENTS_REREAD) {
            let (kills, moves) = self
                .store
                .as_ref()
                .map(|s| (s.fleet_kills(fleet_id), s.fleet_moves(fleet_id)))
                .unwrap_or_default();
            b.kills = std::sync::Arc::new(crate::fleets::movement::in_fleet_kills(&kills, &moves));
            b.counted_at = Some(std::time::Instant::now());
        }
        if let Some(done) = b.job.lock().unwrap().take() {
            b.busy = false;
            match done {
                Ok((url, key)) => {
                    if let Some(store) = &self.store {
                        store.set_fleet_br(fleet_id, &url, &key, chrono::Utc::now().timestamp());
                        store.kv_set(&format!("fleet_br_kills:{fleet_id}"), &b.pending_from.to_string());
                    }
                    b.url = Some(url);
                    b.made_from = b.pending_from;
                    b.error = None;
                }
                Err(e) => b.error = Some(e),
            }
        }
        BrView {
            url: b.url.clone(),
            busy: b.busy,
            backfilling: b.backfill.running.contains(fleet_id),
            has_kills: !b.kills.is_empty(),
            // A report made from other kills than the fleet has now: later ones came in, or some
            // turned out not to be the fleet's. br.evetools cannot edit one, so it is made again
            // and the saved link replaced.
            stale: b.url.is_some() && b.kills.len() != b.made_from,
            error: b.error.clone(),
        }
    }

    pub(crate) fn fleet_br_click(&mut self, click: BrClick, ctx: &egui::Context) {
        let b = &mut self.fleet_br;
        match click {
            BrClick::Create => {
                let windows = crate::fleets::br::windows(&b.kills);
                let (job, ctx) = (b.job.clone(), ctx.clone());
                b.pending_from = b.kills.len();
                b.busy = true;
                b.error = None;
                std::thread::spawn(move || {
                    *job.lock().unwrap() = Some(crate::fleets::br::create(&windows));
                    ctx.request_repaint();
                });
            }
            BrClick::Open => {
                if let Some(url) = &b.url {
                    let _ = open::that(url);
                }
            }
            BrClick::Copy => {
                if let Some(url) = &b.url {
                    ctx.copy_text(url.clone());
                }
            }
            BrClick::InApp => {
                // The Battles tab clusters by its own rules, which split a fight over several
                // systems into several battles, and it only holds kills near the player or intel.
                // So the fleet's kills are fetched where missing and tagged as one battle, the same
                // override a manual merge makes, and that battle is opened once it is rebuilt.
                let ids: Vec<i64> = b.kills.iter().filter(|k| k.pod_of == 0).map(|k| k.kill_id).collect();
                if ids.is_empty() {
                    return;
                }
                let held: std::collections::HashSet<i64> = self
                    .battles
                    .lock()
                    .unwrap()
                    .iter()
                    .flat_map(|bt| bt.engagements.iter().map(|e| e.kill_id))
                    .collect();
                self.battle_add_queue.lock().unwrap().extend(ids.iter().filter(|id| !held.contains(id)));
                let tagged = ids.clone();
                self.apply_battle_edit(ctx, move |s| {
                    let t = s.next_battle_tag();
                    for kid in &tagged {
                        s.set_battle_tag(*kid, Some(t));
                    }
                });
                self.battle_select_pending = Some((ids, std::time::Instant::now()));
                self.show_history = false;
                self.view = View::Battles;
            }
        }
    }

    /// A closed fleet whose fights were not recorded live gets them from zKillboard once: its
    /// participants' kills and losses during its run. Saved with the fleet, so it happens once.
    pub(crate) fn fleet_backfill_poll(&mut self) {
        if self.headless || !self.settings.fleet_enabled {
            return;
        }
        // First, before anything below can return: a backfill that finished is noticed every frame,
        // or its fleet reads as still fetching for good.
        let finished: Vec<String> = std::mem::take(&mut *self.fleet_br.backfill.done.lock().unwrap());
        for f in finished {
            self.fleet_br.backfill.running.remove(&f);
            if f == self.fleet_br.fleet {
                self.fleet_br.counted_at = None;
                self.fleet_map.events = None;
            }
        }
        let Some(store) = &self.store else { return };
        let job = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let Some(open) = st.open.value.as_ref() else { return };
            let Some(closed) = open.fleet.closed_at.as_deref() else { return };
            let ts = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp());
            let (Some(start), Some(end)) = (ts(&open.fleet.started_at), ts(closed)) else { return };
            let mut pilots: std::collections::HashSet<i64> = open.report.characters.iter().map(|c| c.id).collect();
            pilots.extend(open.composition.members().map(|m| m.character_id));
            pilots.retain(|p| *p > 0);
            (open.fleet.id.0.clone(), pilots, start, end)
        };
        let (fleet_id, pilots, start, end) = job;
        if !self.fleet_br.backfill.tried.insert(fleet_id.clone()) {
            return;
        }
        let key = format!("fleet_backfilled:{fleet_id}");
        if store.kv_get(&key).is_some() {
            return;
        }
        self.fleet_br.backfill.running.insert(fleet_id.clone());
        let done = self.fleet_br.backfill.done.clone();
        let ctx = self.ui_ctx.clone();
        std::thread::spawn(move || {
            let found = crate::zkill::fleet_history_kills(&fleet_id, &pilots, start, end);
            if let Ok(store) = crate::store::Store::open() {
                let found = crate::fleets::movement::in_fleet_kills(&found, &store.fleet_moves(&fleet_id));
                for k in &found {
                    store.add_fleet_kill(k);
                }
                store.kv_set(&key, &found.len().to_string());
            }
            done.lock().unwrap().push(fleet_id);
            ctx.request_repaint();
        });
    }

    /// Who was in one system at the moment on the slider, and in what. A name follows that pilot.
    fn fleet_map_system_dialog(
        &mut self,
        ctx: &egui::Context,
        pilots: &std::collections::BTreeMap<i64, Seen>,
        until: i64,
        with_date: bool,
        name: &dyn Fn(i64) -> String,
    ) {
        let Some(sys) = self.fleet_map.system_dialog else { return };
        let mut who: Vec<(i64, &Seen)> = pilots.iter().filter(|(_, s)| s.system_id == sys).map(|(id, s)| (*id, s)).collect();
        who.sort_by_key(|p| p.1.name.to_lowercase());
        let mut open = true;
        let mut follow: Option<i64> = None;
        let when = if until == i64::MAX { "now".to_owned() } else { format!("at {}", clock(until, with_date)) };
        egui::Window::new(format!("{} {when}", name(sys)))
            .id(egui::Id::new("fleet_map_system_dialog"))
            .open(&mut open)
            .collapsible(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("{} pilots", who.len())).strong());
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (id, s) in &who {
                        ui.horizontal(|ui| {
                            if ui.link(&s.name).on_hover_text("Follow this pilot on the map").clicked() {
                                follow = Some(*id);
                            }
                            ui.label(egui::RichText::new(&s.ship_name).weak());
                        });
                    }
                });
            });
        if let Some(id) = follow {
            self.fleet_map.focus = Some(id);
            open = false;
        }
        if !open {
            self.fleet_map.system_dialog = None;
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
                    members: self.fleet_members.clone(),
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
/// Closest two systems may be on screen before their dots start to shrink.
const FULL_SIZE_SPACING: f32 = 40.0;
/// The smallest dot radius: about two and a half pixels across.
const MIN_DOT_R: f32 = 1.25;
/// How many pixels a light year may span at full zoom.
const MAX_LY_PX: f32 = 400.0;
/// Screen distance to the nearest other dot over which a system's name fades in.
const NAME_FADE_FROM: f32 = 35.0;
const NAME_FADE_TO: f32 = 55.0;

/// Most of the way to grey, so an unused system still hints at its security but reads as background.
fn desaturate(c: egui::Color32) -> egui::Color32 {
    let l = (c.r() as f32 * 0.3 + c.g() as f32 * 0.59 + c.b() as f32 * 0.11) as u8;
    let mix = |v: u8| ((v as u16 + 3 * l as u16) / 4) as u8;
    egui::Color32::from_rgba_unmultiplied(mix(c.r()), mix(c.g()), mix(c.b()), c.a())
}

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

/// " by A, B and 3 more" for the fleet's pilots on a kill, or nothing when none are known.
fn by_names(ids: &[i64], known: &std::collections::HashMap<i64, String>) -> String {
    const SHOWN: usize = 3;
    let named: Vec<&str> = ids.iter().filter_map(|id| known.get(id).map(String::as_str)).collect();
    match named.len() {
        0 => String::new(),
        n if n <= SHOWN => format!(" by {}", named.join(", ")),
        n => format!(" by {} and {} more", named[..SHOWN].join(", "), n - SHOWN),
    }
}

/// Timeline lines for the fleet's kills and losses, or one pilot's. A pod goes on the line of the
/// ship loss it followed rather than a line of its own.
fn kill_rows(
    kills: &[crate::store::FleetKill],
    focus: Option<i64>,
    ship: &dyn Fn(i64) -> String,
    who: &dyn Fn(&[i64]) -> String,
    name: &dyn Fn(i64) -> String,
) -> Vec<TimelineRow> {
    kills
        .iter()
        .filter(|k| k.pod_of == 0)
        .filter(|k| focus.is_none_or(|f| k.victim_char == f || k.members.contains(&f)))
        .map(|k| {
            let pod: f64 = kills.iter().filter(|p| p.pod_of == k.kill_id).map(|p| p.value).sum();
            let pod = if pod > 0.0 { format!(" + pod {}", fmt_isk(pod)) } else { String::new() };
            let verb = if k.loss { "Lost" } else { "Killed" };
            let by = if k.loss { String::new() } else { who(&k.members) };
            let text = format!("{verb} {}'s {} ({}{pod}) in {}{by}", k.victim_name, ship(k.ship_type_id), fmt_isk(k.value), name(k.system_id));
            TimelineRow { at: k.at, tip: Some(text.clone()), text }
        })
        .collect()
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

/// The legs to draw up to `until`, for one pilot or (`focus` None) the bulk, and each pilot's last
/// system on the map.
///
/// A trip through wormhole space is one leg, a bridge of sorts, from where they went in to where
/// they came out: the map has no place for the systems in between.
fn paths(
    events: &[MoveEvent],
    until: i64,
    focus: Option<i64>,
    on_map: &dyn Fn(i64) -> bool,
) -> (Vec<(i64, i64, Option<Via>)>, std::collections::HashMap<i64, i64>) {
    let mut legs: Vec<(i64, i64, Option<Via>)> = Vec::new();
    // By pilot, the bulk being 0.
    let mut went_in: std::collections::HashMap<i64, i64> = Default::default();
    for e in events
        .iter()
        .filter(|e| e.at <= until && e.from_system > 0 && e.system_id > 0)
        .filter(|e| match focus {
            Some(f) => e.kind == Kind::Move && e.character_id == f,
            None => e.kind == Kind::Bulk,
        })
    {
        match (on_map(e.from_system), on_map(e.system_id)) {
            (true, true) => legs.push((e.from_system, e.system_id, e.via)),
            (true, false) => {
                went_in.insert(e.character_id, e.from_system);
            }
            (false, true) => {
                if let Some(a) = went_in.remove(&e.character_id) {
                    legs.push((a, e.system_id, Some(Via::Wormhole)));
                }
            }
            (false, false) => {}
        }
    }
    let mut last_on: std::collections::HashMap<i64, i64> = Default::default();
    for e in events.iter().filter(|e| e.at <= until && matches!(e.kind, Kind::Join | Kind::Move)) {
        if e.system_id > 0 && on_map(e.system_id) {
            last_on.insert(e.character_id, e.system_id);
        }
    }
    (legs, last_on)
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

    fn mv(at: i64, who: i64, from: i64, to: i64) -> MoveEvent {
        MoveEvent { character_id: who, from_system: from, system_id: to, via: Some(Via::Gate), ..MoveEvent::new(at, Kind::Move) }
    }

    /// Into wormhole space and out elsewhere is one leg from the way in to the way out, and while
    /// inside the pilot is placed by the way in.
    #[test]
    fn a_trip_through_a_wormhole_is_one_leg() {
        let on_map = |id: i64| id < 31_000_000;
        let join = MoveEvent { character_id: 7, system_id: 1, ..MoveEvent::new(0, Kind::Join) };
        let events = vec![join, mv(10, 7, 1, 2), mv(20, 7, 2, 31_000_001), mv(30, 7, 31_000_001, 31_000_002), mv(40, 7, 31_000_002, 5)];
        let (legs, last_on) = paths(&events, 35, Some(7), &on_map);
        assert_eq!(legs, vec![(1, 2, Some(Via::Gate))], "still inside: no way out yet");
        assert_eq!(last_on[&7], 2, "placed by the way in");
        let (legs, _) = paths(&events, 40, Some(7), &on_map);
        assert_eq!(legs, vec![(1, 2, Some(Via::Gate)), (2, 5, Some(Via::Wormhole))]);
    }

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
