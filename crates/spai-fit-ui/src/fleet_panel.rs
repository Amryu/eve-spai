//! Fleets: which fits fly in what numbers, the boosters, and how the fleet fares against another.

use std::collections::HashMap;
use std::sync::Arc;

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

/// Opens an opponent id naming an enemy doctrine group rather than a saved fleet.
pub const ENEMY_DOCTRINE: &str = "doctrine:";
/// The opponent id of a mirror match: the open fleet against itself.
pub const MIRROR: &str = "mirror";

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
        self.doctrine_groups_of(None)
    }

    /// The doctrine groups of one side only, or of all.
    pub fn doctrine_groups_of(&self, side: Option<crate::DoctrineSide>) -> Vec<(String, Vec<String>)> {
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for d in self.doctrines.iter().filter(|d| side.is_none_or(|s| d.side == s)) {
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

    /// The opponent by its id: a saved fleet, or an enemy doctrine group (`ENEMY_DOCTRINE` and its
    /// name) built into a fleet of the open one's size.
    pub(crate) fn enemy_fleet(&self, id: &str) -> Option<Fleet> {
        if id == MIRROR {
            // The fleet as it is being edited, not as last saved.
            let mut fleet = self.fleet.as_ref()?.fleet.clone();
            fleet.name = trf!("{name} (mirror)", name = fleet.name);
            fleet.enemy = None;
            fleet.scene = None;
            return Some(fleet);
        }
        let Some(group) = id.strip_prefix(ENEMY_DOCTRINE) else { return self.fleet_docs.get(id).cloned() };
        use std::hash::{Hash, Hasher};
        let dogma = self.dogma.as_ref()?;
        let (_, ids) = self.doctrine_groups_of(Some(crate::DoctrineSide::Enemy)).into_iter().find(|(g, _)| g == group)?;
        let fits: Vec<(String, Fit)> = ids.iter().filter_map(|id| self.fit_docs.get(id).map(|f| (id.clone(), f.clone()))).collect();
        if fits.is_empty() {
            return None;
        }
        let size = self.fleet.as_ref().map_or(50, |f| f.fleet.size);
        // Building it computes every fit; done once per what it is built from.
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (id, size, self.profile).hash(&mut h);
        serde_json::to_string(&fits).unwrap_or_default().hash(&mut h);
        let key = h.finish();
        if let Some((_, f)) = self.fleet_cache.borrow().doctrine.iter().find(|(k, _)| *k == key) {
            return Some(f.clone());
        }
        let mut fleet = spai_fit::fleet::from_doctrine(dogma, group, &fits, self.skills());
        fleet.id = id.to_owned();
        fleet.size = size;
        let mut cache = self.fleet_cache.borrow_mut();
        cache.doctrine.insert(0, (key, fleet.clone()));
        cache.doctrine.truncate(CACHED);
        Some(fleet)
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
            self.fleet_sent = Some(content_hash(&f.fleet));
            self.requests.push(HostRequest::SaveFleet { id: f.id.clone(), name: f.fleet.name.clone(), doc: serde_json::to_string(&f.fleet).unwrap_or_default() });
        }
    }

    fn fleet_skills(&self, profile: Option<&str>) -> SkillSet {
        profile.and_then(|p| self.profiles.iter().find(|x| x.id == p)).map_or_else(|| self.skills().clone(), |p| p.skills.clone())
    }

    /// The fleet's numbers, and the enemy's with both comparisons, from the documents at hand.
    /// Each fleet's numbers are worked out once per content (`stats_key`), so a frame only
    /// redoes the two comparisons.
    pub fn fleet_stats(&self) -> Option<(Arc<FleetStats>, Option<(Arc<FleetStats>, Against, Against)>)> {
        let (dogma, f) = (self.dogma.as_ref()?, self.fleet.as_ref()?);
        let mine = self.cached_stats(&f.fleet);
        let enemy = f.enemy.as_ref().and_then(|id| self.enemy_fleet(id)).map(|e| {
            let theirs = self.cached_stats(&e);
            let a = fleet::against(dogma, &f.fleet, &mine, &e, &theirs);
            let b = fleet::against(dogma, &e, &theirs, &f.fleet, &mine);
            (theirs, a, b)
        });
        Some((mine, enemy))
    }

    /// What a fleet's numbers depend on: its slots, boosters, size and main, the documents of the
    /// fits it flies and the skills they fly with. Not its range, motion or formation.
    pub(crate) fn stats_key(&self, f: &Fleet) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&(&f.name, f.size, &f.slots, &f.boosters, &f.boost_shortcuts, f.main, f.drones)).unwrap_or_default().hash(&mut h);
        for id in f.slots.iter().map(|s| &s.fit_id).chain(f.boosters.iter().map(|(_, id)| id)) {
            id.hash(&mut h);
            if let Some(doc) = self.fit_docs.get(id) {
                serde_json::to_string(doc).unwrap_or_default().hash(&mut h);
            }
        }
        let mut profiles: Vec<Option<&str>> = f.slots.iter().map(|s| s.profile.as_deref()).collect();
        profiles.sort();
        profiles.dedup();
        for p in profiles {
            let sk = self.fleet_skills(p);
            let mut levels: Vec<(u32, u8)> = sk.levels.iter().map(|(k, v)| (*k, *v)).collect();
            levels.sort_unstable();
            (p, sk.fallback, levels).hash(&mut h);
        }
        h.finish()
    }

    fn cached_stats(&self, f: &Fleet) -> Arc<FleetStats> {
        let key = self.stats_key(f);
        if let Some((_, s)) = self.fleet_cache.borrow().stats.iter().find(|(k, _)| *k == key) {
            return s.clone();
        }
        let Some(dogma) = self.dogma.as_ref() else { return Arc::new(FleetStats::default()) };
        let docs = &self.fit_docs;
        let fits = |id: &str| docs.get(id).cloned();
        let skills = |p: Option<&str>| self.fleet_skills(p);
        let s = Arc::new(fleet::stats(dogma, f, &fits, &skills));
        let mut cache = self.fleet_cache.borrow_mut();
        cache.stats.insert(0, (key, s.clone()));
        cache.stats.truncate(CACHED);
        s
    }

    /// Every weapon's options among its charges. Each recomputes the fit, the dearest part of an
    /// analysis, so they are kept per fit, skills and boosts: not per fleet, whose size or counts
    /// change them not.
    fn cached_ammo(&self, s: &FleetStats) -> Arc<Vec<Vec<spai_fit::fleet::ammo::AmmoChoice>>> {
        use std::hash::{Hash, Hasher};
        let Some(dogma) = self.dogma.as_ref() else { return Arc::default() };
        let env = format!("{:?}", s.env);
        let out = s
            .slots
            .iter()
            .map(|x| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                serde_json::to_string(&x.fit).unwrap_or_default().hash(&mut h);
                let mut levels: Vec<(u32, u8)> = x.skills.levels.iter().map(|(k, v)| (*k, *v)).collect();
                levels.sort_unstable();
                (levels, x.skills.fallback, &env).hash(&mut h);
                let key = h.finish();
                if let Some((_, a)) = self.fleet_cache.borrow().ammo.iter().find(|(k, _)| *k == key) {
                    return (**a).clone();
                }
                let a = Arc::new(spai_fit::fleet::ammo::choices(dogma, &x.fit, &x.skills, &s.env));
                let mut cache = self.fleet_cache.borrow_mut();
                cache.ammo.insert(0, (key, a.clone()));
                cache.ammo.truncate(CACHED_SLOTS);
                (*a).clone()
            })
            .collect();
        Arc::new(out)
    }
}

/// Slots' ammo options kept at once.
const CACHED_SLOTS: usize = 48;

/// A fleet document's content, to tell an edit from what was saved.
pub fn content_hash(f: &Fleet) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(f).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// Fleets' numbers and ammo options kept at once: the open one, its opponent, a few before.
const CACHED: usize = 6;

/// How long the knobs must rest before the engagement is worked out again; the last one stays on
/// screen meanwhile.
pub const SETTLE_MS: u64 = 250;

#[derive(Default)]
pub struct FleetCache {
    stats: Vec<(u64, Arc<FleetStats>)>,
    ammo: Vec<(u64, Arc<Vec<spai_fit::fleet::ammo::AmmoChoice>>)>,
    /// Enemy doctrine groups built into fleets, by what they were built from.
    doctrine: Vec<(u64, Fleet)>,
    /// An analysis asked for but not yet worked out, and since when.
    pending: Option<(u64, u64)>,
    /// The volley window's figures under its what-if overrides: by analysis, direction, target
    /// slot and the overrides, the charge options and the best charges' application.
    what_if: Option<(Arc<Analysis>, usize, usize, String, Arc<Vec<spai_fit::fleet::sweep::ChargeOption>>, Arc<Vec<spai_fit::fleet::sweep::Application>>)>,
    /// The weapons section's figures, by analysis, direction and target slot.
    apps: Option<(Arc<Analysis>, usize, usize, Arc<Vec<spai_fit::fleet::sweep::Application>>)>,
    /// Per burst kind the fits carrying it, by the fits and documents at hand.
    boosting: Option<(u64, Vec<(BurstKind, Vec<(String, String)>)>)>,
}

