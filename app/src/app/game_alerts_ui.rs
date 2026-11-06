//! The game log alerts: a section on the Alerts tab and the rule editor behind it.

use super::*;
use crate::gamealerts::{GameAlertRule, Trigger, Who};
use spai_ui::i18n::Tr;
use spai_ui::widgets::SteadySelect as _;

impl SpaiApp {
    /// The Alerts tab's game log block: the master switch, the editor button and the latest alerts.
    pub(crate) fn game_alerts_summary(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(tr!("Game log alerts")).strong());
            crate::app::help_icon(ui, tr!("Alerts on what your own client logs: tackled, decloaked, out of charges, invited to a fleet. Read from the Gamelogs folder, so they work without any intel channel."));
        });
        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut self.settings.game_alerts.enabled, tr!("Enable game log alerts")).changed() {
                self.needs_save = true;
            }
            let n = self.settings.game_alerts.rules.iter().filter(|r| r.enabled).count();
            if ui
                .button(trf!("{icon}  Game log rules ({n} on)", icon = egui_phosphor::regular::SLIDERS_HORIZONTAL, n = n))
                .clicked()
            {
                self.game_alerts_open = true;
            }
        });
        let recent: Vec<crate::gamealerts::Fired> =
            self.game_alerts.feed.lock().map(|f| f.iter().rev().take(8).cloned().collect()).unwrap_or_default();
        if recent.is_empty() {
            ui.label(egui::RichText::new(tr!("No game log alert yet.")).weak());
        }
        let now = crate::clock::utc().timestamp();
        for f in recent {
            fired_row(ui, &f, now);
        }
    }

    pub(crate) fn game_alerts_editor(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as ic;
        let mut changed = false;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(trf!("{arrow_left}  Back", arrow_left = ic::ARROW_LEFT)).on_hover_text(tr!("Back to alerts")).clicked() {
                self.game_alerts_open = false;
            }
            ui.separator();
            ui.menu_button(trf!("{plus}  Add rule", plus = ic::PLUS), |ui| {
                for t in Trigger::ALL {
                    if ui.button(t.label().tr()).on_hover_text(t.help().tr()).clicked() {
                        let mut r = GameAlertRule::of(t);
                        r.sound = "warning".into();
                        self.settings.game_alerts.rules.push(r);
                        crate::gamealerts::ensure_ids(&mut self.settings.game_alerts.rules);
                        self.game_alert_selected = self.settings.game_alerts.rules.last().map(|r| r.id);
                        changed = true;
                        ui.close();
                    }
                }
            });
            changed |= ui.checkbox(&mut self.settings.game_alerts.enabled, tr!("Enabled")).changed();
        });
        let (dir, seen) = self.game_alerts.status.lock().map(|s| s.clone()).unwrap_or_default();
        ui.horizontal_wrapped(|ui| match (&dir, self.headless) {
            (Some(d), _) => {
                ui.label(egui::RichText::new(tr!("Reading")).weak());
                ui.label(egui::RichText::new(d.display().to_string()).monospace().weak());
                if !seen.is_empty() {
                    ui.label(egui::RichText::new(trf!("for {names}", names = seen.join(", "))).weak());
                }
            }
            (None, true) => {}
            (None, false) => {
                ui.colored_label(crate::theme::standing::WARNING, tr!("EVE's Gamelogs folder was not found. Set the logs folder in Settings."));
            }
        });
        ui.add_space(4.0);
        ui.separator();

        let ids: Vec<u64> = self.settings.game_alerts.rules.iter().map(|r| r.id).collect();
        if self.game_alert_selected.is_none_or(|id| !ids.contains(&id)) {
            self.game_alert_selected = ids.first().copied();
        }
        egui::Panel::left("game_alert_rules_split")
            .resizable(true)
            .default_size(260.0)
            .size_range(180.0..=400.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("game_alert_list").show(ui, |ui| {
                    for r in self.settings.game_alerts.rules.iter_mut() {
                        let selected = self.game_alert_selected == Some(r.id);
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut r.enabled, "").changed();
                            let label = shown(r);
                            let txt = if r.enabled { egui::RichText::new(&label) } else { egui::RichText::new(&label).weak().strikethrough() };
                            if ui
                                .add(
                                    egui::Button::new((txt, egui::Atom::grow()))
                                        .selected(selected)
                                        .frame_when_inactive(selected)
                                        .stroke(egui::Stroke::NONE)
                                        .truncate()
                                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                                )
                                .on_hover_text(&label)
                                .clicked()
                            {
                                self.game_alert_selected = Some(r.id);
                            }
                        });
                    }
                });
            });

        let mut remove: Option<usize> = None;
        let mut test: Option<crate::gamealerts::Fired> = None;
        let mut chars: Vec<String> = self.characters.iter().map(|c| c.name.clone()).collect();
        for s in &seen {
            if !chars.iter().any(|c| c.eq_ignore_ascii_case(s)) {
                chars.push(s.clone());
            }
        }
        let feed: Vec<crate::gamealerts::Fired> = self.game_alerts.feed.lock().map(|f| f.iter().rev().cloned().collect()).unwrap_or_default();
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let Some(idx) = self.game_alert_selected.and_then(|id| self.settings.game_alerts.rules.iter().position(|r| r.id == id)) else {
                ui.add_space(20.0);
                ui.label(egui::RichText::new(tr!("Add a rule to get started.")).weak());
                return;
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("game_alert_config").show(ui, |ui| {
                let r = &mut self.settings.game_alerts.rules[idx];
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut r.enabled, "").on_hover_text(tr!("Rule on")).changed();
                    let hint = r.trigger().map(|t| t.label().tr()).unwrap_or(tr!("Rule name"));
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut r.name).hint_text(hint).desired_width((ui.available_width() - 200.0).clamp(160.0, 420.0)))
                        .changed();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(trf!("{trash}  Delete", trash = ic::TRASH)).clicked() {
                            remove = Some(idx);
                        }
                        if ui.button(trf!("{icon}  Test", icon = ic::PLAY)).on_hover_text(tr!("Fire this rule once, as it would")).clicked() {
                            test = Some(crate::gamealerts::Fired {
                                at: crate::clock::utc().timestamp(),
                                character: String::new(),
                                rule_id: r.id,
                                title: r.title(),
                                body: tr!("A test of this rule").to_owned(),
                                sound: r.sound.clone(),
                                volume: r.volume,
                                notification: r.notification,
                                push: r.push,
                            });
                        }
                    });
                });
                ui.add_space(6.0);
                changed |= rule_config(ui, r, &chars);
                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new(tr!("Recent alerts from this rule")).strong());
                let id = r.id;
                let now = crate::clock::utc().timestamp();
                let mine: Vec<&crate::gamealerts::Fired> = feed.iter().filter(|f| f.rule_id == id).take(30).collect();
                if mine.is_empty() {
                    ui.label(egui::RichText::new(tr!("Nothing yet.")).weak());
                }
                for f in mine {
                    fired_row(ui, f, now);
                }
            });
        });
        if let Some(i) = remove {
            self.settings.game_alerts.rules.remove(i);
            changed = true;
        }
        if let Some(f) = test {
            if f.notification {
                crate::app::notify_game(f.title.clone(), f.body.clone());
            }
            let sound = if f.sound.is_empty() { "warning" } else { f.sound.as_str() };
            crate::sound::play_prio(sound, 2, f.volume.unwrap_or(self.settings.alerts.alert_volume));
            if f.push {
                crate::push::Targets::of(&self.settings.alerts).send(&f.title, &f.body, 2);
            }
            if let Ok(mut feed) = self.game_alerts.feed.lock() {
                feed.push_back(f);
            }
        }
        if changed {
            self.needs_save = true;
        }
    }
}

