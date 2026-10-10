//! Wormhole connections and the wormholes view, with the kill history they depend on.

use spai_ui::i18n::Tr;

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
            let mut sigs: std::collections::HashMap<i64, Vec<crate::store::SystemSig>> = std::collections::HashMap::new();
            for (sys, sig) in store.all_system_sigs() {
                sigs.entry(sys).or_default().push(sig);
            }
            let sigs_at = |sys: Option<i64>| sys.and_then(|id| sigs.get(&id)).map_or(&[][..], Vec::as_slice);
            for w in &mut whs {
                if let Some((created, edited)) = authors.get(&w.uid) {
                    w.created_by = Some(created.clone());
                    w.edited_by = edited.clone();
                }
                w.born_after = w.opened_after(sigs_at(Some(w.system_id)), sigs_at(w.dest_system_id));
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
                let far = far.and_then(|f| self.systems.as_ref()?.info_of(f)).map_or_else(|| w.dest.label().tr().to_owned(), |i| i.name.clone());
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
        ui.label(egui::RichText::new(tr!("Wormholes routes may use")).weak());
        let mut all = !self.wh_system_disabled(sid);
        if ui
            .checkbox(&mut all, trf!("Any hole in {name}", name = name))
            .on_hover_text(tr!("Off keeps every hole here off routes, those found later too"))
            .changed()
        {
            self.toggle_wh_system(sid);
        }
        ui.add_enabled_ui(all, |ui| {
            for (uid, label) in holes.iter().take(LISTED) {
                let mut on = !self.settings.wh_disabled_holes.contains(uid);
                if ui.checkbox(&mut on, label).on_hover_text(tr!("Both sides of this hole")).changed() {
                    self.toggle_wh_hole(uid);
                }
            }
        });
        if holes.len() > LISTED {
            ui.label(egui::RichText::new(trf!("{v} more in the wormhole tab", v = holes.len() - LISTED)).weak());
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
            ui.label(tr!("Through"));
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
            0 => tr!("No holes").to_owned(),
            n if n == HoleKind::ALL.len() => tr!("All kinds").to_owned(),
            1 => HoleKind::ALL.iter().find(|k| on(**k, &self.settings)).map_or_else(String::new, |k| k.label().tr().to_owned()),
            n => trf!("{n} of {total} kinds", n = n, total = HoleKind::ALL.len()),
        };
        let button = ui.button(format!("{text}  {}", egui_phosphor::regular::CARET_DOWN));
        // Stays open while kinds are picked: each click is one of several choices.
        egui::Popup::from_toggle_button_response(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for k in HoleKind::ALL {
                let was = on(k, &self.settings);
                if ui.menu_label(was, k.label().tr()).clicked() {
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
            ui.label(tr!("Through"));
            changed |= self.wh_route_kinds_button(ui);
            ui.end_row();
            ui.label(tr!("Mass"));
            changed |= combo(
                ui,
                "mass",
                &mut self.settings.wh_route_min_mass,
                &[
                    ("", tr!("Any"), tr!("Any mass left")),
                    (Mass::Reduced.code(), tr!("Not critical"), tr!("Skip holes with under 10% mass left. Holes with their mass not read pass.")),
                    (Mass::Fresh.code(), tr!("Over 50%"), tr!("Only holes with over half their mass left. Holes with their mass not read pass.")),
                ],
            );
            ui.end_row();
            ui.label(tr!("Time left"));
            changed |= combo(
                ui,
                "time",
                &mut self.settings.wh_route_min_time,
                &[
                    ("", tr!("Any"), tr!("Any time left, even holes that could close any moment")),
                    (TimeLeft::Expiring.code(), tr!("Not expired"), tr!("Skip holes past their time")),
                    (TimeLeft::Under1h.code(), tr!("1h or more"), tr!("Skip holes with under an hour left")),
                    (TimeLeft::Under4h.code(), tr!("4h or more"), tr!("Skip holes with under 4 hours left")),
                    (TimeLeft::Under12h.code(), tr!("12h or more"), tr!("Skip holes with under 12 hours left")),
                ],
            );
            ui.end_row();
            ui.label(tr!("Size"));
            changed |= combo(
                ui,
                "size",
                &mut self.settings.wh_route_min_size,
                &[
                    ("", tr!("Any"), tr!("Any hole size")),
                    (ShipSize::Medium.code(), tr!("Medium or bigger"), tr!("Holes a cruiser fits through. Holes of unknown size and type pass.")),
                    (ShipSize::Large.code(), tr!("Large or bigger"), tr!("Holes a battleship fits through. Holes of unknown size and type pass.")),
                    (ShipSize::XLarge.code(), "XL", tr!("Holes a capital fits through. Holes of unknown size and type pass.")),
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

    /// Redoes every route after the holes they may use changed: the map's planned route and the one
    /// in the game. `was` is the game route's hole waypoints from before;
    /// the game is only sent the route again when they differ. `None` sends it anyway.
    pub(crate) fn wh_routes_refresh(&mut self, was: Option<Option<Vec<i64>>>) {
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
        if let Some(dest) = self.route_destination {
            if self.active_character != "No character" {
                let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
                self.set_destination_esi(cid, self.active_character.clone(), dest);
            }
        }
    }

    /// Where a signed-in character is, by name.
    pub(crate) fn char_system(&self, name: &str) -> Option<i64> {
        let p = self.player.lock().unwrap();
        p.locations.get(name).map(|(s, _)| *s).or_else(|| (name == self.active_character).then_some(p.system_id).flatten())
    }

    /// Characters a route can be sent to: signed in, with the scope to set waypoints.
    pub(crate) fn waypoint_characters(&self) -> Vec<String> {
        self.characters
            .iter()
            .filter(|c| c.scopes.split_whitespace().any(|s| s == "esi-ui.write_waypoint.v1"))
            .map(|c| c.name.clone())
            .collect()
    }

    /// Routes each of `names` to `dest`, each from where that character is. The app's own route
    /// follows only when the active character is among them.
    pub(crate) fn set_destination_for(&mut self, names: &[String], dest: i64) {
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        for n in names {
            self.set_destination_esi(cid.clone(), n.clone(), dest);
        }
        if names.contains(&self.active_character) {
            self.route_destination = Some(dest);
            // The planner's own route would hide this one, holes and all.
            self.map_route_clear();
            self.note_ingame_route();
        }
    }

    /// The menu of characters a route may also go to: all of them, then each one. Returns who was
    /// picked.
    pub(crate) fn destination_characters_menu(&self, ui: &mut egui::Ui) -> Option<Vec<String>> {
        pick_characters_menu(ui, &self.destination_choices())
    }

    /// The characters a route may go to, each with the system it is in when known.
    pub(crate) fn destination_choices(&self) -> Vec<(String, Option<String>)> {
        self.waypoint_characters()
            .into_iter()
            .map(|n| {
                let here = self.char_system(&n).and_then(|s| self.systems.as_ref()?.info_of(s).map(|i| i.name.clone()));
                (n, here)
            })
            .collect()
    }

    /// `[ label | ▾ ]`: the button sends to the active character, the arrow to any of them. Returns
    /// who to send to.
    pub(crate) fn destination_split_button(&self, ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, enabled: bool) -> Option<Vec<String>> {
        let mut picked = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if ui
                .add_enabled(enabled, egui::Button::new(label))
                .on_disabled_hover_text(tr!("Log a character in to route in the game"))
                .clicked()
            {
                picked = Some(vec![self.active_character.clone()]);
            }
            let r = ui.menu_button(egui_phosphor::regular::CARET_DOWN, |ui| {
                if let Some(p) = self.destination_characters_menu(ui) {
                    picked = Some(p);
                }
            });
            r.response.on_hover_text(tr!("Set it for other characters"));
        });
        picked
    }

    pub(crate) fn set_destination_esi(&self, cid: String, cname: String, dest: i64) {
        let from = self.char_system(&cname);
        if self.settings.route_via_wormholes {
            if let Some(from) = from {
                if let Some(wp) = self.wh_route_waypoints(from, dest) {
                    if wp.len() > 1 {
                        crate::esi::set_route(cid, cname, wp);
                        return;
                    }
                }
            }
        }
        if self.force_ansiblex_route(&cid, &cname, from, dest) {
            return;
        }
        crate::esi::set_waypoint(cid, cname, dest, true);
    }

    /// The client's route planner ignores Ansiblex zones, so a route that avoids the costly ones
    /// is pinned with waypoints. Returns whether it took over; the work happens off the UI thread,
    /// since it walks the whole map a few times.
    fn force_ansiblex_route(&self, cid: &str, cname: &str, from: Option<i64>, dest: i64) -> bool {
        let bridges = self.settings.jump_bridges.clone();
        if bridges.is_empty() {
            return false;
        }
        let (Some(from), Some(graph)) = (from, self.systems.clone()) else {
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
            ui.heading(trf!("{icon}  Wormholes", icon = icon::SPIRAL));
            ui.label(egui::RichText::new(trf!("{v} known", v = self.wh_cache.len())).weak());
            ui.add_space(8.0);
            let (table, sigs) = (self.wh_graph.table, self.wh_graph.sig_browser);
            if ui.menu_label(!table && !sigs, trf!("{icon}  Map", icon = icon::GRAPH)).clicked() {
                (self.wh_graph.table, self.wh_graph.sig_browser) = (false, false);
            }
            if ui.menu_label(table && !sigs, trf!("{icon}  Table", icon = icon::TABLE)).clicked() {
                (self.wh_graph.table, self.wh_graph.sig_browser) = (true, false);
            }
            if ui.menu_label(sigs, trf!("{icon}  Signatures", icon = icon::LIST_MAGNIFYING_GLASS)).on_hover_text(tr!("Every probe scan signature pasted, to search and clean up")).clicked() {
                self.wh_graph.sig_browser = true;
            }
            ui.add_space(8.0);
            if ui.button(trf!("{icon}  Add", icon = icon::PLUS)).on_hover_text(tr!("Enter a hole by hand")).clicked() {
                let form = self.wh_form_here();
                self.wh_form = Some(form);
            }
            let active = self.settings.wh_filter.active();
            let label = if active == 0 { trf!("{icon}  Filter", icon = icon::FUNNEL) } else { trf!("{icon}  Filter ({active})", icon = icon::FUNNEL, active = active) };
            let filter_btn = ui.button(label).on_hover_text(tr!("Which holes the map and the table show"));
            let mut filter_changed = false;
            egui::Popup::from_toggle_button_response(&filter_btn)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    filter_changed |= wh_filter_ui(ui, &mut self.settings.wh_filter);
                });
            if ui
                .add_enabled(active > 0, egui::Button::new(icon::FUNNEL_X))
                .on_hover_text(tr!("Clear the filter"))
                .on_disabled_hover_text(tr!("No filter set"))
                .clicked()
            {
                self.settings.wh_filter = Default::default();
                filter_changed = true;
            }
            let routes_changed = ui
                .checkbox(&mut self.settings.wh_route_filtered, tr!("Only filtered WH for routes"))
                .on_hover_text(tr!("Routes use only the holes the filter shows. The holes switched off by hand stay off either way."))
                .changed();
            if routes_changed || (filter_changed && self.settings.wh_route_filtered) {
                self.wh_routing_changed();
            } else if filter_changed {
                self.needs_save = true;
            }
            ui.add_space(8.0);
            let look = ui.add(
                egui::TextEdit::singleline(&mut self.wh_info_query).hint_text(tr!("System facts: J-name or system")).desired_width(200.0),
            );
            if look.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.wh_info = self.systems.as_ref().and_then(|g| g.lookup(self.wh_info_query.trim())).map(|i| i.id);
            }
            let mut changed = false;
            ui.menu_button(icon::GEAR_SIX, |ui| {
                changed |= ui
                    .checkbox(&mut self.settings.wh_detect, tr!("Record holes my characters go through"))
                    .on_hover_text(tr!("Watches how your signed-in characters move. A jump the gates cannot explain is recorded as a wormhole, or asked about when a filament, clone or capital jump fits too."))
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.wh_ask, tr!("Ask for the signature"))
                    .on_hover_text(tr!("A small window beside the EVE client asks for the signature, type and state of each hole"))
                    .changed();
                ui.horizontal(|ui| {
                    ui.label(tr!("Pinned systems join clusters within"));
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.settings.wh_pin_jumps).range(1..=crate::settings::WH_PIN_JUMPS_MAX))
                        .on_hover_text(tr!("Gate jumps from a cluster's nearest exit. Further out, a pinned system shows on its own."))
                        .changed();
                    ui.label(tr!("jumps"));
                });
                ui.separator();
                if ui.button(trf!("{icon}  Sharing…", icon = icon::USERS_THREE)).on_hover_text(tr!("Share wormholes with others, end-to-end encrypted")).clicked() {
                    self.wh_share.open = true;
                    ui.close();
                }
            })
            .response
            .on_hover_text(tr!("Wormhole settings"));
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
                if r.on_hover_text(trf!("{hover}\nClick for the sharing window", hover = hover)).clicked() {
                    self.wh_share.open = true;
                }
            }
            if let Some(names) = &missing {
                ui.label(egui::RichText::new(icon::WARNING).color(crate::theme::standing::WARNING)).on_hover_text(trf!("Sign in again with {names} to let deaths, clone jumps and bridges be told from wormholes.", names = names));
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
                ui.label(egui::RichText::new(tr!("No wormholes known yet.")).weak());
                ui.label(
                    egui::RichText::new(
                        tr!("Seeded from EVE-Scout (Thera/Turnur), intel channels, your own characters' jumps and what you add."),
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
        let shown: Vec<&crate::wormholes::Wormhole> = self.wh_cache.iter().filter(|w| self.wh_shown(w, now)).collect();
        let Some(geo) = self.systems.clone() else { return };
        let act = spai_ui::wh_tab::holes_table(
            ui,
            &geo,
            &shown,
            now,
            &|w| self.settings.wh_disabled_holes.contains(&w.uid),
            &|uid| self.wh_group_of.get(uid).and_then(|g| self.share_group_name(g)).map(str::to_owned),
        );
        if let Some(id) = act.open {
            self.open_system(id);
        }
        if let Some(id) = act.kill {
            self.kill_wormhole(id);
        }
        if let Some(id) = act.edit {
            self.wh_edit(id);
        }
        if act.info.is_some() {
            self.wh_info = act.info;
        }
        if let Some(uid) = act.toggle {
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
        // Adding a hole between two systems a live one already joins: that one, unless the user
        // says it is another.
        let twin = form.id.is_none().then(|| self.wh_twin(&form.system, &form.dest)).flatten();
        form.twin = twin.as_ref().map(crate::app::wh_prompt::twin_label);
        form.second &= form.twin.is_some();
        let (type_was, dest_was) = (form.wh_type.clone(), form.dest.clone());
        egui::Window::new(if form.id.is_some() { tr!("Edit wormhole") } else { tr!("Add wormhole") })
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
            // Only a different signature makes another hole between the same two systems.
            if let Some(known) = twin.as_ref().filter(|t| form.second && t.same_hole(&fresh)) {
                form.error = Some(trf!("Another hole needs a signature other than the known one's ({known}).", known = crate::app::wh_prompt::twin_label(known)));
                self.wh_form = Some(form);
                return;
            }
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
                    None if form.second => store.upsert_wormhole(&fresh),
                    None => match fresh.dest_system_id.and_then(|b| store.wormholes_between(fresh.system_id, b).into_iter().find(|w| w.same_hole(&fresh))) {
                        Some(mut row) => {
                            let facing = fresh.facing(&row);
                            crate::app::wh_prompt::fill_in(&mut row, facing);
                            row.seen_by |= Source::Manual.bit();
                            store.write_wormhole(&row);
                            row.id
                        }
                        None => store.upsert_wormhole(&fresh),
                    },
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

    /// The live hole joining the systems named, either way round.
    fn wh_twin(&self, system: &str, dest: &str) -> Option<crate::wormholes::Wormhole> {
        let geo = self.systems.as_ref()?;
        let (a, b) = (geo.lookup(system.trim())?.id, geo.lookup(dest.trim())?.id);
        let now = crate::clock::utc().timestamp();
        self.store.as_ref()?.wormhole_between(a, b).filter(|w| !w.is_expired(now))
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
            if ui.button(egui_phosphor::regular::X).on_hover_text(tr!("Close")).clicked() {
                close = true;
            }
        });
        ui.label(format!("{} \u{b7} {}", whdata::class_of(sys, info.security, &info.region).label().tr(), info.region));
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
pub(crate) use spai_ui::wh_tab::{wh_filter_ui, wh_system_facts};

/// The destination class of a hole whose far side is system `id`.
pub(crate) use spai_core::wormholes::dest_class;










/// All of `choices`, then each one by name and where it is. Returns who was picked.
pub(crate) fn pick_characters_menu(ui: &mut egui::Ui, choices: &[(String, Option<String>)]) -> Option<Vec<String>> {
    let mut picked = None;
    if choices.is_empty() {
        ui.label(egui::RichText::new(tr!("No character may set waypoints. Sign one in again to allow it.")).weak());
        return None;
    }
    if choices.len() > 1 && ui.button(trf!("{icon}  All characters", icon = egui_phosphor::regular::USERS)).clicked() {
        picked = Some(choices.iter().map(|(n, _)| n.clone()).collect());
        ui.close();
    }
    for (n, here) in choices {
        let label = match here {
            Some(sys) => format!("{n}  ({sys})"),
            None => n.clone(),
        };
        if ui.button(label).clicked() {
            picked = Some(vec![n.clone()]);
            ui.close();
        }
    }
    picked
}
