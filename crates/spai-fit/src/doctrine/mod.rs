//! Doctrines: an alliance's fleets written down as the fits that fly together, nested by
//! alliance, then by kind (strategic, peacetime, capitals...), then by doctrine.
//!
//! Supplied doctrines come from the server and carry their fits inline. A pilot's own doctrines
//! name fits of their library instead, so a fit in one edits like any other; a copy of a supplied
//! doctrine moves its fits into the library, and a link shows the supplied one as it is now.

use serde::{Deserialize, Serialize};

use crate::dogma::Dogma;
use crate::fit::Fit;
use crate::fleet::{CountRule, Fleet, FleetSlot};
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
