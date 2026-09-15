//! The intel, dashboard, lookup and characters views, the pilot window and the fit window.

use super::*;

impl SpaiApp {
    pub(crate) fn intel_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        if self.chat_dir.is_none() {
            ui.colored_label(
                crate::theme::standing::WARNING,
                "EVE chat logs not found. Set the logs directory in Settings.",
            );
            return;
        }

        let player_sys = self.player_system();
        let rings = self.char_rings();
        let systems = self.systems.clone();

        toolbar(ui, |ui| {
            use IntelTypeFilter::*;
            // The five options are one control, so they are grouped tight rather than spaced like
            // the independent controls that follow.
            let gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, 2.0);
            for (lbl, v) in [
                ("All", All),
                ("Hostile", Hostile),
                ("Clear", Clear),
                ("Kill", Kill),
                ("Threat", Threat),
            ] {
                if selectable_chip(ui, self.intel_type == v, lbl).clicked() {
                    self.intel_type = v;
                }
            }
            ui.spacing_mut().item_spacing.x = gap;
            toolbar_sep(ui);
            ui.add(
                egui::DragValue::new(&mut self.intel_max_jumps)
                    .range(0..=50)
                    .prefix("\u{2264} ")
                    .custom_formatter(|n, _| if n == 0.0 { "any".to_owned() } else { format!("{n}") }),
            )
            .on_hover_text(
                "Hide intel further than this many jumps from you. \"any\" keeps every distance.",
            );
            if ui
                .checkbox(&mut self.settings.intel_count_bridges, "jump bridges")
                .on_hover_text(
                    "Count your jump bridges in the card distances and the \u{2264} jumps filter. \
                     Off = gate-only, how far a hostile, who can't use your bridges, really is.",
                )
                .changed()
            {
                self.needs_save = true;
            }
            toolbar_sep(ui);
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.intel_ttl_secs)
                        .range(30..=3600)
                        .prefix(format!("{}  ", egui_phosphor::regular::CLOCK_COUNTDOWN))
                        .suffix("s"),
                )
                .on_hover_text("How long until intel is outdated")
                .changed()
            {
                self.needs_save = true;
            }
            toolbar_sep(ui);
            if ui
                .button(egui_phosphor::regular::PALETTE)
                .on_hover_text("Configure intel severity colours")
                .clicked()
            {
                self.severity_open = true;
            }
            if ui
                .button(egui_phosphor::regular::TAG)
                .on_hover_text("Pilot notes and tags")
                .clicked()
            {
                self.open_notes_manager(crate::notes::NoteKind::Pilot);
            }
            toolbar_sep(ui);
            if ui
                .checkbox(&mut self.settings.kill_intel, "zKill intel")
                .on_hover_text("Show zKill killmails within range as intel cards")
                .changed()
            {
                self.needs_save = true;
            }
            if self.settings.kill_intel
                && Self::kill_intel_range(ui, &mut self.settings.kill_intel_jumps).changed()
            {
                self.needs_save = true;
            }
            toolbar_sep(ui);
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add_sized(
                [
                    ui.available_rect_before_wrap().width().max(Self::INTEL_FILTER_MIN_W),
                    ui.spacing().interact_size.y,
                ],
                egui::TextEdit::singleline(&mut self.intel_query).hint_text(Self::INTEL_FILTER_HINT),
            );
        });
        ui.add_space(6.0);

        let now = chrono::Utc::now().timestamp();
        let query = self.intel_query.trim().to_lowercase();
        let type_filter = self.intel_type;
        let max_jumps = self.intel_max_jumps;
        let bridges = self.settings.intel_count_bridges;
        let sev_rules = self.settings.severity.clone();
        let notes_view = self.notes_view.clone();
        let state = self.intel_state.lock().unwrap();

        let mut matches: Vec<&crate::intel::IntelReport> = state
            .reports
            .iter()
            .filter(|r| r.primary_system().is_some() || !r.gates.is_empty())
            .filter(|r| type_filter.matches(r))
            .filter(|r| {
                max_jumps == 0
                    || jumps_from_you(&systems, player_sys, r.primary_system().map(|s| s.id), bridges)
                        .is_some_and(|j| j <= max_jumps)
            })
            .filter(|r| query.is_empty() || intel_query_matches(r, &query, &notes_view))
            .collect();
        matches.sort_by(|a, b| b.received.cmp(&a.received));
        let last_ship = build_last_ship(&state.reports);

        ui.label(egui::RichText::new(format!("{} reports", matches.len())).weak());
        ui.add_space(4.0);
        let filters_active = !query.is_empty()
            || type_filter != IntelTypeFilter::All
            || max_jumps != 0;
        if matches.is_empty() && filters_active {
            let mut clear = false;
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("No reports match the current filters.").weak(),
                );
                ui.add_space(8.0);
                clear = ui.button("Clear Filters").clicked();
            });
            if clear {
                self.intel_query.clear();
                self.intel_type = IntelTypeFilter::All;
                self.intel_max_jumps = 0;
            }
            return;
        }
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> = matches
            .iter()
            .flat_map(|r| r.ships.iter().map(|s| s.id))
            .collect::<std::collections::HashSet<i64>>()
            .into_iter()
            .filter_map(|id| self.ship_details_cached(id).map(|d| (id, d)))
            .collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> = matches
            .iter()
            .flat_map(|r| r.ships.iter().map(|s| s.id))
            .collect::<std::collections::HashSet<i64>>()
            .into_iter()
            .map(|id| (id, self.ship_roles_cached(id)))
            .collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp =
                cache.display_ids(matches.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let mut action: Option<IntelClick> = None;
        let ttl = self.settings.intel_ttl_secs;
        {
            let status = self.system_status.lock().unwrap();
            const CARD_CAP: usize = 250;
            // A tag chip changes a card's height without changing its report.
            if self.intel_heights.len() > 2000 || self.intel_heights_notes_rev != self.notes_view.rev {
                self.intel_heights_notes_rev = self.notes_view.rev;
                self.intel_heights.clear();
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(
                ui,
                |ui, viewport| {
                    let origin = ui.cursor().top();
                    for r in matches.iter().take(CARD_CAP) {
                        let y = ui.cursor().top() - origin;
                        let key = report_key(r);
                        if let Some(h) = self.intel_heights.get(&key).copied() {
                            if y + h < viewport.min.y - 400.0 || y > viewport.max.y + 400.0 {
                                ui.add_space(h);
                                continue;
                            }
                        }
                        let stale = state.is_stale(r) || (now - r.received) > ttl;
                        let target = r.primary_system().map(|s| s.id);
                        let from_you = jumps_from_you(&systems, player_sys, target, bridges);
                        let via = jump_via(&systems, player_sys, target, bridges, from_you);
                        let cchars = rings.card_for(r);
                        let sev = severity_of(r, &sev_rules);
                        let kc = self.kill_cache.clone();
                        let affil = self.affiliations.clone();
                        let inner = ui.scope(|ui| {
                            intel_row(
                                ui, r, now, stale, from_you, via, &cchars, &systems, &status, &ship_details,
                                &ship_roles, &resolved_pilots, &uncertain, &last_ship, &kc, sev, true,
                            &affil, &self.notes_view, false, &mut None,
                            )
                        });
                        if let Some(a) = inner.inner {
                            action = Some(a);
                        }
                        let h = inner.response.rect.height();
                        if h > 0.0 {
                            self.intel_heights.insert(key, h);
                        }
                    }
                    if matches.len() > CARD_CAP {
                        ui.label(
                            egui::RichText::new(format!("+{} older", matches.len() - CARD_CAP))
                                .weak(),
                        );
                    }
                },
            );
        }
        drop(state);
        if let Some(c) = action {
            self.act_on_intel_click(c, ui.ctx());
        }
    }

    pub(crate) fn open_pilot_verdict(&mut self, name: String) {
        if !self.settings.verdict_explained {
            self.verdict_explainer_open = true;
        }
        self.verdict_popup = Some(name);
    }

    pub(crate) fn apply_pilot_verdict(&mut self, name: &str, hidden: bool) {
        self.pilots.lock().unwrap_or_else(|e| e.into_inner()).set_verdict(name, hidden);
        if let Some(store) = &self.store {
            store.set_pilot_verdict(name, hidden);
        }
    }

    pub(crate) fn verdict_dialog(&mut self, ctx: &egui::Context) {
        if self.verdict_explainer_open {
            let mut ack = false;
            let resp = egui::Modal::new(egui::Id::new("verdict_explainer")).show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.heading("Uncertain pilot (?)");
                ui.add_space(4.0);
                ui.label(
                    "A \"?\" means this name matched a real EVE character, but that character looks \
                     inactive (no recent kills, corp move, or wide roaming). It may be a real but \
                     rarely-used pilot, or a chat word that happens to match a character name.",
                );
                ui.add_space(6.0);
                ui.label(
                    "Mark it \"Real pilot\" to keep it (the ? clears), or \"Not a pilot\" to hide it. \
                     Your choice is remembered.",
                );
                ui.add_space(8.0);
                if ui.button("Got it").clicked() {
                    ack = true;
                }
            });
            if ack || resp.should_close() {
                self.verdict_explainer_open = false;
                self.settings.verdict_explained = true;
                self.needs_save = true;
            }
            return;
        }
        let Some(name) = self.verdict_popup.clone() else {
            return;
        };
        let mut verdict: Option<bool> = None;
        let resp = egui::Modal::new(egui::Id::new("verdict_popup")).show(ctx, |ui| {
            ui.heading(format!("Is \"{name}\" a pilot?"));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("\"{name}\" matched a character that looks inactive."))
                    .weak(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Real pilot").clicked() {
                    verdict = Some(false);
                }
                if ui.button("Not a pilot (hide)").clicked() {
                    verdict = Some(true);
                }
            });
        });
        if let Some(hidden) = verdict {
            self.apply_pilot_verdict(&name, hidden);
            self.verdict_popup = None;
            ctx.request_repaint();
        } else if resp.should_close() {
            self.verdict_popup = None;
        }
    }

    pub(crate) fn render_intel_cards(
        &mut self,
        ui: &mut egui::Ui,
        reports: &[crate::intel::IntelReport],
    ) -> Option<IntelClick> {
        if reports.is_empty() {
            ui.label(egui::RichText::new("No intel in range.").weak());
            return None;
        }
        let now = chrono::Utc::now().timestamp();
        let player_sys = self.player_system();
        let rings = self.char_rings();
        let systems = self.systems.clone();
        let bridges = self.settings.intel_count_bridges;
        let sev_rules = self.settings.severity.clone();
        let ttl = self.settings.intel_ttl_secs;
        let ids: std::collections::HashSet<i64> =
            reports.iter().flat_map(|r| r.ships.iter().map(|s| s.id)).collect();
        let ship_details: std::collections::HashMap<i64, crate::store::ShipDetails> = ids
            .iter()
            .filter_map(|id| self.ship_details_cached(*id).map(|d| (*id, d)))
            .collect();
        let ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>> =
            ids.iter().map(|id| (*id, self.ship_roles_cached(*id))).collect();
        let (resolved_pilots, uncertain) = {
            let mut cache = self.pilots.lock().unwrap();
            let rp =
                cache.display_ids(reports.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
            let unc = uncertain_set(&cache, &rp);
            (rp, unc)
        };
        let last_ship = build_last_ship(reports);
        let kc = self.kill_cache.clone();
        let affil = self.affiliations.clone();
        let mut action = None;
        let state = self.intel_state.lock().unwrap();
        let status = self.system_status.lock().unwrap();
        for r in reports {
            let stale = state.is_stale(r) || (now - r.received) > ttl;
            let target = r.primary_system().map(|s| s.id);
            let from_you = jumps_from_you(&systems, player_sys, target, bridges);
            let via = jump_via(&systems, player_sys, target, bridges, from_you);
            let cchars = rings.card_for(r);
            let sev = severity_of(r, &sev_rules);
            let inner = ui.scope(|ui| {
                intel_row(
                    ui, r, now, stale, from_you, via, &cchars, &systems, &status, &ship_details, &ship_roles,
                    &resolved_pilots, &uncertain, &last_ship, &kc, sev, true,
                &affil, &self.notes_view, false, &mut None,
                )
            });
            if let Some(a) = inner.inner {
                action = Some(a);
            }
        }
        action
    }

    pub(crate) fn dashboard_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let now = chrono::Utc::now().timestamp();
        let player_sys = self.player_system();
        let systems = self.systems.clone();
        let bridges = self.settings.intel_count_bridges;

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&self.active_character).strong());
                match player_sys.and_then(|s| systems.as_ref().and_then(|sy| sy.info_of(s))) {
                    Some(info) => {
                        ui.label("in");
                        ui.label(security_badge(info.security));
                        ui.label(egui::RichText::new(&info.name).strong());
                        system_chips(ui, &systems, &self.system_status.lock().unwrap(), info.id);
                    }
                    None => {
                        ui.label(egui::RichText::new("location unknown").weak());
                    }
                }
            });
        });
        ui.add_space(6.0);

        let (intel_count, nearest) = {
            let state = self.intel_state.lock().unwrap();
            let live: Vec<&crate::intel::IntelReport> =
                state.reports.iter().filter(|r| !r.clear && !state.is_stale(r)).collect();
            let nearest = live
                .iter()
                .filter_map(|r| {
                    let id = r.primary_system()?.id;
                    let j = jumps_from_you(&systems, player_sys, Some(id), bridges)?;
                    Some((j, r.primary_system().unwrap().name.clone()))
                })
                .min_by_key(|(j, _)| *j);
            (live.len(), nearest)
        };
        let battle_count = self.battles.lock().unwrap().iter().filter(|b| b.kills >= 2).count();

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Live intel: {intel_count}"));
            ui.separator();
            if let Some((j, name)) = &nearest {
                ui.label("Nearest hostile:");
                ui.label(egui::RichText::new(name).strong());
                ui.label(egui::RichText::new(format!("({j}j)")).weak());
            } else {
                ui.label(egui::RichText::new("no nearby hostiles").weak());
            }
            ui.separator();
            if battle_count > 0 {
                if ui.link(format!("Battles: {battle_count}")).clicked() {
                    self.view = View::Battles;
                }
            } else {
                ui.label(format!("Battles: {battle_count}"));
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        ui.label(egui::RichText::new("Recent alerts").strong());
        let log = self.recent_alerts.lock().unwrap();
        if log.is_empty() {
            ui.label(egui::RichText::new("None.").weak());
        } else {
            for (t, text) in log.iter().rev().take(5) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{:>7}", fmt_age(now - t))).monospace().weak());
                    ui.label(text);
                });
            }
        }
    }

    pub(crate) fn add_lookup_names(&mut self, text: &str) {
        for line in text.lines() {
            let name = line.split('\t').next().unwrap_or(line).trim();
            if name.len() < 3 || name.len() > 37 {
                continue;
            }
            if self.lookup_tabs.iter().any(|t| t.eq_ignore_ascii_case(name)) {
                continue;
            }
            self.lookup_tabs.push(name.to_owned());
            if let Some(tx) = &self.lookup_tx {
                let _ = tx.send(name.to_owned());
            }
        }
        if !self.lookup_tabs.is_empty() {
            self.lookup_active = self.lookup_tabs.len() - 1;
        }
    }

    pub(crate) fn lookup_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let text = f
                .bytes
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_else(|| f.name.clone());
            self.add_lookup_names(&text);
        }

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Paste pilot names (one per line, e.g. the local member list) or drop them here.",
            )
            .weak(),
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.lookup_input)
                .hint_text("Pilot names, one per line…")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Look up", icon::MAGNIFYING_GLASS)).clicked()
                && !self.lookup_input.trim().is_empty()
            {
                let text = std::mem::take(&mut self.lookup_input);
                self.add_lookup_names(&text);
            }
            if !self.lookup_tabs.is_empty() && ui.button("Close all").clicked() {
                self.lookup_tabs.clear();
                self.lookup_active = 0;
            }
        });
        ui.separator();
        if self.lookup_tabs.is_empty() {
            ui.label(egui::RichText::new("No lookups yet.").weak());
            return;
        }

        let tabs = self.lookup_tabs.clone();
        let mut close: Option<usize> = None;
        egui::ScrollArea::horizontal().id_salt("lookup_tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, name) in tabs.iter().enumerate() {
                    let label = self
                        .lookup_cache
                        .lock()
                        .unwrap()
                        .get(&name.to_lowercase())
                        .and_then(|o| o.as_ref())
                        .filter(|inf| inf.found)
                        .map(|inf| inf.name.clone())
                        .unwrap_or_else(|| name.clone());
                    if ui.selectable_label(self.lookup_active == i, label).clicked() {
                        self.lookup_active = i;
                    }
                    if ui
                        .add(egui::Button::new(egui::RichText::new(icon::X).small()).frame(false))
                        .on_hover_text("Close tab")
                        .clicked()
                    {
                        close = Some(i);
                    }
                    ui.separator();
                }
            });
        });
        if let Some(i) = close {
            self.lookup_tabs.remove(i);
            if self.lookup_active >= self.lookup_tabs.len() {
                self.lookup_active = self.lookup_tabs.len().saturating_sub(1);
            }
        }
        ui.separator();

        let Some(name) = self.lookup_tabs.get(self.lookup_active).cloned() else { return };
        let info = self.lookup_cache.lock().unwrap().get(&name.to_lowercase()).cloned();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Overview, "Overview");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Kills, "Kills");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Solo, "Solo");
            ui.selectable_value(&mut self.pilot_tab, PilotTab::Losses, "Losses");
        });
        ui.separator();
        let feed = if self.pilot_tab != PilotTab::Overview {
            Some(
                self.feed_cache
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let s = std::sync::Arc::new(std::sync::Mutex::new(crate::lookup::LookupState::Idle));
                        crate::lookup::spawn_lookup(name.clone(), s.clone(), ui.ctx().clone());
                        s
                    })
                    .clone(),
            )
        } else {
            None
        };
        egui::ScrollArea::vertical().id_salt("lookup_body").show(ui, |ui| {
            if self.pilot_tab == PilotTab::Overview {
                match info {
                    None | Some(None) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("Looking up {name}..."));
                        });
                    }
                    Some(Some(inf)) if !inf.found => {
                        ui.label(format!("No character named \"{name}\" was found."));
                    }
                    Some(Some(inf)) => Self::lookup_profile(ui, &inf),
                }
                return;
            }
            match feed.map(|f| f.lock().unwrap().clone()) {
                Some(crate::lookup::LookupState::Done(report)) => {
                    let list = match self.pilot_tab {
                        PilotTab::Kills => &report.kills,
                        PilotTab::Solo => &report.solo,
                        _ => &report.losses,
                    };
                    self.km_list(ui, list, report.loading, true);
                }
                Some(crate::lookup::LookupState::Failed(e)) => {
                    ui.label(egui::RichText::new(e).weak());
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading killmails\u{2026}");
                    });
                }
            }
        });
    }

    pub(crate) fn lookup_profile(ui: &mut egui::Ui, info: &crate::charlookup::LookupInfo) {
        use egui_phosphor::regular as icon;
        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(eve_portrait_url(info.char_id, 72.0))
                    .fit_to_exact_size(egui::Vec2::splat(72.0)),
            );
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&info.name).strong().size(18.0));
                ui.horizontal(|ui| {
                    if let Some(aid) = info.alliance_id {
                        ui.add(
                            egui::Image::new(eve_alliance_logo_url(aid, 40.0))
                                .fit_to_exact_size(egui::Vec2::splat(40.0)),
                        )
                        .on_hover_text(if info.alliance.is_empty() {
                            "Alliance"
                        } else {
                            info.alliance.as_str()
                        });
                    }
                    if let Some(cid) = info.corp_id {
                        ui.add(
                            egui::Image::new(eve_corp_logo_url(cid, 40.0))
                                .fit_to_exact_size(egui::Vec2::splat(40.0)),
                        )
                        .on_hover_text(if info.corp.is_empty() {
                            "Corporation"
                        } else {
                            info.corp.as_str()
                        });
                    }
                });
            });
        });
        ui.separator();
        egui::Grid::new("lookup_stats").spacing([24.0, 4.0]).show(ui, |ui| {
            ui.label("Kills");
            ui.label(egui::RichText::new(info.ships_destroyed.to_string()).strong());
            ui.label("Losses");
            ui.label(info.ships_lost.to_string());
            ui.end_row();
            ui.label("ISK destroyed");
            ui.label(fmt_isk(info.isk_destroyed));
            ui.label("ISK lost");
            ui.label(fmt_isk(info.isk_lost));
            ui.end_row();
            ui.label("Danger");
            ui.label(format!("{}%", info.danger_ratio));
            ui.label("Gang");
            ui.label(format!("{}%", info.gang_ratio));
            ui.end_row();
        });
        if !info.top_ships.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Most-used ships").strong());
            ui.horizontal_wrapped(|ui| {
                for (id, name, kills) in &info.top_ships {
                    ui.add(
                        egui::Image::new(eve_type_icon_url(id, 28.0))
                            .fit_to_exact_size(egui::Vec2::splat(28.0)),
                    )
                    .on_hover_text(format!("{name}: {kills} kills"));
                }
            });
        }
        if !info.top_systems.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Most active systems").strong());
            ui.horizontal_wrapped(|ui| {
                for (sys, kills) in &info.top_systems {
                    ui.label(egui::RichText::new(format!("{sys} ({kills})")).weak());
                }
            });
        }
        ui.separator();
        if ui.button(format!("{}  Open on zKillboard", icon::ARROW_SQUARE_OUT)).clicked() {
            let _ = open::that(format!("https://zkillboard.com/character/{}/", info.char_id));
        }
    }

    pub(crate) fn refresh_characters(&mut self) {
        if let Some(store) = &self.store {
            self.characters = store.list_characters();
        }
        self.active_character = resolve_active_character(&self.active_character, &self.characters);
        self.remember_active_character();
    }

    /// Mirror the working selection into settings so it survives a restart. Runs every frame via
    /// `refresh_characters`, so it must only mark a save when the pick actually changed.
    pub(crate) fn remember_active_character(&mut self) {
        if self.settings.active_character != self.active_character {
            self.settings.active_character = self.active_character.clone();
            self.needs_save = true;
        }
    }

    pub(crate) fn start_login(&self, ctx: &egui::Context) {
        let client_id = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        let callback = non_empty_or(&self.settings.sso_callback, auth::DEFAULT_CALLBACK);
        let scopes = auth::DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect();
        if let Some(store) = &self.store {
            auth::spawn_login(
                client_id,
                callback,
                scopes,
                store.path().to_path_buf(),
                self.auth_status.clone(),
                ctx.clone(),
            );
        }
    }

    pub(crate) fn characters_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if selectable_chip(ui, !self.copy_settings.active, "Characters").clicked() {
                self.copy_settings.active = false;
            }
            if selectable_chip(ui, self.copy_settings.active, "Copy settings")
                .on_hover_text("Copy one character's EVE settings onto other characters")
                .clicked()
            {
                self.copy_settings.active = true;
                self.copy_settings.invalidate();
            }
        });
        ui.add_space(8.0);

        if self.copy_settings.active {
            let configured = self.settings.eve_settings_dir.clone();
            let logs = self.settings.eve_logs_dir.clone();
            let clients = self.eve_clients.clone();
            if let Some(store) = self.store.as_ref() {
                crate::copysettings::ui(
                    &mut self.copy_settings,
                    ui,
                    store,
                    &configured,
                    &logs,
                    &clients,
                );
            } else {
                ui.colored_label(
                    crate::theme::standing::WARNING,
                    "No database, cannot copy settings.",
                );
            }
            return;
        }

        match self.auth_status.lock().unwrap().clone() {
            AuthStatus::Waiting(msg) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(msg);
                });
            }
            AuthStatus::Success(name) => {
                ui.colored_label(
                    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
                    format!("Logged in as {name}"),
                );
            }
            AuthStatus::Failed(err) => {
                ui.colored_label(crate::theme::standing::WARNING, format!("Login failed: {err}"));
            }
            AuthStatus::Idle => {}
        }

        // Where the refresh token actually lives. Not a warning: the fallback works, and it is only
        // reached when the machine has no usable keychain. But it is a real difference from what the
        // app normally does with a credential, and not one the user chose, so it is said out loud.
        if crate::tokens::fallback_in_use() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!(
                    "{}  Saved logins are in EVE Spai's encrypted file, not the system keychain.",
                    egui_phosphor::regular::LOCK_KEY
                ))
                .color(crate::theme::standing::WARNING),
            )
            .on_hover_text(
                "No system keychain could be used on this machine, so the refresh token is \
                 encrypted with a key derived from this user account and this machine. It cannot be \
                 opened from another account, another machine, or a copy of the profile folder. It \
                 cannot protect against a program already running as you — neither can an unlocked \
                 keychain. Install or start a keychain provider and the token moves back on its own.",
            );
        }

        ui.add_space(6.0);
        if ui.button("Add character (EVE SSO)").clicked() {
            self.start_login(&ui.ctx().clone());
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);

        if self.characters.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No characters yet. Click \"Add character\" to log in with EVE SSO.",
                )
                .weak(),
            );
            return;
        }

        let now = chrono::Utc::now().timestamp();
        let mut remove: Option<i64> = None;
        let mut toggle: Option<(String, bool)> = None;
        let mut reauth = false;
        for c in &self.characters {
            let have: std::collections::HashSet<&str> = c.scopes.split(' ').collect();
            let scope_count = have.iter().filter(|s| !s.is_empty()).count();
            let missing: Vec<&str> =
                auth::DEFAULT_SCOPES.iter().copied().filter(|s| !have.contains(s)).collect();
            // The real question is whether the saved *login* still works, not whether the
            // twenty-minute access token happens to be fresh this second. The old check read the
            // latter, so a perfectly healthy character showed "token expired" between refreshes and
            // a dead one showed nothing until the next call happened to run.
            let problem = crate::esi::auth_problem(c.id);
            let token_ok = problem.is_none() && (c.expires_at > now || crate::tokens::load_refresh(c.id).is_some());
            let mut intel_on =
                !self.settings.intel_disabled_chars.iter().any(|d| d.eq_ignore_ascii_case(&c.name));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&c.name).strong());
                ui.label(egui::RichText::new(format!("· {scope_count} scopes")).weak());
                let (col, txt) = if token_ok {
                    (egui::Color32::from_rgb(0x5A, 0xC8, 0x6A), "signed in")
                } else if problem == Some(crate::esi::AuthProblem::NoKeychain) {
                    (crate::theme::standing::HOSTILE, "keychain unavailable")
                } else {
                    (crate::theme::standing::WARNING, "login expired")
                };
                ui.label(egui::RichText::new("·").weak());
                ui.label(egui::RichText::new(txt).color(col));
                if !missing.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("{} missing scopes", egui_phosphor::regular::WARNING))
                            .color(crate::theme::standing::WARNING),
                    )
                    .on_hover_text(format!("Re-auth to grant: {}", missing.join(", ")));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Remove").clicked() {
                        remove = Some(c.id);
                    }
                    if !missing.is_empty()
                        && ui
                            .button("Re-auth")
                            .on_hover_text("Log in again to grant the new scopes")
                            .clicked()
                    {
                        reauth = true;
                    }
                    if ui
                        .checkbox(&mut intel_on, "Alert")
                        .on_hover_text("Raise intel alerts while this character is active")
                        .changed()
                    {
                        toggle = Some((c.name.clone(), intel_on));
                    }
                });
            });
        }
        if reauth {
            self.start_login(&ui.ctx().clone());
        }
        if let Some((name, on)) = toggle {
            self.settings.intel_disabled_chars.retain(|d| !d.eq_ignore_ascii_case(&name));
            if !on {
                self.settings.intel_disabled_chars.push(name);
            }
            self.needs_save = true;
        }
        if let Some(id) = remove {
            if let Some(store) = &self.store {
                let _ = store.remove_character(id);
            }
            self.refresh_characters();
        }
    }

    pub(crate) fn ship_roles_cached(&self, id: i64) -> Vec<(&'static str, &'static str)> {
        match self.store.as_ref() {
            Some(s) => ship_roles_cached(s, &self.ship_roles_cache, id),
            None => Vec::new(),
        }
    }

    pub(crate) fn ship_details_cached(&self, id: i64) -> Option<crate::store::ShipDetails> {
        match self.store.as_ref() {
            Some(s) => ship_details_cached(s, &self.ship_cache, id),
            None => None,
        }
    }

    pub(crate) fn pilot_window(&mut self, ctx: &egui::Context) {
        use crate::lookup::LookupState;
        if !self.pilot_window_open {
            return;
        }
        let keep = Self::dialog_viewport(ctx, "pilot_window", "EVE Spai - Pilot", [440.0, 580.0], |ui| {
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.pilot_query)
                        .hint_text("Character name")
                        .desired_width(220.0),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button(format!("{}  Look up", egui_phosphor::regular::MAGNIFYING_GLASS)).clicked() || enter {
                    crate::lookup::spawn_lookup(
                        self.pilot_query.clone(),
                        self.pilot_lookup.clone(),
                        ui.ctx().clone(),
                    );
                }
            });
            ui.separator();

            let state = self.pilot_lookup.lock().unwrap().clone();
            match state {
                LookupState::Idle => {
                    ui.label(egui::RichText::new("Enter a pilot name.").weak());
                }
                LookupState::Loading(n) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Looking up {n}…"));
                    });
                }
                LookupState::Failed(e) => {
                    ui.colored_label(crate::theme::standing::WARNING, e);
                }
                LookupState::Done(report) => self.pilot_report_ui(ui, &report),
            }
        });
        if !keep {
            self.pilot_window_open = false;
        }
    }

    /// `show_system` is off for a list already scoped to one system, where naming it on every row is
    /// just noise.
    pub(crate) fn km_list(
        &mut self,
        ui: &mut egui::Ui,
        list: &[crate::lookup::Loss],
        loading: bool,
        show_system: bool,
    ) {
        if list.is_empty() {
            let msg = if loading { "Loading\u{2026}" } else { "Nothing in this category." };
            ui.label(egui::RichText::new(msg).weak());
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let mut clicked: Option<crate::lookup::Loss> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for l in list {
                let det = self.ship_details_cached(l.ship_type_id);
                let skip = det.as_ref().is_some_and(|d| {
                    d.group == "Capsule"
                        || d.group == "Corvette"
                        || matches!(
                            d.name.as_str(),
                            "Caldari Shuttle" | "Gallente Shuttle" | "Amarr Shuttle" | "Minmatar Shuttle"
                        )
                });
                if skip {
                    continue;
                }
                ui.horizontal(|ui| {
                    let url = eve_type_icon_url(l.ship_type_id, 26.0);
                    let img = ui.add(
                        egui::Image::new(url)
                            .fit_to_exact_size(egui::Vec2::splat(26.0))
                            .sense(egui::Sense::click()),
                    );
                    let ship = det.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "?".to_owned());
                    let age_s = human_ago(now - l.time);
                    // The fixed-width tail is laid out from the right, so the ship name gets whatever
                    // is left and truncates. Left to itself, a long name sets the row's minimum width
                    // and drags the whole side panel wider as the list loads.
                    let mut hit = img.on_hover_text("Show fit").clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("\u{2197}").on_hover_text("Open on zKillboard").clicked() {
                            let _ =
                                open::that(format!("https://zkillboard.com/kill/{}/", l.killmail_id));
                        }
                        ui.label(egui::RichText::new(age_s).weak());
                        if l.value > 0.0 {
                            ui.label(fmt_isk(l.value));
                        }
                        if show_system {
                            if let Some(sys) =
                                self.systems.as_ref().and_then(|g| g.info_of(l.system_id))
                            {
                                ui.label(egui::RichText::new(&sys.name).weak());
                            }
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let name = ui.add(
                                egui::Label::new(egui::RichText::new(&ship).strong())
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            );
                            hit |= name.on_hover_text(&ship).clicked();
                        });
                    });
                    if hit {
                        clicked = Some(l.clone());
                    }
                });
            }
        });
        if let Some(l) = clicked {
            self.fit_loss = Some(l);
        }
    }

    pub(crate) fn pilot_report_ui(&mut self, ui: &mut egui::Ui, report: &crate::lookup::PilotReport) {
        use egui_phosphor::regular as icon;
        let now = chrono::Utc::now().timestamp();
        let id = report.character_id;
        let profile = report.profile.as_ref();

        ui.horizontal(|ui| {
            ui.add(egui::Image::new(eve_portrait_url(id, 64.0)).fit_to_exact_size(egui::Vec2::splat(64.0)));
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&report.name).strong().size(18.0));
                    if report.loading {
                        ui.spinner();
                    }
                });
                match profile {
                    Some(p) => {
                        let org = |ui: &mut egui::Ui, logo: Option<String>, name: &str, none: &str| {
                            ui.horizontal(|ui| {
                                if let Some(url) = logo {
                                    ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(20.0)));
                                }
                                if name.is_empty() {
                                    ui.label(egui::RichText::new(none).weak());
                                } else {
                                    ui.label(name);
                                }
                            });
                        };
                        org(ui, p.alliance_id.map(|a| eve_alliance_logo_url(a, 20.0)), &p.alliance_name, "No alliance");
                        org(ui, p.corp_id.map(|c| eve_corp_logo_url(c, 20.0)), &p.corp_name, "Unknown corporation");
                        let mut facts = Vec::new();
                        if let Some(b) = p.birthday {
                            facts.push(format!("{} old, born {}", span_text(now - b), day_text(b)));
                        }
                        if let Some(sec) = p.security {
                            facts.push(format!("sec {sec:.1}"));
                        }
                        if !facts.is_empty() {
                            ui.label(egui::RichText::new(facts.join(" · ")).weak());
                        }
                    }
                    None if report.loading => {
                        ui.label(egui::RichText::new("Loading profile…").weak());
                    }
                    None => {
                        ui.label(egui::RichText::new("Profile unavailable").weak());
                    }
                }
            });
        });

        ui.horizontal_wrapped(|ui| {
            if ui.button(format!("{}  Notes and tags", icon::NOTE_PENCIL)).clicked() && id > 0 {
                self.note_editor_pending =
                    Some(crate::notes::Subject::Pilot { id, name: report.name.clone() });
            }
            if ui.button(format!("{}  zKillboard", icon::ARROW_SQUARE_OUT)).clicked() {
                let _ = open::that(format!("https://zkillboard.com/character/{id}/"));
            }
        });
        match NoteTip::of(&self.notes_view, self.notes_view.pilot(id)) {
            Some(n) => note_tip_ui(ui, &n),
            None => {
                ui.label(egui::RichText::new("No notes or tags.").weak());
            }
        }

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (pane, label) in [
                (PilotPane::Info, "Info".to_owned()),
                (PilotPane::Ships, "Ships".to_owned()),
                (PilotPane::Kills, format!("Kills ({})", report.kills.len())),
                (PilotPane::Solo, format!("Solo ({})", report.solo.len())),
                (PilotPane::Losses, format!("Losses ({})", report.losses.len())),
            ] {
                if selectable_chip(ui, self.pilot_pane == pane, label).clicked() {
                    self.pilot_pane = pane;
                }
            }
        });
        ui.separator();
        match self.pilot_pane {
            PilotPane::Kills => return self.km_list(ui, &report.kills, report.loading, true),
            PilotPane::Solo => return self.km_list(ui, &report.solo, report.loading, true),
            PilotPane::Losses => return self.km_list(ui, &report.losses, report.loading, true),
            PilotPane::Info => return self.pilot_info_pane(ui, report, now),
            PilotPane::Ships => {}
        }
        ui.horizontal(|ui| {
            ui.label("Sort:");
            ui.selectable_value(&mut self.pilot_sort, PilotSort::MostLost, "Most lost");
            ui.selectable_value(&mut self.pilot_sort, PilotSort::Recent, "Recent");
        });

        let mut agg: std::collections::HashMap<i64, (u32, i64)> = std::collections::HashMap::new();
        for l in &report.losses {
            let skip = self
                .ship_details_cached(l.ship_type_id)
                .map(|d| matches!(d.group.as_str(), "Capsule" | "Corvette" | "Shuttle"))
                .unwrap_or(false);
            if skip {
                continue;
            }
            let e = agg.entry(l.ship_type_id).or_insert((0, 0));
            e.0 += 1;
            e.1 = e.1.max(l.time);
        }
        let mut ships: Vec<(i64, u32, i64)> = agg.into_iter().map(|(id, (c, t))| (id, c, t)).collect();
        match self.pilot_sort {
            PilotSort::MostLost => ships.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2))),
            PilotSort::Recent => ships.sort_by(|a, b| b.2.cmp(&a.2)),
        }

        ui.add_space(4.0);
        if ships.is_empty() {
            ui.label(egui::RichText::new("No relevant losses.").weak());
            return;
        }
        egui::ScrollArea::vertical().id_salt("pilot_ships").auto_shrink([false, false]).show(ui, |ui| {
            for (ship_id, count, _) in ships {
                let name = self
                    .ship_details_cached(ship_id)
                    .map(|d| d.name)
                    .unwrap_or_else(|| "Other".to_owned());
                ui.horizontal(|ui| {
                    let url = eve_type_icon_url(ship_id, 24.0);
                    ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(24.0)));
                    if ui
                        .add(egui::Button::new(format!("{name}  ×{count}")).frame(false))
                        .on_hover_text("View fits")
                        .clicked()
                    {
                        self.fit_view = Some((ship_id, FitMode::Recent));
                    }
                });
            }
        });
    }

    /// zKillboard's summary and the corporation history, the two things worth knowing about a
    /// stranger before their kill list.
    pub(crate) fn pilot_info_pane(&mut self, ui: &mut egui::Ui, report: &crate::lookup::PilotReport, now: i64) {
        egui::ScrollArea::vertical().id_salt("pilot_info").auto_shrink([false, false]).show(ui, |ui| {
            ui.label(egui::RichText::new("zKillboard").strong());
            match &report.stats {
                Some(s) => {
                    egui::Grid::new("pilot_zk_stats").num_columns(4).spacing([18.0, 4.0]).show(ui, |ui| {
                        ui.label("Kills");
                        ui.label(egui::RichText::new(s.ships_destroyed.to_string()).strong());
                        ui.label("Losses");
                        ui.label(s.ships_lost.to_string());
                        ui.end_row();
                        ui.label("ISK killed");
                        ui.label(fmt_isk(s.isk_destroyed));
                        ui.label("ISK lost");
                        ui.label(fmt_isk(s.isk_lost));
                        ui.end_row();
                        ui.label("Danger");
                        ui.label(format!("{}%", s.danger_ratio));
                        ui.label("Gang");
                        ui.label(format!("{}%", s.gang_ratio));
                        ui.end_row();
                    });
                    if !s.top_ships.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Flies").weak());
                            for (tid, name, kills) in &s.top_ships {
                                ui.add(
                                    egui::Image::new(eve_type_icon_url(*tid, 28.0))
                                        .fit_to_exact_size(egui::Vec2::splat(28.0)),
                                )
                                .on_hover_text(format!("{name}: {kills} kills"));
                            }
                        });
                    }
                    if !s.top_systems.is_empty() {
                        let list: Vec<String> = s.top_systems.iter().map(|(n, k)| format!("{n} ({k})")).collect();
                        ui.label(egui::RichText::new(format!("Active in {}", list.join(", "))).weak());
                    }
                }
                None if report.loading => {
                    ui.label(egui::RichText::new("Loading…").weak());
                }
                None => {
                    ui.label(egui::RichText::new("No zKillboard record.").weak());
                }
            }

            ui.add_space(8.0);
            let history = report.profile.as_ref().map(|p| p.history.as_slice()).unwrap_or_default();
            ui.label(egui::RichText::new(format!("Employment history ({})", history.len())).strong());
            if history.is_empty() {
                ui.label(egui::RichText::new(if report.loading { "Loading…" } else { "Nothing known." }).weak());
            }
            for (i, e) in history.iter().enumerate() {
                let end = if i == 0 { now } else { history[i - 1].start };
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(eve_corp_logo_url(e.corp_id, 20.0)).fit_to_exact_size(egui::Vec2::splat(20.0)));
                    let name = if e.corp_name.is_empty() { format!("Corporation {}", e.corp_id) } else { e.corp_name.clone() };
                    ui.add(egui::Label::new(name).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("{} · {}", day_text(e.start), span_text(end - e.start))).weak());
                    });
                });
            }
        });
    }

    pub(crate) fn ensure_type_names(&self, ids: &[i64], ctx: &egui::Context) {
        let missing: Vec<i64> = {
            let names = self.type_names.lock().unwrap();
            ids.iter().copied().filter(|id| !names.contains_key(id)).collect()
        };
        if missing.is_empty() {
            return;
        }
        {
            let mut loading = self.type_names_loading.lock().unwrap();
            if *loading {
                return;
            }
            *loading = true;
        }
        let cache = self.type_names.clone();
        let loading = self.type_names_loading.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let resolved = crate::universe::lookup_names(&missing);
            cache.lock().unwrap().extend(resolved);
            *loading.lock().unwrap() = false;
            ctx.request_repaint();
        });
    }

    pub(crate) fn fit_window(&mut self, ctx: &egui::Context) {
        let (loss, ship_id, mode, has_mode) = if let Some(l) = self.fit_loss.clone() {
            let sid = l.ship_type_id;
            (Some(l), sid, FitMode::Recent, false)
        } else if let Some((ship_id, mode)) = self.fit_view {
            let l = {
                let state = self.pilot_lookup.lock().unwrap();
                match &*state {
                    crate::lookup::LookupState::Done(report) => pick_loss(report, ship_id, mode),
                    _ => None,
                }
            };
            (l, ship_id, mode, true)
        } else {
            return;
        };
        if let Some(l) = &loss {
            let mut ids: Vec<i64> = l.items.iter().map(|i| i.type_id).collect();
            ids.push(ship_id);
            self.ensure_type_names(&ids, ctx);
        }
        let ship_name = self.ship_details_cached(ship_id).map(|d| d.name).unwrap_or_default();
        let names = self.type_names.lock().unwrap().clone();
        let mut new_mode = mode;

        let keep = Self::dialog_viewport(ctx, "fit_window", "EVE Spai - Fit", [460.0, 620.0], |ui| {
            ui.horizontal(|ui| {
                let url = eve_type_icon_url(ship_id, 28.0);
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(28.0)));
                ui.heading(&ship_name);
            });
            if has_mode {
                ui.horizontal(|ui| {
                    ui.label("Fit:");
                    ui.selectable_value(&mut new_mode, FitMode::Recent, "Most recent");
                    ui.selectable_value(&mut new_mode, FitMode::MostUsed, "Most used");
                });
            }
            ui.separator();
            let Some(loss) = &loss else {
                ui.label(egui::RichText::new("No fit found.").weak());
                return;
            };

            egui::ScrollArea::vertical().max_height(330.0).auto_shrink([false, false]).show(ui, |ui| {
                use crate::lookup::Slot;
                let cargo = fit_cargo(loss);
                let section = |ui: &mut egui::Ui, title: &str, slot: Slot| {
                    let mods: Vec<&crate::lookup::Item> = loss
                        .items
                        .iter()
                        .filter(|i| crate::lookup::slot_of(i.flag) == slot && i.qty == 1)
                        .collect();
                    if mods.is_empty() {
                        return;
                    }
                    ui.label(egui::RichText::new(title).strong().color(ui.visuals().hyperlink_color));
                    for it in mods {
                        ui.label(names.get(&it.type_id).cloned().unwrap_or_else(|| "…".to_owned()));
                    }
                    ui.add_space(4.0);
                };
                section(ui, "High", Slot::High);
                section(ui, "Mid", Slot::Mid);
                section(ui, "Low", Slot::Low);
                section(ui, "Rigs", Slot::Rig);
                section(ui, "Subsystems", Slot::Subsystem);
                if !cargo.is_empty() {
                    ui.label(
                        egui::RichText::new("Cargo & drones").strong().color(ui.visuals().hyperlink_color),
                    );
                    for (tid, q) in &cargo {
                        let n = names.get(tid).cloned().unwrap_or_else(|| "…".to_owned());
                        if *q > 1 {
                            ui.label(format!("{n}  ×{q}"));
                        } else {
                            ui.label(n);
                        }
                    }
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Copy EFT").clicked() {
                    ui.ctx().copy_text(eft_string(&ship_name, loss, &names));
                }
                let has_char = self.active_character != "No character";
                ui.add_enabled_ui(has_char, |ui| {
                    if ui.button("Save Fit").on_hover_text("Save to your in-game fittings").clicked() {
                        use crate::lookup::Slot;
                        let mut items: Vec<(i64, i64, i64)> = loss
                            .items
                            .iter()
                            .filter(|i| {
                                !matches!(crate::lookup::slot_of(i.flag), Slot::Cargo | Slot::Other)
                                    && i.qty == 1
                            })
                            .map(|i| (i.type_id, i.flag, 1))
                            .collect();
                        for (tid, q) in fit_cargo(loss) {
                            items.push((tid, 5, q)); // flag 5 = cargo
                        }
                        let cid =
                            non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID).to_owned();
                        crate::esi::save_fitting(
                            cid,
                            self.active_character.clone(),
                            format!("{}'s {ship_name} Fit", self.active_character),
                            ship_id,
                            items,
                        );
                    }
                });
                let site = self.settings.fit_site.clone();
                if site.is_empty() {
                    ui.label("Open in:");
                    for (id, label) in FIT_SITES {
                        if ui.button(*label).clicked() {
                            self.settings.fit_site = (*id).to_owned();
                            self.needs_save = true;
                        }
                    }
                } else if ui.button(format!("Open in {}", site_label(&site))).clicked() {
                    let _ = open::that(fit_url(&site, loss));
                }
            });
        });

        if has_mode && new_mode != mode {
            self.fit_view = Some((ship_id, new_mode));
        } else if !keep {
            self.fit_view = None;
            self.fit_loss = None;
        }
    }
}
