//! Wormhole connections and the wormholes view, with the kill history they depend on.

use super::*;

impl SpaiApp {
    pub(crate) fn ensure_ship_by_id(&mut self) {
        if self.ship_by_id.is_empty() {
            if let Some(idx) = &self.ship_index {
                for (id, name) in idx.values() {
                    self.ship_by_id.insert(*id, name.clone());
                }
            }
        }
    }

    pub(crate) fn load_persisted_kills(&mut self) {
        if self.kills_loaded {
            return;
        }
        let Some(geo) = self.systems.clone() else { return };
        self.ensure_ship_by_id();
        if self.ship_by_id.is_empty() {
            return;
        }
        self.kills_loaded = true;
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - 3600;
        let (saved, details) = {
            let Some(store) = &self.store else { return };
            let rows = store.load_kill_intel(cutoff);
            store.prune_kill_intel(cutoff);
            let details = store.load_kill_details();
            (rows, details)
        };
        if !details.is_empty() {
            let mut c = self.kill_cache.lock().unwrap();
            for k in details {
                let id = k.kill_id;
                c.entry(id).or_insert(Some(k));
            }
        }
        if saved.is_empty() {
            return;
        }
        let mut reports = Vec::new();
        for (killmail_id, system_id, ship_type_id, time, value) in saved {
            let near_celestial = self
                .kill_cache
                .lock()
                .unwrap()
                .get(&killmail_id)
                .and_then(|o| o.as_ref())
                .and_then(|k| k.near_celestial.clone());
            let ev = crate::zkill::KillEvent {
                system_id,
                ship_type_id,
                time,
                value,
                killmail_id,
                info: crate::kills::KillInfo { near_celestial, ..Default::default() },
            };
            if let Some(report) = kill_report(&ev, &geo, &self.ship_by_id) {
                reports.push(report);
            }
        }
        let ids: Vec<u64> = {
            let mut st = self.intel_state.lock().unwrap();
            reports.into_iter().map(|report| st.push(report)).collect()
        };
        // Historical kills, not live events. Pre-mark them alerted so the recency gate in the
        // alert daemon doesn't pop them into the alert window at startup.
        let now = chrono::Utc::now().timestamp();
        {
            let mut rt = self.alerts_engine.runtime.lock().unwrap();
            for id in ids {
                rt.alerted.insert(id, now);
            }
        }
    }

    pub(crate) fn reload_wormholes(&mut self) {
        let due = self.wh_reloaded.map(|t| t.elapsed().as_millis() > 2000).unwrap_or(true);
        if !due {
            return;
        }
        self.wh_reloaded = Some(std::time::Instant::now());
        if let Some(store) = &self.store {
            let now = chrono::Utc::now().timestamp();
            store.prune_wormholes(now);
            let mut whs = store.wormholes();
            whs.retain(|w| !w.is_expired(now));
            whs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            self.wh_overlay = WhOverlay::build(&whs);
            self.wh_cache = whs;
            self.wh_group_of = store.wormhole_groups();
        }
    }

    /// The live hole graph. Built from `wh_cache`, not `wh_overlay`: the overlay drops high-degree
    /// hubs to keep the map readable, which is exactly what would remove Thera from a route.
    /// The holes routes may use: known far side, and a kind the routing settings allow.
    pub(crate) fn wh_adjacency(&self) -> std::collections::HashMap<i64, Vec<i64>> {
        let mut adj: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        let allowed = |w: &crate::wormholes::Wormhole, b: i64| {
            self.systems.as_ref().is_none_or(|g| {
                let kind = crate::wormholes::HoleKind::of(g, w.system_id, b, w.is_drifter);
                self.settings.wh_route_kinds.iter().any(|k| k == kind.code())
            })
        };
        for w in &self.wh_cache {
            if let Some(b) = w.dest_system_id.filter(|b| allowed(w, *b)) {
                adj.entry(w.system_id).or_default().push(b);
                adj.entry(b).or_default().push(w.system_id);
            }
        }
        adj
    }