/// The fight between the open fleet and its opponent: ammo per range, the sweep over ranges, and
/// the race at the fleet's range.
pub struct Analysis {
    pub fleet: Fleet,
    pub range: f32,
    pub mine: Arc<FleetStats>,
    pub theirs: Arc<FleetStats>,
    pub enemy: Fleet,
    pub ammo: [Arc<Vec<Vec<spai_fit::fleet::ammo::AmmoChoice>>>; 2],
    pub sweep: spai_fit::fleet::sweep::Sweep,
    pub race: spai_fit::fleet::race::Race,
    /// Per side, per slot and charged weapon, its best carried charge by range against the other
    /// side's first target, ewar counted.
    pub ammo_bands: [Vec<(usize, spai_fit::ids::TypeId, Vec<spai_fit::fleet::ammo::Band>)>; 2],
}

impl SimUi {
    /// The engagement analysis. Worked out again once the fleets, their fits or skills, or the
    /// fight's settings change and then rest for `SETTLE_MS`; until then the last one is given,
    /// so dragging a knob stays smooth.
    pub(crate) fn fleet_analysis(&mut self) -> Option<Arc<Analysis>> {
        use std::hash::{Hash, Hasher};
        let (dogma, f) = (self.dogma.clone()?, self.fleet.as_ref()?);
        let mut enemy = f.enemy.as_ref().and_then(|id| self.enemy_fleet(id))?;
        // Placed in the scene, the opponent moves as it was placed.
        if let Some(scene) = &f.fleet.scene {
            enemy.motion = scene.read().2;
        }
        let (mine_key, theirs_key) = (self.stats_key(&f.fleet), self.stats_key(&enemy));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (mine_key, theirs_key).hash(&mut h);
        for x in [&f.fleet, &enemy] {
            serde_json::to_string(&(x.range, x.motion, x.formation, &x.target_order, x.adc, x.volley_efficiency, x.ewar_auto, &x.ewar)).unwrap_or_default().hash(&mut h);
        }
        let key = h.finish();
        let now = self.now_ms;
        let cached = self.fleet_analysis.as_ref().map(|(k, a)| (*k, a.clone()));
        if let Some((k, a)) = &cached {
            if *k == key {
                self.fleet_cache.get_mut().pending = None;
                return Some(a.clone());
            }
            let pending = &mut self.fleet_cache.get_mut().pending;
            match pending {
                Some((pk, since)) if *pk == key => {
                    if now.saturating_sub(*since) < SETTLE_MS {
                        return Some(a.clone());
                    }
                }
                _ => {
                    *pending = Some((key, now));
                    return Some(a.clone());
                }
            }
        }
        self.fleet_cache.get_mut().pending = None;
        let mine = self.cached_stats(&f.fleet);
        let theirs = self.cached_stats(&enemy);
        let ammo = [self.cached_ammo(&mine), self.cached_ammo(&theirs)];
        let f = self.fleet.as_ref()?;
        let a = spai_fit::fleet::sweep::Side { fleet: &f.fleet, stats: &mine, ammo: &ammo[0] };
        let b = spai_fit::fleet::sweep::Side { fleet: &enemy, stats: &theirs, ammo: &ammo[1] };
        let mut ranges = spai_fit::fleet::sweep::ranges(&mine, &theirs);
        ranges.push(f64::from(f.fleet.range));
        ranges.sort_by(|x, y| x.total_cmp(y));
        ranges.dedup();
        let sweep = spai_fit::fleet::sweep::sweep(&a, &b, &ranges);
        let race = spai_fit::fleet::race::race(&dogma, &a, &b, f64::from(f.fleet.range), 1800.0);
        let ranges: Vec<f64> = sweep.points.iter().map(|p| p.distance).collect();
        let bands_of = |x: &spai_fit::fleet::sweep::Side, y: &spai_fit::fleet::sweep::Side| -> Vec<(usize, spai_fit::ids::TypeId, Vec<spai_fit::fleet::ammo::Band>)> {
            let Some(t) = spai_fit::fleet::sweep::primary(x.fleet, y.stats) else { return Vec::new() };
            (0..x.stats.slots.len()).filter(|k| x.stats.slots[*k].count > 0).flat_map(|k| spai_fit::fleet::sweep::ammo_bands(x, y, k, t, &ranges).into_iter().map(move |(w, b)| (k, w, b))).collect()
        };
        let ammo_bands = [bands_of(&a, &b), bands_of(&b, &a)];
        let out = Arc::new(Analysis { fleet: f.fleet.clone(), range: f.fleet.range, mine: mine.clone(), theirs: theirs.clone(), enemy, ammo, sweep, race, ammo_bands });
        self.fleet_analysis = Some((key, out.clone()));
        Some(out)
    }

