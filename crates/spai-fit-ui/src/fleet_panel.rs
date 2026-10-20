//! Fleets: which fits fly in what numbers, the boosters, and how the fleet fares against another.

use std::collections::HashMap;

use egui_phosphor::regular as icon;
use spai_fit::fit::Fit;
use spai_fit::fleet::{self, Against, BurstKind, CountRule, Fleet, FleetSlot, FleetStats};
use spai_fit::skills::SkillSet;
use spai_ui::widgets::{icon_button, SteadySelect as _};
use spai_ui::{tr, trf};

use crate::{text, FitSummary, HostRequest, SimUi};

/// A saved fleet as the library lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetSummary {
    pub id: String,
    pub name: String,
    pub updated_at: i64,
}

pub struct FleetSession {
    pub id: String,
    pub fleet: Fleet,
    pub dirty: bool,
    /// The fleet picked to compare against, by library id.
    pub enemy: Option<String>,
}

pub fn kind_name(kind: BurstKind) -> &'static str {
    match kind {
        BurstKind::Shield => tr!("Shield"),
        BurstKind::Armor => tr!("Armor"),
        BurstKind::Skirmish => tr!("Skirmish"),
        BurstKind::Information => tr!("Information"),
        BurstKind::Mining => tr!("Mining"),
    }
}

fn rule_name(rule: CountRule) -> &'static str {
    match rule {
        CountRule::Fixed(_) => tr!("Fixed"),
        CountRule::Percent(_) => tr!("Percent"),
        CountRule::Fill { .. } => tr!("Fill"),
    }
}

impl SimUi {
    /// Fit documents the fleets need that are not here yet; the host is asked for them once.
    fn want_fit_docs(&mut self, ids: impl Iterator<Item = String>) {
        let missing: Vec<String> = ids.filter(|id| !self.fit_docs.contains_key(id) && !self.fit_docs_asked.contains(id)).collect();
        if !missing.is_empty() {
            self.fit_docs_asked.extend(missing.iter().cloned());
            self.requests.push(HostRequest::FitDocs(missing));
        }
    }

    pub fn new_fleet(&mut self, now_ms: u64) {
        let id = self.new_id(now_ms);
        self.fleet = Some(FleetSession { id, fleet: Fleet { name: tr!("New fleet").to_owned(), ..Fleet::default() }, dirty: true, enemy: None });
    }

