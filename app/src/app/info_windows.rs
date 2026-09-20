//! System, constellation, region and ship information windows.

use super::*;

impl SpaiApp {
    pub(crate) fn system_info_body(&mut self, ui: &mut egui::Ui, id: i64, docked: bool) -> SystemInfoOut {
        let mut nav: Option<i64> = None;
        let mut show_on_map = false;
        let now = chrono::Utc::now().timestamp();

        let ttl = self.settings.intel_ttl_secs;
        let player_sys = self.player_system();
        let rings = self.char_rings();
        let (sys_reports, stale_flags, sys_last_ship): (
            Vec<crate::intel::IntelReport>,
            Vec<bool>,
            std::collections::HashMap<String, (i64, String, i64)>,
        ) = {
            let st = self.intel_state.lock().unwrap();
            let mut reps = Vec::new();
            let mut stale = Vec::new();
            for r in st.reports.iter().rev() {
                if r.systems.iter().any(|s| s.id == id) {
                    stale.push(st.is_stale(r) || (now - r.received) > ttl);
                    reps.push(r.clone());
                }
            }
            (reps, stale, build_last_ship(&st.reports))
        };
        let ship_ids: std::collections::HashSet<i64> =
            sys_reports.iter().flat_map(|r| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> =
            ship_ids.iter().filter_map(|&i| self.ship_details_cached(i).map(|d| (i, d))).collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ship_ids.iter().map(|&i| (i, self.ship_roles_cached(i))).collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp = cache
                .display_ids(sys_reports.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let status_snapshot = self.system_status.lock().unwrap().clone();
        let mut intel_click: Option<IntelClick> = None;
        let constellation = self.store.as_ref().and_then(|s| s.constellation_of_system(id));
        let region_loc = self.store.as_ref().and_then(|s| s.region_of_system(id));
        let mut open_const: Option<i64> = None;
        let mut open_region: Option<i64> = None;

        let Some(graph) = self.systems.clone() else {
            ui.label("SDE not ready.");
            return SystemInfoOut::default();
        };
        let Some(info) = graph.info_of(id).cloned() else {
            ui.label("Unknown system.");
            return SystemInfoOut::default();
        };

        {
            let status = self.system_status.lock().unwrap();
            let flags = status.get(&id).cloned().unwrap_or_default();
            let adm_color = |adm: f64| {
                if adm >= 5.0 {
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
                } else if adm >= 3.0 {
                    crate::theme::standing::WARNING
                } else {
                    crate::theme::standing::HOSTILE
                }
            };
            let mut edit_notes = false;
            ui.horizontal(|ui| {
                ui.label(security_badge(info.security));
                ui.heading(&info.name);
                let teal = egui::Color32::from_rgb(0x4D, 0xB6, 0xAC);
                let marked = self.settings.bookmarks.contains(&id);
                let icon = egui::RichText::new(egui_phosphor::regular::BOOKMARK_SIMPLE)
                    .size(18.0)
                    .color(if marked { teal } else { ui.visuals().weak_text_color() });
                if ui
                    .add(egui::Button::new(icon).frame(false))
                    .on_hover_text(if marked { "Remove bookmark" } else { "Bookmark this system" })
                    .clicked()
                {
                    if marked {
                        self.settings.bookmarks.retain(|&b| b != id);
                    } else {
                        self.settings.bookmarks.push(id);
                    }
                    self.needs_save = true;
                }
                let noted = self.notes_view.system(id).is_some();
                let pencil = egui::RichText::new(egui_phosphor::regular::NOTE_PENCIL)
                    .size(18.0)
                    .color(if noted { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() });
                if ui.add(egui::Button::new(pencil).frame(false)).on_hover_text("Notes and tags").clicked() {
                    edit_notes = true;
                }
                if docked {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(adm) = flags.adm {
                            ui.label(
                                egui::RichText::new(format!("ADM {adm:.1}")).color(adm_color(adm)).strong(),
                            )
                            .on_hover_text("Activity Defense Multiplier");
                        }
                        if let Some(aid) = flags.sov_alliance {
                            let url = eve_alliance_logo_url(aid, 28.0);
                            let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(28.0)));
                            if let Some(sov) = &flags.sov {
                                r.on_hover_text(sov);
                            }
                        }
                    });
                }
            });
            if edit_notes {
                self.note_editor_pending = Some(crate::notes::Subject::System(id));
            }
            match NoteTip::of(&self.notes_view, self.notes_view.system(id)) {
                Some(n) => note_tip_ui(ui, &n),
                None => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}  No notes or tags", egui_phosphor::regular::TAG)).weak());
                        if ui.link("Add").clicked() {
                            self.note_editor_pending = Some(crate::notes::Subject::System(id));
                        }
                    });
                }
            }
            if !docked && (flags.sov_alliance.is_some() || flags.adm.is_some()) {
                egui::Area::new(egui::Id::new("sys_sov"))
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        ui.vertical_centered(|ui| {
                            if let Some(aid) = flags.sov_alliance {
                                let url = eve_alliance_logo_url(aid, 64.0);
                                let r = ui.add(
                                    egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(64.0)),
                                );
                                if let Some(sov) = &flags.sov {
                                    r.on_hover_text(sov);
                                }
                            }
                            if let Some(adm) = flags.adm {
                                let col = if adm >= 5.0 {
                                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)
                                } else if adm >= 3.0 {
                                    crate::theme::standing::WARNING
                                } else {
                                    crate::theme::standing::HOSTILE
                                };
                                ui.label(egui::RichText::new(format!("ADM {adm:.1}")).color(col).strong())
                                    .on_hover_text(
                                        "Activity Defense Multiplier (ESI gives only the \
                                         total, not the military/industry/strategic split)",
                                    );
                            }
                        });
                    });
            }
            system_chips_ex(ui, &self.systems, &status, id, false, false);
            ui.horizontal_wrapped(|ui| {
                if let Some((cid, cname)) = &constellation {
                    if ui.link(egui::RichText::new(cname).weak()).clicked() {
                        open_const = Some(*cid);
                    }
                }
                if let Some(r) = region_loc {
                    ui.label(egui::RichText::new("‹").weak());
                    if ui.link(egui::RichText::new(&info.region).weak()).clicked() {
                        open_region = Some(r);
                    }
                }
            });
            let region_ids: Vec<i64> = self
                .store
                .as_ref()
                .and_then(|s| s.region_of_system(id).map(|r| s.region_systems(r)))
                .map(|v| v.into_iter().map(|m| m.id).collect())
                .unwrap_or_default();
            let avg = |sel: &dyn Fn(&crate::systemstatus::SysFlags) -> u32| -> f64 {
                if region_ids.is_empty() {
                    return 0.0;
                }
                let sum: u64 = region_ids.iter().filter_map(|s| status.get(s)).map(|f| sel(f) as u64).sum();
                sum as f64 / region_ids.len() as f64
            };
            let (aj, ak, an) = (avg(&|f| f.jumps), avg(&|f| f.ship_kills), avg(&|f| f.npc_kills));
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Last hour:").weak());
                let stat = |ui: &mut egui::Ui, label: &str, v: u32, avg: f64| {
                    let col = if avg > 0.0 && v as f64 >= 2.0 * avg {
                        crate::theme::standing::HOSTILE
                    } else if avg > 0.0 && v as f64 > avg {
                        crate::theme::standing::WARNING
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.label(egui::RichText::new(format!("{v} {label}")).color(col));
                };
                stat(ui, "jumps", flags.jumps, aj);
                stat(ui, "ship kills", flags.ship_kills, ak);
                stat(ui, "pod kills", flags.pod_kills, ak);
                stat(ui, "NPC kills", flags.npc_kills, an);
            });
        }
        self.camp_line(ui, info.id);
        if let Some(rp) = crate::rats::rat_profile(&info.region) {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{}  rats", egui_phosphor::regular::SKULL)).strong());
                ui.label(egui::RichText::new(rp.faction).strong());
            });
            ui.label(
                egui::RichText::new(format!(
                    "Deals {} / {}   ·   weak to {} / {}",
                    rp.deal[0], rp.deal[1], rp.weak[0], rp.weak[1]
                ))
                .weak(),
            )
            .on_hover_text("Tank against the damage they deal; deal the damage they're weak to.");
            if rp.ewar != "None" {
                ui.label(egui::RichText::new(format!("EWAR: {}", rp.ewar)).weak());
            }
        }
        self.wormhole_section(ui, id);
        let upgrades: Vec<&str> = self
            .settings
            .sov_upgrades
            .iter()
            .filter(|u| u.system.eq_ignore_ascii_case(&info.name))
            .flat_map(|u| split_upgrade_label(&u.upgrade))
            .collect();
        if !upgrades.is_empty() {
            ui.label(egui::RichText::new("Sov upgrades").weak());
            for u in upgrades {
                let (kind, level) = upgrade_info(u);
                let lcol = level_color(level);
                ui.horizontal(|ui| {
                    match kind {
                        UpgradeIcon::Glyph(g) => {
                            ui.label(egui::RichText::new(g).color(lcol).size(16.0));
                        }
                        UpgradeIcon::Mineral(tid) => {
                            let url = eve_type_icon_url(tid, 18.0);
                            ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(18.0, 18.0)));
                        }
                    }
                    ui.label(egui::RichText::new(u).color(crate::theme::standing::CORP));
                });
            }
        }
        let has_char = self.active_character != "No character";
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        let cname = self.active_character.clone();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Show on map").clicked() {
                show_on_map = true;
            }
            if ui.add_enabled(has_char, egui::Button::new("Set Destination")).clicked() {
                self.set_destination_esi(cid.clone(), cname.clone(), id);
                self.route_destination = Some(id);
                self.note_ingame_route();
            }
            if ui.add_enabled(has_char, egui::Button::new("Add Waypoint")).clicked() {
                crate::esi::set_waypoint(cid.clone(), cname.clone(), id, false);
            }
        });
        ui.separator();

        let state = self.intel_state.lock().unwrap();
        let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for r in &state.reports {
            if r.clear || state.is_stale(r) {
                continue;
            }
            for s in &r.systems {
                *counts.entry(s.id).or_default() += 1;
            }
        }
        drop(state);

        ui.label(egui::RichText::new("Neighbours").strong());
        ui.horizontal_wrapped(|ui| {
            for &nid in graph.neighbors(id) {
                if let Some(ni) = graph.info_of(nid) {
                    let cnt = counts.get(&nid).copied().unwrap_or(0);
                    let sec = (ni.security * 10.0).round() / 10.0;
                    let mut label = format!("{sec:.1} {}", ni.name);
                    if cnt > 0 {
                        label.push_str(&format!(" ({cnt})"));
                    }
                    let text = egui::RichText::new(label).color(security_color(ni.security)).strong();
                    let mut btn = egui::Button::new(text);
                    let cross_region = ni.region != info.region && !ni.region.is_empty();
                    let cross_const = ni.constellation != info.constellation;
                    if cross_region {
                        btn = btn.fill(ui.visuals().hyperlink_color.gamma_multiply(0.22));
                    } else if cross_const {
                        btn = btn.fill(ui.visuals().hyperlink_color.gamma_multiply(0.10));
                    }
                    let mut resp = ui.add(btn);
                    let arrow = egui_phosphor::regular::ARROW_RIGHT;
                    if cross_region {
                        resp = resp.on_hover_text(format!("{arrow} {} ({})", ni.constellation, ni.region));
                    } else if cross_const {
                        resp = resp.on_hover_text(format!("{arrow} {}", ni.constellation));
                    }
                    if cnt > 0 {
                        resp = resp.on_hover_text(format!("{cnt} active intel"));
                    }
                    if resp.clicked() {
                        nav = Some(nid);
                    }
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.menu_value(&mut self.system_kills_tab, false, "Intel");
            ui.menu_value(&mut self.system_kills_tab, true, "Recent kills");
        });
        ui.separator();
        // The window's list takes the rest of the window so it is never left half empty; the dock
        // already scrolls as a whole, where an unbounded list would push everything else away.
        let list_h = if docked { 280.0 } else { ui.available_height().max(120.0) };
        if self.system_kills_tab {
            let feed = self
                .system_kills_cache
                .entry(id)
                .or_insert_with(|| {
                    let s =
                        std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle));
                    crate::lookup::spawn_system_kills(id, s.clone(), ui.ctx().clone());
                    s
                })
                .clone();
            egui::ScrollArea::vertical().id_salt("syskills").max_height(list_h).auto_shrink([false, docked]).show(ui, |ui| {
                match feed.lock().unwrap().clone() {
                    crate::lookup::LookupState::Done(report) => {
                        self.km_list(ui, &report.kills, report.loading, false);
                    }
                    crate::lookup::LookupState::Failed(e) => {
                        ui.label(egui::RichText::new(e).weak());
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading kills\u{2026}");
                        });
                    }
                }
            });
        } else {
            egui::ScrollArea::vertical().id_salt("sysintel").max_height(list_h).auto_shrink([false, docked]).show(ui, |ui| {
                if sys_reports.is_empty() {
                    ui.label(egui::RichText::new("No recent intel.").weak());
                }
                for (i, r) in sys_reports.iter().enumerate() {
                    let target = r.primary_system().map(|s| s.id);
                    let bridges = self.settings.intel_count_bridges;
                    let from_you = jumps_from_you(&self.systems, player_sys, target, bridges);
                    let via = jump_via(&self.systems, player_sys, target, bridges, from_you);
                    let cchars = rings.card_for(r);
                    let sev = severity_of(r, &self.settings.severity);
                    let kc = self.kill_cache.clone();
                    let affil = self.affiliations.clone();
                    if let Some(c) = intel_row(
                        ui, r, now, stale_flags[i], from_you, via, &cchars, &self.systems,
                        &status_snapshot,
                        &ship_details, &ship_roles, &resolved_pilots, &uncertain, &sys_last_ship,
                        &kc, sev, false,
                    &affil, &self.notes_view, false, &mut None,
                    ) {
                        intel_click = Some(c);
                    }
                }
            });
        }
        SystemInfoOut { nav, show_on_map, intel_click, open_const, open_region }
    }

    pub(crate) fn apply_system_info_out(
        &mut self,
        out: SystemInfoOut,
        id: i64,
        ctx: &egui::Context,
        docked: bool,
    ) {
        if let Some(nid) = out.nav {
            if docked {
                self.map_docked_system = Some(nid);
            } else {
                self.system_window = Some(nid);
            }
        }
        if let Some(c) = out.open_const {
            self.constellation_window = Some(c);
            self.focus_window = Some(egui::ViewportId::from_hash_of("constellation_window"));
        }
        if let Some(r) = out.open_region {
            self.region_window = Some(r);
            self.focus_window = Some(egui::ViewportId::from_hash_of("region_window"));
        }
        if let Some(c) = out.intel_click {
            self.act_on_intel_click(c, ctx);
        }
        if out.show_on_map {
            self.view = View::Map;
            if let Some(r) = self.store.as_ref().and_then(|s| s.region_of_system(id)) {
                self.map_go(crate::map::MapView::Region(r));
            }
            self.map_focus = Some(id);
        }
    }

    pub(crate) fn system_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.system_window else {
            return;
        };
        let mut out = SystemInfoOut::default();
        let keep = Self::dialog_viewport(
            ctx,
            "system_window",
            "EVE Spai - System info",
            [470.0, 620.0],
            |ui| {
                out = self.system_info_body(ui, id, false);
            },
        );
        self.apply_system_info_out(out, id, ctx, false);
        if !keep {
            self.system_window = None;
        }
    }

    /// Only a colour the user actually set. No name-hashed fallback: on the map the logo's own mean
    /// colour is a better guess than a random hue.
    pub(crate) fn alliance_color_of(&self, name: &str) -> Option<egui::Color32> {
        self.settings
            .alliances
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .and_then(|a| a.color)
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
    }

    pub(crate) fn coalition_paint(c: &crate::settings::Coalition) -> egui::Color32 {
        c.color
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
            .unwrap_or_else(|| name_color(&c.name))
    }

    pub(crate) fn discover_sov_alliances(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.sov_discover_last < 3.0 {
            return;
        }
        self.sov_discover_last = now;
        let names: std::collections::HashSet<String> = {
            let st = self.system_status.lock().unwrap();
            st.values().filter_map(|f| f.sov.clone()).collect()
        };
        let mut added = false;
        for name in names {
            if !self.settings.alliances.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
                self.settings.alliances.push(crate::settings::AllianceConfig { name, color: None });
                added = true;
            }
        }
        if added {
            self.settings.alliances.sort_by(|a, b| a.name.cmp(&b.name));
            self.needs_save = true;
        }
    }

    pub(crate) fn dominant_alliances(&self, ids: &[i64]) -> Vec<(i64, Option<String>, usize)> {
        let status = self.system_status.lock().unwrap();
        let mut counts: std::collections::HashMap<i64, (Option<String>, usize)> =
            std::collections::HashMap::new();
        for &id in ids {
            if let Some(f) = status.get(&id) {
                if let Some(aid) = f.sov_alliance {
                    let e = counts.entry(aid).or_insert((f.sov.clone(), 0));
                    e.1 += 1;
                    if e.0.is_none() {
                        e.0 = f.sov.clone();
                    }
                }
            }
        }
        let mut v: Vec<(i64, Option<String>, usize)> =
            counts.into_iter().map(|(k, (n, c))| (k, n, c)).collect();
        v.sort_by(|a, b| b.2.cmp(&a.2));
        v.truncate(3);
        v
    }

    pub(crate) fn dominant_logos(&self, ui: &mut egui::Ui, ids: &[i64], area_id: &str) {
        let dom = self.dominant_alliances(ids);
        if dom.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new(area_id.to_owned()))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    for (i, (aid, name, count)) in dom.iter().enumerate() {
                        let sz = if i == 0 { 56.0 } else { 34.0 };
                        let url = eve_alliance_logo_url(aid, sz);
                        let r = ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(sz)));
                        let label = name.clone().unwrap_or_else(|| "Alliance".to_owned());
                        r.on_hover_text(format!("{label} — {count} systems"));
                    }
                });
            });
    }

    pub(crate) fn rat_line(ui: &mut egui::Ui, region_name: &str) {
        if let Some(rp) = crate::rats::rat_profile(region_name) {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{}  rats", egui_phosphor::regular::SKULL)).strong());
                ui.label(egui::RichText::new(rp.faction).strong());
            });
            ui.label(
                egui::RichText::new(format!(
                    "Deals {} / {}   ·   weak to {} / {}",
                    rp.deal[0], rp.deal[1], rp.weak[0], rp.weak[1]
                ))
                .weak(),
            );
            if rp.ewar != "None" {
                ui.label(egui::RichText::new(format!("EWAR: {}", rp.ewar)).weak());
            }
        }
    }

    pub(crate) fn constellation_window(&mut self, ctx: &egui::Context) {
        let Some(cid) = self.constellation_window else { return };
        let Some(store) = &self.store else { return };
        let name = store.constellation_name(cid).unwrap_or_else(|| "Constellation".to_owned());
        let region = store.region_of_constellation(cid);
        let region_name = region.and_then(|r| store.region_name(r)).unwrap_or_default();
        let systems = store.constellation_systems(cid);
        let neighbours = store.constellation_neighbours(cid);
        let sys_ids: Vec<i64> = systems.iter().map(|s| s.id).collect();

        let mut open_region: Option<i64> = None;
        let mut open_constellation: Option<i64> = None;
        let mut open_system: Option<i64> = None;
        let keep = Self::dialog_viewport(
            ctx,
            "constellation_window",
            "EVE Spai - Constellation",
            [420.0, 560.0],
            |ui| {
                ui.heading(&name);
                if let Some(r) = region {
                    if ui.link(egui::RichText::new(format!("◤ {region_name}")).weak()).clicked() {
                        open_region = Some(r);
                    }
                }
                self.dominant_logos(ui, &sys_ids, "constellation_dom");
                Self::rat_line(ui, &region_name);
                if !neighbours.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new("Neighbouring constellations").strong());
                    ui.horizontal_wrapped(|ui| {
                        for (nid, nname) in &neighbours {
                            if ui.button(nname).clicked() {
                                open_constellation = Some(*nid);
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(egui::RichText::new(format!("Systems ({})", systems.len())).strong());
                let h = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("const_sys")
                    .auto_shrink([false, false])
                    .max_height(h)
                    .show(ui, |ui| {
                        for s in &systems {
                            ui.horizontal(|ui| {
                                ui.label(security_badge(s.security));
                                if ui.link(&s.name).clicked() {
                                    open_system = Some(s.id);
                                }
                            });
                        }
                    });
            },
        );
        if let Some(r) = open_region {
            self.region_window = Some(r);
            self.focus_window = Some(egui::ViewportId::from_hash_of("region_window"));
        }
        if let Some(c) = open_constellation {
            self.constellation_window = Some(c);
        }
        if let Some(s) = open_system {
            self.open_system(s);
        }
        if !keep {
            self.constellation_window = None;
        }
    }

    pub(crate) fn region_window(&mut self, ctx: &egui::Context) {
        let Some(rid) = self.region_window else { return };
        let Some(store) = &self.store else { return };
        let name = store.region_name(rid).unwrap_or_else(|| "Region".to_owned());
        let constellations = store.constellations_in_region(rid);
        let neighbours = store.region_neighbours(rid);
        let sys_ids: Vec<i64> = store.region_systems(rid).iter().map(|s| s.id).collect();

        let mut open_constellation: Option<i64> = None;
        let mut open_region: Option<i64> = None;
        let mut show_map = false;
        let keep = Self::dialog_viewport(
            ctx,
            "region_window",
            "EVE Spai - Region",
            [420.0, 580.0],
            |ui| {
                ui.heading(&name);
                self.dominant_logos(ui, &sys_ids, "region_dom");
                Self::rat_line(ui, &name);
                ui.separator();
                if ui.button("Show on map").clicked() {
                    show_map = true;
                }
                if !neighbours.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new("Neighbouring regions").strong());
                    ui.horizontal_wrapped(|ui| {
                        for (nid, nname) in &neighbours {
                            if ui.button(nname).clicked() {
                                open_region = Some(*nid);
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(egui::RichText::new(format!("Constellations ({})", constellations.len())).strong());
                let h = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("region_const")
                    .auto_shrink([false, false])
                    .max_height(h)
                    .show(ui, |ui| {
                        for (cid, cname) in &constellations {
                            if ui.link(cname).clicked() {
                                open_constellation = Some(*cid);
                            }
                        }
                    });
            },
        );
        if let Some(c) = open_constellation {
            self.constellation_window = Some(c);
            self.focus_window = Some(egui::ViewportId::from_hash_of("constellation_window"));
        }
        if let Some(r) = open_region {
            self.region_window = Some(r);
        }
        if show_map {
            self.view = View::Map;
            self.map_go(crate::map::MapView::Region(rid));
        }
        if !keep {
            self.region_window = None;
        }
    }

    pub(crate) fn ship_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.ship_window else {
            return;
        };
        let details = self.store.as_ref().and_then(|s| s.ship_details(id));
        let traits = self.store.as_ref().map(|s| s.ship_traits(id)).unwrap_or_default();
        let roles = derive_roles(&traits);
        let skill_ids: Vec<i64> = {
            let mut s: Vec<i64> = traits.iter().map(|t| t.0).filter(|&s| s > 0).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        self.ensure_type_names(&skill_ids, ctx);
        let names = self.type_names.lock().unwrap().clone();
        let keep = Self::dialog_viewport(ctx, "ship_window", "EVE Spai - Ship", [380.0, 600.0], |ui| {
            ui.horizontal(|ui| {
                let url = eve_type_render_url(id, 96.0);
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(96.0)));
                ui.vertical(|ui| {
                    match &details {
                        Some(d) => {
                            ui.heading(&d.name);
                            let size = hull_size(&d.group);
                            let type_line = if size.is_empty() || d.group.contains(size) {
                                d.group.clone()
                            } else {
                                format!("{size} · {}", d.group)
                            };
                            ui.label(egui::RichText::new(type_line).weak());
                        }
                        None => {
                            ui.heading("Ship");
                            ui.label(egui::RichText::new("No SDE details.").weak());
                        }
                    }
                    role_badges(ui, &roles);
                });
            });
            ui.separator();
            if let Some(d) = &details {
                ship_stats(ui, d);
            }
            if !traits.is_empty() {
                ui.separator();
                let fmt = |bonus: f64, text: &str| {
                    if bonus != 0.0 {
                        format!("• {bonus:.0}% {text}")
                    } else {
                        format!("• {text}")
                    }
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .id_salt("ship_traits")
                    .show(ui, |ui| {
                        let mut skills: Vec<i64> = Vec::new();
                        for (s, _, _) in &traits {
                            if *s > 0 && !skills.contains(s) {
                                skills.push(*s);
                            }
                        }
                        for skill in &skills {
                            let sname =
                                names.get(skill).cloned().unwrap_or_else(|| "…".to_owned());
                            ui.label(egui::RichText::new(format!("{sname} (per level)")).strong());
                            ui.indent(*skill, |ui| {
                                for (s, bonus, text) in &traits {
                                    if s == skill {
                                        ui.label(fmt(*bonus, text));
                                    }
                                }
                            });
                        }
                        let role: Vec<&(i64, f64, String)> =
                            traits.iter().filter(|t| t.0 == -1).collect();
                        if !role.is_empty() {
                            ui.label(egui::RichText::new("Role Bonuses").strong());
                            ui.indent("trait_role", |ui| {
                                for (_, bonus, text) in role {
                                    ui.label(fmt(*bonus, text));
                                }
                            });
                        }
                    });
            }
        });
        if !keep {
            self.ship_window = None;
        }
    }
}