    /// Whether an analysis waits for the knobs to rest, so the host keeps drawing until it is done.
    pub fn fleet_analysis_pending(&self) -> bool {
        self.fleet_cache.borrow().pending.is_some()
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
    // Per burst kind, the fits that carry its module, among those loaded; looked through again
    // only when the set of fits or documents changes.
    let boosting: Vec<(BurstKind, Vec<(String, String)>)> = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut ids: Vec<&String> = sim.fit_docs.keys().collect();
        ids.sort();
        (ids, library.iter().map(|x| (&x.id, &x.name)).collect::<Vec<_>>(), &doctrine_fits).hash(&mut h);
        let key = h.finish();
        let hit = sim.fleet_cache.borrow().boosting.as_ref().filter(|(k, _)| *k == key).map(|(_, v)| v.clone());
        hit.unwrap_or_else(|| {
            let v: Vec<(BurstKind, Vec<(String, String)>)> = BurstKind::ALL
                .iter()
                .map(|&kind| {
                    let fits = library.iter().map(|x| (x.id.clone(), x.name.clone())).chain(doctrine_fits.iter().cloned()).filter(|(id, _)| dogma.as_ref().zip(sim.fit_docs.get(id)).is_some_and(|(d, f)| fleet::fit_has_burst(d, f, kind))).collect();
                    (kind, fits)
                })
                .collect();
            sim.fleet_cache.borrow_mut().boosting = Some((key, v.clone()));
            v
        })
    };
    let mut want_all_docs = false;
    let enemy_doctrines: Vec<String> = sim.doctrine_groups_of(Some(crate::DoctrineSide::Enemy)).into_iter().map(|(g, _)| g).collect();
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
            ui.label(tr!("Formation")).on_hover_text(tr!("Ball: every ship on a sphere of this radius around the anchor. Flock: a swarm strung out along the heading, densest in the middle; this is its spread along the line, less across it and up and down."));
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
            ui.label(tr!("Logistics efficiency")).on_hover_text(tr!("The share of the logistics' repairs that does any good: late locks, the wrong ship, cycles landing on full health"));
            changed |= ui.add(egui::Slider::new(&mut f.logi_efficiency, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
            ui.end_row();
            ui.label(tr!("Logi pre-locked")).on_hover_text(tr!("The share of the logistics already holding the primary locked when it is first hurt: they skip the lock"));
            changed |= ui.add(egui::Slider::new(&mut f.logi_prelock, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
            ui.end_row();
            ui.label(tr!("Logi reaction")).on_hover_text(tr!("Seconds until the logistics see a ship hurt, at the median; some quicker, a few slower"));
            changed |= ui.add(egui::Slider::new(&mut f.logi_reaction_s, 0.0..=10.0).suffix(" s").step_by(0.1)).changed();
            ui.end_row();
            ui.label(tr!("Logi arrival spread")).on_hover_text(tr!("Seconds over which the logistics' first repairs arrive, one after another, on top of their reactions"));
            changed |= ui.add(egui::Slider::new(&mut f.logi_spread_s, 0.0..=15.0).suffix(" s").step_by(0.5)).changed();
            ui.end_row();
            ui.label(tr!("Drones"));
            changed |= ui.checkbox(&mut f.drones, tr!("Launched")).on_hover_text(tr!("Drones and fighters add damage per second, never to a volley: they attack on their own time, no alpha before logi react")).changed();
            ui.end_row();
            ui.label(tr!("ADCs and suppressors")).on_hover_text(tr!("Assault damage controls and signature suppressors: switched on, once, when their ship is called primary"));
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
            let enemy_name = match session.enemy.as_deref() {
                Some(MIRROR) => tr!("Itself (mirror match)").to_owned(),
                Some(id) => id.strip_prefix(ENEMY_DOCTRINE).map(|g| trf!("{name} (enemy doctrine)", name = g)).or_else(|| fleets.iter().find(|x| x.id == id).map(|x| x.name.clone())).unwrap_or_else(|| tr!("none").to_owned()),
                None => tr!("none").to_owned(),
            };
            egui::ComboBox::from_id_salt("sim_fleet_enemy").selected_text(text::clip(&enemy_name, 28)).width(220.0).show_ui(ui, |ui| {
                if ui.menu_label(session.enemy.is_none(), tr!("none")).clicked() {
                    session.enemy = None;
                    f.enemy = None;
                    changed = true;
                }
                if ui.menu_label(session.enemy.as_deref() == Some(MIRROR), tr!("Itself (mirror match)")).clicked() {
                    session.enemy = Some(MIRROR.to_owned());
                    f.enemy = Some(MIRROR.to_owned());
                    changed = true;
                }
                for x in fleets.iter().filter(|x| x.id != my_id) {
                    if ui.menu_label(session.enemy.as_deref() == Some(x.id.as_str()), x.name.clone()).clicked() {
                        session.enemy = Some(x.id.clone());
                        f.enemy = Some(x.id.clone());
                        changed = true;
                    }
                }
                if !enemy_doctrines.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new(tr!("Enemy doctrines")).weak());
                    for g in &enemy_doctrines {
                        let id = format!("{ENEMY_DOCTRINE}{g}");
                        if ui.menu_label(session.enemy.as_deref() == Some(id.as_str()), g.clone()).clicked() {
                            session.enemy = Some(id.clone());
                            f.enemy = Some(id);
                            changed = true;
                        }
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
        if let Some(group) = enemy.strip_prefix(ENEMY_DOCTRINE) {
            let ids = sim.doctrine_groups_of(Some(crate::DoctrineSide::Enemy)).into_iter().find(|(g, _)| g == group).map(|(_, ids)| ids).unwrap_or_default();
            sim.want_fit_docs(ids.into_iter());
        } else if enemy != MIRROR && !sim.fleet_docs.contains_key(&enemy) {
            sim.request(HostRequest::LoadFleetDoc(enemy));
        }
        let wanted: Vec<String> = sim.fleet_docs.values().flat_map(|e| e.slots.iter().map(|s| s.fit_id.clone()).chain(e.boosters.iter().map(|(_, id)| id.clone()))).collect();
        sim.want_fit_docs(wanted.into_iter());
    }
}

/// The numbers: one direction of the fight at a time, picked at the top, then the shooting
/// fleet's own figures.
pub fn results(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some((mine, enemy)) = sim.fleet_stats() else {
        ui.label(egui::RichText::new(tr!("No fleet open.")).weak());
        return;
    };
    let names: HashMap<String, String> = sim.library.iter().map(|f| (f.id.clone(), f.name.clone())).collect();
    let analysis = sim.fleet_analysis();
    if let Some((theirs, _, _)) = &enemy {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            use spai_ui::widgets::SteadySelect as _;
            ui.add_space(crate::rows::GAP);
            let w = egui::vec2(((ui.available_width() - crate::rows::GAP * 2.0 - ui.spacing().item_spacing.x) / 2.0).max(60.0), 24.0);
            // One name a button, the shooter's: two would not fit the column.
            let chars = ((w.x / 7.5) as usize).saturating_sub(7).max(6);
            for (dir, (a, b)) in [(0, (&mine.name, &theirs.name)), (1, (&theirs.name, &mine.name))] {
                let label = trf!("{a} shoots", a = text::clip(a, chars));
                if ui.menu_label_sized(w, sim.fleet_view == dir, label).on_hover_text(trf!("{a} shooting {b}", a = a, b = b)).clicked() {
                    sim.fleet_view = dir;
                }
            }
        });
        ui.separator();
    }
    let dir = sim.fleet_view.min(1);
    // The ship the summary aims at in the target fleet, the main hull until picked.
    let mut aim = sim.fleet_aim[dir];
    egui::ScrollArea::vertical().id_salt("sim_fleet_results").auto_shrink([false, false]).show(ui, |ui| {
        // Room on the right for the scroll bar.
        ui.set_max_width(ui.available_width() - 12.0);
        ui.spacing_mut().item_spacing.y = 1.0;
        let Some((theirs, a, b)) = &enemy else {
            fleet_card(ui, &dogma, &mine, &names);
            ui.add_space(6.0);
            note(ui, tr!("Pick a fleet under Against to compare."));
            return;
        };
        let (shooter, target, against) = if dir == 0 { (&mine, theirs, a) } else { (theirs, &mine, b) };
        if let Some(an) = &analysis {
            engagement_card(ui, &dogma, an, dir, &names);
        }
        let main = target.slots.iter().position(|x| x.role == spai_fit::fleet::Role::Main).unwrap_or(0);
        let slot = aim.filter(|k| target.slots.get(*k).is_some_and(|x| x.count > 0)).unwrap_or(main);
        // The sweep's own model when the analysis is at hand; the break point from the fleets.
        let aimed = match &analysis {
            Some(an) => {
                let (fa, fb) = if dir == 0 { (&an.fleet, &an.enemy) } else { (&an.enemy, &an.fleet) };
                let plain = if slot == main { against.clone() } else { fleet::against_slot(&dogma, fa, shooter, fb, target, slot) };
                let apps = applications_cached(sim, an, dir, slot);
                let (sa, sb) = (spai_fit::fleet::sweep::Side { fleet: fa, stats: shooter, ammo: &an.ammo[dir] }, spai_fit::fleet::sweep::Side { fleet: fb, stats: target, ammo: &an.ammo[1 - dir] });
                let mut x = target.slots.get(slot).map_or_else(|| plain.clone(), |t| spai_fit::fleet::sweep::summary_from(&sa, &sb, t, f64::from(an.range), &apps));
                x.break_point = plain.break_point;
                x.margin = x.applied_dps * f64::from(fa.volley_efficiency.clamp(0.01, 1.0)) - x.break_point;
                x
            }
            None => against.clone(),
        };
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        subhead(ui, &trf!("{a} volleys on", a = shooter.name));
        // Hull first, then the fit and how many fly it.
        let slot_label = |k: usize| target.slots.get(k).map_or_else(String::new, |x| format!("{} · {} ({})", dogma.type_(x.ship).map_or("?", |t| t.name.as_str()), names.get(&x.fit_id).unwrap_or(&x.name), x.count));
        let mut open_details = false;
        ui.horizontal(|ui| {
            ui.add_space(crate::rows::GAP);
            let w = ui.available_width() - crate::rows::GAP - 30.0;
            egui::ComboBox::from_id_salt(("sim_fleet_aim", dir)).selected_text(text::clip(&slot_label(slot), ((w / 7.5) as usize).max(12))).width(w).show_ui(ui, |ui| {
                use spai_ui::widgets::SteadySelect as _;
                for (k, _) in target.slots.iter().enumerate().filter(|(_, x)| x.count > 0) {
                    if ui.menu_label(k == slot, slot_label(k)).clicked() {
                        aim = Some(k);
                    }
                }
            })
            .response
            .on_hover_text(tr!("The ship of the other fleet these figures aim at"));
            if spai_ui::widgets::icon_button(ui, icon::TABLE).on_hover_text(tr!("Every charge on this ship, worked out")).clicked() {
                open_details = true;
            }
        });
        if open_details {
            sim.fleet_volley_window = Some((dir, slot));
        }
        against_card(ui, &aimed, shooter, target, slot, analysis.as_ref().map_or(0.75, |an| if dir == 0 { an.enemy.logi_efficiency } else { an.fleet.logi_efficiency }));
        if let Some(an) = &analysis {
            let apps = applications_cached(sim, an, dir, slot);
            weapons_card(ui, &dogma, an, dir, slot, &apps, &names);
        }
        ui.add_space(4.0);
        ui.separator();
        fleet_card(ui, &dogma, shooter, &names);
    });
    sim.fleet_aim[dir] = aim;
}

fn line(ui: &mut egui::Ui, label: &str, value: String) {
    crate::stats_panel::line(ui, label, value);
}

/// A line whose breakdown shows on hover.
fn tline(ui: &mut egui::Ui, label: &str, value: String, tip: impl FnOnce(&mut egui::Ui)) {
    let r = ui.scope(|ui| line(ui, label, value)).response.interact(egui::Sense::hover());
    r.on_hover_ui(|ui| {
        ui.set_max_width(360.0);
        tip(ui);
    });
}

/// A breakdown: a heading, rows of a name and a right-aligned figure, and a note under them.
fn tip(ui: &mut egui::Ui, head: &str, rows: &[(String, String)], note_text: &str) {
    ui.label(egui::RichText::new(head).strong());
    if !rows.is_empty() {
        egui::Grid::new(ui.next_auto_id()).num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            for (k, v) in rows {
                ui.label(k);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(egui::RichText::new(v).monospace()));
                ui.end_row();
            }
        });
    }
    if !note_text.is_empty() {
        ui.add_space(2.0);
        ui.add(egui::Label::new(egui::RichText::new(note_text).weak()).wrap());
    }
}

