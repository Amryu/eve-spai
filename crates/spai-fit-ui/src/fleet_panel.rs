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

pub fn role_name(role: spai_fit::fleet::Role) -> &'static str {
    use spai_fit::fleet::Role as R;
    match role {
        R::Main => tr!("Main damage"),
        R::Mainline => tr!("Other damage"),
        R::Logi => tr!("Logistics"),
        R::Support => tr!("Support"),
        R::Tackle => tr!("Tackle"),
        R::Ewar => tr!("Ewar"),
        R::Command => tr!("Command"),
    }
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
    pub(crate) fn want_fit_docs(&mut self, ids: impl Iterator<Item = String>) {
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

/// The fight between the open fleet and its opponent: ammo per range, the sweep over ranges, and
/// the race at the fleet's range.
pub struct Analysis {
    pub fleet: Fleet,
    pub range: f32,
    pub mine: FleetStats,
    pub theirs: FleetStats,
    pub enemy: Fleet,
    pub ammo: [Vec<Vec<spai_fit::fleet::ammo::AmmoChoice>>; 2],
    pub sweep: spai_fit::fleet::sweep::Sweep,
    pub race: spai_fit::fleet::race::Race,
}

impl SimUi {
    /// The engagement analysis, worked out again only when either fleet, the fits they fly or the
    /// skills change.
    pub(crate) fn fleet_analysis(&mut self) -> Option<std::sync::Arc<Analysis>> {
        use std::hash::{Hash, Hasher};
        let (dogma, f) = (self.dogma.clone()?, self.fleet.as_ref()?);
        let enemy = f.enemy.as_ref().and_then(|id| self.fleet_docs.get(id))?.clone();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&f.fleet).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&enemy).unwrap_or_default().hash(&mut h);
        let mut ids: Vec<&String> = self.fit_docs.keys().collect();
        ids.sort();
        ids.hash(&mut h);
        self.profile.hash(&mut h);
        let key = h.finish();
        if let Some((k, a)) = &self.fleet_analysis {
            if *k == key {
                return Some(a.clone());
            }
        }
        let docs = &self.fit_docs;
        let fits = |id: &str| docs.get(id).cloned();
        let skills = |p: Option<&str>| self.fleet_skills(p);
        let mine = fleet::stats(&dogma, &f.fleet, &fits, &skills);
        let theirs = fleet::stats(&dogma, &enemy, &fits, &skills);
        let ammo_of = |s: &FleetStats| s.slots.iter().map(|x| spai_fit::fleet::ammo::choices(&dogma, &x.fit, &x.skills, &s.env)).collect::<Vec<_>>();
        let ammo = [ammo_of(&mine), ammo_of(&theirs)];
        let a = spai_fit::fleet::sweep::Side { fleet: &f.fleet, stats: &mine, ammo: &ammo[0] };
        let b = spai_fit::fleet::sweep::Side { fleet: &enemy, stats: &theirs, ammo: &ammo[1] };
        let mut ranges = spai_fit::fleet::sweep::ranges(&mine, &theirs);
        ranges.push(f64::from(f.fleet.range));
        ranges.sort_by(|x, y| x.total_cmp(y));
        ranges.dedup();
        let sweep = spai_fit::fleet::sweep::sweep(&a, &b, &ranges);
        let race = spai_fit::fleet::race::race(&dogma, &a, &b, f64::from(f.fleet.range), 1800.0);
        let out = std::sync::Arc::new(Analysis { fleet: f.fleet.clone(), range: f.fleet.range, mine, theirs, enemy, ammo, sweep, race });
        self.fleet_analysis = Some((key, out.clone()));
        Some(out)
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
    let dogma = sim.dogma.clone();
    // Per burst kind, the fits that carry its module, among those loaded.
    let boosting: Vec<(BurstKind, Vec<(String, String)>)> = BurstKind::ALL
        .iter()
        .map(|&kind| {
            let fits = library.iter().map(|x| (x.id.clone(), x.name.clone())).chain(doctrine_fits.iter().cloned()).filter(|(id, _)| dogma.as_ref().zip(sim.fit_docs.get(id)).is_some_and(|(d, f)| fleet::fit_has_burst(d, f, kind))).collect();
            (kind, fits)
        })
        .collect();
    let mut want_all_docs = false;
    // Each fit's variations, to fly a slot with one of them.
    let variations: HashMap<String, Vec<(u32, String)>> = sim.fit_docs.iter().map(|(id, f)| (id.clone(), f.variations.iter().map(|v| (v.id, v.name.clone())).collect())).collect();
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
            let variation_box = |ui: &mut egui::Ui, slot: &mut FleetSlot, changed: &mut bool| {
                let list = variations.get(&slot.fit_id).cloned().unwrap_or_default();
                let flying = slot.variation.and_then(|v| list.iter().find(|(x, _)| *x == v)).map_or_else(|| tr!("Base").to_owned(), |(_, n)| n.clone());
                ui.add_enabled_ui(!list.is_empty(), |ui| {
                    egui::ComboBox::from_id_salt(("sim_fleet_variation", i)).selected_text(text::clip(&flying, 14)).width(120.0).show_ui(ui, |ui| {
                        if ui.menu_label(slot.variation.is_none(), tr!("Base")).clicked() {
                            slot.variation = None;
                            *changed = true;
                        }
                        for (v, n) in &list {
                            if ui.menu_label(slot.variation == Some(*v), n.clone()).clicked() {
                                slot.variation = Some(*v);
                                *changed = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text(tr!("The variation this slot flies"));
                });
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
                let rest = if narrow { 40.0 } else { 490.0 };
                let fit_w = (ui.available_width() - rest).max(100.0);
                egui::ComboBox::from_id_salt(("sim_fleet_fit", i)).selected_text(text::clip(&fit_name, 28)).width(fit_w).show_ui(ui, |ui| {
                    let before = slot.fit_id.clone();
                    for x in &library {
                        changed |= ui.menu_value(&mut slot.fit_id, x.id.clone(), x.name.clone()).changed();
                    }
                    for (id, name) in &doctrine_fits {
                        changed |= ui.menu_value(&mut slot.fit_id, id.clone(), name.clone()).changed();
                    }
                    // Another fit has other variations.
                    if slot.fit_id != before {
                        slot.variation = None;
                    }
                });
                if !narrow {
                    variation_box(ui, slot, &mut changed);
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
                    variation_box(ui, &mut f.slots[i], &mut changed);
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
        for (kind, fits) in &boosting {
            let kind = *kind;
            ui.horizontal(|ui| {
                ui.set_height(24.0);
                ui.add_sized([90.0, 22.0], egui::Label::new(kind_name(kind)));
                let current = f.boosters.iter().find(|(k, _)| *k == kind).map(|(_, id)| id.clone());
                let shortcut = f.boost_shortcuts.iter().position(|b| b.kind == kind);
                let name = if let Some(i) = shortcut {
                    let hull = f.boost_shortcuts[i].hull.zip(dogma.as_ref()).map_or_else(|| tr!("no hull bonus").to_owned(), |(h, d)| spai_fit::formats::name_of(d, h));
                    trf!("Generic booster · {hull}", hull = hull)
                } else {
                    current.as_ref().and_then(|id| library.iter().find(|x| &x.id == id).map(|x| x.name.clone()).or_else(|| doctrine_fits.iter().find(|(d, _)| d == id).map(|(_, n)| n.clone()))).unwrap_or_else(|| tr!("none").to_owned())
                };
                // None clears both; Some(Ok(fit)) a fit; Some(Err(hull)) a generic booster.
                let mut pick: Option<Option<Result<String, Option<spai_fit::ids::TypeId>>>> = None;
                let combo_w = (ui.available_width() - 8.0 - if shortcut.is_some() { 4.0 * 26.0 } else { 0.0 }).clamp(120.0, 260.0);
                egui::ComboBox::from_id_salt(("sim_fleet_booster", kind_name(kind))).selected_text(text::clip(&name, 36)).width(combo_w).show_ui(ui, |ui| {
                    want_all_docs = true;
                    if ui.menu_label(current.is_none() && shortcut.is_none(), tr!("none")).clicked() {
                        pick = Some(None);
                    }
                    for (id, n) in fits {
                        if ui.menu_label(current.as_deref() == Some(id.as_str()), n.clone()).clicked() {
                            pick = Some(Some(Ok(id.clone())));
                        }
                    }
                    if let Some(d) = &dogma {
                        ui.separator();
                        ui.label(egui::RichText::new(tr!("Generic booster, in no slot")).weak());
                        let on_hull = |h: Option<spai_fit::ids::TypeId>| shortcut.is_some_and(|i| f.boost_shortcuts[i].hull == h);
                        if ui.menu_label(on_hull(None), tr!("A hull without burst bonuses")).clicked() {
                            pick = Some(Some(Err(None)));
                        }
                        for hull in fleet::boosting_hulls(d, kind) {
                            if ui.menu_label(on_hull(Some(hull)), spai_fit::formats::name_of(d, hull)).clicked() {
                                pick = Some(Some(Err(Some(hull))));
                            }
                        }
                    }
                });
                // The generic booster's buttons, as on a fit's projected tab.
                if let (Some(i), Some(d)) = (shortcut, &dogma) {
                    let b = f.boost_shortcuts[i].clone();
                    let mut next = b.clone();
                    let module = b.module.or_else(|| d.type_named(b.kind.module_name()).map(|t| t.id)).unwrap_or(0);
                    let skills_btn = spai_ui::widgets::icon_button(ui, icon::GRADUATION_CAP).on_hover_text(tr!("The booster's skills"));
                    crate::dock::shortcut_skills_popup(&skills_btn, d, &b, &mut next, ("sim_fleet_boost_skills", i));
                    if spai_ui::widgets::icon_button(ui, crate::dock::mindlink_glyph(ui, b.mindlink)).on_hover_text(crate::dock::mindlink_hint(b.mindlink)).clicked() {
                        next.mindlink = !next.mindlink;
                    }
                    let charges_btn = spai_ui::widgets::icon_button(ui, icon::LIST_CHECKS).on_hover_text(tr!("The burst module and its charges"));
                    crate::dock::shortcut_charges_popup(&charges_btn, d, &b, &mut next, module);
                    if spai_ui::widgets::icon_button(ui, crate::dock::switch_glyph(ui, b.active)).on_hover_text(if b.active { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                        next.active = !next.active;
                    }
                    if next != b {
                        f.boost_shortcuts[i] = next;
                        changed = true;
                    }
                }
                if let Some(p) = pick {
                    f.boosters.retain(|(k, _)| *k != kind);
                    match p {
                        Some(Err(hull)) => match f.boost_shortcuts.iter_mut().find(|b| b.kind == kind) {
                            // Another hull keeps the skills, mindlink and charges set.
                            Some(b) => b.hull = hull,
                            None => f.boost_shortcuts.push(spai_fit::fit::BoostShortcut { kind, hull, ..Default::default() }),
                        },
                        Some(Ok(id)) => {
                            f.boost_shortcuts.retain(|b| b.kind != kind);
                            f.boosters.push((kind, id));
                        }
                        None => f.boost_shortcuts.retain(|b| b.kind != kind),
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
            // How the ships sit: a ball of a radius, or a flock bell-curved around the anchor.
            ui.label(tr!("Formation")).on_hover_text(tr!("Ball: every ship on a sphere of this radius around the anchor. Flock: ships follow the anchor, their distance to it spread like a bell curve of this width."));
            ui.horizontal(|ui| {
                use spai_ui::widgets::SteadySelect as _;
                let (ball, mut metres) = match f.formation {
                    spai_fit::fleet::Formation::Ball { radius_m } => (true, radius_m),
                    spai_fit::fleet::Formation::Flock { sigma_m } => (false, sigma_m),
                };
                let mut pick_ball = ball;
                if ui.menu_label_sized(egui::vec2(60.0, 22.0), ball, tr!("Ball")).clicked() {
                    pick_ball = true;
                }
                if ui.menu_label_sized(egui::vec2(60.0, 22.0), !ball, tr!("Flock")).clicked() {
                    pick_ball = false;
                }
                let mut km = metres / 1000.0;
                let r = ui.add(egui::DragValue::new(&mut km).range(0.0..=100.0).speed(0.1).suffix(" km"));
                if r.changed() || pick_ball != ball {
                    metres = km * 1000.0;
                    f.formation = if pick_ball { spai_fit::fleet::Formation::Ball { radius_m: metres } } else { spai_fit::fleet::Formation::Flock { sigma_m: metres } };
                    changed = true;
                }
            });
            ui.end_row();
            ui.label(tr!("Movement")).on_hover_text(tr!("The fleet's share of its slowest ship's top speed and its heading against the line to the enemy: 0° straight at it, 90° across."));
            let before = f.motion;
            crate::graphs_panel::motion_dial(ui, "", &mut f.motion);
            changed |= f.motion != before;
            ui.end_row();
            ui.label(tr!("Drones"));
            changed |= ui.checkbox(&mut f.drones, tr!("Launched")).on_hover_text(tr!("Drones and fighters add damage per second, never to a volley: they attack on their own time, no alpha before logi react")).changed();
            ui.end_row();
            ui.label(tr!("Assault damage controls"));
            changed |= ui.checkbox(&mut f.adc, tr!("Switched on when primaried")).changed();
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
                    f.enemy = None;
                    changed = true;
                }
                for x in fleets.iter().filter(|x| x.id != my_id) {
                    if ui.menu_label(session.enemy.as_deref() == Some(x.id.as_str()), x.name.clone()).clicked() {
                        session.enemy = Some(x.id.clone());
                        f.enemy = Some(x.id.clone());
                        changed = true;
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
        ui.add_space(4.0);
        ui.label(tr!("Shoots first")).on_hover_text(tr!("The order this fleet calls the enemy's ships as primary, by role"));
        ui.horizontal_wrapped(|ui| {
            let mut up: Option<usize> = None;
            // One widget per entry so the row wraps between entries.
            for (i, role) in f.target_order.iter().enumerate() {
                let name = format!("{}. {}", i + 1, role_name(*role));
                if i == 0 {
                    ui.label(name);
                } else if ui.button(format!("{} {name}", egui_phosphor::regular::CARET_LEFT)).on_hover_text(tr!("Earlier")).clicked() {
                    up = Some(i);
                }
            }
            if let Some(i) = up {
                f.target_order.swap(i, i - 1);
                changed = true;
            }
        });
    });
    if changed {
        session.dirty = true;
    }
    if want_all_docs {
        let all: Vec<String> = sim.library.iter().map(|x| x.id.clone()).chain(sim.doctrines.iter().map(|d| d.id.clone())).collect();
        sim.want_fit_docs(all.into_iter());
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
    let analysis = sim.fleet_analysis();
    egui::ScrollArea::vertical().id_salt("sim_fleet_results").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        if let Some(an) = &analysis {
            engagement_card(ui, &dogma, an, &names);
            ui.add_space(4.0);
            ui.separator();
        }
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

/// Where each fleet wants to fight, what holds it there, which ammo for which range, and how the
/// fight at the set range goes.
fn engagement_card(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, an: &Analysis, names: &HashMap<String, String>) {
    use spai_fit::fleet::race::{Event, Verdict};
    let (a, b) = (&an.mine.name, &an.theirs.name);
    let slot_name = |s: &spai_fit::fleet::SlotStats| names.get(&s.fit_id).cloned().unwrap_or_else(|| s.fit_id.clone());
    title(ui, tr!("Engagement"), false);
    let sw = &an.sweep;
    line(ui, &trf!("{a} best range", a = a), text::metres(sw.a_best));
    line(ui, &trf!("{a} best range", a = b), text::metres(sw.b_best));
    let bands: Vec<String> = sw.a_favoured.iter().map(|(f, t)| format!("{} - {}", text::metres(*f), text::metres(*t))).collect();
    line(ui, &trf!("{a} favoured", a = a), if bands.is_empty() { tr!("nowhere").to_owned() } else { bands.join(", ") });
    line(ui, &trf!("{a} favoured", a = b), {
        let v: Vec<String> = sw.b_favoured.iter().map(|(f, t)| format!("{} - {}", text::metres(*f), text::metres(*t))).collect();
        if v.is_empty() { tr!("nowhere").to_owned() } else { v.join(", ") }
    });
    for (side, name, cuts) in [(0, a, &sw.a_cutoffs), (1, b, &sw.b_cutoffs)] {
        if cuts.is_empty() {
            continue;
        }
        title(ui, &trf!("{a} ewar reach", a = name), true);
        for (kind, optimal, edge) in cuts {
            let Some(label) = crate::stats_panel::ewar_name(*kind) else { continue };
            line(ui, label, trf!("{o}, gone past {e}", o = text::metres(*optimal), e = text::metres(*edge)));
        }
        // Neutralisers are counted against recharge in the score; the pilots still watch them.
        let neut = cuts.iter().filter(|(k, _, _)| matches!(k, spai_fit::stats::RemoteKind::Neut)).map(|(_, o, _)| *o).fold(0.0, f64::max);
        if neut > 0.0 && f64::from(an.range) <= neut * 1.2 {
            let victim = if side == 0 { b } else { a };
            warn(ui, &trf!("{v} sits in or near neutraliser range ({r})", v = victim, r = text::metres(neut)));
        }
    }
    let at = |d: f64| sw.points.iter().min_by(|x, y| (x.distance - d).abs().total_cmp(&(y.distance - d).abs()));
    if let Some(p) = at(f64::from(an.range)) {
        title(ui, &trf!("At {r}", r = text::metres(p.distance)), true);
        for (name, f) in [(a, &p.a), (b, &p.b)] {
            let mut holds = Vec::new();
            if f.webbed {
                holds.push(tr!("webbed").to_owned());
            }
            if f.tackled {
                holds.push(tr!("tackled").to_owned());
            }
            if f.neut_pressure > 0.05 {
                holds.push(trf!("neuts {p}% of recharge", p = format!("{:.0}", f.neut_pressure * 100.0)));
            }
            let extra = if holds.is_empty() { String::new() } else { format!(", {}", holds.join(", ")) };
            line(ui, name, trf!("{d} DPS on the primary{x}", d = text::num(f.applied_dps), x = extra));
        }
    }
    let r = &an.race;
    title(ui, tr!("Race at this range"), true);
    let winner = match r.winner {
        Some(0) => trf!("{a} wins in {s}", a = a, s = text::secs(r.seconds)),
        Some(_) => trf!("{a} wins in {s}", a = b, s = text::secs(r.seconds)),
        None => trf!("both stand after {s}", s = text::secs(r.seconds)),
    };
    line(ui, tr!("Outcome"), winner);
    for (side, name) in [(0, a), (1, b)] {
        if let Some(t) = r.critical_mass_lost[side] {
            let alive = r.timeline.iter().find(|x| x.0 >= t).map_or(0, |x| if side == 0 { x.1 } else { x.2 });
            line(ui, &trf!("{a} loses critical mass", a = name), trf!("at {s}, {n} ships left", s = text::secs(t), n = alive));
        }
    }
    let adcs = r.events.iter().filter(|(_, e)| matches!(e, Event::Adc { .. })).count();
    if adcs > 0 {
        line(ui, tr!("Assault damage controls"), trf!("{n} switched on", n = adcs));
    }
    for (side, stats) in [(0, &an.mine), (1, &an.theirs)] {
        if r.verdicts[side].is_empty() {
            continue;
        }
        title(ui, &trf!("{a} when primaried", a = stats.name), true);
        for (k, v) in &r.verdicts[side] {
            let Some(s) = stats.slots.get(*k) else { continue };
            let text = match v {
                Verdict::OneVolley => tr!("dies in one volley").to_owned(),
                Verdict::BeforeReps { seconds } => trf!("dies in {s}, before reps land", s = text::secs(*seconds)),
                Verdict::Outpaced { seconds, by } if *by > 0.0 && has_logi(stats) => trf!("dies in {s}, damage {d}/s over reps", s = text::secs(*seconds), d = text::num(*by)),
                Verdict::Outpaced { seconds, .. } => trf!("dies in {s}", s = text::secs(*seconds)),
                Verdict::Held => tr!("holds").to_owned(),
            };
            line(ui, &slot_name(s), text);
        }
    }
    // Ammo per range for the open fleet, against the enemy's first target.
    let ranges: Vec<f64> = sw.points.iter().map(|p| p.distance).collect();
    let Some(target) = spai_fit::fleet::sweep::primary(&an.fleet, &an.theirs) else { return };
    let profile = spai_fit::fit::TargetProfile { signature: target.computed.mobility.signature as f32, velocity: 0.0, distance: 0.0, transversal: 0.0, radius: 0.0 };
    let mut shown = false;
    for (k, slot) in an.mine.slots.iter().enumerate() {
        for c in an.ammo[0].get(k).into_iter().flatten() {
            let bands = spai_fit::fleet::ammo::bands(c, &ranges, &profile);
            if bands.len() < 2 && bands.iter().all(|b| b.better.is_none()) {
                continue;
            }
            if !shown {
                title(ui, tr!("Ammo by range"), true);
                shown = true;
            }
            let gun = dogma.type_(c.weapon).map_or("?", |t| t.name.as_str());
            line(ui, &format!("{} · {}", slot_name(slot), gun), String::new());
            for band in bands {
                let charge = dogma.type_(band.charge).map_or("?", |t| t.name.as_str());
                line(ui, &format!("{} - {}", text::metres(band.from), text::metres(band.to)), charge.to_owned());
                if let Some(t) = band.better.and_then(|(id, share)| dogma.type_(id).map(|t| (t.name.clone(), share))) {
                    line(ui, "", trf!("better, not carried: {c} +{p}%", c = t.0, p = format!("{:.0}", t.1 * 100.0)));
                }
            }
        }
    }
}

fn has_logi(s: &FleetStats) -> bool {
    s.slots.iter().any(|x| x.count > 0 && x.computed.remote.modules.iter().any(|m| m.kind.is_rep()))
}

fn warn(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(crate::rows::GAP);
        ui.add(egui::Label::new(egui::RichText::new(format!("{} {text}", egui_phosphor::regular::WARNING)).color(ui.visuals().warn_fg_color)).wrap());
    });
}

fn fleet_card(ui: &mut egui::Ui, _dogma: &spai_fit::Dogma, s: &FleetStats, names: &HashMap<String, String>) {
    title(ui, &s.name, false);
    line(ui, tr!("Ships"), s.ships.to_string());
    line(ui, tr!("DPS / volley"), trf!("{dps} / {volley}", dps = text::num(s.dps), volley = text::num(s.volley)));
    line(ui, tr!("Logistics"), trf!("{v}/s", v = text::num(s.logi_rep)));
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
    line(ui, tr!("Break point (their reps)"), trf!("{v}/s", v = text::num(a.break_point)));
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 19.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(crate::rows::GAP);
        ui.label(tr!("Margin"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(crate::rows::GAP);
            let color = if a.margin >= 0.0 { ui.visuals().hyperlink_color } else { ui.visuals().error_fg_color };
            ui.label(egui::RichText::new(trf!("{v}/s", v = text::num(a.margin))).monospace().color(color));
        });
    });
}
