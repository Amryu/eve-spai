//! Routes planned on the map: gate and jump legs, mixed or not, their waypoints, avoidance and the intel along them.

use super::*;

/// Whether planning a destination should also set it in the game.
///
/// Only while the plan is a plain start and destination: a bare destination carries
/// `clear_other_waypoints`, so pushing one on a route that has waypoints wipes them in the game.
/// A route with waypoints reaches the game through the panel's "Set in game" instead, whole.
pub(crate) fn plain_gate_plan(kind: &str, anchors: usize) -> bool {
    kind == "gate" && anchors == 2
}

/// The kind of route a mode starts new legs as: a mixed route asks, and until it has, gates.
pub(crate) fn default_leg(mode: &str) -> &'static str {
    if mode == "jump" { "jump" } else { "gate" }
}

fn mode_of(kind: &str) -> &'static str {
    match kind {
        "jump" => "jump",
        "mixed" => "mixed",
        "scan" => "scan",
        _ => "gate",
    }
}

impl SpaiApp {
    /// What a finished route drag can mean, arranged around where it was let go: a new route's kind,
    /// or, for a leg of a mixed route, whether to gate or jump it.
    ///
    /// One area with one disc and its buttons placed on it, since separate popups overlap each other.
    pub(crate) fn map_link_menu_ui(&mut self, ui: &mut egui::Ui) {
        let Some((from, to, at, leg_only)) = self.map_link_menu else { return };
        // A drag off a system of the route extends it: a plain route by its own kind, a mixed one
        // asks how to fly the new leg.
        let extends = self.map_route_anchors.len() > 1 && self.map_route_anchors.contains(&from);
        if extends && !leg_only {
            if self.map_route_kind == "mixed" {
                self.map_link_menu = Some((from, to, at, true));
            } else {
                self.map_link_menu = None;
                let kind = self.map_route_kind;
                self.map_take_route(kind, default_leg(kind), from, to);
            }
            return;
        }
        use egui_phosphor::regular as i;
        const R: f32 = 58.0;
        const BTN: egui::Vec2 = egui::vec2(104.0, 28.0);
        let opts: Vec<(&str, &str, &str)> = if leg_only {
            vec![("gate", i::SIGN_IN, tr!("Gate")), ("jump", i::SPIRAL, tr!("Jump")), ("cancel", i::X, tr!("Cancel"))]
        } else {
            vec![
                ("gate", i::SIGN_IN, tr!("Gate route")),
                ("jump", i::SPIRAL, tr!("Jump route")),
                ("mixed", i::SHUFFLE, tr!("Mixed route")),
                ("cancel", i::X, tr!("Cancel")),
            ]
        };
        let half = egui::vec2(R + BTN.x / 2.0 + 8.0, R + BTN.y / 2.0 + 8.0);
        let mut chose: Option<&str> = None;
        let area = egui::Area::new(egui::Id::new("map_link_menu").with(leg_only))
            .order(egui::Order::Foreground)
            .fixed_pos(at - half)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(half * 2.0, egui::Sense::hover());
                let c = rect.center();
                let v = ui.visuals().clone();
                let p = ui.painter();
                p.circle_filled(c, R + 16.0, v.window_fill.gamma_multiply(0.94));
                p.circle_stroke(c, R + 16.0, egui::Stroke::new(1.0, v.window_stroke.color));
                p.circle_filled(c, 3.5, v.hyperlink_color);
                for (idx, (kind, glyph, label)) in opts.iter().enumerate() {
                    let a = (idx as f32 / opts.len() as f32) * std::f32::consts::TAU
                        - std::f32::consts::FRAC_PI_2;
                    let bc = c + egui::vec2(a.cos() * R, a.sin() * R);
                    let br = egui::Rect::from_center_size(bc, BTN);
                    let text = if *kind == "cancel" {
                        egui::RichText::new(format!("{glyph}  {label}")).color(v.weak_text_color())
                    } else {
                        egui::RichText::new(format!("{glyph}  {label}"))
                    };
                    if ui.put(br, egui::Button::new(text)).clicked() {
                        chose = Some(kind);
                    }
                }
                rect
            });
        // On release, not on press: clearing the menu on press removes a button before its own
        // click can land.
        if chose.is_none() && ui.input(|i| i.pointer.any_click()) {
            let inside = ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| area.inner.contains(p));
            if !inside {
                self.map_link_menu = None;
            }
        }
        match chose {
            None => {}
            Some("cancel") => self.map_link_menu = None,
            // A new mixed route: the first leg is asked for like every later one.
            Some("mixed") => self.map_link_menu = Some((from, to, at, true)),
            Some(leg) if leg_only => {
                self.map_link_menu = None;
                let mode = if extends { self.map_route_kind } else { "mixed" };
                self.map_take_route(mode, default_leg(leg), from, to);
            }
            Some(kind) => {
                self.map_link_menu = None;
                self.map_take_route(kind, default_leg(kind), from, to);
            }
        }
    }

    /// Act on a route the user picked off the map: `mode` for the route, `leg` for the part just
    /// dragged.
    ///
    /// The same `web::route` the phone calls, so the two maps answer the question identically.
    pub(crate) fn map_take_route(&mut self, mode: &str, leg: &'static str, from: i64, to: i64) {
        self.map_route_opts.clear();
        self.map_route_at = 0;
        // A drag off a system already on the route rewrites it from there: everything after that
        // system goes, and the new target becomes the destination. Off the destination that is the
        // same thing as appending a waypoint. Starting anywhere else is a new route, which is the
        // only way to abandon one.
        match self.map_route_anchors.iter().position(|&a| a == from) {
            Some(at) if self.map_route_anchors.len() > 1 => {
                self.map_route_anchors.truncate(at + 1);
                self.map_route_anchors.push(to);
                self.map_leg_kinds.truncate(at);
                self.map_leg_kinds.push(leg);
            }
            _ => {
                self.map_route_anchors = vec![from, to];
                self.map_leg_kinds = vec![leg];
            }
        }
        self.map_route_kind = mode_of(mode);
        if plain_gate_plan(self.map_route_kind, self.map_route_anchors.len()) {
            self.web_set_destination(to);
            self.route_destination = Some(to);
        }
        self.map_replan_route();
    }

    /// Whether a route is being planned at all, which decides what the map's menu offers.
    /// The in-game destination whose route the map draws. Hidden while the planner is drawing one of
    /// its own, since two routes over the same systems cannot be told apart.
    pub(crate) fn set_route_shown(&self) -> Option<i64> {
        self.route_destination.filter(|_| self.map_route_anchors.is_empty())
    }

    pub(crate) fn map_route_kind_active(&self) -> bool {
        !self.map_route_anchors.is_empty()
    }

    /// Whether the map's menu offers the plain in-game "Set Destination". Not while a gate route is
    /// planned: that has its own "Set as Destination", and the plain one would throw the plan away.
    pub(crate) fn plain_destination_offered(&self) -> bool {
        !(self.map_route_kind_active() && self.map_route_kind == "gate")
    }

    /// How leg `i` is flown.
    pub(crate) fn map_leg_kind(&self, i: usize) -> &'static str {
        self.map_leg_kinds.get(i).copied().unwrap_or(default_leg(self.map_route_kind))
    }

    /// Whether any leg is flown as `kind`. With no leg yet, what the next one would be.
    pub(crate) fn map_route_has(&self, kind: &str) -> bool {
        let n = self.map_route_anchors.len().saturating_sub(1);
        if n == 0 {
            return self.map_route_kind == kind || (self.map_route_kind == "mixed" && kind == "gate");
        }
        (0..n).any(|i| self.map_leg_kind(i) == kind)
    }

    /// Flies leg `i` the other way. A plain route with one leg changed is a mixed one.
    pub(crate) fn map_set_leg_kind(&mut self, i: usize, kind: &'static str) {
        let n = self.map_route_anchors.len().saturating_sub(1);
        self.map_leg_kinds = (0..n).map(|k| self.map_leg_kind(k)).collect();
        if let Some(k) = self.map_leg_kinds.get_mut(i) {
            *k = kind;
        }
        if self.map_route_kind != "mixed" && self.map_leg_kinds.iter().any(|k| *k != self.map_route_kind) {
            self.map_route_kind = "mixed";
        }
        self.map_replan_route();
    }

    /// A route mode for the whole route: gate or jump sets every leg, mixed keeps them as they are.
    pub(crate) fn map_set_route_mode(&mut self, kind: &str) {
        self.map_route_kind = mode_of(kind);
        if matches!(self.map_route_kind, "gate" | "jump") {
            let k = self.map_route_kind;
            self.map_leg_kinds.iter_mut().for_each(|l| *l = k);
        }
    }

    /// Takes anchor `i` out, and the leg into it with it: the leg out of it now starts one earlier.
    pub(crate) fn map_route_remove_anchor(&mut self, i: usize) {
        if i == 0 || i >= self.map_route_anchors.len() {
            return;
        }
        self.map_route_anchors.remove(i);
        if i - 1 < self.map_leg_kinds.len() {
            self.map_leg_kinds.remove(i - 1);
        }
        self.map_replan_route();
    }

    /// Start a route here: this system, nowhere to go yet.
    pub(crate) fn map_route_start(&mut self, kind: &str, sid: i64) {
        self.map_route_kind = mode_of(kind);
        self.map_route_anchors = vec![sid];
        self.map_leg_kinds.clear();
        self.map_avoid_once.clear();
        self.map_route_opts.clear();
        self.map_route_legs.clear();
        self.scan_route.clear();
        // A scan route needs nothing more than its centre.
        if self.map_route_kind == "scan" {
            self.map_replan_route();
        }
    }

    /// One anchor becomes the destination. With only a start that completes it; with a route it
    /// replaces the far end and leaves the waypoints alone. `leg` says how to fly there; `None`
    /// keeps the last leg's way, or the route's.
    pub(crate) fn map_route_set_dest(&mut self, sid: i64, leg: Option<&'static str>) {
        if self.map_route_anchors.len() <= 1 {
            self.map_route_anchors.push(sid);
            self.map_leg_kinds = vec![leg.unwrap_or(default_leg(self.map_route_kind))];
        } else {
            self.map_route_anchors.pop();
            self.map_route_anchors.push(sid);
            if let (Some(k), Some(last)) = (leg, self.map_leg_kinds.last_mut()) {
                *last = k;
            }
        }
        if let Some(k) = leg {
            if self.map_route_kind != "mixed" && k != self.map_route_kind {
                self.map_route_kind = "mixed";
            }
        }
        if plain_gate_plan(self.map_route_kind, self.map_route_anchors.len()) {
            self.web_set_destination(sid);
        }
        self.map_replan_route();
    }

    /// A waypoint goes in before the destination, which is the difference between the two. `leg` is
    /// how to fly to it; the leg on to the destination keeps its way.
    pub(crate) fn map_route_add_waypoint(&mut self, sid: i64, leg: Option<&'static str>) {
        let at = self.map_route_anchors.len().saturating_sub(1);
        self.map_route_anchors.insert(at, sid);
        let k = leg.unwrap_or(default_leg(self.map_route_kind));
        let into = at.saturating_sub(1).min(self.map_leg_kinds.len());
        self.map_leg_kinds.insert(into, k);
        if self.map_route_kind != "mixed" && k != self.map_route_kind {
            self.map_route_kind = "mixed";
        }
        self.map_replan_route();
    }

    pub(crate) fn map_route_clear(&mut self) {
        self.map_route_anchors.clear();
        self.map_leg_kinds.clear();
        self.map_route_opts.clear();
        self.map_route_legs.clear();
        self.map_leg_pick.clear();
        self.map_forks.clear();
        self.map_avoid_once.clear();
        self.scan_route.clear();
    }

    /// Recompute the route from the anchors as they stand. Split out so a control in the window can
    /// change one input without the anchors being rebuilt around it.
    pub(crate) fn map_replan_route(&mut self) {
        // The route goes where the system goes: the dock, on its own tab. Otherwise a route planned
        // from the map has nowhere to be read without hunting for it.
        if !self.map_route_anchors.is_empty() {
            self.right_dock_open = true;
            self.right_dock_tab = RightDockTab::Route;
        }
        self.map_recompute_route();
    }

    /// [`Self::map_replan_route`] without bringing the dock up, for a change made elsewhere.
    pub(crate) fn map_recompute_route(&mut self) {
        self.map_route_opts.clear();
        self.map_route_at = 0;
        if self.map_route_kind == "scan" {
            self.scan_replan();
            return;
        }
        self.ensure_jump_systems();
        let Some(graph) = self.route_graph() else { return };
        let coords = self.jump_systems.clone().unwrap_or_default();
        let bridges = self.settings.intel_count_bridges;
        let danger = self.route_danger();
        let (mut avoid_gate, mut avoid_jump) = (self.route_avoid(false), self.route_avoid(true));
        let rules = self.route_rule_avoid();
        avoid_gate.once.extend(rules.iter().copied());
        avoid_jump.once.extend(rules);
        let holes =
            if self.settings.route_via_wormholes { self.wh_adjacency() } else { Default::default() };
        let kinds: Vec<&str> = (0..self.map_route_anchors.len().saturating_sub(1)).map(|i| self.map_leg_kind(i)).collect();
        let (legs, opts) = crate::web::route::chain(
            &graph,
            &coords,
            &self.map_route_anchors,
            &kinds,
            &crate::jumproute::SHIP_CLASSES[self.jump_ship.min(crate::jumproute::SHIP_CLASSES.len() - 1)],
            self.jump_jdc,
            self.jump_jfc,
            bridges,
            &avoid_gate,
            &avoid_jump,
            &holes,
            &self.map_leg_pick,
            &self.map_forks,
        );
        self.map_route_legs = legs;
        self.map_route_opts = opts;
        crate::web::route::annotate(&mut self.map_route_opts, &danger);
        let anchors = self.map_route_anchors.clone();
        crate::web::route::mark_anchors(&mut self.map_route_opts, &anchors);
        // After the anchors: a fork is a choice within a leg, so the marking needs to know where the
        // legs end.
        crate::web::route::mark_forks(&mut self.map_route_opts, &graph, bridges, &avoid_gate, &holes);
    }

    /// The avoid lists this route is planned against: the gate list for gate legs, the jump list for
    /// jump legs.
    pub(crate) fn map_route_avoid_lists(&self) -> Vec<bool> {
        [(false, "gate"), (true, "jump")].into_iter().filter(|(_, k)| self.map_route_has(k)).map(|(j, _)| j).collect()
    }

    /// The graph the route planner walks: the shared one, or one re-laid for the route's own
    /// Ansiblex zone limit.
    pub(crate) fn route_graph(&mut self) -> Option<std::sync::Arc<crate::geo::Systems>> {
        let base = self.systems.clone()?;
        let zone = self.map_route_zone.filter(|z| *z != self.settings.ansiblex_max_zone);
        let regions = self.settings.route_region_gates;
        if zone.is_none() && regions {
            return Some(base);
        }
        let key = std::sync::Arc::as_ptr(&base) as usize;
        let z = zone.unwrap_or(u8::MAX);
        if let Some((k, zz, r, g)) = &self.map_route_graph {
            if *k == key && *zz == z && *r == regions {
                return Some(g.clone());
            }
        }
        let mut g = match zone {
            Some(0) => base.gates_only(),
            Some(zone) => crate::ansiblex::with_max_zone(&base, &self.settings, zone),
            None => (*base).clone(),
        };
        if !regions {
            g = g.without_region_gates();
        }
        let g = std::sync::Arc::new(g);
        self.map_route_graph = Some((key, z, regions, g.clone()));
        Some(g)
    }

    /// The systems the route rules keep a route out of: gate camps, security bands, busy systems
    /// and sov held by the alliances named. The route's own anchors are never in it.
    pub(crate) fn route_rule_avoid(&self) -> std::collections::HashSet<i64> {
        let st = &self.settings;
        let mut out = std::collections::HashSet::new();
        let Some(geo) = self.systems.as_ref() else { return out };
        if st.route_avoid_camps {
            let now = crate::clock::utc().timestamp();
            out.extend(
                self.camps.lock().unwrap().camped(now).into_iter().filter(|(_, l)| *l >= crate::camp::CampLevel::Possible).map(|(id, _)| id),
            );
        }
        let sov: std::collections::HashSet<String> = st.route_avoid_sov.iter().map(|s| s.to_lowercase()).collect();
        if st.route_max_kills > 0 || !sov.is_empty() {
            for (id, f) in self.system_status.lock().unwrap_or_else(|e| e.into_inner()).iter() {
                if st.route_max_kills > 0 && f.ship_kills > st.route_max_kills {
                    out.insert(*id);
                }
                if f.sov.as_deref().is_some_and(|h| sov.contains(&h.to_lowercase())) {
                    out.insert(*id);
                }
            }
        }
        if st.route_sec != [true; 3] {
            // J-space has no band to keep out of: filtering Thera as "null" would defeat a hole route.
            out.extend(geo.all_ids().filter(|id| !crate::geo::is_wormhole_system(*id)).filter(|id| {
                geo.info_of(*id).is_some_and(|i| {
                    let band = if i.security >= 0.45 { 0 } else if i.security > 0.0 { 1 } else { 2 };
                    !st.route_sec[band]
                })
            }));
        }
        for a in &self.map_route_anchors {
            out.remove(a);
        }
        out
    }

    /// The intel behind a route warning, as its own window.
    ///
    /// "Danger intel 4m" is a summary of something somebody wrote, and the words are the part worth
    /// reading. Rendered with the feed's own row, so a card here is the card there.
    pub(crate) fn route_intel_window(&mut self, ctx: &egui::Context) {
        let Some(sid) = self.map_intel_for else { return };
        let name = self
            .systems
            .as_ref()
            .and_then(|g| g.info_of(sid).map(|i| i.name.clone()))
            .unwrap_or_default();
        let reports: Vec<crate::intel::IntelReport> = {
            let st = self.intel_state.lock().unwrap_or_else(|e| e.into_inner());
            st.reports
                .iter()
                .filter(|r| r.systems.iter().any(|s| s.id == sid))
                .rev()
                .take(30)
                .cloned()
                .collect()
        };
        let mut open = true;
        egui::Window::new(format!("{}  {name}", egui_phosphor::regular::WARNING))
            .id(egui::Id::new("route_intel"))
            .collapsible(false)
            .default_width(420.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if reports.is_empty() {
                    ui.label(
                        egui::RichText::new(tr!("Nothing in the feed for this system any more.")).weak(),
                    );
                    return;
                }
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for r in &reports {
                        let sev = severity_of(r, &self.settings.severity);
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  {}",
                                fmt_age(
                                    (crate::clock::utc().timestamp() - r.received).max(0)
                                ),
                                r.text.trim()
                            ))
                            .color(severity_color(sev)),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} · {}", r.reporter, r.channel))
                                .weak()
                                .size(11.0),
                        );
                        ui.separator();
                    }
                });
            });
        if !open {
            self.map_intel_for = None;
        }
    }

    /// Where a route may not go: the persistent list for this kind of route, plus whatever was
    /// avoided for the route currently being planned.
    pub(crate) fn route_avoid(&self, jump: bool) -> crate::web::route::Avoid {
        let always = if jump {
            &self.settings.route_avoid_jump
        } else {
            &self.settings.route_avoid_gate
        };
        crate::web::route::Avoid {
            always: always.iter().copied().collect(),
            once: self.map_avoid_once.clone(),
        }
    }

    /// The danger map from the app's own state, for the route views.
    pub(crate) fn route_danger(&self) -> std::collections::HashMap<i64, crate::web::route::HopWarning> {
        let kills: Vec<(i64, u32, u32)> = self
            .system_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, f)| (*id, f.ship_kills, f.pod_kills))
            .collect();
        let st = self.intel_state.lock().unwrap_or_else(|e| e.into_inner());
        crate::web::route::danger_from_reports(
            &st.reports,
            &self.settings.severity,
            self.settings.intel_ttl_secs,
            crate::clock::utc().timestamp(),
            &kills,
        )
    }

    /// The readout beside the system a route drag is aimed at.
    ///
    /// Three numbers, because they answer different questions and are often far apart: a system four
    /// light years away can be twenty gates out.
    pub(crate) fn map_link_tip(&self, ui: &mut egui::Ui, at: egui::Pos2, from: i64, to: i64) {
        let Some(graph) = &self.systems else { return };
        let ly = self.jump_systems.as_ref().and_then(|c| {
            let find = |id: i64| c.iter().find(|s| s.id == id);
            find(from).zip(find(to)).map(|(a, b)| crate::map::ly_distance(a, b))
        });
        let gates = graph.jumps_gates_only(from, to, JUMP_SCAN_CAP);
        let bridged = graph.jumps(from, to, JUMP_SCAN_CAP);
        let jumps = |n: Option<u32>| match n {
            Some(1) => tr!("1 jump").to_owned(),
            Some(n) => trf!("{n} jumps", n = n),
            None => tr!("no route").to_owned(),
        };
        egui::Area::new(egui::Id::new("map_link_tip"))
            .order(egui::Order::Tooltip)
            .fixed_pos(at + egui::vec2(14.0, 14.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        if let Some(i) = graph.info_of(to) {
                            ui.label(egui::RichText::new(&i.name).strong());
                        }
                        if let Some(ly) = ly {
                            ui.label(egui::RichText::new(trf!("{ly} ly", ly = format!("{:.1}", ly))).weak());
                        }
                        ui.label(egui::RichText::new(trf!("{v} by gate", v = jumps(gates))).weak());
                        if bridged.is_some() && bridged != gates {
                            ui.label(
                                egui::RichText::new(trf!("{v} with bridges", v = jumps(bridged)))
                                    .weak(),
                            );
                        }
                    });
                });
            });
    }
}