/// Wrapped text across the column, indented like the lines.
fn note(ui: &mut egui::Ui, text: &str) {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(crate::rows::GAP as i8, 0)).show(ui, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
    });
}

/// A charge's name without its size and the words every charge of its kind shares, faction
/// prefixes shortened: "Caldari Navy Antimatter Charge L" is "CN Antimatter".
pub fn short_charge(name: &str) -> String {
    const PREFIXES: [(&str, &str); 14] = [
        ("Caldari Navy ", "CN "),
        ("Federation Navy ", "FN "),
        ("Imperial Navy ", "IN "),
        ("Republic Fleet ", "RF "),
        ("Khanid Navy ", "KN "),
        ("Ammatar Navy ", "AN "),
        ("Dread Guristas ", "DG "),
        ("True Sansha ", "TS "),
        ("Shadow Serpentis ", "SS "),
        ("Dark Blood ", "DB "),
        ("Domination ", "Dom "),
        ("Guristas ", "Gur "),
        ("Serpentis ", "Serp "),
        ("Arch Angel ", "AA "),
    ];
    let mut s = name.to_owned();
    for (long, short) in PREFIXES {
        if let Some(rest) = s.strip_prefix(long) {
            s = format!("{short}{rest}");
            break;
        }
    }
    let words: Vec<&str> = s.split(' ').collect();
    let size = |w: &str| matches!(w, "S" | "M" | "L" | "XL");
    let filler = |w: &str| matches!(w, "Charge" | "Missile" | "Torpedo" | "Rocket" | "Ammo");
    let kept: Vec<&str> = words.iter().enumerate().filter(|(i, w)| !(size(w) && *i == words.len() - 1) && !filler(w)).map(|(_, w)| *w).collect();
    if kept.is_empty() { name.to_owned() } else { kept.join(" ") }
}

/// A line with a charge on its right, its icon and short name; returns the row for a tooltip.
fn ammo_row(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, label: &str, charge: spai_fit::ids::TypeId) -> egui::Response {
    let name = dogma.type_(charge).map_or("?", |t| t.name.as_str());
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 19.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(crate::rows::GAP);
        ui.add(egui::Label::new(label).selectable(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(crate::rows::GAP);
            ui.add(egui::Label::new(egui::RichText::new(short_charge(name)).monospace()).truncate().selectable(false));
            ui.add(egui::Image::new(crate::icons::type_icon_uri(charge)).fit_to_exact_size(egui::vec2(16.0, 16.0)));
        });
    })
    .response
    .interact(egui::Sense::hover())
}

/// An ammo line whose tooltip names the charge in full, with a note under it if given.
fn ammo_line(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, label: &str, charge: spai_fit::ids::TypeId, why: Option<&str>) {
    let name = dogma.type_(charge).map_or_else(String::new, |t| t.name.clone());
    ammo_row(ui, dogma, label, charge).on_hover_ui(|ui| {
        ui.set_max_width(360.0);
        tip(ui, &name, &[], why.unwrap_or(""));
    });
}

/// A strong heading in a line's row.
fn subhead(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 19.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(crate::rows::GAP);
        ui.add(egui::Label::new(egui::RichText::new(text).strong()).truncate().selectable(false));
    });
}

fn title(ui: &mut egui::Ui, text: &str, weak: bool) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(crate::rows::GAP);
        let t = egui::RichText::new(text);
        ui.add(egui::Label::new(if weak { t.weak() } else { t.strong() }).truncate());
    });
}

fn stretches(v: &[(f64, f64)]) -> String {
    if v.is_empty() {
        return tr!("nowhere").to_owned();
    }
    v.iter().map(|(f, t)| format!("{} - {}", text::metres(*f), text::metres(*t))).collect::<Vec<_>>().join(", ")
}

