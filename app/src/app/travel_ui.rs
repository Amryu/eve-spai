//! Map modes and the travel planner: saved travel routes, in-game destinations and route planning.

use super::*;

impl SpaiApp {
    pub(crate) fn set_map_mode(&mut self, new: MapMode) {
        if new == self.map_mode {
            return;
        }
        let keeps_layers = |m: MapMode| m == MapMode::Standard;
        if keeps_layers(self.map_mode) {
            self.standard_overlays = self.map_overlays;
        }
        self.map_overlays = if keeps_layers(new) {
            self.standard_overlays
        } else {
            new.overlay_preset()
        };
        if new == MapMode::Safety {
            if !self.map_layout.is_threat() {
                self.safety_prev_layout = Some(self.map_layout);
                self.map_layout = crate::map::MapLayout::Tree;
            }
        } else if self.map_mode == MapMode::Safety {
            if let Some(prev) = self.safety_prev_layout.take() {
                self.map_layout = prev;
            }
        }
        self.map_mode = new;
        if new != MapMode::Standard {
            self.right_dock_open = true;
            self.right_dock_tab = RightDockTab::Mode;
        }
        self.needs_save = true;
    }

    pub(crate) fn load_route(&mut self, r: &crate::settings::SavedRoute) {
        let nm = |id: i64| {
            self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default()
        };
        let (s, e) = (nm(r.start), nm(r.end));
        self.travel_start = Some(r.start);
        self.travel_end = Some(r.end);
        self.travel_start_q = s;
        self.travel_end_q = e;
        self.travel_waypoints = r.waypoints.clone();
        if let Some(c) = &r.constraints {
            self.travel_sec = c.sec;
            self.travel_metric = ActivityMode::from_u8(c.metric);
            self.travel_regional_gates = c.regional_gates;
            self.travel_jump_bridges = c.jump_bridges;
            self.travel_avoid_camps = c.avoid_camps;
            self.travel_avoid = c.avoid.clone();
            self.travel_avoid_sov = c.avoid_sov.iter().cloned().collect();
        }
        self.plan_route();
    }

