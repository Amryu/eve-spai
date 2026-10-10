//! The map's side panels and windows: the route panel, travel and threat panels, controls, search and pop-outs.

use super::*;

impl SpaiApp {
    /// The route panel: the same thing the browser's route window shows, in the sidebar.
    ///
    /// The route is the subject. Everything about the ship is one collapsed header away, and only
    /// for the kind of route that has a ship.
    /// The route's own Ansiblex limit, over the setting's. Returns whether it changed.
    pub(crate) fn route_zone_combo(&mut self, ui: &mut egui::Ui) -> bool {
        if self.settings.jump_bridges.is_empty() {
            return false;
        }
        let mut changed = false;
        let setting = self.settings.ansiblex_max_zone;
        let current = self.map_route_zone.unwrap_or(setting);
        // Zone 0 is no Ansiblex at all: gates only.
        let zone_text = |z: u8| if z == 0 { "None".to_owned() } else { format!("Up to {}", crate::ansiblex::zone_label(z)) };
        ui.horizontal(|ui| {
            ui.label(tr!("Ansiblexes"));
            egui::ComboBox::from_id_salt(ui.id().with("route_zone"))
                .selected_text(zone_text(current))
                .show_ui(ui, |ui| {
                    for z in 0..=crate::ansiblex::MAX_ZONE {
                        let mut label = zone_text(z);
                        if z == setting {
                            label.push_str(" (setting)");
                        }
                        if ui.menu_value(&mut self.map_route_zone, Some(z), label).clicked() {
                            if z == setting {
                                self.map_route_zone = None;
                            }
                            changed = true;
                        }
                    }
                })
                .response
                .on_hover_text(tr!("For this route only. The jump bridge settings keep their own limit."));
        });
        changed
    }