    /// Toggles for the kinds of hole routes may use. Returns whether any changed.
    pub(crate) fn wh_route_kinds_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::app::SteadySelect as _;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Through:").weak());
            for k in crate::wormholes::HoleKind::ALL {
                let on = self.settings.wh_route_kinds.iter().any(|c| c == k.code());
                if ui.menu_label(on, k.label()).clicked() {
                    if on {
                        self.settings.wh_route_kinds.retain(|c| c != k.code());
                    } else {
                        self.settings.wh_route_kinds.push(k.code().to_owned());
                    }
                    changed = true;
                }
            }
        });
        changed
    }

    pub(crate) fn wh_route_waypoints(&self, from: i64, dest: i64) -> Option<Vec<i64>> {
        let geo = self.systems.as_ref()?;
        wh_route_waypoints(geo, &self.wh_adjacency(), from, dest)
    }

    /// The hole collapsed. Drop it, then re-route: any plan that was going through it is now wrong.
    pub(crate) fn kill_wormhole(&mut self, id: i64) {
        if let Some(store) = self.store.as_ref() {
            store.kill_wormhole(id);
        }
        self.wh_reloaded = None; // bypass the reload debounce, the map must not show it again
        self.reload_wormholes();
        self.replan_routes();
    }

    /// `crossed_jspace` covers the case where the step passed through systems the k-space map cannot
    /// place, which can only have happened through a hole.
    pub(crate) fn leg_kind(&self, a: i64, b: i64, crossed_jspace: bool) -> Leg {
        let Some(g) = self.systems.as_ref() else { return Leg::Gate };
        if crossed_jspace || g.is_hole_step(a, b) {
            Leg::Hole
        } else if g.is_bridge(a, b) {
            Leg::Bridge
        } else {
            Leg::Gate
        }
    }

    /// Re-run every route that could depend on the hole graph: the planned map route, and the
    /// destination we last pushed to the client.
    pub(crate) fn replan_routes(&mut self) {
        self.plan_route();
        if let Some(dest) = self.route_destination {
            if self.active_character != "No character" {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                self.set_destination_esi(cid, self.active_character.clone(), dest);
            }
        }
    }

    /// Push the rescue capital's system to the active character as an ESI autopilot destination.
    /// No-op without an active character.
    #[cfg(feature = "fleet")]
    pub(crate) fn rescue_push_destination(&mut self, sid: i64) {
        if self.active_character == "No character" {
            return;
        }
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        self.set_destination_esi(cid, self.active_character.clone(), sid);
    }

    pub(crate) fn set_destination_esi(&self, cid: String, cname: String, dest: i64) {
        if self.settings.route_via_wormholes {
            let from = self.player_system();
            if let Some(from) = from {
                if let Some(wp) = self.wh_route_waypoints(from, dest) {
                    if wp.len() > 1 {
                        crate::esi::set_route(cid, cname, wp);
                        return;
                    }
                }
            }
        }
        if self.force_ansiblex_route(&cid, &cname, dest) {
            return;
        }
        crate::esi::set_waypoint(cid, cname, dest, true);
    }

    /// The client's route planner ignores Ansiblex zones, so a route that avoids the costly ones
    /// is pinned with waypoints. Returns whether it took over; the work happens off the UI thread,
    /// since it walks the whole map a few times.
    fn force_ansiblex_route(&self, cid: &str, cname: &str, dest: i64) -> bool {
        let bridges = self.settings.jump_bridges.clone();
        if bridges.is_empty() {
            return false;
        }
        let (Some(from), Some(graph)) = (self.player_system(), self.systems.clone()) else {
            return false;
        };
        if from == dest {
            return false;
        }
        let (capital, max_zone) =
            (self.settings.ansiblex_capital.clone(), self.settings.ansiblex_max_zone);
        let (cid, cname) = (cid.to_owned(), cname.to_owned());
        let _ = std::thread::Builder::new().name("route-force".into()).spawn(move || {
            let holes = std::collections::HashMap::new();
            let Some(path) = graph.route_with(from, dest, true, true, &holes, |_| true) else {
                crate::esi::set_waypoint(cid, cname, dest, true);
                return;
            };
            let game = crate::ansiblex::game_graph(&graph, &bridges);
            let permitted: std::collections::HashSet<(i64, i64)> =
                crate::ansiblex::permitted_edges(&bridges, &game, &capital, max_zone).into_iter().collect();
            let ok = |a: i64, b: i64| !game.is_bridge(a, b) || permitted.contains(&(a, b));
            let wp = crate::routeforce::waypoints(&path, &game, &ok);
            match wp.len() {
                0 => crate::esi::set_waypoint(cid, cname, dest, true),
                1 => crate::esi::set_waypoint(cid, cname, dest, true),
                _ => crate::esi::set_route(cid, cname, wp),
            }
        });
        true
    }

    /// Remembers that the game now holds a route this app set.
    pub(crate) fn note_ingame_route(&mut self) {
        self.ingame_route = true;
    }

    pub(crate) fn wormholes_view(&mut self, ui: &mut egui::Ui) {
        use crate::app::SteadySelect as _;
        use egui_phosphor::regular as icon;
        // Without clone locations a death or a clone jump cannot be told from a hole.
        let missing: Vec<&str> = self
            .characters
            .iter()
            .filter(|c| {
                let has = |scope: &str| c.scopes.split_whitespace().any(|s| s == scope);
                !has(crate::esi::CLONES_SCOPE) || !has(crate::esi::FATIGUE_SCOPE)
            })
            .map(|c| c.name.as_str())
            .collect();
        let missing = (self.settings.wh_detect && !missing.is_empty()).then(|| missing.join(", "));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            use crate::wormholes::{DestClass, Source};
            ui.heading(format!("{}  Wormholes", icon::SPIRAL));
            ui.label(egui::RichText::new(format!("{} known", self.wh_cache.len())).weak());
            ui.add_space(8.0);
            if ui.menu_label(!self.wh_graph.table, format!("{}  Map", icon::GRAPH)).clicked() {
                self.wh_graph.table = false;
            }
            if ui.menu_label(self.wh_graph.table, format!("{}  Table", icon::TABLE)).clicked() {
                self.wh_graph.table = true;
            }
            ui.add_space(8.0);
            if ui.button(format!("{}  Add", icon::PLUS)).on_hover_text("Enter a hole by hand").clicked() {
                self.wh_form = Some(WhForm::default());
            }
            ui.add_space(8.0);
            // A combo box does not wrap on its own.
            if ui.max_rect().right() - ui.cursor().min.x < 130.0 + 8.0 {
                ui.end_row();
            }
            egui::ComboBox::from_id_salt("wh_dest_filter")
                .width(130.0)
                .selected_text(format!("Dest: {}", self.wh_filter_dest.map_or("Any", |d| d.label())))
                .show_ui(ui, |ui| {
                    ui.menu_value(&mut self.wh_filter_dest, None, "Any");
                    for d in [
                        DestClass::Highsec,
                        DestClass::Lowsec,
                        DestClass::Nullsec,
                        DestClass::Wspace,
                        DestClass::Thera,
                        DestClass::Turnur,
                        DestClass::Unknown,
                    ] {
                        ui.menu_value(&mut self.wh_filter_dest, Some(d), d.label());
                    }
                });
            // A combo box does not wrap on its own.
            if ui.max_rect().right() - ui.cursor().min.x < 150.0 + 8.0 {
                ui.end_row();
            }
            egui::ComboBox::from_id_salt("wh_src_filter")
                .width(150.0)
                .selected_text(format!("Source: {}", self.wh_filter_source.map_or("Any", |s| s.label())))
                .show_ui(ui, |ui| {
                    ui.menu_value(&mut self.wh_filter_source, None, "Any");
                    for s in Source::ALL {
                        ui.menu_value(&mut self.wh_filter_source, Some(s), s.label());
                    }
                });
            ui.checkbox(&mut self.wh_filter_expiring, "Expiring <4h");
            ui.add_space(8.0);
            let look = ui.add(
                egui::TextEdit::singleline(&mut self.wh_info_query).hint_text("System facts: J-name or system").desired_width(200.0),
            );
            if look.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.wh_info = self.systems.as_ref().and_then(|g| g.lookup(self.wh_info_query.trim())).map(|i| i.id);
            }
            let mut changed = false;
            ui.menu_button(icon::GEAR_SIX, |ui| {
                changed |= ui
                    .checkbox(&mut self.settings.wh_detect, "Record holes my characters go through")
                    .on_hover_text("Watches how your signed-in characters move. A jump the gates cannot explain is recorded as a wormhole, or asked about when a filament, clone or capital jump fits too.")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.wh_ask, "Ask for the signature")
                    .on_hover_text("A small window beside the EVE client asks for the signature, type and state of each hole")
                    .changed();
                ui.separator();
                if ui.button(format!("{}  Sharing…", icon::USERS_THREE)).on_hover_text("Share wormholes with others, end-to-end encrypted").clicked() {
                    self.wh_share.open = true;
                    ui.close();
                }
                if ui
                    .button(format!("{}  Reset map layout", icon::ARROWS_CLOCKWISE))
                    .on_hover_text("Forget where systems were dragged and lay the map out again")
                    .clicked()
                {
                    self.wh_graph_reset_layout();
                    ui.close();
                }
            })
            .response
            .on_hover_text("Wormhole settings");
            if changed {
                self.needs_save = true;
            }
            if let Some((text, color, hover)) = self.share_status_line() {
                // A fixed width, so "Syncing" and "Synced 2m ago" wrap the row the same way.
                let size = egui::vec2(160.0, ui.spacing().interact_size.y);
                let r = ui
                    .allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_size(size);
                        ui.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate().sense(egui::Sense::click()))
                    })
                    .inner;
                if r.on_hover_text(format!("{hover}\nClick for the sharing window")).clicked() {
                    self.wh_share.open = true;
                }
            }
            if let Some(names) = &missing {
                ui.label(egui::RichText::new(icon::WARNING).color(crate::theme::standing::WARNING)).on_hover_text(format!(
                    "Sign in again with {names} to let deaths, clone jumps and bridges be told from wormholes."
                ));
            }
        });
        self.wh_form_window(ui.ctx());
        if let Some(sys) = self.wh_info {
            let mut close = false;
            egui::Panel::right("wh_info_panel").resizable(true).default_size(340.0).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    close = self.wh_info_panel(ui, sys);
                });
            });
            if close {
                self.wh_info = None;
            }
        }
        ui.separator();
        if self.wh_cache.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No wormholes known yet.").weak());
                ui.label(
                    egui::RichText::new(
                        "Seeded from EVE-Scout (Thera/Turnur), intel channels, your own characters' jumps and what you add.",
                    )
                    .weak(),
                );
            });
            return;
        }

        if !self.wh_graph.table {
            self.wh_graph_view(ui);
            return;
        }
        let now = chrono::Utc::now().timestamp();
        struct Row {
            id: i64,
            sys_id: i64,
            sys: String,
            wh_type: String,
            drifter: bool,
            dest: String,
            dest_click: Option<i64>,
            dest_const: String,
            dest_region: String,
            size: String,
            life: String,
            source: String,
        }
        let info_of = |id: i64| self.systems.as_ref().and_then(|s| s.info_of(id)).cloned();
        let (fd, fs, fe) = (self.wh_filter_dest, self.wh_filter_source, self.wh_filter_expiring);
        let rows: Vec<Row> = self
            .wh_cache
            .iter()
            .filter(|w| {
                fd.map_or(true, |d| w.dest == d)
                    && fs.map_or(true, |s| w.source == s)
                    && (!fe || w.hours_left(now).is_some_and(|h| h <= 4))
            })
            .map(|w| {
                let mut sys = info_of(w.system_id)
                    .map(|i| i.name)
                    .unwrap_or_else(|| format!("#{}", w.system_id));
                if let Some(sig) = &w.signature {
                    sys = format!("{sys}  [{sig}]");
                }
                let (dest, dest_const, dest_region) = match w.dest_system_id.and_then(info_of) {
                    Some(i) => (i.name, i.constellation, i.region),
                    None => (w.dest.label().to_string(), String::new(), String::new()),
                };
                let seen = |at: Option<i64>| at.map(|t| format!(", seen {} ago", human_ago(now - t))).unwrap_or_default();
                let life = if let Some(l) = w.life {
                    format!("{}{}", l.label(), seen(w.observed_at))
                } else if w.explicit_expiry.is_some() {
                    match w.hours_left(now) {
                        Some(h) => format!("< {h}h left"),
                        None => "expired".into(),
                    }
                } else {
                    format!("reported {} ago", human_ago(now - w.reported_at))
                };
                // The entry's origin first, then whoever else has seen it.
                let mut source = match (&w.detected_by, w.source) {
                    (Some(who), crate::wormholes::Source::Auto) => format!("{} ({who})", w.source.label()),
                    _ => w.source.label().to_string(),
                };
                let also: Vec<&str> = crate::wormholes::Source::ALL
                    .into_iter()
                    .filter(|s| *s != w.source && w.seen_by & s.bit() != 0)
                    .map(|s| s.label())
                    .collect();
                if !also.is_empty() {
                    source.push_str(&format!(", also {}", also.join(", ")));
                }
                if let Some(name) = self.wh_group_of.get(&w.uid).and_then(|g| self.share_group_name(g)) {
                    source.push_str(&format!(", in {name}"));
                }
                Row {
                    id: w.id,
                    sys_id: w.system_id,
                    sys,
                    wh_type: w.wh_type.clone().unwrap_or_else(|| "—".into()),
                    drifter: w.is_drifter,
                    dest,
                    dest_click: w.dest_system_id,
                    dest_const,
                    dest_region,
                    size: {
                        let size = w.effective_size().map(|s| s.label().to_string()).unwrap_or_else(|| "—".into());
                        match w.mass {
                            Some(m) => format!("{size}, mass {}", m.label().to_lowercase()),
                            None => size,
                        }
                    },
                    life,
                    source,
                }
            })
            .collect();

        let mut kill: Option<i64> = None;
        let mut edit: Option<i64> = None;
        let mut info: Option<i64> = None;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("wh_grid").striped(true).num_columns(8).spacing([16.0, 6.0]).show(
                ui,
                |ui| {
                    for h in
                        ["System", "Type", "Destination", "Constellation", "Region", "Size", "Life", "Source"]
                    {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for r in &rows {
                        // In the first column, not the last: this grid is eight columns and scrolls
                        // sideways, so anything at the far end is off the screen exactly when the
                        // window is small enough to need it.
                        ui.horizontal(|ui| {
                            if ui
                                .small_button(icon::X)
                                .on_hover_text("Mark this hole dead")
                                .clicked()
                            {
                                kill = Some(r.id);
                            }
                            if ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Edit this hole").clicked() {
                                edit = Some(r.id);
                            }
                            if ui.small_button(icon::INFO).on_hover_text("Wormhole facts about this system").clicked() {
                                info = Some(r.sys_id);
                            }
                            if ui.link(&r.sys).clicked() {
                                self.open_system(r.sys_id);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(&r.wh_type);
                            if r.drifter {
                                ui.label(
                                    egui::RichText::new(format!("{} drifter", icon::WARNING))
                                        .color(crate::theme::standing::WARNING),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon::ARROW_RIGHT).weak());
                            if let Some(id) = r.dest_click {
                                if ui.link(&r.dest).clicked() {
                                    self.open_system(id);
                                }
                            } else {
                                ui.label(&r.dest);
                            }
                        });
                        ui.label(&r.dest_const);
                        ui.label(&r.dest_region);
                        ui.label(&r.size);
                        ui.label(&r.life);
                        ui.label(&r.source);
                        ui.end_row();
                    }
                },
            );
        });
        if let Some(id) = kill {
            self.kill_wormhole(id);
        }
        if let Some(id) = edit {
            self.wh_edit(id);
        }
        if info.is_some() {
            self.wh_info = info;
        }
    }

    pub(crate) fn wh_edit(&mut self, id: i64) {
        {
            self.wh_form = self.wh_cache.iter().find(|w| w.id == id).map(|w| {
                let mut f = WhForm::of(w, self.systems.as_deref());
                f.history = self
                    .store
                    .as_ref()
                    .map(|s| s.wormhole_audit(&w.uid))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(at, who, source, field, value)| {
                        let when = chrono::DateTime::from_timestamp(at, 0).map(|d| d.format("%m-%d %H:%M").to_string()).unwrap_or_default();
                        format!("{when}  {who} ({source}): {field} {value}")
                    })
                    .collect();
                f
            });
        }
    }

    /// The add/edit form. A new entry is Manual; an edited one keeps the origin it had.
    fn wh_form_window(&mut self, ctx: &egui::Context) {
        use crate::wormholes::{ShipSize, Source, Wormhole};
        let Some(mut form) = self.wh_form.take() else { return };
        let mut open = true;
        let mut save = false;
        egui::Window::new(if form.id.is_some() { "Edit wormhole" } else { "Add wormhole" })
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("wh_form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("System");
                    ui.add(egui::TextEdit::singleline(&mut form.system).desired_width(200.0));
                    ui.end_row();
                    ui.label("Signature");
                    ui.add(egui::TextEdit::singleline(&mut form.sig).hint_text("ABC-123").desired_width(200.0));
                    ui.end_row();
                    ui.label("Type");
                    let codes: Vec<&str> = crate::whdata::types().iter().map(|t| t.code.as_str()).collect();
                    if wh_type_picker(ui, "wh_form_type", 200.0, &mut form.wh_type, &codes) {
                        // A known type decides the size; it can still be corrected below.
                        let sizes = crate::wormholes::sizes_for(&[form.wh_type.as_str()]);
                        if sizes.len() == 1 {
                            form.size = Some(sizes[0]);
                        }
                    }
                    ui.end_row();
                    ui.label("Leads to");
                    ui.add(egui::TextEdit::singleline(&mut form.dest).hint_text("system, if known").desired_width(200.0));
                    ui.end_row();
                    ui.label("Its signature");
                    ui.add(egui::TextEdit::singleline(&mut form.dest_sig).hint_text("the other side").desired_width(200.0));
                    ui.end_row();
                    ui.label("Size");
                    // Every size stays open: a recorded type can be wrong, or be the other side's.
                    let sizes: Vec<_> = [ShipSize::Frigate, ShipSize::Medium, ShipSize::Large, ShipSize::XLarge]
                        .into_iter()
                        .map(|s| (s, s.short(), s.label()))
                        .collect();
                    ui.vertical(|ui| {
                        choice_row(ui, &mut form.size, &sizes);
                        let implied = crate::whdata::hole_type(&form.wh_type).filter(|t| t.jump_mass > 0);
                        if let Some(t) = implied {
                            let fits = crate::wormholes::size_for_jump_mass(t.jump_mass);
                            match form.size {
                                Some(chosen) if chosen != fits => {
                                    ui.colored_label(
                                        crate::theme::standing::WARNING,
                                        format!(
                                            "{} {} takes up to {}, not {}: check the type or the size",
                                            egui_phosphor::regular::WARNING,
                                            t.code,
                                            fits.short(),
                                            chosen.short()
                                        ),
                                    );
                                }
                                _ => {
                                    ui.label(egui::RichText::new(format!("{} takes up to {}", t.code, fits.short())).weak());
                                }
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Time left");
                    let lives: Vec<_> = crate::wormholes::Life::ALL.into_iter().map(|l| (l, l.short(), l.label())).collect();
                    choice_row(ui, &mut form.life, &lives);
                    ui.end_row();
                    ui.label("Mass left");
                    let masses: Vec<_> = crate::wormholes::Mass::ALL.into_iter().map(|m| (m, m.short(), m.label())).collect();
                    choice_row(ui, &mut form.mass, &masses);
                    ui.end_row();
                    ui.label("Note");
                    ui.add(egui::TextEdit::singleline(&mut form.note).desired_width(200.0));
                    ui.end_row();
                });
                if let Some(e) = &form.error {
                    ui.label(egui::RichText::new(e).color(crate::theme::standing::HOSTILE));
                }
                if !form.history.is_empty() {
                    egui::CollapsingHeader::new(format!("History ({})", form.history.len())).show(ui, |ui| {
                        for line in &form.history {
                            ui.label(line);
                        }
                    });
                }
                if ui.button(format!("{}  Save", egui_phosphor::regular::FLOPPY_DISK)).clicked() {
                    save = true;
                }
            });
        if save {
            let geo = self.systems.clone();
            let lookup = |name: &str| geo.as_ref().and_then(|g| g.lookup(name.trim())).map(|i| i.id);
            let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_uppercase());
            let Some(sys) = lookup(&form.system) else {
                form.error = Some(format!("No system called {:?}.", form.system.trim()));
                self.wh_form = Some(form);
                return;
            };
            let dest_id = if form.dest.trim().is_empty() { None } else { lookup(&form.dest) };
            if !form.dest.trim().is_empty() && dest_id.is_none() {
                form.error = Some(format!("No system called {:?}.", form.dest.trim()));
                self.wh_form = Some(form);
                return;
            }
            let now = chrono::Utc::now().timestamp();
            let hole = crate::whdata::hole_type(&form.wh_type);
            let dest = match (dest_id, hole.map(|h| h.dest)) {
                (Some(d), _) => geo.as_ref().map_or(crate::wormholes::DestClass::Unknown, |g| dest_class(g, d)),
                (None, Some(crate::whdata::Dest::Class(c))) => class_dest(c),
                _ => crate::wormholes::DestClass::Unknown,
            };
            let fresh = Wormhole {
                system_id: sys,
                signature: text(&form.sig),
                wh_type: text(&form.wh_type),
                dest,
                dest_system_id: dest_id,
                dest_signature: text(&form.dest_sig),
                size: form.size,
                is_drifter: matches!(hole.map(|h| h.dest), Some(crate::whdata::Dest::Class(crate::whdata::Class::Drifter(_)))),
                reported_at: now,
                explicit_expiry: form.life.and_then(|l| l.closes_by(now)),
                source: Source::Manual,
                updated_at: now,
                mass: form.mass,
                life: form.life,
                observed_at: (form.mass.is_some() || form.life.is_some()).then_some(now),
                note: (!form.note.trim().is_empty()).then(|| form.note.trim().to_owned()),
                ..Default::default()
            };
            let who = if self.settings.active_character.is_empty() { "me".to_owned() } else { self.settings.active_character.clone() };
            let changes: Vec<(&str, String)> = [
                ("signature", fresh.signature.clone()),
                ("type", fresh.wh_type.clone()),
                ("leads to", (!form.dest.trim().is_empty()).then(|| form.dest.trim().to_owned())),
                ("far signature", fresh.dest_signature.clone()),
                ("size", fresh.size.map(|s| s.label().to_owned())),
                ("time left", fresh.life.map(|l| l.label().to_owned())),
                ("mass left", fresh.mass.map(|m| m.label().to_owned())),
                ("note", fresh.note.clone()),
            ]
            .into_iter()
            .filter_map(|(f, v)| Some((f, v?)))
            .collect();
            if let Some(store) = &self.store {
                let id = match form.id.and_then(|id| store.wormhole_by_id(id)) {
                    Some(was) => {
                        let id = was.id;
                        store.write_wormhole(&Wormhole {
                        id: was.id,
                        source: was.source,
                        seen_by: was.seen_by | Source::Manual.bit(),
                        reported_at: was.reported_at,
                        detected_by: was.detected_by,
                        jumped_at: was.jumped_at,
                        uid: was.uid,
                        explicit_expiry: fresh.explicit_expiry.or(was.explicit_expiry),
                        mass: fresh.mass.or(was.mass),
                        life: fresh.life.or(was.life),
                        observed_at: fresh.observed_at.or(was.observed_at),
                        ..fresh
                    });
                        id
                    }
                    None => store.upsert_wormhole(&fresh),
                };
                if let Some(row) = store.wormhole_by_id(id) {
                    store.audit_wormhole(&row.uid, &who, Source::Manual, &changes);
                }
            }
            self.wh_reloaded = None;
            return;
        }
        if open {
            self.wh_form = Some(form);
        }
    }

    /// What is known about one system as a place for wormholes. Returns whether it was closed.
    fn wh_info_panel(&mut self, ui: &mut egui::Ui, sys: i64) -> bool {
        use crate::whdata;
        let Some(geo) = self.systems.clone() else { return false };
        let Some(info) = geo.info_of(sys).cloned() else { return true };
        let mut close = false;
        ui.horizontal(|ui| {
            ui.heading(&info.name);
            if ui.button(egui_phosphor::regular::X).on_hover_text("Close").clicked() {
                close = true;
            }
        });
        ui.label(format!("{} \u{b7} {}", whdata::class_of(sys, info.security, &info.region).label(), info.region));
        wh_system_facts(ui, sys, &info, true);
        ui.add_space(8.0);
        ui.label(egui::RichText::new(whdata::ATTRIBUTION).weak());
        close
    }
}

