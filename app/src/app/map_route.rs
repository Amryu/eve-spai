//! Routes planned on the map: gate, jump and titan legs, their waypoints, avoidance and the intel along them.

use super::*;

impl SpaiApp {
    /// The four things a finished route drag can mean, arranged around where it was let go.
    ///
    /// One area with one disc and four buttons placed on it, rather than four popups: four separate
    /// frames overlapped each other and looked like a mistake.
    pub(crate) fn map_link_menu_ui(&mut self, ui: &mut egui::Ui) {
        let Some((from, to, at)) = self.map_link_menu else { return };
        // A drag off the current destination is adding a waypoint to a route that already has a
        // kind, and the kind is one answer per route rather than one per leg.
        if self.map_route_anchors.len() > 1 && self.map_route_anchors.contains(&from) {
            self.map_link_menu = None;
            let kind = self.map_route_kind;
            self.map_take_route(kind, from, to);
            return;
        }
        use egui_phosphor::regular as i;
        const R: f32 = 58.0;
        const BTN: egui::Vec2 = egui::vec2(96.0, 28.0);
        let opts: [(&str, &str, &str); 4] = [
            ("gate", i::SIGN_IN, "Gate route"),
            ("jump", i::SPIRAL, "Jump route"),
            ("titan", i::CROSSHAIR_SIMPLE, "Titan route"),
            ("cancel", i::X, "Cancel"),
        ];
        let half = egui::vec2(R + BTN.x / 2.0 + 8.0, R + BTN.y / 2.0 + 8.0);
        let mut chose: Option<&str> = None;
        let area = egui::Area::new(egui::Id::new("map_link_menu"))
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
        // On release, not on press: clearing the menu the moment a button was pressed took the
        // button away before its own click could land, which is why none of them worked.
        if chose.is_none() && ui.input(|i| i.pointer.any_click()) {
            let inside = ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| area.inner.contains(p));
            if !inside {
                self.map_link_menu = None;
            }
        }
        if let Some(kind) = chose {
            self.map_link_menu = None;
            self.map_take_route(kind, from, to);
        }
    }

    /// Act on a route the user picked off the map.
    ///
    /// The same `web::route` the phone calls, so the two maps answer the question identically rather
    /// than each having its own idea of what a titan route is.
    pub(crate) fn map_take_route(&mut self, kind: &str, from: i64, to: i64) {
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
            }
            _ => self.map_route_anchors = vec![from, to],
        }
        if kind == "gate" {
            self.web_set_destination(to);
            self.route_destination = Some(to);
        }
        self.map_route_kind = match kind {
            "jump" => "jump",
            "titan" => "titan",
            _ => "gate",
        };
        self.map_replan_route();
    }

    /// Whether a route is being planned at all, which decides what the map's menu offers.
    pub(crate) fn map_route_kind_active(&self) -> bool {
        !self.map_route_anchors.is_empty()
    }

    /// Start a route here: this system, nowhere to go yet, and a kind chosen once for the whole
    /// route rather than once per leg.
    pub(crate) fn map_route_start(&mut self, kind: &str, sid: i64) {
        self.map_route_kind = match kind {
            "jump" => "jump",
            "titan" => "titan",
            _ => "gate",
        };
        self.map_route_anchors = vec![sid];
        self.map_avoid_once.clear();
        self.map_titans.clear();
        self.map_route_opts.clear();
        self.map_route_legs.clear();
    }

    /// One anchor becomes the destination. With only a start that completes it; with a route it
    /// replaces the far end and leaves the waypoints alone.
    pub(crate) fn map_route_set_dest(&mut self, sid: i64) {
        if self.map_route_anchors.len() <= 1 {
            self.map_route_anchors.push(sid);
        } else {
            self.map_route_anchors.pop();
            self.map_route_anchors.push(sid);
        }
        if self.map_route_kind == "gate" {
            self.web_set_destination(sid);
        }
        self.map_replan_route();
    }

    /// A waypoint goes in before the destination, which is the difference between the two.
    pub(crate) fn map_route_add_waypoint(&mut self, sid: i64) {
        let at = self.map_route_anchors.len().saturating_sub(1);
        self.map_route_anchors.insert(at, sid);
        self.map_replan_route();
    }

    pub(crate) fn map_route_clear(&mut self) {
        self.map_titans.clear();
        self.map_route_anchors.clear();
        self.map_route_opts.clear();
        self.map_route_legs.clear();
        self.map_leg_pick.clear();
        self.map_avoid_once.clear();
    }

    /// Recompute the route from the anchors as they stand. Split out so a control in the window can
    /// change one input without the anchors being rebuilt around it.
    pub(crate) fn map_replan_route(&mut self) {
        self.map_route_opts.clear();
        self.map_route_at = 0;
        // The route goes where the system goes: the dock, on its own tab. Otherwise a route planned
        // from the map has nowhere to be read without hunting for it.
        if !self.map_route_anchors.is_empty() {
            self.right_dock_open = true;
            self.right_dock_tab = RightDockTab::Route;
        }
        self.ensure_jump_systems();
        let Some(graph) = self.systems.clone() else { return };
        let coords = self.jump_systems.clone().unwrap_or_default();
        // A titan at JDC V. The same figure the rescue planner uses, stated here because that one is
        // behind a feature flag and this is not.
        const TITAN_LY: f64 = 6.0;
        let bridges = self.settings.intel_count_bridges;
        let danger = self.route_danger();
        let avoid = self.route_avoid(self.map_route_kind == "jump");
        let holes =
            if self.settings.route_via_wormholes { self.wh_adjacency() } else { Default::default() };
        let (legs, opts) = crate::web::route::chain(
            &graph,
            &coords,
            &self.map_route_anchors,
            self.map_route_kind,
            &crate::jumproute::SHIP_CLASSES[self.jump_ship.min(crate::jumproute::SHIP_CLASSES.len() - 1)],
            self.jump_jdc,
            self.jump_jfc,
            TITAN_LY,
            self.map_titan_at_start,
            &self.map_titans.clone(),
            self.map_titan_self_jump,
            bridges,
            &avoid,
            &holes,
            &self.map_leg_pick,
        );
        self.map_route_legs = legs;
        self.map_route_opts = opts;
        crate::web::route::annotate(&mut self.map_route_opts, &danger);
        let anchors = self.map_route_anchors.clone();
        crate::web::route::mark_anchors(&mut self.map_route_opts, &anchors);
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
                        egui::RichText::new("Nothing in the feed for this system any more.").weak(),
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
                                    (chrono::Utc::now().timestamp() - r.received).max(0)
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
            chrono::Utc::now().timestamp(),
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
            Some(1) => "1 jump".to_owned(),
            Some(n) => format!("{n} jumps"),
            None => "no route".to_owned(),
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
                            ui.label(egui::RichText::new(format!("{ly:.1} ly")).weak());
                        }
                        ui.label(egui::RichText::new(format!("{} by gate", jumps(gates))).weak());
                        if bridged.is_some() && bridged != gates {
                            ui.label(
                                egui::RichText::new(format!("{} with bridges", jumps(bridged)))
                                    .weak(),
                            );
                        }
                    });
                });
            });
    }
}