    /// The doctrines by tag, each tag one doctrine's presets; an untagged preset stands alone.
    pub fn doctrine_groups(&self) -> Vec<(String, Vec<String>)> {
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for d in &self.doctrines {
            let names: Vec<String> = if d.tags.is_empty() { vec![d.name.clone()] } else { d.tags.clone() };
            for n in names {
                match groups.iter_mut().find(|(g, _)| *g == n) {
                    Some((_, ids)) => ids.push(d.id.clone()),
                    None => groups.push((n, vec![d.id.clone()])),
                }
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        groups
    }

    /// A fleet of the doctrine's presets, each a slot by its hull's role.
    pub fn new_fleet_from_doctrine(&mut self, group: &str, now_ms: u64) {
        let Some(dogma) = self.dogma.clone() else { return };
        let Some((_, ids)) = self.doctrine_groups().into_iter().find(|(g, _)| g == group) else { return };
        let fits: Vec<(String, Fit)> = ids.iter().filter_map(|id| self.fit_docs.get(id).map(|f| (id.clone(), f.clone()))).collect();
        let skills = self.skills().clone();
        let mut fleet = spai_fit::fleet::from_doctrine(&dogma, group, &fits, &skills);
        fleet.size = 50;
        let id = self.new_id(now_ms);
        self.fleet = Some(FleetSession { id, fleet, dirty: true, enemy: None });
    }

    pub fn save_fleet(&mut self) {
        if let Some(f) = &self.fleet {
            self.requests.push(HostRequest::SaveFleet { id: f.id.clone(), name: f.fleet.name.clone(), doc: serde_json::to_string(&f.fleet).unwrap_or_default() });
        }
    }

    fn fleet_skills(&self, profile: Option<&str>) -> SkillSet {
        profile.and_then(|p| self.profiles.iter().find(|x| x.id == p)).map_or_else(|| self.skills().clone(), |p| p.skills.clone())
    }

    /// The fleet's numbers, and the enemy's with both comparisons, from the documents at hand.
    pub fn fleet_stats(&self) -> Option<(FleetStats, Option<(FleetStats, Against, Against)>)> {
        let (dogma, f) = (self.dogma.as_ref()?, self.fleet.as_ref()?);
        let docs = &self.fit_docs;
        let fits = |id: &str| docs.get(id).cloned();
        let skills = |p: Option<&str>| self.fleet_skills(p);
        let mine = fleet::stats(dogma, &f.fleet, &fits, &skills);
        let enemy = f.enemy.as_ref().and_then(|id| self.fleet_docs.get(id)).map(|e| {
            let theirs = fleet::stats(dogma, e, &fits, &skills);
            let a = fleet::against(dogma, &f.fleet, &mine, e, &theirs);
            let b = fleet::against(dogma, e, &theirs, &f.fleet, &mine);
            (theirs, a, b)
        });
        Some((mine, enemy))
    }
}

/// The fleet library column.
pub fn library(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr!("Fleets")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(trf!("{icon} New", icon = icon::PLUS)).clicked() {
                sim.new_fleet(now_ms);
            }
            let groups = sim.doctrine_groups();
            if !groups.is_empty() {
                let mut from: Option<String> = None;
                ui.menu_button(tr!("From doctrine"), |ui| {
                    for (name, _) in &groups {
                        if ui.button(name).clicked() {
                            from = Some(name.clone());
                            ui.close();
                        }
                    }
                });
                if let Some(name) = from {
                    sim.new_fleet_from_doctrine(&name, now_ms);
                }
            }
        });
    });
    ui.separator();
    let current = sim.fleet.as_ref().map(|f| f.id.clone());
    let entries = sim.fleets.clone();
    let mut load: Option<String> = None;
    let mut delete: Option<String> = None;
    egui::ScrollArea::vertical().id_salt("sim_fleet_list").auto_shrink([false, false]).show(ui, |ui| {
        if entries.is_empty() {
            ui.label(egui::RichText::new(tr!("No fleets yet. A fleet is a list of saved fits with how many of each fly.")).weak());
        }
        for e in &entries {
            let selected = current.as_deref() == Some(e.id.as_str());
            let r = crate::rows::row(ui, "fleets", ("sim_fleet", &e.id), crate::rows::ROW_H, selected, |ui| {
                let w = (ui.available_width() - 26.0 - crate::rows::GAP).max(40.0);
                let label = if selected { egui::RichText::new(&e.name).strong() } else { egui::RichText::new(&e.name) };
                crate::rows::cell(ui, w, label, &e.name);
                if icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                    delete = Some(e.id.clone());
                }
            });
            if r.clicked() {
                load = Some(e.id.clone());
            }
        }
    });
    if let Some(id) = load {
        sim.request(HostRequest::LoadFleet(id));
    }
    if let Some(id) = delete {
        if current.as_deref() == Some(id.as_str()) {
            sim.fleet = None;
        }
        sim.fleets.retain(|e| e.id != id);
        sim.fleet_docs.remove(&id);
        sim.request(HostRequest::DeleteFleet(id));
    }
}