/// The add/edit form's fields, as typed.
#[derive(Default)]
pub(crate) struct WhForm {
    id: Option<i64>,
    system: String,
    sig: String,
    wh_type: String,
    dest: String,
    dest_sig: String,
    size: Option<crate::wormholes::ShipSize>,
    mass: Option<crate::wormholes::Mass>,
    life: Option<crate::wormholes::Life>,
    note: String,
    error: Option<String>,
    /// Who said what about this hole, oldest first.
    history: Vec<String>,
}

impl WhForm {
    /// A new hole behind a scanned signature.
    pub(crate) fn at(system: String, sig: String) -> Self {
        WhForm { system, sig, ..Default::default() }
    }

    fn of(w: &crate::wormholes::Wormhole, geo: Option<&crate::geo::Systems>) -> Self {
        let name = |id: i64| geo.and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
        WhForm {
            id: Some(w.id),
            system: name(w.system_id),
            sig: w.signature.clone().unwrap_or_default(),
            wh_type: w.wh_type.clone().unwrap_or_default(),
            dest: w.dest_system_id.map(name).unwrap_or_default(),
            dest_sig: w.dest_signature.clone().unwrap_or_default(),
            size: w.size,
            mass: w.mass,
            life: w.life,
            note: w.note.clone().unwrap_or_default(),
            error: None,
            history: Vec::new(),
        }
    }
}

