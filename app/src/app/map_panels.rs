//! The map's side panels and windows: the route panel, travel and threat panels, controls, search and pop-outs.

use super::*;

impl SpaiApp {
    /// The route panel: the same thing the browser's route window shows, in the sidebar.
    ///
    /// The route is the subject. Everything about the ship is one collapsed header away, and only
    /// for the kind of route that has a ship.
    pub(crate) fn jump_plan_content(&mut self, ui: &mut egui::Ui) {
        use crate::jumproute::{max_range_ly, SHIP_CLASSES};
        use egui_phosphor::regular as icon;

        let fmt_min = |m: f64| -> String {
            let t = m.round() as i64;
            if t >= 60 {
                format!("{}h {:02}m", t / 60, t % 60)
            } else {
                format!("{t}m")
            }
        };

        ui.add_space(4.0);
        let mut replan = false;
        ui.horizontal(|ui| {
            for (kind, label) in [("gate", "Gates"), ("jump", "Jumps"), ("titan", "Titan")] {
                if ui.selectable_label(self.map_route_kind == kind, label).clicked()
                    && self.map_route_kind != kind
                {
                    self.map_route_kind = match kind {
                        "jump" => "jump",
                        "titan" => "titan",
                        _ => "gate",
                    };
                    replan = true;
                }
            }
        });

        if self.map_route_anchors.len() < 2 {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Drag from one system to another on the map, or right-click a system to start a \
                     route.",
                )
                .weak(),
            );
            if replan {
                self.map_replan_route();
            }
            return;
        }

        // The systems the user named, in order, each removable. The start is not: a route has to
        // begin somewhere, and removing it would leave an anchor list that means nothing.
        let anchors = self.map_route_anchors.clone();
        let name = |id: i64, g: &Option<std::sync::Arc<crate::geo::Systems>>| {
            g.as_ref().and_then(|s| s.info_of(id).map(|i| i.name.clone())).unwrap_or_default()
        };
        let mut drop_anchor: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, &id) in anchors.iter().enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new(icon::ARROW_RIGHT).weak());
                }
                let txt = egui::RichText::new(name(id, &self.systems)).strong();
                ui.label(if i == 0 || i == anchors.len() - 1 {
                    txt.color(ui.visuals().hyperlink_color)
                } else {
                    txt
                });
                if i > 0 && ui.small_button(icon::X).on_hover_text("Remove").clicked() {
                    drop_anchor = Some(i);
                }
            }
        });
        if let Some(i) = drop_anchor {
            self.map_route_anchors.remove(i);
            replan = true;
        }

        if self.map_route_kind == "titan" {
            if ui
                .checkbox(&mut self.map_titan_at_start, "Titan is in the starting system")
                .changed()
            {
                replan = true;
            }
            if self.map_titan_at_start
                && ui
                    .checkbox(&mut self.map_titan_self_jump, "Titan may reposition first")
                    .on_hover_text(
                        "The titan jumps somewhere that bridges better and the fleet gates out to \
                         meet it. Often the difference between two gates and twenty.",
                    )
                    .changed()
            {
                replan = true;
            }
        }
        if self.map_route_kind != "jump"
            && ui
                .checkbox(&mut self.settings.route_via_wormholes, "Route via scanned wormholes")
                .changed()
        {
            self.needs_save = true;
            replan = true;
        }

        if self.map_route_kind == "jump" {
            egui::CollapsingHeader::new(format!(
                "{}  {} · {:.1} ly",
                icon::SPIRAL,
                SHIP_CLASSES[self.jump_ship].name,
                max_range_ly(&SHIP_CLASSES[self.jump_ship], self.jump_jdc)
            ))
            .id_salt("route_ship")
            .show(ui, |ui| {
                egui::ComboBox::from_id_salt(ui.id().with("jump_ship"))
                    .selected_text(SHIP_CLASSES[self.jump_ship].name)
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, c) in SHIP_CLASSES.iter().enumerate() {
                            if ui.selectable_value(&mut self.jump_ship, i, c.name).changed() {
                                replan = true;
                            }
                        }
                    });
                if let Some((jdc, jfc)) = self.jump_skills.lock().unwrap().take() {
                    self.jump_jdc = jdc.min(5);
                    self.jump_jfc = jfc.min(5);
                    replan = true;
                }
                ui.horizontal(|ui| {
                    ui.label("JDC").on_hover_text("Jump Drive Calibration (range)");
                    replan |= ui
                        .add(egui::DragValue::new(&mut self.jump_jdc).range(0..=5))
                        .changed();
                    ui.label("JFC").on_hover_text("Jump Fuel Conservation (fuel)");
                    replan |= ui
                        .add(egui::DragValue::new(&mut self.jump_jfc).range(0..=5))
                        .changed();
                });
                if self.active_character != "No character"
                    && ui
                        .button("Use my skills (ESI)")
                        .on_hover_text("Needs the skills scope on this character")
                        .clicked()
                {
                    let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                    crate::esi::fetch_jump_skills(
                        cid,
                        self.active_character.clone(),
                        self.jump_skills.clone(),
                        ui.ctx().clone(),
                    );
                }
            });
        }

        // What the route is being planned around, and which systems those are: a count on its own is
        // not something anyone can check.
        let avoid = self.route_avoid(self.map_route_kind == "jump");
        let listed = self
            .systems
            .as_ref()
            .map(|g| crate::web::route::avoided(g, &avoid))
            .unwrap_or_default();
        if !listed.is_empty() {
            let mut stop: Option<(i64, bool)> = None;
            egui::CollapsingHeader::new(format!(
                "{}  avoiding {} system{}",
                icon::EYE_SLASH,
                listed.len(),
                if listed.len() == 1 { "" } else { "s" }
            ))
            .id_salt("route_avoid")
            .show(ui, |ui| {
                for a in &listed {
                    ui.horizontal(|ui| {
                        ui.label(&a.name);
                        if a.always {
                            ui.label(egui::RichText::new("always").weak().size(11.0));
                        }
                        if ui.small_button(icon::X).on_hover_text("Stop avoiding").clicked() {
                            stop = Some((a.id, a.always));
                        }
                    });
                }
            });
            if let Some((id, always)) = stop {
                if always {
                    let jump = self.map_route_kind == "jump";
                    let list = if jump {
                        &mut self.settings.route_avoid_jump
                    } else {
                        &mut self.settings.route_avoid_gate
                    };
                    list.retain(|&s| s != id);
                    self.needs_save = true;
                } else {
                    self.map_avoid_once.remove(&id);
                }
                replan = true;
            }
        }

        if replan {
            self.map_replan_route();
        }

        let Some(o) = self.map_route_opts.get(self.map_route_at) else {
            ui.separator();
            ui.label(
                egui::RichText::new("No route with these settings.")
                    .color(crate::theme::standing::WARNING),
            );
            return;
        };
        ui.separator();
        let mut head = format!("{} jumps", o.jumps);
        if o.gates > 0 {
            head.push_str(&format!(" · {} gates", o.gates));
        }
        if o.total_ly > 0.0 {
            head.push_str(&format!(" · {:.1} ly", o.total_ly));
        }
        ui.label(egui::RichText::new(head).strong());
        if let Some(saved) = o.saved {
            let thin = saved <= 2;
            let mut line = format!("saves {saved} gate{}", if saved == 1 { "" } else { "s" });
            if thin {
                line.push_str(" — barely worth the cyno, a direct route may be simpler");
            }
            ui.label(egui::RichText::new(line).color(if thin {
                crate::theme::standing::WARNING
            } else {
                crate::theme::standing::FRIENDLY
            }));
        }
        if let Some(d) = &o.detour {
            ui.label(
                egui::RichText::new(format!("{}  {d}", egui_phosphor::regular::EYE_SLASH))
                    .color(crate::theme::standing::WARNING),
            );
        }
        if let Some(tj) = &o.titan_jump {
            ui.label(
                egui::RichText::new(format!(
                    "{}  Titan jumps {} → {}, {:.1} ly",
                    egui_phosphor::regular::STAR_FOUR,
                    tj.from_name,
                    tj.to_name,
                    tj.ly
                ))
                .color(egui::Color32::from_rgb(0xFF, 0x7A, 0x3D)),
            );
        }
        if let Some(n) = &o.note {
            ui.label(egui::RichText::new(n).weak());
        }

        // Alternatives, one row per leg that has more than one way to fly it. Same jump count, so
        // the row reads as "these cost the same, shortest first".
        let legs: Vec<(String, String, Vec<String>, bool)> = self
            .map_route_legs
            .iter()
            .map(|l| {
                (
                    l.from_name.clone(),
                    l.to_name.clone(),
                    // The label names the system that makes this option different, since every
                    // option has the same jump count by construction.
                    l.options.iter().map(|o| o.label.clone()).collect(),
                    l.whole_route,
                )
            })
            .collect();
        let mut pick: Option<(usize, usize)> = None;
        for (i, (from, to, opts, whole)) in legs.iter().enumerate() {
            // A titan leg's options are the route's options, already shown in the option row
            // above, and this leg's pick does not feed them.
            if opts.len() < 2 || *whole {
                continue;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{from} → {to}")).weak().size(11.0));
                for (k, label) in opts.iter().enumerate() {
                    let on = self.map_leg_pick.get(i).copied().unwrap_or(0) == k;
                    if ui.selectable_label(on, label).clicked() {
                        pick = Some((i, k));
                    }
                }
            });
        }
        if let Some((i, k)) = pick {
            if self.map_leg_pick.len() <= i {
                self.map_leg_pick.resize(i + 1, 0);
            }
            self.map_leg_pick[i] = k;
            self.map_replan_route();
            return;
        }

        let ingame = self
            .map_route_opts
            .get(self.map_route_at)
            .map(|o| crate::web::route::ingame_waypoints(o, self.player_system()))
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            let has_char = self.active_character != "No character";
            if ui
                .add_enabled(has_char && !ingame.is_empty(), egui::Button::new(format!("{}  Set in game", icon::MAP_PIN_LINE)))
                .on_hover_text(if self.map_route_kind == "gate" {
                    "Set this route in the game, one waypoint per system"
                } else {
                    "Set waypoints in the game at both ends of each leg you fly yourself"
                })
                .on_disabled_hover_text("Log a character in to route in the game")
                .clicked()
            {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                crate::esi::set_route(cid, self.active_character.clone(), ingame.clone());
                self.note_ingame_route();
            }
            if ui.button(format!("{}  Save route", icon::COPY)).clicked() {
                self.map_save_name.clear();
                self.map_save_open = true;
            }
            if ui.button(format!("{}  Load…", icon::ARROW_SQUARE_OUT)).clicked() {
                self.map_load_open = true;
            }
        });

        ui.separator();
        let hops: Vec<crate::web::route::Hop> = self
            .map_route_opts
            .get(self.map_route_at)
            .map(|o| {
                o.hops
                    .iter()
                    .map(|h| crate::web::route::Hop {
                        id: h.id,
                        name: h.name.clone(),
                        security: h.security,
                        kind: h.kind,
                        ly: h.ly,
                        fuel: h.fuel,
                        fatigue_min: h.fatigue_min,
                        reactivation_min: h.reactivation_min,
                        warn: h.warn,
                        anchor: h.anchor,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut avoid_now: Option<i64> = None;
        let mut unavoid_now: Option<i64> = None;
        let mut waypoint_now: Option<i64> = None;
        let mut titan_now: Option<(i64, bool)> = None;
        let mut drop_anchor_row: Option<usize> = None;
        let mut alts_for: Option<usize> = None;
        let mut show_intel: Option<i64> = None;
        let mut warn_intel: Option<i64> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, h) in hops.iter().enumerate() {
                // A system the user named gets its own ground, so the route reads as the legs it was
                // built from rather than as one long list.
                let frame = if h.anchor {
                    egui::Frame::new()
                        .fill(ui.visuals().hyperlink_color.gamma_multiply(0.16))
                        .inner_margin(egui::Margin::symmetric(4, 1))
                } else {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 1))
                };
                let cost = h.fuel.zip(h.fatigue_min).zip(h.reactivation_min);
                let warn = h.warn.filter(|w| warn_text(w).is_some());
                // One button rather than one per action: a row is a system and a distance, and three
                // buttons beside that is more chrome than content. It keeps the right edge of the
                // row on every kind of hop, so it never moves with the costs or the warning.
                let mut menu = |ui: &mut egui::Ui| {
                    {
                        ui.menu_button(icon::DOTS_THREE, |ui| {
                            if h.warn.is_some_and(|w| w.sev >= crate::web::route::WARN_SEVERITY)
                                && ui.button("Show intel").clicked()
                            {
                                show_intel = Some(h.id);
                                ui.close();
                            }
                            // An anchor is a choice the user made, so the row it sits on is where
                            // taking it back belongs. Not the start: a route has to begin somewhere.
                            let anchor_at =
                                self.map_route_anchors.iter().position(|&a| a == h.id);
                            if let Some(i) = anchor_at.filter(|&i| i > 0) {
                                let last = i == self.map_route_anchors.len() - 1;
                                let label =
                                    if last { "Remove destination" } else { "Remove waypoint" };
                                if ui.button(label).clicked() {
                                    drop_anchor_row = Some(i);
                                    ui.close();
                                }
                            }
                            if !h.anchor {
                                let on = self.map_avoid_once.contains(&h.id);
                                if ui
                                    .button(if on { "Stop avoiding" } else { "Avoid this system" })
                                    .clicked()
                                {
                                    if on {
                                        unavoid_now = Some(h.id);
                                    } else {
                                        avoid_now = Some(h.id);
                                    }
                                    ui.close();
                                }
                                if ui.button("Add waypoint here").clicked() {
                                    waypoint_now = Some(h.id);
                                    ui.close();
                                }
                            }
                            if self.map_route_kind == "jump"
                                && i > 0
                                && i + 1 < hops.len()
                                && ui.button("Other systems between…").clicked()
                            {
                                alts_for = Some(i);
                                ui.close();
                            }
                            if ui.button("Show info").clicked() {
                                self.map_selected = Some(h.id);
                                self.right_dock_open = true;
                                self.right_dock_tab = RightDockTab::System;
                                ui.close();
                            }
                            if self.map_route_kind == "titan" {
                                let t = self.map_titans.contains(&h.id);
                                if ui
                                    .button(if t {
                                        "Not a titan system"
                                    } else {
                                        "Set as titan system"
                                    })
                                    .clicked()
                                {
                                    titan_now = Some((h.id, !t));
                                    ui.close();
                                }
                            }
                        });
                    }
                };
                frame.show(ui, |ui| {
                    // The parts of a hop share a line whenever they fit and wrap when they do not,
                    // so a short row is one line. The menu takes its place on the right first, so it
                    // keeps the right edge instead of riding along in the wrap.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        menu(ui);
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(&h.name)
                                        .color(security_color(h.security))
                                        .strong(),
                                );
                                let tail = if i == 0 {
                                    "start".to_owned()
                                } else {
                                    match h.kind {
                                        2 => format!("jump {:.1} ly", h.ly.unwrap_or_default()),
                                        1 => "ansiblex".to_owned(),
                                        _ => "gate".to_owned(),
                                    }
                                };
                                ui.label(egui::RichText::new(tail).weak().size(11.0));
                                if let Some(((fuel, fat), react)) = cost {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} iso · fatigue {} · ready in {}",
                                            fuel.round() as i64,
                                            fmt_min(fat),
                                            fmt_min(react)
                                        ))
                                        .weak()
                                        .size(11.0),
                                    );
                                }
                                if let Some(w) = &warn {
                                    if warn_button(ui, w) {
                                        warn_intel = Some(h.id);
                                    }
                                }
                            });
                        });
                    });
                });
            }
        });
        let show_intel = show_intel.or(warn_intel);
        if let Some(id) = avoid_now {
            self.map_avoid_once.insert(id);
            self.map_replan_route();
        }
        if let Some(id) = unavoid_now {
            self.map_avoid_once.remove(&id);
            self.map_replan_route();
        }
        if let Some(id) = waypoint_now {
            self.map_route_add_waypoint(id);
        }
        if let Some(i) = drop_anchor_row {
            if self.map_route_anchors.len() > 2 || i == self.map_route_anchors.len() - 1 {
                self.map_route_anchors.remove(i);
                self.map_replan_route();
            }
        }
        if let Some((id, on)) = titan_now {
            self.map_titans.retain(|&t| t != id);
            if on {
                self.map_titans.push(id);
            }
            self.map_replan_route();
        }
        if let Some(id) = show_intel {
            self.map_intel_for = Some(id);
        }
        if let Some(i) = alts_for {
            // The systems a capital could stop in between the two hops either side of this one.
            // Picking one inserts it as a waypoint, which is what makes it a steer rather than a
            // different route.
            let (a, b) = (hops.get(i - 1).map(|h| h.id), hops.get(i + 1).map(|h| h.id));
            if let (Some(a), Some(b)) = (a, b) {
                self.ensure_jump_systems();
                let coords = self.jump_systems.clone().unwrap_or_default();
                let max_ly = crate::jumproute::max_range_ly(
                    &crate::jumproute::SHIP_CLASSES[self.jump_ship],
                    self.jump_jdc,
                );
                let mut ids = crate::jumproute::alternatives(&coords, max_ly, a, b);
                ids.sort_unstable();
                ids.dedup();
                ids.truncate(40);
                self.map_alts = Some(ids);
            }
        }
    }

    /// Saving and loading a route, the same store the page writes to.
    ///
    /// The whole route, not just the endpoints: a route is the anchors and what you told the planner
    /// about them, and one that came back without its avoid list would be a different route with the
    /// same name.
    pub(crate) fn map_route_store_windows(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        if self.map_save_open {
            let mut open = true;
            let mut go = false;
            // Whether the route flies through a hole, not whether the setting allows one: a jump
            // route never does, and a gate route that found no hole is not on a clock either.
            let wh = self
                .map_route_opts
                .get(self.map_route_at)
                .is_some_and(|o| o.uses_wormhole);
            egui::Window::new(format!("{}  Save route", icon::COPY))
                .id(egui::Id::new("map_save_route"))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.set_min_width(280.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.map_save_name).hint_text("Name"),
                    );
                    if wh {
                        ui.label(
                            egui::RichText::new(
                                "Planned through scanned wormholes. Those chains move, so this is \
                                 deleted a day after saving rather than quietly becoming wrong.",
                            )
                            .color(crate::theme::standing::WARNING),
                        );
                    }
                    go = ui.button("Save").clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                });
            if go && !self.map_save_name.trim().is_empty() {
                let route = crate::settings::SavedMapRoute {
                    name: self.map_save_name.trim().to_owned(),
                    kind: self.map_route_kind.to_owned(),
                    anchors: self.map_route_anchors.clone(),
                    avoid: self.map_avoid_once.iter().copied().collect(),
                    titans: self.map_titans.clone(),
                    titan_at_start: self.map_titan_at_start,
                    titan_self_jump: self.map_titan_self_jump,
                    hull: self.jump_ship,
                    jdc: self.jump_jdc,
                    jfc: self.jump_jfc,
                    saved_at: 0,
                    via_wormholes: wh,
                };
                self.apply_overlay_message(crate::ipc::OverlayToMain::SaveRoute { route }, ctx);
                self.map_save_open = false;
            }
            if !open {
                self.map_save_open = false;
            }
        }
        if self.map_load_open {
            let mut open = true;
            let mut load: Option<crate::settings::SavedMapRoute> = None;
            let mut forget: Option<String> = None;
            let now = chrono::Utc::now().timestamp();
            let rows: Vec<crate::settings::SavedMapRoute> = self
                .settings
                .saved_map_routes
                .iter()
                .filter(|r| {
                    !r.via_wormholes
                        || now - r.saved_at < crate::settings::WORMHOLE_ROUTE_TTL_SECS
                })
                .cloned()
                .collect();
            egui::Window::new(format!("{}  Saved routes", icon::ARROW_SQUARE_OUT))
                .id(egui::Id::new("map_load_route"))
                .collapsible(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    if rows.is_empty() {
                        ui.label(egui::RichText::new("Nothing saved yet.").weak());
                        return;
                    }
                    let name_of = |id: i64| {
                        self.systems
                            .as_ref()
                            .and_then(|g| g.info_of(id).map(|i| i.name.clone()))
                            .unwrap_or_default()
                    };
                    for r in &rows {
                        ui.horizontal(|ui| {
                            if ui.button(&r.name).clicked() {
                                load = Some(r.clone());
                            }
                            let ends = format!(
                                "{} → {} · {}{}",
                                r.anchors.first().copied().map(name_of).unwrap_or_default(),
                                r.anchors.last().copied().map(name_of).unwrap_or_default(),
                                r.kind,
                                if r.via_wormholes { " · expires" } else { "" }
                            );
                            ui.label(egui::RichText::new(ends).weak().size(11.0));
                            if ui.small_button(icon::X).on_hover_text("Forget").clicked() {
                                forget = Some(r.name.clone());
                            }
                        });
                    }
                });
            if let Some(name) = forget {
                self.apply_overlay_message(crate::ipc::OverlayToMain::DeleteRoute { name }, ctx);
            }
            if let Some(r) = load {
                self.map_route_kind = match r.kind.as_str() {
                    "jump" => "jump",
                    "titan" => "titan",
                    _ => "gate",
                };
                self.map_route_anchors = r.anchors;
                self.map_avoid_once = r.avoid.into_iter().collect();
                self.map_titans = r.titans;
                self.map_titan_at_start = r.titan_at_start;
                self.map_titan_self_jump = r.titan_self_jump;
                self.jump_ship = r.hull.min(crate::jumproute::SHIP_CLASSES.len() - 1);
                self.jump_jdc = r.jdc.min(5);
                self.jump_jfc = r.jfc.min(5);
                self.map_leg_pick.clear();
                self.map_replan_route();
                self.map_load_open = false;
            }
            if !open {
                self.map_load_open = false;
            }
        }
    }

    /// The alternatives picker: systems in range of both neighbours, any of which becomes a waypoint.
    pub(crate) fn map_alts_window(&mut self, ctx: &egui::Context) {
        let Some(ids) = self.map_alts.clone() else { return };
        let mut open = true;
        let mut pick: Option<i64> = None;
        egui::Window::new("In range of both")
            .id(egui::Id::new("map_alts"))
            .collapsible(false)
            .default_width(240.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if ids.is_empty() {
                    ui.label(egui::RichText::new("Nothing else is in range of both.").weak());
                    return;
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for id in &ids {
                        if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(*id)) {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(&info.name)
                                            .color(security_color(info.security)),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                pick = Some(*id);
                            }
                        }
                    }
                });
            });
        if let Some(id) = pick {
            self.map_route_add_waypoint(id);
            self.map_alts = None;
        } else if !open {
            self.map_alts = None;
        }
    }


    pub(crate) fn travel_panel_content(&mut self, ui: &mut egui::Ui) {
        fn travel_field(
            ui: &mut egui::Ui,
            q: &mut String,
            sel: &mut usize,
            hint: &str,
            suggestions: &[SysHit],
        ) -> Option<i64> {
            let mut pick = None;
            let resp = ui.add(
                egui::TextEdit::singleline(q).hint_text(hint).desired_width(ui.available_width()),
            );
            if resp.changed() {
                *sel = 0;
            }
            // A singleline TextEdit surrenders focus the instant Enter is pressed, so by now
            // `has_focus` is already false. The key itself is still in the queue, so the accept has
            // to hang off `lost_focus` or Enter would never pick the highlighted suggestion.
            let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if !suggestions.is_empty() && (resp.has_focus() || entered) {
                let n = suggestions.len();
                if resp.has_focus() {
                    let (down, up) = ui.input(|i| {
                        (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp))
                    });
                    if down {
                        *sel = (*sel + 1).min(n - 1);
                    }
                    if up {
                        *sel = sel.saturating_sub(1);
                    }
                    let moving = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
                    let below = resp.rect.left_bottom() + egui::vec2(0.0, 2.0);
                    let width = resp.rect.width();
                    egui::Area::new(ui.id().with(("travel_sugg", hint)))
                        .order(egui::Order::Foreground)
                        .fixed_pos(below)
                        .constrain(true)
                        .show(ui.ctx(), |ui| {
                            ui.set_min_width(width);
                            ui.set_max_width(width);
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                for (i, (id, name, sec, c, r)) in suggestions.iter().enumerate() {
                                    let row = format!("{name}    {sec:.1}\n{c} \u{2022} {r}");
                                    let rr = ui.selectable_label(i == *sel, row);
                                    if rr.hovered() && moving {
                                        *sel = i;
                                    }
                                    if rr.clicked() {
                                        pick = Some(*id);
                                    }
                                }
                            });
                        });
                }
                if entered && pick.is_none() {
                    pick = suggestions.get((*sel).min(n - 1)).map(|x| x.0);
                }
            }
            if pick.is_some() {
                resp.surrender_focus();
            }
            pick
        }

        let name_of = |id: Option<i64>| -> Option<String> {
            id.and_then(|i| self.systems.as_ref().and_then(|g| g.info_of(i)).map(|s| s.name.clone()))
        };
        let start_name = name_of(self.travel_start);
        let end_name = name_of(self.travel_end);
        let key = (
            self.travel_start_q.clone(),
            self.travel_start,
            self.travel_end_q.clone(),
            self.travel_end,
        );
        if key != self.travel_sugg_key {
            let s0 = self.travel_suggestions(&self.travel_start_q);
            let s1 = self.travel_suggestions(&self.travel_end_q);
            self.travel_sugg = (s0, s1);
            self.travel_sugg_key = key;
        }
        let start_suggestions = self.travel_sugg.0.clone();
        let end_suggestions = self.travel_sugg.1.clone();
        if self.travel_wp_q != self.travel_wp_sugg_key {
            self.travel_wp_sugg = self.travel_suggestions(&self.travel_wp_q);
            self.travel_wp_sugg_key = self.travel_wp_q.clone();
        }
        let wp_suggestions = self.travel_wp_sugg.clone();
        // An empty From means "where I am". Skipped while a field is focused, so it cannot overwrite
        // a box the user has just cleared to type into.
        if self.travel_start.is_none()
            && self.travel_start_q.trim().is_empty()
            && ui.memory(|m| m.focused()).is_none()
        {
            if let Some(me) = self.player_system() {
                self.travel_set_start(me);
            }
        }
        let mut wp_pick: Option<i64> = None;
        let mut set_dest = false;
        let name_id = |id: i64| -> (i64, String) {
            (
                id,
                self.systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| id.to_string()),
            )
        };
        let wp_names: Vec<(i64, String)> = self.travel_waypoints.iter().map(|&id| name_id(id)).collect();
        let avoid_names: Vec<(i64, String)> = self.travel_avoid.iter().map(|&id| name_id(id)).collect();
        let mut remove_wp: Option<i64> = None;
        let mut remove_avoid: Option<i64> = None;
        let summary = self.travel_route.as_ref().map(|r| {
            let planned = r.len().saturating_sub(1);
            let holes = self
                .systems
                .as_ref()
                .map(|g| r.windows(2).filter(|w| g.is_hole_step(w[0], w[1])).count())
                .unwrap_or(0);
            let mut s = match self.travel_direct_route.as_ref().map(|d| d.len().saturating_sub(1)) {
                Some(direct) if planned > direct => {
                    format!("{planned} jumps \u{2022} direct {direct} (+{})", planned - direct)
                }
                _ => format!("{planned} jumps"),
            };
            if holes > 0 {
                s.push_str(&format!(" \u{2022} {holes} via wormhole"));
                if holes > 1 {
                    s.push('s');
                }
            }
            s
        });
        let mut clear = false;
        let mut start_pick: Option<i64> = None;
        let mut end_pick: Option<i64> = None;
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Travel route").strong().size(15.0));
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label("From");
            if let Some(id) = travel_field(
                ui,
                &mut self.travel_start_q,
                &mut self.travel_start_sel,
                start_name.as_deref().unwrap_or("system"),
                &start_suggestions,
            ) {
                start_pick = Some(id);
            }
            ui.add_space(2.0);
            ui.label("To");
            if let Some(id) = travel_field(
                ui,
                &mut self.travel_end_q,
                &mut self.travel_end_sel,
                end_name.as_deref().unwrap_or("system"),
                &end_suggestions,
            ) {
                end_pick = Some(id);
            }
            ui.label(egui::RichText::new("\u{2026}or right-click a system on the map.").weak());
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Waypoints").strong());
            for (id, name) in &wp_names {
                ui.horizontal(|ui| {
                    if ui.button(egui_phosphor::regular::X).on_hover_text("Remove").clicked() {
                        remove_wp = Some(*id);
                    }
                    ui.label(name);
                });
            }
            if let Some(id) = travel_field(
                ui,
                &mut self.travel_wp_q,
                &mut self.travel_wp_sel,
                "+ add waypoint",
                &wp_suggestions,
            ) {
                wp_pick = Some(id);
            }
            if !avoid_names.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Avoid").strong());
                for (id, name) in &avoid_names {
                    ui.horizontal(|ui| {
                        if ui.button(egui_phosphor::regular::X).on_hover_text("Remove").clicked() {
                            remove_avoid = Some(*id);
                        }
                        ui.label(name);
                    });
                }
            }
            ui.add_space(4.0);
            ui.checkbox(&mut self.travel_live, "Live mode").on_hover_text(
                "Track your position; continuously re-plan and re-route in-game on changes",
            );
            if ui
                .checkbox(&mut self.settings.travel_auto_dest, "Auto-set destination in EVE")
                .on_hover_text("When off, Live mode tracks + re-plans but never writes the route into the game")
                .changed()
            {
                self.needs_save = true;
            }
            if ui
                .button(format!("{}  Saved routes\u{2026}", egui_phosphor::regular::FOLDER))
                .on_hover_text("Save, organise and load named routes")
                .clicked()
            {
                self.routes_dialog_open = true;
            }
            ui.checkbox(&mut self.travel_regional_gates, "Region-crossing gates");
            ui.checkbox(&mut self.travel_jump_bridges, "Jump bridges");
            ui.checkbox(&mut self.travel_avoid_camps, "Avoid gate camps");
            ui.horizontal(|ui| {
                ui.label("Sec");
                ui.checkbox(&mut self.travel_sec[0], "Hi");
                ui.checkbox(&mut self.travel_sec[1], "Lo");
                ui.checkbox(&mut self.travel_sec[2], "Null");
            });
            let metric_before = self.travel_metric;
            ui.horizontal(|ui| {
                ui.label("Max");
                ui.add(
                    egui::DragValue::new(&mut self.travel_max_ship_kills)
                        .range(0..=20000)
                        .custom_formatter(|n, _| {
                            if n <= 0.0 { "any".to_owned() } else { format!("{n}") }
                        }),
                );
                egui::ComboBox::from_id_salt(ui.id().with("travel_metric"))
                    .selected_text(self.travel_metric.label())
                    .show_ui(ui, |ui| {
                        for m in [
                            ActivityMode::ShipKills,
                            ActivityMode::PodKills,
                            ActivityMode::NpcKills,
                            ActivityMode::Jumps,
                        ] {
                            ui.selectable_value(&mut self.travel_metric, m, m.label());
                        }
                    });
                ui.label("/h");
            });
            if self.travel_metric != metric_before {
                self.map_overlays.activity = self.travel_metric;
            }
            if ui
                .button(format!("Avoid sov held by\u{2026} ({})", self.travel_avoid_sov.len()))
                .clicked()
            {
                self.travel_sov_dialog_open = true;
            }
            ui.add_space(4.0);
            let has_route = self.travel_start.is_some()
                || self.travel_end.is_some()
                || !self.travel_waypoints.is_empty();
            ui.horizontal(|ui| {
                if self.travel_route.is_some()
                    && ui
                        .button("Set destination")
                        .on_hover_text("Write the planned route to EVE as individual waypoints")
                        .clicked()
                {
                    set_dest = true;
                }
                if has_route && ui.button("Clear route").clicked() {
                    clear = true;
                }
            });
            match &summary {
                Some(s) => {
                    ui.label(egui::RichText::new(s).color(egui::Color32::from_rgb(0x4F, 0xC3, 0xF7)).strong());
                }
                None => {
                    ui.label(
                        egui::RichText::new("Set a from / to. The route updates automatically.")
                            .weak(),
                    );
                }
            }
        });
        if let Some(id) = start_pick {
            self.travel_start = Some(id);
            self.travel_start_q =
                self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
            self.travel_start_sel = 0;
            self.plan_route();
        }
        if let Some(id) = end_pick {
            self.travel_end = Some(id);
            self.travel_end_q =
                self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
            self.travel_end_sel = 0;
            self.plan_route();
        }
        if let Some(id) = wp_pick {
            if !self.travel_waypoints.contains(&id) {
                self.travel_waypoints.push(id);
            }
            self.travel_avoid.retain(|&a| a != id);
            self.travel_wp_q.clear();
            self.travel_wp_sel = 0;
        }
        if set_dest {
            if let Some(route) = self.travel_route.clone() {
                let mut seen = std::collections::HashSet::new();
                let unique: Vec<i64> = route.into_iter().filter(|s| seen.insert(*s)).collect();
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                crate::esi::set_route(cid, self.active_character.clone(), unique);
            }
        }
        if let Some(id) = remove_wp {
            self.travel_waypoints.retain(|&w| w != id);
            self.plan_route();
        }
        if let Some(id) = remove_avoid {
            self.travel_avoid.retain(|&a| a != id);
            self.plan_route();
        }
        if clear {
            self.clear_travel();
        }
        if self.travel_live {
            let now_t = ui.input(|i| i.time);
            if let Some(me) = self.player_system() {
                if self.travel_start != Some(me) {
                    self.travel_start = Some(me);
                    self.travel_start_q = self
                        .systems
                        .as_ref()
                        .and_then(|g| g.info_of(me))
                        .map(|i| i.name.clone())
                        .unwrap_or_default();
                }
            }
            if now_t >= self.travel_live_next {
                self.travel_live_next = now_t + 4.0;
                self.plan_route();
                if self.travel_live_base.is_none() {
                    self.travel_live_base = self.travel_route.clone();
                }
            }
            if self.settings.travel_auto_dest {
                self.push_ingame_dest();
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(900));
        } else {
            self.travel_live_base = None;
            self.travel_ingame_dest = None;
        }
        let now = ui.input(|i| i.time);
        let h = self.travel_input_hash();
        if h != self.travel_planned_hash {
            if h != self.travel_pending_hash {
                self.travel_pending_hash = h;
                self.travel_dirty_at = Some(now);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(380));
            } else if let Some(t) = self.travel_dirty_at {
                if now - t >= 0.35 {
                    self.plan_route();
                    self.travel_pending_hash = self.travel_planned_hash;
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
                }
            }
        }
    }

    pub(crate) fn safety_watch(&mut self, ctx: &egui::Context) {
        if self.map_mode != MapMode::Safety {
            self.safety_prev = None;
            return;
        }
        let (Some(me), Some(geo)) = (self.player_system(), self.systems.clone()) else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if now - self.safety_last_scan < 1.0 {
            return;
        }
        self.safety_last_scan = now;
        let range = self.map_threat_jumps;
        let mut current: std::collections::HashSet<i64> = std::collections::HashSet::new();
        {
            let st = self.intel_state.lock().unwrap();
            for r in &st.reports {
                if r.clear {
                    continue;
                }
                if let Some(sys) = r.primary_system() {
                    if geo.jumps(me, sys.id, range).is_some() {
                        current.insert(sys.id);
                    }
                }
            }
        }
        {
            let status = self.system_status.lock().unwrap();
            for (sid, f) in status.iter() {
                if f.ship_kills > 0 && geo.jumps(me, *sid, range).is_some() {
                    current.insert(*sid);
                }
            }
        }
        match self.safety_prev.take() {
            None => {}
            Some(prev) => {
                if current.iter().any(|s| !prev.contains(s)) {
                    crate::sound::play_prio("danger", 2, 1.0);
                    self.flash_until = ctx.input(|i| i.time) + 0.8;
                    ctx.request_repaint();
                }
            }
        }
        self.safety_prev = Some(current);
    }

    pub(crate) fn screen_flash(&self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now >= self.flash_until {
            return;
        }
        let alpha = (((self.flash_until - now) / 0.8).clamp(0.0, 1.0) as f32) * 0.45;
        let painter = ctx
            .layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("safety_flash")));
        painter.rect_filled(
            ctx.content_rect(),
            0.0,
            egui::Color32::from_rgb(0xEF, 0x44, 0x44).gamma_multiply(alpha),
        );
        ctx.request_repaint();
    }

    pub(crate) fn threat_board(&mut self, ui: &mut egui::Ui, hunting: bool) {
        let red = egui::Color32::from_rgb(0xEF, 0x53, 0x50);
        let orange = egui::Color32::from_rgb(0xFF, 0xA7, 0x26);
        let yellow = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
        let green = egui::Color32::from_rgb(0x66, 0xBB, 0x6A);
        let prox = |j: u32| if j <= 1 { red } else if j <= 3 { orange } else { yellow };

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(if hunting { "Hunting board" } else { "Safety watch" })
                .strong()
                .size(15.0),
        );
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Range");
            ui.add(egui::DragValue::new(&mut self.map_threat_jumps).range(1..=15).suffix("j"));
        });
        ui.label(
            egui::RichText::new(if hunting {
                "Targets and activity nearby, nearest first."
            } else {
                "Alarms when a new threat enters range."
            })
            .weak(),
        );

        let me_sys = self.player_system();
        let range = self.map_threat_jumps;
        let mut reports: Vec<(u32, crate::intel::IntelReport)> = Vec::new();
        let mut kills: Vec<(String, u32, u32, u32)> = Vec::new();
        if let (Some(me), Some(geo)) = (me_sys, self.systems.clone()) {
            {
                let st = self.intel_state.lock().unwrap();
                for r in &st.reports {
                    if r.clear {
                        continue;
                    }
                    if let Some(sys) = r.primary_system() {
                        if let Some(j) = geo.jumps(me, sys.id, range) {
                            reports.push((j, r.clone()));
                        }
                    }
                }
            }
            reports.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.received.cmp(&a.1.received)));
            let mut seen = std::collections::HashSet::new();
            reports.retain(|(_, r)| r.primary_system().map(|s| seen.insert(s.id)).unwrap_or(false));
            {
                let status = self.system_status.lock().unwrap();
                for (sid, f) in status.iter() {
                    if f.ship_kills == 0 && f.pod_kills == 0 {
                        continue;
                    }
                    if let Some(j) = geo.jumps(me, *sid, range) {
                        let name = geo.info_of(*sid).map(|i| i.name.clone()).unwrap_or_default();
                        kills.push((name, j, f.ship_kills, f.pod_kills));
                    }
                }
            }
            kills.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
        }

        ui.separator();
        if me_sys.is_none() {
            ui.label(egui::RichText::new("No active-character location.").weak());
            return;
        }
        let danger = !reports.is_empty();
        ui.label(
            egui::RichText::new(format!("Intel within {range}j: {}", reports.len()))
                .strong()
                .size(14.0)
                .color(if danger { red } else { green }),
        );
        let reports_only: Vec<crate::intel::IntelReport> =
            reports.into_iter().map(|(_, r)| r).collect();
        let action = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .id_salt("threat_cards")
            .show(ui, |ui| {
                let action = self.render_intel_cards(ui, &reports_only);
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Kill hotspots (last hour)").strong().size(14.0));
                if kills.is_empty() {
                    ui.label(egui::RichText::new("none in range").weak());
                }
                for (name, j, sk, pk) in kills.iter().take(15) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(name).strong().color(prox(*j)));
                        ui.label(egui::RichText::new(format!("{j}j")).weak());
                        if *sk > 0 {
                            ui.label(egui::RichText::new(format!("{sk} ship")).color(red));
                        }
                        if *pk > 0 {
                            ui.label(egui::RichText::new(format!("{pk} pod")).color(orange));
                        }
                    });
                }
                action
            })
            .inner;
        if let Some(c) = action {
            self.act_on_intel_click(c, ui.ctx());
        }
    }

    pub(crate) fn travel_sov_dialog(&mut self, ctx: &egui::Context) {
        if !self.travel_sov_dialog_open {
            return;
        }
        let coalitions: Vec<(String, Vec<String>)> = self
            .settings
            .coalitions
            .iter()
            .map(|c| (c.name.clone(), c.alliances.clone()))
            .collect();
        let in_coalition: std::collections::HashSet<String> =
            coalitions.iter().flat_map(|(_, m)| m.iter().cloned()).collect();
        let mut others: Vec<String> = self
            .settings
            .alliances
            .iter()
            .map(|a| a.name.clone())
            .filter(|n| !in_coalition.contains(n))
            .collect();
        others.sort();
        let npc: Vec<String> = {
            let status = self.system_status.lock().unwrap();
            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for f in status.values() {
                if f.sov_alliance.is_none() {
                    if let Some(h) = &f.sov {
                        set.insert(h.clone());
                    }
                }
            }
            set.into_iter().collect()
        };
        let mut clear = false;
        let keep = Self::dialog_viewport(
            ctx,
            "travel_sov_dialog",
            "EVE Spai \u{2014} Avoid sov",
            [420.0, 600.0],
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Tick coalitions or alliances whose sovereign space the route should \
                         avoid. Manage the groups in Settings \u{2192} Coalitions.",
                    )
                    .weak(),
                );
                if ui.button("Clear all").clicked() {
                    clear = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(egui::RichText::new("Player alliances").strong().size(14.0));
                    for (cname, members) in &coalitions {
                        egui::CollapsingHeader::new(egui::RichText::new(cname).strong())
                            .id_salt(cname)
                            .show(ui, |ui| {
                                let all = !members.is_empty()
                                    && members.iter().all(|m| self.travel_avoid_sov.contains(m));
                                let mut all_mut = all;
                                if ui.checkbox(&mut all_mut, "Avoid entire coalition").changed() {
                                    for m in members {
                                        if all_mut {
                                            self.travel_avoid_sov.insert(m.clone());
                                        } else {
                                            self.travel_avoid_sov.remove(m);
                                        }
                                    }
                                }
                                ui.separator();
                                for m in members {
                                    let mut on = self.travel_avoid_sov.contains(m);
                                    if ui.checkbox(&mut on, m).changed() {
                                        if on {
                                            self.travel_avoid_sov.insert(m.clone());
                                        } else {
                                            self.travel_avoid_sov.remove(m);
                                        }
                                    }
                                }
                            });
                    }
                    if !others.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Independent").weak());
                        for a in &others {
                            let mut on = self.travel_avoid_sov.contains(a);
                            if ui.checkbox(&mut on, a).changed() {
                                if on {
                                    self.travel_avoid_sov.insert(a.clone());
                                } else {
                                    self.travel_avoid_sov.remove(a);
                                }
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("NPC sov").strong().size(14.0));
                    if npc.is_empty() {
                        ui.label(egui::RichText::new("none in the current sov data").weak());
                    }
                    for n in &npc {
                        let mut on = self.travel_avoid_sov.contains(n);
                        if ui.checkbox(&mut on, n).changed() {
                            if on {
                                self.travel_avoid_sov.insert(n.clone());
                            } else {
                                self.travel_avoid_sov.remove(n);
                            }
                        }
                    }
                });
            },
        );
        if clear {
            self.travel_avoid_sov.clear();
        }
        if !keep {
            self.travel_sov_dialog_open = false;
        }
    }

    pub(crate) fn map_area(&mut self, ui: &mut egui::Ui) {
        if !self.map_overlay_mode {
            if self.left_dock_open {
                egui::Panel::left("map_standard_dock")
                    .resizable(true)
                    .default_size(212.0)
                    .size_range(170.0..=300.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("\u{00AB}").on_hover_text("Minimize panel").clicked() {
                                self.left_dock_open = false;
                            }
                            ui.label(egui::RichText::new("Map").strong());
                        });
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            self.map_controls_content(ui);
                        });
                    });
            }
            let has_mode = self.map_mode != MapMode::Standard;
            let has_route = !self.map_route_anchors.is_empty();
            if self.right_dock_open && (has_mode || has_route || self.map_docked_system.is_some()) {
                use egui_phosphor::regular as icon;
                let mut pending: Option<(SystemInfoOut, i64)> = None;
                egui::Panel::right("map_mode_dock")
                    .resizable(true)
                    .default_size(260.0)
                    .size_range(190.0..=380.0)
                    .show_inside(ui, |ui| {
                        let has_system = self.map_docked_system.is_some();
                        // Fall back to whatever this dock actually has, in that order, rather than
                        // showing an empty tab because the thing it was on has gone.
                        let tabs = [
                            (RightDockTab::Route, has_route),
                            (RightDockTab::System, has_system),
                            (RightDockTab::Mode, has_mode),
                        ];
                        if !tabs.iter().any(|(t, ok)| *ok && *t == self.right_dock_tab) {
                            if let Some((t, _)) = tabs.iter().find(|(_, ok)| *ok) {
                                self.right_dock_tab = *t;
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.button("\u{00BB}").on_hover_text("Minimize panel").clicked() {
                                self.right_dock_open = false;
                            }
                            if has_mode {
                                let label = match self.map_mode {
                                    MapMode::Travel => "Travel",
                                    MapMode::Safety | MapMode::Hunting => "Threat",
                                    MapMode::Standard => "",
                                };
                                if ui
                                    .selectable_label(self.right_dock_tab == RightDockTab::Mode, label)
                                    .clicked()
                                {
                                    self.right_dock_tab = RightDockTab::Mode;
                                }
                            }
                            if has_route
                                && ui
                                    .selectable_label(
                                        self.right_dock_tab == RightDockTab::Route,
                                        "Route",
                                    )
                                    .clicked()
                            {
                                self.right_dock_tab = RightDockTab::Route;
                            }
                            if has_system {
                                let name = self
                                    .map_docked_system
                                    .and_then(|sid| {
                                        self.systems.as_ref().and_then(|g| g.info_of(sid).map(|i| i.name.clone()))
                                    })
                                    .unwrap_or_else(|| "System".to_string());
                                if ui
                                    .selectable_label(self.right_dock_tab == RightDockTab::System, name)
                                    .clicked()
                                {
                                    self.right_dock_tab = RightDockTab::System;
                                }
                                if ui.button(icon::ARROW_SQUARE_OUT).on_hover_text("Pop out to window").clicked() {
                                    if let Some(sid) = self.map_docked_system.take() {
                                        self.system_window = Some(sid);
                                        self.focus_window = Some(egui::ViewportId::from_hash_of("system_window"));
                                    }
                                }
                                if ui.button(icon::X).on_hover_text("Close").clicked() {
                                    self.map_docked_system = None;
                                }
                            }
                        });
                        ui.separator();
                        match self.right_dock_tab {
                            RightDockTab::Route => self.jump_plan_content(ui),
                            RightDockTab::Mode => match self.map_mode {
                                MapMode::Travel => self.travel_panel_content(ui),
                                MapMode::Safety => self.threat_board(ui, false),
                                MapMode::Hunting => self.threat_board(ui, true),
                                MapMode::Standard => {}
                            },
                            RightDockTab::System => {
                                if let Some(sid) = self.map_docked_system {
                                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        pending = Some((self.system_info_body(ui, sid, true), sid));
                                    });
                                }
                            }
                        }
                    });
                if let Some((out, sid)) = pending {
                    let ctx = ui.ctx().clone();
                    self.apply_system_info_out(out, sid, &ctx, true);
                }
            }
        }
        ui.push_id("map:main", |ui| self.draw_map(ui));
    }

    pub(crate) fn map_controls_content(&mut self, ui: &mut egui::Ui) {
        use crate::map::{MapLayout, MapView};
        use egui_phosphor::regular as icon;

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Mode");
            let mut mode = self.map_mode;
            egui::ComboBox::from_id_salt(ui.id().with("map_mode"))
                .selected_text(mode.label())
                .show_ui(ui, |ui| {
                    for m in [
                        MapMode::Standard,
                        MapMode::Travel,
                        MapMode::Hunting,
                        MapMode::Safety,
                    ] {
                        ui.selectable_value(&mut mode, m, m.label());
                    }
                });
            if mode != self.map_mode {
                self.set_map_mode(mode);
            }
        });
        ui.separator();

        ui.label(egui::RichText::new("View").strong());
        if ui.button(format!("{}  Universe map", icon::GLOBE_HEMISPHERE_WEST)).clicked() {
            self.map_go(MapView::Universe);
        }
        egui::ComboBox::from_id_salt(ui.id().with("map_layout"))
            .selected_text(match self.map_layout {
                MapLayout::Geographic => "3D (geographic)",
                MapLayout::Spaced => "2D (in-game layout)",
                MapLayout::Radial => "Radial (jumps)",
                MapLayout::Tree => "Tree (jumps)",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.map_layout, MapLayout::Geographic, "3D (geographic)");
                ui.selectable_value(&mut self.map_layout, MapLayout::Spaced, "2D (in-game layout)");
                ui.selectable_value(&mut self.map_layout, MapLayout::Radial, "Radial (jumps)");
                ui.selectable_value(&mut self.map_layout, MapLayout::Tree, "Tree (jumps)");
            });
        if self.map_layout.is_threat() {
            ui.horizontal(|ui| {
                ui.label("Max jumps");
                ui.add(egui::DragValue::new(&mut self.map_threat_jumps).range(1..=15).suffix("j"));
            });
            ui.checkbox(&mut self.threat_include_bridges, "Include jump bridges");
        }
        if ui
            .add(egui::Button::new(format!("{}  Follow character", icon::CROSSHAIR)).selected(self.map_follow))
            .clicked()
        {
            self.map_follow = !self.map_follow;
        }
        if ui.button(format!("{}  Reset view", icon::ARROW_COUNTER_CLOCKWISE)).clicked() {
            self.map_pan = egui::Vec2::ZERO;
            self.map_zoom = 1.0;
            self.map_follow = false;
        }
        if (self.route_destination.is_some() || self.ingame_route)
            && ui.button(format!("{}  Clear route", icon::X)).clicked()
        {
            self.clear_route();
        }

        if !self.map_in_popout {
            ui.separator();
            ui.label(egui::RichText::new("Window").strong());
            let active = self.active_character.clone();
            let others: Vec<String> = {
                let p = self.player.lock().unwrap();
                let mut v: Vec<String> =
                    p.locations.keys().filter(|n| !n.eq_ignore_ascii_case(&active)).cloned().collect();
                v.sort();
                v
            };
            if !others.is_empty() {
                ui.menu_button(format!("{}  Pop out character map", icon::USERS_THREE), |ui| {
                    for n in &others {
                        let open = self.map_char_popouts.contains(n);
                        if ui.selectable_label(open, n).clicked() {
                            if open {
                                self.map_char_popouts.retain(|x| x != n);
                                self.map_char_view.remove(n);
                            } else {
                                self.map_char_popouts.push(n.clone());
                            }
                            ui.close();
                        }
                    }
                });
            }
            if !self.map_popped {
                if ui.button(format!("{}  Pop out map window", icon::ARROW_SQUARE_OUT)).clicked() {
                    self.map_popped = true;
                }
            } else {
                if ui
                    .add(egui::Button::new(format!("{}  Keep on top", icon::PUSH_PIN)).selected(self.map_window_on_top))
                    .clicked()
                {
                    self.map_window_on_top = !self.map_window_on_top;
                }
                if ui.button(format!("{}  Overlay mode", icon::FRAME_CORNERS)).clicked() {
                    self.map_overlay_mode = true;
                }
            }
        }

        ui.separator();
        if ui.button(format!("{}  System notes and tags", icon::TAG)).clicked() {
            self.open_notes_manager(crate::notes::NoteKind::System);
        }

        if !self.map_layout.is_threat() {
            ui.separator();
            egui::CollapsingHeader::new(format!("{}  Layers", icon::STACK_SIMPLE))
                .default_open(true)
                .show(ui, |ui| self.map_layers_content(ui));
        }
    }

    pub(crate) fn map_search_overlay(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        use crate::map::MapView;
        use egui_phosphor::regular as icon;

        enum Hit {
            System { id: i64, name: String, sec: f64 },
            Constellation { name: String, region: i64 },
            Region { id: i64, name: String },
        }
        enum Action {
            Focus(i64),
            Region(i64),
        }
        let hit_action = |h: &Hit| match h {
            Hit::System { id, .. } => Action::Focus(*id),
            Hit::Constellation { region, .. } => Action::Region(*region),
            Hit::Region { id, .. } => Action::Region(*id),
        };

        let screen = ui.ctx().content_rect();
        const SEARCH_PANEL_W: f32 = 320.0;
        let query = self.map_search.trim().to_owned();
        let has_query = !query.is_empty();
        let (down, up, enter, esc) = if has_query {
            ui.input(|i| {
                use egui::Key;
                (
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Escape),
                )
            })
        } else {
            (false, false, false, false)
        };

        if !has_query {
            self.map_search_key.clear();
            self.map_search_sys.clear();
            self.map_search_const.clear();
            self.map_search_reg.clear();
        } else if query != self.map_search_key {
            let (sys, cons, reg) = if let Some(store) = &self.store {
                (
                    store.search_systems(&query, 6),
                    store
                        .search_constellations(&query, 4)
                        .into_iter()
                        .map(|(_c, name, region)| (name, region))
                        .collect::<Vec<_>>(),
                    store.search_regions(&query, 4),
                )
            } else {
                (Vec::new(), Vec::new(), Vec::new())
            };
            self.map_search_sys = sys;
            self.map_search_const = cons;
            self.map_search_reg = reg;
            let ql = query.to_lowercase();
            let mut names: std::collections::BTreeSet<String> = Default::default();
            for u in &self.settings.sov_upgrades {
                for p in split_upgrade_label(&u.upgrade) {
                    if p.to_lowercase().contains(&ql) {
                        names.insert(p.to_owned());
                    }
                }
            }
            self.map_search_upgrades = names.into_iter().take(5).collect();
            self.map_search_key = query.clone();
        }
        let mut hits: Vec<Hit> = Vec::new();
        for (id, name, sec) in &self.map_search_sys {
            hits.push(Hit::System { id: *id, name: name.clone(), sec: *sec });
        }
        for (name, region) in &self.map_search_const {
            hits.push(Hit::Constellation { name: name.clone(), region: *region });
        }
        for (id, name) in &self.map_search_reg {
            hits.push(Hit::Region { id: *id, name: name.clone() });
        }
        let mut sel = self.map_search_sel;
        if hits.is_empty() {
            sel = 0;
        } else {
            if down {
                sel = (sel + 1).min(hits.len() - 1);
            }
            if up {
                sel = sel.saturating_sub(1);
            }
            sel = sel.min(hits.len() - 1);
        }

        let mut action: Option<Action> = None;
        if esc {
            self.map_search.clear();
        }
        if enter && !hits.is_empty() {
            action = Some(hit_action(&hits[sel]));
        }
        let mut chosen_upgrade: Option<String> = None;
        let mut clear_upgrade = false;
        let mut clear_search = false;

        const INPUT_H: f32 = 40.0;
        if has_query {
            let roff = egui::vec2(
                rect.left() - screen.left() + 8.0,
                rect.bottom() - screen.bottom() - 10.0 - INPUT_H,
            );
            egui::Area::new(egui::Id::new("map_search_results"))
                .anchor(egui::Align2::LEFT_BOTTOM, roff)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(SEARCH_PANEL_W);
                        ui.set_max_width(SEARCH_PANEL_W);
                        if let Some(up) = self.map_highlight_upgrade.clone() {
                            if ui
                                .button(format!("{}  {up}  {}", icon::MAP_PIN_LINE, icon::X))
                                .on_hover_text("Clear upgrade highlight")
                                .clicked()
                            {
                                clear_upgrade = true;
                            }
                        }
                        for up in self.map_search_upgrades.clone() {
                            if ui
                                .selectable_label(
                                    self.map_highlight_upgrade.as_deref() == Some(up.as_str()),
                                    format!("{}  {up}", icon::MAP_PIN_LINE),
                                )
                                .clicked()
                            {
                                chosen_upgrade = Some(up);
                                clear_search = true;
                            }
                        }
                        if hits.is_empty() {
                            ui.label(egui::RichText::new("No match").weak());
                        } else {
                            for (i, h) in hits.iter().enumerate().rev() {
                                let label = match h {
                                    Hit::System { name, sec, .. } => egui::RichText::new(
                                        format!("{:.1}  {name}", (sec * 10.0).round() / 10.0),
                                    )
                                    .color(security_color(*sec)),
                                    Hit::Constellation { name, .. } => {
                                        egui::RichText::new(format!("{}  {name}", icon::POLYGON))
                                            .weak()
                                    }
                                    Hit::Region { name, .. } => {
                                        egui::RichText::new(format!("{}  {name}", icon::MAP_TRIFOLD))
                                            .weak()
                                    }
                                };
                                if ui.selectable_label(i == sel, label).clicked() {
                                    action = Some(hit_action(h));
                                }
                            }
                        }
                    });
                });
        }

        let ioff = egui::vec2(
            rect.left() - screen.left() + 8.0,
            rect.bottom() - screen.bottom() - 10.0,
        );
        if self.map_regions.is_empty() {
            if let Some(r) = self.store.as_ref().map(|s| s.regions()) {
                self.map_regions = r;
            }
        }
        let cur_view = self.map_view;
        let cur_region: String = match cur_view {
            MapView::Region(id) => self
                .map_regions
                .iter()
                .find(|(r, _)| *r == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "Region".to_owned()),
            MapView::Universe => "Region".to_owned(),
        };
        let region_list: Vec<(i64, String)> = self
            .map_regions
            .iter()
            .filter(|(_, n)| !is_hidden_region(n))
            .cloned()
            .collect();
        let can_back = !self.map_history.is_empty();
        let can_fwd = !self.map_forward.is_empty();
        let mut nav_back = false;
        let mut nav_fwd = false;
        let mut region_pick: Option<i64> = None;
        egui::Area::new(egui::Id::new("map_search"))
            .anchor(egui::Align2::LEFT_BOTTOM, ioff)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(SEARCH_PANEL_W);
                    ui.set_max_width(SEARCH_PANEL_W);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(can_back, |ui| {
                            if ui.button(icon::ARROW_LEFT).on_hover_text("Back").clicked() {
                                nav_back = true;
                            }
                        });
                        ui.add_enabled_ui(can_fwd, |ui| {
                            if ui.button(icon::ARROW_RIGHT).on_hover_text("Forward").clicked() {
                                nav_fwd = true;
                            }
                        });
                        egui::ComboBox::from_id_salt(ui.id().with("map_region_pick"))
                            .selected_text(cur_region.clone())
                            .show_ui(ui, |ui| {
                                for (rid, rname) in &region_list {
                                    let sel = matches!(cur_view, MapView::Region(r) if r == *rid);
                                    if ui.selectable_label(sel, rname).clicked() {
                                        region_pick = Some(*rid);
                                    }
                                }
                            });
                        ui.label(icon::MAGNIFYING_GLASS);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.map_search)
                                .id(egui::Id::new("map_search_input"))
                                .hint_text("Search system / constellation / region")
                                .desired_width(240.0),
                        );
                        if has_query && ui.button(icon::X).clicked() {
                            clear_search = true;
                        }
                    });
                });
            });
        if nav_back {
            self.map_back();
        }
        if nav_fwd {
            self.map_forward_nav();
        }
        if let Some(id) = region_pick {
            self.map_go(MapView::Region(id));
        }

        if clear_upgrade {
            self.map_highlight_upgrade = None;
        }
        if let Some(up) = chosen_upgrade {
            self.map_highlight_upgrade = Some(up);
        }
        self.map_search_sel = sel;
        match action {
            Some(Action::Focus(id)) => {
                self.map_search.clear();
                self.map_search_sel = 0;
                self.focus_map_on_select(id);
            }
            Some(Action::Region(id)) => {
                self.map_search.clear();
                self.map_search_sel = 0;
                self.map_go(MapView::Region(id));
            }
            None if clear_search => {
                self.map_search.clear();
                self.map_search_sel = 0;
            }
            None => {}
        }
    }

    /// Mean colour of an already-decoded logo. `None` while the image is still loading, so the dot
    /// keeps its security colour until the logo arrives and is then recoloured.
    pub(crate) fn logo_avg_color(&mut self, ctx: &egui::Context, url: &str) -> Option<egui::Color32> {
        if let Some(c) = self.logo_avg.get(url) {
            return Some(*c);
        }
        let hint = egui::SizeHint::Size { width: 32, height: 32, maintain_aspect_ratio: true };
        let egui::load::ImagePoll::Ready { image } = ctx.try_load_image(url, hint).ok()? else {
            return None;
        };
        let col = mean_logo_color(&image)?;
        self.logo_avg.insert(url.to_owned(), col);
        Some(col)
    }

    /// Per-system dot colour and icon for whoever holds the system: the alliance logo and its mean
    /// colour under player sov, the faction logo (at any security) with the dot left alone for NPCs.
    /// One fixed logo size, so zooming does not churn the URL cache; the icon is scaled when drawn.
    pub(crate) fn sov_art(&mut self, ctx: &egui::Context) -> std::collections::HashMap<i64, SovArt> {
        const LOGO_PX: f32 = 64.0;
        if self.map_overlays.sov == SovMode::Off {
            return std::collections::HashMap::new();
        }
        let holders: Vec<(i64, Option<i64>, Option<i64>, Option<String>)> = {
            let status = self.system_status.lock().unwrap();
            self.map_draw
                .iter()
                .filter_map(|s| {
                    let f = status.get(&s.id)?;
                    (f.sov_alliance.is_some() || f.sov_faction.is_some()).then(|| {
                        (s.id, f.sov_alliance, f.sov_faction, f.sov.clone())
                    })
                })
                .collect()
        };
        let coalition = self.map_overlays.sov == SovMode::Coalition;
        let mut out = std::collections::HashMap::new();
        for (id, alliance, faction, holder) in holders {
            let art = match (alliance, faction) {
                (Some(aid), _) => {
                    let url = eve_alliance_logo_url(aid, LOGO_PX);
                    let dot = match holder {
                        // By coalition, the coalition's own colour says more than the logo's mean.
                        Some(name) if coalition => Some(self.coalition_color_of(&name)),
                        // A colour the user picked for this alliance outranks the logo's mean.
                        Some(name) => self
                            .alliance_color_of(&name)
                            .or_else(|| self.logo_avg_color(ctx, &url)),
                        None => self.logo_avg_color(ctx, &url),
                    };
                    SovArt { icon: url, dot }
                }
                (None, Some(fid)) => match crate::factions::corporation_id(fid) {
                    Some(cid) => SovArt { icon: eve_corp_logo_url(cid, LOGO_PX), dot: None },
                    None => continue,
                },
                _ => continue,
            };
            out.insert(id, art);
        }
        out
    }

    pub(crate) fn coalition_color_of(&self, alliance: &str) -> egui::Color32 {
        self.settings
            .coalitions
            .iter()
            .find(|c| c.alliances.iter().any(|a| a.eq_ignore_ascii_case(alliance)))
            .map(Self::coalition_paint)
            .unwrap_or(egui::Color32::from_rgb(0x60, 0x60, 0x60))
    }

    pub(crate) fn focus_map_on_select(&mut self, id: i64) {
        if matches!(self.map_view, crate::map::MapView::Region(_)) {
            if let Some(r) = self.store.as_ref().and_then(|s| s.region_of_system(id)) {
                self.map_go(crate::map::MapView::Region(r));
            }
        }
        self.map_zoom = 18.0;
        self.map_focus = Some(id);
        self.map_selected = Some(id);
    }

    #[allow(deprecated)]
    pub(crate) fn show_map_viewport(&mut self, ctx: &egui::Context) {
        let overlay = self.map_overlay_mode;
        if overlay && self.settings.map_overlay_smart {
            let due = self.eve_focus_checked.map(|t| t.elapsed().as_millis() > 800).unwrap_or(true);
            if due {
                self.eve_focused.store(eve_is_focused(), std::sync::atomic::Ordering::Relaxed);
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        let on_top = if overlay {
            !self.settings.map_overlay_smart
                || self.eve_focused.load(std::sync::atomic::Ordering::Relaxed)
        } else {
            self.map_window_on_top
        };
        let mut keep = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("map_window"),
            egui::ViewportBuilder::default().with_icon(app_icon())
                .with_title("EVE Spai - Map")
                .with_inner_size([960.0, 720.0])
                .with_decorations(!overlay)
                .with_transparent(overlay)
                .with_resizable(!(overlay && self.map_overlay_locked))
                .with_window_level(if on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            |ctx, _class| {
                let frame = if overlay {
                    let a = (self.settings.map_overlay_opacity.clamp(0.2, 1.0) * 255.0) as u8;
                    egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(0x0A, 0x0C, 0x10, a))
                } else {
                    egui::Frame::central_panel(&ctx.style())
                };
                let locked = self.map_overlay_locked;
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    self.map_area(ui);
                    if overlay && !locked {
                        resize_grip(ui);
                    }
                });
                let want = (!overlay, !(overlay && self.map_overlay_locked));
                if self.map_vp_props != Some(want) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(want.0));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(want.1));
                    self.map_vp_props = Some(want);
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            },
        );
        if !keep {
            self.map_popped = false;
            self.map_overlay_mode = false;
            self.map_vp_props = None;
        }
    }

    #[allow(deprecated)]
    pub(crate) fn char_popout_windows(&mut self, ctx: &egui::Context) {
        if self.map_char_popouts.is_empty() {
            return;
        }
        let names = self.map_char_popouts.clone();
        let locs = self.player.lock().unwrap().locations.clone();
        let mut closed: Vec<String> = Vec::new();
        let (sv_view, sv_pan, sv_zoom, sv_focus, sv_follow, sv_rect) = (
            self.map_view,
            self.map_pan,
            self.map_zoom,
            self.map_focus,
            self.map_follow,
            self.map_last_rect,
        );
        self.map_in_popout = true;
        for name in &names {
            let Some(&(sys, _)) = locs.get(name) else { continue };
            let region = self.store.as_ref().and_then(|s| s.region_of_system(sys));
            let (cv, cpan, czoom, centered, crect) =
                *self.map_char_view.entry(name.clone()).or_insert_with(|| {
                    let v = region
                        .map(crate::map::MapView::Region)
                        .unwrap_or(crate::map::MapView::Universe);
                    (v, egui::Vec2::ZERO, 6.0, false, None)
                });
            self.map_view = cv;
            self.map_pan = cpan;
            self.map_zoom = czoom;
            self.map_focus = if centered { None } else { Some(sys) };
            self.map_follow = false;
            self.map_last_rect = crect;
            let mut keep = true;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(format!("charmap_{name}")),
                egui::ViewportBuilder::default().with_icon(app_icon())
                    .with_title(format!("EVE Spai - {name}"))
                    .with_inner_size([640.0, 520.0])
                    .with_min_inner_size([360.0, 280.0]),
                |ctx, _| {
                    egui::CentralPanel::default().show(ctx, |ui| { ui.push_id(name.as_str(), |ui| self.draw_map(ui)); });
                    ontop_pin(ctx, &format!("charmap_{name}"));
                    if ctx.input(|i| i.viewport().close_requested()) {
                        keep = false;
                    }
                },
            );
            self.map_char_view.insert(
                name.clone(),
                (self.map_view, self.map_pan, self.map_zoom, true, self.map_last_rect),
            );
            if !keep {
                closed.push(name.clone());
            }
        }
        self.map_view = sv_view;
        self.map_pan = sv_pan;
        self.map_zoom = sv_zoom;
        self.map_focus = sv_focus;
        self.map_follow = sv_follow;
        self.map_last_rect = sv_rect;
        self.map_in_popout = false;
        for n in closed {
            self.map_char_popouts.retain(|x| x != &n);
            self.map_char_view.remove(&n);
        }
    }
}
