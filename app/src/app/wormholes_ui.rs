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
        let now = crate::clock::utc().timestamp();
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
        let now = crate::clock::utc().timestamp();
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
        let usable = self.wh_usable();
        let was = self.route_destination.and_then(|d| self.ingame_hole_waypoints(d));
        if let Some(store) = &self.store {
            let now = crate::clock::utc().timestamp();
            store.prune_wormholes(now);
            let mut whs = store.wormholes();
            // Saved before these were refused, or by an older version.
            whs.retain(|w| !w.is_expired(now) && crate::whdata::connection_problem(w.system_id, w.dest_system_id, |_| None, None, None).is_none());
            let groups = store.wormhole_groups();
            let hidden = store.share_hidden_groups();
            if !hidden.is_empty() {
                whs.retain(|w| groups.get(&w.uid).is_none_or(|g| !hidden.contains(g)));
            }
            whs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            let authors = store.wormhole_authors();
            for w in &mut whs {
                if let Some((created, edited)) = authors.get(&w.uid) {
                    w.created_by = Some(created.clone());
                    w.edited_by = edited.clone();
                }
            }
            // A hole switched off that has since closed is forgotten with it.
            let before = self.settings.wh_disabled_holes.len();
            self.settings.wh_disabled_holes.retain(|u| whs.iter().any(|w| &w.uid == u));
            self.needs_save |= self.settings.wh_disabled_holes.len() != before;
            self.wh_cache = whs;
            self.wh_overlay = WhOverlay::build(&self.wh_cache, |w| !self.wh_blocked(w, now));
            self.wh_group_of = groups;
        }
        // Synced, expired, edited or found: a route through a hole that changed is out of date.
        if self.wh_usable() != usable {
            self.wh_routes_refresh(Some(was));
        }
    }

    /// The live hole graph. Built from `wh_cache`, not `wh_overlay`: the overlay drops high-degree
    /// hubs to keep the map readable, which is exactly what would remove Thera from a route.
    /// The holes routes may use: known far side, and a kind the routing settings allow.
    pub(crate) fn wh_adjacency(&self) -> std::collections::HashMap<i64, Vec<i64>> {
        let mut adj: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        let now = crate::clock::utc().timestamp();
        let limits = self.wh_route_limits();
        let allowed = |w: &crate::wormholes::Wormhole, b: i64| {
            let kind_ok = self.systems.as_ref().is_none_or(|g| {
                let kind = crate::wormholes::HoleKind::of(g, w.system_id, b, w.is_drifter);
                self.settings.wh_route_kinds.iter().any(|k| k == kind.code())
            });
            kind_ok && limits.allows(w, now) && !self.wh_blocked(w, now)
        };
        for w in &self.wh_cache {
            if let Some(b) = w.dest_system_id.filter(|b| allowed(w, *b)) {
                adj.entry(w.system_id).or_default().push(b);
                adj.entry(b).or_default().push(w.system_id);
            }
        }
        adj
    }

    pub(crate) fn wh_route_limits(&self) -> crate::wormholes::RouteLimits {
        use crate::wormholes::{Mass, ShipSize, TimeLeft};
        crate::wormholes::RouteLimits {
            min_mass: Mass::from_code(&self.settings.wh_route_min_mass),
            time_below: TimeLeft::ALL.into_iter().find(|t| t.code() == self.settings.wh_route_min_time),
            min_size: ShipSize::from_code(&self.settings.wh_route_min_size),
        }
    }

    /// Whether the wormhole tab's filter lets the hole through.
    pub(crate) fn wh_shown(&self, w: &crate::wormholes::Wormhole, now: i64) -> bool {
        self.settings.wh_filter.matches(w, now, |d| self.wh_touches(w, d))
    }

    /// Switched off for routes by hand, on its own or through a system at either end.
    pub(crate) fn wh_disabled(&self, w: &crate::wormholes::Wormhole) -> bool {
        self.settings.wh_disabled_holes.contains(&w.uid) || self.wh_system_disabled(w.system_id) || w.dest_system_id.is_some_and(|b| self.wh_system_disabled(b))
    }

    pub(crate) fn wh_system_disabled(&self, id: i64) -> bool {
        self.settings.wh_disabled_systems.contains(&id)
    }

    /// Kept off routes by the user: switched off, or hidden by the filter while routes follow it.
    /// The route limits are not counted, they belong to the route being planned.
    pub(crate) fn wh_blocked(&self, w: &crate::wormholes::Wormhole, now: i64) -> bool {
        self.wh_disabled(w) || (self.settings.wh_route_filtered && !self.wh_shown(w, now))
    }

    pub(crate) fn toggle_wh_hole(&mut self, uid: &str) {
        let list = &mut self.settings.wh_disabled_holes;
        match list.iter().position(|u| u == uid) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(uid.to_owned()),
        }
        self.wh_routing_changed();
    }

    pub(crate) fn toggle_wh_system(&mut self, id: i64) {
        let list = &mut self.settings.wh_disabled_systems;
        match list.iter().position(|s| *s == id) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(id),
        }
        self.wh_routing_changed();
    }

    /// A system's holes as right-click menu entries, each to switch on or off for routes, and the
    /// whole system at once. Shows nothing for a system without a known hole.
    pub(crate) fn wh_disable_menu(&mut self, ui: &mut egui::Ui, sid: i64) {
        const LISTED: usize = 8;
        let holes: Vec<(String, String)> = self
            .wh_cache
            .iter()
            .filter(|w| w.system_id == sid || w.dest_system_id == Some(sid))
            .map(|w| {
                let (sig, far) = if w.system_id == sid { (&w.signature, w.dest_system_id) } else { (&w.dest_signature, Some(w.system_id)) };
                let far = far.and_then(|f| self.systems.as_ref()?.info_of(f)).map_or_else(|| w.dest.label().to_owned(), |i| i.name.clone());
                let sig = sig.as_deref().map(|s| format!("{} ", s.chars().take(3).collect::<String>())).unwrap_or_default();
                (w.uid.clone(), format!("{sig}to {far}"))
            })
            .collect();
        let hub = sid == 31_000_005 || self.wh_system_disabled(sid);
        if holes.is_empty() && !hub {
            return;
        }
        let name = self.systems.as_ref().and_then(|g| g.info_of(sid)).map_or_else(|| format!("#{sid}"), |i| i.name.clone());
        ui.separator();
        ui.label(egui::RichText::new("Wormholes routes may use").weak());
        let mut all = !self.wh_system_disabled(sid);
        if ui
            .checkbox(&mut all, format!("Any hole in {name}"))
            .on_hover_text("Off keeps every hole here off routes, those found later too")
            .changed()
        {
            self.toggle_wh_system(sid);
        }
        ui.add_enabled_ui(all, |ui| {
            for (uid, label) in holes.iter().take(LISTED) {
                let mut on = !self.settings.wh_disabled_holes.contains(uid);
                if ui.checkbox(&mut on, label).on_hover_text("Both sides of this hole").changed() {
                    self.toggle_wh_hole(uid);
                }
            }
        });
        if holes.len() > LISTED {
            ui.label(egui::RichText::new(format!("{} more in the wormhole tab", holes.len() - LISTED)).weak());
        }
    }

    pub(crate) fn clear_wh_disabled(&mut self) {
        self.settings.wh_disabled_holes.clear();
        self.settings.wh_disabled_systems.clear();
        self.wh_routing_changed();
    }

    /// Holes and systems switched off that are still on the map.
    pub(crate) fn wh_disabled_count(&self) -> usize {
        self.wh_cache.iter().filter(|w| self.settings.wh_disabled_holes.contains(&w.uid)).count() + self.settings.wh_disabled_systems.len()
    }

    /// Which holes routes may use changed: the plan in hand may go through one no longer allowed.
    pub(crate) fn wh_routing_changed(&mut self) {
        self.needs_save = true;
        self.wh_overlay = WhOverlay::build(&self.wh_cache, |w| !self.wh_blocked(w, crate::clock::utc().timestamp()));
        self.wh_routes_refresh(None);
    }

    /// Toggles for the kinds of hole routes may use. Returns whether any changed.
    pub(crate) fn wh_route_kinds_ui(&mut self, ui: &mut egui::Ui) -> bool {
        ui.horizontal(|ui| {
            ui.label("Through");
            self.wh_route_kinds_button(ui)
        })
        .inner
    }

    /// A button naming how many kinds of hole routes may use, opening a list to pick them.
    fn wh_route_kinds_button(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::app::SteadySelect as _;
        use crate::wormholes::HoleKind;
        let mut changed = false;
        let on = |k: HoleKind, s: &crate::settings::Settings| s.wh_route_kinds.iter().any(|c| c == k.code());
        let n = HoleKind::ALL.iter().filter(|k| on(**k, &self.settings)).count();
        let text = match n {
            0 => "No holes".to_owned(),
            n if n == HoleKind::ALL.len() => "All kinds".to_owned(),
            1 => HoleKind::ALL.iter().find(|k| on(**k, &self.settings)).map_or_else(String::new, |k| k.label().to_owned()),
            n => format!("{n} of {} kinds", HoleKind::ALL.len()),
        };
        let button = ui.button(format!("{text}  {}", egui_phosphor::regular::CARET_DOWN));
        // Stays open while kinds are picked: each click is one of several choices.
        egui::Popup::from_toggle_button_response(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for k in HoleKind::ALL {
                let was = on(k, &self.settings);
                if ui.menu_label(was, k.label()).clicked() {
                    if was {
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

    /// The kinds of hole routes may use and what each hole must still have, one dropdown each.
    /// Returns whether any changed.
    pub(crate) fn wh_route_options_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::app::SteadySelect as _;
        use crate::wormholes::{Mass, ShipSize, TimeLeft};
        let mut changed = false;
        let combo = |ui: &mut egui::Ui, id: &str, value: &mut String, items: &[(&str, &str, &str)]| -> bool {
            let mut changed = false;
            let current = items.iter().find(|i| i.0 == value.as_str()).unwrap_or(&items[0]).1;
            egui::ComboBox::from_id_salt(("wh_route_opt", id)).selected_text(current).show_ui(ui, |ui| {
                for (code, label, hint) in items {
                    if ui.menu_label(value == code, *label).on_hover_text(*hint).clicked() && value != code {
                        *value = (*code).to_owned();
                        changed = true;
                    }
                }
            });
            changed
        };
        egui::Grid::new(ui.id().with("wh_route_opts")).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Through");
            changed |= self.wh_route_kinds_button(ui);
            ui.end_row();
            ui.label("Mass");
            changed |= combo(
                ui,
                "mass",
                &mut self.settings.wh_route_min_mass,
                &[
                    ("", "Any", "Any mass left"),
                    (Mass::Reduced.code(), "Not critical", "Skip holes with under 10% mass left. Holes with their mass not read pass."),
                    (Mass::Fresh.code(), "Over 50%", "Only holes with over half their mass left. Holes with their mass not read pass."),
                ],
            );
            ui.end_row();
            ui.label("Time left");
            changed |= combo(
                ui,
                "time",
                &mut self.settings.wh_route_min_time,
                &[
                    ("", "Any", "Any time left, even holes that could close any moment"),
                    (TimeLeft::Expiring.code(), "Not expired", "Skip holes past their time"),
                    (TimeLeft::Under1h.code(), "1h or more", "Skip holes with under an hour left"),
                    (TimeLeft::Under4h.code(), "4h or more", "Skip holes with under 4 hours left"),
                    (TimeLeft::Under12h.code(), "12h or more", "Skip holes with under 12 hours left"),
                ],
            );
            ui.end_row();
            ui.label("Size");
            changed |= combo(
                ui,
                "size",
                &mut self.settings.wh_route_min_size,
                &[
                    ("", "Any", "Any hole size"),
                    (ShipSize::Medium.code(), "Medium or bigger", "Holes a cruiser fits through. Holes of unknown size and type pass."),
                    (ShipSize::Large.code(), "Large or bigger", "Holes a battleship fits through. Holes of unknown size and type pass."),
                    (ShipSize::XLarge.code(), "XL", "Holes a capital fits through. Holes of unknown size and type pass."),
                ],
            );
            ui.end_row();
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
    }

    /// Redoes every route after the holes they may use changed: the travel route, the map's
    /// planned route and the one in the game. `was` is the game route's hole waypoints from before;
    /// the game is only sent the route again when they differ. `None` sends it anyway.
    pub(crate) fn wh_routes_refresh(&mut self, was: Option<Option<Vec<i64>>>) {
        self.plan_route();
        if !self.map_route_anchors.is_empty() {
            self.map_recompute_route();
        }
        let Some(dest) = self.route_destination else { return };
        if self.active_character == "No character" || was.is_some_and(|w| w == self.ingame_hole_waypoints(dest)) {
            return;
        }
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        self.set_destination_esi(cid, self.active_character.clone(), dest);
    }

    fn ingame_hole_waypoints(&self, dest: i64) -> Option<Vec<i64>> {
        if !self.settings.route_via_wormholes {
            return None;
        }
        self.wh_route_waypoints(self.player_system()?, dest)
    }

    /// Holes routes may use, as a stable value to compare.
    fn wh_usable(&self) -> Vec<(i64, Vec<i64>)> {
        let mut v: Vec<(i64, Vec<i64>)> = self.wh_adjacency().into_iter().map(|(k, mut n)| {
            n.sort_unstable();
            (k, n)
        }).collect();
        v.sort_unstable();
        v
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
        self.track_scanner();
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
            ui.heading(format!("{}  Wormholes", icon::SPIRAL));
            ui.label(egui::RichText::new(format!("{} known", self.wh_cache.len())).weak());
            ui.add_space(8.0);
            let (table, sigs) = (self.wh_graph.table, self.wh_graph.sig_browser);
            if ui.menu_label(!table && !sigs, format!("{}  Map", icon::GRAPH)).clicked() {
                (self.wh_graph.table, self.wh_graph.sig_browser) = (false, false);
            }
            if ui.menu_label(table && !sigs, format!("{}  Table", icon::TABLE)).clicked() {
                (self.wh_graph.table, self.wh_graph.sig_browser) = (true, false);
            }
            if ui.menu_label(sigs, format!("{}  Signatures", icon::LIST_MAGNIFYING_GLASS)).on_hover_text("Every probe scan signature pasted, to search and clean up").clicked() {
                self.wh_graph.sig_browser = true;
            }
            ui.add_space(8.0);
            if ui.button(format!("{}  Add", icon::PLUS)).on_hover_text("Enter a hole by hand").clicked() {
                let form = self.wh_form_here();
                self.wh_form = Some(form);
            }
            ui.add_space(8.0);
            let active = self.settings.wh_filter.active();
            let label = if active == 0 { format!("{}  Filter", icon::FUNNEL) } else { format!("{}  Filter ({active})", icon::FUNNEL) };
            let filter_btn = ui.button(label).on_hover_text("Which holes the map and the table show");
            let mut filter_changed = false;
            egui::Popup::from_toggle_button_response(&filter_btn)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    filter_changed |= wh_filter_ui(ui, &mut self.settings.wh_filter);
                });
            if ui
                .add_enabled(active > 0, egui::Button::new(icon::FUNNEL_X))
                .on_hover_text("Clear the filter")
                .on_disabled_hover_text("No filter set")
                .clicked()
            {
                self.settings.wh_filter = Default::default();
                filter_changed = true;
            }
            let routes_changed = ui
                .checkbox(&mut self.settings.wh_route_filtered, "Only filtered WH for routes")
                .on_hover_text("Routes use only the holes the filter shows. The holes switched off by hand stay off either way.")
                .changed();
            if routes_changed || (filter_changed && self.settings.wh_route_filtered) {
                self.wh_routing_changed();
            } else if filter_changed {
                self.needs_save = true;
            }
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
                ui.horizontal(|ui| {
                    ui.label("Pinned systems join clusters within");
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.settings.wh_pin_jumps).range(1..=crate::settings::WH_PIN_JUMPS_MAX))
                        .on_hover_text("Gate jumps from a cluster's nearest exit. Further out, a pinned system shows on its own.")
                        .changed();
                    ui.label("jumps");
                });
                ui.separator();
                if ui.button(format!("{}  Sharing…", icon::USERS_THREE)).on_hover_text("Share wormholes with others, end-to-end encrypted").clicked() {
                    self.wh_share.open = true;
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
        if self.wh_graph.sig_browser {
            self.sig_browser_view(ui);
            return;
        }
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
        let now = crate::clock::utc().timestamp();
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
            uid: String,
            off: bool,
        }
        let info_of = |id: i64| self.systems.as_ref().and_then(|s| s.info_of(id)).cloned();
        let rows: Vec<Row> = self
            .wh_cache
            .iter()
            .filter(|w| self.wh_shown(w, now))
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
                    uid: w.uid.clone(),
                    off: self.settings.wh_disabled_holes.contains(&w.uid),
                }
            })
            .collect();

        let mut kill: Option<i64> = None;
        let mut edit: Option<i64> = None;
        let mut info: Option<i64> = None;
        let mut toggle: Option<String> = None;
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
                            if super::wh_graph::wh_route_toggle(ui, r.off) {
                                toggle = Some(r.uid.clone());
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
        if let Some(uid) = toggle {
            self.toggle_wh_hole(&uid);
        }
    }

    #[cfg(test)]
    pub(crate) fn open_wh_form(&mut self, form: WhForm) {
        self.wh_form = Some(form);
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
    pub(crate) fn wh_form_window(&mut self, ctx: &egui::Context) {
        use crate::wormholes::{Source, Wormhole};
        let Some(mut form) = self.wh_form.take() else { return };
        let mut open = true;
        let mut save = false;
        // The signatures saved for each side's system, to pick from.
        let sigs_of = |name: &str| -> Vec<(String, String)> {
            let Some(id) = self.systems.as_ref().and_then(|g| g.lookup(name.trim())).map(|i| i.id) else { return Vec::new() };
            let mut list = self.store.as_ref().map(|s| s.system_sigs(id)).unwrap_or_default();
            list.retain(|s| offerable(s, id, &self.wh_cache, form.id));
            // Wormholes first, then what is not scanned yet, then the rest; by signature within.
            let rank = |g: &str| if g.to_lowercase().contains("wormhole") { 0 } else if g.is_empty() { 1 } else { 2 };
            list.sort_by(|a, b| rank(&a.group).cmp(&rank(&b.group)).then(a.sig.cmp(&b.sig)));
            list.into_iter()
                .map(|s| {
                    let what = if s.name.is_empty() { if s.group.is_empty() { "not scanned yet".to_owned() } else { s.group } } else { s.name };
                    (s.sig, what)
                })
                .collect()
        };
        let (here_sigs, there_sigs) = (sigs_of(&form.system), sigs_of(&form.dest));
        let (type_was, dest_was) = (form.wh_type.clone(), form.dest.clone());
        egui::Window::new(if form.id.is_some() { "Edit wormhole" } else { "Add wormhole" })
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                save = spai_ui::wh_form::form_ui(ui, &mut form, &mut |ui, key, q, hint, w| self.system_input(ui, key, q, hint, w), &here_sigs, &there_sigs);
            });
        if let Some(g) = self.systems.clone() {
            let (t, d) = (form.wh_type != type_was, form.dest != dest_was);
            drifter_autofill(&g, &mut form.wh_type, &mut form.dest, t, d);
        }
        if save {
            let Some(geo) = self.systems.clone() else {
                self.wh_form = Some(form);
                return;
            };
            let now = crate::clock::utc().timestamp();
            let editing = form.id;
            let complete = |sys: Option<i64>, typed: &str| self.wh_complete_sig(sys, typed, editing);
            let built = spai_ui::wh_form::build(&mut form, &geo, &complete, now);
            let (fresh, changes) = match built {
                Ok(b) => b,
                Err(why) => {
                    form.error = Some(why);
                    self.wh_form = Some(form);
                    return;
                }
            };
            let who = if self.settings.active_character.is_empty() { "me".to_owned() } else { self.settings.active_character.clone() };
            self.scanner_track.last_manual = Some((who.clone(), now));
            if let Some(store) = &self.store {
                let id = match form.id.and_then(|id| store.wormhole_by_id(id)) {
                    Some(was) => {
                        let id = was.id;
                        let expiry = was.expiry_after_reading(fresh.life, now);
                        store.write_wormhole(&Wormhole {
                        id: was.id,
                        source: was.source,
                        seen_by: was.seen_by | Source::Manual.bit(),
                        reported_at: was.reported_at,
                        detected_by: was.detected_by,
                        jumped_at: was.jumped_at,
                        uid: was.uid,
                        explicit_expiry: expiry,
                        mass: fresh.mass.or(was.mass),
                        life: fresh.life.or(was.life),
                        observed_at: fresh.observed_at.or(was.observed_at),
                        ..fresh
                    });
                        id
                    }
                    None => store.upsert_wormhole(&fresh),
                };
                store.absorb_twins(id);
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

    /// Full signature ids known in `system`: scanned there, or on another hole's end there.
    pub(crate) fn wh_known_sigs(&self, system: i64, except: Option<i64>) -> Vec<String> {
        let Some(store) = &self.store else { return Vec::new() };
        let now = crate::clock::utc().timestamp();
        let mut v: Vec<String> = store.sigs_in(system, false).into_iter().map(|s| s.sig).collect();
        for w in store.wormholes().into_iter().filter(|w| Some(w.id) != except && !w.is_expired(now)) {
            if w.system_id == system {
                v.extend(w.signature);
            }
            if w.dest_system_id == Some(system) {
                v.extend(w.dest_signature);
            }
        }
        v
    }

    /// A typed signature in `system` made whole from what is known there. `Ok(None)` keeps what was
    /// typed; `Err` says which ids it could be, as a partial code must name one.
    pub(crate) fn wh_complete_sig(&self, system: Option<i64>, typed: &str, except: Option<i64>) -> Result<Option<String>, String> {
        let Some(system) = system else { return Ok(None) };
        crate::wormholes::complete_sig(typed, &self.wh_known_sigs(system, except)).map_err(|ids| {
            let name = self.systems.as_ref().and_then(|g| g.info_of(system)).map(|i| i.name.clone()).unwrap_or_default();
            format!("{} in {name} could be {}. Type more of it.", typed.trim().to_uppercase(), ids.join(" or "))
        })
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

/// Whether a saved signature in `system` can still be picked for a hole: not scanned as something
/// else, and not already another hole's (`except`, the hole being edited, may keep its own).
pub(crate) fn offerable(s: &crate::store::SystemSig, system: i64, holes: &[crate::wormholes::Wormhole], except: Option<i64>) -> bool {
    let group = s.group.to_lowercase();
    if s.kind.to_lowercase().contains("anomal") || (!group.is_empty() && !group.contains("wormhole")) {
        return false;
    }
    let letters = |x: &str| x.trim().chars().take(3).collect::<String>().to_uppercase();
    let mine = letters(&s.sig);
    !holes.iter().filter(|w| Some(w.id) != except).any(|w| {
        let here = if w.system_id == system {
            w.signature.as_deref()
        } else if w.dest_system_id == Some(system) {
            w.dest_signature.as_deref()
        } else {
            None
        };
        here.is_some_and(|h| letters(h) == mine)
    })
}

pub(crate) use spai_ui::wh_form::{choice_row, drifter_autofill, sig_field, wh_type_picker, WhForm};
pub(crate) use spai_ui::wh_tab::wh_filter_ui;

/// The destination class of a hole whose far side is system `id`.
pub(crate) use spai_core::wormholes::dest_class;



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
        ui.label(format!("{} \u{2192} {dest}, {}", t.code, t.size_label())).on_hover_text(format!(
            "{} t per jump\n{} t in all\nLasts {}h{}",
            tonnes(t.jump_mass),
            tonnes(t.total_mass),
            t.lifetime_h,
            if t.is_static { "\nA static somewhere" } else { "" }
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