/// The destination class of a hole whose far side is system `id`.
pub(crate) fn dest_class(geo: &crate::geo::Systems, id: i64) -> crate::wormholes::DestClass {
    geo.info_of(id).map_or(crate::wormholes::DestClass::Unknown, |i| class_dest(crate::whdata::class_of(id, i.security, &i.region)))
}

pub(crate) fn class_dest(c: crate::whdata::Class) -> crate::wormholes::DestClass {
    use crate::whdata::Class;
    use crate::wormholes::DestClass;
    match c {
        Class::Hs => DestClass::Highsec,
        Class::Ls => DestClass::Lowsec,
        Class::Ns => DestClass::Nullsec,
        Class::W(_) | Class::Drifter(_) => DestClass::Wspace,
        Class::Thera => DestClass::Thera,
        Class::Turnur => DestClass::Turnur,
        Class::Pochven | Class::Tabbetzur => DestClass::Unknown,
    }
}


/// A hole type combo box with a search field: there are close to a hundred codes. Typing filters
/// by code or by where the hole leads, Enter takes the first match.
pub(crate) fn wh_type_picker(ui: &mut egui::Ui, salt: &str, width: f32, value: &mut String, codes: &[&str]) -> bool {
    use crate::app::SteadySelect as _;
    let id = ui.make_persistent_id(salt);
    let (q_id, open_id) = (id.with("q"), id.with("open"));
    let before = value.clone();
    let describe = |code: &str| {
        crate::whdata::hole_type(code).map_or(String::new(), |t| {
            let dest = match t.dest {
                crate::whdata::Dest::Class(c) => c.label(),
                crate::whdata::Dest::AnyKspace => "k-space".into(),
                crate::whdata::Dest::Unknown => "the other side".into(),
            };
            format!("\u{2192} {dest}, {}", t.size_label())
        })
    };
    let shown = egui::ComboBox::from_id_salt(salt)
        .width(width)
        .height(320.0)
        .selected_text(if value.is_empty() { "unknown".to_owned() } else { value.clone() })
        .show_ui(ui, |ui| {
            let mut q: String = ui.data(|d| d.get_temp(q_id)).unwrap_or_default();
            let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search, e.g. C5 or H296").desired_width(width));
            if ui.data(|d| d.get_temp::<bool>(open_id)).is_none() {
                r.request_focus();
                ui.data_mut(|d| d.insert_temp(open_id, true));
            }
            let needle = q.trim().to_lowercase();
            let hits: Vec<&str> = codes
                .iter()
                .copied()
                .filter(|c| needle.is_empty() || c.to_lowercase().contains(&needle) || describe(c).to_lowercase().contains(&needle))
                .collect();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(first) = hits.first() {
                    *value = (*first).to_owned();
                    ui.close();
                }
            }
            if needle.is_empty() {
                ui.menu_value(value, String::new(), "unknown");
            }
            for c in &hits {
                ui.menu_value(value, (*c).to_owned(), format!("{c}  {}", describe(c)));
            }
            if hits.is_empty() {
                ui.label(egui::RichText::new("No type matches").weak());
            }
            ui.data_mut(|d| d.insert_temp(q_id, q));
        });
    if shown.inner.is_none() {
        ui.data_mut(|d| {
            d.remove::<String>(q_id);
            d.remove::<bool>(open_id);
        });
    }
    *value != before
}

