//! The star map: view navigation, drawing, the threat layout, layers and tooltips.

use super::*;

impl SpaiApp {
    pub(crate) fn maybe_rebuild_graph(&mut self, ctx: &egui::Context) {
        if self.systems.is_none() || self.settings.jump_bridges == self.bridges_applied {
            return;
        }
        let Some(store) = &self.store else { return };
        let mut systems = store.load_systems();
        let bridges: Vec<(i64, i64)> = self
            .settings
            .jump_bridges
            .iter()
            .filter_map(|b| Some((systems.lookup(&b.from)?.id, systems.lookup(&b.to)?.id)))
            .collect();
        systems.add_bridges(&bridges);
        self.systems = Some(std::sync::Arc::new(systems));
        self.bridges_applied = self.settings.jump_bridges.clone();
        self.map_loaded = None;
        self.map_draw_key = None;
        self.map_systems_cache.clear();
        self.map_draw_cache.clear();
        ctx.request_repaint();
    }

    pub(crate) fn map_view(&mut self, ui: &mut egui::Ui) {
        let status = self.sde_status.lock().unwrap().clone();
        match status {
            SdeStatus::Ready => {
                if self.map_popped {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Map is in its own window.").weak());
                    if ui.button("Dock map").clicked() {
                        self.map_popped = false;
                    }
                } else {
                    self.map_area(ui);
                }
            }
            SdeStatus::Downloading(msg) => {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(msg);
                });
            }
            SdeStatus::NotReady => {
                ui.add_space(10.0);
                ui.label("Static data has not been downloaded yet.");
                if ui.button("Download static data").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
            SdeStatus::Failed(err) => {
                ui.add_space(10.0);
                ui.colored_label(crate::theme::standing::WARNING, format!("SDE download failed: {err}"));
                if ui.button("Retry").clicked() {
                    self.start_sde(&ui.ctx().clone());
                }
            }
        }
    }

    pub(crate) fn set_map_view(&mut self, v: crate::map::MapView) {
        self.map_view = v;
        self.map_pan = egui::Vec2::ZERO;
        self.map_zoom = 1.0;
        self.map_follow = false;
    }
    pub(crate) fn map_go(&mut self, v: crate::map::MapView) {
        if self.map_view == v {
            return;
        }
        self.map_history.push(self.map_view);
        self.map_forward.clear();
        self.set_map_view(v);
    }
    pub(crate) fn map_back(&mut self) {
        if let Some(v) = self.map_history.pop() {
            self.map_forward.push(self.map_view);
            self.set_map_view(v);
        }
    }
    pub(crate) fn map_forward_nav(&mut self) {
        if let Some(v) = self.map_forward.pop() {
            self.map_history.push(self.map_view);
            self.set_map_view(v);
        }
    }

    #[allow(deprecated)]
    pub(crate) fn draw_map(&mut self, ui: &mut egui::Ui) {
        use crate::map::MapView;
        if self.map_regions.is_empty() {
            if let Some(store) = &self.store {
                self.map_regions = store.regions();
            }
        }
        let active_char = self.active_character.clone();
        let (player_sys, char_here) = {
            let p = self.player.lock().unwrap();
            let mut here: std::collections::HashMap<i64, (u32, bool)> =
                std::collections::HashMap::new();
            for (name, (sys, _)) in &p.locations {
                let e = here.entry(*sys).or_insert((0, false));
                e.0 += 1;
                if name.eq_ignore_ascii_case(&active_char) {
                    e.1 = true;
                }
            }
            let sys = p.locations.get(&active_char).map(|(s, _)| *s).or(p.system_id);
            (sys, here)
        };
        if !self.map_initialized {
            self.map_view = MapView::Universe;
            self.map_initialized = true;
        }

        if self.map_follow {
            if let (MapView::Region(r), Some(psys)) = (self.map_view, player_sys) {
                let pr = match self.map_follow_region {
                    Some((s, reg)) if s == psys => Some(reg),
                    _ => {
                        let reg = self.store.as_ref().and_then(|s| s.region_of_system(psys));
                        if let Some(reg) = reg {
                            self.map_follow_region = Some((psys, reg));
                        }
                        reg
                    }
                };
                if let Some(pr) = pr {
                    if pr != r {
                        self.map_view = MapView::Region(pr);
                    }
                }
            }
        }

        if self.map_layout.is_threat() {
            let rect = ui.available_rect_before_wrap();
            self.map_last_rect = Some(rect);
            if self.map_overlay_mode {
                ui.set_opacity(self.settings.map_overlay_opacity.clamp(0.2, 1.0));
            }
            self.draw_threat_view(ui, rect, player_sys);
            self.map_chrome(ui, rect);
            return;
        }

        if self.map_loaded != Some(self.map_view) {
            if let Some(old) = self.map_loaded {
                self.map_systems_cache.insert(old, std::mem::take(&mut self.map_systems));
            }
            if let Some(cached) = self.map_systems_cache.remove(&self.map_view) {
                self.map_systems = cached;
            } else {
                let raw = match self.map_view {
                    MapView::Universe => self.store.as_ref().map(|s| s.all_map_systems()),
                    MapView::Region(id) => self.store.as_ref().map(|s| s.region_systems(id)),
                }
                .unwrap_or_default();
                self.map_systems = if let Some(g) = &self.systems {
                    raw.into_iter()
                        .filter(|s| !g.neighbors(s.id).is_empty())
                        .filter(|s| {
                            g.info_of(s.id).map(|i| !is_hidden_region(&i.region)).unwrap_or(true)
                        })
                        .collect()
                } else {
                    raw
                };
            }
            self.map_loaded = Some(self.map_view);
        }

        let spaced = self.map_layout == crate::map::MapLayout::Spaced;
        let want = (self.map_view, spaced);
        if self.map_draw_key != Some(want) {
            if let Some(old) = self.map_draw_key {
                self.map_draw_cache.insert(old, std::mem::take(&mut self.map_draw));
            }
            if let Some(cached) = self.map_draw_cache.remove(&want) {
                self.map_draw = cached;
            } else {
                self.map_draw = if spaced {
                    self.map_systems
                        .iter()
                        .map(|s| crate::store::MapSystem { x: s.x2d, z: s.z2d, ..s.clone() })
                        .collect()
                } else {
                    self.map_systems.clone()
                };
            }
            self.map_draw_spaced = spaced;
            self.map_draw_key = Some(want);
        }
        let schematic = self.map_draw_spaced;

        let Some(bounds) = crate::map::Bounds::of(&self.map_draw) else {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("No systems to show.").weak());
            return;
        };

        if self.map_overlay_mode {
            ui.set_opacity(self.settings.map_overlay_opacity.clamp(0.2, 1.0));
        }
        let rect = ui.available_rect_before_wrap();
        if let Some(prev) = self.map_last_rect {
            let d = prev.size() - rect.size();
            if d.x.abs() > 0.5 || d.y.abs() > 0.5 {
                let old_s = bounds.base_scale(prev, 30.0);
                let new_s = bounds.base_scale(rect, 30.0);
                if old_s > 0.0 {
                    self.map_pan *= new_s / old_s;
                }
            }
        }
        self.map_last_rect = Some(rect);
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1)) {
            self.map_back();
        }
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra2)) {
            self.map_forward_nav();
        }
        // A drag that starts on a system draws a route, not a pan. Hit-tested against last frame's
        // positions, which is where the user pressed: this frame's have already moved under them.
        if resp.drag_started_by(egui::PointerButton::Primary) && !self.map_overlay_mode {
            self.map_link = ui
                .input(|i| i.pointer.press_origin())
                .and_then(|p| nearest_system(p, &self.map_pos_prev, 12.0));
            // The coordinates the light-year readout and the jump routes need. Loaded lazily for the
            // jump planner already; a route drag is the other thing that wants them.
            if self.map_link.is_some() {
                self.ensure_jump_systems();
            }
        }
        if resp.dragged() && !self.map_overlay_drag && self.map_link.is_none() {
            self.map_pan += resp.drag_delta();
            self.map_follow = false;
        }
        if !resp.dragged() {
            self.map_overlay_drag = false;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
                    let old = self.map_zoom;
                    let new = (old * (scroll * 0.003).exp()).clamp(0.7, 60.0);
                    let q = cursor - (rect.center() + self.map_pan);
                    self.map_pan += q * (1.0 - new / old);
                    self.map_zoom = new;
                }
            }
        }
        if self.map_follow {
            if let Some(ps) = player_sys.and_then(|id| self.map_draw.iter().find(|s| s.id == id)) {
                let base = crate::map::project(ps.x, ps.z, &bounds, rect, self.map_zoom, egui::Vec2::ZERO);
                self.map_pan = rect.center() - base;
            }
        }

        let mut pos: std::collections::HashMap<i64, egui::Pos2> = std::collections::HashMap::new();
        for s in &self.map_draw {
            pos.insert(s.id, crate::map::project(s.x, s.z, &bounds, rect, self.map_zoom, self.map_pan));
        }

        self.map_pos_prev = pos.clone();

        if let Some(fid) = self.map_focus.take() {
            if let Some(s) = self.map_draw.iter().find(|s| s.id == fid) {
                let base = crate::map::project(s.x, s.z, &bounds, rect, self.map_zoom, egui::Vec2::ZERO);
                self.map_pan = rect.center() - base;
            }
        }

        if self.map_overlay_mode && !self.map_overlay_locked && resp.drag_started() {
            let on_obj = ui
                .input(|i| i.pointer.press_origin())
                .and_then(|p| nearest_system(p, &pos, 10.0))
                .is_some();
            if !on_obj {
                self.map_overlay_drag = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        }

        if resp.clicked() {
            if let Some(click) = ui.input(|i| i.pointer.interact_pos()) {
                match nearest_system(click, &pos, 10.0) {
                    Some(id) => {
                        self.map_selected = (self.map_selected != Some(id)).then_some(id);
                        self.dock_system(id);
                    }
                    None => self.map_selected = None,
                }
            }
        }

        if resp.secondary_clicked() {
            self.ctx_menu_system =
                ui.input(|i| i.pointer.interact_pos()).and_then(|p| nearest_system(p, &pos, 10.0));
        }
        let ctx_sys = self.ctx_menu_system;
        resp.context_menu(|ui| {
            ui.set_min_width(210.0);
            let Some(sid) = ctx_sys else {
                ui.close();
                return;
            };
            if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(sid)) {
                ui.label(egui::RichText::new(&info.name).strong());
            }
            let has_char = self.active_character != "No character";
            if ui
                .add_enabled(has_char, egui::Button::new("Set Destination"))
                .on_disabled_hover_text("Log a character in to route in the game")
                .clicked()
            {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                self.set_destination_esi(cid, self.active_character.clone(), sid);
                self.route_destination = Some(sid);
                ui.close();
            }
            ui.separator();
            // The same menu the browser has, in the same order, plus the one thing the browser has
            // no use for. The route drag covers everything else.
            let anchors = self.map_route_anchors.clone();
            let at = anchors.iter().position(|&a| a == sid);
            let planning = self.map_route_kind_active();
            if planning && !anchors.is_empty() {
                match at {
                    None => {
                        if ui.button("Set as Destination").clicked() {
                            self.map_route_set_dest(sid);
                            ui.close();
                        }
                        if anchors.len() > 1 && ui.button("Add Waypoint").clicked() {
                            self.map_route_add_waypoint(sid);
                            ui.close();
                        }
                    }
                    Some(i) if i > 0 => {
                        let last = i == anchors.len() - 1;
                        let label = if last { "Remove Destination" } else { "Remove Waypoint" };
                        if ui.button(label).clicked() {
                            self.map_route_anchors.remove(i);
                            self.map_replan_route();
                            ui.close();
                        }
                    }
                    _ => {}
                }
                if ui
                    .button(egui::RichText::new("Clear Route").color(crate::theme::standing::HOSTILE))
                    .clicked()
                {
                    self.map_route_clear();
                    ui.close();
                }
                if self.map_route_kind == "titan" {
                    // Any system, not only ones on the route: where the ships are is the question
                    // the titan route is asking, and the answer is often nowhere near the path.
                    let t = self.map_titans.contains(&sid);
                    if ui
                        .button(if t { "Not a titan system" } else { "Set as titan system" })
                        .clicked()
                    {
                        self.map_titans.retain(|&x| x != sid);
                        if !t {
                            self.map_titans.push(sid);
                        }
                        self.map_replan_route();
                        ui.close();
                    }
                }
                ui.separator();
                let once = self.map_avoid_once.contains(&sid);
                if ui.button(if once { "Stop avoiding here" } else { "Avoid for this route" }).clicked()
                {
                    if once {
                        self.map_avoid_once.remove(&sid);
                    } else {
                        self.map_avoid_once.insert(sid);
                    }
                    self.map_replan_route();
                    ui.close();
                }
                let jump = self.map_route_kind == "jump";
                let always = if jump {
                    self.settings.route_avoid_jump.contains(&sid)
                } else {
                    self.settings.route_avoid_gate.contains(&sid)
                };
                if ui.button(if always { "Stop avoiding always" } else { "Avoid always" }).clicked() {
                    self.apply_overlay_message(
                        crate::ipc::OverlayToMain::AvoidSystem { id: sid, jump, on: !always },
                        ui.ctx(),
                    );
                    self.map_replan_route();
                    ui.close();
                }
                ui.separator();
            }
            let verb = if planning && !anchors.is_empty() { "Restart as" } else { "Start" };
            for (kind, name) in [("gate", "Gate Route"), ("jump", "Jump Route"), ("titan", "Titan Route")] {
                if ui.button(format!("{verb} {name}")).clicked() {
                    self.map_route_start(kind, sid);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Show info").clicked() {
                self.map_selected = Some(sid);
                self.right_dock_open = true;
                self.right_dock_tab = RightDockTab::System;
                ui.close();
            }
            ui.separator();
            let view = self.notes_view.clone();
            let label = notes_folder_label(&view);
            let subject = crate::notes::Subject::System(sid);
            // A long tag list under a full system menu runs off the window, so it gets its own.
            let mut picked = None;
            ui.menu_button(format!("{}  Tags", egui_phosphor::regular::TAG), |ui| {
                picked = notes_tag_toggles(ui, &view, &label, &subject);
            });
            if ui.button(format!("{}  Edit note and tags…", egui_phosphor::regular::NOTE_PENCIL)).clicked() {
                picked = Some(IntelClick::Annotate(subject.clone()));
                ui.close();
            }
            if let Some(c) = picked {
                self.notes_click(c);
            }
        });

        let painter = ui.painter_at(rect);
        // In overlay mode the transparent viewport frame already supplies the opacity-scaled
        // backdrop; an opaque canvas rect here would mask it and keep the overlay solid.
        if !self.map_overlay_mode {
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        }

        let dot = (0.5 * self.map_zoom).clamp(0.7, 12.0);
        #[cfg(feature = "fc-rescue")]
        let rescue_active = self.settings.fc_rescue_enabled;
        let ov = self.map_overlays;
        let zoomed = matches!(self.map_view, MapView::Region(_)) || self.map_zoom >= 12.0;
        let show_sys_labels = zoomed;
        let cull = rect.expand(8.0);

        // The row above a dot reads: wormhole/camp icons, name, sov upgrade icons, all centred on the
        // dot as one block. It is laid out here, once, because the pieces draw in different passes:
        // the upgrade icons on the right need the width of everything to their left, and all of them
        // need to know whether the name survived culling.
        const NAME_FONT: f32 = 12.0;
        const NAME_GAP: f32 = 4.0;
        let name_font = egui::FontId::proportional(NAME_FONT);
        // Icons track the dot, so they neither float away from a tiny dot nor crowd a fat one.
        let icon_h = (dot * 1.6 + 8.0).clamp(11.0, 20.0);
        let icon_w = icon_h + 3.0;
        let icon_font = egui::FontId::proportional(icon_h);

        let mut lead_icons: std::collections::HashMap<i64, Vec<(&str, egui::Color32)>> =
            std::collections::HashMap::new();
        if ov.wormholes {
            let wh_col = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
            for sid in &self.wh_overlay.jspace_holes {
                lead_icons
                    .entry(*sid)
                    .or_default()
                    .push((egui_phosphor::regular::SPIRAL, wh_col));
            }
        }
        if ov.jove {
            for s in &self.map_draw {
                if crate::jove::has(s.id) {
                    lead_icons
                        .entry(s.id)
                        .or_default()
                        .push((egui_phosphor::regular::CELL_TOWER, JOVE_COLOR));
                }
            }
        }
        if ov.notes {
            let weak = ui.visuals().weak_text_color();
            for (id, m) in &self.notes_view.systems {
                let icons = lead_icons.entry(*id).or_default();
                // One marker per tag, so a second tag is visible without hovering. Capped so a heavily
                // tagged system does not push its name off across its neighbours.
                for t in self.notes_view.tags_of(m).take(MAP_TAG_MARKS) {
                    icons.push((egui_phosphor::regular::TAG, crate::notes::color32(t.color)));
                }
                if m.has_note() {
                    icons.push((egui_phosphor::regular::NOTE, weak));
                }
            }
        }
        if ov.camps {
            let now = chrono::Utc::now().timestamp();
            if now - self.camped_cache_at >= 2 {
                self.camped_cache = self.camps.lock().unwrap().camped(now);
                self.camped_cache_at = now;
            }
            for (id, level) in &self.camped_cache {
                lead_icons
                    .entry(*id)
                    .or_default()
                    .push((egui_phosphor::regular::CAMPFIRE, camp_color(*level)));
            }
        }

        let mut upgrade_icons: std::collections::HashMap<i64, Vec<String>> =
            std::collections::HashMap::new();
        if ov.upgrades && zoomed {
            let mut by_name: std::collections::HashMap<String, Vec<&str>> =
                std::collections::HashMap::new();
            for u in &self.settings.sov_upgrades {
                by_name.entry(u.system.to_lowercase()).or_default().push(u.upgrade.as_str());
            }
            let kinds = self.upgrade_kinds;
            for s in &self.map_draw {
                if let Some(ups) = by_name.get(&s.name.to_lowercase()) {
                    let parts: Vec<String> = ups
                        .iter()
                        .flat_map(|u| split_upgrade_label(u))
                        .filter(|up| kinds[upgrade_kind(up) as usize])
                        .take(6)
                        .map(str::to_owned)
                        .collect();
                    if !parts.is_empty() {
                        upgrade_icons.insert(s.id, parts);
                    }
                }
            }
        }

        let mut label_at: std::collections::HashMap<i64, LabelRow> =
            std::collections::HashMap::new();
        {
            let mut placed: Vec<egui::Rect> = Vec::new();
            for s in &self.map_draw {
                let p = pos[&s.id];
                if !cull.contains(p) {
                    continue;
                }
                let lead = lead_icons.get(&s.id).map_or(0, Vec::len) as f32;
                let right = upgrade_icons.get(&s.id).map_or(0, Vec::len) as f32;
                if lead == 0.0 && right == 0.0 && !show_sys_labels {
                    continue;
                }
                // Extra lift without a name, to clear the halos that ring the bare dot.
                let mid_y = p.y - dot - if show_sys_labels { 2.0 } else { 8.0 } - icon_h / 2.0;
                let mut name_w = if show_sys_labels {
                    painter
                        .layout_no_wrap(s.name.clone(), name_font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                } else {
                    0.0
                };
                let lay = |name_w: f32| {
                    let name_span = if name_w > 0.0 { name_w + NAME_GAP } else { 0.0 };
                    let total = (lead + right) * icon_w + name_span;
                    let left = p.x - total / 2.0;
                    let name_x = left + lead * icon_w;
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(left, mid_y - icon_h / 2.0),
                        egui::pos2(left + total, mid_y + icon_h / 2.0),
                    );
                    LabelRow {
                        lead_x: left,
                        name_x,
                        icons_x: name_x + name_span,
                        mid_y,
                        name_shown: name_w > 0.0,
                        rect,
                    }
                };
                if name_w > 0.0 && placed.iter().any(|r| r.expand(2.0).intersects(lay(name_w).rect))
                {
                    // No room for the name. The icons stay, and re-centre on the dot as if the name
                    // had never been there.
                    name_w = 0.0;
                }
                let row = lay(name_w);
                if row.name_shown {
                    placed.push(row.rect);
                }
                label_at.insert(s.id, row);
            }
        }
        let mut activity_heat: std::collections::HashMap<i64, egui::Color32> =
            std::collections::HashMap::new();
        // Zoomed out, only the busy systems are worth a number; a quiet system shows nothing at all.
        let act_floor: u32 = match self.map_zoom {
            z if z >= 12.0 => 1,
            z if z >= 6.0 => 3,
            z if z >= 3.0 => 10,
            _ => 25,
        };

        if let Some(up) = &self.map_highlight_upgrade {
            let upl = up.to_lowercase();
            let hi: std::collections::HashSet<String> = self
                .settings
                .sov_upgrades
                .iter()
                .filter(|u| split_upgrade_label(&u.upgrade).iter().any(|p| p.to_lowercase() == upl))
                .map(|u| u.system.to_lowercase())
                .collect();
            let col = ui.visuals().hyperlink_color;
            for s in &self.map_draw {
                if hi.contains(&s.name.to_lowercase()) {
                    painter.circle_filled(pos[&s.id], dot + 9.0, col.gamma_multiply(0.28));
                }
            }
        }

        let bridges: std::collections::HashSet<(i64, i64)> = if let Some(g) = &self.systems {
            self.settings
                .jump_bridges
                .iter()
                .filter_map(|b| {
                    let a = g.lookup(&b.from)?.id;
                    let c = g.lookup(&b.to)?.id;
                    Some((a.min(c), a.max(c)))
                })
                .collect()
        } else {
            Default::default()
        };

        let cull = rect.expand(8.0);
        let seg_visible = |a: egui::Pos2, b: egui::Pos2| egui::Rect::from_two_pos(a, b).intersects(cull);

        let line_col = ui.visuals().weak_text_color().gamma_multiply(0.5);
        // A gate says where you are as much as where you can go: inside a constellation, out of it,
        // or out of the region entirely. On a map of identical solid lines none of those boundaries
        // would be visible.
        let region_of: std::collections::HashMap<i64, i64> =
            self.map_draw.iter().map(|s| (s.id, s.region_id)).collect();
        let constel_of: std::collections::HashMap<i64, &str> = self
            .systems
            .as_ref()
            .map(|g| {
                self.map_draw
                    .iter()
                    .filter_map(|s| g.info_of(s.id).map(|i| (s.id, i.constellation.as_str())))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(graph) = &self.systems {
            for s in &self.map_draw {
                let p1 = pos[&s.id];
                for &n in graph.neighbors(s.id) {
                    if s.id < n && !bridges.contains(&(s.id, n)) {
                        if let Some(p2) = pos.get(&n) {
                            if seg_visible(p1, *p2) {
                                let stroke = egui::Stroke::new(1.0, line_col);
                                let other_region =
                                    region_of.get(&n).is_some_and(|r| *r != s.region_id);
                                let other_constel = constel_of.get(&s.id).zip(constel_of.get(&n))
                                    .is_some_and(|(a, b)| a != b);
                                if other_region {
                                    painter.extend(egui::Shape::dashed_line(
                                        &[p1, *p2],
                                        stroke,
                                        4.0,
                                        4.0,
                                    ));
                                } else if other_constel {
                                    // Dotted: a short dash with a wide gap reads as dots without
                                    // needing a separate shape.
                                    painter.extend(egui::Shape::dashed_line(
                                        &[p1, *p2],
                                        stroke,
                                        1.0,
                                        3.0,
                                    ));
                                } else {
                                    painter.line_segment([p1, *p2], stroke);
                                }
                            }
                        }
                    }
                }
            }
        }
        if ov.bridges {
            let bridge_col = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);
            // A bridge a route is flying is drawn by that route, animated and in the route's colour,
            // so the plain arc is skipped rather than putting two lines on one hop.
            let mut routed: std::collections::HashSet<(i64, i64)> = Default::default();
            let mut note = |a: i64, b: i64| {
                routed.insert((a.min(b), a.max(b)));
            };
            if let Some(o) = self.map_route_opts.get(self.map_route_at) {
                for w in o.hops.windows(2) {
                    if w[1].kind == 1 {
                        note(w[0].id, w[1].id);
                    }
                }
            }
            if let Some(r) = &self.travel_route {
                for w in r.windows(2) {
                    note(w[0], w[1]);
                }
            }
            // And the in-game destination route, which walks the same graph a few lines below.
            if let (Some(ps), Some(dest), Some(g)) =
                (player_sys, self.set_route_shown(), self.systems.as_ref())
            {
                let holes = if self.settings.route_via_wormholes {
                    self.wh_adjacency()
                } else {
                    std::collections::HashMap::new()
                };
                if let Some(r) = g.route_with(ps, dest, true, true, &holes, |_| true) {
                    for w in r.windows(2) {
                        note(w[0], w[1]);
                    }
                }
            }
            for &(a, c) in &bridges {
                if routed.contains(&(a.min(c), a.max(c))) {
                    continue;
                }
                if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&c)) {
                    if seg_visible(*p1, *p2) {
                        painter.add(egui::Shape::line(
                            arc_polyline(*p1, *p2, BRIDGE_BOW),
                            egui::Stroke::new(1.5, bridge_col),
                        ));
                    }
                }
            }
        }

        if ov.wormholes {
            let wh_col = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
            let chain_col = egui::Color32::from_rgb(0xB0, 0x7C, 0xE8);
            const TURNUR: i64 = 30_002_086;
            for &(a, b) in &self.wh_overlay.direct {
                if !ov.turnur && (a == TURNUR || b == TURNUR) {
                    continue;
                }
                if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&b)) {
                    painter.line_segment([*p1, *p2], egui::Stroke::new(1.6, wh_col));
                }
            }
            for &(a, b, hops) in &self.wh_overlay.chains {
                if !ov.turnur && (a == TURNUR || b == TURNUR) {
                    continue;
                }
                if let (Some(p1), Some(p2)) = (pos.get(&a), pos.get(&b)) {
                    painter.extend(egui::Shape::dashed_line(
                        &[*p1, *p2],
                        egui::Stroke::new(1.8, chain_col),
                        6.0,
                        4.0,
                    ));
                    let mid = egui::pos2((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
                    let txt = format!("{hops}J");
                    let r = painter.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        &txt,
                        egui::FontId::proportional(11.0),
                        chain_col,
                    );
                    painter.rect_filled(r.expand(2.0), 3.0, ui.visuals().extreme_bg_color.gamma_multiply(0.7));
                    painter.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        &txt,
                        egui::FontId::proportional(11.0),
                        chain_col,
                    );
                }
            }
            if ov.thera {
                let conns: Vec<&crate::store::MapSystem> = self
                    .wh_overlay
                    .thera_conns
                    .iter()
                    .filter_map(|id| self.map_draw.iter().find(|s| s.id == *id))
                    .collect();
                let conn_screen: Vec<egui::Pos2> =
                    conns.iter().filter_map(|s| pos.get(&s.id).copied()).collect();
                if !conns.is_empty() && !conn_screen.is_empty() {
                    let mut cx = conns.iter().map(|s| s.x).sum::<f64>() / conns.len() as f64;
                    let min_z = conns.iter().map(|s| s.z).fold(f64::INFINITY, f64::min);
                    let max_z = conns.iter().map(|s| s.z).fold(f64::NEG_INFINITY, f64::max);
                    let mut tz = min_z - (max_z - min_z).max(1.0) * 0.25;
                    if self.map_layout == crate::map::MapLayout::Spaced {
                        let rc = |rid: i64| -> Option<(f64, f64)> {
                            let sys: Vec<&crate::store::MapSystem> =
                                self.map_draw.iter().filter(|s| s.region_id == rid).collect();
                            if sys.is_empty() {
                                return None;
                            }
                            let n = sys.len() as f64;
                            Some((
                                sys.iter().map(|s| s.x).sum::<f64>() / n,
                                sys.iter().map(|s| s.z).sum::<f64>() / n,
                            ))
                        };
                        if let (Some(sl), Some(dm)) = (rc(10_000_053), rc(10_000_045)) {
                            cx = (sl.0 + dm.0) / 2.0;
                            tz = (sl.1 + dm.1) / 2.0;
                        }
                    }
                    let tp = crate::map::project(cx, tz, &bounds, rect, self.map_zoom, self.map_pan);
                    let line_col = egui::Color32::from_rgb(0x6E, 0xC8, 0xF0);
                    let tcol = egui::Color32::from_rgb(0xB0, 0x70, 0xE0);
                    for p in &conn_screen {
                        painter.line_segment([tp, *p], egui::Stroke::new(1.6, line_col));
                    }
                    painter.circle_filled(tp, dot + 3.0, tcol);
                    painter.circle_stroke(tp, dot + 6.0, egui::Stroke::new(2.0, tcol));
                    let lp = tp + egui::vec2(0.0, -dot - 11.0);
                    let r = painter.text(lp, egui::Align2::CENTER_CENTER, "Thera",
                        egui::FontId::proportional(12.0), tcol);
                    painter.rect_filled(r.expand(2.0), 3.0,
                        ui.visuals().extreme_bg_color.gamma_multiply(0.7));
                    painter.text(lp, egui::Align2::CENTER_CENTER, "Thera",
                        egui::FontId::proportional(12.0), tcol);
                }
            }
            if ov.turnur {
                if let Some(tp) = pos.get(&TURNUR).copied() {
                    let col = egui::Color32::from_rgb(0xE0, 0xA8, 0x4C);
                    painter.circle_stroke(tp, dot + 6.0, egui::Stroke::new(2.0, col));
                    let lp = tp + egui::vec2(0.0, -dot - 11.0);
                    let r = painter.text(lp, egui::Align2::CENTER_CENTER, "Turnur",
                        egui::FontId::proportional(12.0), col);
                    painter.rect_filled(r.expand(2.0), 3.0,
                        ui.visuals().extreme_bg_color.gamma_multiply(0.7));
                    painter.text(lp, egui::Align2::CENTER_CENTER, "Turnur",
                        egui::FontId::proportional(12.0), col);
                }
            }
        }

        if ov.adm || ov.activity != ActivityMode::Off || ov.upgrades {
            let status = self.system_status.lock().unwrap();
            for s in &self.map_draw {
                let p = pos[&s.id];
                if let Some(f) = status.get(&s.id) {
                    if ov.adm {
                        if let Some(adm) = f.adm {
                            let c = if adm >= 5.0 {
                                egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
                            } else if adm >= 3.0 {
                                crate::theme::standing::WARNING
                            } else {
                                crate::theme::standing::HOSTILE
                            };
                            painter.circle_filled(p, dot + 7.0, c.gamma_multiply(0.30));
                        }
                    }
                    if ov.activity != ActivityMode::Off {
                        let v = ov.activity.value(f);
                        if v >= act_floor {
                            activity_heat.insert(s.id, activity_color(v, ov.activity.scale()));
                            // Zoomed out there is no room for a number under every dot; the dot
                            // itself carries the heat instead (see the dot loop).
                            if show_sys_labels {
                                painter.text(
                                    p + egui::vec2(0.0, dot + 3.0),
                                    egui::Align2::CENTER_TOP,
                                    compact_count(v),
                                    egui::FontId::proportional(12.0),
                                    activity_color(v, ov.activity.scale()),
                                );
                            }
                        }
                    }
                }
                if let (Some(ups), Some(row)) =
                    (upgrade_icons.get(&s.id), label_at.get(&s.id))
                {
                    for (k, up) in ups.iter().enumerate() {
                        let ip = egui::pos2(row.icons_x + k as f32 * icon_w, row.mid_y);
                        if ip.x + icon_w > rect.right()
                            || ip.y - icon_h / 2.0 < rect.top()
                            || !rect.contains(ip)
                        {
                            continue;
                        }
                        let (kind, level) = upgrade_info(up);
                        let lcol = level_color(level);
                        match kind {
                            UpgradeIcon::Glyph(g) => {
                                painter.text(
                                    ip,
                                    egui::Align2::LEFT_CENTER,
                                    g,
                                    icon_font.clone(),
                                    lcol,
                                );
                            }
                            UpgradeIcon::Mineral(tid) => {
                                let r = egui::Rect::from_min_size(
                                    egui::pos2(ip.x, ip.y - icon_h / 2.0),
                                    egui::Vec2::splat(icon_h),
                                );
                                ui.put(r, egui::Image::new(eve_type_icon_url(tid, icon_h)))
                                    .on_hover_text(up);
                                painter.circle_filled(r.right_top(), 3.0, lcol);
                            }
                        }
                    }
                }
            }
        }

        let mut reached_dest = false;
        if let (Some(dest), Some(ps)) = (self.set_route_shown(), player_sys) {
            if ps == dest {
                reached_dest = true;
            } else {
                // Drawing the client's own idea of the route would trace the long k-space path a
                // hole route deliberately skips, so this walks the same graph the waypoints came from.
                let holes = if self.settings.route_via_wormholes {
                    self.wh_adjacency()
                } else {
                    std::collections::HashMap::new()
                };
                let route = self
                    .systems
                    .as_ref()
                    .and_then(|g| g.route_with(ps, dest, true, true, &holes, |_| true));
                if let Some(route) = route {
                    let phase = (ui.input(|i| i.time) * 28.0) as f32;
                    // A J-space leg has no place on the map, so the hop is drawn between the k-space
                    // systems on either side of the hole.
                    let mut last: Option<(i64, egui::Pos2)> = None;
                    let mut jumped_hole = false;
                    for &id in &route {
                        let Some(&p) = pos.get(&id) else {
                            jumped_hole = true;
                            continue;
                        };
                        if let Some((prev_id, prev_p)) = last {
                            let leg = self.leg_kind(prev_id, id, jumped_hole);
                            // A bridge arcs here too: every other layer arcs them, so a straight
                            // line would read as a gate.
                            if leg == Leg::Bridge {
                                polyline_flow(
                                    &painter,
                                    &arc_polyline(prev_p, p, BRIDGE_BOW),
                                    leg.color(),
                                    phase,
                                );
                            } else {
                                dashed_flow(&painter, prev_p, p, leg.color(), phase);
                            }
                        }
                        last = Some((id, p));
                        jumped_hole = false;
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                }
            }
        }
        if reached_dest {
            self.route_destination = None;
        }

        // The campfire itself rides the label row (see `lead_icons`); only its glow stays on the dot.
        if ov.camps {
            for (id, level) in &self.camped_cache {
                if let Some(p) = pos.get(id) {
                    let glow = match level {
                        crate::camp::CampLevel::Likely => 0.30,
                        crate::camp::CampLevel::Possible => 0.20,
                        crate::camp::CampLevel::Flag => 0.12,
                    };
                    painter.circle_filled(*p, dot + 7.0, camp_color(*level).gamma_multiply(glow));
                }
            }
        }

        if self.map_mode == MapMode::Travel {
            let cyan = egui::Color32::from_rgb(0x4F, 0xC3, 0xF7);
            if let Some(direct) = &self.travel_direct_route {
                let gray = egui::Color32::from_rgb(0x9E, 0x9E, 0x9E);
                for w in direct.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, gray));
                    }
                }
            }
            if let Some(base) = self.travel_live.then_some(self.travel_live_base.as_ref()).flatten() {
                let purple = egui::Color32::from_rgb(0x95, 0x75, 0xCD);
                for w in base.windows(2) {
                    if let (Some(p1), Some(p2)) = (pos.get(&w[0]), pos.get(&w[1])) {
                        painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, purple));
                    }
                }
            }
            if let Some(route) = &self.travel_route {
                // A leg through J-space has no position on the k-space map, so it is drawn as one
                // dashed hop between the k-space systems on either side of the hole.
                let mut last: Option<(i64, egui::Pos2)> = None;
                let mut jumped_hole = false;
                for &id in route {
                    let Some(&p) = pos.get(&id) else {
                        jumped_hole = true;
                        continue;
                    };
                    if let Some((prev_id, prev_p)) = last {
                        match self.leg_kind(prev_id, id, jumped_hole) {
                            Leg::Gate => {
                                painter.line_segment([prev_p, p], egui::Stroke::new(2.5, cyan));
                            }
                            // A bridge leg follows the same arch the bridge itself is drawn as, in
                            // the route colour, so the route overrides it rather than crossing it
                            // with a second line of a different shape.
                            Leg::Bridge => {
                                painter.add(egui::Shape::line(
                                    arc_polyline(prev_p, p, BRIDGE_BOW),
                                    egui::Stroke::new(2.5, Leg::Bridge.color()),
                                ));
                            }
                            kind => {
                                painter.extend(egui::Shape::dashed_line(
                                    &[prev_p, p],
                                    egui::Stroke::new(2.5, kind.color()),
                                    7.0,
                                    5.0,
                                ));
                            }
                        }
                    }
                    last = Some((id, p));
                    jumped_hole = false;
                }
            }
            let mark = |p: egui::Pos2, color: egui::Color32| {
                let r = egui::Rect::from_center_size(p, egui::vec2(14.0, 14.0));
                let st = egui::Stroke::new(2.0, color);
                painter.line_segment([r.left_top(), r.right_top()], st);
                painter.line_segment([r.right_top(), r.right_bottom()], st);
                painter.line_segment([r.right_bottom(), r.left_bottom()], st);
                painter.line_segment([r.left_bottom(), r.left_top()], st);
            };
            for wp in &self.travel_waypoints {
                if let Some(p) = pos.get(wp) {
                    mark(*p, cyan);
                }
            }
            if let Some(p) = self.travel_start.and_then(|s| pos.get(&s)) {
                mark(*p, egui::Color32::from_rgb(0x66, 0xBB, 0x6A));
            }
            if let Some(p) = self.travel_end.and_then(|e| pos.get(&e)) {
                mark(*p, egui::Color32::from_rgb(0xFF, 0xA7, 0x26));
            }
            if let Some(at) = self.travel_changed_at {
                if chrono::Utc::now().timestamp() - at < 6 {
                    let blink = ((ui.input(|i| i.time) * 5.0).sin() * 0.5 + 0.5) as f32;
                    let warn = egui::Color32::from_rgb(0xFF, 0xD5, 0x4F);
                    for id in &self.travel_changed {
                        if let Some(p) = pos.get(id) {
                            painter.circle_stroke(
                                *p,
                                11.0,
                                egui::Stroke::new(2.5, warn.gamma_multiply(blink)),
                            );
                        }
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                }
            }
        }


        // Where a capital can sit, while a capital route is being planned.
        if !self.map_route_anchors.is_empty() && self.map_route_kind != "gate" {
            let teal = egui::Color32::from_rgb(0x4D, 0xB6, 0xAC);
            for d in self.jump_dockable_ids() {
                if let Some(p) = pos.get(&d) {
                    painter.circle_stroke(*p, 9.0, egui::Stroke::new(1.5, teal));
                }
            }
        }

        // Everything the route is being planned around, while it is being planned. A cross, not a
        // ring: a ring is what this map uses for "look here", and this is the opposite.
        if !self.map_route_anchors.is_empty() {
            let jumping = self.map_route_kind == "jump";
            let always: Vec<i64> = if jumping {
                self.settings.route_avoid_jump.clone()
            } else {
                self.settings.route_avoid_gate.clone()
            };
            for id in always.iter().chain(self.map_avoid_once.iter()) {
                if let Some(&p) = pos.get(id) {
                    let d = (dot * 2.2).max(4.0);
                    let st = egui::Stroke::new(1.6, crate::theme::standing::HOSTILE);
                    painter.line_segment([p - egui::vec2(d, d), p + egui::vec2(d, d)], st);
                    painter.line_segment([p + egui::vec2(d, -d), p - egui::vec2(d, -d)], st);
                }
            }
        }

        // A route with a start and nowhere to go yet, or the menu's "Start ... Route" looks like it
        // did nothing until a destination is picked.
        if self.map_route_anchors.len() == 1 {
            if let Some(&p) = pos.get(&self.map_route_anchors[0]) {
                painter.circle_stroke(
                    p,
                    (dot * 3.8).max(7.0),
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(0xF2, 0xB1, 0x34)),
                );
            }
        }

        // Where the ships are, whether or not the route currently goes near them.
        if self.map_route_kind == "titan" {
            const TITAN_COL: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x7A, 0x3D);
            for t in &self.map_titans {
                if let Some(&p) = pos.get(t) {
                    painter.circle_filled(p, 11.0, TITAN_COL.gamma_multiply(0.22));
                    painter.text(
                        p - egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::STAR_FOUR,
                        egui::FontId::proportional(14.0),
                        TITAN_COL,
                    );
                }
            }
        }

        // The route the drag settled on, in its own colours so it does not read as the travel route.
        if let Some(o) = self.map_route_opts.get(self.map_route_at) {
            let phase = (ui.input(|i| i.time) * 28.0) as f32;
            ui.ctx().request_repaint();
            const PICK_GATE: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xB1, 0x34);
            const PICK_JUMP: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x7B, 0xE0);
            const PICK_BRIDGE: egui::Color32 = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);
            for (i, h) in o.hops.iter().enumerate().skip(1) {
                let (Some(&a), Some(&b)) = (pos.get(&o.hops[i - 1].id), pos.get(&h.id)) else {
                    continue;
                };
                match h.kind {
                    2 | 1 => {
                        let col = if h.kind == 2 { PICK_JUMP } else { PICK_BRIDGE };
                        // Dashed and crawling like the gates and like the browser's: an arc drawn
                        // solid while the rest of the route moves reads as a different kind of thing.
                        polyline_flow(&painter, &arc_polyline(a, b, BRIDGE_BOW), col, phase);
                    }
                    // Crawling dashes, the same as the browser's and the same as this map's own
                    // travel route: a static line is hard to pick out of a map already full of them.
                    _ => dashed_flow(&painter, a, b, PICK_GATE, phase),
                }
            }
            // The systems the user named, as opposed to the ones the route passes through.
            for h in &o.hops {
                if !h.anchor {
                    continue;
                }
                if let Some(&p) = pos.get(&h.id) {
                    let r = (dot * 4.4).max(8.0);
                    painter.circle_filled(p, r, PICK_GATE.gamma_multiply(0.18));
                    painter.circle_stroke(p, r, egui::Stroke::new(2.5, PICK_GATE));
                    painter.circle_stroke(p, r * 0.6, egui::Stroke::new(1.2, PICK_GATE));
                }
            }
            // The titan's own jump, which the fleet does not fly: a long dash the other way round,
            // so it reads as a second ship moving rather than as part of the route.
            if let Some(tj) = &o.titan_jump {
                if let (Some(&a), Some(&b)) = (pos.get(&tj.from), pos.get(&tj.to)) {
                    // Its own colour, and running the other way: a different ship doing a different
                    // thing, which the capital-jump colour would present as the same move.
                    polyline_flow(
                        &painter,
                        &arc_polyline(a, b, BRIDGE_BOW),
                        egui::Color32::from_rgb(0xFF, 0x7A, 0x3D),
                        -phase,
                    );
                }
            }
        }

        // The route drag: a line from where it started to the pointer, snapped to whatever it is
        // over, with the three distances beside it. Drawn before the hover ring so the ring lands on
        // top of the endpoint rather than under it.
        if let Some(from) = self.map_link {
            let cursor = ui.input(|i| i.pointer.interact_pos());
            if let (Some(&a), Some(c)) = (pos.get(&from), cursor) {
                let over = nearest_system(c, &pos, 14.0).filter(|id| *id != from);
                let b = over.and_then(|id| pos.get(&id).copied()).unwrap_or(c);
                let col = ui.visuals().hyperlink_color;
                if over.is_some() {
                    painter.line_segment([a, b], egui::Stroke::new(2.5, col));
                    painter.circle_stroke(b, 10.0, egui::Stroke::new(2.0, col));
                } else {
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        egui::Stroke::new(1.5, col),
                        5.0,
                        4.0,
                    ));
                }
                if let Some(to) = over {
                    self.map_link_tip(ui, b, from, to);
                }
                if resp.drag_stopped() {
                    self.map_link = None;
                    if let Some(to) = over {
                        self.map_link_menu = Some((from, to, b));
                    }
                }
            }
            if resp.drag_stopped() {
                self.map_link = None;
            }
        }

        self.map_link_menu_ui(ui);

        let hovered_id = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| resp.hovered())
            .and_then(|p| nearest_system(p, &pos, 8.0));
        // A selected system keeps its hover effects; hovering something else takes over.
        let focus_id = hovered_id.or(self.map_selected);
        if let (true, Some(h_id)) = (ov.jump_range, focus_id) {
            if let Some(real_h) = self.map_systems.iter().find(|s| s.id == h_id) {
                let hp = pos[&h_id];
                let band_color = [
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
                    egui::Color32::from_rgb(0xE0, 0xA4, 0x3A),
                    egui::Color32::from_rgb(0x4F, 0x9B, 0xD8),
                    egui::Color32::from_rgb(0xD8, 0x4C, 0x4C),
                ];
                if !schematic {
                    for (i, (name, ly)) in crate::map::JUMP_RANGES.iter().enumerate().rev() {
                        let col = band_color.get(i).copied().unwrap_or(band_color[3]);
                        let r = crate::map::ly_to_pixels(*ly, &bounds, rect, self.map_zoom);
                        painter.circle_stroke(hp, r, egui::Stroke::new(1.5, col.gamma_multiply(0.85)));
                        painter.text(
                            hp + egui::vec2(0.0, -r),
                            egui::Align2::CENTER_BOTTOM,
                            format!("{name} {ly:.0} ly"),
                            egui::FontId::proportional(12.0),
                            col,
                        );
                    }
                }
                // map_draw and map_systems share order, so index zips draw↔real.
                for (i, s) in self.map_draw.iter().enumerate() {
                    if s.id == h_id {
                        continue;
                    }
                    let d = crate::map::ly_distance(real_h, &self.map_systems[i]);
                    if let Some(b) = crate::map::JUMP_RANGES.iter().position(|(_, ly)| d <= *ly) {
                        let col = band_color.get(b).copied().unwrap_or(band_color[3]);
                        painter.circle_filled(pos[&s.id], dot + 4.0, col.gamma_multiply(0.70));
                    }
                }
            }
        }

        if self.map_hover_since.map(|(id, _)| id) != hovered_id {
            self.map_hover_since = hovered_id.map(|id| (id, std::time::Instant::now()));
        }
        let dwelled = self.map_hover_since.is_some_and(|(_, since)| {
            let waited = since.elapsed();
            if waited < MAP_TIP_DELAY {
                // Nothing else will redraw while the pointer sits still, so ask for the frame that
                // brings the tooltip up.
                ui.ctx().request_repaint_after(MAP_TIP_DELAY - waited);
                return false;
            }
            true
        });
        if let Some(h_id) = hovered_id.filter(|_| dwelled) {
            if let Some(ptr) = ui.ctx().pointer_hover_pos() {
                egui::Area::new(ui.id().with("map_hover_tip"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(ptr + egui::vec2(14.0, -6.0))
                    .pivot(egui::Align2::LEFT_BOTTOM)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            self.map_system_tooltip(ui, h_id);
                        });
                    });
            }
        }

        let now_ts = chrono::Utc::now().timestamp();
        let sev_rules = self.settings.severity.clone();
        let intel_map: std::collections::HashMap<i64, (crate::settings::Severity, i64)> = {
            let st = self.intel_state.lock().unwrap();
            let mut m: std::collections::HashMap<i64, (crate::settings::Severity, i64)> =
                std::collections::HashMap::new();
            for r in &st.reports {
                if r.clear || st.is_stale(r) {
                    continue;
                }
                if let Some(s) = r.primary_system() {
                    let sev = severity_of(r, &sev_rules);
                    let e = m.entry(s.id).or_insert((sev, r.received));
                    e.0 = e.0.max(sev);
                    e.1 = e.1.max(r.received);
                }
            }
            m
        };
        let blink = (ui.input(|i| i.time) as f32 * 6.0).sin().abs();
        let mut any_fresh = false;
        // The holder's colour rides the dot at every zoom; the logo only appears once the dots are
        // big enough to hang one on, below which a logo per system is unreadable clutter.
        let icon_px = (dot * 2.6).floor();
        let sov_art = self.sov_art(ui.ctx());
        let show_icons = icon_px >= 10.0;
        for s in &self.map_draw {
            let p = pos[&s.id];
            if !cull.contains(p) {
                continue;
            }
            let art = sov_art.get(&s.id);
            // Zoomed out the dot is the only thing left to say it with, so heat outranks the
            // holder's colour there; zoomed in the number below the dot carries it instead.
            let dot_col = activity_heat
                .get(&s.id)
                .copied()
                .filter(|_| !show_sys_labels)
                .or_else(|| art.and_then(|a| a.dot))
                .unwrap_or_else(|| security_color(s.security));
            painter.circle_filled(p, dot, dot_col);
            if let Some(a) = art.filter(|_| show_icons) {
                // Drawn through the map's clipped painter, not `Image::paint_at`, which would paint
                // into the panel layer and spill the logo over the side bars.
                let hint = egui::SizeHint::Size {
                    width: 64,
                    height: 64,
                    maintain_aspect_ratio: true,
                };
                if let Ok(egui::load::TexturePoll::Ready { texture }) =
                    ui.ctx().try_load_texture(&a.icon, egui::TextureOptions::LINEAR, hint)
                {
                    let r = egui::Rect::from_center_size(p, egui::vec2(icon_px, icon_px));
                    painter.image(
                        texture.id,
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
            if self.settings.bookmarks.contains(&s.id) {
                painter.circle_stroke(
                    p,
                    dot,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(0x4D, 0xB6, 0xAC)),
                );
            }
            if let Some((sev, received)) = intel_map.get(&s.id) {
                let base = severity_color(*sev);
                let fresh = now_ts - received < 15;
                let (fill_a, ring_w) = if fresh {
                    any_fresh = true;
                    (0.45 + 0.45 * blink, 3.0)
                } else {
                    (0.40, 2.5)
                };
                painter.circle_filled(p, dot + 5.0, base.gamma_multiply(fill_a));
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(ring_w, base));
            }
            if let Some((count, has_active)) = char_here.get(&s.id) {
                let blue = if *has_active {
                    egui::Color32::from_rgb(0x4F, 0xC3, 0xF7)
                } else {
                    egui::Color32::from_rgb(0xA8, 0xDE, 0xF7)
                };
                painter.circle_stroke(p, dot + 8.0, egui::Stroke::new(2.5, blue));
                if *count > 1 {
                    painter.text(
                        p + egui::vec2(dot + 9.0, -(dot + 9.0)),
                        egui::Align2::LEFT_BOTTOM,
                        count.to_string(),
                        egui::FontId::proportional(11.0),
                        blue,
                    );
                }
            }
            if Some(s.id) == hovered_id {
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if self.map_selected == Some(s.id) {
                painter.circle_stroke(p, dot + 6.0, egui::Stroke::new(2.5, egui::Color32::WHITE));
            }
            if let Some(row) = label_at.get(&s.id).filter(|_| rect.contains(p)) {
                // The icons always draw: dropping one would silently hide a camp or a hole. Only the
                // name is culled, and when it is, the row re-centres without it.
                if let Some(icons) = lead_icons.get(&s.id) {
                    for (k, (glyph, col)) in icons.iter().enumerate() {
                        painter.text(
                            egui::pos2(row.lead_x + k as f32 * icon_w, row.mid_y),
                            egui::Align2::LEFT_CENTER,
                            *glyph,
                            icon_font.clone(),
                            *col,
                        );
                    }
                }
                if row.name_shown {
                    let at = egui::pos2(row.name_x, row.mid_y);
                    // Outlined, so the name survives whatever it lands on: halos, sov icons, routes.
                    for off in OUTLINE {
                        painter.text(
                            at + off,
                            egui::Align2::LEFT_CENTER,
                            &s.name,
                            name_font.clone(),
                            egui::Color32::BLACK,
                        );
                    }
                    painter.text(
                        at,
                        egui::Align2::LEFT_CENTER,
                        &s.name,
                        name_font.clone(),
                        ui.visuals().text_color(),
                    );
                }
            }
        }
        if any_fresh {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }

        if !show_sys_labels {
            let mut acc: std::collections::HashMap<i64, (egui::Vec2, u32)> =
                std::collections::HashMap::new();
            for s in &self.map_draw {
                let e = acc.entry(s.region_id).or_insert((egui::Vec2::ZERO, 0));
                e.0 += pos[&s.id].to_vec2();
                e.1 += 1;
            }
            let mut labels: Vec<(i64, egui::Pos2)> =
                acc.into_iter().map(|(rid, (sum, n))| (rid, (sum / n as f32).to_pos2())).collect();
            labels.sort_by_key(|(rid, _)| *rid);
            let font = egui::FontId::proportional(16.0);
            for (rid, c) in labels {
                if !rect.contains(c) {
                    continue;
                }
                let Some((_, name)) = self.map_regions.iter().find(|(id, _)| *id == rid) else {
                    continue;
                };
                painter.text(
                    c + egui::vec2(1.0, 1.0),
                    egui::Align2::CENTER_CENTER,
                    name,
                    font.clone(),
                    egui::Color32::from_black_alpha(180),
                );
                painter.text(c, egui::Align2::CENTER_CENTER, name, font.clone(), egui::Color32::from_gray(220));
            }
        }

        // Cyno-generator layer + rescue highlights. Isolated in catch_unwind so a stale id can't
        // crash the map mid-rescue. Cyno markers show whenever the layer is on; staging/capital
        // rings only in rescue mode.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if ov.cyno_gen {
                let cyan = egui::Color32::from_rgb(0x3A, 0xC8, 0xE0);
                for id in &self.settings.cyno_generators {
                    if let Some(p) = pos.get(id) {
                        painter.circle_stroke(*p, dot + 5.0, egui::Stroke::new(2.0, cyan));
                        painter.circle_filled(*p, 2.0, cyan);
                    }
                }
            }
            #[cfg(feature = "fc-rescue")]
            if rescue_active {
                if let Some(sid) = self.systems.as_ref().and_then(|g| {
                    g.lookup(&self.settings.rescue_staging_system).map(|i| i.id)
                }) {
                    if let Some(p) = pos.get(&sid) {
                        let gold = egui::Color32::from_rgb(0xF0, 0xC0, 0x40);
                        painter.circle_stroke(*p, dot + 9.0, egui::Stroke::new(2.5, gold));
                        painter.text(
                            *p + egui::vec2(0.0, -(dot + 14.0)),
                            egui::Align2::CENTER_BOTTOM,
                            "STAGING",
                            egui::FontId::proportional(12.0),
                            gold,
                        );
                    }
                }
                let cap = self.rescue.lock().unwrap().capital_system;
                if let Some(cap) = cap {
                    if let Some(p) = pos.get(&cap) {
                        let red = egui::Color32::from_rgb(0xE0, 0x40, 0x40);
                        let a = (0.4 + 0.5 * blink).min(1.0);
                        painter.circle_filled(*p, dot + 7.0, red.gamma_multiply(a));
                        painter.circle_stroke(*p, dot + 9.0, egui::Stroke::new(3.0, red));
                        painter.text(
                            *p + egui::vec2(0.0, -(dot + 14.0)),
                            egui::Align2::CENTER_BOTTOM,
                            "CAPITAL",
                            egui::FontId::proportional(12.0),
                            red,
                        );
                    }
                }
            }
        }));

        self.map_chrome(ui, rect);
    }

    pub(crate) fn draw_threat_view(&mut self, ui: &mut egui::Ui, rect: egui::Rect, player_sys: Option<i64>) {
        use crate::map::MapLayout;
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();

        if resp.dragged() {
            self.map_pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = self.map_zoom;
                let new = (old * (scroll * 0.003).exp()).clamp(0.3, 6.0);
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let rel = m - (rect.center() + self.map_pan);
                    self.map_pan += rel * (1.0 - new / old);
                }
                self.map_zoom = new;
            }
        }

        let Some(graph) = self.systems.clone() else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "SDE not ready.", egui::FontId::proportional(14.0), visuals.weak_text_color());
            return;
        };
        let Some(center) = self.map_threat_center.or(player_sys) else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No centre system. Set an active character, or right-click a system on the map.",
                egui::FontId::proportional(13.0),
                visuals.weak_text_color(),
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            return;
        };
        let depth = self.map_threat_jumps.max(1);

        let (dist, children, order) = bfs_tree(&graph, center, depth, self.threat_include_bridges);
        let leaves = order.iter().filter(|id| children.get(id).map_or(true, |c| c.is_empty())).count();
        let mut frac: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
        let mut next = 0u32;
        assign_fracs(center, &children, leaves.max(1) as f32, &mut next, &mut frac);

        let zoom = self.map_zoom;
        let mut pos: std::collections::HashMap<i64, egui::Pos2> = std::collections::HashMap::new();
        match self.map_layout {
            MapLayout::Radial => {
                let c = rect.center() + self.map_pan;
                let ring = (rect.size().min_elem() * 0.44 / depth as f32) * zoom;
                for &id in &order {
                    let d = dist[&id];
                    if d == 0 {
                        pos.insert(id, c);
                    } else {
                        let ang =
                            frac[&id] * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                        pos.insert(id, c + egui::Vec2::angled(ang) * (d as f32 * ring));
                    }
                }
            }
            _ => {
                let level = (rect.height() * 0.82 / (depth as f32 + 0.5)) * zoom;
                let width = rect.width() * 0.92 * zoom;
                let cx = rect.center().x + self.map_pan.x;
                let top = rect.top() + 34.0 + self.map_pan.y;
                for &id in &order {
                    let x = cx + (frac[&id] - 0.5) * width;
                    let y = top + dist[&id] as f32 * level;
                    pos.insert(id, egui::pos2(x, y));
                }
            }
        }

        let sev_rules = self.settings.severity.clone();
        let intel_map: std::collections::HashMap<i64, (crate::settings::Severity, i64)> = {
            let st = self.intel_state.lock().unwrap();
            let mut m: std::collections::HashMap<i64, (crate::settings::Severity, i64)> =
                std::collections::HashMap::new();
            for r in &st.reports {
                if r.clear || st.is_stale(r) {
                    continue;
                }
                if let Some(sy) = r.primary_system() {
                    let sev = severity_of(r, &sev_rules);
                    let e = m.entry(sy.id).or_insert((sev, r.received));
                    e.0 = e.0.max(sev);
                    e.1 = e.1.max(r.received);
                }
            }
            m
        };
        let now_ts = chrono::Utc::now().timestamp();
        let blink = (ui.input(|i| i.time) as f32 * 6.0).sin().abs();
        let mut any_fresh = false;

        let edge = visuals.weak_text_color().gamma_multiply(0.5);
        let bridge_col = egui::Color32::from_rgb(0x4C, 0xC2, 0x6A);
        for &a in &order {
            for &b in graph.neighbors(a) {
                if a < b {
                    if let (Some(&pa), Some(&pb)) = (pos.get(&a), pos.get(&b)) {
                        if graph.is_bridge(a, b) {
                            painter.line_segment([pa, pb], egui::Stroke::new(2.0, bridge_col));
                        } else {
                            painter.line_segment([pa, pb], egui::Stroke::new(1.0, edge));
                        }
                    }
                }
            }
        }

        let label_max = 3;
        let line_h = 13.0;
        let stagger: std::collections::HashMap<i64, f32> = if matches!(self.map_layout, MapLayout::Radial) {
            std::collections::HashMap::new()
        } else {
            let mut ring: Vec<i64> = order.iter().copied().filter(|id| dist[id] == label_max).collect();
            ring.sort_by(|a, b| frac[a].partial_cmp(&frac[b]).unwrap_or(std::cmp::Ordering::Equal));
            ring.iter().enumerate().map(|(i, id)| (*id, (i % 3) as f32 * line_h)).collect()
        };
        let hovered = ui.input(|i| i.pointer.hover_pos()).and_then(|hp| nearest_system(hp, &pos, 12.0));

        let node_r = (5.5 * zoom.clamp(0.6, 1.6)).max(3.5);
        let font = egui::FontId::proportional((12.0 * zoom).clamp(9.0, 15.0));
        for &id in &order {
            let p = pos[&id];
            let info = graph.info_of(id);
            let sec = info.map(|i| i.security).unwrap_or(0.0);
            let is_center = id == center;
            let r = if is_center { node_r + 2.5 } else { node_r };
            if let Some((isev, received)) = intel_map.get(&id) {
                let base = severity_color(*isev);
                let fresh = now_ts - received < 15;
                let (glow, ring_w) = if fresh {
                    any_fresh = true;
                    (0.30 + 0.35 * blink, 3.0)
                } else {
                    (0.28, 2.5)
                };
                painter.circle_filled(p, r + 6.0, base.gamma_multiply(glow));
                painter.circle_stroke(p, r + 3.0, egui::Stroke::new(ring_w, base));
            }
            painter.circle_filled(p, r, security_color(sec));
            let outline = if is_center {
                egui::Color32::WHITE
            } else if Some(id) == player_sys {
                crate::theme::standing::ALLIANCE
            } else {
                visuals.window_stroke.color
            };
            painter.circle_stroke(p, r, egui::Stroke::new(if is_center { 2.0 } else { 1.0 }, outline));
            if Some(id) == hovered {
                painter.circle_stroke(p, r + 2.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if let (Some(info), true) = (info, dist[&id] <= label_max) {
                let extra = stagger.get(&id).copied().unwrap_or(0.0);
                painter.text(
                    p - egui::vec2(0.0, r + 2.0 + extra),
                    egui::Align2::CENTER_BOTTOM,
                    &info.name,
                    font.clone(),
                    visuals.text_color(),
                );
            }
        }

        if let Some(hid) = hovered {
            if dist[&hid] > label_max {
                if let Some(info) = graph.info_of(hid) {
                    let p = pos[&hid];
                    let anchor = p - egui::vec2(0.0, node_r + 3.0);
                    let g = painter.layout_no_wrap(info.name.clone(), font.clone(), visuals.text_color());
                    let r = egui::Rect::from_min_size(
                        anchor - egui::vec2(g.size().x / 2.0, g.size().y),
                        g.size(),
                    )
                    .expand(3.0);
                    painter.rect_filled(r, 3.0, visuals.window_fill.gamma_multiply(0.92));
                    painter.galley(r.min + egui::vec2(3.0, 3.0), g, visuals.text_color());
                }
            }
        }

        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.clicked() {
            if let Some(id) = pointer.and_then(|p| nearest_system(p, &pos, 12.0)) {
                self.dock_system(id);
            }
        }
        if resp.secondary_clicked() {
            if let Some(id) = pointer.and_then(|p| nearest_system(p, &pos, 12.0)) {
                self.map_threat_center = Some(id);
                self.map_pan = egui::Vec2::ZERO;
                self.map_zoom = 1.0;
            }
        }

        let cname = graph.info_of(center).map(|i| i.name.clone()).unwrap_or_default();
        painter.text(
            rect.left_bottom() + egui::vec2(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            format!("◎ {cname}  ·  ≤{depth} jumps  ·  {} systems", order.len()),
            egui::FontId::proportional(12.0),
            visuals.weak_text_color(),
        );
        if any_fresh {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    pub(crate) fn map_chrome(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        if self.map_overlay_mode {
            self.map_overlay_controls(ui, rect);
        } else if self.map_controls_hidden {
            egui::Area::new(ui.id().with("map_show_controls"))
                .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if ui
                            .button(egui_phosphor::regular::EYE)
                            .on_hover_text("Show controls")
                            .clicked()
                        {
                            self.map_controls_hidden = false;
                        }
                    });
                });
        } else {
            if !self.left_dock_open {
                egui::Area::new(ui.id().with("reopen_left"))
                    .fixed_pos(rect.left_top() + egui::vec2(6.0, 6.0))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            if ui.button("\u{00BB}").on_hover_text("Show map panel").clicked() {
                                self.left_dock_open = true;
                            }
                        });
                    });
            }
            if (self.map_mode != MapMode::Standard || self.map_docked_system.is_some())
                && !self.right_dock_open
            {
                egui::Area::new(ui.id().with("reopen_right"))
                    .fixed_pos(rect.right_top() + egui::vec2(-38.0, 6.0))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            if ui.button("\u{00AB}").on_hover_text("Show mode panel").clicked() {
                                self.right_dock_open = true;
                            }
                        });
                    });
            }
            self.map_search_overlay(ui, rect);
        }
    }

    pub(crate) fn map_layers_content(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        ui.label(egui::RichText::new(format!("{}  Sovereignty", icon::FLAG)).strong());
        ui.radio_value(&mut self.map_overlays.sov, SovMode::Off, "Off");
        ui.radio_value(&mut self.map_overlays.sov, SovMode::Alliance, "By alliance");
        ui.radio_value(&mut self.map_overlays.sov, SovMode::Coalition, "By coalition");
        ui.separator();
        ui.label(egui::RichText::new(format!("{}  Activity (last hour)", icon::FIRE)).strong());
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::Off, "Off");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::ShipKills, "Ship kills");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::PodKills, "Pod kills");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::NpcKills, "NPC kills");
        ui.radio_value(&mut self.map_overlays.activity, ActivityMode::Jumps, "Jumps");
        ui.separator();
        ui.checkbox(&mut self.map_overlays.adm, format!("{}  ADM", icon::SHIELD_CHECK));
        ui.checkbox(&mut self.map_overlays.bridges, format!("{}  Jump bridges", icon::ARROWS_LEFT_RIGHT));
        ui.checkbox(&mut self.map_overlays.cyno_gen, format!("{}  Cyno generators", icon::CROSSHAIR_SIMPLE));
        ui.checkbox(&mut self.map_overlays.upgrades, format!("{}  Sov upgrades", icon::MAP_PIN_LINE));
        if self.map_overlays.upgrades {
            ui.indent("upgrade_kinds", |ui| {
                ui.checkbox(&mut self.upgrade_kinds[0], "Ratting");
                ui.checkbox(&mut self.upgrade_kinds[1], "Exploration");
                ui.checkbox(&mut self.upgrade_kinds[2], "Mining");
                ui.checkbox(&mut self.upgrade_kinds[3], "Other");
            });
        }
        ui.checkbox(&mut self.map_overlays.jump_range, format!("{}  Jump range (hover)", icon::CROSSHAIR_SIMPLE));
        ui.separator();
        ui.checkbox(&mut self.map_overlays.wormholes, format!("{}  Wormhole connections", icon::SPIRAL));
        if self.map_overlays.wormholes {
            ui.indent("wh_hubs", |ui| {
                ui.checkbox(&mut self.map_overlays.thera, format!("{}  Thera", icon::PLANET));
                ui.checkbox(&mut self.map_overlays.turnur, format!("{}  Turnur", icon::PLANET));
            });
        }
        ui.checkbox(&mut self.map_overlays.camps, format!("{}  Gate camps", icon::CAMPFIRE));
        ui.checkbox(&mut self.map_overlays.jove, format!("{}  Jove observatories", icon::CELL_TOWER))
            .on_hover_text("Marks systems that hold a Jove Observatory");
        ui.checkbox(&mut self.map_overlays.notes, format!("{}  Notes and tags", icon::TAG))
            .on_hover_text("Marks systems you tagged or wrote a note on, in online folders");
        if ui
            .checkbox(&mut self.settings.route_via_wormholes, format!("{}  Route via wormholes", icon::SPIRAL))
            .on_hover_text("Routes and Set Destination use scanned holes, with a waypoint at each hole entrance")
            .changed()
        {
            self.needs_save = true;
            // Toggling this changes what the current destination should be, so re-send it.
            self.replan_routes();
        }
        if self.map_overlays.upgrades {
            ui.separator();
            ui.label(egui::RichText::new("Upgrade icons").strong());
            let mut row = |g: &str, txt: &str| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(g).size(16.0));
                    ui.label(txt);
                });
            };
            row(icon::SKULL, "Ratting / threat detection");
            row(icon::BROADCAST, "Exploration / scanning");
            row(icon::RADIOACTIVE, "Cyno");
            row(icon::GEAR, "Other upgrade");
            ui.label(egui::RichText::new("Mining shows the ore icon").weak());
            ui.horizontal(|ui| {
                ui.label("Level:");
                ui.colored_label(level_color(1), "1");
                ui.colored_label(level_color(2), "2");
                ui.colored_label(level_color(3), "3\u{2013}5");
            });
        }

        #[cfg(feature = "fc-rescue")]
        if self.settings.fc_rescue_enabled {
            ui.separator();
            ui.label(egui::RichText::new("delve911 rescue").strong());
            if ui.button(format!("{}  Open delve911 feed", icon::CHAT_CENTERED_DOTS)).clicked() {
                self.view = nav::View::Jabber;
                self.jabber_chat = None;
            }
            // Opens the window only. The feature being on is the mode, so a toggle here would be a
            // second switch for one decision.
            let btn = egui::Button::new(format!(
                "{}  Open rescue window",
                icon::WARNING_OCTAGON
            ));
            let btn = if self.rescue_armed {
                btn.fill(egui::Color32::from_rgb(0x80, 0x30, 0x30))
            } else {
                btn
            };
            if ui.add(btn).clicked() {
                self.view = nav::View::Rescue;
            }
        }
    }

    pub(crate) fn wormhole_section(&self, ui: &mut egui::Ui, id: i64) {
        let now = chrono::Utc::now().timestamp();
        let holes: Vec<&crate::wormholes::Wormhole> = self
            .wh_cache
            .iter()
            .filter(|w| w.system_id == id || w.dest_system_id == Some(id))
            .collect();
        if holes.is_empty() {
            return;
        }
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{}  Wormholes", egui_phosphor::regular::SPIRAL)).strong(),
        );
        for w in holes {
            let here_is_near = w.system_id == id;
            let other_id = if here_is_near { w.dest_system_id } else { Some(w.system_id) };
            let other = other_id
                .and_then(|sid| {
                    self.systems.as_ref().and_then(|g| g.info_of(sid)).map(|i| i.name.clone())
                })
                .unwrap_or_else(|| w.dest.label().to_owned());
            let sig = if here_is_near { w.signature.as_deref() } else { w.dest_signature.as_deref() }
                .unwrap_or("?");
            let mut parts: Vec<String> = Vec::new();
            if let Some(t) = &w.wh_type {
                parts.push(t.clone());
            }
            if let Some(s) = w.effective_size() {
                parts.push(s.label().to_owned());
            }
            parts.push(match w.hours_left(now) {
                Some(h) => format!("< {h}h"),
                None => "expiring".to_owned(),
            });
            ui.label(egui::RichText::new(format!("{sig} → {other}  ({})", parts.join(", "))));
        }
    }

    pub(crate) fn camp_line(&self, ui: &mut egui::Ui, id: i64) {
        let now = chrono::Utc::now().timestamp();
        if let Some(c) = self.camps.lock().unwrap().camp(id, now) {
            let mins = (c.age / 60).max(0);
            let (label, col) = match c.level {
                crate::camp::CampLevel::Likely => {
                    ("Likely gate camp", egui::Color32::from_rgb(0xEF, 0x44, 0x44))
                }
                crate::camp::CampLevel::Possible => {
                    ("Possible camp", egui::Color32::from_rgb(0xFF, 0xA7, 0x26))
                }
                crate::camp::CampLevel::Flag => {
                    ("Recent gate kills", egui::Color32::from_rgb(0xFF, 0xD5, 0x4F))
                }
            };
            let over = (c.span / 60).max(0);
            ui.label(
                egui::RichText::new(format!(
                    "{}  {label}: {} kills over {over}m, last {mins}m ago",
                    egui_phosphor::regular::CAMPFIRE,
                    c.kills,
                ))
                .strong()
                .color(col),
            );
        }
    }

    pub(crate) fn map_system_tooltip(&self, ui: &mut egui::Ui, id: i64) {
        ui.set_max_width(270.0);
        let status = self.system_status.lock().unwrap();
        let flags = status.get(&id).cloned().unwrap_or_default();
        if let Some(info) = self.systems.as_ref().and_then(|g| g.info_of(id)) {
            ui.horizontal(|ui| {
                ui.label(security_badge(info.security));
                ui.label(egui::RichText::new(&info.name).strong());
                if let Some(aid) = flags.sov_alliance {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let url = eve_alliance_logo_url(aid, 26.0);
                        let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(26.0)));
                        if let Some(sov) = &flags.sov {
                            r.on_hover_text(sov);
                        }
                    });
                }
            });
        }
        system_chips_ex(ui, &self.systems, &status, id, true, false);
        if let Some(n) = NoteTip::of(&self.notes_view, self.notes_view.system(id)) {
            note_tip_ui(ui, &n);
        }
        if let Some(f) = status.get(&id) {
            if f.jumps + f.ship_kills + f.pod_kills + f.npc_kills > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "Last hour — {} jumps · {} ship · {} pod · {} NPC kills",
                        f.jumps, f.ship_kills, f.pod_kills, f.npc_kills
                    ))
                    .weak(),
                );
            }
        }
        drop(status);
        self.camp_line(ui, id);

        let now = chrono::Utc::now().timestamp();
        let state = self.intel_state.lock().unwrap();
        let green = crate::theme::chip::CLEAR;
        let mut shown = 0;
        for r in state.reports.iter().rev() {
            if !r.systems.iter().any(|s| s.id == id) {
                continue;
            }
            if shown == 0 {
                ui.separator();
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:>6}", fmt_age((now - r.received).max(0))))
                        .monospace()
                        .weak(),
                );
                if let Some(n) = r.count {
                    ui.label(egui::RichText::new(format!("{n}x")).strong());
                }
                if r.clear {
                    ui.label(egui::RichText::new("CLEAR").color(green));
                }
                for sh in &r.ships {
                    ui.label(egui::RichText::new(&sh.name).weak());
                }
                ui.label(egui::RichText::new(format!("- {}", r.reporter)).weak());
            });
            shown += 1;
            if shown >= 4 {
                ui.label(egui::RichText::new("…").weak());
                break;
            }
        }
        if shown == 0 {
            ui.label(egui::RichText::new("Click for details.").weak());
        }
        drop(state);
        self.wormhole_section(ui, id);
    }

    pub(crate) fn map_overlay_controls(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        use egui_phosphor::regular as icon;
        egui::Area::new(ui.id().with("map_overlay_bar"))
            .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.map_overlay_locked {
                            if ui.button(icon::LOCK).on_hover_text("Unlock").clicked() {
                                self.map_overlay_locked = false;
                            }
                            if ui
                                .add(egui::Button::new(icon::CROSSHAIR).selected(self.map_follow))
                                .on_hover_text("Follow active character")
                                .clicked()
                            {
                                self.map_follow = !self.map_follow;
                            }
                            return;
                        }
                        if ui.button(icon::FRAME_CORNERS).on_hover_text("Exit overlay mode").clicked() {
                            self.map_overlay_mode = false;
                        }
                        if ui.button(icon::LOCK_OPEN).on_hover_text("Lock (no move/resize)").clicked() {
                            self.map_overlay_locked = true;
                        }
                        if ui
                            .add(egui::Button::new(icon::CROSSHAIR).selected(self.map_follow))
                            .on_hover_text("Follow active character")
                            .clicked()
                        {
                            self.map_follow = !self.map_follow;
                        }
                        if ui
                            .add(egui::Button::new(icon::CPU).selected(self.settings.map_overlay_smart))
                            .on_hover_text("Smart on-top (above only while EVE is active)")
                            .clicked()
                        {
                            self.settings.map_overlay_smart = !self.settings.map_overlay_smart;
                            self.needs_save = true;
                        }
                        ui.label("Opacity");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.settings.map_overlay_opacity, 0.2..=1.0)
                                    .show_value(false),
                            )
                            .changed()
                        {
                            self.needs_save = true;
                        }
                    });
                });
            });
    }
}
