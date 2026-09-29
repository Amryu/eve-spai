//! The scan route planner: every system worth scanning around a centre, split between scouts.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::*;
use crate::scanroute::{Plan, Scout};

/// A scout's colour, by its place in the plan.
pub(crate) const SCOUT_COLORS: [egui::Color32; 6] = [
    egui::Color32::from_rgb(0xF2, 0xB1, 0x34),
    egui::Color32::from_rgb(0x5A, 0xC8, 0xFF),
    egui::Color32::from_rgb(0xE8, 0x6B, 0xD1),
    egui::Color32::from_rgb(0x7B, 0xD6, 0x6B),
    egui::Color32::from_rgb(0xFF, 0x8A, 0x5A),
    egui::Color32::from_rgb(0xB5, 0x9C, 0xFF),
];

/// The most systems one scout is handed to the game at once; the rest follow on a replan.
const MAX_WAYPOINTS: usize = 50;
const MAX_RADIUS: u32 = 10;
const MAX_DETOUR: u32 = 5;
const MAX_TARGETS: usize = 400;
/// Stands in for the scout when nobody picked has a known location.
pub(crate) const NO_SCOUT: &str = "Route";

#[derive(Default)]
pub(crate) struct ScanState {
    pub plan: Option<Plan>,
    working: Option<Arc<Mutex<Option<Plan>>>>,
    /// Ticked off by hand in this plan: scanned, or not worth it.
    pub done: HashSet<i64>,
    /// Targets the plan was made for, for the summary.
    pub targets: usize,
}

impl ScanState {
    pub(crate) fn clear(&mut self) {
        *self = ScanState::default();
    }
}

impl SpaiApp {
    /// The systems within `radius` of `centre` the scan settings pick.
    pub(crate) fn scan_targets(&self, graph: &crate::geo::Systems, centre: i64, radius: u32) -> HashSet<i64> {
        use crate::whdata::{self, Class};
        let s = &self.settings.scan;
        let avoid = self.route_avoid(false);
        let scanned = match (s.skip_hours, self.store.as_ref()) {
            (0, _) | (_, None) => HashSet::new(),
            (h, Some(store)) => store.scanned_since(chrono::Utc::now().timestamp() - h as i64 * 3600),
        };
        let mut out: Vec<(u32, i64)> = graph
            .distances_from(centre, radius)
            .into_iter()
            .filter(|(id, _)| {
                if crate::geo::is_wormhole_system(*id) || crate::geo::is_no_transit(*id) || avoid.blocked(*id) || scanned.contains(id) || self.scan_route.done.contains(id) {
                    return false;
                }
                let Some(info) = graph.info_of(*id) else { return false };
                let class = whdata::class_of(*id, info.security, &info.region);
                let band = match class {
                    Class::Hs => "hs",
                    Class::Ls | Class::Turnur | Class::Tabbetzur => "ls",
                    Class::Ns | Class::Pochven => "ns",
                    _ => return false,
                };
                let kind = match s.look_for.as_str() {
                    "drifter" => crate::jove::has(*id),
                    "pochven" => !whdata::c729_targets(&info.name).is_empty(),
                    _ => true,
                };
                kind && (s.security.is_empty() || s.security.iter().any(|b| b == band))
            })
            .map(|(id, d)| (d, id))
            .collect();
        // Nearest first when there are too many to plan at once.
        out.sort_unstable();
        out.into_iter().take(MAX_TARGETS).map(|(_, id)| id).collect()
    }

    /// The picked characters where they are, or one scout at the centre when none of them is
    /// anywhere known.
    pub(crate) fn scan_scouts(&self, centre: i64) -> Vec<Scout> {
        let locations = self.player.lock().unwrap().locations.clone();
        let mut scouts: Vec<Scout> = self
            .settings
            .scan
            .scouts
            .iter()
            .filter_map(|n| Some(Scout { name: n.clone(), start: locations.get(n)?.0 }))
            .collect();
        if scouts.is_empty() {
            scouts.push(Scout { name: NO_SCOUT.into(), start: centre });
        }
        scouts
    }

