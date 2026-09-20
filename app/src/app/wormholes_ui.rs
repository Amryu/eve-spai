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
        }
    }

    /// The live hole graph. Built from `wh_cache`, not `wh_overlay`: the overlay drops high-degree
    /// hubs to keep the map readable, which is exactly what would remove Thera from a route.
    pub(crate) fn wh_adjacency(&self) -> std::collections::HashMap<i64, Vec<i64>> {
        let mut adj: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        for w in &self.wh_cache {
            if let Some(b) = w.dest_system_id {
                adj.entry(w.system_id).or_default().push(b);
                adj.entry(b).or_default().push(w.system_id);
            }
        }
        adj
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
    #[cfg(feature = "fc-rescue")]
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
        crate::esi::set_waypoint(cid, cname, dest, true);
    }

    /// Remembers that the game now holds a route this app set.
    pub(crate) fn note_ingame_route(&mut self) {
        self.ingame_route = true;
    }

    pub(crate) fn wormholes_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(format!("{}  Wormholes", egui_phosphor::regular::SPIRAL));
            ui.label(egui::RichText::new(format!("{} known", self.wh_cache.len())).weak());
        });
        ui.horizontal(|ui| {
            use crate::wormholes::{DestClass, Source};
            ui.label("Dest:");
            egui::ComboBox::from_id_salt("wh_dest_filter")
                .selected_text(self.wh_filter_dest.map_or("Any", |d| d.label()))
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
            ui.label("Source:");
            egui::ComboBox::from_id_salt("wh_src_filter")
                .selected_text(self.wh_filter_source.map_or("Any", |s| s.label()))
                .show_ui(ui, |ui| {
                    ui.menu_value(&mut self.wh_filter_source, None, "Any");
                    for s in [Source::EveScout, Source::Intel, Source::Manual] {
                        ui.menu_value(&mut self.wh_filter_source, Some(s), s.label());
                    }
                });
            ui.checkbox(&mut self.wh_filter_expiring, "Expiring <4h");
        });
        ui.separator();
        if self.wh_cache.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No wormholes known yet.").weak());
                ui.label(
                    egui::RichText::new("Seeded from EVE-Scout (Thera/Turnur) and intel channels.")
                        .weak(),
                );
            });
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
                let life = if w.explicit_expiry.is_some() {
                    match w.hours_left(now) {
                        Some(h) => format!("< {h}h left"),
                        None => "expired".into(),
                    }
                } else {
                    format!("reported {} ago", human_ago(now - w.reported_at))
                };
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
                    size: w.effective_size().map(|s| s.label().to_string()).unwrap_or_else(|| "—".into()),
                    life,
                    source: w.source.label().to_string(),
                }
            })
            .collect();

        use egui_phosphor::regular as icon;
        let mut kill: Option<i64> = None;
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
    }
}