/// The fleet being edited.
pub fn editor(sim: &mut SimUi, ui: &mut egui::Ui, _now_ms: u64) {
    if sim.fleet.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(tr!("No fleet open. Start one, or open a saved fleet from the list."));
        });
        return;
    }
    let wanted: Vec<String> = sim.fleet.as_ref().map(|f| f.fleet.slots.iter().map(|s| s.fit_id.clone()).chain(f.fleet.boosters.iter().map(|(_, id)| id.clone())).collect()).unwrap_or_default();
    sim.want_fit_docs(wanted.into_iter());
    let library: Vec<FitSummary> = sim.library.clone();
    let profiles: Vec<(String, String)> = sim.profiles.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
    let fleets: Vec<FleetSummary> = sim.fleets.clone();
    let my_id = sim.fleet.as_ref().map(|f| f.id.clone()).unwrap_or_default();
    let projected = sim.fleet_stats().map(|(mine, _)| mine.projected).unwrap_or_default();
    let doctrine_fits: Vec<(String, String)> = sim.doctrines.iter().map(|d| (d.id.clone(), trf!("{name} (doctrine)", name = d.name))).collect();
    let Some(session) = sim.fleet.as_mut() else { return };
    let f = &mut session.fleet;
    let mut changed = false;
    egui::ScrollArea::vertical().id_salt("sim_fleet_editor").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 260.0).max(120.0);
            changed |= ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(w).hint_text(tr!("Fleet name"))).changed();
            ui.label(tr!("Size"));
            changed |= ui.add(egui::DragValue::new(&mut f.size).range(1..=2000)).changed();
        });
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tr!("Ships")).strong());
        let mut remove: Option<usize> = None;
        let counts = fleet::resolve_counts(f);
        // Below this width the count controls go on a second line under the fit.
        let narrow = ui.available_width() < 720.0;
        for i in 0..f.slots.len() {
            let count = counts.get(i).copied().unwrap_or(0);
            let rule_row = |ui: &mut egui::Ui, slot: &mut FleetSlot, changed: &mut bool| {
                let mut rule = slot.rule;
                egui::ComboBox::from_id_salt(("sim_fleet_rule", i)).selected_text(rule_name(rule)).width(84.0).show_ui(ui, |ui| {
                    for r in [CountRule::Fixed(count.max(1)), CountRule::Percent(10.0), CountRule::Fill { min: 0, max: 1000 }] {
                        if ui.menu_label(std::mem::discriminant(&rule) == std::mem::discriminant(&r), rule_name(r)).clicked() {
                            rule = r;
                        }
                    }
                });
                if std::mem::discriminant(&rule) != std::mem::discriminant(&slot.rule) {
                    slot.rule = rule;
                    *changed = true;
                }
                match &mut slot.rule {
                    CountRule::Fixed(n) => *changed |= ui.add(egui::DragValue::new(n).range(0..=2000)).changed(),
                    CountRule::Percent(p) => *changed |= ui.add(egui::DragValue::new(p).range(0.0..=100.0).suffix("%")).changed(),
                    CountRule::Fill { min, max } => {
                        *changed |= ui.add(egui::DragValue::new(min).range(0..=2000).prefix(tr!("min "))).changed();
                        *changed |= ui.add(egui::DragValue::new(max).range(0..=2000).prefix(tr!("max "))).changed();
                    }
                }
                let mut typed = slot.override_count.is_some();
                if ui.checkbox(&mut typed, "").on_hover_text(tr!("Type the number instead")).changed() {
                    slot.override_count = typed.then_some(count);
                    *changed = true;
                }
                match &mut slot.override_count {
                    Some(n) => *changed |= ui.add(egui::DragValue::new(n).range(0..=2000)).changed(),
                    None => {
                        ui.add_sized([44.0, 22.0], egui::Label::new(egui::RichText::new(trf!("= {n}", n = count)).monospace()));
                    }
                }
            };
            ui.horizontal(|ui| {
                ui.set_height(26.0);
                let is_main = f.main.unwrap_or(0) == i;
                if ui.add(egui::RadioButton::new(is_main, "")).on_hover_text(tr!("Main damage dealer, for the enemy's critical mass")).clicked() {
                    f.main = Some(i);
                    changed = true;
                }
                let slot = &mut f.slots[i];
                let fit_name = library.iter().find(|x| x.id == slot.fit_id).map(|x| x.name.clone()).or_else(|| doctrine_fits.iter().find(|(id, _)| *id == slot.fit_id).map(|(_, n)| n.clone())).unwrap_or_else(|| tr!("Choose a fit").to_owned());
                let rest = if narrow { 40.0 } else { 360.0 };
                let fit_w = (ui.available_width() - rest).max(100.0);
                egui::ComboBox::from_id_salt(("sim_fleet_fit", i)).selected_text(text::clip(&fit_name, 28)).width(fit_w).show_ui(ui, |ui| {
                    for x in &library {
                        changed |= ui.menu_value(&mut slot.fit_id, x.id.clone(), x.name.clone()).changed();
                    }
                    for (id, name) in &doctrine_fits {
                        changed |= ui.menu_value(&mut slot.fit_id, id.clone(), name.clone()).changed();
                    }
                });
                if !narrow {
                    rule_row(ui, slot, &mut changed);
                }
                if icon_button(ui, icon::X).clicked() {
                    remove = Some(i);
                }
            });
            if narrow {
                ui.horizontal(|ui| {
                    ui.set_height(26.0);
                    ui.add_space(28.0);
                    rule_row(ui, &mut f.slots[i], &mut changed);
                });
            }
        }
        if let Some(i) = remove {
            f.slots.remove(i);
            changed = true;
        }
        if ui.small_button(trf!("{icon} Add ship", icon = icon::PLUS)).clicked() {
            f.slots.push(FleetSlot { rule: if f.slots.is_empty() { CountRule::Fill { min: 0, max: 1000 } } else { CountRule::Fixed(1) }, ..FleetSlot::default() });
            changed = true;
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("Boosters")).strong());
        for kind in BurstKind::ALL {
            ui.horizontal(|ui| {
                ui.set_height(24.0);
                ui.add_sized([90.0, 22.0], egui::Label::new(kind_name(kind)));
                let current = f.boosters.iter().find(|(k, _)| *k == kind).map(|(_, id)| id.clone());
                let name = current.as_ref().and_then(|id| library.iter().find(|x| &x.id == id).map(|x| x.name.clone()).or_else(|| doctrine_fits.iter().find(|(d, _)| d == id).map(|(_, n)| n.clone()))).unwrap_or_else(|| tr!("none").to_owned());
                let mut pick: Option<Option<String>> = None;
                egui::ComboBox::from_id_salt(("sim_fleet_booster", kind_name(kind))).selected_text(text::clip(&name, 28)).width((ui.available_width() - 8.0).min(260.0)).show_ui(ui, |ui| {
                    if ui.menu_label(current.is_none(), tr!("none")).clicked() {
                        pick = Some(None);
                    }
                    for x in &library {
                        if ui.menu_label(current.as_deref() == Some(x.id.as_str()), x.name.clone()).clicked() {
                            pick = Some(Some(x.id.clone()));
                        }
                    }
                    for (id, name) in &doctrine_fits {
                        if ui.menu_label(current.as_deref() == Some(id.as_str()), name.clone()).clicked() {
                            pick = Some(Some(id.clone()));
                        }
                    }
                });
                if let Some(p) = pick {
                    f.boosters.retain(|(k, _)| *k != kind);
                    if let Some(id) = p {
                        f.boosters.push((kind, id));
                    }
                    changed = true;
                }
            });
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("Engagement")).strong());
        egui::Grid::new("sim_fleet_engagement").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr!("Range"));
            let mut km = f.range / 1000.0;
            if ui.add(egui::Slider::new(&mut km, 0.0..=300.0).suffix(" km").step_by(0.5)).changed() {
                f.range = km * 1000.0;
                changed = true;
            }
            ui.end_row();
            ui.label(tr!("Volley efficiency"));
            changed |= ui.add(egui::Slider::new(&mut f.volley_efficiency, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
            ui.end_row();
            ui.label(tr!("Ewar on the enemy"));
            ui.vertical(|ui| {
                let p = projected;
                let parts: Vec<String> = [(p.webs, tr!("webs")), (p.painters, tr!("painters")), (p.tracking_disruptors, tr!("tracking disruptors"))].iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect();
                let summary = if parts.is_empty() { tr!("none: no ship of the fleet projects any").to_owned() } else { trf!("{list}, from the ships", list = parts.join(", ")) };
                ui.add(egui::Label::new(egui::RichText::new(summary).weak()).truncate());
                let mut tune = !f.ewar_auto;
                if ui.checkbox(&mut tune, tr!("Fine-tune")).on_hover_text(tr!("Type how many land on one target instead of counting every module fitted")).changed() {
                    f.ewar_auto = !tune;
                    if tune {
                        f.ewar = p;
                    }
                    changed = true;
                }
                if tune {
                    ui.horizontal(|ui| {
                        for (n, label, have) in [(&mut f.ewar.webs, tr!("Webs"), p.webs > 0), (&mut f.ewar.painters, tr!("Painters"), p.painters > 0), (&mut f.ewar.tracking_disruptors, tr!("Tracking disruptors"), p.tracking_disruptors > 0)] {
                            if have || *n > 0 {
                                ui.label(label);
                                changed |= ui.add(egui::DragValue::new(n).range(0..=50)).changed();
                            }
                        }
                    });
                }
            });
            ui.end_row();
            ui.label(tr!("Against"));
            let enemy_name = session.enemy.as_ref().and_then(|id| fleets.iter().find(|x| &x.id == id)).map_or_else(|| tr!("none").to_owned(), |x| x.name.clone());
            egui::ComboBox::from_id_salt("sim_fleet_enemy").selected_text(text::clip(&enemy_name, 28)).width(220.0).show_ui(ui, |ui| {
                if ui.menu_label(session.enemy.is_none(), tr!("none")).clicked() {
                    session.enemy = None;
                }
                for x in fleets.iter().filter(|x| x.id != my_id) {
                    if ui.menu_label(session.enemy.as_deref() == Some(x.id.as_str()), x.name.clone()).clicked() {
                        session.enemy = Some(x.id.clone());
                    }
                }
            });
            ui.end_row();
            if !profiles.is_empty() {
                ui.label(tr!("Skills"));
                ui.label(egui::RichText::new(tr!("the profile chosen above")).weak());
                ui.end_row();
            }
        });
    });
    if changed {
        session.dirty = true;
    }
    if let Some(enemy) = sim.fleet.as_ref().and_then(|f| f.enemy.clone()) {
        if !sim.fleet_docs.contains_key(&enemy) {
            sim.request(HostRequest::LoadFleetDoc(enemy));
        }
        let wanted: Vec<String> = sim.fleet_docs.values().flat_map(|e| e.slots.iter().map(|s| s.fit_id.clone()).chain(e.boosters.iter().map(|(_, id)| id.clone()))).collect();
        sim.want_fit_docs(wanted.into_iter());
    }
}