    /// What a gate route keeps out of, and whether it keeps itself current. Returns whether any of it
    /// changed.
    pub(crate) fn route_rules_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use egui_phosphor::regular as icon;
        let mut changed = false;
        let st = &self.settings;
        let mut n = st.route_avoid_camps as usize + (st.route_sec != [true; 3]) as usize + (st.route_max_kills > 0) as usize;
        n += (!st.route_avoid_sov.is_empty()) as usize + (!st.route_region_gates) as usize;
        let head = if n > 0 { trf!("{icon}  Avoid ({n})", icon = icon::PROHIBIT, n = n) } else { trf!("{icon}  Avoid", icon = icon::PROHIBIT) };
        egui::CollapsingHeader::new(head).id_salt("route_rules").show(ui, |ui| {
            let st = &mut self.settings;
            changed |= ui.checkbox(&mut st.route_avoid_camps, tr!("Gate camps")).on_hover_text(tr!("Systems with a likely or possible camp")).changed();
            ui.horizontal(|ui| {
                ui.label(tr!("Allow"));
                changed |= ui.checkbox(&mut st.route_sec[0], tr!("High")).changed();
                changed |= ui.checkbox(&mut st.route_sec[1], tr!("Low")).changed();
                changed |= ui.checkbox(&mut st.route_sec[2], tr!("Null")).changed();
            });
            ui.horizontal(|ui| {
                ui.label(tr!("Kills last hour"));
                changed |= ui
                    .add(egui::DragValue::new(&mut st.route_max_kills).range(0..=500).custom_formatter(|n, _| if n == 0.0 { "any".into() } else { format!("at most {n}") }))
                    .on_hover_text(tr!("Ship kills in the system in the last hour"))
                    .changed();
            });
            changed |= ui.checkbox(&mut st.route_region_gates, tr!("Cross regions by gate")).changed();
            ui.horizontal(|ui| {
                ui.set_max_width(ui.available_width());
                ui.label(tr!("Sov held by"));
                let text = if st.route_avoid_sov.is_empty() { "nobody".to_owned() } else { st.route_avoid_sov.join(", ") };
                let menu = egui::containers::menu::MenuButton::from_button(egui::Button::new((text, egui::Atom::grow(), icon::CARET_DOWN)).truncate().min_size(egui::vec2(ui.available_width() - 2.0, 0.0)))
                    .config(egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside));
                menu.ui(ui, |ui| {
                    ui.set_width(240.0);
                    let mut remove = None;
                    for (i, a) in st.route_avoid_sov.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(icon::X).clicked() {
                                    remove = Some(i);
                                }
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(a).truncate()).on_hover_text(a);
                                });
                            });
                        });
                    }
                    if let Some(i) = remove {
                        st.route_avoid_sov.remove(i);
                        changed = true;
                    }
                    let id = egui::Id::new("route_sov_input");
                    let mut q = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
                    let resp = ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr!("Alliance")).desired_width(f32::INFINITY));
                    let ql = q.trim().to_lowercase();
                    let mut holders: Vec<String> = self
                        .system_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .values()
                        .filter_map(|f| f.sov.clone())
                        .filter(|h| !ql.is_empty() && h.to_lowercase().contains(&ql))
                        .collect();
                    holders.sort();
                    holders.dedup();
                    holders.retain(|h| !st.route_avoid_sov.contains(h));
                    let mut add = None;
                    for h in holders.iter().take(6) {
                        if ui.add(egui::Button::new(format!("{}  {h}", icon::PLUS)).frame(false).truncate()).clicked() {
                            add = Some(h.clone());
                        }
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ql.is_empty() {
                        add = Some(holders.first().cloned().unwrap_or_else(|| q.trim().to_owned()));
                    }
                    if let Some(h) = add {
                        st.route_avoid_sov.push(h);
                        q.clear();
                        changed = true;
                    }
                    ui.data_mut(|d| d.insert_temp(id, q));
                });
            });
            ui.separator();
            changed |= ui
                .checkbox(&mut st.route_live, tr!("Keep current"))
                .on_hover_text(tr!("Replan as intel and kills come in, with a sound when the route grows by more than four jumps"))
                .changed();
            if st.route_live {
                changed |= ui.checkbox(&mut st.travel_auto_dest, tr!("Update the route in game")).changed();
            }
        });
        changed
    }

    /// Replans a kept-current route every few seconds when what it avoids has changed.
    pub(crate) fn route_live_tick(&mut self, ctx: &egui::Context) {
        if !self.settings.route_live || self.map_route_anchors.len() < 2 || self.map_route_kind == "scan" {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now < self.route_live_next {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.route_live_next - now));
            return;
        }
        self.route_live_next = now + 5.0;
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            let mut rules: Vec<i64> = self.route_rule_avoid().into_iter().collect();
            rules.sort_unstable();
            rules.hash(&mut h);
            let mut danger: Vec<i64> = self.route_danger().into_keys().collect();
            danger.sort_unstable();
            danger.hash(&mut h);
            h.finish()
        };
        if sig == self.route_live_sig {
            return;
        }
        self.route_live_sig = sig;
        let before = self.map_route_opts.get(self.map_route_at).map(|o| o.jumps);
        let was = self.map_route_opts.get(self.map_route_at).map(|o| crate::web::route::ingame_waypoints(o, self.player_system()));
        self.map_recompute_route();
        let after = self.map_route_opts.first().map(|o| o.jumps);
        let now_wp = self.map_route_opts.first().map(|o| crate::web::route::ingame_waypoints(o, self.player_system()));
        if was.is_some() && now_wp != was {
            if let (Some(b), Some(a)) = (before, after) {
                if a > b + 4 {
                    crate::sound::play_prio(&self.settings.sound_reroute, 2, self.settings.sound_reroute_volume);
                }
            }
            if self.settings.travel_auto_dest && self.active_character != "No character" {
                if let Some(wp) = now_wp.filter(|w| !w.is_empty()) {
                    let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                    crate::esi::set_route(cid, self.active_character.clone(), wp);
                }
            }
        }
    }

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
        const KINDS: [(&str, &str, &str); 4] = [
            ("gate", tr_noop!("By gate"), tr_noop!("Gates, bridges and holes")),
            ("jump", tr_noop!("By jump drive"), tr_noop!("Capital jumps between cyno systems")),
            ("mixed", tr_noop!("Mixed"), tr_noop!("Each leg flown by gate or jumped, switched per leg")),
            ("scan", tr_noop!("Scan sweep"), tr_noop!("A sweep through the systems around a centre, for scouts")),
        ];
        let current = KINDS.iter().find(|k| k.0 == self.map_route_kind).map_or_else(|| tr!("By gate").to_owned(), |k| spai_ui::i18n::t_dyn(k.1));
        let mut pick: Option<&str> = None;
        egui::ComboBox::from_id_salt("route_kind").selected_text(current).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
            for (kind, label, hint) in KINDS {
                if ui.menu_label(self.map_route_kind == kind, spai_ui::i18n::t_dyn(label)).on_hover_text(spai_ui::i18n::t_dyn(hint)).clicked() {
                    pick = Some(kind);
                }
            }
        });
        if let Some(kind) = pick.filter(|k| *k != self.map_route_kind) {
            self.map_set_route_mode(kind);
            replan = true;
        }
        if self.map_route_kind == "scan" {
            if replan {
                self.scan_replan();
            }
            self.scan_panel(ui);
            return;
        }

        if self.map_route_anchors.len() < 2 {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    tr!("Drag from one system to another on the map, or right-click a system to start a \
                     route."),
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
        let mut flip: Option<(usize, &'static str)> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, &id) in anchors.iter().enumerate() {
                if i > 0 {
                    // How the leg into this anchor is flown, switched in place.
                    let jump = self.map_leg_kind(i - 1) == "jump";
                    let (glyph, tip, other) = if jump {
                        (icon::SPIRAL, tr!("Jumped; click to gate it"), "gate")
                    } else {
                        (icon::SIGN_IN, tr!("Gated; click to jump it"), "jump")
                    };
                    if spai_ui::widgets::icon_button(ui, glyph).on_hover_text(tip).clicked() {
                        flip = Some((i - 1, other));
                    }
                }
                let txt = egui::RichText::new(name(id, &self.systems)).strong();
                ui.label(if i == 0 || i == anchors.len() - 1 {
                    txt.color(ui.visuals().hyperlink_color)
                } else {
                    txt
                });
                if i > 0 && spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Remove")).clicked() {
                    drop_anchor = Some(i);
                }
            }
        });
        if let Some(i) = drop_anchor {
            self.map_route_remove_anchor(i);
            return;
        }
        if let Some((i, k)) = flip {
            self.map_set_leg_kind(i, k);
            return;
        }

        let gating = self.map_route_has("gate");
        if gating && self.route_zone_combo(ui) {
            replan = true;
        }
        if gating
            && ui
                .checkbox(&mut self.settings.route_via_wormholes, tr!("Route via scanned wormholes"))
                .changed()
        {
            self.needs_save = true;
            replan = true;
        }
        if gating && self.settings.route_via_wormholes && self.wh_route_options_ui(ui) {
            self.needs_save = true;
            replan = true;
        }
        if gating && self.route_rules_ui(ui) {
            self.needs_save = true;
            replan = true;
        }

        if self.map_route_has("jump") {
            egui::CollapsingHeader::new(trf!("{icon}  {v} · {v2} ly", icon = icon::SPIRAL, v = SHIP_CLASSES[self.jump_ship].name, v2 = format!("{:.1}", max_range_ly(&SHIP_CLASSES[self.jump_ship], self.jump_jdc))))
            .id_salt("route_ship")
            .show(ui, |ui| {
                egui::ComboBox::from_id_salt(ui.id().with("jump_ship"))
                    .selected_text(SHIP_CLASSES[self.jump_ship].name)
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, c) in SHIP_CLASSES.iter().enumerate() {
                            if ui.menu_value(&mut self.jump_ship, i, c.name).changed() {
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
                    ui.label(tr!("JDC")).on_hover_text(tr!("Jump Drive Calibration (range)"));
                    replan |= ui
                        .add(egui::DragValue::new(&mut self.jump_jdc).range(0..=5))
                        .changed();
                    ui.label(tr!("JFC")).on_hover_text(tr!("Jump Fuel Conservation (fuel)"));
                    replan |= ui
                        .add(egui::DragValue::new(&mut self.jump_jfc).range(0..=5))
                        .changed();
                });
                if self.active_character != "No character"
                    && ui
                        .button(tr!("Use my skills (ESI)"))
                        .on_hover_text(tr!("Needs the skills scope on this character"))
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
        let mut avoid = crate::web::route::Avoid { always: Default::default(), once: self.map_avoid_once.clone() };
        for jump in self.map_route_avoid_lists() {
            avoid.always.extend(self.route_avoid(jump).always);
        }
        let listed = self
            .systems
            .as_ref()
            .map(|g| crate::web::route::avoided(g, &avoid))
            .unwrap_or_default();
        if !listed.is_empty() {
            let mut stop: Option<(i64, bool)> = None;
            egui::CollapsingHeader::new(if listed.len() == 1 { trf!("{icon}  avoiding 1 system", icon = icon::EYE_SLASH) } else { trf!("{icon}  avoiding {listed} systems", icon = icon::EYE_SLASH, listed = listed.len()) })
            .id_salt("route_avoid")
            .show(ui, |ui| {
                for a in &listed {
                    ui.horizontal(|ui| {
                        ui.label(&a.name);
                        if a.always {
                            ui.label(egui::RichText::new(tr!("always")).weak().size(11.0));
                        }
                        if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Stop avoiding")).clicked() {
                            stop = Some((a.id, a.always));
                        }
                    });
                }
            });
            if let Some((id, always)) = stop {
                if always {
                    for jump in self.map_route_avoid_lists() {
                        let list = if jump { &mut self.settings.route_avoid_jump } else { &mut self.settings.route_avoid_gate };
                        list.retain(|&s| s != id);
                    }
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
                egui::RichText::new(tr!("No route with these settings."))
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
            let mut line = if saved == 1 { trf!("saves {n} gate", n = saved) } else { trf!("saves {n} gates", n = saved) };
            if thin {
                line.push_str(tr!(" — barely worth the cyno, a direct route may be simpler"));
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
                egui::RichText::new(trf!("{icon}  Titan jumps {v} → {v2}, {v3} ly", icon = egui_phosphor::regular::STAR_FOUR, v = tj.from_name, v2 = tj.to_name, v3 = format!("{:.1}", tj.ly)))
                .color(egui::Color32::from_rgb(0xFF, 0x7A, 0x3D)),
            );
        }
        if let Some(n) = &o.note {
            ui.label(egui::RichText::new(n).weak());
        }

        // A leg's equally long alternatives are picked at its forks, in the steps below.

        let ingame = self
            .map_route_opts
            .get(self.map_route_at)
            .map(|o| crate::web::route::ingame_waypoints(o, self.player_system()))
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            let has_char = self.active_character != "No character";
            if ui
                .add_enabled(has_char && !ingame.is_empty(), egui::Button::new(trf!("{icon}  Set in game", icon = icon::MAP_PIN_LINE)))
                .on_hover_text(if self.map_route_kind == "gate" {
                    tr!("Set this route in the game, one waypoint per system")
                } else {
                    tr!("Set waypoints in the game at both ends of each leg you fly yourself")
                })
                .on_disabled_hover_text(tr!("Log a character in to route in the game"))
                .clicked()
            {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                crate::esi::set_route(cid, self.active_character.clone(), ingame.clone());
                self.note_ingame_route();
            }
            if ui.button(trf!("{icon}  Save route", icon = icon::COPY)).clicked() {
                self.map_save_name.clear();
                self.map_save_open = true;
            }
            if ui.button(trf!("{icon}  Load…", icon = icon::ARROW_SQUARE_OUT)).clicked() {
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
                        hole: h.hole,
                        ly: h.ly,
                        fuel: h.fuel,
                        fatigue_min: h.fatigue_min,
                        reactivation_min: h.reactivation_min,
                        warn: h.warn,
                        anchor: h.anchor,
                        fork: h.fork.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut avoid_now: Option<i64> = None;
        let mut unavoid_now: Option<i64> = None;
        let mut waypoint_now: Option<(i64, &'static str)> = None;
        let mut drop_anchor_row: Option<usize> = None;
        let mut alts_for: Option<usize> = None;
        let mut show_intel: Option<i64> = None;
        let mut warn_intel: Option<i64> = None;
        let mut fork_now: Option<(i64, i64)> = None;
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
                                && ui.button(tr!("Show intel")).clicked()
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
                                    if last { tr!("Remove destination") } else { tr!("Remove waypoint") };
                                if ui.button(label).clicked() {
                                    drop_anchor_row = Some(i);
                                    ui.close();
                                }
                            }
                            if !h.anchor {
                                let on = self.map_avoid_once.contains(&h.id);
                                if ui
                                    .button(if on { tr!("Stop avoiding") } else { tr!("Avoid this system") })
                                    .clicked()
                                {
                                    if on {
                                        unavoid_now = Some(h.id);
                                    } else {
                                        avoid_now = Some(h.id);
                                    }
                                    ui.close();
                                }
                                if ui.button(tr!("Add waypoint here")).clicked() {
                                    waypoint_now = Some((h.id, if h.kind == 2 { "jump" } else { "gate" }));
                                    ui.close();
                                }
                            }
                            if h.kind == 2
                                && i > 0
                                && i + 1 < hops.len()
                                && ui.button(tr!("Other systems between…")).clicked()
                            {
                                alts_for = Some(i);
                                ui.close();
                            }
                            if ui.button(tr!("Show info")).clicked() {
                                self.map_selected = Some(h.id);
                                self.right_dock_open = true;
                                self.right_dock_tab = RightDockTab::System;
                                ui.close();
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
                                let (tail, colour) = spai_ui::star_map::hop_tail(h, i == 0);
                                let tail = egui::RichText::new(tail).size(11.0);
                                ui.label(match colour {
                                    Some(c) => tail.color(c),
                                    None => tail.weak(),
                                });
                                if let Some(((fuel, fat), react)) = cost {
                                    let text = egui::RichText::new(trf!("{v} iso · fatigue {v2} · ready in {v3}", v = fuel.round() as i64, v2 = fmt_min(fat), v3 = fmt_min(react)))
                                        .weak()
                                        .size(11.0);
                                    // A line of its own when the rest of this one is too short, rather
                                    // than starting here and wrapping under the hop's name.
                                    let w = egui::WidgetText::from(text.clone()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x;
                                    if w > ui.available_size_before_wrap().x {
                                        ui.end_row();
                                    }
                                    ui.label(text);
                                }
                                if let Some(w) = &warn {
                                    if warn_button(ui, w) {
                                        warn_intel = Some(h.id);
                                    }
                                }
                                // Every way on is the same length, so the choice is the user's and
                                // belongs in the list rather than in a menu three clicks away.
                                // A fork: the way this route goes, with the equally long others to pick from.
                                if !h.fork.is_empty() {
                                    let taken = hops.get(i + 1).map(|n| n.id);
                                    let current = h.fork.iter().find(|a| Some(a.id) == taken).map_or("?", |a| a.name.as_str());
                                    egui::ComboBox::from_id_salt(("route_fork", h.id))
                                        .selected_text(egui::RichText::new(format!("{}  {current}", icon::ARROWS_SPLIT)).size(11.0))
                                        .show_ui(ui, |ui| {
                                            for alt in &h.fork {
                                                let on = taken == Some(alt.id);
                                                if ui.menu_label(on, &alt.name).clicked() && !on {
                                                    fork_now = Some((h.id, alt.id));
                                                }
                                            }
                                        })
                                        .response
                                        .on_hover_text(tr!("Ways on from here, all the same length"));
                                }
                            });
                        });
                    });
                });
            }
        });
        let show_intel = show_intel.or(warn_intel);
        if let Some((at, next)) = fork_now {
            self.map_forks.insert(at, next);
            self.map_replan_route();
        }
        if let Some(id) = avoid_now {
            self.map_avoid_once.insert(id);
            self.map_replan_route();
        }
        if let Some(id) = unavoid_now {
            self.map_avoid_once.remove(&id);
            self.map_replan_route();
        }
        if let Some((id, leg)) = waypoint_now {
            self.map_route_add_waypoint(id, Some(leg));
        }
        if let Some(i) = drop_anchor_row {
            if self.map_route_anchors.len() > 2 || i == self.map_route_anchors.len() - 1 {
                self.map_route_remove_anchor(i);
            }
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
            egui::Window::new(trf!("{icon}  Save route", icon = icon::COPY))
                .id(egui::Id::new("map_save_route"))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.set_min_width(280.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.map_save_name).hint_text(tr!("Name")),
                    );
                    if wh {
                        ui.label(
                            egui::RichText::new(
                                tr!("Planned through scanned wormholes, so it is deleted a day after saving."),
                            )
                            .color(crate::theme::standing::WARNING),
                        );
                    }
                    go = ui.button(tr!("Save")).clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                });
            if go && !self.map_save_name.trim().is_empty() {
                let route = crate::settings::SavedMapRoute {
                    name: self.map_save_name.trim().to_owned(),
                    kind: self.map_route_kind.to_owned(),
                    anchors: self.map_route_anchors.clone(),
                    avoid: self.map_avoid_once.iter().copied().collect(),
                    titans: Vec::new(),
                    titan_at_start: true,
                    titan_self_jump: false,
                    legs: self.map_leg_kinds.iter().map(|k| k.to_string()).collect(),
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
            let now = crate::clock::utc().timestamp();
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
            egui::Window::new(trf!("{icon}  Saved routes", icon = icon::ARROW_SQUARE_OUT))
                .id(egui::Id::new("map_load_route"))
                .collapsible(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    if rows.is_empty() {
                        ui.label(egui::RichText::new(tr!("Nothing saved yet.")).weak());
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
                            if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Forget")).clicked() {
                                forget = Some(r.name.clone());
                            }
                        });
                    }
                });
            if let Some(name) = forget {
                self.apply_overlay_message(crate::ipc::OverlayToMain::DeleteRoute { name }, ctx);
            }
            if let Some(r) = load {
                // A saved titan route comes back as the gate route it was built on.
                self.map_route_kind = match r.kind.as_str() {
                    "jump" => "jump",
                    "mixed" => "mixed",
                    _ => "gate",
                };
                let legs = r.anchors.len().saturating_sub(1);
                self.map_leg_kinds = (0..legs)
                    .map(|i| match r.legs.get(i).map(String::as_str) {
                        Some("jump") => "jump",
                        Some(_) => "gate",
                        None => super::map_route::default_leg(self.map_route_kind),
                    })
                    .collect();
                self.map_route_anchors = r.anchors;
                self.map_avoid_once = r.avoid.into_iter().collect();
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
        egui::Window::new(tr!("In range of both"))
            .id(egui::Id::new("map_alts"))
            .collapsible(false)
            .default_width(240.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if ids.is_empty() {
                    ui.label(egui::RichText::new(tr!("Nothing else is in range of both.")).weak());
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
            self.map_route_add_waypoint(id, Some("jump"));
            self.map_alts = None;
        } else if !open {
            self.map_alts = None;
        }
    }


    pub(crate) fn safety_watch(&mut self, ctx: &egui::Context) {
        if !(self.map_layout.is_threat() && self.settings.map_threat_alarm) {
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
                    crate::sound::play_prio(&self.settings.sound_safety, 2, self.settings.sound_safety_volume);
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

    pub(crate) fn threat_board(&mut self, ui: &mut egui::Ui) {
        let red = egui::Color32::from_rgb(0xEF, 0x53, 0x50);
        let orange = egui::Color32::from_rgb(0xFF, 0xA7, 0x26);
        let yellow = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
        let green = egui::Color32::from_rgb(0x66, 0xBB, 0x6A);
        let prox = |j: u32| if j <= 1 { red } else if j <= 3 { orange } else { yellow };

        ui.add_space(4.0);
        ui.label(egui::RichText::new(trf!("Within {v} jumps, nearest first", v = self.map_threat_jumps)).weak());

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
            ui.label(egui::RichText::new(tr!("No active-character location.")).weak());
            return;
        }
        let danger = !reports.is_empty();
        ui.label(
            egui::RichText::new(trf!("Intel within {range}j: {reports}", range = range, reports = reports.len()))
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
                ui.label(egui::RichText::new(tr!("Kill hotspots (last hour)")).strong().size(14.0));
                if kills.is_empty() {
                    ui.label(egui::RichText::new(tr!("none in range")).weak());
                }
                for (name, j, sk, pk) in kills.iter().take(15) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(name).strong().color(prox(*j)));
                        ui.label(egui::RichText::new(format!("{j}j")).weak());
                        if *sk > 0 {
                            ui.label(egui::RichText::new(trf!("{sk} ship", sk = sk)).color(red));
                        }
                        if *pk > 0 {
                            ui.label(egui::RichText::new(trf!("{pk} pod", pk = pk)).color(orange));
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

    pub(crate) fn map_area(&mut self, ui: &mut egui::Ui) {
        if !self.map_overlay_mode {
            if self.left_dock_open {
                // As wide as its longest row needs in the chosen language, within the usual range.
                use egui_phosphor::regular as ic;
                let widest = [
                    trf!("{icon}  Sov upgrades", icon = ic::MAP_PIN_LINE),
                    trf!("{icon}  Cyno generators", icon = ic::CROSSHAIR_SIMPLE),
                    trf!("{icon}  Jump bridges", icon = ic::ARROWS_LEFT_RIGHT),
                    trf!("{icon}  Intel highlight", icon = ic::CLOCK_COUNTDOWN),
                    trf!("{icon}  Jove observatories", icon = ic::CELL_TOWER),
                    trf!("{icon}  Ansiblex zones", icon = ic::CIRCLES_THREE),
                ]
                .iter()
                .map(|t| egui::WidgetText::from(t.as_str()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x)
                    .fold(0.0, f32::max);
                egui::Panel::left("map_standard_dock")
                    .resizable(true)
                    .default_size((widest + 90.0).clamp(212.0, 300.0))
                    .size_range(170.0..=300.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("\u{00AB}").on_hover_text(tr!("Hide the panel")).clicked() {
                                self.left_dock_open = false;
                            }
                            ui.label(egui::RichText::new(tr!("Map")).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.map_window_menu(ui));
                        });
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            self.map_controls_content(ui);
                        });
                    });
            }
            let has_nearby = self.map_layout.is_threat();
            let has_route = !self.map_route_anchors.is_empty();
            if self.right_dock_open && (has_nearby || has_route || self.map_docked_system.is_some()) {
                use egui_phosphor::regular as icon;
                let mut pending: Option<(SystemInfoOut, i64)> = None;
                egui::Panel::right("map_mode_dock")
                    .resizable(true)
                    .default_size(280.0)
                    .size_range(220.0..=420.0)
                    .show_inside(ui, |ui| {
                        let has_system = self.map_docked_system.is_some();
                        // Fall back to whatever this dock actually has, in that order, rather than
                        // showing an empty tab because the thing it was on has gone.
                        let tabs = [
                            (RightDockTab::System, has_system),
                            (RightDockTab::Route, has_route),
                            (RightDockTab::Nearby, has_nearby),
                        ];
                        if !tabs.iter().any(|(t, ok)| *ok && *t == self.right_dock_tab) {
                            if let Some((t, _)) = tabs.iter().find(|(_, ok)| *ok) {
                                self.right_dock_tab = *t;
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.button("\u{00BB}").on_hover_text(tr!("Hide the panel")).clicked() {
                                self.right_dock_open = false;
                            }
                            for (tab, ok, label) in [
                                (RightDockTab::System, has_system, trf!("{icon}  System", icon = icon::INFO)),
                                (RightDockTab::Route, has_route, trf!("{icon}  Route", icon = icon::PATH)),
                                (RightDockTab::Nearby, has_nearby, trf!("{icon}  Nearby", icon = icon::WARNING)),
                            ] {
                                if ok && ui.menu_label(self.right_dock_tab == tab, label).clicked() {
                                    self.right_dock_tab = tab;
                                }
                            }
                        });
                        ui.separator();
                        match self.right_dock_tab {
                            RightDockTab::Route => self.jump_plan_content(ui),
                            RightDockTab::Nearby => self.threat_board(ui),
                            RightDockTab::System => {
                                if let Some(sid) = self.map_docked_system {
                                    pending = Some((self.system_info_body(ui, sid, true), sid));
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

    /// The map's window actions: popping it or a character's map out, keeping it on top, overlay.
    pub(crate) fn map_window_menu(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        if self.map_in_popout {
            return;
        }
        ui.menu_button(icon::DOTS_THREE, |ui| {
            if !self.map_popped && ui.button(trf!("{icon}  Pop out the map", icon = icon::ARROW_SQUARE_OUT)).clicked() {
                self.map_popped = true;
                ui.close();
            }
            if self.map_popped && !self.map_overlay_mode {
                ui.checkbox(&mut self.map_window_on_top, trf!("{icon}  Keep on top", icon = icon::PUSH_PIN));
            }
            let label = if self.map_overlay_mode { tr!("Close the overlay") } else { tr!("Overlay over EVE") };
            if ui
                .button(format!("{}  {label}", icon::FRAME_CORNERS))
                .on_hover_text(tr!("A borderless, see-through map to lay over the game"))
                .clicked()
            {
                self.map_overlay_mode = !self.map_overlay_mode;
                self.map_popped = self.map_overlay_mode;
                ui.close();
            }
            let active = self.active_character.clone();
            let others: Vec<String> = {
                let p = self.player.lock().unwrap();
                let mut v: Vec<String> = p.locations.keys().filter(|n| !n.eq_ignore_ascii_case(&active)).cloned().collect();
                v.sort();
                v
            };
            if !others.is_empty() {
                ui.menu_button(trf!("{icon}  A character's own map", icon = icon::USERS_THREE), |ui| {
                    for n in &others {
                        let mut open = self.map_char_popouts.contains(n);
                        if ui.checkbox(&mut open, n).changed() {
                            if open {
                                self.map_char_popouts.push(n.clone());
                            } else {
                                self.map_char_popouts.retain(|x| x != n);
                                self.map_char_view.remove(n);
                            }
                        }
                    }
                });
            }
        })
        .response
        .on_hover_text(tr!("Map windows"));
    }

    pub(crate) fn map_controls_content(&mut self, ui: &mut egui::Ui) {
        use crate::map::{MapLayout, MapView};
        use egui_phosphor::regular as icon;

        ui.add_space(4.0);
        let label = |l: MapLayout| match l {
            MapLayout::Geographic => tr!("3D, as in space"),
            MapLayout::Spaced => tr!("2D, as in game"),
            MapLayout::Radial => tr!("Rings of jumps around you"),
            MapLayout::Tree => tr!("Tree of jumps from you"),
        };
        egui::ComboBox::from_id_salt(ui.id().with("map_layout"))
            .selected_text(label(self.map_layout))
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for l in [MapLayout::Geographic, MapLayout::Spaced, MapLayout::Radial, MapLayout::Tree] {
                    ui.menu_value(&mut self.map_layout, l, label(l));
                }
            })
            .response
            .on_hover_text(tr!("The last two centre on you and list threats in range under Nearby"));
        if self.map_layout.is_threat() {
            ui.horizontal(|ui| {
                ui.label(tr!("Range"));
                ui.add(egui::DragValue::new(&mut self.map_threat_jumps).range(1..=15).suffix(" jumps"));
            });
            ui.checkbox(&mut self.threat_include_bridges, tr!("Count jump bridges"));
            if ui
                .checkbox(&mut self.settings.map_threat_alarm, tr!("Alarm on a new threat"))
                .on_hover_text(tr!("A sound and a red flash when a report or kill turns up in range"))
                .changed()
            {
                self.needs_save = true;
            }
        }
        ui.add_space(4.0);
        // One row of same-sized icon buttons for moving the view.
        ui.horizontal(|ui| {
            if ui.button(icon::GLOBE_HEMISPHERE_WEST).on_hover_text(tr!("Universe (U)")).clicked() {
                self.map_go(MapView::Universe);
            }
            let follow = egui::Button::new(icon::CROSSHAIR).selected(self.map_follow);
            if ui.add(follow).on_hover_text(tr!("Follow your character (F)")).clicked() {
                self.map_follow = !self.map_follow;
            }
            if ui.button(icon::ARROW_COUNTER_CLOCKWISE).on_hover_text(tr!("Reset the view (Home)")).clicked() {
                self.map_pan = egui::Vec2::ZERO;
                self.map_zoom = 1.0;
                self.map_follow = false;
            }
            if (self.route_destination.is_some() || self.ingame_route) && ui.button(icon::X).on_hover_text(tr!("Clear the route")).clicked() {
                self.clear_route();
            }
            if !self.ai_highlight.is_empty() && ui.button(icon::ERASER).on_hover_text(tr!("Clear the assistant's marks")).clicked() {
                self.ai_highlight.clear();
            }
        });

        if !self.map_layout.is_threat() {
            ui.add_space(2.0);
            ui.separator();
            self.map_layers_content(ui);
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
            // The list grows upwards from the field, the first hit nearest it: Up moves away.
            if up {
                sel = (sel + 1).min(hits.len() - 1);
            }
            if down {
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
                                .on_hover_text(tr!("Clear upgrade highlight"))
                                .clicked()
                            {
                                clear_upgrade = true;
                            }
                        }
                        for up in self.map_search_upgrades.clone() {
                            if ui
                                .menu_label(
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
                            ui.label(egui::RichText::new(tr!("No match")).weak());
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
                                if ui.menu_label(i == sel, label).clicked() {
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
                .unwrap_or_else(|| tr!("Region").to_owned()),
            MapView::Universe => tr!("Region").to_owned(),
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
                            if ui.button(icon::ARROW_LEFT).on_hover_text(tr!("Back")).clicked() {
                                nav_back = true;
                            }
                        });
                        ui.add_enabled_ui(can_fwd, |ui| {
                            if ui.button(icon::ARROW_RIGHT).on_hover_text(tr!("Forward")).clicked() {
                                nav_fwd = true;
                            }
                        });
                        egui::ComboBox::from_id_salt(ui.id().with("map_region_pick"))
                            .selected_text(cur_region.clone())
                            .show_ui(ui, |ui| {
                                for (rid, rname) in &region_list {
                                    let sel = matches!(cur_view, MapView::Region(r) if r == *rid);
                                    if ui.menu_label(sel, rname).clicked() {
                                        region_pick = Some(*rid);
                                    }
                                }
                            });
                        ui.label(icon::MAGNIFYING_GLASS).on_hover_text(tr!("Ctrl+F"));
                        let w = (ui.available_width() - if has_query { 30.0 } else { 0.0 }).max(60.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.map_search)
                                .id(egui::Id::new("map_search_input"))
                                .hint_text(tr!("Find (Ctrl+F)"))
                                .desired_width(w),
                        )
                        .on_hover_text(tr!("A system, constellation, region or sov upgrade"));
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
        // The overlay is a window of its own: a window's transparency is fixed when it is made, so
        // turning the popped-out map into an overlay in place would leave it opaque.
        let (id, title) = if overlay { ("map_overlay", tr!("EVE Spai - Map overlay")) } else { ("map_window", tr!("EVE Spai - Map")) };
        let opacity = self.settings.map_overlay_opacity.clamp(0.2, 1.0);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(id),
            egui::ViewportBuilder::default().with_icon(app_icon())
                .with_title(title)
                .with_inner_size(if overlay { [520.0, 520.0] } else { [960.0, 720.0] })
                .with_decorations(!overlay)
                .with_transparent(overlay && crate::window_alpha::PER_PIXEL)
                .with_taskbar(!overlay)
                .with_window_level(if on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            |ctx, _class| {
                let frame = if overlay && crate::window_alpha::PER_PIXEL {
                    egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(0x0A, 0x0C, 0x10, (opacity * 255.0) as u8))
                } else if overlay {
                    egui::Frame::new().fill(egui::Color32::from_rgb(0x0A, 0x0C, 0x10))
                } else {
                    egui::Frame::central_panel(&ctx.style())
                };
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    self.map_area(ui);
                    if overlay && ui.ui_contains_pointer() {
                        resize_grip(ui);
                    }
                });
                if overlay {
                    crate::window_alpha::set(title, opacity);
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            },
        );
        if !keep {
            self.map_popped = false;
            self.map_overlay_mode = false;
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

pub(crate) use spai_ui::wh_form::system_field;

