//! Doctrines: an alliance's fleets written down as the fits that fly together, nested by
//! alliance, then by kind (strategic, peacetime, capitals...), then by doctrine.
//!
//! Supplied doctrines come from the server and carry their fits inline. A pilot's own doctrines
//! name fits of their library instead, so a fit in one edits like any other; a copy of a supplied
//! doctrine moves its fits into the library, and a link shows the supplied one as it is now.

use serde::{Deserialize, Serialize};

use crate::dogma::Dogma;
use crate::fit::Fit;
use crate::fleet::{BurstKind, CountRule, Fleet, FleetSlot};
use crate::skills::SkillSet;

pub mod forum;

/// What a fit does for its doctrine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FitRole {
    #[default]
    Line,
    Logi,
    Command,
    Tackle,
    Ewar,
    Support,
}

impl FitRole {
    pub const ALL: [FitRole; 6] = [FitRole::Line, FitRole::Logi, FitRole::Command, FitRole::Tackle, FitRole::Ewar, FitRole::Support];

    /// How many fly of a fit in this role: the mainline fills what the others leave, several
    /// mainline hulls share it.
    pub fn rule(self, main: bool) -> CountRule {
        match (main, self) {
            (true, _) => CountRule::Fill { min: 0, max: 1000 },
            (_, FitRole::Logi) => CountRule::Percent(15.0),
            (_, FitRole::Line) => CountRule::Percent(5.0),
            (_, FitRole::Tackle) => CountRule::Fixed(3),
            (_, FitRole::Command | FitRole::Ewar | FitRole::Support) => CountRule::Fixed(2),
        }
    }