/// The numbers: the fleet, and both directions against the enemy.
pub fn results(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some((mine, enemy)) = sim.fleet_stats() else {
        ui.label(egui::RichText::new(tr!("No fleet open.")).weak());
        return;
    };
    let names: HashMap<String, String> = sim.library.iter().map(|f| (f.id.clone(), f.name.clone())).collect();
    egui::ScrollArea::vertical().id_salt("sim_fleet_results").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        fleet_card(ui, &dogma, &mine, &names);
        if let Some((theirs, a, b)) = &enemy {
            ui.add_space(4.0);
            ui.separator();
            fleet_card(ui, &dogma, theirs, &names);
            against_card(ui, &trf!("{a} against {b}", a = mine.name, b = theirs.name), a);
            against_card(ui, &trf!("{a} against {b}", a = theirs.name, b = mine.name), b);
        } else {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(crate::rows::GAP);
                ui.add(egui::Label::new(egui::RichText::new(tr!("Pick a fleet under Against to compare.")).weak()).wrap());
            });
        }
    });
}

fn line(ui: &mut egui::Ui, label: &str, value: String) {
    crate::stats_panel::line(ui, label, value);
}

fn title(ui: &mut egui::Ui, text: &str, weak: bool) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(crate::rows::GAP);
        let t = egui::RichText::new(text);
        ui.add(egui::Label::new(if weak { t.weak() } else { t.strong() }).truncate());
    });
}

