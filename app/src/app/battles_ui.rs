//! Battles: the battle list, filters, edits, review panels, report import and export, and sharing.

use super::*;

impl SpaiApp {
    pub(crate) fn battle_filter_dialog(&mut self, ctx: &egui::Context) {
        use crate::settings::{BattleCond, RuleAction, ShipSize};
        use egui_phosphor::regular as icon;
        if !self.battle_filter_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(ctx, "battle_filter", "EVE Spai - Battle rules", [580.0, 620.0], |ui| {
            ui.label(
                egui::RichText::new(
                    "Battles near your intel are shown by default. Add rules to include or exclude \
                     others. The first rule that matches a battle wins.",
                )
                .weak(),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let rules = &mut self.settings.battles.rules;
                let n = rules.len();
                let mut delete: Option<usize> = None;
                let mut swap: Option<(usize, usize)> = None;
                for (i, rule) in rules.iter_mut().enumerate() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut act = rule.action;
                            egui::ComboBox::from_id_salt(("br_act", i))
                                .selected_text(if act == RuleAction::Include { "Include" } else { "Exclude" })
                                .width(84.0)
                                .show_ui(ui, |ui| {
                                    changed |= ui.menu_value(&mut act, RuleAction::Include, "Include").changed();
                                    changed |= ui.menu_value(&mut act, RuleAction::Exclude, "Exclude").changed();
                                });
                            rule.action = act;
                            let mut all = rule.match_all;
                            egui::ComboBox::from_id_salt(("br_all", i))
                                .selected_text(if all { "All of" } else { "Any of" })
                                .width(72.0)
                                .show_ui(ui, |ui| {
                                    changed |= ui.menu_value(&mut all, true, "All of").changed();
                                    changed |= ui.menu_value(&mut all, false, "Any of").changed();
                                });
                            rule.match_all = all;
                            if rule.is_broad() {
                                ui.label(egui::RichText::new(icon::WARNING).color(egui::Color32::from_rgb(0xE0, 0xB0, 0x4C)))
                                    .on_hover_text("Matches anywhere in EVE and can store a lot of battle history. Add a region/constellation/system/jumps or participant condition to bound it.");
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(icon::TRASH).clicked() {
                                    delete = Some(i);
                                }
                                if i + 1 < n && ui.button(icon::ARROW_DOWN).clicked() {
                                    swap = Some((i, i + 1));
                                }
                                if i > 0 && ui.button(icon::ARROW_UP).clicked() {
                                    swap = Some((i, i - 1));
                                }
                            });
                        });
                        let mut del_cond: Option<usize> = None;
                        for (j, cond) in rule.conditions.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("br_ck", i, j))
                                    .selected_text(cond.kind_label())
                                    .width(140.0)
                                    .show_ui(ui, |ui| {
                                        for k in BattleCond::kinds() {
                                            if ui.menu_label(cond.kind_label() == k.kind_label(), k.kind_label()).clicked()
                                                && cond.kind_label() != k.kind_label()
                                            {
                                                *cond = k;
                                                changed = true;
                                            }
                                        }
                                    });
                                match cond {
                                    BattleCond::IntelArea => {
                                        ui.label(egui::RichText::new("(default tracked area)").weak());
                                    }
                                    BattleCond::Coalition(s)
                                    | BattleCond::Alliance(s)
                                    | BattleCond::Corporation(s)
                                    | BattleCond::Player(s)
                                    | BattleCond::Region(s)
                                    | BattleCond::Constellation(s)
                                    | BattleCond::System(s)
                                    | BattleCond::ShipType(s) => {
                                        changed |= ui
                                            .add(egui::TextEdit::singleline(s).desired_width(200.0).hint_text("name"))
                                            .changed();
                                    }
                                    BattleCond::JumpsFromMe(nn) => {
                                        changed |= ui.add(egui::DragValue::new(nn).range(0..=100).suffix(" jumps")).changed();
                                    }
                                    BattleCond::HullSizeAtLeast(sz) => {
                                        egui::ComboBox::from_id_salt(("br_sz", i, j))
                                            .selected_text(sz.label())
                                            .show_ui(ui, |ui| {
                                                for opt in ShipSize::CHOICES {
                                                    changed |= ui.menu_value(sz, opt, opt.label()).changed();
                                                }
                                            });
                                    }
                                    BattleCond::IskAtLeast(v) | BattleCond::IskAtMost(v) => {
                                        let mut m = *v / 1e6;
                                        if ui
                                            .add(egui::DragValue::new(&mut m).speed(50.0).range(0.0..=1e9).suffix("M ISK"))
                                            .changed()
                                        {
                                            *v = m * 1e6;
                                            changed = true;
                                        }
                                    }
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(icon::X).on_hover_text("Remove condition").clicked() {
                                        del_cond = Some(j);
                                    }
                                });
                            });
                            if matches!(
                                cond,
                                BattleCond::Alliance(_)
                                    | BattleCond::Corporation(_)
                                    | BattleCond::Player(_)
                                    | BattleCond::ShipType(_)
                            ) {
                                ui.label(
                                    egui::RichText::new(
                                        "   pairs with a region/coalition/hull condition to pull in new battles",
                                    )
                                    .weak()
                                    .size(11.0),
                                );
                            }
                        }
                        if let Some(j) = del_cond {
                            rule.conditions.remove(j);
                            changed = true;
                        }
                        if ui.button(format!("{}  condition", icon::PLUS)).clicked() {
                            rule.conditions.push(BattleCond::Region(String::new()));
                            changed = true;
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some((a, b)) = swap {
                    rules.swap(a, b);
                    changed = true;
                }
                if let Some(i) = delete {
                    rules.remove(i);
                    changed = true;
                }
                if ui.button(format!("{}  Add rule", icon::PLUS)).clicked() {
                    rules.push(crate::settings::BattleRule::default());
                    changed = true;
                }
                ui.add_space(8.0);
                ui.separator();
                if self.battle_filter_confirm_reset {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Replace all rules with the default?").strong());
                        if ui.button("Restore").clicked() {
                            *rules = crate::settings::BattleFilter::default_rules();
                            changed = true;
                            self.battle_filter_confirm_reset = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.battle_filter_confirm_reset = false;
                        }
                    });
                } else if ui.button("Restore defaults").clicked() {
                    self.battle_filter_confirm_reset = true;
                }
            });
        });
        if !keep {
            self.battle_filter_open = false;
            self.battle_filter_confirm_reset = false;
        }
        if changed {
            self.needs_save = true;
            self.battle_filter_gen_shared.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *self.battle_filter.lock().unwrap() = self.settings.battles.clone();
        }
    }


    pub(crate) fn open_dscan(&mut self, url: String, ctx: &egui::Context) {
        if let Some(view) = open_dscan_view(url, self.ship_index.clone(), ctx) {
            self.dscan_view = Some(view);
        }
    }

    pub(crate) fn dscan_view_dialog(&mut self, ctx: &egui::Context) {
        let mut open_ship: Option<i64> = None;
        dscan_view_dialog_ui(ctx, &mut self.dscan_view, false, &mut open_ship);
        if let Some(id) = open_ship {
            self.open_ship(id);
        }
    }

    pub(crate) fn load_battle_history(&self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        if self.battle_history_loading.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(systems) = self.systems.clone() else {
            self.battle_history_loading.store(false, Ordering::SeqCst);
            return;
        };
        let out = self.battle_history.clone();
        let loading = self.battle_history_loading.clone();
        let break_gap = self.settings.battle_break_secs;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let battles = crate::store::Store::open()
                .ok()
                .map(|s| {
                    // Bounded by the retention window: the whole table can be hundreds of MB of
                    // JSON, and the prune that bounds it runs asynchronously.
                    let since = chrono::Utc::now().timestamp()
                        - crate::store::ENGAGEMENT_RETENTION_SECS;
                    let engs = s.load_engagements(since);
                    let overrides = s.load_battle_overrides();
                    br_core::battle::cluster(
                        &engs,
                        br_core::battle::BATTLE_WINDOW_SECS,
                        br_core::battle::BATTLE_MAX_JUMPS,
                        break_gap,
                        &overrides,
                        |a, b| systems.jumps(a, b, br_core::battle::BATTLE_MAX_JUMPS),
                    )
                    .into_iter()
                    .filter(|b| b.is_anchored() && b.is_two_sided())
                    .collect()
                })
                .unwrap_or_default();
            *out.lock().unwrap() = battles;
            loading.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    pub(crate) fn apply_battle_edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&crate::store::Store)) {
        {
            let Some(store) = &self.store else { return };
            f(store);
            let fresh = store.load_battle_overrides();
            *self.battle_overrides.lock().unwrap() = fresh;
            self.battle_excluded_count = store.count_excluded();
            self.battle_scrub_count = store.count_scrubs();
        }
        self.battle_overrides_gen_shared
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.battle_detail_cache = None;
        if self.show_history {
            self.load_battle_history(ctx);
        }
    }

    pub(crate) fn battle_edit_view(&mut self, ui: &mut egui::Ui, now: i64) {
        use egui_phosphor::regular as icon;
        let ctx = ui.ctx().clone();
        let mut engs: Vec<br_core::battle::Engagement> = self
            .battle_detail_cache
            .as_ref()
            .map(|c| c.battle.engagements.clone())
            .unwrap_or_default();
        engs.sort_by_key(|e| e.time);
        let splits = self
            .battle_detail_cache
            .as_ref()
            .map(|c| c.battle.suggested_splits.clone())
            .unwrap_or_default();
        if engs.is_empty() {
            ui.label(egui::RichText::new("No kills to edit.").weak());
            return;
        }
        let ship_ids: Vec<i64> = engs.iter().map(|e| e.victim_ship).filter(|&i| i != 0).collect();
        self.ensure_type_names(&ship_ids, &ctx);
        let names: std::collections::HashMap<i64, String> = {
            let g = self.type_names.lock().unwrap();
            ship_ids
                .iter()
                .map(|&id| (id, g.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"))))
                .collect()
        };
        let name_of = |id: i64| -> String {
            if id == 0 {
                return "?".to_owned();
            }
            crate::intel::structure_name_by_type(id)
                .map(|s| s.to_owned())
                .or_else(|| names.get(&id).cloned())
                .unwrap_or_else(|| format!("Type {id}"))
        };
        let break_gap = self.settings.battle_break_secs;

        let mut do_exclude: Option<i64> = None;
        let mut open_sys: Option<i64> = None;
        let mut purge_pilot: Option<i64> = None;
        let mut do_split = false;

        if !splits.is_empty() {
            let ship_count = |pred: &dyn Fn(&br_core::battle::Engagement) -> bool| -> usize {
                let mut set: std::collections::HashSet<i64> = std::collections::HashSet::new();
                for e in engs.iter().filter(|e| pred(e)) {
                    if e.victim_char != 0 {
                        set.insert(e.victim_char);
                    }
                    for a in &e.attackers {
                        if a.char_id != 0 {
                            set.insert(a.char_id);
                        }
                    }
                }
                set.len()
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Suggested splits:").strong());
                for sug in &splits {
                    let boundary = sug.time;
                    let before_ships = ship_count(&|e| e.time < boundary);
                    let after_ships = ship_count(&|e| e.time >= boundary);
                    let before_kills = engs.iter().filter(|e| e.time < boundary).count();
                    let after_kills = engs.len() - before_kills;
                    let split_before = before_ships < after_ships
                        || (before_ships == after_ships && before_kills < after_kills);
                    let off = before_ships.min(after_ships);
                    let hhmm = chrono::DateTime::from_timestamp(boundary, 0)
                        .map(|t| t.format("%H:%M").to_string())
                        .unwrap_or_default();
                    let reason = sug.reason.label();
                    if ui
                        .button(format!("{} Split off {off} ships: {reason} ({hhmm})", icon::SCISSORS))
                        .on_hover_text(format!(
                            "Suggested because of a {reason} at {hhmm}. Selects the smaller group ({off} ships) to split off."
                        ))
                        .clicked()
                    {
                        self.battle_kill_sel = engs
                            .iter()
                            .filter(|e| if split_before { e.time < boundary } else { e.time >= boundary })
                            .map(|e| e.kill_id)
                            .collect();
                    }
                }
            });
            ui.add_space(6.0);
        }

        let sel_ids = self.battle_kill_sel.clone();
        if sel_ids.is_empty() {
            self.battle_split_preview = None;
        }
        if !sel_ids.is_empty() {
            let stale = self
                .battle_split_preview
                .as_ref()
                .map(|(s, _, _)| s != &sel_ids)
                .unwrap_or(true);
            if stale {
                let sel: Vec<br_core::battle::Engagement> =
                    engs.iter().filter(|e| sel_ids.contains(&e.kill_id)).cloned().collect();
                let rest: Vec<br_core::battle::Engagement> =
                    engs.iter().filter(|e| !sel_ids.contains(&e.kill_id)).cloned().collect();
                let pa = br_core::battle::preview_battle(sel, break_gap);
                let pb = br_core::battle::preview_battle(rest, break_gap);
                self.battle_split_preview = Some((sel_ids.clone(), pa, pb));
            }
            let (pa, pb) = self
                .battle_split_preview
                .as_ref()
                .map(|(_, a, b)| (a, b))
                .unwrap();
            let rest_empty = pb.engagements.is_empty();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(format!("Split preview: {} kills selected", sel_ids.len())).strong(),
                );
                ui.horizontal_wrapped(|ui| {
                    battle_preview_summary(ui, "Split off", pa);
                    ui.separator();
                    battle_preview_summary(ui, "Remaining", pb);
                });
                ui.horizontal(|ui| {
                    if rest_empty {
                        ui.label(
                            egui::RichText::new("Leave at least one kill behind.")
                                .color(crate::theme::standing::WARNING),
                        );
                    } else if ui
                        .button(format!("{} Split off {} kills", icon::SCISSORS, sel_ids.len()))
                        .on_hover_text("Tag both halves so they cluster as two separate battles")
                        .clicked()
                    {
                        do_split = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.battle_kill_sel.clear();
                    }
                });
            });
            ui.add_space(6.0);
        }

        let avail_h = (ui.available_height() - 8.0).max(160.0);
        egui::ScrollArea::vertical().id_salt("battle_edit_kills").max_height(avail_h).show(ui, |ui| {
            for e in &engs {
                let mut sel = self.battle_kill_sel.contains(&e.kill_id);
                egui::Frame::new()
                    .fill(if sel {
                        crate::theme::standing::WARNING.gamma_multiply(0.12)
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut sel, "").changed() {
                                if sel {
                                    self.battle_kill_sel.insert(e.kill_id);
                                } else {
                                    self.battle_kill_sel.remove(&e.kill_id);
                                }
                            }
                            ui.label(
                                egui::RichText::new(format!("{:>7}", fmt_age(now - e.time))).monospace().weak(),
                            );
                            hull_badge(ui, e.victim_ship, 22.0);
                            ui.label(egui::RichText::new(name_of(e.victim_ship)).strong());
                            ui.label(&e.victim_pilot);
                            if ui
                                .link(egui::RichText::new(&e.system_name).weak())
                                .on_hover_text("Open system info")
                                .clicked()
                            {
                                open_sys = Some(e.system_id);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .button(egui_phosphor::regular::TRASH)
                                    .on_hover_text("Remove this kill from battle reports")
                                    .clicked()
                                {
                                    do_exclude = Some(e.kill_id);
                                }
                                ui.label(egui::RichText::new(fmt_isk(e.isk)).weak());
                            });
                        });
                    });
                ui.add_space(2.0);
            }

            ui.add_space(6.0);
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{} Pilots", egui_phosphor::regular::USERS)).strong(),
            )
            .id_salt("battle_edit_pilots")
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Purge removes a pilot from this battle: their losses are excluded and their \
                         attacker entries scrubbed.",
                    )
                    .weak(),
                );
                let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
                let mut pilots: Vec<(i64, String)> = Vec::new();
                for e in &engs {
                    if e.victim_char != 0 && seen.insert(e.victim_char) {
                        pilots.push((e.victim_char, e.victim_pilot.clone()));
                    }
                    for a in &e.attackers {
                        if a.char_id != 0 && seen.insert(a.char_id) {
                            pilots.push((a.char_id, a.pilot.clone()));
                        }
                    }
                }
                pilots.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
                for (char_id, pilot) in &pilots {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(pilot).strong());
                        if ui
                            .button(format!("{} Remove pilot", egui_phosphor::regular::USER_MINUS))
                            .on_hover_text("Exclude their losses and scrub their attacker entries here")
                            .clicked()
                        {
                            purge_pilot = Some(*char_id);
                        }
                    });
                }
                if pilots.is_empty() {
                    ui.label(egui::RichText::new("No identified pilots.").weak());
                }
            });
        });

        if let Some(sid) = open_sys {
            self.open_system(sid);
        }
        if let Some(kid) = do_exclude {
            self.apply_battle_edit(&ctx, |s| s.set_battle_excluded(kid, true));
        }
        if let Some(p) = purge_pilot {
            let engs2 = engs.clone();
            self.apply_battle_edit(&ctx, move |s| {
                for e in &engs2 {
                    if e.victim_char == p {
                        s.set_battle_excluded(e.kill_id, true);
                    } else if e.attackers.iter().any(|a| a.char_id == p) {
                        s.set_scrub(e.kill_id, p, true);
                    }
                }
            });
        }
        if do_split {
            let selected: Vec<i64> =
                engs.iter().map(|e| e.kill_id).filter(|k| sel_ids.contains(k)).collect();
            let rest_ids: Vec<i64> =
                engs.iter().map(|e| e.kill_id).filter(|k| !sel_ids.contains(k)).collect();
            self.apply_battle_edit(&ctx, move |s| {
                let ta = s.next_battle_tag();
                for kid in &selected {
                    s.set_battle_tag(*kid, Some(ta));
                }
                let tb = ta + 1;
                for kid in &rest_ids {
                    s.set_battle_tag(*kid, Some(tb));
                }
            });
            self.battle_kill_sel.clear();
            self.battle_edit_mode = false;
            self.battle_selected = None;
            self.battle_detail_cache = None;
        }
    }

    pub(crate) fn battle_review_panels(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        let now = chrono::Utc::now().timestamp();

        if self.battle_excluded_open {
            let list = self.store.as_ref().map(|s| s.list_excluded_engagements()).unwrap_or_default();
            let ids: Vec<i64> = list.iter().map(|e| e.victim_ship).filter(|&i| i != 0).collect();
            self.ensure_type_names(&ids, ctx);
            let names: std::collections::HashMap<i64, String> = {
                let g = self.type_names.lock().unwrap();
                ids.iter()
                    .map(|&id| (id, g.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"))))
                    .collect()
            };
            let hull = |id: i64| -> String {
                crate::intel::structure_name_by_type(id)
                    .map(|s| s.to_owned())
                    .or_else(|| names.get(&id).cloned())
                    .unwrap_or_else(|| "?".to_owned())
            };
            let mut open = true;
            let mut restore: Option<i64> = None;
            egui::Window::new(format!("{} Excluded kills", icon::TRASH))
                .open(&mut open)
                .resizable(true)
                .default_width(460.0)
                .show(ctx, |ui| {
                    if list.is_empty() {
                        ui.label(egui::RichText::new("No excluded kills.").weak());
                    }
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for e in &list {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:>7}", fmt_age(now - e.time)))
                                        .monospace()
                                        .weak(),
                                );
                                hull_badge(ui, e.victim_ship, 20.0);
                                ui.label(egui::RichText::new(hull(e.victim_ship)).strong());
                                ui.label(&e.victim_pilot);
                                ui.label(egui::RichText::new(&e.system_name).weak());
                                ui.label(egui::RichText::new(fmt_isk(e.isk)).weak());
                                if ui
                                    .button(format!("{} Restore", icon::ARROW_COUNTER_CLOCKWISE))
                                    .clicked()
                                {
                                    restore = Some(e.kill_id);
                                }
                            });
                        }
                    });
                });
            self.battle_excluded_open = open;
            if let Some(kid) = restore {
                self.apply_battle_edit(ctx, |s| s.set_battle_excluded(kid, false));
            }
        }

        if self.battle_scrubs_open {
            let list = self.store.as_ref().map(|s| s.list_scrubs()).unwrap_or_default();
            let mut open = true;
            let mut restore: Option<(i64, i64)> = None;
            egui::Window::new(format!("{} Scrubbed pilots", icon::BROOM))
                .open(&mut open)
                .resizable(true)
                .default_width(360.0)
                .show(ctx, |ui| {
                    if list.is_empty() {
                        ui.label(egui::RichText::new("No scrubbed pilots.").weak());
                    }
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (kill_id, char_id) in &list {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("kill {kill_id}")).monospace().weak());
                                ui.label(format!("char {char_id}"));
                                if ui
                                    .button(format!("{} Restore", icon::ARROW_COUNTER_CLOCKWISE))
                                    .clicked()
                                {
                                    restore = Some((*kill_id, *char_id));
                                }
                            });
                        }
                    });
                });
            self.battle_scrubs_open = open;
            if let Some((kill_id, char_id)) = restore {
                self.apply_battle_edit(ctx, |s| s.set_scrub(kill_id, char_id, false));
            }
        }

        if self.battle_add_open {
            let target_kids: Vec<i64> = self
                .battle_detail_cache
                .as_ref()
                .map(|c| c.battle.engagements.iter().map(|e| e.kill_id).collect())
                .unwrap_or_default();
            let existing_tag = {
                let o = self.battle_overrides.lock().unwrap();
                target_kids.iter().find_map(|k| o.tag.get(k).copied())
            };
            let known: std::collections::HashSet<i64> = {
                let mut s = std::collections::HashSet::new();
                for src in [&self.battles, &self.battle_history] {
                    for b in src.lock().unwrap().iter() {
                        for e in &b.engagements {
                            s.insert(e.kill_id);
                        }
                    }
                }
                s
            };
            let mut rows = self.store.as_ref().map(|s| s.load_kill_intel(now - 86_400)).unwrap_or_default();
            rows.reverse();
            rows.truncate(300);
            let ship_ids: Vec<i64> = rows.iter().map(|r| r.2).filter(|&i| i != 0).collect();
            self.ensure_type_names(&ship_ids, ctx);
            let names: std::collections::HashMap<i64, String> = {
                let g = self.type_names.lock().unwrap();
                ship_ids
                    .iter()
                    .map(|&id| (id, g.get(&id).cloned().unwrap_or_else(|| format!("Type {id}"))))
                    .collect()
            };
            let systems = self.systems.clone();
            let sys_name = |id: i64| -> String {
                systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| format!("Sys {id}"))
            };
            let hull = |id: i64| -> String {
                crate::intel::structure_name_by_type(id)
                    .map(|s| s.to_owned())
                    .or_else(|| names.get(&id).cloned())
                    .unwrap_or_else(|| "?".to_owned())
            };

            let mut open = true;
            let mut add_kid: Option<i64> = None;
            let mut link_input = std::mem::take(&mut self.battle_add_link);
            egui::Window::new(format!("{} Add kill to battle", icon::PLUS))
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("zKill link");
                        ui.add(
                            egui::TextEdit::singleline(&mut link_input)
                                .hint_text("https://zkillboard.com/kill/…")
                                .desired_width(300.0),
                        );
                        if ui.button("Add").clicked() {
                            if let Some(k) =
                                crate::intel::extract_links(&link_input).into_iter().find_map(|l| l.kill_id)
                            {
                                add_kid = Some(k);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Recent kills (last 24h). Pick one to attach to this battle.")
                            .weak(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "A kill not yet in a battle is tagged now and will appear when fetched.",
                        )
                        .weak(),
                    );
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (kill_id, system_id, ship_type_id, time, value) in &rows {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:>7}", fmt_age(now - time)))
                                        .monospace()
                                        .weak(),
                                );
                                hull_badge(ui, *ship_type_id, 20.0);
                                ui.label(egui::RichText::new(hull(*ship_type_id)).strong());
                                ui.label(egui::RichText::new(sys_name(*system_id)).weak());
                                ui.label(egui::RichText::new(fmt_isk(*value)).weak());
                                if known.contains(kill_id) {
                                    ui.label(
                                        egui::RichText::new("in a BR")
                                            .color(crate::theme::standing::WARNING),
                                    )
                                    .on_hover_text("Already part of a clustered battle");
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(format!("{} Add", icon::PLUS)).clicked() {
                                            add_kid = Some(*kill_id);
                                        }
                                    },
                                );
                            });
                        }
                    });
                });
            self.battle_add_link = link_input;
            self.battle_add_open = open;
            if let Some(nk) = add_kid {
                let tids = target_kids.clone();
                self.apply_battle_edit(ctx, move |s| {
                    let tag = existing_tag.unwrap_or_else(|| s.next_battle_tag());
                    if existing_tag.is_none() {
                        for k in &tids {
                            s.set_battle_tag(*k, Some(tag));
                        }
                    }
                    s.set_battle_tag(nk, Some(tag));
                });
                self.battle_add_queue.lock().unwrap().push(nk);
                self.battle_add_link.clear();
            }
        }
    }

    pub(crate) fn save_battle_report(
        &self,
        battle: &br_core::battle::Battle,
    ) -> anyhow::Result<Option<std::path::PathBuf>> {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(battle_file_name(battle))
            .add_filter("EVE Spai battle report", &["json"])
            .save_file()
        else {
            return Ok(None);
        };
        let overrides = self.battle_overrides.lock().unwrap().clone();
        let now = chrono::Utc::now().timestamp();
        let ship_names = self.battle_ship_names(battle);
        let affiliations = self.battle_affiliations(battle);
        let doc = br_core::battle::BattleReportDoc::new(
            battle.clone(),
            battle.engagements.clone(),
            overrides,
            None,
            now,
            ship_names,
            affiliations,
        );
        std::fs::write(&path, doc.to_json()?)?;
        Ok(Some(path))
    }

    pub(crate) fn br_authed_chars(&self) -> Vec<(i64, String)> {
        self.characters
            .iter()
            .filter(|c| crate::tokens::load_refresh(c.id).is_some())
            .map(|c| (c.id, c.name.clone()))
            .collect()
    }

    pub(crate) fn share_identity(&self) -> Option<(i64, std::path::PathBuf)> {
        let path = self.store.as_ref()?.path().to_path_buf();
        let authed = |id: i64| crate::tokens::load_refresh(id).is_some();
        let id = self
            .br_character
            .filter(|id| authed(*id))
            .or_else(|| {
                self.characters
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(&self.active_character) && authed(c.id))
                    .map(|c| c.id)
            })
            .or_else(|| self.characters.iter().map(|c| c.id).find(|id| authed(*id)))?;
        Some((id, path))
    }

    pub(crate) fn battle_ship_names(
        &self,
        battle: &br_core::battle::Battle,
    ) -> std::collections::BTreeMap<i64, String> {
        let mut ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for e in &battle.engagements {
            ids.insert(e.victim_ship);
            for a in &e.attackers {
                ids.insert(a.ship);
            }
        }
        for i in 0..battle.sides.len() {
            for p in battle.roster(i) {
                ids.insert(p.ship);
                if let Some(l) = &p.lost {
                    ids.insert(l.pod_ship);
                }
            }
        }
        ids.remove(&0);
        let type_names = self.type_names.lock().unwrap();
        ids.into_iter()
            .filter_map(|id| type_names.get(&id).map(|n| (id, n.clone())))
            .collect()
    }

    pub(crate) fn battle_affiliations(
        &self,
        battle: &br_core::battle::Battle,
    ) -> std::collections::BTreeMap<i64, br_core::battle::Affil> {
        let mut ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for e in &battle.engagements {
            ids.insert(e.victim_char);
            for a in &e.attackers {
                ids.insert(a.char_id);
            }
        }
        for i in 0..battle.sides.len() {
            for p in battle.roster(i) {
                ids.insert(p.char_id);
            }
        }
        ids.remove(&0);
        let cache = self.affiliations.lock().unwrap();
        ids.into_iter()
            .filter_map(|id| {
                let a = cache.get(id)?;
                let corp_id = a.corp?;
                Some((
                    id,
                    br_core::battle::Affil {
                        corp_id,
                        corp_name: a.corp_name.unwrap_or_default(),
                        alliance_id: a.alliance.unwrap_or(0),
                        alliance_name: a.alliance_name.unwrap_or_default(),
                    },
                ))
            })
            .collect()
    }

    pub(crate) fn build_share_doc(&self, battle: &br_core::battle::Battle) -> br_core::battle::BattleReportDoc {
        let overrides = self.battle_overrides.lock().unwrap().clone();
        let now = chrono::Utc::now().timestamp();
        let ship_names = self.battle_ship_names(battle);
        let affiliations = self.battle_affiliations(battle);
        br_core::battle::BattleReportDoc::new(
            battle.clone(),
            battle.engagements.clone(),
            overrides,
            None,
            now,
            ship_names,
            affiliations,
        )
    }

    pub(crate) fn start_share(&mut self, battle: &br_core::battle::Battle, ctx: &egui::Context) {
        match self.share_identity() {
            Some((char_id, path)) => {
                let doc = self.build_share_doc(battle);
                crate::brshare::spawn_share(
                    doc,
                    path,
                    char_id,
                    self.br_unlisted,
                    self.br_share.clone(),
                    ctx.clone(),
                );
            }
            None => {
                *self.br_share.lock().unwrap() =
                    crate::brshare::ShareStatus::Error("Log in to share (opening EVE SSO…).".into());
                self.start_login(ctx);
            }
        }
    }

    pub(crate) fn open_my_shared(&mut self, ctx: &egui::Context) {
        self.br_mine_open = true;
        match self.share_identity() {
            Some((char_id, path)) => {
                crate::brshare::spawn_load_mine(path, char_id, self.br_mine.clone(), ctx.clone());
            }
            None => {
                self.br_mine.lock().unwrap().status =
                    crate::brshare::MineStatus::Error("Log in to see your shared reports.".into());
            }
        }
    }

    pub(crate) fn share_status_ui(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        enum Action {
            None,
            Dismiss,
            Delete(String),
        }
        let mut action = Action::None;
        {
            let state = self.br_share.lock().unwrap();
            match &*state {
                crate::brshare::ShareStatus::Idle => return,
                crate::brshare::ShareStatus::Uploading => {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Sharing to eve-spai.com…");
                    });
                }
                crate::brshare::ShareStatus::Done { id, url } => {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}  Shared:", icon::SHARE_NETWORK))
                                .color(egui::Color32::from_rgb(0x5A, 0xC8, 0x6A)),
                        );
                        ui.hyperlink(url);
                        if ui.button(format!("{} Copy", icon::COPY)).clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                        if ui.button(format!("{} Open", icon::GLOBE)).clicked() {
                            let _ = open::that(url);
                        }
                        if ui.button(format!("{} Delete", icon::TRASH)).clicked() {
                            action = Action::Delete(id.clone());
                        }
                        if ui.button(icon::X).on_hover_text("Dismiss").clicked() {
                            action = Action::Dismiss;
                        }
                    });
                }
                crate::brshare::ShareStatus::Error(e) => {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(crate::theme::standing::WARNING, e);
                        if ui.button(icon::X).on_hover_text("Dismiss").clicked() {
                            action = Action::Dismiss;
                        }
                    });
                }
            }
        }
        match action {
            Action::None => {}
            Action::Dismiss => {
                *self.br_share.lock().unwrap() = crate::brshare::ShareStatus::Idle;
            }
            Action::Delete(id) => {
                if let Some((char_id, path)) = self.share_identity() {
                    crate::brshare::spawn_delete_share(
                        path,
                        char_id,
                        id,
                        self.br_share.clone(),
                        ui.ctx().clone(),
                    );
                }
            }
        }
    }

    pub(crate) fn my_shared_window(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        if !self.br_mine_open {
            return;
        }
        let mut open = true;
        let base = crate::brshare::api_base();
        let mut reload = false;
        let mut delete_id: Option<String> = None;
        egui::Window::new(format!("{}  My shared BRs", icon::SHARE_NETWORK))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Refresh", icon::ARROWS_CLOCKWISE)).clicked() {
                        reload = true;
                    }
                });
                ui.add_space(4.0);
                let state = self.br_mine.lock().unwrap();
                if let Some(msg) = &state.msg {
                    ui.colored_label(crate::theme::standing::WARNING, msg);
                    ui.add_space(4.0);
                }
                match &state.status {
                    crate::brshare::MineStatus::Idle => {}
                    crate::brshare::MineStatus::Loading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading your shared reports…");
                        });
                    }
                    crate::brshare::MineStatus::Error(e) => {
                        ui.colored_label(crate::theme::standing::WARNING, e);
                    }
                    crate::brshare::MineStatus::Loaded(rows) if rows.is_empty() => {
                        ui.label(egui::RichText::new("You haven't shared any battle reports yet.").weak());
                    }
                    crate::brshare::MineStatus::Loaded(rows) => {
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            for r in rows {
                                egui::Frame::new()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(egui::Margin::symmetric(8, 6))
                                    .corner_radius(4.0)
                                    .show(ui, |ui| {
                                        let title = r.title.clone().unwrap_or_else(|| {
                                            r.systems.first().cloned().unwrap_or_else(|| "Battle report".into())
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(title).strong());
                                            if r.unlisted == Some(true) {
                                                ui.label(
                                                    egui::RichText::new("unlisted")
                                                        .color(crate::theme::standing::WARNING),
                                                );
                                            }
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .button(format!("{} Delete", icon::TRASH))
                                                        .clicked()
                                                    {
                                                        delete_id = Some(r.id.clone());
                                                    }
                                                    if ui
                                                        .button(format!("{} Open", icon::GLOBE))
                                                        .clicked()
                                                    {
                                                        let _ = open::that(r.url(&base));
                                                    }
                                                },
                                            );
                                        });
                                        ui.horizontal_wrapped(|ui| {
                                            if !r.systems.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(r.systems.join(", ")).weak(),
                                                );
                                                ui.label(egui::RichText::new("·").weak());
                                            }
                                            if let Some(d) = r
                                                .started_at
                                                .as_deref()
                                                .and_then(|s| {
                                                    chrono::DateTime::parse_from_rfc3339(s).ok()
                                                })
                                            {
                                                ui.label(
                                                    egui::RichText::new(
                                                        d.format("%Y-%m-%d %H:%M").to_string(),
                                                    )
                                                    .weak(),
                                                );
                                                ui.label(egui::RichText::new("·").weak());
                                            }
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} kills · {:.1}B ISK · {} {} views",
                                                    r.kills,
                                                    r.total_isk / 1e9,
                                                    icon::EYE,
                                                    r.views
                                                ))
                                                .weak(),
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                        });
                    }
                }
            });
        if let Some(id) = delete_id {
            if let Some((char_id, path)) = self.share_identity() {
                crate::brshare::spawn_delete_mine(
                    path,
                    char_id,
                    id,
                    self.br_mine.clone(),
                    ctx.clone(),
                );
            }
        }
        if reload {
            self.open_my_shared(ctx);
        }
        if !open {
            self.br_mine_open = false;
        }
    }

    pub(crate) fn load_battle_report(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let parsed = std::fs::read_to_string(path)
            .map_err(anyhow::Error::from)
            .and_then(|s| br_core::battle::BattleReportDoc::from_json(&s));
        match parsed {
            Ok(doc) => {
                let b = if doc.engagements.is_empty() {
                    doc.battle
                } else {
                    br_core::battle::preview_battle(doc.engagements, self.settings.battle_break_secs)
                };
                let title = doc.title.clone().unwrap_or_else(|| {
                    b.systems.first().map(|(_, n, _)| n.clone()).unwrap_or_else(|| "Battle report".into())
                });
                self.show_imported_report(b, title, ctx);
            }
            Err(e) => self.report_msg = Some(format!("Could not open report: {e}")),
        }
    }

    pub(crate) fn show_imported_report(&mut self, b: br_core::battle::Battle, title: String, ctx: &egui::Context) {
        let ids: Vec<i64> = b
            .engagements
            .iter()
            .flat_map(|e| {
                let mut v = vec![e.victim_ship];
                v.extend(e.attackers.iter().map(|a| a.ship));
                v
            })
            .filter(|&id| id != 0)
            .collect();
        self.ensure_type_names(&ids, ctx);
        let inv = b.involvement();
        let rosters: Vec<Vec<br_core::battle::Participant>> =
            (0..b.sides.len()).map(|i| b.roster(i)).collect();
        self.loaded_report = Some(LoadedReport {
            title,
            battle: b,
            inv,
            rosters,
            sorted: Vec::new(),
            condensed_rows: Vec::new(),
            sorted_for: None,
            hover: None,
        });
        self.report_msg = None;
    }

    pub(crate) fn poll_build_from_kill(&mut self, ctx: &egui::Context) {
        let done = {
            let mut g = self.build_from_kill.lock().unwrap();
            match &*g {
                crate::zkill::BuildFromKill::Done(..) | crate::zkill::BuildFromKill::Failed(..) => {
                    Some(std::mem::replace(&mut *g, crate::zkill::BuildFromKill::Idle))
                }
                _ => None,
            }
        };
        match done {
            Some(crate::zkill::BuildFromKill::Done(engs, _seed)) => {
                let b = br_core::battle::preview_battle(engs, self.settings.battle_break_secs);
                let title = b
                    .systems
                    .first()
                    .map(|(_, n, _)| n.clone())
                    .unwrap_or_else(|| "Battle report".into());
                self.show_imported_report(b, title, ctx);
                self.build_kill_input.clear();
                self.build_kill_error = None;
            }
            Some(crate::zkill::BuildFromKill::Failed(msg)) => self.build_kill_error = Some(msg),
            _ => {}
        }
    }

    pub(crate) fn loaded_report_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let Some(lr) = self.loaded_report.as_ref() else { return };
        let mut go_back = false;
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Back to battles", icon::ARROW_LEFT)).clicked() {
                go_back = true;
            }
            ui.separator();
            ui.label(egui::RichText::new(&lr.title).strong());
            ui.label(
                egui::RichText::new(format!("{}  Imported", icon::DOWNLOAD_SIMPLE))
                    .color(crate::theme::standing::WARNING),
            );
            ui.separator();
            ui.checkbox(&mut self.battle_condensed, "Condensed");
        });
        if go_back {
            self.loaded_report = None;
            return;
        }
        ui.add_space(6.0);
        let condensed = self.battle_condensed;
        let sort = self.battle_roster_sort;
        // A single static report: (re)sort only when the toggle changes, then render pre-sorted.
        if let Some(lr) = self.loaded_report.as_mut() {
            if lr.sorted_for != Some((sort, condensed)) {
                let type_names = self.type_names.lock().unwrap();
                let (sorted, cond) =
                    crate::brview::sorted_detail(&lr.rosters, sort, &self.ship_sizes, &type_names);
                lr.sorted = sorted;
                lr.condensed_rows = cond;
                lr.sorted_for = Some((sort, condensed));
            }
        }
        let prev_hover = self.loaded_report.as_ref().and_then(|lr| lr.hover);
        let (clicked_system, hover) = {
            let lr = self.loaded_report.as_ref().unwrap();
            let type_names = self.type_names.lock().unwrap();
            battle_detail(
                ui,
                &lr.battle,
                &type_names,
                &lr.inv,
                &lr.sorted,
                &lr.condensed_rows,
                condensed,
                prev_hover,
            )
        };
        if hover != prev_hover {
            if let Some(lr) = self.loaded_report.as_mut() {
                lr.hover = hover;
            }
            ui.ctx().request_repaint();
        }
        if let Some(sid) = clicked_system {
            self.open_system(sid);
        }
    }

    /// Drawn when the card list is empty and the worker has published nothing current. A spinner is
    /// only honest while a compute is in flight, so a worker that is off, never started, or wedged
    /// settles on a message instead of spinning forever.
    pub(crate) fn battles_wait_note(&self, ui: &mut egui::Ui, waited: std::time::Duration) {
        fn spin(ui: &mut egui::Ui, msg: String) {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(egui::RichText::new(msg).weak());
            });
            ui.ctx().request_repaint();
        }
        if !self.settings.battles_enabled {
            ui.label(
                egui::RichText::new("Battle reports are off. Tick Enabled to compute them.").weak(),
            );
            return;
        }
        if !self.watcher_started {
            match self.sde_status.lock().unwrap().clone() {
                SdeStatus::Downloading(msg) => spin(ui, format!("Waiting for static data: {msg}")),
                SdeStatus::Failed(err) => {
                    ui.colored_label(
                        crate::theme::standing::WARNING,
                        format!("Battle reports need the static data, which failed: {err}"),
                    );
                }
                _ => {
                    ui.label(
                        egui::RichText::new(
                            "Battle reports have not started. They begin once the static data is \
                             downloaded.",
                        )
                        .weak(),
                    );
                }
            }
            return;
        }
        if waited >= BATTLE_STALL {
            ui.colored_label(
                crate::theme::standing::WARNING,
                format!(
                    "No battle report after {}s. The background worker is not responding.",
                    waited.as_secs()
                ),
            );
            return;
        }
        spin(ui, "Loading battles…".to_owned());
    }

    pub(crate) fn battles_view(&mut self, ui: &mut egui::Ui) {
        crate::brview::want(&self.br_demand);
        self.my_shared_window(&ui.ctx().clone());
        self.poll_build_from_kill(&ui.ctx().clone());
        if self.loaded_report.is_some() {
            self.loaded_report_view(ui);
            return;
        }
        if !self.settings.battles_enabled {
            ui.add_space(10.0);
            if ui
                .checkbox(&mut self.settings.battles_enabled, "Enable battle reports")
                .on_hover_text(
                    "Generate and compute battle reports from the zKill feed. \
                     While off, no battles are clustered or computed.",
                )
                .changed()
            {
                self.battles_enabled_shared
                    .store(self.settings.battles_enabled, std::sync::atomic::Ordering::Relaxed);
                self.needs_save = true;
                ui.ctx().request_repaint();
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Battle reports are off. No battles are generated or computed. \
                     Gate-camp warnings and the kill feed keep working.",
                )
                .weak(),
            );
            return;
        }
        ui.add_space(10.0);
        let now = chrono::Utc::now().timestamp();
        let source = if self.show_history { self.battle_history.clone() } else { self.battles.clone() };

        // A battle asked for from elsewhere (a fleet's report) that is still being fetched and
        // rebuilt: opened as soon as it exists, given up on after a minute.
        if let Some((ids, since)) = self.battle_select_pending.clone() {
            let found = source
                .lock()
                .unwrap()
                .iter()
                .find(|b| b.engagements.iter().any(|e| ids.contains(&e.kill_id)))
                .and_then(|b| b.engagements.iter().map(|e| e.kill_id).max());
            if let Some(kid) = found {
                self.battle_selected = Some(kid);
                self.battle_select_pending = None;
            } else if since.elapsed() > std::time::Duration::from_secs(60) {
                self.battle_select_pending = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        if let Some(kid) = self.battle_selected {
            let exists = source
                .lock()
                .unwrap()
                .iter()
                .any(|b| b.engagements.iter().any(|e| e.kill_id == kid));
            match exists {
                false => {
                    self.battle_selected = None;
                    self.battle_detail_cache = None;
                }
                true => {
                    // The brview worker builds the detail off-thread. Mirror it into the cache only
                    // when the worker publishes new output or the selection changed, since the
                    // battle is too heavy to clone per frame.
                    let need = self.battle_detail_cache.as_ref().map(|c| c.kid) != Some(kid)
                        || self.battle_detail_out_sig != self.br_outputs.lock().unwrap().sig;
                    if need {
                        let out = self.br_outputs.lock().unwrap();
                        self.battle_detail_out_sig = out.sig;
                        let fresh = out.detail.as_ref().is_some_and(|d| d.kid == kid);
                        if fresh {
                            let d = out.detail.clone();
                            drop(out);
                            if let Some(d) = &d {
                                self.ensure_type_names(&d.ship_ids, ui.ctx());
                            }
                            self.battle_detail_cache = d;
                        } else if self.battle_detail_cache.as_ref().map(|c| c.kid) != Some(kid) {
                            // No spinner: the worker repaints when this battle is ready.
                            self.battle_detail_cache = None;
                        }
                    }
                    if self.battle_detail_cache.is_some() {
                        use egui_phosphor::regular as icon;
                        let ambiguous =
                            self.battle_detail_cache.as_ref().map(|c| c.battle.ambiguous).unwrap_or(false);
                        let excl_n = self.battle_excluded_count;
                        let scrub_n = self.battle_scrub_count;
                        let mut go_back = false;
                        let mut save_clicked = false;
                        let mut share_clicked = false;
                        let mut mine_clicked = false;
                        toolbar(ui, |ui| {
                            if ui
                                .button(format!("{}  Back to battles", icon::ARROW_LEFT))
                                .clicked()
                            {
                                go_back = true;
                            }
                            toolbar_sep(ui);
                            ui.toggle_value(&mut self.battle_edit_mode, format!("{}  Edit", icon::PENCIL))
                                .on_hover_text("Split off kills, remove kills/pilots, add a kill");
                            toolbar_sep(ui);
                            ui.checkbox(&mut self.battle_condensed, "Condensed")
                                .on_hover_text("Stack each side's ships by hull (count + losses)");
                            toolbar_sep(ui);
                            ui.label("Sort");
                            let sort_label = match self.battle_roster_sort {
                                RosterSort::Value => "ISK loss",
                                RosterSort::Hull => "Hull size",
                            };
                            toolbar_combo(
                                ui,
                                "battle_roster_sort",
                                sort_label.to_owned(),
                                |ui| {
                                    ui.menu_value(
                                        &mut self.battle_roster_sort,
                                        RosterSort::Value,
                                        "ISK loss",
                                    );
                                    ui.menu_value(
                                        &mut self.battle_roster_sort,
                                        RosterSort::Hull,
                                        "Hull size",
                                    );
                                },
                            );
                            toolbar_sep(ui);
                            if ui.button(format!("{}  Add kill", icon::PLUS)).clicked() {
                                self.battle_add_open = true;
                            }
                            if ui.button(format!("{} Excluded ({excl_n})", icon::TRASH)).clicked() {
                                self.battle_excluded_open = true;
                            }
                            if ui.button(format!("{} Scrubbed ({scrub_n})", icon::BROOM)).clicked() {
                                self.battle_scrubs_open = true;
                            }
                            toolbar_sep(ui);
                            if ui
                                .button(format!("{}  Save JSON", icon::FLOPPY_DISK))
                                .on_hover_text("Save this battle report as a JSON file you can re-open or share")
                                .clicked()
                            {
                                save_clicked = true;
                            }
                            toolbar_sep(ui);
                            let authed = self.br_authed_chars();
                            if authed.len() > 1 {
                                let current = self.share_identity().map(|(id, _)| id);
                                let sel_name = current
                                    .and_then(|id| {
                                        authed.iter().find(|(a, _)| *a == id).map(|(_, n)| n.clone())
                                    })
                                    .unwrap_or_else(|| "Select character".to_owned());
                                ui.label("Manage as:");
                                toolbar_combo(ui, "br_manage_as", sel_name, |ui| {
                                    for (id, name) in &authed {
                                        if ui
                                            .menu_label(self.br_character == Some(*id), name)
                                            .clicked()
                                        {
                                            self.br_character = Some(*id);
                                        }
                                    }
                                })
                                .on_hover_text("Battle reports are owned per character; pick which one to upload + manage under");
                                toolbar_sep(ui);
                            }
                            let sharing = matches!(
                                *self.br_share.lock().unwrap(),
                                crate::brshare::ShareStatus::Uploading
                            );
                            if ui
                                .add_enabled(
                                    !sharing,
                                    egui::Button::new(format!("{}  Share to eve-spai.com", icon::SHARE_NETWORK)),
                                )
                                .on_hover_text("Upload this battle report to eve-spai.com and get a shareable link")
                                .clicked()
                            {
                                share_clicked = true;
                            }
                            ui.checkbox(&mut self.br_unlisted, "Unlisted")
                                .on_hover_text("Don't list it in the public directory (reachable only by link)");
                            if ui
                                .button(format!("{}  My shared BRs", icon::GLOBE))
                                .on_hover_text("List and manage the reports you've shared")
                                .clicked()
                            {
                                mine_clicked = true;
                            }
                        });
                        if share_clicked {
                            if let Some(b) = self.battle_detail_cache.as_ref().map(|c| c.battle.clone()) {
                                let ctx = ui.ctx().clone();
                                self.br_share_kid = self.battle_selected;
                                self.start_share(&b, &ctx);
                            }
                        }
                        if mine_clicked {
                            let ctx = ui.ctx().clone();
                            self.open_my_shared(&ctx);
                        }
                        if self.battle_selected == self.br_share_kid {
                            self.share_status_ui(ui);
                        }
                        if save_clicked {
                            if let Some(b) = self.battle_detail_cache.as_ref().map(|c| c.battle.clone()) {
                                self.report_msg = match self.save_battle_report(&b) {
                                    Ok(Some(path)) => Some(format!("Saved report to {}", path.display())),
                                    Ok(None) => None,
                                    Err(e) => Some(format!("Could not save report: {e}")),
                                };
                            }
                        }
                        if let Some(msg) = self.report_msg.clone() {
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new(msg).weak());
                        }
                        if go_back {
                            self.battle_selected = None;
                            self.battle_hover = None;
                            self.battle_detail_cache = None;
                            self.battle_edit_mode = false;
                            self.battle_kill_sel.clear();
                            return;
                        }
                        ui.add_space(6.0);
                        self.battle_review_panels(ui.ctx());
                        if ambiguous && !self.battle_edit_mode {
                            egui::Frame::new()
                                .fill(crate::theme::standing::WARNING.gamma_multiply(0.14))
                                .inner_margin(egui::Margin::symmetric(8, 5))
                                .corner_radius(4.0)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}  Possible separate engagements",
                                                icon::WARNING
                                            ))
                                            .color(crate::theme::standing::WARNING)
                                            .strong(),
                                        );
                                        if ui.button(format!("{} Review / split", icon::SCISSORS)).clicked() {
                                            self.battle_edit_mode = true;
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        }
                        if self.battle_edit_mode {
                            self.battle_edit_view(ui, now);
                            return;
                        }
                        let prev_hover = self.battle_hover;
                        let condensed = self.battle_condensed;
                        let cache = self.battle_detail_cache.as_ref().unwrap();
                        // Snapshot only the type names this battle needs, so the render loop
                        // (hundreds of rows) does not hold the shared type_names lock and stall
                        // the brview worker. 670 = the default capsule pod fallback.
                        let names: std::collections::HashMap<i64, String> = {
                            let t = self.type_names.lock().unwrap();
                            cache
                                .ship_ids
                                .iter()
                                .chain(std::iter::once(&670))
                                .filter_map(|id| t.get(id).map(|n| (*id, n.clone())))
                                .collect()
                        };
                        let (clicked_system, hover) = battle_detail(
                            ui,
                            &cache.battle,
                            &names,
                            &cache.inv,
                            &cache.rosters,
                            &cache.condensed,
                            condensed,
                            prev_hover,
                        );
                        if hover != prev_hover {
                            self.battle_hover = hover;
                            ui.ctx().request_repaint();
                        }
                        if let Some(sid) = clicked_system {
                            self.open_system(sid);
                        }
                        return;
                    }
                }
            }
        }

        if self.chat_dir.is_none() && self.settings.intel_channels.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Battle reports cluster killmails near systems seen in intel. \
                     Configure intel channels (Settings) so there's an area to watch.",
                )
                .weak(),
            );
        }

        let mut to_load: Option<std::path::PathBuf> = None;
        let mut open_my_shared = false;
        let mut do_build = false;
        let building =
            matches!(*self.build_from_kill.lock().unwrap(), crate::zkill::BuildFromKill::Loading);
        toolbar(ui, |ui| {
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add(
                egui::TextEdit::singleline(&mut self.battle_search)
                    .hint_text("Filter by system, alliance, pilot…")
                    .desired_width(240.0),
            );
            if !self.battle_search.is_empty() && ui.button("Clear").clicked() {
                self.battle_search.clear();
            }
            toolbar_sep(ui);
            ui.label("\u{2265} ISK").on_hover_text("Only list battles whose total ISK destroyed is at least this many billions");
            let mut bn = self.settings.min_battle_isk / 1e9;
            if ui
                .add(
                    egui::DragValue::new(&mut bn)
                        .range(0.0..=100_000.0)
                        .speed(0.5)
                        .custom_formatter(|n, _| if n == 0.0 { "off".to_owned() } else { format!("{n:.0}B") }),
                )
                .changed()
            {
                self.settings.min_battle_isk = (bn * 1e9).max(0.0);
                self.needs_save = true;
            }
            toolbar_sep(ui);
            ui.label("Split gap (min)")
                .on_hover_text("Auto-split a battle when there's a lull longer than this.");
            let mut mins = (self.settings.battle_break_secs / 60).clamp(1, 30);
            if ui
                .add(egui::DragValue::new(&mut mins).range(1..=30).speed(0.2))
                .on_hover_text("Auto-split a battle when there's a lull longer than this.")
                .changed()
            {
                let secs = mins.clamp(1, 30) * 60;
                self.settings.battle_break_secs = secs;
                self.needs_save = true;
                self.battle_break_shared.store(secs, std::sync::atomic::Ordering::Relaxed);
            }
            if ui
                .checkbox(&mut self.show_history, "Last 30 days")
                .on_hover_text(
                    "EVE Spai keeps 30 days of battle history on disk. Export a battle to JSON to \
                     keep it longer; Open JSON reads it back.",
                )
                .changed()
            {
                self.battle_selected = None;
                if self.show_history {
                    self.load_battle_history(ui.ctx());
                }
            }
            if ui.button(format!("{}  Rules…", egui_phosphor::regular::FUNNEL)).clicked() {
                self.battle_filter_open = true;
            }
            toolbar_sep(ui);
            if ui
                .button(format!("{}  Open JSON", egui_phosphor::regular::FOLDER_OPEN))
                .on_hover_text("Open a saved battle-report JSON file")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("EVE Spai battle report", &["json"])
                    .pick_file()
                {
                    to_load = Some(path);
                }
            }
            if ui
                .button(format!("{}  My shared BRs", egui_phosphor::regular::GLOBE))
                .on_hover_text("List and manage the reports you've shared to eve-spai.com")
                .clicked()
            {
                open_my_shared = true;
            }
            toolbar_sep(ui);
            let input = ui.add_enabled(
                !building,
                egui::TextEdit::singleline(&mut self.build_kill_input)
                    .hint_text("Paste a zKill kill link or id")
                    .desired_width(220.0),
            );
            let submit =
                input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !building;
            if ui
                .add_enabled(
                    !building,
                    egui::Button::new(format!(
                        "{}  Build from kill",
                        egui_phosphor::regular::HAMMER
                    )),
                )
                .on_hover_text("Build a battle report from one zKillboard kill link or id")
                .clicked()
                || submit
            {
                do_build = true;
            }
            if building {
                ui.add(egui::Spinner::new());
            }
            let mut th = self.settings.work_throttle;
            toolbar_combo(
                ui,
                "work_throttle",
                format!("{}  {}", egui_phosphor::regular::GAUGE, th.label()),
                |ui| {
                    for opt in crate::settings::WorkThrottle::CHOICES {
                        ui.menu_value(&mut th, opt, opt.label());
                    }
                },
            )
            .on_hover_text("Throttle background work (battle feed + clustering) to limit CPU.");
            if th != self.settings.work_throttle {
                self.settings.work_throttle = th;
                self.work_throttle_shared.store(th.as_u8(), std::sync::atomic::Ordering::Relaxed);
                self.needs_save = true;
            }
            toolbar_sep(ui);
            if ui
                .checkbox(&mut self.settings.battles_enabled, "Enabled")
                .on_hover_text(
                    "Generate and compute battle reports. Turn off to stop all \
                     battle-report computation.",
                )
                .changed()
            {
                self.battles_enabled_shared
                    .store(self.settings.battles_enabled, std::sync::atomic::Ordering::Relaxed);
                self.needs_save = true;
            }
        });
        if open_my_shared {
            self.open_my_shared(&ui.ctx().clone());
        }
        if do_build {
            match crate::zkill::parse_kill_id(&self.build_kill_input) {
                None => {
                    self.build_kill_error = Some("Not a valid zKill kill link or id".to_owned());
                }
                Some(id) => {
                    self.build_kill_error = None;
                    if let (Some(systems), Some(ship_ids)) =
                        (self.systems.clone(), self.battle_ship_ids.clone())
                    {
                        crate::zkill::spawn_build_from_kill(
                            id,
                            systems,
                            ship_ids,
                            self.build_from_kill.clone(),
                            ui.ctx().clone(),
                        );
                    } else {
                        self.build_kill_error =
                            Some("Ship data is still loading, try again in a moment".to_owned());
                    }
                }
            }
        }
        if let Some(err) = self.build_kill_error.clone() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(err).color(crate::theme::standing::WARNING));
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    self.build_kill_error = None;
                }
            });
        }
        if let Some(path) = to_load {
            self.load_battle_report(&path, ui.ctx());
            return;
        }
        if let Some(msg) = self.report_msg.clone() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(msg).weak());
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    self.report_msg = None;
                }
            });
        }
        ui.add_space(4.0);
        let query = self.battle_search.trim().to_lowercase();
        let loading = self.battle_history_loading.load(std::sync::atomic::Ordering::Relaxed);

        // All filtering/roster work runs on the brview worker; the UI only publishes inputs and
        // reads results, showing a spinner while the worker catches up.
        {
            let mut inp = self.br_inputs.lock().unwrap();
            inp.query = self.battle_search.trim().to_owned();
            inp.min_isk = self.settings.min_battle_isk;
            inp.show_history = self.show_history;
            inp.break_secs = self.settings.battle_break_secs;
            inp.player_sys = self.player_system().unwrap_or(0);
            inp.selected_kid = self.battle_selected;
            inp.sort = self.battle_roster_sort;
            inp.condensed = self.battle_condensed;
        }
        let want_sig = {
            let inp = self.br_inputs.lock().unwrap();
            crate::brview::ui_signature(
                &self.battles,
                &self.battle_history,
                &self.battle_filter_gen_shared,
                &self.battle_overrides_gen_shared,
                &self.intel_state,
                &inp,
            )
        };
        if want_sig != self.br_last_sent_sig {
            self.br_last_sent_sig = want_sig;
            crate::brview::poke(&self.br_wake);
        }
        {
            let out = self.br_outputs.lock().unwrap();
            if out.sig != self.battle_cards_out_sig {
                self.battle_cards_out_sig = out.sig;
                self.battle_cards = out.cards.clone();
                self.battle_cards_total = out.total;
                self.battle_cards_filtered = out.filtered;
                self.battle_cards_ready = out.ready;
            }
        }
        let total = self.battle_cards_total;
        let filtered = self.battle_cards_filtered;
        let ready = self.battle_cards_ready;
        let fresh = self.battle_cards_out_sig == want_sig;
        let waited = if ready && fresh {
            self.battle_wait_since = None;
            std::time::Duration::ZERO
        } else {
            self.battle_wait_since.get_or_insert_with(std::time::Instant::now).elapsed()
        };

        if self.battle_cards.is_empty() {
            if !ready || !fresh {
                self.battles_wait_note(ui, waited);
                return;
            }
            let msg = if self.show_history && loading {
                "Loading full history…".to_owned()
            } else if filtered > 0 {
                format!(
                    "{} battle(s) below the {} ISK minimum.",
                    filtered,
                    fmt_isk(self.settings.min_battle_isk)
                )
            } else if self.show_history {
                "No battles recorded in the last 30 days.".to_owned()
            } else if query.is_empty() {
                "No active battles near the tracked area.".to_owned()
            } else {
                "No battles match the filter.".to_owned()
            };
            ui.label(egui::RichText::new(msg).weak());
            return;
        }

        let shown_n = self.battle_cards.len();
        let count_txt = if filtered > 0 {
            format!("{total} battles ({filtered} filtered)")
        } else {
            format!("{total} battles")
        };
        let mut do_merge = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(count_txt).weak());
            ui.separator();
            ui.toggle_value(
                &mut self.battle_edit_mode,
                format!("{}  Merge", egui_phosphor::regular::ARROWS_MERGE),
            )
            .on_hover_text("Tick two or more battles to merge them into one");
            if self.battle_edit_mode {
                let n = self.battle_merge_sel.len();
                if n >= 2
                    && ui
                        .button(format!("{} Merge {n} battles", egui_phosphor::regular::ARROWS_MERGE))
                        .clicked()
                {
                    do_merge = true;
                }
                if n > 0 && ui.button("Clear").clicked() {
                    self.battle_merge_sel.clear();
                }
            }
        });
        ui.add_space(4.0);
        let mut open: Option<i64> = None;
        let edit = self.battle_edit_mode;
        let mut merge_sel = std::mem::take(&mut self.battle_merge_sel);
        let cards = &self.battle_cards;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (kid, from_you, b) in cards {
                if edit {
                    ui.horizontal(|ui| {
                        let mut on = merge_sel.contains(kid);
                        if ui.checkbox(&mut on, "").changed() {
                            if on {
                                merge_sel.insert(*kid);
                            } else {
                                merge_sel.remove(kid);
                            }
                        }
                        if battle_row(ui, b, now, *from_you) {
                            open = Some(*kid);
                        }
                    });
                } else if battle_row(ui, b, now, *from_you) {
                    open = Some(*kid);
                }
                ui.add_space(4.0);
            }
            if total > shown_n {
                ui.label(
                    egui::RichText::new(format!(
                        "Showing the newest {shown_n}. Narrow with search or rules to see the other {}.",
                        total - shown_n
                    ))
                    .weak(),
                );
            }
        });
        self.battle_merge_sel = merge_sel;
        if let Some(kid) = open {
            self.battle_selected = Some(kid);
            self.battle_edit_mode = false;
        }
        if do_merge {
            let sel = self.battle_merge_sel.clone();
            let mut kids: Vec<i64> = Vec::new();
            {
                let guard = source.lock().unwrap();
                for b in guard.iter() {
                    let rep = b.engagements.iter().map(|e| e.kill_id).max().unwrap_or(0);
                    if sel.contains(&rep) {
                        kids.extend(b.engagements.iter().map(|e| e.kill_id));
                    }
                }
            }
            let ctx = ui.ctx().clone();
            self.apply_battle_edit(&ctx, move |s| {
                let t = s.next_battle_tag();
                for kid in &kids {
                    s.set_battle_tag(*kid, Some(t));
                }
            });
            self.battle_merge_sel.clear();
            self.battle_edit_mode = false;
        }
    }
}