/// One direction of the fight: where the shooter wants to be, what holds it, the race, how the
/// target's ships fare when primaried, and the shooter's ammo by range.
fn engagement_card(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, an: &Analysis, dir: usize, names: &HashMap<String, String>) {
    use spai_fit::fleet::race::{Event, Verdict};
    use spai_fit::fleet::sweep::{self, Side};
    let (shooter, target) = if dir == 0 { (&an.mine, &an.theirs) } else { (&an.theirs, &an.mine) };
    let (shooter_fleet, target_fleet) = if dir == 0 { (&an.fleet, &an.enemy) } else { (&an.enemy, &an.fleet) };
    let (sa, sb) = (Side { fleet: shooter_fleet, stats: shooter, ammo: &an.ammo[dir] }, Side { fleet: target_fleet, stats: target, ammo: &an.ammo[1 - dir] });
    let slot_name = |s: &spai_fit::fleet::SlotStats| names.get(&s.fit_id).cloned().unwrap_or_else(|| s.fit_id.clone());
    let primary = sweep::primary(shooter_fleet, target);
    let primary_name = primary.map_or_else(|| tr!("none").to_owned(), |p| slot_name(p));
    let sw = &an.sweep;
    let (best, favoured, own_cuts, their_cuts) = if dir == 0 { (sw.a_best, &sw.a_favoured, &sw.a_cutoffs, &sw.b_cutoffs) } else { (sw.b_best, &sw.b_favoured, &sw.b_cutoffs, &sw.a_cutoffs) };
    title(ui, tr!("Engagement"), false);
    tline(ui, tr!("Best range"), text::metres(best), |ui| tip(ui, tr!("Best range"), &[], &trf!("The farthest range where {a} still lands 90% of its most on {b}'s first target ({p}).", a = shooter.name, b = target.name, p = primary_name)));
    tline(ui, tr!("Favoured"), stretches(favoured), |ui| {
        tip(ui, tr!("Favoured"), &[(tr!("Webbed").to_owned(), "-15%".into()), (tr!("Scrambled or disrupted").to_owned(), "-20%".into()), (tr!("Neutralised, at full").to_owned(), "-30%".into())], tr!("Where its score leads the other's by 5% or more. The score is the enemy's first targets killed per minute, less these penalties when the other side's ewar reaches it."))
    });
    let at = sw.points.iter().min_by(|x, y| (x.distance - f64::from(an.range)).abs().total_cmp(&(y.distance - f64::from(an.range)).abs()));
    if let Some(p) = at {
        let f = if dir == 0 { &p.a } else { &p.b };
        title(ui, &trf!("At {r}", r = text::metres(p.distance)), true);
        tline(ui, &trf!("On {p}", p = text::clip(&primary_name, 24)), trf!("{d} DPS", d = text::num(f.applied_dps)), |ui| {
            let rows: Vec<(String, String)> = primary
                .map(|t| shooter.slots.iter().enumerate().filter(|(_, s)| s.count > 0).map(|(k, s)| (format!("{} x {}", s.count, slot_name(s)), text::num(sweep::weapons_on(&sa, &sb, k, t, p.distance).iter().map(|(w, f)| w.dps * f).sum::<f64>() * f64::from(s.count)))).collect())
                .unwrap_or_default();
            tip(ui, tr!("Damage landing on the first target"), &rows, tr!("Each weapon with its best carried charge, under the other side's disruptors, averaged over every shooter and target pair of the formations as they move."));
        });
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
        if !holds.is_empty() {
            line(ui, tr!("Held by"), holds.join(", "));
        }
    }
    for (head, cuts) in [(trf!("{a} ewar reach", a = shooter.name), own_cuts), (trf!("{a} ewar reach", a = target.name), their_cuts)] {
        if cuts.is_empty() {
            continue;
        }
        title(ui, &head, true);
        for (kind, optimal, edge) in cuts {
            let Some(label) = crate::stats_panel::ewar_name(*kind) else { continue };
            let value = if edge > optimal { trf!("{o}, half at {h}", o = text::metres(*optimal), h = text::metres(*edge)) } else { text::metres(*optimal) };
            tline(ui, label, value, |ui| {
                let mut rows = vec![(tr!("Full strength to").to_owned(), text::metres(*optimal))];
                if edge > optimal {
                    rows.push((tr!("Half strength at").to_owned(), text::metres(*edge)));
                    rows.push((tr!("A sixteenth at").to_owned(), text::metres(*optimal + 2.0 * (edge - optimal))));
                    rows.push((tr!("Nothing past").to_owned(), text::metres(*optimal + 3.0 * (edge - optimal))));
                }
                tip(ui, label, &rows, tr!("The strongest module of the kind: its strength halves at one falloff past optimal, falls to a sixteenth at two and to nothing past three; tackle holds within optimal only. On a ship it stacks with the ship's own penalised modifiers and is cut by its resistances."));
            });
        }
    }
    let neut = their_cuts.iter().filter(|(k, _, _)| matches!(k, spai_fit::stats::RemoteKind::Neut)).map(|(_, o, _)| *o).fold(0.0, f64::max);
    if neut > 0.0 && f64::from(an.range) <= neut * 1.2 {
        warn(ui, &trf!("{v} sits in or near neutraliser range ({r})", v = shooter.name, r = text::metres(neut)));
    }
    let r = &an.race;
    title(ui, tr!("Race at this range"), true);
    let name_of = |side: usize| if side == 0 { &an.mine.name } else { &an.theirs.name };
    let outcome = match r.winner {
        Some(w) => trf!("{a} wins in {s}", a = name_of(w), s = text::secs(r.seconds)),
        None if r.timeline.last().is_some_and(|x| x.1 == 0 && x.2 == 0) => trf!("both destroyed after {s}", s = text::secs(r.seconds)),
        None => trf!("both stand after {s}", s = text::secs(r.seconds)),
    };
    tline(ui, tr!("Outcome"), outcome, |ui| {
        let adcs = r.events.iter().filter(|(_, e)| matches!(e, Event::Adc { .. })).count();
        let sups = r.events.iter().filter(|(_, e)| matches!(e, Event::Suppressor { .. })).count();
        tip(ui, tr!("The race"), &[(tr!("Assault damage controls on").to_owned(), adcs.to_string()), (tr!("Signature suppressors on").to_owned(), sups.to_string()), (tr!("Fight lasted").to_owned(), text::secs(r.seconds))], tr!("Each side calls one primary at a time by its target order. Ships fire after a reaction delay, on their own cycle, and reload after each load; missiles land after their flight; a volley's damage past what kills is lost. Logistics start only once the primary is hurt, each after its reaction, its lock unless pre-locked, its place in the arrival spread and its first cycle, at the fleet's logistics efficiency, lessened as repairs pile up, and heal no more than the damage there; assault damage controls go on when their ship is called."));
    });
    // Per side, shooter first: ships left, its first loss, and when its reps catch up (the other
    // side loses its critical mass).
    let (start, end) = (r.timeline.first(), r.timeline.last());
    for side in [dir, 1 - dir] {
        let count = |x: Option<&(f64, u32, u32, f64, f64)>| x.map_or(0, |x| if side == 0 { x.1 } else { x.2 });
        let name = text::clip(name_of(side), 20);
        tline(ui, &trf!("{a} ships left", a = name), trf!("{n} of {m}", n = count(end), m = count(start)), |ui| {
            let rows: Vec<(String, String)> = r.timeline.iter().step_by((r.timeline.len() / 6).max(1)).map(|x| (text::secs(x.0), if side == 0 { x.1 } else { x.2 }.to_string())).collect();
            tip(ui, tr!("Ships left over the fight"), &rows, "");
        });
        let first_loss = r.events.iter().find(|(_, e)| matches!(e, Event::Kill { side: s, .. } if *s == side)).map(|(t, _)| *t);
        line(ui, &trf!("{a} first loss", a = name), first_loss.map_or_else(|| tr!("none").to_owned(), text::secs));
        let other = 1 - side;
        tline(ui, &trf!("{a} reps catch up", a = name), match r.critical_mass_lost[other] {
            Some(t) => {
                let left = r.timeline.iter().find(|x| x.0 >= t).map_or(0, |x| if other == 0 { x.1 } else { x.2 });
                trf!("at {s}, {n} enemies left", s = text::secs(t), n = left)
            }
            None => tr!("never").to_owned(),
        }, |ui| tip(ui, tr!("Reps catch up"), &[], tr!("From here the other side's damage on its primary, through the resistances of the layer being worn, no longer beats the repairs on it: it has lost its critical mass.")));
    }
    let target_side = 1 - dir;
    if !r.verdicts[target_side].is_empty() {
        title(ui, &trf!("{a} when primaried", a = target.name), true);
        for (k, v) in &r.verdicts[target_side] {
            let Some(s) = target.slots.get(*k) else { continue };
            let text = match v {
                Verdict::OneVolley => tr!("dies in one volley").to_owned(),
                Verdict::BeforeReps { seconds } => trf!("dies in {s}, before reps land", s = text::secs(*seconds)),
                Verdict::Outpaced { seconds, by } if *by > 0.0 && has_logi(target) => trf!("dies in {s}, {d}/s over reps", s = text::secs(*seconds), d = text::num(*by)),
                Verdict::Outpaced { seconds, .. } => trf!("dies in {s}", s = text::secs(*seconds)),
                Verdict::Held => tr!("holds").to_owned(),
                Verdict::NotCalled => tr!("never called").to_owned(),
            };
            tline(ui, &slot_name(s), text, |ui| tip(ui, &slot_name(s), &[(tr!("Effective HP").to_owned(), text::num(s.computed.tank.ehp))], tr!("How its first ship to be called primary fared: dead in one volley from full health, dead before any repair landed on it, dead with the damage outpacing the repairs, still standing at the end, or never called because the fight stalled or ended first.")));
        }
    }
    // The shooter's ammo by range, against its first target as the fight leaves it.
    let mut shown = false;
    for (k, weapon, bands) in &an.ammo_bands[dir] {
        if bands.len() < 2 && bands.iter().all(|b| b.better.is_none()) {
            continue;
        }
        let Some(slot) = shooter.slots.get(*k) else { continue };
        if !shown {
            title(ui, tr!("Ammo by range"), true);
            shown = true;
        }
        let gun = dogma.type_(*weapon).map_or("?", |t| t.name.as_str());
        ui.add_space(3.0);
        subhead(ui, &format!("{} · {}", slot_name(slot), gun));
        for band in bands {
            ammo_line(ui, dogma, &format!("{} - {}", text::metres(band.from), text::metres(band.to)), band.charge, Some(tr!("The carried charge landing most over this stretch on the first target, its speed after this fleet's webs, its signature after the painters, the weapons under the other side's disruptors.")));
            if let Some((id, share)) = band.better {
                ammo_line(ui, dogma, &trf!("not carried, +{p}%", p = format!("{:.0}", share * 100.0)), id, Some(tr!("A charge not carried that would land this much more over the stretch, on average.")));
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
        ui.set_max_width(ui.available_width() - crate::rows::GAP);
        ui.add(egui::Label::new(egui::RichText::new(format!("{} {text}", egui_phosphor::regular::WARNING)).color(ui.visuals().warn_fg_color)).wrap());
    });
}

fn weapon_label(dogma: &spai_fit::Dogma, w: &spai_fit::stats::WeaponGroup) -> String {
    let name = dogma.type_(w.type_id).map_or("?", |t| t.name.as_str());
    match w.charge.and_then(|c| dogma.type_(c)) {
        Some(c) => format!("{} x {} ({})", w.count, text::clip(name, 24), text::clip(&c.name, 16)),
        None => format!("{} x {}", w.count, text::clip(name, 30)),
    }
}

fn fleet_card(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, s: &FleetStats, names: &HashMap<String, String>) {
    let name = |slot: &spai_fit::fleet::SlotStats| names.get(&slot.fit_id).cloned().unwrap_or_else(|| slot.name.clone());
    let per_slot = |f: &dyn Fn(&spai_fit::fleet::SlotStats) -> String| s.slots.iter().filter(|x| x.count > 0).map(|x| (format!("{} x {}", x.count, name(x)), f(x))).collect::<Vec<_>>();
    title(ui, &s.name, false);
    tline(ui, tr!("Ships"), s.ships.to_string(), |ui| tip(ui, tr!("Ships"), &per_slot(&|x| x.count.to_string()), ""));
    tline(ui, tr!("DPS / volley"), trf!("{dps} / {volley}", dps = text::num(s.dps), volley = text::num(s.volley)), |ui| {
        let rows = per_slot(&|x| {
            let n = f64::from(x.count);
            let volley: f64 = x.computed.weapons.iter().filter(|w| w.alpha()).map(|w| w.volley.total()).sum();
            format!("{} / {}", text::num(x.computed.dps() * n), text::num(volley * n))
        });
        tip(ui, tr!("Damage per second / volley"), &rows, tr!("Listed figures, reloads counted. Drones and fighters add to damage per second, never to a volley."));
    });
    tline(ui, tr!("Logistics"), trf!("{v}/s", v = text::num(s.logi_rep)), |ui| {
        let rows: Vec<(String, String)> = per_slot(&|x| {
            let r = &x.computed.remote;
            trf!("{v}/s", v = text::num((r.shield_rep + r.armor_rep + r.hull_rep) * f64::from(x.count)))
        })
        .into_iter()
        .filter(|(_, v)| !v.starts_with("0"))
        .collect();
        let mut rows = rows;
        let landing: f64 = fleet::reps_on_one(s, &|_, x| x.count).iter().sum();
        if landing > 0.0 {
            rows.push((tr!("All on one ship, landing").to_owned(), trf!("{v}/s", v = text::num(landing))));
        }
        tip(ui, tr!("Remote repairs"), &rows, tr!("Shield, armor and hull repairs per second the fleet's logistics give, all together. Piled on one ship they land less, as the game lessens every repair by all the repairs a ship receives of that layer."));
    });
    if s.neut > 0.0 {
        line(ui, tr!("Neuts"), trf!("{v} GJ/s", v = text::num(s.neut)));
    }
    tline(ui, tr!("Speed"), trf!("{min} to {max} m/s", min = text::num(s.speed_min), max = text::num(s.speed_max)), |ui| {
        tip(ui, tr!("Top speed"), &per_slot(&|x| trf!("{v} m/s", v = text::num(x.computed.mobility.max_velocity))), tr!("The fleet moves at its share of the slowest ship's top speed."));
    });
    let p = s.projected;
    if p.webs + p.painters + p.tracking_disruptors > 0 {
        line(ui, tr!("Projects"), trf!("{w} webs, {p} painters, {t} TDs", w = p.webs, p = p.painters, t = p.tracking_disruptors));
    }
    for (i, slot) in s.slots.iter().enumerate() {
        let c = &slot.computed;
        title(ui, &format!("{} x {}{}", slot.count, name(slot), if i == s.main { " *" } else { "" }), true);
        tline(ui, tr!("DPS / volley each"), trf!("{dps} / {volley}", dps = text::num(c.dps()), volley = text::num(c.volley())), |ui| {
            let rows: Vec<(String, String)> = c.weapons.iter().map(|w| (weapon_label(dogma, w), format!("{} / {}", text::num(w.dps), text::num(w.volley.total())))).collect();
            tip(ui, tr!("By weapon"), &rows, "");
        });
        tline(ui, tr!("Effective HP"), text::num(c.tank.ehp), |ui| {
            let t = &c.tank;
            let res = |l: &spai_fit::stats::Layer| format!("{} HP, {:.0}%", text::num(l.hp), 100.0 * (1.0 - l.resonance.iter().sum::<f64>() / 4.0));
            tip(ui, tr!("Hit points and average resistance"), &[(tr!("Shield").to_owned(), res(&t.shield)), (tr!("Armor").to_owned(), res(&t.armor)), (tr!("Hull").to_owned(), res(&t.hull))], tr!("Effective hit points against the incoming damage profile of the fit."));
        });
        if let Some(w) = c.weapons.first() {
            tline(ui, tr!("Range"), match &w.missile {
                Some(m) => text::metres(m.range),
                None => trf!("{o} + {f}", o = text::metres(w.optimal), f = text::metres(w.falloff)),
            }, |ui| {
                let rows: Vec<(String, String)> = c.weapons.iter().map(|w| (weapon_label(dogma, w), match &w.missile {
                    Some(m) => text::metres(m.range),
                    None => format!("{} + {}", text::metres(w.optimal), text::metres(w.falloff)),
                })).collect();
                tip(ui, tr!("By weapon"), &rows, tr!("Optimal plus falloff for turrets, flight range for missiles."));
            });
        }
        line(ui, tr!("Speed"), trf!("{v} m/s", v = text::num(c.mobility.max_velocity)));
    }
}

/// Each weapon group of the shooter on the picked ship at the set range: the charge it fires,
/// the ship's effective hit points against that charge, and how much of the raw damage lands.
/// `applications`, kept per analysis, direction and target slot.
fn applications_cached(sim: &SimUi, an: &Arc<Analysis>, dir: usize, slot: usize) -> Arc<Vec<spai_fit::fleet::sweep::Application>> {
    let hit = sim.fleet_cache.borrow().apps.as_ref().filter(|(a, d, k, _)| Arc::ptr_eq(a, an) && *d == dir && *k == slot).map(|x| x.3.clone());
    hit.unwrap_or_else(|| {
        let v = Arc::new(applications(an, dir, slot));
        sim.fleet_cache.borrow_mut().apps = Some((an.clone(), dir, slot, v.clone()));
        v
    })
}

fn applications(an: &Analysis, dir: usize, slot: usize) -> Vec<spai_fit::fleet::sweep::Application> {
    use spai_fit::fleet::sweep::{self, Side};
    let (shooter, target) = if dir == 0 { (&an.mine, &an.theirs) } else { (&an.theirs, &an.mine) };
    let (sf, tf) = if dir == 0 { (&an.fleet, &an.enemy) } else { (&an.enemy, &an.fleet) };
    let Some(t) = target.slots.get(slot) else { return Vec::new() };
    let (a, b) = (Side { fleet: sf, stats: shooter, ammo: &an.ammo[dir] }, Side { fleet: tf, stats: target, ammo: &an.ammo[1 - dir] });
    sweep::application(&a, &b, t, f64::from(an.range))
}

fn weapons_card(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, an: &Analysis, dir: usize, slot: usize, apps: &[spai_fit::fleet::sweep::Application], names: &HashMap<String, String>) {
    let (shooter, target) = if dir == 0 { (&an.mine, &an.theirs) } else { (&an.theirs, &an.mine) };
    let Some(t) = target.slots.get(slot) else { return };
    if apps.is_empty() {
        return;
    }
    title(ui, &trf!("Weapons on {t} at {r}", t = text::clip(names.get(&t.fit_id).unwrap_or(&t.name), 22), r = text::metres(f64::from(an.range))), true);
    for x in apps {
        let Some(s) = shooter.slots.get(x.slot) else { continue };
        let gun = dogma.type_(x.weapon.type_id).map_or("?", |t| t.name.as_str());
        ui.add_space(3.0);
        subhead(ui, &format!("{} x {} · {}", s.count, names.get(&s.fit_id).unwrap_or(&s.name), gun));
        let v = &x.weapon.volley;
        match x.weapon.charge {
            Some(c) => {
                let name = dogma.type_(c).map_or_else(String::new, |t| t.name.clone());
                let r = ammo_row(ui, dogma, tr!("Ammo"), c);
                r.on_hover_ui(|ui| {
                    ui.set_max_width(360.0);
                    tip(ui, &name, &[(tr!("EM").to_owned(), text::num(v.em)), (tr!("Thermal").to_owned(), text::num(v.thermal)), (tr!("Kinetic").to_owned(), text::num(v.kinetic)), (tr!("Explosive").to_owned(), text::num(v.explosive))], tr!("Its best carried charge at this range against this ship, with the group's volley by damage type."));
                });
            }
            None => line(ui, tr!("Ammo"), tr!("none").to_owned()),
        }
        tline(ui, tr!("Target EHP against it"), text::num(x.target_ehp), |ui| {
            tip(ui, tr!("Effective hit points against this charge"), &[(tr!("Against the fit's damage profile").to_owned(), text::num(t.computed.tank.ehp))], tr!("Each layer's hit points over the share of this charge's damage its resistances let through."));
        });
        let pct = |f: f64| format!("{:.0}%", f * 100.0);
        tline(ui, tr!("Raw damage applied"), trf!("{m} ({l} to {h})", m = pct(x.mean), l = pct(x.low), h = pct(x.high)), |ui| {
            tip(ui, tr!("Share of the raw damage that lands"), &[(tr!("Average").to_owned(), pct(x.mean)), (tr!("Bottom 5% of pairs").to_owned(), pct(x.low)), (tr!("Top 5% of pairs").to_owned(), pct(x.high))], tr!("Over every shooter and target pair of the two formations as they move: tracking or explosion against the target's speed and signature, range against optimal and falloff, the ewar on both."));
        });
    }
}

fn against_card(ui: &mut egui::Ui, a: &Against, shooter: &FleetStats, target: &FleetStats, slot: usize, logi_efficiency: f32) {
    let main = target.slots.get(slot);
    tline(ui, tr!("Applied DPS / volley"), trf!("{dps} / {volley}", dps = text::num(a.applied_dps), volley = text::num(a.applied_volley)), |ui| {
        tip(ui, tr!("Applied damage"), &[(tr!("Listed").to_owned(), format!("{} / {}", text::num(shooter.dps), text::num(shooter.volley))), (tr!("Landing").to_owned(), format!("{} / {}", text::num(a.applied_dps), text::num(a.applied_volley)))], tr!("On the picked ship at the set range, crossing at its speed after this fleet's webs, its signature after the painters, through the other side's disruptors."));
    });
    tline(ui, tr!("Target speed / signature"), trf!("{v} m/s, {s} m", v = text::num(a.target_velocity), s = text::num(a.target_signature)), |ui| {
        let rows = main.map(|m| vec![(tr!("Speed").to_owned(), trf!("{a} to {b} m/s", a = text::num(m.computed.mobility.max_velocity), b = text::num(a.target_velocity))), (tr!("Signature").to_owned(), trf!("{a} to {b} m", a = text::num(m.computed.mobility.signature), b = text::num(a.target_signature)))]).unwrap_or_default();
        tip(ui, tr!("Under this fleet's ewar"), &rows, "");
    });
    let ehp = main.map_or(0.0, |m| m.computed.tank.ehp);
    tline(ui, tr!("Critical mass"), a.critical_mass.map_or_else(|| tr!("n/a").to_owned(), |n| trf!("{n} ships", n = n)), |ui| {
        tip(ui, tr!("Ships to volley one off the field"), &[(tr!("Target effective HP").to_owned(), text::num(ehp))], "");
    });
    tline(ui, tr!("Kills per volley"), text::num(a.kills_per_volley), |ui| tip(ui, tr!("Kills per volley"), &[], tr!("The whole fleet's applied volley times the volley efficiency, over the picked ship's effective hit points.")));
    tline(ui, tr!("Break point (their reps)"), trf!("{v}/s", v = text::num(a.break_point)), |ui| {
        // From what all their logi give to what lands on the one ship.
        let landing: f64 = fleet::reps_on_one(target, &|_, x| x.count).iter().sum();
        let eff = f64::from(logi_efficiency.clamp(0.0, 1.0));
        let intake = main.map_or(1.0, |m| m.computed.rep_intake);
        let own = main.map_or(0.0, |m| m.computed.tank.reps.armor + m.computed.tank.reps.shield + m.computed.tank.reps.hull);
        let per_s = |v: f64| trf!("{v}/s", v = text::num(v));
        let mut rows = vec![
            (tr!("All their logistics, each at full").to_owned(), per_s(target.logi_rep)),
            (tr!("Landing all on one ship").to_owned(), per_s(landing)),
            (trf!("At {p}% logistics efficiency", p = format!("{:.0}", eff * 100.0)), per_s(landing * eff)),
        ];
        if intake < 1.0 {
            rows.push((tr!("Through its repair impedance").to_owned(), per_s(landing * eff * intake)));
        }
        rows.push((tr!("Its own repairs").to_owned(), per_s(own)));
        rows.push((tr!("Break point").to_owned(), per_s(a.break_point)));
        tip(ui, tr!("Break point"), &rows, tr!("The repairs the enemy can put on one ship. Piled on one ship, repairs land less, as the game lessens each by all the repairs it receives; then the share that does any good. Below it, nothing dies."));
    });
    let color = if a.margin >= 0.0 { ui.visuals().hyperlink_color } else { ui.visuals().error_fg_color };
    let r = ui.scope(|ui| crate::stats_panel::line_colored(ui, tr!("Margin"), trf!("{v}/s", v = text::num(a.margin)), Some(color))).response.interact(egui::Sense::hover());
    r.on_hover_ui(|ui| {
        ui.set_max_width(360.0);
        tip(ui, tr!("Margin"), &[], tr!("Applied damage per second times the volley efficiency, less the break point; negative when they out-repair this fleet."));
    });
}

/// The picked ship of the other fleet under the shooter's every charge, in a window: per weapon
/// group each charge it could fire, with what lands, the ship's hit points against it and how
/// many ships one volley kill takes.
pub fn volley_window(sim: &mut SimUi, ctx: &egui::Context) {
    let Some((dir, slot)) = sim.fleet_volley_window else { return };
    let (Some(dogma), Some(an)) = (sim.dogma.clone(), sim.fleet_analysis()) else { return };
    // The fight as the what-if overrides change it, for this window only.
    let wi = sim.fleet_what_if.clone();
    let (sf0, tf0) = if dir == 0 { (&an.fleet, &an.enemy) } else { (&an.enemy, &an.fleet) };
    let mut sf = sf0.clone();
    let mut tf = tf0.clone();
    if let Some(e) = wi.volley_efficiency {
        sf.volley_efficiency = e;
    }
    if let Some(m) = wi.shooter_motion {
        sf.motion = m;
    }
    if let Some(m) = wi.target_motion {
        tf.motion = m;
    }
    for (f, off) in [(&mut sf, &wi.shooter_off), (&mut tf, &wi.target_off)] {
        f.boosters.retain(|(k, _)| !off.contains(k));
        f.boost_shortcuts.retain(|b| !off.contains(&b.kind));
    }
    let distance = f64::from(wi.distance.unwrap_or(an.range));
    let boosts_changed = !wi.shooter_off.is_empty() || !wi.target_off.is_empty();
    let (shooter, target) = if boosts_changed {
        (sim.cached_stats(&sf), sim.cached_stats(&tf))
    } else if dir == 0 {
        (an.mine.clone(), an.theirs.clone())
    } else {
        (an.theirs.clone(), an.mine.clone())
    };
    let Some(t) = target.slots.get(slot) else {
        sim.fleet_volley_window = None;
        return;
    };
    let names: HashMap<String, String> = sim.library.iter().map(|f| (f.id.clone(), f.name.clone())).collect();
    let name_of = |x: &spai_fit::fleet::SlotStats| names.get(&x.fit_id).cloned().unwrap_or_else(|| x.name.clone());
    // Worked out once per analysis, direction, ship and overrides.
    let key = serde_json::to_string(&wi).unwrap_or_default();
    let hit = sim.fleet_cache.borrow().what_if.as_ref().filter(|w| Arc::ptr_eq(&w.0, &an) && w.1 == dir && w.2 == slot && w.3 == key).map(|w| (w.4.clone(), w.5.clone()));
    let (options, best) = hit.unwrap_or_else(|| {
        use spai_fit::fleet::sweep::{self, Side};
        let ammo = if boosts_changed { [sim.cached_ammo(&shooter), sim.cached_ammo(&target)] } else { [an.ammo[dir].clone(), an.ammo[1 - dir].clone()] };
        let (a, b) = (Side { fleet: &sf, stats: &shooter, ammo: &ammo[0] }, Side { fleet: &tf, stats: &target, ammo: &ammo[1] });
        let options = Arc::new(sweep::charge_options(&a, &b, t, distance));
        let best = Arc::new(sweep::application(&a, &b, t, distance));
        sim.fleet_cache.borrow_mut().what_if = Some((an.clone(), dir, slot, key.clone(), options.clone(), best.clone()));
        (options, best)
    });
    let eff = f64::from(sf.volley_efficiency.clamp(0.01, 1.0));
    let hull = dogma.type_(t.ship).map_or("?", |x| x.name.as_str()).to_owned();
    // The burst kinds each side flies, to switch off one by one.
    let kinds = |f: &Fleet| {
        let mut v: Vec<BurstKind> = f.boosters.iter().map(|(k, _)| *k).chain(f.boost_shortcuts.iter().filter(|b| b.active).map(|b| b.kind)).collect();
        v.sort_by_key(|k| BurstKind::ALL.iter().position(|x| x == k));
        v.dedup();
        v
    };
    let (shooter_kinds, target_kinds) = (kinds(sf0), kinds(tf0));
    let mut wi_next = wi.clone();
    let mut open = true;
    let mut carried_only = sim.fleet_volley_carried;
    let mut sort = sim.fleet_volley_sort;
    egui::Window::new(trf!("{a} volleys on {b}", a = shooter.name, b = format!("{hull} · {}", name_of(t))))
        .id(egui::Id::new("sim_fleet_volley_window"))
        .open(&mut open)
        .resizable(true)
        .default_size([960.0, 540.0])
        .default_pos(ctx.content_rect().center() - egui::vec2(480.0, 270.0))
        .show(ctx, |ui| {
            // Room either side, and for the scroll bar on the right.
            egui::Frame::NONE.inner_margin(egui::Margin { left: 8, right: 18, top: 2, bottom: 2 }).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(trf!("At {r}; {e} effective HP against the fit's profile; volley efficiency {v}%", r = text::metres(distance), e = text::num(t.computed.tank.ehp), v = format!("{:.0}", eff * 100.0)));
                    ui.checkbox(&mut carried_only, tr!("Carried charges only"));
                });
                what_if_panel(ui, &mut wi_next, &an, sf0, tf0, &shooter_kinds, &target_kinds, &shooter.name, &target.name);
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(ui.available_width() - 12.0);
                    let mut groups: Vec<(usize, spai_fit::ids::TypeId)> = options.iter().map(|o| (o.slot, o.weapon)).collect();
                    groups.dedup();
                    const HEADS: usize = 6;
                    for (k, weapon) in groups {
                        let Some(s) = shooter.slots.get(k) else { continue };
                        let n = f64::from(s.count);
                        // Per row its figures: (applied DPS, applied volley, EHP against it, applied, low, high, ships per kill).
                        let mut rows: Vec<(&spai_fit::fleet::sweep::ChargeOption, [f64; 7])> = options
                            .iter()
                            .filter(|o| o.slot == k && o.weapon == weapon && (!carried_only || o.carried))
                            .map(|o| {
                                // The whole ship's volley with this charge in this group, the others at their best.
                                let ship = spai_fit::fleet::sweep::ship_volley(&best, k, Some((weapon, &o.group, o.mean)));
                                let kill = spai_fit::fleet::sweep::ships_per_kill(t, &ship, eff).map_or(f64::INFINITY, f64::from);
                                (o, [o.group.dps * o.mean * n * eff, o.group.volley.total() * o.mean * n * eff, o.target_ehp, o.mean, o.low, o.high, kill])
                            })
                            .collect();
                        if rows.is_empty() {
                            continue;
                        }
                        let (col, desc) = sort;
                        // Column 7 is the charge's name.
                        let charge_name = |o: &spai_fit::fleet::sweep::ChargeOption| dogma.type_(o.charge).map_or(String::new(), |x| x.name.clone());
                        rows.sort_by(|x, y| {
                            let o = if col == 7 { charge_name(x.0).cmp(&charge_name(y.0)) } else { x.1[col].total_cmp(&y.1[col]) };
                            if desc { o.reverse() } else { o }
                        });
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(format!("{} x {} · {}", s.count, name_of(s), dogma.type_(weapon).map_or("?", |x| x.name.as_str()))).strong());
                        // The figures in columns of their own width; the charge takes what is left.
                        const COLS: [f32; HEADS + 1] = [100.0, 110.0, 150.0, 70.0, 100.0, 0.0, 140.0];
                        let spacing = 12.0;
                        let fixed: f32 = COLS.iter().sum::<f32>() + spacing * 7.0;
                        let charge_w = (ui.available_width() - fixed).max(150.0);
                        egui::Grid::new(("sim_fleet_volley_grid", k, weapon)).striped(true).num_columns(7).spacing([spacing, 3.0]).min_col_width(0.0).show(ui, |ui| {
                            let mark = |c: usize| if col == c { if desc { format!(" {}", icon::CARET_DOWN) } else { format!(" {}", icon::CARET_UP) } } else { String::new() };
                            let r = ui.allocate_ui_with_layout(egui::vec2(charge_w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_min_width(charge_w);
                                ui.add(egui::Button::new(egui::RichText::new(format!("{}{}", tr!("Charge"), mark(7))).weak()).frame(false)).on_hover_text(tr!("Sort by this"))
                            }).inner;
                            if r.clicked() {
                                sort = if col == 7 { (7, !desc) } else { (7, false) };
                            }
                            // Headings sort; the arrow marks the column and its direction.
                            for (c, (h, w)) in [(0usize, tr!("Applied DPS")), (1, tr!("Applied volley")), (2, tr!("Target EHP against it")), (3, tr!("Applied")), (4, tr!("5% to 95%")), (6, tr!("Ships per volley kill"))].into_iter().map(|(c, h)| (c, (h, COLS[c]))) {
                                let mark = if col == c { if desc { format!(" {}", icon::CARET_DOWN) } else { format!(" {}", icon::CARET_UP) } } else { String::new() };
                                let r = ui.allocate_ui_with_layout(egui::vec2(w, 18.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.set_min_width(w);
                                    ui.add(egui::Button::new(egui::RichText::new(format!("{h}{mark}")).weak()).frame(false)).on_hover_text(tr!("Sort by this"))
                                }).inner;
                                if r.clicked() {
                                    sort = if col == c { (c, !desc) } else { (c, c != 2 && c != 6) };
                                }
                            }
                            ui.end_row();
                            for (o, v) in rows {
                                let name = dogma.type_(o.charge).map_or("?", |x| x.name.as_str());
                                ui.allocate_ui_with_layout(egui::vec2(charge_w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.set_min_width(charge_w);
                                    ui.add(egui::Image::new(crate::icons::type_icon_uri(o.charge)).fit_to_exact_size(egui::vec2(16.0, 16.0)));
                                    let t = egui::RichText::new(name);
                                    ui.add(egui::Label::new(if o.carried { t } else { t.weak() }).truncate()).on_hover_text(if o.carried { name.to_owned() } else { trf!("{n}, not carried", n = name) });
                                });
                                let cells = [
                                    (text::num(v[0]), COLS[0]),
                                    (text::num(v[1]), COLS[1]),
                                    (text::num(v[2]), COLS[2]),
                                    (format!("{:.0}%", v[3] * 100.0), COLS[3]),
                                    (format!("{:.0}% - {:.0}%", v[4] * 100.0, v[5] * 100.0), COLS[4]),
                                    (if v[6].is_finite() { format!("{:.0}", v[6]) } else { tr!("n/a").to_owned() }, COLS[6]),
                                ];
                                for (c, w) in cells {
                                    crate::rows::num_cell(ui, w, c, !o.carried);
                                }
                                ui.end_row();
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(egui::RichText::new(tr!("Applied figures are the whole slot's at the set range, averaged over every shooter and target pair of both formations, the ship slowed by this fleet's webs and grown by its painters, the weapons under the other side's disruptors, times the volley efficiency. Ships per volley kill: the ship's hit points against the damage mix of one ship's whole volley, this charge in this group and its other weapons at their best carried charges, over that volley. Greyed: not carried.")).weak()).wrap());
                });
            });
        });
    sim.fleet_volley_carried = carried_only;
    sim.fleet_volley_sort = sort;
    sim.fleet_what_if = wi_next;
    if !open {
        sim.fleet_volley_window = None;
    }
}