/// The rule's own name, else its trigger in the UI language.
fn shown(r: &GameAlertRule) -> String {
    match (r.name.trim(), r.trigger()) {
        ("", Some(t)) => t.label().tr().to_owned(),
        _ => r.title(),
    }
}

fn fired_row(ui: &mut egui::Ui, f: &crate::gamealerts::Fired, now: i64) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(crate::app::eve_time_label(f.at, now)).monospace().weak());
        ui.label(egui::RichText::new(&f.title).strong());
        ui.label(&f.body);
    });
}

/// The rule's settings. True when anything changed.
fn rule_config(ui: &mut egui::Ui, r: &mut GameAlertRule, chars: &[String]) -> bool {
    let mut changed = false;
    let trigger = r.trigger();
    let id = r.id;
    egui::Grid::new(("game_rule_grid", id)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(tr!("When"));
        ui.vertical(|ui| {
            let shown = trigger.map(|t| t.label().tr()).unwrap_or(tr!("Unknown trigger"));
            egui::ComboBox::from_id_salt(("game_rule_trigger", id)).selected_text(shown).width(260.0).show_ui(ui, |ui| {
                for t in Trigger::ALL {
                    if ui.menu_label(trigger == Some(t), t.label().tr()).on_hover_text(t.help().tr()).clicked() && trigger != Some(t) {
                        r.trigger = t.key().to_owned();
                        changed = true;
                    }
                }
            });
            if let Some(t) = trigger {
                ui.label(egui::RichText::new(t.help().tr()).weak());
            }
        });
        ui.end_row();

        ui.label(tr!("Characters"));
        let shown = if r.characters.is_empty() { tr!("All of yours").to_owned() } else { r.characters.join(", ") };
        ui.menu_button(shown, |ui| {
            if ui.menu_label(r.characters.is_empty(), tr!("All of yours")).clicked() {
                r.characters.clear();
                changed = true;
            }
            for c in chars {
                let mut on = r.characters.iter().any(|x| x.eq_ignore_ascii_case(c));
                if ui.checkbox(&mut on, c).changed() {
                    if on {
                        r.characters.push(c.clone());
                    } else {
                        r.characters.retain(|x| !x.eq_ignore_ascii_case(c));
                    }
                    changed = true;
                }
            }
        });
        ui.end_row();

        let Some(t) = trigger else { return };
        if t.has_party() {
            ui.label(tr!("By or against"));
            let who = Who::from_key(&r.who);
            egui::ComboBox::from_id_salt(("game_rule_who", id)).selected_text(who.label().tr()).show_ui(ui, |ui| {
                for w in [Who::Any, Who::Players, Who::Npcs] {
                    if ui.menu_label(who == w, w.label().tr()).clicked() {
                        r.who = w.key().to_owned();
                        changed = true;
                    }
                }
            })
            .response
            .on_hover_text(tr!("Players carry a corporation ticker in the log, NPCs do not."));
            ui.end_row();
        }
        if let Some(hint) = t.filter_hint() {
            if t == Trigger::Custom {
                ui.label(tr!("Text"));
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.contains).hint_text(tr!("part of the line")).desired_width(260.0)).changed();
                    changed |= ui.checkbox(&mut r.regex, tr!("Regular expression")).changed();
                });
                ui.end_row();
                if r.regex && !r.contains.trim().is_empty() && regex::Regex::new(r.contains.trim()).is_err() {
                    ui.label("");
                    ui.colored_label(crate::theme::standing::HOSTILE, tr!("Not a valid regular expression"));
                    ui.end_row();
                }
            } else {
                ui.label(tr!("Only if it names")).on_hover_text(tr!("Empty matches anything"));
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut r.contains).hint_text(trf!("{what}, comma separated", what = hint.tr())).desired_width(320.0))
                    .changed();
                ui.end_row();
            }
            ui.label(tr!("Never if it names"));
            changed |= ui.add(egui::TextEdit::singleline(&mut r.ignore).hint_text(tr!("comma separated")).desired_width(320.0)).changed();
            ui.end_row();
        }
        match t {
            Trigger::Tackled => {
                ui.label("");
                changed |= ui.checkbox(&mut r.scram_only, tr!("Scramblers only, not disruptors")).changed();
                ui.end_row();
            }
            Trigger::Decloaked => {
                ui.label("");
                changed |= ui.checkbox(&mut r.ships_only, tr!("Only by a ship, not a gate, wormhole or structure")).changed();
                ui.end_row();
            }
            Trigger::UnderAttack => {
                ui.label(tr!("Smallest hit"));
                changed |= ui.add(egui::DragValue::new(&mut r.amount).range(0..=100_000).custom_formatter(|n, _| if n == 0.0 { tr!("any").into() } else { format!("{n} HP") })).on_hover_text(tr!("Hits below this are ignored. Misses, points and neuts always count.")).changed();
                ui.end_row();
            }
            Trigger::Neutralized => {
                ui.label(tr!("Smallest drain"));
                changed |= ui.add(egui::DragValue::new(&mut r.amount).range(0..=100_000).custom_formatter(|n, _| if n == 0.0 { tr!("any").into() } else { format!("{n} GJ") })).changed();
                ui.end_row();
            }
            Trigger::BurstsLow => {
                ui.label(tr!("Fewer members than"));
                changed |= ui.add(egui::DragValue::new(&mut r.amount).range(1..=256)).changed();
                ui.end_row();
            }
            t if t.is_timer() => {
                ui.label(tr!("Quiet for"));
                changed |= ui.add(egui::DragValue::new(&mut r.seconds).range(5..=3600).suffix(" s")).changed();
                ui.end_row();
            }
            _ => {}
        }
    });

    ui.add_space(10.0);
    ui.label(egui::RichText::new(tr!("Then")).strong());
    egui::Grid::new(("game_rule_then", id)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(tr!("Show in"));
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut r.notification, tr!("Desktop notification")).changed();
            changed |= ui.checkbox(&mut r.push, tr!("Push")).on_hover_text(tr!("To your phone, when push is set up")).changed();
        });
        ui.end_row();
        ui.label(tr!("Sound"));
        let volume = r.volume.unwrap_or(1.0);
        changed |= sound_picker(ui, ("game_rule_sound", id), true, &mut r.sound, volume);
        ui.end_row();
        ui.label(tr!("Quiet for")).on_hover_text(tr!("After an alert, the same rule stays quiet this long for that character"));
        changed |= ui
            .add(egui::DragValue::new(&mut r.cooldown_secs).range(0..=3600).custom_formatter(|n, _| if n == 0.0 { tr!("no pause").into() } else { format!("{n}s") }))
            .changed();
        ui.end_row();
    });
    changed
}