    /// Plans again from where everyone is now, off the UI thread; the result turns up in
    /// [`Self::scan_poll`].
    pub(crate) fn scan_replan(&mut self) {
        let Some(&centre) = self.map_route_anchors.first() else {
            self.scan_route.plan = None;
            return;
        };
        let Some(graph) = self.route_graph() else { return };
        let radius = self.settings.scan.radius.min(MAX_RADIUS);
        let detour = self.settings.scan.detour.min(MAX_DETOUR);
        let targets = self.scan_targets(&graph, centre, radius);
        // Beyond the radius, but close enough that a route passing by could take them in.
        let extras: HashSet<i64> =
            if detour == 0 { HashSet::new() } else { self.scan_targets(&graph, centre, radius + detour).difference(&targets).copied().collect() };
        let scouts = self.scan_scouts(centre);
        let avoid = self.route_avoid(false);
        let depth = (radius + detour) * 2 + 4;
        self.scan_route.targets = targets.len();
        let run = move || crate::scanroute::plan(&graph, centre, &targets, &extras, detour, &scouts, depth, |id| !avoid.blocked(id));
        if cfg!(test) {
            self.scan_route.plan = Some(run());
            return;
        }
        let slot = Arc::new(Mutex::new(None));
        let out = slot.clone();
        self.scan_route.working = Some(slot);
        let _ = std::thread::Builder::new().name("scan-route".into()).spawn(move || {
            *out.lock().unwrap() = Some(run());
        });
    }