/// Overrides for the volley window only, none of them saved: each `None` or empty leaves the
/// fight as set.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct WhatIf {
    pub distance: Option<f32>,
    pub volley_efficiency: Option<f32>,
    pub shooter_motion: Option<(f32, f32)>,
    pub target_motion: Option<(f32, f32)>,
    /// Burst kinds switched off, on the shooting and the target side.
    pub shooter_off: Vec<BurstKind>,
    pub target_off: Vec<BurstKind>,
}

fn what_if_panel(ui: &mut egui::Ui, wi: &mut WhatIf, an: &Analysis, sf: &Fleet, tf: &Fleet, shooter_kinds: &[BurstKind], target_kinds: &[BurstKind], shooter: &str, target: &str) {
    let changed = *wi != WhatIf::default();
    let head = if changed { trf!("{t} (changed)", t = tr!("What if")) } else { tr!("What if").to_owned() };
    egui::CollapsingHeader::new(head).id_salt("sim_fleet_what_if").default_open(changed).show(ui, |ui| {
        egui::Grid::new("sim_fleet_what_if_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(tr!("Distance"));
            let mut km = wi.distance.unwrap_or(an.range) / 1000.0;
            if ui.add(egui::Slider::new(&mut km, 0.0..=300.0).suffix(" km").step_by(0.5)).changed() {
                wi.distance = Some(km * 1000.0);
            }
            ui.end_row();
            ui.label(tr!("Volley efficiency"));
            let mut e = wi.volley_efficiency.unwrap_or(sf.volley_efficiency);
            if ui.add(egui::Slider::new(&mut e, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed() {
                wi.volley_efficiency = Some(e);
            }
            ui.end_row();
        });
        // Out of the grid: a grid row does not grow to two dials side by side.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(tr!("Movement")).on_hover_text(tr!("Each fleet's share of its slowest ship's top speed and its heading against the line to the other: the speeds and angular velocities the weapons see"));
            ui.add_space(12.0);
            for (label, m, base) in [(shooter, &mut wi.shooter_motion, sf.motion), (target, &mut wi.target_motion, tf.motion)] {
                let mut v = m.unwrap_or(base);
                crate::graphs_panel::motion_dial(ui, &text::clip(label, 14), &mut v);
                if v != m.unwrap_or(base) {
                    *m = Some(v);
                }
                ui.add_space(12.0);
            }
        });
        ui.add_space(4.0);
        egui::Grid::new("sim_fleet_what_if_boosts").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            for (label, kinds, off) in [(shooter, shooter_kinds, &mut wi.shooter_off), (target, target_kinds, &mut wi.target_off)] {
                if kinds.is_empty() {
                    continue;
                }
                ui.label(trf!("{a} boosts", a = text::clip(label, 20)));
                ui.horizontal_wrapped(|ui| {
                    for k in kinds {
                        let mut on = !off.contains(k);
                        if ui.checkbox(&mut on, kind_name(*k)).changed() {
                            if on {
                                off.retain(|x| x != k);
                            } else {
                                off.push(*k);
                            }
                        }
                    }
                });
                ui.end_row();
            }
        });
        if ui.add_enabled(changed, egui::Button::new(tr!("Back to the fleets' settings"))).clicked() {
            *wi = WhatIf::default();
        }
    });
}