    /// By the hull's group, the way the fleet screen counts a slot.
    pub fn of_hull(dogma: &Dogma, ship: crate::ids::TypeId) -> FitRole {
        let group = dogma.type_(ship).map(|t| t.group);
        match group {
            Some(g) if crate::fleet::LOGI_GROUPS.contains(&g) => FitRole::Logi,
            Some(g) if crate::fleet::COMMAND_GROUPS.contains(&g) => FitRole::Command,
            Some(g) if crate::fleet::TACKLE_GROUPS.contains(&g) => FitRole::Tackle,
            Some(g) if crate::fleet::EWAR_GROUPS.contains(&g) => FitRole::Ewar,
            _ => FitRole::Line,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DoctrineFit {
    /// The fit's id: a library fit's for a pilot's own doctrine, `doctrine~key` for a supplied one.
    pub fit_id: String,
    /// The fit itself, for a supplied doctrine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    pub role: FitRole,
    /// The doctrine's mainline damage: one hull, or several that fly as one (bombers).
    pub main: bool,
    /// What the source says about it: how many to bring, when to fly it.
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Doctrine {
    pub id: String,
    pub alliance: String,
    /// Strategic, Peacetime, Capitals...: the level under the alliance.
    pub category: String,
    pub name: String,
    pub tags: Vec<String>,
    /// Where the doctrine is kept, such as its forum topic.
    pub url: Option<String>,
    /// When the source last changed, in Unix seconds.
    pub updated: Option<i64>,
    pub notes: String,
    pub fits: Vec<DoctrineFit>,
    /// The supplied doctrine this one was copied from, or links to.
    pub origin: Option<String>,
    /// A link: it shows the supplied doctrine `origin` as it is now and has no fits of its own.
    pub link: bool,
}

/// The order kinds of doctrine list in; others follow by name.
pub const CATEGORIES: [&str; 6] = ["Strategic", "Peacetime", "Capitals", "SIGs", "Utility", "Retired"];

pub fn category_rank(category: &str) -> usize {
    CATEGORIES.iter().position(|c| c.eq_ignore_ascii_case(category)).unwrap_or(CATEGORIES.len())
}

/// A supplied doctrine's fit id.
pub fn supplied_fit_id(doctrine: &str, key: &str) -> String {
    format!("{doctrine}~{key}")
}

impl Doctrine {
    /// The supplied doctrine's fits under their ids, ready for the fleet screen.
    pub fn inline_fits(&self) -> impl Iterator<Item = (&str, &Fit)> {
        self.fits.iter().filter_map(|f| f.fit.as_ref().map(|fit| (f.fit_id.as_str(), fit)))
    }

    /// The fits a fleet of it flies, mainline first, looked up by id where they are not inline.
    pub fn fleet(&self, dogma: &Dogma, lookup: &dyn Fn(&str) -> Option<Fit>, skills: &SkillSet) -> Fleet {
        let mut order: Vec<&DoctrineFit> = self.fits.iter().collect();
        order.sort_by_key(|f| !f.main);
        let fits: Vec<(String, Fit, &DoctrineFit)> = order.into_iter().filter_map(|f| f.fit.clone().or_else(|| lookup(&f.fit_id)).map(|fit| (f.fit_id.clone(), fit, f))).collect();
        let pairs: Vec<(String, Fit)> = fits.iter().map(|(id, fit, _)| (id.clone(), fit.clone())).collect();
        let mut fleet = crate::fleet::from_doctrine(dogma, &self.name, &pairs, skills);
        for (slot, (_, _, f)) in fleet.slots.iter_mut().zip(&fits) {
            slot.rule = f.role.rule(f.main);
        }
        fleet.main = fits.iter().position(|(_, _, f)| f.main).or(fleet.main);
        fleet
    }

    /// A pilot's own copy: each fit a new library fit under `new_id`, the doctrine naming them.
    pub fn copy_into_library(&self, new_id: &mut dyn FnMut() -> String) -> (Doctrine, Vec<Fit>) {
        let id = new_id();
        let mut fits = Vec::new();
        let mut copy = Doctrine { id, origin: Some(self.id.clone()), link: false, fits: Vec::new(), ..self.clone() };
        for f in &self.fits {
            let Some(fit) = &f.fit else {
                copy.fits.push(f.clone());
                continue;
            };
            let fit_id = new_id();
            let mut fit = fit.clone();
            fit.id = fit_id.clone();
            if !fit.tags.iter().any(|t| t == &self.name) {
                fit.tags.push(self.name.clone());
            }
            copy.fits.push(DoctrineFit { fit_id, fit: None, ..f.clone() });
            fits.push(fit);
        }
        // Boosters projected between the doctrine's fits follow them into the library.
        let moved: Vec<(String, String)> = self.fits.iter().zip(&copy.fits).filter(|(a, _)| a.fit.is_some()).map(|(a, b)| (a.fit_id.clone(), b.fit_id.clone())).collect();
        for fit in &mut fits {
            for p in &mut fit.projected_fits {
                if let Some((_, new)) = moved.iter().find(|(old, _)| *old == p.fit_id) {
                    p.fit_id = new.clone();
                }
            }
        }
        (copy, fits)
    }

    /// A pilot's link to a supplied doctrine: their own tags and place, the supplied fits.
    pub fn link_to(&self, id: String) -> Doctrine {
        Doctrine { id, alliance: self.alliance.clone(), category: self.category.clone(), name: self.name.clone(), tags: self.tags.clone(), origin: Some(self.id.clone()), link: true, ..Doctrine::default() }
    }

    /// The refit keys its fits share, for swapping a whole fleet at once.
    pub fn refit_keys(&self, lookup: &dyn Fn(&str) -> Option<Fit>) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for f in &self.fits {
            let Some(fit) = f.fit.clone().or_else(|| lookup(&f.fit_id)) else { continue };
            for k in fit.variations.iter().filter_map(|v| v.refit_key.clone()) {
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        keys.sort();
        keys
    }
}

/// Every slot of the fleet on the variation with this refit key, or on its base fit; a fit
/// without that key stays as it is.
pub fn swap_refit(fleet: &mut Fleet, key: Option<&str>, fits: &dyn Fn(&str) -> Option<Fit>) {
    for slot in &mut fleet.slots {
        let Some(fit) = fits(&slot.fit_id) else { continue };
        match key {
            None => slot.variation = None,
            Some(k) => {
                if let Some(v) = fit.variations.iter().find(|v| v.refit_key.as_deref() == Some(k)) {
                    slot.variation = Some(v.id);
                }
            }
        }
    }
}

/// The refit key every slot that has it flies, if they agree; none when the fleet flies its
/// base fits.
pub fn current_refit(fleet: &Fleet, fits: &dyn Fn(&str) -> Option<Fit>) -> Option<String> {
    let mut seen: Option<Option<String>> = None;
    for slot in &fleet.slots {
        let Some(fit) = fits(&slot.fit_id) else { continue };
        if fit.variations.iter().all(|v| v.refit_key.is_none()) {
            continue;
        }
        let key = slot.variation.and_then(|id| fit.variations.iter().find(|v| v.id == id)).and_then(|v| v.refit_key.clone());
        match &seen {
            None => seen = Some(key),
            Some(k) if *k == key => {}
            Some(_) => return None,
        }
    }
    seen.flatten()
}

/// The group a booster's mindlink variations sit under.
pub const MINDLINK_GROUP: &str = "Mindlink";

/// Charges an empty burst of the kind loads, first unused first.
fn default_charges(kind: BurstKind) -> &'static [&'static str] {
    match kind {
        BurstKind::Shield => &["Shield Harmonizing Charge", "Shield Extension Charge", "Active Shielding Charge"],
        BurstKind::Armor => &["Armor Energizing Charge", "Armor Reinforcement Charge", "Rapid Repair Charge"],
        BurstKind::Skirmish => &["Rapid Deployment Charge", "Interdiction Maneuvers Charge", "Evasive Maneuvers Charge"],
        BurstKind::Information => &["Sensor Optimization Charge", "Electronic Superiority Charge", "Electronic Hardening Charge"],
        BurstKind::Mining => &[],
    }
}

fn burst_kind(dogma: &Dogma, type_id: crate::ids::TypeId) -> Option<BurstKind> {
    let t = dogma.type_(type_id)?;
    BurstKind::COMBAT.into_iter().find(|k| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|x| x.name == k.effect())))
}

/// The combat burst kinds a fit runs, most bursts first.
pub fn burst_kinds(dogma: &Dogma, fit: &Fit) -> Vec<BurstKind> {
    let mut counts: Vec<(BurstKind, usize)> = Vec::new();
    for k in fit.modules.iter().filter_map(|m| burst_kind(dogma, m.type_id)) {
        match counts.iter_mut().find(|(c, _)| *c == k) {
            Some((_, n)) => *n += 1,
            None => counts.push((k, 1)),
        }
    }
    counts.sort_by_key(|(k, n)| (std::cmp::Reverse(*n), BurstKind::COMBAT.iter().position(|c| c == k)));
    counts.into_iter().map(|(k, _)| k).collect()
}

fn mindlink_variation(fit: &Fit, kind: BurstKind) -> Option<u32> {
    fit.variations.iter().find(|v| v.group.as_deref() == Some(MINDLINK_GROUP) && v.name == kind.label()).map(|v| v.id)
}

/// A booster's empty bursts loaded and one variation per kind it runs with that kind's Tech II
/// mindlink.
pub fn mindlink_variations(dogma: &Dogma, fit: &mut Fit) {
    let kinds = burst_kinds(dogma, fit);
    for kind in &kinds {
        let mut loaded: Vec<crate::ids::TypeId> = fit.modules.iter().filter(|m| burst_kind(dogma, m.type_id) == Some(*kind)).filter_map(|m| m.charge).collect();
        for m in fit.modules.iter_mut().filter(|m| m.charge.is_none() && burst_kind(dogma, m.type_id) == Some(*kind)) {
            let fits_in = crate::fleet::burst_charges(dogma, m.type_id);
            let next = default_charges(*kind).iter().filter_map(|n| dogma.type_named(n)).map(|t| t.id).find(|c| fits_in.contains(c) && !loaded.contains(c));
            if let Some(c) = next {
                m.charge = Some(c);
                loaded.push(c);
            }
        }
    }
    let slot_ten = |id: &crate::ids::TypeId| dogma.type_(*id).and_then(|t| t.attr(crate::dogma::attr::IMPLANTNESS)) == Some(10.0);
    for kind in kinds {
        let Some(link) = dogma.type_named(kind.mindlink_name()) else { continue };
        if mindlink_variation(fit, kind).is_some() {
            continue;
        }
        let mut implants: Vec<crate::ids::TypeId> = fit.implants.iter().copied().filter(|i| !slot_ten(i)).collect();
        implants.push(link.id);
        let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
        fit.variations.push(crate::variations::Variation {
            id,
            name: kind.label().to_owned(),
            kind: crate::variations::VariationKind::Implants,
            refit_key: None,
            ops: vec![crate::variations::RefitOp::SetImplants(implants)],
            group: Some(MINDLINK_GROUP.to_owned()),
        });
    }
}

/// Every charge the fit's bursts take, so one booster stands for all of its kinds' boosts.
pub fn covered_charges(dogma: &Dogma, fit: &Fit) -> Vec<crate::ids::TypeId> {
    let mut out: Vec<crate::ids::TypeId> = Vec::new();
    for m in fit.modules.iter().filter(|m| burst_kind(dogma, m.type_id).is_some()) {
        for c in dogma.compatible_charges(m.type_id) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// The group a booster's burst swaps sit under.
pub const BURSTS_GROUP: &str = "Bursts";

/// The kinds a booster's hull strengthens, the ones it runs first; the ones it runs when its hull
/// strengthens none.
pub fn hull_kinds(dogma: &Dogma, fit: &Fit) -> Vec<BurstKind> {
    let mut kinds = burst_kinds(dogma, fit);
    let bonused: Vec<BurstKind> = BurstKind::COMBAT.into_iter().filter(|k| crate::fleet::hull_boosts(dogma, fit.ship, *k)).collect();
    if bonused.is_empty() {
        return kinds;
    }
    kinds.retain(|k| bonused.contains(k));
    let rest: Vec<BurstKind> = bonused.into_iter().filter(|k| !kinds.contains(k)).collect();
    kinds.extend(rest);
    kinds
}

fn mindlinked(dogma: &Dogma, fit: &Fit, kind: BurstKind) -> Vec<crate::ids::TypeId> {
    let slot_ten = |id: &crate::ids::TypeId| dogma.type_(*id).and_then(|t| t.attr(crate::dogma::attr::IMPLANTNESS)) == Some(10.0);
    let mut implants: Vec<crate::ids::TypeId> = fit.implants.iter().copied().filter(|i| !slot_ten(i)).collect();
    implants.extend(dogma.type_named(kind.mindlink_name()).map(|t| t.id));
    implants
}

/// The booster's bursts spread over every kind its hull strengthens, at least one each, so it
/// covers all their charges: the bursts it has keep their kind while that kind has more than its
/// share, the rest take the kinds missing, each loaded with a charge of its own. None when it
/// already runs every one.
fn spread_ops(dogma: &Dogma, fit: &Fit, kinds: &[BurstKind]) -> Option<Vec<crate::variations::RefitOp>> {
    use crate::variations::RefitOp;
    let bursts: Vec<&crate::fit::Module> = fit.modules.iter().filter(|m| burst_kind(dogma, m.type_id).is_some()).collect();
    let run = burst_kinds(dogma, fit);
    if kinds.iter().all(|k| run.contains(k)) || bursts.len() < kinds.len() {
        return None;
    }
    // Each kind's share of the bursts, the kinds first in `kinds` taking the odd ones.
    let share = |k: usize| bursts.len() / kinds.len() + usize::from(k < bursts.len() % kinds.len());
    let mut want: Vec<(BurstKind, usize)> = kinds.iter().enumerate().map(|(i, k)| (*k, share(i))).collect();
    let mut kind_at: Vec<Option<BurstKind>> = bursts.iter().map(|_| None).collect();
    for (at, m) in bursts.iter().enumerate() {
        let Some(k) = burst_kind(dogma, m.type_id) else { continue };
        if let Some((_, left)) = want.iter_mut().find(|(w, left)| *w == k && *left > 0) {
            *left -= 1;
            kind_at[at] = Some(k);
        }
    }
    for slot in kind_at.iter_mut().filter(|k| k.is_none()) {
        if let Some((k, left)) = want.iter_mut().find(|(_, left)| *left > 0) {
            *left -= 1;
            *slot = Some(*k);
        }
    }
    // Which run at once: one of each kind first, then the rest, as many as the hull lets.
    let mut order: Vec<usize> = Vec::new();
    for round in 0..bursts.len() {
        for k in kinds {
            if let Some(at) = kind_at.iter().enumerate().filter(|(_, x)| **x == Some(*k)).map(|(i, _)| i).nth(round) {
                order.push(at);
            }
        }
    }
    let build = |running: usize| {
        let mut ops = Vec::new();
        for k in kinds {
            let Some(module) = dogma.type_named(k.module_name()).map(|t| t.id) else { continue };
            let mut charges = default_charges(*k).iter().filter_map(|n| dogma.type_named(n)).map(|t| t.id).filter(|c| dogma.takes_charge(module, *c));
            for (at, m) in bursts.iter().enumerate().filter(|(at, _)| kind_at[*at] == Some(*k)) {
                if burst_kind(dogma, m.type_id) != Some(*k) {
                    ops.push(RefitOp::Replace { slot: m.slot, index: m.index, with: Some(module) });
                }
                ops.push(RefitOp::SetCharge { slot: m.slot, index: m.index, charge: charges.next() });
                let on = order.iter().position(|o| *o == at).is_some_and(|p| p < running);
                ops.push(RefitOp::SetState { slot: m.slot, index: m.index, state: if on { crate::fit::State::Active } else { crate::fit::State::Offline } });
            }
        }
        ops
    };
    let skills = crate::skills::SkillSet::all_v();
    let limited = |ops: &Vec<RefitOp>| {
        let mut x = fit.clone();
        x.variations = vec![crate::variations::Variation { id: u32::MAX, ops: ops.clone(), ..Default::default() }];
        x.active_variations = vec![u32::MAX];
        let flown = crate::variations::resolve(dogma, &x);
        let stats = crate::stats::compute(dogma, &flown, &skills, &crate::calc::Env::default());
        crate::warnings::check(dogma, &flown, &stats, &[]).iter().any(|w| matches!(w.kind, crate::warnings::WarningKind::GroupLimit { .. }))
    };
    (1..=bursts.len()).rev().map(build).find(|ops| !limited(ops)).or_else(|| Some(build(1)))
}

fn kinds_label(kinds: &[BurstKind]) -> String {
    kinds.iter().map(|k| k.label()).collect::<Vec<_>>().join(" + ")
}

/// The kinds a booster can cover: its hull's, where it has a burst for each of them or the room
/// to spread to them; else only the ones it runs.
pub fn served_kinds(dogma: &Dogma, fit: &Fit) -> Vec<BurstKind> {
    let kinds = hull_kinds(dogma, fit);
    let run = burst_kinds(dogma, fit);
    let bursts = fit.modules.iter().filter(|m| burst_kind(dogma, m.type_id).is_some()).count();
    if kinds.iter().all(|k| run.contains(k)) || bursts >= kinds.len() {
        kinds
    } else {
        run
    }
}

/// The variation a booster flies with the kind's mindlink: its bursts spread over its hull's
/// kinds where the source does not run them all, else its own bursts.
pub fn booster_variation(dogma: &Dogma, fit: &Fit, kind: BurstKind) -> Option<u32> {
    let spread = format!("{}, {} mindlink", kinds_label(&served_kinds(dogma, fit)), kind.label());
    fit.variations.iter().find(|v| v.group.as_deref() == Some(BURSTS_GROUP) && v.name == spread).map(|v| v.id).or_else(|| mindlink_variation(fit, kind))
}

/// A booster's variations: for each kind its hull strengthens, that kind's mindlink, on its bursts
/// spread over all of them where the source fit runs fewer.
pub fn booster_variations(dogma: &Dogma, fit: &mut Fit) {
    let kinds = served_kinds(dogma, fit);
    let Some(ops) = spread_ops(dogma, fit, &kinds) else {
        mindlink_variations(dogma, fit);
        for kind in kinds {
            if mindlink_variation(fit, kind).is_some() {
                continue;
            }
            let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
            let implants = mindlinked(dogma, fit, kind);
            fit.variations.push(crate::variations::Variation { id, name: kind.label().to_owned(), kind: crate::variations::VariationKind::Implants, refit_key: None, ops: vec![crate::variations::RefitOp::SetImplants(implants)], group: Some(MINDLINK_GROUP.to_owned()) });
        }
        return;
    };
    // Its loaded charges first, as the spread starts from them.
    mindlink_variations(dogma, fit);
    fit.variations.retain(|v| v.group.as_deref() != Some(MINDLINK_GROUP));
    for kind in &kinds {
        let name = format!("{}, {} mindlink", kinds_label(&kinds), kind.label());
        if fit.variations.iter().any(|v| v.group.as_deref() == Some(BURSTS_GROUP) && v.name == name) {
            continue;
        }
        let mut with = ops.clone();
        with.push(crate::variations::RefitOp::SetImplants(mindlinked(dogma, fit, *kind)));
        let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
        fit.variations.push(crate::variations::Variation { id, name, kind: crate::variations::VariationKind::Custom, refit_key: None, ops: with, group: Some(BURSTS_GROUP.to_owned()) });
    }
}

/// A booster entry for each kind the doctrine's boosters cover, on the strongest of their hulls
/// for it, with the mindlink and every charge.
pub fn doctrine_boosts(dogma: &Dogma, doctrine: &Doctrine) -> Vec<crate::fit::BoostShortcut> {
    let fits: Vec<&Fit> = doctrine.fits.iter().filter(|f| f.role == FitRole::Command).filter_map(|f| f.fit.as_ref()).collect();
    crate::fleet::boosts_for(dogma, &fits)
}

/// The doctrine's command fits as boosters: each gets its mindlink and burst swap variations, and
/// every fit of the doctrine, the boosters too, gets a generic booster for each kind on the
/// strongest of their hulls for it.
pub fn project_boosters(dogma: &Dogma, doctrine: &mut Doctrine) {
    let boosters: Vec<usize> = doctrine.fits.iter().enumerate().filter(|(_, f)| f.role == FitRole::Command && f.fit.as_ref().is_some_and(|fit| !burst_kinds(dogma, fit).is_empty())).map(|(i, _)| i).collect();
    for &i in &boosters {
        if let Some(fit) = doctrine.fits[i].fit.as_mut() {
            booster_variations(dogma, fit);
        }
    }
    let boosts = doctrine_boosts(dogma, doctrine);
    let ids: Vec<String> = boosters.iter().map(|&i| doctrine.fits[i].fit_id.clone()).collect();
    for f in &mut doctrine.fits {
        let Some(fit) = f.fit.as_mut() else { continue };
        // Generic boosters, not the doctrine's own fits, and on the boosters too, so each has every
        // boost and not only its own.
        fit.projected_fits.retain(|p| !ids.contains(&p.fit_id));
        fit.boosts.clone_from(&boosts);
    }
}

/// A slot a doctrine's fit flies in, for a fleet built by hand.
pub fn slot_for(dogma: &Dogma, fit_id: &str, ship: crate::ids::TypeId) -> FleetSlot {
    FleetSlot { fit_id: fit_id.to_owned(), rule: crate::fleet::role_rule(dogma, ship), ..FleetSlot::default() }
}

/// A refit key's name for people: `prop:ab` reads "Afterburner".
pub fn refit_label(key: &str) -> String {
    if key.contains('|') {
        return key.split('|').map(refit_label).collect::<Vec<_>>().join(" · ");
    }
    if let Some(n) = forum::ewar_name_of(key) {
        return n.to_owned();
    }
    match key {
        "prop:ab" => "Afterburner".into(),
        "prop:mwd" => "Microwarpdrive".into(),
        "point:scram" => "Warp scrambler".into(),
        "point:disruptor" => "Warp disruptor".into(),
        k => k.split_once(':').map_or(k, |(_, v)| v).replace(['-', '_'], " "),
    }
}

/// A fit's name without what the tree above it already says: the alliance's tag, the doctrine's
/// name, the hull. `GSF FNI - DPS - V25.1` under HammerFleet (FNI) reads `DPS - V25.1`; a name
/// with nothing else left reads `Standard`.
pub fn short_fit_name(name: &str, hull: &str, said: &[&str]) -> String {
    let norm = |w: &str| w.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_ascii_lowercase();
    let mut known: Vec<String> = Vec::new();
    for s in said.iter().chain(std::iter::once(&hull)) {
        // Written as one word: TigersClaw for Tiger's Claw.
        let squashed: String = s.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
        if squashed.len() >= 3 {
            known.push(squashed);
        }
        for w in s.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '/') {
            let w = norm(w);
            if w.len() >= 2 {
                known.push(w.trim_end_matches('s').to_owned());
                known.push(w);
            }
        }
    }
    let words: Vec<&str> = hull.split_whitespace().collect();
    if words.len() > 1 {
        known.push(words.iter().filter_map(|w| w.chars().next()).collect::<String>().to_ascii_lowercase());
    }
    let dropped = |w: &str| {
        let w = norm(w);
        let stem = w.strip_suffix("fleet").filter(|s| s.len() >= 3).unwrap_or(&w);
        w.is_empty() || w == "fleet" || known.iter().any(|k| *k == w || *k == stem || (stem.len() >= 3 && k.starts_with(stem)))
    };
    let parts: Vec<String> = name
        .split(" - ")
        .flat_map(|p| p.split("- ").map(str::to_owned).collect::<Vec<_>>())
        .map(|p| {
            let p = p.trim();
            let p = p.strip_prefix("GSF-").or_else(|| p.strip_prefix("gsf-")).unwrap_or(p);
            p.split_whitespace().filter(|w| !dropped(w)).collect::<Vec<_>>().join(" ")
        })
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        "Standard".into()
    } else {
        parts.join(" - ")
    }
}

#[cfg(test)]
mod tests;
