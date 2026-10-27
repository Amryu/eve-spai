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
        for d in self.supplied.iter().filter(|d| side.is_none_or(|s| d.side == s)) {
            if !groups.iter().any(|(g, _)| *g == d.doctrine.name) {
                groups.push((d.doctrine.name.clone(), d.doctrine.fits.iter().map(|f| f.fit_id.clone()).collect()));
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        groups
    }

    /// The opponent by its id: a saved fleet, or an enemy doctrine group (`ENEMY_DOCTRINE` and its
    /// name) built into a fleet of the open one's size.
    pub(crate) fn enemy_fleet(&self, id: &str) -> Option<Fleet> {
        if id == MIRROR {
            // The fleet as it is being edited, not as last saved, at the copy's own numbers.
            let mut fleet = self.fleet.as_ref()?.fleet.clone();
            fleet.name = trf!("{name} (mirror)", name = fleet.name);
            if let Some(n) = fleet.mirror_size {
                fleet.size = n;
            }
            for (slot, n) in fleet.slots.iter_mut().zip(fleet.mirror_counts.clone()) {
                if n.is_some() {
                    slot.override_count = n;
                }
            }
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
        let mut fleet = match self.supplied.iter().find(|d| d.side == crate::DoctrineSide::Enemy && d.doctrine.name == group) {
            Some(d) => d.doctrine.fleet(dogma, &|id| self.fit_docs.get(id).cloned(), self.skills()),
            None => spai_fit::fleet::from_doctrine(dogma, group, &fits, self.skills()),
        };
        fleet.id = id.to_owned();
        fleet.size = size;
        let mut cache = self.fleet_cache.borrow_mut();
        cache.doctrine.insert(0, (key, fleet.clone()));
        cache.doctrine.truncate(CACHED);
        Some(fleet)
    }

    /// A fleet of the doctrine's presets, each a slot by its hull's role.
    pub fn new_fleet_from_doctrine(&mut self, group: &str, now_ms: u64) {
        if let Some(d) = self.supplied.iter().find(|d| d.doctrine.name == group).map(|d| d.doctrine.clone()) {
            self.new_fleet_of(&d, now_ms);
            return;
        }
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

    /// Per burst kind, the fits that carry its module, among those loaded; looked through again
    /// only when the set of fits or documents changes.
    pub(crate) fn boosting_fits(&self) -> Vec<(BurstKind, Vec<(String, String)>)> {
        use std::hash::{Hash, Hasher};
        let doctrine_fits = self.doctrine_fit_names();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut ids: Vec<&String> = self.fit_docs.keys().collect();
        ids.sort();
        (ids, self.library.iter().map(|x| (&x.id, &x.name)).collect::<Vec<_>>(), &doctrine_fits).hash(&mut h);
        let key = h.finish();
        let hit = self.fleet_cache.borrow().boosting.as_ref().filter(|(k, _)| *k == key).map(|(_, v)| v.clone());
        hit.unwrap_or_else(|| {
            let dogma = self.dogma.as_ref();
            let v: Vec<(BurstKind, Vec<(String, String)>)> = BurstKind::COMBAT
                .iter()
                .map(|&kind| {
                    let fits = self.library.iter().map(|x| (x.id.clone(), x.name.clone())).chain(doctrine_fits.iter().cloned()).filter(|(id, _)| dogma.zip(self.fit_docs.get(id)).is_some_and(|(d, f)| fleet::fit_has_burst(d, f, kind))).collect();
                    (kind, fits)
                })
                .collect();
            self.fleet_cache.borrow_mut().boosting = Some((key, v.clone()));
            v
        })
    }

    pub(crate) fn cached_stats(&self, f: &Fleet) -> Arc<FleetStats> {
        let key = self.stats_key(f);
        // Which fleet it is, whatever its content: what to show while its numbers are redone.
        let lineage = format!("{}\u{1f}{}", f.id, f.name);
        {
            let mut cache = self.fleet_cache.borrow_mut();
            if let Some((_, s)) = cache.stats.iter().find(|(k, _)| *k == key) {
                return s.clone();
            }
            if let Some(i) = cache.stats_jobs.iter().position(|(k, _)| *k == key) {
                match cache.stats_jobs[i].1.try_recv() {
                    Ok(s) => {
                        let s = Arc::new(s);
                        cache.stats_jobs.remove(i);
                        cache.stats.insert(0, (key, s.clone()));
                        cache.stats.truncate(CACHED);
                        cache.last_of.insert(lineage, s.clone());
                        return s;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        if let Some(s) = cache.last_of.get(&lineage) {
                            return s.clone();
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        cache.stats_jobs.remove(i);
                    }
                }
            }
        }
        let Some(dogma) = self.dogma.clone() else { return Arc::new(FleetStats::default()) };
        let previous = self.fleet_cache.borrow().last_of.get(&lineage).cloned();
        // A fleet seen before keeps its last numbers on screen while the new ones are worked out
        // on their own thread: a charge swapped redoes every slot, too long for one frame. The web
        // build has no threads, and a fleet seen for the first time has nothing to show.
        if let (Some(previous), false) = (previous, cfg!(target_arch = "wasm32")) {
            let ids: Vec<&String> = f.slots.iter().map(|s| &s.fit_id).chain(f.boosters.iter().map(|(_, id)| id)).collect();
            let docs: HashMap<String, Fit> = ids.into_iter().filter_map(|id| self.fit_docs.get(id).map(|d| (id.clone(), d.clone()))).collect();
            let mut profiles: Vec<Option<String>> = f.slots.iter().map(|s| s.profile.clone()).collect();
            profiles.push(None);
            profiles.dedup();
            let skills: Vec<(Option<String>, SkillSet)> = profiles.into_iter().map(|p| (p.clone(), self.fleet_skills(p.as_deref()))).collect();
            let fleet = f.clone();
            let dogma = dogma.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let started = std::thread::Builder::new().name("fleet-stats".into()).spawn(move || {
                let fits = |id: &str| docs.get(id).cloned();
                let fallback = skills.iter().find(|(p, _)| p.is_none()).map(|(_, s)| s.clone()).unwrap_or_default();
                let skill = |p: Option<&str>| skills.iter().find(|(x, _)| x.as_deref() == p).map_or_else(|| fallback.clone(), |(_, s)| s.clone());
                let _ = tx.send(fleet::stats(&dogma, &fleet, &fits, &skill));
            });
            if started.is_ok() {
                let mut cache = self.fleet_cache.borrow_mut();
                cache.stats_jobs.push((key, rx));
                let now = self.now_ms;
                cache.pending.get_or_insert((key, now));
                return previous;
            }
        }
        let docs = &self.fit_docs;
        let fits = |id: &str| docs.get(id).cloned();
        let skills = |p: Option<&str>| self.fleet_skills(p);
        let _t = spai_ui::timing::span("sim: fleet stats, on the frame");
        let s = Arc::new(fleet::stats(&dogma, f, &fits, &skills));
        let mut cache = self.fleet_cache.borrow_mut();
        cache.stats.insert(0, (key, s.clone()));
        cache.stats.truncate(CACHED);
        cache.last_of.insert(lineage, s.clone());
        s
    }

}

/// The options as this fit flies them: the charge each weapon has loaded, and which it carries.
fn relabel(choices: &mut [spai_fit::fleet::ammo::AmmoChoice], fit: &Fit) {
    for c in choices {
        c.loaded = fit.modules.iter().find(|m| m.type_id == c.weapon).and_then(|m| m.charge);
        for (charge, carried, _) in &mut c.options {
            *carried = c.loaded == Some(*charge) || fit.cargo.iter().any(|x| x.type_id == *charge && x.quantity > 0);
        }
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
    /// Fleets' numbers being worked out on their own threads, by content key.
    stats_jobs: Vec<(u64, std::sync::mpsc::Receiver<FleetStats>)>,
    /// Each fleet's last numbers, by which fleet it is, shown while new ones are worked out.
    last_of: HashMap<String, Arc<FleetStats>>,
    /// An analysis being worked out on its own thread, by what it is of, and how far along it is
    /// in thousandths.
    job: Option<(u64, std::sync::mpsc::Receiver<(Analysis, Vec<(u64, Arc<Choices>)>)>, Arc<std::sync::atomic::AtomicU32>)>,
    /// The web build's run, taken a few steps a frame.
    local: Option<(u64, FightJob)>,
    /// Slots' charge options being worked out on their own threads, by `ammo_key`.
    ammo_jobs: Vec<(u64, std::sync::mpsc::Receiver<Arc<Choices>>)>,
    /// The web build has slots' options still to work out, one a frame.
    ammo_waiting: bool,
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
        let _t = spai_ui::timing::span("sim: fleet analysis");
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
            // Everything the sweep and the race read off the fleet: a knob left out here changed
            // nothing on screen, the last analysis kept as if it were still the one.
            let logi = (x.logi_efficiency, x.logi_prelock, x.logi_reaction_s, x.logi_spread_s, x.race_length_s);
            serde_json::to_string(&(x.range, x.motion, x.formation, &x.target_order, x.adc, x.volley_efficiency, x.ewar_auto, &x.ewar, &x.ewar_plan, x.split_fire, logi)).unwrap_or_default().hash(&mut h);
        }
        let key = h.finish();
        let now = self.now_ms;
        let cached = self.fleet_analysis.as_ref().map(|(k, a)| (*k, a.clone()));
        if cached.as_ref().is_some_and(|(k, _)| *k == key) {
            self.fleet_cache.get_mut().pending = None;
            return cached.map(|(_, a)| a);
        }
        // The web build's run under way: a few more of its steps this frame.
        if self.fleet_cache.get_mut().local.as_ref().is_some_and(|(k, _)| *k == key) {
            return self.step_local(key).or_else(|| cached.map(|(_, a)| a));
        }
        // A run under way: its result once it is in, the last one until then.
        let done = match &self.fleet_cache.get_mut().job {
            Some((k, rx, _)) if *k == key => match rx.try_recv() {
                Ok((out, fresh)) => {
                    let cache = self.fleet_cache.get_mut();
                    cache.ammo.extend(fresh);
                    let over = cache.ammo.len().saturating_sub(CACHED_SLOTS);
                    cache.ammo.drain(..over);
                    Some(Some(Arc::new(out)))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(None),
            },
            _ => Some(None),
        };
        match done {
            Some(Some(out)) => {
                let cache = self.fleet_cache.get_mut();
                cache.job = None;
                cache.pending = None;
                self.fleet_analysis = Some((key, out.clone()));
                return Some(out);
            }
            None => return cached.map(|(_, a)| a),
            Some(None) => {}
        }
        if let Some((_, a)) = &cached {
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
        let mine = self.cached_stats(&f.fleet);
        let theirs = self.cached_stats(&enemy);
        // Not on the numbers shown while new ones are worked out: they would be kept as the new.
        let ready = |k: u64| self.fleet_cache.borrow().stats.iter().any(|(x, _)| *x == k);
        if !ready(mine_key) || !ready(theirs_key) {
            return cached.map(|(_, a)| a);
        }
        let fleet = self.fleet.as_ref()?.fleet.clone();
        let known = self.fleet_cache.borrow().ammo.clone();
        let job = FightJob::new(dogma, fleet, enemy, mine, theirs, known);
        // Everything dear off the frame, the charges' options too: a big fight held the window
        // still for seconds. The web build has no threads: it takes the steps a few a frame.
        if cfg!(target_arch = "wasm32") {
            let cache = self.fleet_cache.get_mut();
            cache.local = Some((key, job));
            cache.pending = Some((key, now));
            return self.step_local(key).or_else(|| cached.map(|(_, a)| a));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let progress = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let along = progress.clone();
        std::thread::Builder::new()
            .name("fleet-analysis".into())
            .spawn(move || {
                let report = |f: f32| along.store((f.clamp(0.0, 1.0) * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed);
                let _ = tx.send(job.run(&report));
            })
            .ok()?;
        let cache = self.fleet_cache.get_mut();
        cache.job = Some((key, rx, progress));
        // Kept pending, so the host keeps drawing until the result is in.
        cache.pending = Some((key, now));
        cached.map(|(_, a)| a)
    }

    /// The web build's run, a frame's worth further: steps for a few milliseconds, then the page
    /// is drawn. The analysis once it is done.
    fn step_local(&mut self, key: u64) -> Option<Arc<Analysis>> {
        const BUDGET_MS: u128 = 12;
        let started = web_time::Instant::now();
        let cache = self.fleet_cache.get_mut();
        let (k, job) = cache.local.as_mut()?;
        if *k != key {
            return None;
        }
        let mut done = false;
        while !done && started.elapsed().as_millis() < BUDGET_MS {
            done = job.step();
        }
        if !done {
            return None;
        }
        let (_, job) = cache.local.take()?;
        let (analysis, fresh) = job.finish();
        cache.ammo.extend(fresh);
        let over = cache.ammo.len().saturating_sub(CACHED_SLOTS);
        cache.ammo.drain(..over);
        cache.pending = None;
        let out = Arc::new(analysis);
        self.fleet_analysis = Some((key, out.clone()));
        Some(out)
    }

    /// Whether an analysis waits for the knobs to rest, so the host keeps drawing until it is done.
    /// The fleet's numbers once every run under way is in, for tests that change a fleet and
    /// read it back at once.
    #[cfg(test)]
    pub(crate) fn fleet_stats_settled(&self) -> Option<(Arc<FleetStats>, Option<(Arc<FleetStats>, Against, Against)>)> {
        for _ in 0..500 {
            let out = self.fleet_stats();
            if self.fleet_cache.borrow().stats_jobs.is_empty() {
                return out;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        self.fleet_stats()
    }

    /// How far along the analysis being worked out is, 0 to 1; none when none is running.
    pub fn fleet_analysis_progress(&self) -> Option<f32> {
        let cache = self.fleet_cache.borrow();
        cache.job.as_ref().map(|(_, _, p)| p.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0).or_else(|| cache.local.as_ref().map(|(_, j)| j.progress()))
    }

    /// The analysis once the run it starts is in, for tests.
    #[cfg(test)]
    pub(crate) fn fleet_analysis_settled(&mut self) -> Option<Arc<Analysis>> {
        for _ in 0..1000 {
            let out = self.fleet_analysis();
            if out.is_some() && self.fleet_cache.borrow().job.is_none() {
                return out;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        self.fleet_analysis()
    }

    pub fn fleet_analysis_pending(&self) -> bool {
        let cache = self.fleet_cache.borrow();
        cache.pending.is_some() || !cache.stats_jobs.is_empty() || !cache.ammo_jobs.is_empty() || cache.ammo_waiting
    }

    /// Every slot's charge options, from those already worked out; a slot not yet has none, and
    /// is worked out off the frame (on the web, one a frame) for the frames after.
    pub(crate) fn ammo_for(&mut self, s: &Arc<FleetStats>) -> Arc<Vec<Choices>> {
        let Some(dogma) = self.dogma.clone() else { return Arc::default() };
        let env = format!("{:?}", s.env);
        let cache = self.fleet_cache.get_mut();
        // Runs that came in.
        let mut i = 0;
        while i < cache.ammo_jobs.len() {
            match cache.ammo_jobs[i].1.try_recv() {
                Ok(a) => {
                    let key = cache.ammo_jobs.remove(i).0;
                    cache.ammo.push((key, a));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => i += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    cache.ammo_jobs.remove(i);
                }
            }
        }
        let over = cache.ammo.len().saturating_sub(CACHED_SLOTS);
        cache.ammo.drain(..over);
        let mut stepped = false;
        cache.ammo_waiting = false;
        let out = s
            .slots
            .iter()
            .map(|x| {
                let key = ammo_key(x, &env);
                if let Some((_, a)) = cache.ammo.iter().find(|(k, _)| *k == key) {
                    let mut a = (**a).clone();
                    relabel(&mut a, &x.fit);
                    return a;
                }
                if cfg!(target_arch = "wasm32") {
                    if !stepped {
                        stepped = true;
                        let a = Arc::new(spai_fit::fleet::ammo::choices(&dogma, &x.fit, &x.skills, &s.env));
                        cache.ammo.push((key, a.clone()));
                        let mut a = (*a).clone();
                        relabel(&mut a, &x.fit);
                        return a;
                    }
                    cache.ammo_waiting = true;
                } else if !cache.ammo_jobs.iter().any(|(k, _)| *k == key) {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let (d, fit, skills, env) = (dogma.clone(), x.fit.clone(), x.skills.clone(), s.env.clone());
                    if std::thread::Builder::new().name("fleet-ammo".into()).spawn(move || {
                        let _ = tx.send(Arc::new(spai_fit::fleet::ammo::choices(&d, &fit, &skills, &env)));
                    }).is_ok() {
                        cache.ammo_jobs.push((key, rx));
                    }
                }
                Vec::new()
            })
            .collect();
        Arc::new(out)
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
/// The fleet editor's narrowest: its slot rows' pickers and counts side by side.
const EDITOR_MIN_W: f32 = 640.0;

pub fn editor(sim: &mut SimUi, ui: &mut egui::Ui, _now_ms: u64) {
    let _t = spai_ui::timing::span("sim: fleet editor");
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
    let doctrine_fits = sim.doctrine_fit_names();
    let dogma = sim.dogma.clone();
    // Per slot fit, its variations' refit keys: the fleet swaps them all at once.
    let refits: HashMap<String, Vec<(u32, String)>> = sim
        .fleet
        .as_ref()
        .map(|f| f.fleet.slots.iter().filter_map(|s| sim.fit_docs.get(&s.fit_id).map(|fit| (s.fit_id.clone(), fit.variations.iter().filter_map(|v| v.refit_key.clone().map(|k| (v.id, k))).collect()))).collect())
        .unwrap_or_default();
    let mut refit_keys: Vec<String> = refits.values().flatten().map(|(_, k)| k.clone()).collect();
    refit_keys.sort();
    refit_keys.dedup();
    let boosting = sim.boosting_fits();
    let mut want_all_docs = false;
    let enemy_doctrines: Vec<String> = sim.doctrine_groups_of(Some(crate::DoctrineSide::Enemy)).into_iter().map(|(g, _)| g).collect();
    // Each fit's variations, to fly a slot with one of them.
    let variations: HashMap<String, Vec<(u32, String)>> = sim.fit_docs.iter().map(|(id, f)| (id.clone(), f.variations.iter().map(|v| (v.id, v.full_name())).collect())).collect();
    let base_names: HashMap<String, String> = sim.fit_docs.iter().map(|(id, f)| (id.clone(), crate::base_label(f))).collect();
    let Some(session) = sim.fleet.as_mut() else { return };
    let f = &mut session.fleet;
    let mut changed = false;
    // Beside docked graphs it may get narrower than its rows: then it scrolls sideways.
    let out = egui::ScrollArea::both().id_salt("sim_fleet_editor").auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width().max(EDITOR_MIN_W));
        // The settings in cards; two columns where they fit, ships and boosters on the left.
        let two = ui.available_width() >= 1000.0;
        let mut part = |ui: &mut egui::Ui, which: usize| match which {
            0 => card(ui, tr!("Ships"), |ui| {
                // The fleet's name and how many fly, at the head of its ships.
                ui.horizontal(|ui| {
                    ui.label(tr!("Name"));
                    let w = (ui.available_width() - 130.0).max(100.0);
                    changed |= ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(w).hint_text(tr!("Fleet name"))).changed();
                    ui.label(tr!("Size"));
                    changed |= ui.add(egui::DragValue::new(&mut f.size).range(1..=2000)).changed();
                });
                if !refit_keys.is_empty() {
                    ui.horizontal(|ui| {
                        use spai_ui::widgets::SteadySelect as _;
                        let keyed: Vec<&FleetSlot> = f.slots.iter().filter(|s| refits.get(&s.fit_id).is_some_and(|v| !v.is_empty())).collect();
                        let key_of = |s: &FleetSlot| s.variation.and_then(|v| refits.get(&s.fit_id).and_then(|l| l.iter().find(|(id, _)| *id == v)).map(|(_, k)| k.clone()));
                        let first = keyed.first().map(|s| key_of(s));
                        let current = match first {
                            Some(k) if keyed.iter().all(|s| key_of(s) == k) => Some(k),
                            _ => None,
                        };
                        let shown = match &current {
                            Some(Some(k)) => crate::doctrine_panel::refit_name(k),
                            Some(None) => tr!("Base fits").to_owned(),
                            None => tr!("Mixed").to_owned(),
                        };
                        ui.label(tr!("Refit"));
                        let mut pick: Option<Option<String>> = None;
                        crate::combo::combo(ui, "sim_fleet_refit", 160.0, shown, |ui| {
                            if ui.menu_label(current == Some(None), tr!("Base fits")).clicked() {
                                pick = Some(None);
                            }
                            for k in &refit_keys {
                                if ui.menu_label(current.as_ref().is_some_and(|c| c.as_deref() == Some(k.as_str())), crate::doctrine_panel::refit_name(k)).clicked() {
                                    pick = Some(Some(k.clone()));
                                }
                            }
                        })
                        .response
                        .on_hover_text(tr!("Every ship that has this refit flies it; the others keep what they fly"));
                        if let Some(k) = pick {
                            for slot in &mut f.slots {
                                let Some(list) = refits.get(&slot.fit_id) else { continue };
                                match &k {
                                    None => {
                                        if slot.variation.is_some_and(|v| list.iter().any(|(id, _)| *id == v)) {
                                            slot.variation = None;
                                        }
                                    }
                                    Some(k) => {
                                        if let Some((id, _)) = list.iter().find(|(_, x)| x == k) {
                                            slot.variation = Some(*id);
                                        }
                                    }
                                }
                            }
                            changed = true;
                        }
                    });
                }
                ui.add_space(6.0);
            let mut remove: Option<usize> = None;
            let counts = fleet::resolve_counts(f);
            // Below this width the count controls go on a second line under the fit.
            let narrow = ui.available_width() < 720.0;
            for i in 0..f.slots.len() {
                let count = counts.get(i).copied().unwrap_or(0);
                let rule_row = |ui: &mut egui::Ui, slot: &mut FleetSlot, changed: &mut bool| {
                    let mut rule = slot.rule;
                    crate::combo::combo(ui, ("sim_fleet_rule", i), 84.0, rule_name(rule), |ui| {
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
                    let base = base_names.get(&slot.fit_id).cloned().unwrap_or_else(|| tr!("Base").to_owned());
                    let flying = slot.variation.and_then(|v| list.iter().find(|(x, _)| *x == v)).map_or_else(|| base.clone(), |(_, n)| n.clone());
                    ui.add_enabled_ui(!list.is_empty(), |ui| {
                        crate::combo::combo(ui, ("sim_fleet_variation", i), 120.0, &flying, |ui| {
                            if ui.menu_label(slot.variation.is_none(), base.as_str()).clicked() {
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
                    crate::combo::combo(ui, ("sim_fleet_fit", i), fit_w, &fit_name, |ui| {
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
            }),
            1 => card(ui, tr!("Boosters"), |ui| {
                let (c, w) = boosters_ui(ui, &mut f.boosters, &mut f.boost_shortcuts, &boosting, dogma.as_ref(), &library, &doctrine_fits, "sim_fleet_booster");
                changed |= c;
                want_all_docs |= w;
            }),
            2 => card(ui, tr!("Opponent"), |ui| {
            egui::Grid::new("sim_fleet_opponent").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tr!("Against"));
                let enemy_name = match session.enemy.as_deref() {
                    Some(MIRROR) => tr!("Itself (mirror match)").to_owned(),
                    Some(id) => id.strip_prefix(ENEMY_DOCTRINE).map(|g| trf!("{name} (enemy doctrine)", name = g)).or_else(|| fleets.iter().find(|x| x.id == id).map(|x| x.name.clone())).unwrap_or_else(|| tr!("none").to_owned()),
                    None => tr!("none").to_owned(),
                };
                crate::combo::combo(ui, "sim_fleet_enemy", 220.0, &enemy_name, |ui| {
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
                ui.label(tr!("Range"));
                let mut km = f.range / 1000.0;
                if ui.add(egui::Slider::new(&mut km, 0.0..=300.0).suffix(" km").step_by(0.5)).changed() {
                    f.range = km * 1000.0;
                    changed = true;
                }
                ui.end_row();
                ui.label(tr!("Race length")).on_hover_text(tr!("How long the race at this range may run before it stops"));
                let mut min = f.race_length_s / 60.0;
                if ui.add(egui::Slider::new(&mut min, 1.0..=240.0).logarithmic(true).suffix(" min").step_by(1.0)).changed() {
                    f.race_length_s = min * 60.0;
                    changed = true;
                }
                ui.end_row();
                if session.enemy.as_deref() == Some(MIRROR) {
                    ui.label(tr!("The copy's size")).on_hover_text(tr!("The mirror's own fleet size; its slots fill by the same rules"));
                    ui.horizontal(|ui| {
                        let mut on = f.mirror_size.is_some();
                        if ui.checkbox(&mut on, "").changed() {
                            f.mirror_size = on.then_some(f.size);
                            changed = true;
                        }
                        if let Some(n) = &mut f.mirror_size {
                            changed |= ui.add(egui::DragValue::new(n).range(1..=2000)).changed();
                        } else {
                            ui.label(egui::RichText::new(trf!("as this fleet, {n}", n = f.size)).weak());
                        }
                    });
                    ui.end_row();
                    f.mirror_counts.resize(f.slots.len(), None);
                    for k in 0..f.slots.len() {
                        let id = f.slots[k].fit_id.clone();
                        let name = library.iter().find(|x| x.id == id).map(|x| x.name.clone()).or_else(|| doctrine_fits.iter().find(|(d, _)| *d == id).map(|(_, n)| n.clone())).unwrap_or_default();
                        ui.label(egui::RichText::new(trf!("Their {n}", n = text::clip(&name, 22))).weak());
                        ui.horizontal(|ui| {
                            let mut on = f.mirror_counts[k].is_some();
                            if ui.checkbox(&mut on, "").on_hover_text(tr!("Type the copy's number of this ship")).changed() {
                                f.mirror_counts[k] = on.then_some(1);
                                changed = true;
                            }
                            if let Some(n) = &mut f.mirror_counts[k] {
                                changed |= ui.add(egui::DragValue::new(n).range(0..=2000)).changed();
                            } else {
                                ui.label(egui::RichText::new(tr!("by its rule")).weak());
                            }
                        });
                        ui.end_row();
                    }
                }
            });
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
            }),
            3 => card(ui, tr!("Formation and movement"), |ui| {
            egui::Grid::new("sim_fleet_formation").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
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
            });
            }),
            4 => card(ui, tr!("Fire"), |ui| {
            egui::Grid::new("sim_fleet_fire").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tr!("Volley efficiency"));
                changed |= ui.add(egui::Slider::new(&mut f.volley_efficiency, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                ui.end_row();
                ui.label(tr!("Drones"));
                changed |= ui.checkbox(&mut f.drones, tr!("Launched")).on_hover_text(tr!("Drones and fighters add damage per second, never to a volley: they attack on their own time, no alpha before logi react")).changed();
                ui.end_row();
                ui.label(tr!("ADCs and suppressors")).on_hover_text(tr!("Assault damage controls and signature suppressors: switched on, once, when their ship is called primary"));
                changed |= ui.checkbox(&mut f.adc, tr!("Switched on when primaried")).changed();
                ui.end_row();
                ui.label(tr!("Split fire")).on_hover_text(tr!("Fire groups each calling its own primary, the ships of each slot shared out by turns; 1 for one primary for all"));
                changed |= ui.add(egui::DragValue::new(&mut f.split_fire).range(1..=10).suffix(" ×")).changed();
                ui.end_row();
            });
            }),
            5 => card(ui, tr!("Logistics"), |ui| {
            egui::Grid::new("sim_fleet_logi").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tr!("Logistics efficiency")).on_hover_text(tr!("The share of the logistics' repairs that does any good: late locks, the wrong ship, cycles landing on full health"));
                changed |= ui.add(egui::Slider::new(&mut f.logi_efficiency, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                ui.end_row();
                ui.label(tr!("Logi pre-locked")).on_hover_text(tr!("The share of the logistics holding the fleet's boosters, recons and tackle locked before they are hurt: on those they skip the lock. Damage dealers are locked once hurt."));
                changed |= ui.add(egui::Slider::new(&mut f.logi_prelock, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                ui.end_row();
                ui.label(tr!("Logi reaction")).on_hover_text(tr!("Seconds until the logistics see a ship hurt, at the median; some quicker, a few slower"));
                changed |= ui.add(egui::Slider::new(&mut f.logi_reaction_s, 0.0..=10.0).suffix(" s").step_by(0.1)).changed();
                ui.end_row();
                ui.label(tr!("Logi arrival spread")).on_hover_text(tr!("Seconds over which the logistics' first repairs arrive, one after another, on top of their reactions"));
                changed |= ui.add(egui::Slider::new(&mut f.logi_spread_s, 0.0..=15.0).suffix(" s").step_by(0.5)).changed();
                ui.end_row();
            });
            }),
            6 => card(ui, tr!("Ewar on the enemy"), |ui| {
                ui.vertical(|ui| {
                    let p = projected;
                    let parts: Vec<String> = [(p.webs, tr!("webs")), (p.painters, tr!("painters")), (p.tracking_disruptors, tr!("tracking disruptors"))].iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect();
                    let summary = if parts.is_empty() { tr!("none: no ship of the fleet projects any").to_owned() } else { trf!("{list}, from the ships", list = parts.join(", ")) };
                    ui.add(egui::Label::new(egui::RichText::new(summary).weak()).truncate());
                    // Each kind on what the fleet shoots, or worked through the enemy fleet.
                    egui::Grid::new("sim_fleet_ewar_plan").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                        use spai_ui::widgets::SteadySelect as _;
                        for kind in spai_fit::fleet::ewar::EwarPlan::KINDS {
                            let name = match kind {
                                spai_fit::stats::RemoteKind::Scram => tr!("Tackle"),
                                k => crate::stats_panel::ewar_name(k).unwrap_or_default(),
                            };
                            ui.label(name);
                            ui.horizontal(|ui| {
                                let spread = f.ewar_plan.spreads(kind);
                                let w = egui::vec2(80.0, 22.0);
                                if ui.menu_label_sized(w, !spread, tr!("Focused")).on_hover_text(tr!("On the ship the fleet shoots, or shared over each group's when its fire splits")).clicked() && spread {
                                    f.ewar_plan.set(kind, false);
                                    changed = true;
                                }
                                if ui.menu_label_sized(w, spread, tr!("Spread")).on_hover_text(tr!("Worked through the whole enemy fleet")).clicked() && !spread {
                                    f.ewar_plan.set(kind, true);
                                    changed = true;
                                }
                            });
                            ui.end_row();
                        }
                        ui.label(tr!("Spread over"));
                        let mut pct = f.ewar_plan.coverage * 100.0;
                        let r = ui.add(egui::Slider::new(&mut pct, 10.0..=100.0).suffix(tr!("% of their fleet")).step_by(5.0));
                        if r.on_hover_text(tr!("Spreading is never even: the share of the enemy fleet the spread ewar actually reaches, the ships it reaches taking the more of it")).changed() {
                            f.ewar_plan.coverage = pct / 100.0;
                            changed = true;
                        }
                        ui.end_row();
                    });
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
            egui::Grid::new("sim_fleet_skills").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                if !profiles.is_empty() {
                    ui.label(tr!("Skills"));
                    ui.label(egui::RichText::new(tr!("the profile chosen above")).weak());
                    ui.end_row();
                }
            });
            }),
            _ => {}
        };
        if two {
            ui.columns(2, |cols| {
                for w in [0, 1] {
                    part(&mut cols[0], w);
                }
                for w in [2, 3, 4, 5, 6] {
                    part(&mut cols[1], w);
                }
            });
        } else {
            for w in 0..7 {
                part(ui, w);
            }
        }
    });
    crate::rows::note_scrolled(ui.ctx(), &out);
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

/// A titled part of the results that opens and closes, remembered.
fn section(ui: &mut egui::Ui, id: &str, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(id).default_open(open).show_unindented(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        body(ui);
    });
    ui.add_space(2.0);
    ui.separator();
}

/// A titled group of settings across the width given.
fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong());
        ui.add_space(4.0);
        body(ui);
    });
    ui.add_space(6.0);
}

/// The numbers: one direction of the fight at a time, picked at the top, then the shooting
/// fleet's own figures.
pub fn results(sim: &mut SimUi, ui: &mut egui::Ui) {
    let _t = spai_ui::timing::span("sim: fleet results");
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some((mine, enemy)) = sim.fleet_stats() else {
        ui.label(egui::RichText::new(tr!("No fleet open.")).weak());
        return;
    };
    let names: HashMap<String, String> = sim.library.iter().map(|f| (f.id.clone(), f.name.clone())).collect();
    let analysis = sim.fleet_analysis();
    if sim.fleet_analysis_pending() {
        working_badge(ui, sim.fleet_analysis_progress());
    }
    if let Some((theirs, _, _)) = &enemy {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            use spai_ui::widgets::SteadySelect as _;
            ui.add_space(crate::rows::GAP);
            let w = egui::vec2(((ui.available_width() - crate::rows::GAP * 2.0 - ui.spacing().item_spacing.x) / 2.0).max(60.0), 24.0);
            // One name a button, the shooter's: two would not fit the column. The button cuts it to
            // its width.
            for (dir, (a, b)) in [(0, (&mine.name, &theirs.name)), (1, (&theirs.name, &mine.name))] {
                let label = trf!("{a} shoots", a = a);
                if ui.menu_label_sized(w, sim.fleet_view == dir, label).on_hover_text(trf!("{a} shooting {b}", a = a, b = b)).clicked() {
                    sim.fleet_view = dir;
                }
            }
        });
        ui.separator();
    }
    let dir = sim.fleet_view.min(1);
    egui::ScrollArea::vertical().id_salt("sim_fleet_results").auto_shrink([false, false]).show(ui, |ui| {
        // Room on the right for the scroll bar.
        ui.set_max_width(ui.available_width() - 12.0);
        ui.spacing_mut().item_spacing.y = 1.0;
        let Some((theirs, _, _)) = &enemy else {
            fleet_card(ui, &dogma, &mine, &names);
            ui.add_space(6.0);
            note(ui, tr!("Pick a fleet under Against to compare."));
            return;
        };
        let shooter = if dir == 0 { &mine } else { theirs };
        if let Some(an) = &analysis {
            engagement_card(ui, &dogma, an, dir, &names);
        }
        section(ui, "sim_fleet_sec_figures", &trf!("{a}: its figures", a = shooter.name), false, |ui| fleet_card(ui, &dogma, shooter, &names));
    });
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
    section(ui, "sim_fleet_sec_engagement", tr!("Engagement"), true, |ui| {
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
    });
    let r = &an.race;
    section(ui, "sim_fleet_sec_race", tr!("Race at this range"), true, |ui| {
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
            if r.fired[side] > 0.0 {
                tline(ui, &trf!("{a} damage on wrecks", a = name), format!("{}%", text::num(100.0 * r.wasted[side] / r.fired[side])), |ui| tip(ui, tr!("Damage on wrecks"), &[], tr!("The share of what it sent that reached a ship already dead: missiles in flight when their target died. A turret holds its volley for the next primary instead.")));
            }
            let other = 1 - side;
            tline(ui, &trf!("{a} reps catch up", a = name), match r.critical_mass_lost[other] {
                Some(t) => {
                    let left = r.timeline.iter().find(|x| x.0 >= t).map_or(0, |x| if other == 0 { x.1 } else { x.2 });
                    trf!("at {s}, {n} enemies left", s = text::secs(t), n = left)
                }
                None => tr!("never").to_owned(),
            }, |ui| tip(ui, tr!("Reps catch up"), &[], tr!("From here the other side's damage on its primary, through the resistances of the layer being worn, no longer beats the repairs on it, and one volley of it no longer kills a ship outright: it has lost its critical mass.")));
        }
    });
    let target_side = 1 - dir;
    if !r.verdicts[target_side].is_empty() {
        section(ui, "sim_fleet_sec_verdicts", &trf!("{a} when primaried", a = target.name), false, |ui| {
            if !r.verdicts[target_side].is_empty() {
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
        });
    }
    if an.ammo_bands[dir].iter().any(|(_, _, b)| b.len() >= 2 || b.iter().any(|x| x.better.is_some())) {
        section(ui, "sim_fleet_sec_ammo", tr!("Ammo by range"), false, |ui| {
            // The shooter's ammo by range, against its first target as the fight leaves it.
            for (k, weapon, bands) in &an.ammo_bands[dir] {
                if bands.len() < 2 && bands.iter().all(|b| b.better.is_none()) {
                    continue;
                }
                let Some(slot) = shooter.slots.get(*k) else { continue };
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
        });
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

/// The burst boosters, one line per kind: a fit of the fleet's that carries the burst, or a
/// generic booster on a hull with its skills, mindlink and charges. (changed, wants every fit's
/// document to look through).
#[allow(clippy::too_many_arguments)]
pub(crate) fn boosters_ui(ui: &mut egui::Ui, boosters: &mut Vec<(BurstKind, String)>, shortcuts: &mut Vec<spai_fit::fit::BoostShortcut>, boosting: &[(BurstKind, Vec<(String, String)>)], dogma: Option<&Arc<spai_fit::Dogma>>, library: &[FitSummary], doctrine_fits: &[(String, String)], salt: &str) -> (bool, bool) {
    let mut changed = false;
    let mut want_all_docs = false;
            for (kind, fits) in boosting {
                let kind = *kind;
                ui.horizontal(|ui| {
                    ui.set_height(24.0);
                    ui.add_sized([90.0, 22.0], egui::Label::new(kind_name(kind)));
                    let current = boosters.iter().find(|(k, _)| *k == kind).map(|(_, id)| id.clone());
                    let shortcut = shortcuts.iter().position(|b| b.kind == kind);
                    let name = if let Some(i) = shortcut {
                        let hull = shortcuts[i].hull.zip(dogma).map_or_else(|| tr!("no hull bonus").to_owned(), |(h, d)| spai_fit::formats::name_of(d, h));
                        trf!("Booster · {hull}", hull = hull)
                    } else {
                        current.as_ref().and_then(|id| library.iter().find(|x| &x.id == id).map(|x| x.name.clone()).or_else(|| doctrine_fits.iter().find(|(d, _)| d == id).map(|(_, n)| n.clone()))).unwrap_or_else(|| tr!("none").to_owned())
                    };
                    // None clears both; Some(Ok(fit)) a fit; Some(Err(hull)) a generic booster.
                    let mut pick: Option<Option<Result<String, Option<spai_fit::ids::TypeId>>>> = None;
                    let combo_w = (ui.available_width() - 8.0 - if shortcut.is_some() { 4.0 * 26.0 } else { 0.0 }).clamp(120.0, 260.0);
                    crate::combo::combo(ui, (salt, kind_name(kind)), combo_w, &name, |ui| {
                        want_all_docs = true;
                        if ui.menu_label(current.is_none() && shortcut.is_none(), tr!("none")).clicked() {
                            pick = Some(None);
                        }
                        for (id, n) in fits {
                            if ui.menu_label(current.as_deref() == Some(id.as_str()), n.clone()).clicked() {
                                pick = Some(Some(Ok(id.clone())));
                            }
                        }
                        if let Some(d) = dogma {
                            ui.separator();
                            ui.label(egui::RichText::new(tr!("Booster, in no slot")).weak());
                            let on_hull = |h: Option<spai_fit::ids::TypeId>| shortcut.is_some_and(|i| shortcuts[i].hull == h);
                            if ui.menu_label(on_hull(None), tr!("A hull without burst bonuses")).clicked() {
                                pick = Some(Some(Err(None)));
                            }
                            for hull in fleet::boosting_hulls(d, kind) {
                                let r = ui.menu_label(on_hull(Some(hull)), spai_fit::formats::name_of(d, hull));
                                let r = match crate::dock::boost_subsystem_hint(d, hull, kind) {
                                    Some(h) => r.on_hover_text(h),
                                    None => r,
                                };
                                if r.clicked() {
                                    pick = Some(Some(Err(Some(hull))));
                                }
                            }
                        }
                    });
                    // The generic booster's buttons, as on a fit's projected tab.
                    if let (Some(i), Some(d)) = (shortcut, dogma) {
                        let b = shortcuts[i].clone();
                        let mut next = b.clone();
                        let module = b.module.or_else(|| d.type_named(b.kind.module_name()).map(|t| t.id)).unwrap_or(0);
                        let skills_btn = spai_ui::widgets::icon_button(ui, icon::GRADUATION_CAP).on_hover_text(tr!("The booster's skills"));
                        crate::dock::shortcut_skills_popup(&skills_btn, d, &b, &mut next, (salt, "skills", i));
                        if spai_ui::widgets::icon_button(ui, crate::dock::mindlink_glyph(ui, b.mindlink)).on_hover_text(crate::dock::mindlink_hint(b.mindlink)).clicked() {
                            next.mindlink = !next.mindlink;
                        }
                        let charges_btn = spai_ui::widgets::icon_button(ui, icon::LIST_CHECKS).on_hover_text(tr!("The burst module and its charges"));
                        crate::dock::shortcut_charges_popup(&charges_btn, d, &b, &mut next, module);
                        if spai_ui::widgets::icon_button(ui, crate::dock::switch_glyph(ui, b.active)).on_hover_text(if b.active { tr!("Switch off") } else { tr!("Switch on") }).clicked() {
                            next.active = !next.active;
                        }
                        if next != b {
                            shortcuts[i] = next;
                            changed = true;
                        }
                    }
                    if let Some(p) = pick {
                        boosters.retain(|(k, _)| *k != kind);
                        match p {
                            Some(Err(hull)) => match shortcuts.iter_mut().find(|b| b.kind == kind) {
                                // Another hull keeps the skills, mindlink and charges set.
                                Some(b) => b.hull = hull,
                                None => shortcuts.push(spai_fit::fit::BoostShortcut { kind, hull, ..Default::default() }),
                            },
                            Some(Ok(id)) => {
                                shortcuts.retain(|b| b.kind != kind);
                                boosters.push((kind, id));
                            }
                            None => shortcuts.retain(|b| b.kind != kind),
                        }
                        changed = true;
                    }
                });
            }
    (changed, want_all_docs)
}

/// Over the bottom right of the area, outside the layout so nothing moves: a spinner and a word while
/// the fight is worked out again.
pub(crate) fn working_badge(ui: &mut egui::Ui, progress: Option<f32>) {
    let text = match progress {
        Some(p) => trf!("Working out the fight… {pct}%", pct = (p * 100.0).round() as u32),
        None => tr!("Working out the fight…").to_owned(),
    };
    let font = egui::TextStyle::Body.resolve(ui.style());
    // Sized for the widest the figure gets, so the badge does not twitch as it counts.
    let widest = progress.map_or_else(|| text.clone(), |_| trf!("Working out the fight… {pct}%", pct = 100));
    let room = ui.painter().layout_no_wrap(widest, font.clone(), ui.visuals().text_color()).size().x;
    let galley = ui.painter().layout_no_wrap(text, font, ui.visuals().text_color());
    let size = egui::vec2(room.max(galley.size().x) + 30.0, galley.size().y + 8.0);
    let max = ui.max_rect();
    // The bottom right corner, clear of the tabs and settings at the top.
    let rect = egui::Rect::from_min_size(egui::pos2(max.max.x - size.x - 12.0, max.max.y - size.y - 12.0), size);
    let over = ui.new_child(egui::UiBuilder::new().layer_id(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("sim_fleet_working"))).max_rect(rect));
    let painter = over.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color.gamma_multiply(0.95));
    painter.rect_stroke(rect, 4.0, ui.visuals().widgets.noninteractive.bg_stroke, egui::StrokeKind::Inside);
    let spin = egui::Rect::from_center_size(egui::pos2(rect.min.x + 13.0, rect.center().y), egui::vec2(14.0, 14.0));
    egui::Spinner::new().size(14.0).paint_at(&over, spin);
    painter.galley(egui::pos2(rect.min.x + 24.0, rect.min.y + 4.0), galley, ui.visuals().text_color());
    ui.ctx().request_repaint();
}

/// The sweep over the ranges, the race, and each side's charges by range: the analysis of two
/// fleets whose numbers are worked out.
type Choices = Vec<spai_fit::fleet::ammo::AmmoChoice>;

/// What a slot's charge options depend on: its fit with what it has loaded left out (that changes
/// which is loaded, not their figures), its pilot and the boosts.
fn ammo_key(x: &spai_fit::fleet::SlotStats, env: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut bare = x.fit.clone();
    for m in &mut bare.modules {
        m.charge = m.charge.map(|_| 0);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&bare).unwrap_or_default().hash(&mut h);
    let mut levels: Vec<(u32, u8)> = x.skills.levels.iter().map(|(k, v)| (*k, *v)).collect();
    levels.sort_unstable();
    (levels, x.skills.fallback, env).hash(&mut h);
    h.finish()
}

/// Ranges of the sweep worked out in one step.
const RANGES_A_STEP: usize = 8;

#[derive(Clone, Copy)]
enum Stage {
    Ammo(usize, usize),
    Points(usize),
    Finish,
    Race,
    Bands(usize, usize),
    Done,
}

/// The fight between two fleets as steps, each short: a thread runs them all, the web build a few
/// a frame, so the page is drawn in between.
pub(crate) struct FightJob {
    dogma: Arc<spai_fit::Dogma>,
    fleet: Fleet,
    enemy: Fleet,
    mine: Arc<FleetStats>,
    theirs: Arc<FleetStats>,
    /// Slot options already worked out, by `ammo_key`; the new ones are handed back for the cache.
    known: Vec<(u64, Arc<Choices>)>,
    fresh: Vec<(u64, Arc<Choices>)>,
    ammo: [Vec<Choices>; 2],
    ranges: Vec<f64>,
    points: Vec<spai_fit::fleet::sweep::RangePoint>,
    sweep: Option<spai_fit::fleet::sweep::Sweep>,
    race: Option<spai_fit::fleet::race::Race>,
    bands: [Vec<(usize, spai_fit::ids::TypeId, Vec<spai_fit::fleet::ammo::Band>)>; 2],
    stage: Stage,
    done: usize,
    total: usize,
}

impl FightJob {
    pub(crate) fn new(dogma: Arc<spai_fit::Dogma>, fleet: Fleet, enemy: Fleet, mine: Arc<FleetStats>, theirs: Arc<FleetStats>, known: Vec<(u64, Arc<Choices>)>) -> Self {
        let mut ranges = spai_fit::fleet::sweep::ranges(&mine, &theirs);
        ranges.push(f64::from(fleet.range));
        ranges.sort_by(|x, y| x.total_cmp(y));
        ranges.dedup();
        let slots = mine.slots.len() + theirs.slots.len();
        let total = slots * 2 + ranges.len().div_ceil(RANGES_A_STEP) + 2;
        FightJob { dogma, fleet, enemy, mine, theirs, known, fresh: Vec::new(), ammo: [Vec::new(), Vec::new()], ranges, points: Vec::new(), sweep: None, race: None, bands: [Vec::new(), Vec::new()], stage: Stage::Ammo(0, 0), done: 0, total }
    }

    pub(crate) fn progress(&self) -> f32 {
        self.done as f32 / self.total.max(1) as f32
    }

    fn stats(&self, side: usize) -> &Arc<FleetStats> {
        if side == 0 { &self.mine } else { &self.theirs }
    }

    /// One step; true once all are done.
    pub(crate) fn step(&mut self) -> bool {
        use spai_fit::fleet::sweep;
        let dogma = self.dogma.clone();
        match self.stage {
            Stage::Ammo(side, k) => {
                let stats = self.stats(side).clone();
                if let Some(x) = stats.slots.get(k) {
                    let key = ammo_key(x, &format!("{:?}", stats.env));
                    let found = self.known.iter().chain(&self.fresh).find(|(kk, _)| *kk == key).map(|(_, a)| a.clone());
                    let a = found.unwrap_or_else(|| {
                        let a = Arc::new(spai_fit::fleet::ammo::choices(&dogma, &x.fit, &x.skills, &stats.env));
                        self.fresh.push((key, a.clone()));
                        a
                    });
                    let mut a = (*a).clone();
                    relabel(&mut a, &x.fit);
                    self.ammo[side].push(a);
                    self.done += 1;
                    self.stage = Stage::Ammo(side, k + 1);
                } else if side == 0 {
                    self.stage = Stage::Ammo(1, 0);
                } else {
                    self.stage = Stage::Points(0);
                }
            }
            Stage::Points(at) => {
                let a = sweep::Side { fleet: &self.fleet, stats: &self.mine, ammo: &self.ammo[0] };
                let b = sweep::Side { fleet: &self.enemy, stats: &self.theirs, ammo: &self.ammo[1] };
                let to = (at + RANGES_A_STEP).min(self.ranges.len());
                let new: Vec<sweep::RangePoint> = self.ranges[at..to].iter().map(|&d| sweep::point(&a, &b, d)).collect();
                self.points.extend(new);
                self.done += 1;
                self.stage = if to >= self.ranges.len() { Stage::Finish } else { Stage::Points(to) };
            }
            Stage::Finish => {
                let a = sweep::Side { fleet: &self.fleet, stats: &self.mine, ammo: &self.ammo[0] };
                let b = sweep::Side { fleet: &self.enemy, stats: &self.theirs, ammo: &self.ammo[1] };
                self.sweep = Some(sweep::finish(&a, &b, std::mem::take(&mut self.points)));
                self.done += 1;
                self.stage = Stage::Race;
            }
            Stage::Race => {
                let a = sweep::Side { fleet: &self.fleet, stats: &self.mine, ammo: &self.ammo[0] };
                let b = sweep::Side { fleet: &self.enemy, stats: &self.theirs, ammo: &self.ammo[1] };
                self.race = Some(spai_fit::fleet::race::race(&dogma, &a, &b, f64::from(self.fleet.range), f64::from(self.fleet.race_length_s.max(60.0))));
                self.done += 1;
                self.stage = Stage::Bands(0, 0);
            }
            Stage::Bands(side, k) => {
                let a = sweep::Side { fleet: &self.fleet, stats: &self.mine, ammo: &self.ammo[0] };
                let b = sweep::Side { fleet: &self.enemy, stats: &self.theirs, ammo: &self.ammo[1] };
                let (x, y) = if side == 0 { (&a, &b) } else { (&b, &a) };
                if k < x.stats.slots.len() {
                    let ranges: Vec<f64> = self.sweep.as_ref().map(|s| s.points.iter().map(|p| p.distance).collect()).unwrap_or_default();
                    let found = match sweep::primary(x.fleet, y.stats) {
                        Some(t) if x.stats.slots[k].count > 0 => sweep::ammo_bands(x, y, k, t, &ranges).into_iter().map(|(w, b)| (k, w, b)).collect(),
                        _ => Vec::new(),
                    };
                    self.bands[side].extend(found);
                    self.done += 1;
                    self.stage = Stage::Bands(side, k + 1);
                } else if side == 0 {
                    self.stage = Stage::Bands(1, 0);
                } else {
                    self.stage = Stage::Done;
                }
            }
            Stage::Done => return true,
        }
        matches!(self.stage, Stage::Done)
    }

    /// Steps until done, telling `report` how far along.
    pub(crate) fn run(mut self, report: &dyn Fn(f32)) -> (Analysis, Vec<(u64, Arc<Choices>)>) {
        while !self.step() {
            report(self.progress());
        }
        self.finish()
    }

    pub(crate) fn finish(self) -> (Analysis, Vec<(u64, Arc<Choices>)>) {
        let range = self.fleet.range;
        let [ma, ta] = self.ammo;
        let analysis = Analysis { fleet: self.fleet, range, mine: self.mine, theirs: self.theirs, enemy: self.enemy, ammo: [Arc::new(ma), Arc::new(ta)], sweep: self.sweep.unwrap_or_default(), race: self.race.unwrap_or_default(), ammo_bands: self.bands };
        (analysis, self.fresh)
    }
}