    pub(crate) fn scan_poll(&mut self, ctx: &egui::Context) {
        let Some(slot) = &self.scan_route.working else { return };
        let done = slot.lock().unwrap().take();
        match done {
            Some(p) => {
                self.scan_route.plan = Some(p);
                self.scan_route.working = None;
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
        }
    }

    /// Hands a scout's route to its client: waypoints enough to pin the autopilot to the planned
    /// path, Ansiblex limit and all.
    fn scan_send(&self, k: usize) {
        let Some(r) = self.scan_route.plan.as_ref().and_then(|p| p.scouts.get(k)) else { return };
        let (Some(base), Some(route)) = (self.systems.clone(), self.map_route_graph_now()) else { return };
        let game = crate::ansiblex::game_graph(&base, &self.settings.jump_bridges);
        let ok = |a: i64, b: i64| !game.is_bridge(a, b) || route.is_bridge(a, b);
        let mut wp = crate::routeforce::waypoints(&r.path, &game, &ok);
        // Every stop is a place to stop, not just to pass: each gets a waypoint of its own.
        for s in &r.stops {
            if !wp.contains(s) && *s != r.path[0] {
                wp.push(*s);
            }
        }
        let order: std::collections::HashMap<i64, usize> = r.path.iter().enumerate().rev().map(|(i, s)| (*s, i)).collect();
        wp.sort_by_key(|s| order.get(s).copied().unwrap_or(usize::MAX));
        wp.dedup();
        wp.truncate(MAX_WAYPOINTS);
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        crate::esi::set_route(cid, r.name.clone(), wp);
    }

    /// The route graph as the planner last built it, without building one.
    fn map_route_graph_now(&self) -> Option<std::sync::Arc<crate::geo::Systems>> {
        match self.map_route_zone.filter(|z| *z != self.settings.ansiblex_max_zone) {
            None => self.systems.clone(),
            Some(_) => self.map_route_graph.as_ref().map(|(_, _, g)| g.clone()),
        }
    }

    /// The Route dock's content for a scan route.
    pub(crate) fn scan_panel(&mut self, ui: &mut egui::Ui) {
        use crate::app::SteadySelect as _;
        use egui_phosphor::regular as icon;
        self.scan_poll(ui.ctx());
        let Some(&centre) = self.map_route_anchors.first() else {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Right-click a system on the map and pick Start Scan Route: it becomes the centre.").weak());
            return;
        };
        let geo = self.systems.clone();
        let name = move |id: i64| geo.as_ref().and_then(|g| g.info_of(id)).map_or_else(|| format!("#{id}"), |i| i.name.clone());
        let mut replan = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Around");
            ui.label(egui::RichText::new(name(centre)).strong().color(ui.visuals().hyperlink_color));
        });
        ui.horizontal(|ui| {
            ui.label("Jumps out");
            replan |= ui.add(egui::Slider::new(&mut self.settings.scan.radius, 1..=MAX_RADIUS)).changed();
        });
        ui.horizontal(|ui| {
            ui.label("Look for");
            let kinds = [
                ("any", "Any hole", "Every k-space system"),
                ("drifter", "Drifter holes", "Only systems with a Jove Observatory, the only ones drifter holes open in"),
                ("pochven", "Pochven holes", "Only systems a Pochven C729 can open in"),
            ];
            let current = kinds.iter().find(|k| k.0 == self.settings.scan.look_for).unwrap_or(&kinds[0]).1;
            egui::ComboBox::from_id_salt("scan_look_for").selected_text(current).show_ui(ui, |ui| {
                for (code, label, hint) in kinds {
                    if ui.menu_label(self.settings.scan.look_for == code, label).on_hover_text(hint).clicked() && self.settings.scan.look_for != code {
                        self.settings.scan.look_for = code.into();
                        replan = true;
                    }
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Security");
            for (code, label) in [("hs", "High"), ("ls", "Low"), ("ns", "Null")] {
                let set = &mut self.settings.scan.security;
                let on = set.is_empty() || set.iter().any(|c| c == code);
                if ui.menu_label(on, label).clicked() {
                    if set.is_empty() {
                        *set = ["hs", "ls", "ns"].iter().filter(|c| **c != code).map(|c| (*c).to_owned()).collect();
                    } else if on {
                        set.retain(|c| c != code);
                    } else {
                        set.push(code.into());
                    }
                    if set.len() == 3 {
                        set.clear();
                    }
                    replan = true;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Detours up to");
            replan |= ui
                .add(egui::DragValue::new(&mut self.settings.scan.detour).range(0..=MAX_DETOUR).suffix(" jumps"))
                .on_hover_text("A system outside the radius joins a route that passes this close to it, when going there and back is worth it. 0 keeps strictly to the radius.")
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Skip scanned in the last");
            replan |= ui.add(egui::DragValue::new(&mut self.settings.scan.skip_hours).range(0..=168).suffix(" h")).on_hover_text("0 plans every system, scanned or not").changed();
        });
        replan |= self.route_zone_combo(ui);
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Scouts").strong());
        let locations = self.player.lock().unwrap().locations.clone();
        let chars: Vec<String> = self.characters.iter().map(|c| c.name.clone()).collect();
        if chars.is_empty() {
            ui.label(egui::RichText::new("Sign in characters to plan a route for each.").weak());
        }
        for c in &chars {
            let at = locations.get(c).map(|(s, _)| *s);
            let mut on = self.settings.scan.scouts.contains(c);
            let label = match at {
                Some(s) => format!("{c}  \u{b7}  {}", name(s)),
                None => format!("{c}  \u{b7}  location unknown"),
            };
            let r = ui.add_enabled(at.is_some(), egui::Checkbox::new(&mut on, label));
            if r.on_disabled_hover_text("Its location is not known yet").changed() {
                if on {
                    self.settings.scan.scouts.push(c.clone());
                } else {
                    self.settings.scan.scouts.retain(|n| n != c);
                }
                replan = true;
            }
        }
        if replan {
            self.needs_save = true;
            self.scan_replan();
        }
        ui.separator();
        let Some(plan) = self.scan_route.plan.clone() else {
            ui.label(egui::RichText::new(if self.scan_route.working.is_some() { "Planning\u{2026}" } else { "Nothing planned yet" }).weak());
            return;
        };
        let detours: usize = plan.scouts.iter().map(|s| s.detours.len()).sum();
        let core = plan.scouts.iter().map(|s| s.stops.len()).sum::<usize>() - detours;
        let mut head = format!("{core} of {} systems", self.scan_route.targets);
        if detours > 0 {
            head.push_str(&format!(" + {detours} detour{}", if detours == 1 { "" } else { "s" }));
        }
        ui.label(egui::RichText::new(head).strong());
        if !plan.unreached.is_empty() {
            ui.label(egui::RichText::new(format!("{} out of reach with these settings", plan.unreached.len())).color(crate::theme::standing::WARNING));
        }
        let can_send = self.characters.iter().filter(|c| c.scopes.split_whitespace().any(|s| s == "esi-ui.write_waypoint.v1")).map(|c| c.name.clone()).collect::<HashSet<_>>();
        let mut send: Option<usize> = None;
        let mut tick: Option<i64> = None;
        for (k, r) in plan.scouts.iter().enumerate() {
            let color = SCOUT_COLORS[k % SCOUT_COLORS.len()];
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(icon::CIRCLE).color(color));
                ui.label(egui::RichText::new(&r.name).strong());
                ui.label(egui::RichText::new(format!("{} systems \u{b7} {} jumps", r.stops.len(), r.jumps)).weak());
                if r.name != NO_SCOUT {
                    let ok = can_send.contains(&r.name);
                    if ui
                        .add_enabled(ok && r.path.len() > 1, egui::Button::new(format!("{}  Set in game", icon::NAVIGATION_ARROW)))
                        .on_hover_text(format!("Waypoints for {} in the game, up to {MAX_WAYPOINTS}", r.name))
                        .on_disabled_hover_text("Sign this character in again to let it set waypoints")
                        .clicked()
                    {
                        send = Some(k);
                    }
                }
            });
            egui::CollapsingHeader::new(format!("Stops ({})", r.stops.len())).id_salt(("scan_stops", k)).show(ui, |ui| {
                for (i, s) in r.stops.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}.", i + 1)).weak());
                        ui.label(name(*s));
                        if r.detours.contains(s) {
                            ui.label(egui::RichText::new("detour").weak()).on_hover_text("Outside the radius, a few jumps off the route");
                        }
                        if ui.small_button(icon::CHECK).on_hover_text("Done: leave it out of the next plan").clicked() {
                            tick = Some(*s);
                        }
                    });
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(format!("{}  Replan from here", icon::ARROWS_CLOCKWISE)).on_hover_text("From where every scout is now, leaving out what was ticked off or scanned").clicked() {
                self.scan_replan();
            }
            if !self.scan_route.done.is_empty() && ui.button(format!("Forget {} ticked", self.scan_route.done.len())).clicked() {
                self.scan_route.done.clear();
                self.scan_replan();
            }
        });
        if let Some(k) = send {
            self.scan_send(k);
        }
        if let Some(s) = tick {
            self.scan_route.done.insert(s);
            self.scan_replan();
        }
    }

    /// Every scout's route on the map, each in its colour, the stops ringed.
    pub(crate) fn scan_draw(&self, painter: &egui::Painter, pos: &std::collections::HashMap<i64, egui::Pos2>, dot: f32) {
        let Some(plan) = &self.scan_route.plan else { return };
        for (k, r) in plan.scouts.iter().enumerate() {
            let color = SCOUT_COLORS[k % SCOUT_COLORS.len()];
            for w in r.path.windows(2) {
                if let (Some(a), Some(b)) = (pos.get(&w[0]), pos.get(&w[1])) {
                    painter.line_segment([*a, *b], egui::Stroke::new(3.0, color));
                }
            }
            let ring = (dot * 3.2).max(6.0);
            for s in &r.stops {
                if let Some(p) = pos.get(s) {
                    painter.circle_stroke(*p, ring, egui::Stroke::new(2.0, color));
                }
            }
            if let Some(p) = r.path.first().and_then(|s| pos.get(s)) {
                painter.circle_filled(*p, ring * 0.7, color);
            }
        }
    }
}