/// What a system is like for wormhole purposes: class, effect with what it does, statics,
/// celestials, and for k-space the holes that can open there when `spawns` is set.
pub(crate) fn wh_system_facts(ui: &mut egui::Ui, sys: i64, info: &crate::geo::SystemInfo, spawns: bool) {
    use crate::whdata::{self, Class, Dest};
    let class = whdata::class_of(sys, info.security, &info.region);
    let summary = whdata::class_summary(class);
    if !summary.is_empty() {
        ui.label(summary);
    }
    let hole_line = |ui: &mut egui::Ui, t: &whdata::HoleType| {
        let dest = match t.dest {
            Dest::Class(c) => c.label(),
            Dest::AnyKspace => "k-space".into(),
            Dest::Unknown => "the other side".into(),
        };
        ui.label(format!(
            "{}{} \u{2192} {dest}: {}, {} t per jump, {} t total, {}h",
            t.code,
            if t.is_static { " (static)" } else { "" },
            t.size_label(),
            tonnes(t.jump_mass),
            tonnes(t.total_mass),
            t.lifetime_h
        ));
    };
    if let Some(j) = whdata::jsystem(sys) {
        if j.shattered() && class != Class::W(13) {
            ui.label(whdata::SHATTERED_NOTE);
        }
        ui.add_space(6.0);
        match &j.effect {
            Some(effect) => {
                ui.label(egui::RichText::new(effect).strong());
                ui.label(whdata::effect_summary(effect));
                for (m, v) in whdata::effect_mods(effect, j.class) {
                    ui.label(format!("{m} {v}"));
                }
            }
            None => {
                ui.label(egui::RichText::new("No system effect").weak());
            }
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Statics").strong());
        if j.statics.is_empty() {
            ui.label(egui::RichText::new("None").weak());
        }
        for code in &j.statics {
            match whdata::hole_type(code) {
                Some(t) => hole_line(ui, t),
                None => {
                    ui.label(code);
                }
            }
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Celestials").strong());
        let mut kinds: Vec<(&str, usize)> = Vec::new();
        for p in &j.planets {
            match kinds.iter_mut().find(|(k, _)| *k == p.as_str()) {
                Some((_, n)) => *n += 1,
                None => kinds.push((p.as_str(), 1)),
            }
        }
        ui.label(format!(
            "Sun {} \u{b7} {} planet{} \u{b7} {} moon{}",
            j.sun,
            j.planets.len(),
            if j.planets.len() == 1 { "" } else { "s" },
            j.moons,
            if j.moons == 1 { "" } else { "s" }
        ));
        if !kinds.is_empty() {
            ui.label(kinds.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", "));
        }
    }
    if !spawns {
        return;
    }
    if class == Class::Pochven {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Its C729 can open in").strong());
        let zone = whdata::c729_zone(&info.name);
        ui.label(if zone.is_empty() { "unknown".into() } else { zone.join(", ") });
    } else if class.is_kspace() {
        let targets = whdata::c729_targets(&info.name);
        if !targets.is_empty() {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Can host the C729 of").strong());
            ui.label(targets.join(", "));
        }
    }
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Holes that can open here").strong());
    let lowsec_hub = matches!(class, Class::Turnur | Class::Tabbetzur);
    for t in whdata::types().iter().filter(|t| t.src.contains(&class) || (lowsec_hub && t.src.contains(&Class::Ls))) {
        hole_line(ui, t);
    }
}

/// One button per choice in a row; clicking the chosen one again clears it back to unknown.
pub(crate) fn choice_row<T: Copy + PartialEq>(ui: &mut egui::Ui, value: &mut Option<T>, items: &[(T, &str, &str)]) {
    use crate::app::SteadySelect as _;
    ui.horizontal(|ui| {
        for (v, short, long) in items {
            let on = *value == Some(*v);
            if ui.menu_label(on, *short).on_hover_text(*long).clicked() {
                *value = if on { None } else { Some(*v) };
            }
        }
    });
}

/// Kilograms as whole tonnes with thousands separators, e.g. 62,000.
fn tonnes(kg: u64) -> String {
    let digits = (kg / 1000).to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