    pub(crate) fn routes_dialog(&mut self, ctx: &egui::Context) {
        if !self.routes_dialog_open {
            return;
        }
        let geo = self.systems.clone();
        let nm = |id: i64| {
            geo.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_else(|| "?".into())
        };
        let mut items: Vec<RouteItem> = Vec::new();
        for r in &self.settings.saved_routes {
            items.push(RouteItem {
                name: r.name.clone(),
                folder: r.folder.clone(),
                from: r.start,
                to: r.end,
                jumps: r.jumps,
                wp: r.waypoints.len(),
            });
        }
        let mut folders: Vec<String> = self.settings.route_folders.clone();
        for it in &items {
            if !it.folder.is_empty() && !folders.contains(&it.folder) {
                folders.push(it.folder.clone());
            }
        }
        folders.sort();
        folders.dedup();

        let q = self.route_search.trim().to_lowercase();
        let can_save = self.travel_start.is_some() && self.travel_end.is_some();
        let view = self.route_view;
        let editing = self.route_edit.clone();
        let mut edit_name = self.route_edit_name.clone();
        let mut edit_folder = self.route_edit_folder.clone();

        let mut do_save = false;
        let mut new_folder = false;
        let mut to_load: Option<RouteItem> = None;
        let mut to_delete: Option<RouteItem> = None;
        let mut start_edit: Option<RouteItem> = None;
        let mut commit_edit = false;
        let mut cancel_edit = false;
        let mut open = true;

        egui::Window::new("Routes")
            .open(&mut open)
            .default_width(520.0)
            .default_height(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.route_save_name)
                            .desired_width(130.0)
                            .hint_text("Name"),
                    );
                    egui::ComboBox::from_id_salt("route_save_folder")
                        .selected_text(if self.route_save_folder.is_empty() {
                            "(root)".to_owned()
                        } else {
                            self.route_save_folder.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.menu_value(&mut self.route_save_folder, String::new(), "(root)");
                            for f in &folders {
                                ui.menu_value(&mut self.route_save_folder, f.clone(), f);
                            }
                        });
                    if ui
                        .add_enabled(can_save, egui::Button::new("Save current route"))
                        .on_hover_text(if can_save {
                            "Save the current route (blank name = auto)"
                        } else {
                            "Plan a route first"
                        })
                        .clicked()
                    {
                        do_save = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.route_new_folder)
                            .desired_width(150.0)
                            .hint_text("New folder"),
                    );
                    if ui
                        .add_enabled(!self.route_new_folder.trim().is_empty(), egui::Button::new("Add folder"))
                        .clicked()
                    {
                        new_folder = true;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("View");
                    ui.menu_value(&mut self.route_view, RouteView::ByName, "By Name");
                    ui.menu_value(&mut self.route_view, RouteView::ByFolder, "By Folder");
                    ui.menu_value(&mut self.route_view, RouteView::BySystem, "By System");
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.route_search)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search routes"),
                );
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let visible: Vec<&RouteItem> =
                        items.iter().filter(|it| q.is_empty() || it.name.to_lowercase().contains(&q)).collect();
                    let mut emit = |ui: &mut egui::Ui, it: &RouteItem| {
                        let is_ed = editing
                            .as_ref()
                            .is_some_and(|(f, n)| *f == it.folder && *n == it.name);
                        match route_item_row(
                            ui,
                            it,
                            &nm(it.from),
                            &nm(it.to),
                            is_ed,
                            &mut edit_name,
                            &mut edit_folder,
                            &folders,
                        ) {
                            RowAction::Load => to_load = Some(it.clone()),
                            RowAction::Delete => to_delete = Some(it.clone()),
                            RowAction::Edit => start_edit = Some(it.clone()),
                            RowAction::Commit => commit_edit = true,
                            RowAction::Cancel => cancel_edit = true,
                            RowAction::None => {}
                        }
                    };
                    if visible.is_empty() {
                        ui.label(egui::RichText::new("No saved routes yet.").weak());
                    }
                    match view {
                        RouteView::ByName => {
                            let mut v = visible.clone();
                            v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                            for it in v {
                                emit(ui, it);
                            }
                        }
                        RouteView::BySystem => {
                            let mut froms: Vec<i64> = visible.iter().map(|it| it.from).collect();
                            froms.sort();
                            froms.dedup();
                            for sys in froms {
                                let label = nm(sys);
                                egui::CollapsingHeader::new(label).default_open(true).show(ui, |ui| {
                                    for it in visible.iter().filter(|it| it.from == sys) {
                                        emit(ui, it);
                                    }
                                });
                            }
                        }
                        RouteView::ByFolder => {
                            for it in visible.iter().filter(|it| it.folder.is_empty()) {
                                emit(ui, it);
                            }
                            for f in &folders {
                                let in_f: Vec<&&RouteItem> = visible.iter().filter(|it| &it.folder == f).collect();
                                if in_f.is_empty() {
                                    continue;
                                }
                                egui::CollapsingHeader::new(format!("{}  {f}", egui_phosphor::regular::FOLDER))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        for it in in_f {
                                            emit(ui, it);
                                        }
                                    });
                            }
                        }
                    }
                });
            });

        self.route_edit_name = edit_name;
        self.route_edit_folder = edit_folder;

        if do_save {
            self.save_current_route();
        }
        if new_folder {
            let f = self.route_new_folder.trim().to_owned();
            if !self.settings.route_folders.contains(&f) {
                self.settings.route_folders.push(f);
            }
            self.route_new_folder.clear();
            self.needs_save = true;
        }
        if let Some(it) = to_delete {
            self.settings.saved_routes.retain(|r| !(r.folder == it.folder && r.name == it.name));
            self.needs_save = true;
        }
        if let Some(it) = start_edit {
            self.route_edit = Some((it.folder.clone(), it.name.clone()));
            self.route_edit_name = it.name;
            self.route_edit_folder = it.folder;
        }
        if cancel_edit {
            self.route_edit = None;
        }
        if commit_edit {
            if let Some((of, on)) = self.route_edit.take() {
                let (nn, nf) = (self.route_edit_name.trim().to_owned(), self.route_edit_folder.clone());
                if !nn.is_empty() {
                    if let Some(r) = self.settings.saved_routes.iter_mut().find(|r| r.folder == of && r.name == on) {
                        r.name = nn;
                        r.folder = nf;
                    }
                    self.needs_save = true;
                }
            }
        }
        if let Some(it) = to_load {
            self.load_route_item(&it);
            self.routes_dialog_open = false;
        }
        if !open {
            self.routes_dialog_open = false;
        }
    }

    pub(crate) fn save_current_route(&mut self) {
        let nm = |id: i64| {
            self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default()
        };
        if self.travel_start.is_none() || self.travel_end.is_none() {
            return;
        }
        let name = if self.route_save_name.trim().is_empty() {
            let mut parts = vec![nm(self.travel_start.unwrap_or(0))];
            parts.extend(self.travel_waypoints.iter().map(|w| nm(*w)));
            parts.push(nm(self.travel_end.unwrap_or(0)));
            parts.join(" \u{2192} ")
        } else {
            self.route_save_name.trim().to_owned()
        };
        self.settings.saved_routes.push(crate::settings::SavedRoute {
            name,
            folder: self.route_save_folder.clone(),
            start: self.travel_start.unwrap_or(0),
            end: self.travel_end.unwrap_or(0),
            waypoints: self.travel_waypoints.clone(),
            jumps: self.travel_route.as_ref().map(|r| r.len().saturating_sub(1)).unwrap_or(0),
            constraints: Some(crate::settings::RouteConstraints {
                sec: self.travel_sec,
                metric: self.travel_metric.to_u8(),
                regional_gates: self.travel_regional_gates,
                jump_bridges: self.travel_jump_bridges,
                avoid_camps: self.travel_avoid_camps,
                avoid: self.travel_avoid.clone(),
                avoid_sov: self.travel_avoid_sov.iter().cloned().collect(),
            }),
        });
        self.route_save_name.clear();
        self.needs_save = true;
    }

    pub(crate) fn load_route_item(&mut self, it: &RouteItem) {
        if let Some(r) = self
            .settings
            .saved_routes
            .iter()
            .find(|r| r.folder == it.folder && r.name == it.name)
            .cloned()
        {
            self.load_route(&r);
            self.set_map_mode(MapMode::Travel);
        }
    }

    pub(crate) fn next_route_hop(&self) -> Option<i64> {
        let route = self.travel_route.as_ref()?;
        if route.len() < 2 {
            return None;
        }
        if let Some(me) = self.player_system() {
            if let Some(i) = route.iter().position(|&s| s == me) {
                return route.get(i + 1).copied();
            }
        }
        route.get(1).copied()
    }

    pub(crate) fn push_ingame_dest(&mut self) {
        let next = self.next_route_hop();
        if next == self.travel_ingame_dest {
            return;
        }
        if let Some(d) = next {
            let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
            self.set_destination_esi(cid, self.active_character.clone(), d);
        }
        self.travel_ingame_dest = next;
    }

    /// Set the travel route's start. The destination is typed into the panel.
    pub(crate) fn travel_set_start(&mut self, id: i64) {
        let name = self
            .systems
            .as_ref()
            .and_then(|g| g.info_of(id))
            .map(|i| i.name.clone())
            .unwrap_or_default();
        self.travel_start = Some(id);
        self.travel_start_q = name;
        self.travel_waypoints.retain(|&w| w != id);
        self.travel_avoid.retain(|&a| a != id);
        self.plan_route();
    }

    pub(crate) fn clear_travel(&mut self) {
        self.travel_start = None;
        self.travel_end = None;
        self.travel_start_q.clear();
        self.travel_end_q.clear();
        self.travel_waypoints.clear();
        self.travel_avoid.clear();
        self.travel_route = None;
        self.travel_direct_route = None;
    }

    pub(crate) fn travel_input_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.travel_start_q.hash(&mut h);
        self.travel_end_q.hash(&mut h);
        self.travel_start.hash(&mut h);
        self.travel_end.hash(&mut h);
        self.travel_waypoints.hash(&mut h);
        self.travel_avoid.hash(&mut h);
        let mut sov: Vec<&String> = self.travel_avoid_sov.iter().collect();
        sov.sort();
        sov.hash(&mut h);
        self.travel_regional_gates.hash(&mut h);
        self.travel_jump_bridges.hash(&mut h);
        self.travel_avoid_camps.hash(&mut h);
        self.travel_sec.hash(&mut h);
        self.travel_max_ship_kills.hash(&mut h);
        (self.travel_metric as u8).hash(&mut h);
        h.finish()
    }

    pub(crate) fn plan_route(&mut self) {
        let Some(geo) = self.systems.clone() else { return };
        if let Some(store) = self.store.as_ref() {
            if !self.travel_start_q.trim().is_empty() {
                self.travel_start =
                    store.search_systems(&self.travel_start_q, 1).first().map(|(id, _, _)| *id);
            }
            if !self.travel_end_q.trim().is_empty() {
                self.travel_end =
                    store.search_systems(&self.travel_end_q, 1).first().map(|(id, _, _)| *id);
            }
        }
        let (Some(s), Some(e)) = (self.travel_start, self.travel_end) else {
            self.travel_route = None;
            self.travel_direct_route = None;
            self.travel_planned_hash = self.travel_input_hash();
            self.travel_dirty_at = None;
            return;
        };
        let mut points = vec![s];
        points.extend(self.travel_waypoints.iter().copied());
        points.push(e);
        let status = self.system_status.lock().unwrap();
        let max_kills = self.travel_max_ship_kills;
        let metric = self.travel_metric;
        let sec = self.travel_sec;
        let avoid = self.travel_avoid.clone();
        let avoid_sov: std::collections::HashSet<String> =
            self.travel_avoid_sov.iter().map(|s| s.to_lowercase()).collect();
        let camped: std::collections::HashSet<i64> = if self.travel_avoid_camps {
            let now = chrono::Utc::now().timestamp();
            self.camps
                .lock()
                .unwrap()
                .camped(now)
                .into_iter()
                .filter(|(_, l)| *l >= crate::camp::CampLevel::Possible)
                .map(|(id, _)| id)
                .collect()
        } else {
            std::collections::HashSet::new()
        };
        let regional = self.travel_regional_gates;
        let bridges = self.travel_jump_bridges;
        let geo2 = geo.clone();
        let allowed = |sys: i64| {
            if avoid.contains(&sys) || camped.contains(&sys) {
                return false;
            }
            if !avoid_sov.is_empty() {
                if let Some(h) = status.get(&sys).and_then(|f| f.sov.as_deref()) {
                    if avoid_sov.contains(&h.to_lowercase()) {
                        return false;
                    }
                }
            }
            // J-space has no security band to prefer, so the hisec/lowsec/null switches don't apply:
            // filtering Thera out as "null" would silently defeat routing through it.
            let sec_ok = crate::geo::is_wormhole_system(sys)
                || geo2
                    .info_of(sys)
                    .map(|i| {
                        if i.security >= 0.45 {
                            sec[0]
                        } else if i.security > 0.0 {
                            sec[1]
                        } else {
                            sec[2]
                        }
                    })
                    .unwrap_or(true);
            let activity_ok =
                max_kills == 0 || status.get(&sys).map(|f| metric.value(f)).unwrap_or(0) <= max_kills;
            sec_ok && activity_ok
        };
        let holes = if self.settings.route_via_wormholes {
            self.wh_adjacency()
        } else {
            std::collections::HashMap::new()
        };
        let mut route = vec![s];
        let mut ok = true;
        for leg in points.windows(2) {
            match geo.route_with(leg[0], leg[1], regional, bridges, &holes, allowed) {
                Some(seg) => route.extend(seg.into_iter().skip(1)),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        let prev = self.travel_route.clone();
        self.travel_route = ok.then_some(route);
        self.travel_direct_route = geo.route(s, e, true, false, |_| true);
        if self.travel_live {
            if let (Some(p), Some(n)) = (&prev, &self.travel_route) {
                if p != n {
                    let pset: std::collections::HashSet<i64> = p.iter().copied().collect();
                    let newsys: Vec<i64> = n.iter().copied().filter(|s| !pset.contains(s)).collect();
                    if !newsys.is_empty() {
                        let much_longer = n.len() > p.len() + 4;
                        self.travel_changed = newsys;
                        self.travel_changed_at = Some(chrono::Utc::now().timestamp());
                        if much_longer {
                            crate::sound::play_prio(&self.settings.sound_reroute, 2, self.settings.sound_reroute_volume);
                        }
                    }
                }
            }
        }
        self.travel_planned_hash = self.travel_input_hash();
        self.travel_dirty_at = None;
    }

    pub(crate) fn travel_suggestions(&self, q: &str) -> Vec<SysHit> {
        let q = q.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let Some(store) = self.store.as_ref() else { return Vec::new() };
        store
            .search_systems(q, 8)
            .into_iter()
            .map(|(id, name, sec)| {
                let (c, r) = self
                    .systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| (i.constellation.clone(), i.region.clone()))
                    .unwrap_or_default();
                (id, name, sec, c, r)
            })
            .collect()
    }
}