fn fleet_card(ui: &mut egui::Ui, _dogma: &spai_fit::Dogma, s: &FleetStats, names: &HashMap<String, String>) {
    title(ui, &s.name, false);
    line(ui, tr!("Ships"), s.ships.to_string());
    line(ui, tr!("DPS / volley"), trf!("{dps} / {volley}", dps = text::num(s.dps), volley = text::num(s.volley)));
    line(ui, tr!("Logistics"), trf!("{v} / s", v = text::num(s.logi_rep)));
    if s.neut > 0.0 {
        line(ui, tr!("Neuts"), trf!("{v} GJ/s", v = text::num(s.neut)));
    }
    line(ui, tr!("Speed"), trf!("{min} to {max} m/s", min = text::num(s.speed_min), max = text::num(s.speed_max)));
    let p = s.projected;
    if p.webs + p.painters + p.tracking_disruptors > 0 {
        line(ui, tr!("Projects"), trf!("{w} webs, {p} painters, {t} TDs", w = p.webs, p = p.painters, t = p.tracking_disruptors));
    }
    for (i, slot) in s.slots.iter().enumerate() {
        let c = &slot.computed;
        title(ui, &format!("{} x {}{}", slot.count, names.get(&slot.fit_id).unwrap_or(&slot.name), if i == s.main { " *" } else { "" }), true);
        line(ui, tr!("DPS / volley each"), trf!("{dps} / {volley}", dps = text::num(c.dps()), volley = text::num(c.volley())));
        line(ui, tr!("Effective HP"), text::num(c.tank.ehp));
        if let Some(w) = c.weapons.first() {
            line(ui, tr!("Range"), match &w.missile {
                Some(m) => text::metres(m.range),
                None => trf!("{o} + {f}", o = text::metres(w.optimal), f = text::metres(w.falloff)),
            });
        }
        line(ui, tr!("Speed"), trf!("{v} m/s", v = text::num(c.mobility.max_velocity)));
    }
}

fn against_card(ui: &mut egui::Ui, head: &str, a: &Against) {
    ui.add_space(4.0);
    ui.separator();
    title(ui, head, false);
    line(ui, tr!("Applied DPS / volley"), trf!("{dps} / {volley}", dps = text::num(a.applied_dps), volley = text::num(a.applied_volley)));
    line(ui, tr!("Target speed / signature"), trf!("{v} m/s, {s} m", v = text::num(a.target_velocity), s = text::num(a.target_signature)));
    line(ui, tr!("Critical mass"), a.critical_mass.map_or_else(|| tr!("n/a").to_owned(), |n| trf!("{n} ships per volley kill", n = n)));
    line(ui, tr!("Kills per volley"), text::num(a.kills_per_volley));
    line(ui, tr!("Break point (their reps)"), trf!("{v} / s", v = text::num(a.break_point)));
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 19.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(crate::rows::GAP);
        ui.label(tr!("Margin"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(crate::rows::GAP);
            let color = if a.margin >= 0.0 { ui.visuals().hyperlink_color } else { ui.visuals().error_fg_color };
            ui.label(egui::RichText::new(trf!("{v} / s", v = text::num(a.margin))).monospace().color(color));
        });
    });
}
