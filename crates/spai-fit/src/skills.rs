//! A pilot's skill levels, and what a fit asks for that the pilot lacks.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::dogma::{attr, Dogma};
use crate::fit::Fit;
use crate::ids::TypeId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillSet {
    pub levels: HashMap<TypeId, u8>,
    /// The level of every skill not listed: 5 for the "all V" pilot, 0 for a real one.
    pub fallback: u8,
}

impl Default for SkillSet {
    fn default() -> Self {
        SkillSet::all_v()
    }
}

impl SkillSet {
    pub fn all_v() -> Self {
        SkillSet { levels: HashMap::new(), fallback: 5 }
    }

    pub fn none() -> Self {
        SkillSet { levels: HashMap::new(), fallback: 0 }
    }

    /// From ESI's `/characters/{id}/skills/`: every trained skill at its active level.
    pub fn from_esi(skills: impl IntoIterator<Item = (TypeId, u8)>) -> Self {
        SkillSet { levels: skills.into_iter().collect(), fallback: 0 }
    }

    pub fn level(&self, skill: TypeId) -> u8 {
        self.levels.get(&skill).copied().unwrap_or(self.fallback).min(5)
    }

    pub fn set(&mut self, skill: TypeId, level: u8) {
        self.levels.insert(skill, level.min(5));
    }
}

/// A skill the fit needs above what the pilot has.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MissingSkill {
    pub skill: TypeId,
    pub needed: u8,
    pub have: u8,
    /// The item that asks for it.
    pub for_type: TypeId,
}

/// Every skill the fit's hull, modules, charges, drones, fighters, implants and boosters need
/// above the pilot's level, including the skills those skills need in turn. Sorted, one entry
/// per `(skill, for_type)`.
pub fn missing(fit: &Fit, dogma: &Dogma, skills: &SkillSet) -> Vec<MissingSkill> {
    let mut items: Vec<TypeId> = vec![fit.ship];
    items.extend(fit.modules.iter().flat_map(|m| std::iter::once(m.type_id).chain(m.charge)));
    items.extend(fit.drones.iter().map(|d| d.type_id));
    items.extend(fit.fighters.iter().map(|f| f.type_id));
    items.extend(fit.implants.iter().copied());
    items.extend(fit.boosters.iter().map(|b| b.type_id));
    items.sort_unstable();
    items.dedup();
    let mut out = Vec::new();
    for item in items {
        let mut stack: Vec<(TypeId, TypeId)> = dogma.type_(item).map(|t| t.required_skills.iter().map(|(s, _)| (item, *s)).collect()).unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        while let Some((of, skill)) = stack.pop() {
            if !seen.insert(skill) {
                continue;
            }
            let Some(needed) = dogma.type_(of).and_then(|t| t.required_skills.iter().find(|(s, _)| *s == skill)).map(|(_, l)| *l) else {
                continue;
            };
            let have = skills.level(skill);
            if have < needed {
                out.push(MissingSkill { skill, needed, have, for_type: item });
            }
            if let Some(s) = dogma.type_(skill) {
                stack.extend(s.required_skills.iter().map(|(p, _)| (skill, *p)));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Where in a fit a skill is asked for: by what flies, by the drones, by what sits in the hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    Fit,
    Drones,
    Cargo,
}

/// A skill a fit asks for, at the level it asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Need {
    pub skill: TypeId,
    pub level: u8,
    pub scope: Scope,
}

const FITTING_ATTRS: [crate::ids::AttrId; 6] = [attr::CPU, attr::POWER, attr::CPU_OUTPUT, attr::POWER_OUTPUT, attr::UPGRADE_CAPACITY, attr::UPGRADE_COST];
/// `skillTimeConstant`: how many times a rank-one skill's points a level takes.
const SKILL_RANK: crate::ids::AttrId = 275;
/// Skill points of a rank-one skill at each level.
const SP: [f64; 6] = [0.0, 250.0, 1_415.0, 8_000.0, 45_255.0, 256_000.0];

/// The skills whose own effects change what a fit has or takes in CPU, powergrid or calibration.
pub fn fitting_skills(dogma: &Dogma) -> Vec<TypeId> {
    dogma
        .types
        .iter()
        .filter(|t| t.category == crate::ids::category::SKILL && t.published)
        .filter(|t| t.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).any(|e| e.modifiers.iter().any(|m| FITTING_ATTRS.contains(&m.modified))))
        .map(|t| t.id)
        .collect()
}

/// The skill points from nothing to `level`.
pub fn skill_points(dogma: &Dogma, skill: TypeId, level: u8) -> f64 {
    let rank = dogma.type_(skill).and_then(|t| t.attr(SKILL_RANK)).unwrap_or(1.0) as f64;
    SP[usize::from(level.min(5))] * rank
}

/// What the items ask for, with `deep` their prerequisites too, each skill once at the highest
/// level asked.
fn needs_of(dogma: &Dogma, items: impl IntoIterator<Item = TypeId>, deep: bool) -> Vec<(TypeId, u8)> {
    let mut out: Vec<(TypeId, u8)> = Vec::new();
    let mut stack: Vec<(TypeId, u8)> = items.into_iter().filter_map(|i| dogma.type_(i)).flat_map(|t| t.required_skills.iter().copied()).collect();
    while let Some((skill, level)) = stack.pop() {
        match out.iter_mut().find(|(s, _)| *s == skill) {
            Some(e) if e.1 >= level => continue,
            Some(e) => e.1 = level,
            None => out.push((skill, level)),
        }
        if let Some(t) = dogma.type_(skill).filter(|_| deep) {
            stack.extend(t.required_skills.iter().copied());
        }
    }
    out
}

fn scope_items(fit: &Fit, scope: Scope) -> Vec<TypeId> {
    match scope {
        Scope::Fit => {
            let mut v = vec![fit.ship];
            v.extend(fit.mode);
            v.extend(fit.modules.iter().flat_map(|m| std::iter::once(m.type_id).chain(m.charge)));
            v.extend(fit.fighters.iter().map(|f| f.type_id));
            v.extend(fit.implants.iter().copied());
            v.extend(fit.boosters.iter().map(|b| b.type_id));
            v
        }
        Scope::Drones => fit.drones.iter().map(|d| d.type_id).collect(),
        Scope::Cargo => fit.cargo.iter().map(|c| c.type_id).collect(),
    }
}

/// Every skill the fit's items ask for themselves, by where it is asked. Fitting skills are
/// left out; a skill the fit itself asks for is not asked again by its drones or hold, nor one
/// the drones ask for by the hold, unless at a higher level.
pub fn required(dogma: &Dogma, fit: &Fit) -> Vec<Need> {
    required_given(dogma, fit, &fitting_skills(dogma))
}

/// [`required`] with the fitting skills found once beforehand.
pub fn required_given(dogma: &Dogma, fit: &Fit, fitting: &[TypeId]) -> Vec<Need> {
    let mut out: Vec<Need> = Vec::new();
    for scope in [Scope::Fit, Scope::Drones, Scope::Cargo] {
        for (skill, level) in needs_of(dogma, scope_items(fit, scope), false) {
            if fitting.contains(&skill) || out.iter().any(|n| n.skill == skill && n.level >= level) {
                continue;
            }
            out.push(Need { skill, level, scope });
        }
    }
    out.sort_by_key(|n| (n.scope, n.skill));
    out
}

/// Raises `skill` to `level` and its prerequisites to theirs; the skill points that took.
fn raise(dogma: &Dogma, set: &mut SkillSet, skill: TypeId, level: u8) -> f64 {
    let mut spent = 0.0;
    let mut stack = vec![(skill, level)];
    while let Some((s, l)) = stack.pop() {
        let have = set.level(s);
        if have >= l {
            continue;
        }
        spent += skill_points(dogma, s, l) - skill_points(dogma, s, have);
        set.set(s, l);
        if let Some(t) = dogma.type_(s) {
            stack.extend(t.required_skills.iter().copied());
        }
    }
    spent
}

/// The least fitting skills, by skill points, that make the fit fit with only what it requires
/// trained besides: each is the skill and the level. Empty when it fits as it is; as far as the
/// skills reach when even all of them at V do not make it fit. `fit` has its variations applied.
pub fn fitting_minimum(dogma: &Dogma, fit: &Fit) -> Vec<(TypeId, u8)> {
    let env = crate::calc::Env::default();
    let over = |set: &SkillSet| {
        let calc = crate::calc::Calc::new(dogma, fit, set, &env);
        let f = crate::stats::fitting(&calc, fit);
        let past = |used: f64, total: f64| if total > 0.0 { (used / total - 1.0).max(0.0) } else { used.max(0.0) };
        past(f.cpu_used, f.cpu_total) + past(f.power_used, f.power_total)
    };
    let mut base = SkillSet::none();
    for (skill, level) in needs_of(dogma, [Scope::Fit, Scope::Drones, Scope::Cargo].into_iter().flat_map(|s| scope_items(fit, s)), true) {
        raise(dogma, &mut base, skill, level);
    }
    let start = over(&base);
    if start <= 1e-9 {
        return Vec::new();
    }
    // Only those that move this fit at all.
    let candidates: Vec<TypeId> = fitting_skills(dogma)
        .into_iter()
        .filter(|s| {
            let mut t = base.clone();
            raise(dogma, &mut t, *s, 5);
            over(&t) < start - 1e-9
        })
        .collect();
    let mut set = base.clone();
    let mut now = start;
    while now > 1e-9 {
        let mut best: Option<(f64, SkillSet, f64)> = None;
        for s in &candidates {
            let level = set.level(*s);
            if level >= 5 {
                continue;
            }
            let mut t = set.clone();
            let cost = raise(dogma, &mut t, *s, level + 1).max(1.0);
            let after = over(&t);
            let gain = (now - after) / cost;
            if gain > 0.0 && best.as_ref().is_none_or(|(g, _, _)| gain > *g) {
                best = Some((gain, t, after));
            }
        }
        let Some((_, t, after)) = best else { break };
        set = t;
        now = after;
    }
    // What a later level made unneeded comes down again, prerequisites kept whole.
    let fits_now = now <= 1e-9;
    let mut raised: Vec<TypeId> = set.levels.iter().filter(|(s, l)| **l > base.level(**s)).map(|(s, _)| *s).collect();
    raised.sort_by(|a, b| skill_points(dogma, *b, set.level(*b)).total_cmp(&skill_points(dogma, *a, set.level(*a))));
    if fits_now {
        for s in &raised {
            while set.level(*s) > base.level(*s) {
                let mut t = set.clone();
                t.set(*s, set.level(*s) - 1);
                let whole = t.levels.iter().all(|(k, l)| *l == 0 || dogma.type_(*k).is_none_or(|ty| ty.required_skills.iter().all(|(p, pl)| t.level(*p) >= *pl)));
                if !whole || over(&t) > 1e-9 {
                    break;
                }
                set = t;
            }
        }
    }
    let mut out: Vec<(TypeId, u8)> = raised.into_iter().filter(|s| set.level(*s) > base.level(*s)).map(|s| (s, set.level(s))).collect();
    out.sort_unstable();
    out
}

/// The least a pilot trains to fly the fit: everything it requires, and the least fitting skills
/// that make it fit. `fit` has its variations applied.
pub fn minimum(dogma: &Dogma, fit: &Fit) -> Vec<(TypeId, u8)> {
    let mut out: Vec<(TypeId, u8)> = required(dogma, fit).into_iter().map(|n| (n.skill, n.level)).collect();
    for (s, l) in fitting_minimum(dogma, fit) {
        match out.iter_mut().find(|(x, _)| *x == s) {
            Some(e) => e.1 = e.1.max(l),
            None => out.push((s, l)),
        }
    }
    out.sort_unstable();
    out.dedup_by_key(|(s, _)| *s);
    out
}

/// Which skills go into a skill plan, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanOptions {
    pub required: bool,
    pub fitting: bool,
    /// The fit's milestones up to and with this many.
    pub milestones: usize,
    /// What flies the fit first and each milestone after, ship and weapon skills ahead in each;
    /// else in the order listed.
    pub sort: bool,
    pub skip_trained: bool,
    /// Every prerequisite as well, down to the bottom.
    pub full_tree: bool,
    /// Within each stage, the steps that train on the same two attributes together, so one remap
    /// serves a run of them.
    pub by_attributes: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions { required: true, fitting: true, milestones: usize::MAX, sort: true, skip_trained: false, full_tree: false, by_attributes: false }
    }
}

/// One of the five attributes skills train on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Attr {
    Charisma,
    Intelligence,
    Memory,
    Perception,
    Willpower,
}

impl Attr {
    pub const ALL: [Attr; 5] = [Attr::Charisma, Attr::Intelligence, Attr::Memory, Attr::Perception, Attr::Willpower];

    /// From the attribute id a skill's `primaryAttribute` and `secondaryAttribute` hold.
    pub fn from_id(id: f32) -> Option<Attr> {
        Some(match id.round() as i64 {
            164 => Attr::Charisma,
            165 => Attr::Intelligence,
            166 => Attr::Memory,
            167 => Attr::Perception,
            168 => Attr::Willpower,
            _ => return None,
        })
    }

    /// The implant attribute that adds to it.
    fn bonus_attr(self) -> crate::ids::AttrId {
        match self {
            Attr::Charisma => 175,
            Attr::Intelligence => 176,
            Attr::Memory => 177,
            Attr::Perception => 178,
            Attr::Willpower => 179,
        }
    }
}

/// A character's attributes as they train, implants and boosters included.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attributes(pub [f64; 5]);

impl Default for Attributes {
    /// An unmapped character without implants.
    fn default() -> Self {
        Attributes([20.0, 20.0, 20.0, 20.0, 20.0])
    }
}

impl Attributes {
    pub fn get(&self, a: Attr) -> f64 {
        self.0[a as usize]
    }

    pub fn plus(mut self, other: &Attributes) -> Self {
        for i in 0..5 {
            self.0[i] += other.0[i];
        }
        self
    }

    /// What implants add, read off their attribute bonuses.
    pub fn of_implants(dogma: &Dogma, implants: &[TypeId]) -> Attributes {
        let mut out = Attributes([0.0; 5]);
        for t in implants.iter().filter_map(|i| dogma.type_(*i)) {
            for a in Attr::ALL {
                out.0[a as usize] += t.attr(a.bonus_attr()).unwrap_or(0.0) as f64;
            }
        }
        out
    }
}

/// The two attributes a skill trains on.
pub fn skill_attributes(dogma: &Dogma, skill: TypeId) -> Option<(Attr, Attr)> {
    let t = dogma.type_(skill)?;
    Some((Attr::from_id(t.attr(180)?)?, Attr::from_id(t.attr(181)?)?))
}

/// Skill points a minute on those two attributes; an Alpha clone trains at half the rate.
pub fn sp_per_minute(attrs: &Attributes, primary: Attr, secondary: Attr, omega: bool) -> f64 {
    let rate = attrs.get(primary) + attrs.get(secondary) / 2.0;
    if omega { rate } else { rate / 2.0 }
}

/// How long one level takes to train from the one below it, in seconds.
pub fn step_seconds(dogma: &Dogma, skill: TypeId, level: u8, attrs: &Attributes, omega: bool) -> f64 {
    let sp = skill_points(dogma, skill, level) - skill_points(dogma, skill, level.saturating_sub(1));
    let (p, s) = skill_attributes(dogma, skill).unwrap_or((Attr::Intelligence, Attr::Memory));
    let rate = sp_per_minute(attrs, p, s, omega);
    if rate > 0.0 { sp / rate * 60.0 } else { f64::INFINITY }
}

/// Whether the hull's bonuses make its drones a weapon.
fn drone_hull(dogma: &Dogma, ship: TypeId) -> bool {
    dogma.traits_of(ship).is_some_and(|t| {
        t.role.iter().chain(&t.misc).chain(t.per_skill.iter().flat_map(|(_, b)| b)).any(|b| {
            let text = b.text.to_lowercase();
            text.contains("drone") && text.contains("damage")
        })
    })
}

/// How soon each skill the fit asks for should be trained: 0 for the hull, the high slots and,
/// on a drone hull, the drones; 1 the mid and low slots and the rigs; 2 drones elsewhere;
/// 3 the hold, implants and boosters. Prerequisites go with what needs them.
fn priorities(dogma: &Dogma, fit: &Fit) -> std::collections::HashMap<TypeId, u8> {
    use crate::fit::Slot;
    let drones = if drone_hull(dogma, fit.ship) { 0 } else { 2 };
    let mut items: Vec<(TypeId, u8)> = vec![(fit.ship, 0)];
    items.extend(fit.mode.map(|m| (m, 0)));
    for m in &fit.modules {
        let p = if matches!(m.slot, Slot::High | Slot::Subsystem) { 0 } else { 1 };
        items.push((m.type_id, p));
        items.extend(m.charge.map(|c| (c, p)));
    }
    items.extend(fit.drones.iter().map(|d| (d.type_id, drones)));
    items.extend(fit.fighters.iter().map(|f| (f.type_id, 0)));
    items.extend(fit.implants.iter().map(|i| (*i, 3)));
    items.extend(fit.boosters.iter().map(|b| (b.type_id, 3)));
    items.extend(fit.cargo.iter().map(|c| (c.type_id, 3)));
    let mut out = std::collections::HashMap::new();
    for (item, p) in items {
        for (skill, _) in needs_of(dogma, [item], true) {
            let e = out.entry(skill).or_insert(p);
            *e = (*e).min(p);
        }
    }
    out
}

/// A skill plan for the fit: one step per level, in an order it can be trained. `fitting` is
/// [`fitting_minimum`]'s answer, `pilot` whose training is skipped when asked. `fit` has its
/// variations applied.
pub fn plan(dogma: &Dogma, fit: &Fit, fitting: &[(TypeId, u8)], pilot: &SkillSet, opts: PlanOptions) -> Vec<(TypeId, u8)> {
    plan_staged(dogma, fit, fitting, pilot, opts).into_iter().map(|(s, l, _)| (s, l)).collect()
}

/// [`plan`] with each step's stage: 0 what flies the fit, then 1 for the first milestone and on.
pub fn plan_staged(dogma: &Dogma, fit: &Fit, fitting: &[(TypeId, u8)], pilot: &SkillSet, opts: PlanOptions) -> Vec<(TypeId, u8, usize)> {
    // Per skill, the level each stage wants: what flies the fit, then each milestone.
    let stages = 1 + fit.milestones.len().min(opts.milestones);
    let mut targets: Vec<(TypeId, Vec<u8>)> = Vec::new();
    fn add(targets: &mut Vec<(TypeId, Vec<u8>)>, stages: usize, skill: TypeId, stage: usize, level: u8) {
        let i = match targets.iter().position(|t| t.0 == skill) {
            Some(i) => i,
            None => {
                targets.push((skill, vec![0; stages]));
                targets.len() - 1
            }
        };
        let v = &mut targets[i].1;
        v[stage] = v[stage].max(level);
    }
    if opts.required {
        for n in required(dogma, fit) {
            add(&mut targets, stages, n.skill, 0, n.level);
        }
    }
    if opts.fitting {
        for (s, l) in fitting {
            add(&mut targets, stages, *s, 0, *l);
        }
    }
    for (i, m) in fit.milestones.iter().take(stages - 1).enumerate() {
        for (s, l) in &m.skills {
            add(&mut targets, stages, *s, i + 1, *l);
        }
    }
    if opts.full_tree {
        let mut i = 0;
        while i < targets.len() {
            let (skill, wants) = targets[i].clone();
            for (stage, level) in wants.iter().enumerate().filter(|(_, l)| **l > 0) {
                let _ = level;
                for (p, pl) in dogma.type_(skill).map(|t| t.required_skills.clone()).unwrap_or_default() {
                    add(&mut targets, stages, p, stage, pl);
                }
            }
            i += 1;
        }
    }
    let prio = priorities(dogma, fit);
    let name = |s: TypeId| dogma.type_(s).map_or_else(String::new, |t| t.name.clone());
    let mut steps: Vec<(TypeId, u8, (usize, u8, u8, String, usize))> = Vec::new();
    for (i, (skill, wants)) in targets.iter().enumerate() {
        let most = wants.iter().copied().max().unwrap_or(0);
        for level in 1..=most {
            if opts.skip_trained && pilot.level(*skill) >= level {
                continue;
            }
            // The first stage that wants this level; a milestone only adds to those before it.
            let stage = wants.iter().position(|w| *w >= level).unwrap_or(0);
            let key = if opts.sort { (stage, prio.get(skill).copied().unwrap_or(2), level, name(*skill), 0) } else { (stage, 0, 0, String::new(), i * 8 + usize::from(level)) };
            steps.push((*skill, level, key));
        }
    }
    steps.sort_by(|a, b| a.2.cmp(&b.2));
    if opts.by_attributes {
        // In each stage, each pair of attributes in the order it first comes up.
        let pair = |s: TypeId| skill_attributes(dogma, s);
        let mut order: Vec<(usize, Option<(Attr, Attr)>)> = Vec::new();
        for (s, _, key) in &steps {
            let k = (key.0, pair(*s));
            if !order.contains(&k) {
                order.push(k);
            }
        }
        steps.sort_by_key(|(s, _, key)| (key.0, order.iter().position(|k| *k == (key.0, pair(*s))).unwrap_or(0)));
    }
    // Whatever a step stands on, if it is in the plan, comes first.
    let wanted: Vec<(TypeId, u8)> = steps.iter().map(|(s, l, _)| (*s, *l)).collect();
    let mut out: Vec<(TypeId, u8, usize)> = Vec::new();
    // A step pulled ahead for a later one trains in that one's stage, so stages never interleave.
    fn place(dogma: &Dogma, step: (TypeId, u8), stage: usize, wanted: &[(TypeId, u8)], out: &mut Vec<(TypeId, u8, usize)>, depth: u8) {
        let (skill, level) = step;
        if out.iter().any(|x| x.0 == skill && x.1 == level) || depth > 40 {
            return;
        }
        if level > 1 && wanted.contains(&(skill, level - 1)) {
            place(dogma, (skill, level - 1), stage, wanted, out, depth + 1);
        }
        for (p, pl) in dogma.type_(skill).map(|t| t.required_skills.clone()).unwrap_or_default() {
            for l in 1..=pl {
                if wanted.contains(&(p, l)) {
                    place(dogma, (p, l), stage, wanted, out, depth + 1);
                }
            }
        }
        out.push((skill, level, stage));
    }
    for (skill, level, key) in &steps {
        place(dogma, (*skill, *level), key.0, &wanted, &mut out, 0);
    }
    out
}

/// The skills out of a pasted plan or list: a skill and a level a line, the level in digits or
/// roman numerals, `Skill 3` or `Skill III`; each skill once at its highest level. What reads as
/// no skill comes back as written.
pub fn parse_plan(dogma: &Dogma, text: &str) -> (Vec<(TypeId, u8)>, Vec<String>) {
    let (mut out, mut unknown): (Vec<(TypeId, u8)>, Vec<String>) = (Vec::new(), Vec::new());
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let parsed = line.rsplit_once(char::is_whitespace).and_then(|(name, level)| {
            let level = match level.trim() {
                "I" | "1" => 1,
                "II" | "2" => 2,
                "III" | "3" => 3,
                "IV" | "4" => 4,
                "V" | "5" => 5,
                _ => return None,
            };
            let t = dogma.type_named(name.trim())?;
            (t.category == crate::ids::category::SKILL).then_some((t.id, level))
        });
        match parsed {
            Some((s, l)) => match out.iter_mut().find(|(x, _)| *x == s) {
                Some(e) => e.1 = e.1.max(l),
                None => out.push((s, l)),
            },
            None => unknown.push(line.to_owned()),
        }
    }
    (out, unknown)
}

/// What a pilot has trained for the fit before milestone `index`: what flies it and every
/// milestone ahead of that one, each skill at its highest level. `fitting` is
/// [`fitting_minimum`]'s answer; `fit` has its variations applied.
pub fn before_milestone(dogma: &Dogma, fit: &Fit, fitting: &[(TypeId, u8)], index: usize) -> Vec<(TypeId, u8)> {
    let mut out: Vec<(TypeId, u8)> = required(dogma, fit).into_iter().map(|n| (n.skill, n.level)).collect();
    out.extend(fitting.iter().copied());
    out.extend(fit.milestones.iter().take(index).flat_map(|m| m.skills.iter().copied()));
    let mut merged: Vec<(TypeId, u8)> = Vec::new();
    for (s, l) in out {
        match merged.iter_mut().find(|(x, _)| *x == s) {
            Some(e) => e.1 = e.1.max(l),
            None => merged.push((s, l)),
        }
    }
    merged
}

/// Only what `skills` adds over `before`: a skill at a level `before` already reaches is dropped.
pub fn added_over(skills: &[(TypeId, u8)], before: &[(TypeId, u8)]) -> Vec<(TypeId, u8)> {
    skills.iter().copied().filter(|(s, l)| !before.iter().any(|(b, bl)| b == s && bl >= l)).collect()
}

/// The milestones of a plan written for the fit: `required`, then `recommended` on top, each
/// keeping only what it adds over what comes before, the computed minimum first. A milestone
/// left with nothing is dropped. `fit` has its variations applied.
pub fn milestones_over_minimum(dogma: &Dogma, fit: &Fit, required_skills: &[(TypeId, u8)], recommended: &[(TypeId, u8)], names: [&str; 2]) -> Vec<crate::fit::Milestone> {
    let mut before = before_milestone(dogma, fit, &fitting_minimum(dogma, fit), 0);
    let mut out = Vec::new();
    for (name, skills) in names.into_iter().zip([required_skills, recommended]) {
        let added = added_over(skills, &before);
        if !added.is_empty() {
            before.extend(added.iter().copied());
            out.push(crate::fit::Milestone { name: name.to_owned(), skills: added });
        }
    }
    out
}

/// A skill plan as the game imports it from the clipboard: a skill and a level a line.
pub fn plan_text(dogma: &Dogma, steps: &[(TypeId, u8)]) -> String {
    steps.iter().filter_map(|(s, l)| dogma.type_(*s).map(|t| format!("{} {l}\n", t.name))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dogma::test_data;
    use crate::fit::Slot;

    #[test]
    fn levels_fall_back_and_cap_at_five() {
        let mut s = SkillSet::all_v();
        assert_eq!(s.level(3329), 5);
        s.set(3329, 9);
        assert_eq!(s.level(3329), 5);
        let e = SkillSet::from_esi([(3329, 3)]);
        assert_eq!((e.level(3329), e.level(1)), (3, 0));
    }

    #[test]
    fn missing_skills_follow_prerequisites_and_name_the_item() {
        let mut d = test_data::small();
        d.types.push(crate::dogma::TypeDef {
            id: 3392,
            name: "Hull Upgrades".into(),
            group: 257,
            category: 16,
            required_skills: vec![(3380, 1)],
            ..d.type_(3329).unwrap().clone()
        });
        d.types.push(crate::dogma::TypeDef { id: 3380, name: "Mechanics".into(), required_skills: vec![], ..d.type_(3329).unwrap().clone() });
        let d = d.finish();
        let mut fit = Fit::new(587, "Rifter");
        fit.add_module(2048, Slot::Low);
        assert!(missing(&fit, &d, &SkillSet::all_v()).is_empty());
        let pilot = SkillSet::from_esi([(3329, 1), (3392, 2)]);
        let m = missing(&fit, &d, &pilot);
        assert_eq!(
            m,
            vec![
                MissingSkill { skill: 3380, needed: 1, have: 0, for_type: 2048 },
                MissingSkill { skill: 3392, needed: 4, have: 2, for_type: 2048 },
            ]
        );
    }

    /// The test data with two fitting skills: CPU Management (rank 1, +5% CPU a level) and a
    /// rank 8 skill taking 5% off every module's CPU a level; and a module too big for the Rifter.
    fn fitting_data() -> (Dogma, TypeId, TypeId, TypeId) {
        use crate::dogma::{EffectCategory, Func, Domain, Op};
        use crate::test_support::{attr_def, effect, modifier, type_def};
        const CPU_MGMT: TypeId = 3426;
        const SLIM: TypeId = 90_001;
        const FAT: TypeId = 90_002;
        const BONUS: crate::ids::AttrId = 424;
        let mut d = crate::test_support::dataset();
        d.attributes.push(attr_def(BONUS, "cpuOutputBonus2", true, true, 0.0));
        d.attributes.push(attr_def(SKILL_RANK, "skillTimeConstant", true, true, 1.0));
        d.effects.push(effect(9001, "skillLevelBonus", EffectCategory::Passive, vec![modifier(Domain::Item, Func::Item, BONUS, attr::SKILL_LEVEL, Op::PreMul)]));
        d.effects.push(effect(9002, "cpuOutputPostPercent", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::CPU_OUTPUT, BONUS, Op::PostPercent)]));
        d.effects.push(effect(9003, "moduleCpuPostPercent", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Location, attr::CPU, BONUS, Op::PostPercent)]));
        d.types.push(type_def(CPU_MGMT, "CPU Management", 1216, 16, &[(BONUS, 5.0), (SKILL_RANK, 1.0)], &[9001, 9002], &[]));
        d.types.push(type_def(SLIM, "Slim Fitting", 1216, 16, &[(BONUS, -5.0), (SKILL_RANK, 8.0)], &[9001, 9003], &[(CPU_MGMT, 2)]));
        d.types.push(type_def(FAT, "Fat Module", 60, 7, &[(attr::CPU, 0.0)], &[], &[(CPU_MGMT, 1), (crate::test_support::ids::SKILL_MF, 1)]));
        (d.finish(), CPU_MGMT, SLIM, FAT)
    }

    fn with_cpu(d: &Dogma, fat: TypeId, cpu: f32) -> (Dogma, Fit) {
        let mut d = d.clone();
        let t = d.types.iter_mut().find(|t| t.id == fat).unwrap();
        t.attrs.retain(|(a, _)| *a != attr::CPU);
        t.attrs.push((attr::CPU, cpu));
        let d = d.finish();
        let mut fit = Fit::new(crate::test_support::ids::SHIP, "Rifter");
        fit.add_module(fat, crate::fit::Slot::Low);
        (d, fit)
    }

    #[test]
    fn fitting_skills_are_those_that_change_cpu_powergrid_or_calibration() {
        let (d, cpu, slim, _) = fitting_data();
        let f = fitting_skills(&d);
        assert!(f.contains(&cpu) && f.contains(&slim));
        assert!(!f.contains(&crate::test_support::ids::SKILL_SPT));
        assert_eq!(skill_points(&d, slim, 2), 8.0 * 1_415.0);
    }

    #[test]
    fn the_least_fitting_skills_by_skill_points_make_the_fit_fit() {
        let (d, cpu, slim, fat) = fitting_data();
        let ship_cpu = d.type_(crate::test_support::ids::SHIP).unwrap().attr(attr::CPU_OUTPUT).unwrap();
        let (d1, fit) = with_cpu(&d, fat, ship_cpu * 0.9);
        assert!(fitting_minimum(&d1, &fit).is_empty(), "fits on what it requires");
        let (d2, fit) = with_cpu(&d, fat, ship_cpu * 1.08);
        assert_eq!(fitting_minimum(&d2, &fit), vec![(cpu, 2)]);
        // Against every pair of levels: no cheaper way to make it fit.
        for factor in [1.18, 1.3, 1.45] {
            let (dn, fit) = with_cpu(&d, fat, ship_cpu * factor);
            let fits = |set: &SkillSet| {
                let calc = crate::calc::Calc::new(&dn, &fit, set, &crate::calc::Env::default());
                let f = crate::stats::fitting(&calc, &fit);
                f.cpu_used <= f.cpu_total + 1e-9
            };
            let cost = |levels: &[(TypeId, u8)]| levels.iter().map(|(s, l)| skill_points(&dn, *s, *l)).sum::<f64>();
            let mut best = f64::INFINITY;
            for c in 1..=5u8 {
                for sl in 0..=5u8 {
                    if sl > 0 && c < 2 {
                        continue;
                    }
                    let mut set = SkillSet::from_esi([(crate::test_support::ids::SKILL_MF, 1), (cpu, c), (slim, sl)]);
                    set.fallback = 0;
                    if fits(&set) {
                        best = best.min(cost(&[(cpu, c), (slim, sl)]));
                    }
                }
            }
            let got = fitting_minimum(&dn, &fit);
            let mut levels = vec![(cpu, 1u8), (slim, 0)];
            for (s, l) in &got {
                levels.iter_mut().find(|(x, _)| x == s).unwrap().1 = *l;
            }
            assert_eq!(cost(&levels), best, "x{factor}: {got:?}");
        }
        let (d3, fit) = with_cpu(&d, fat, ship_cpu * 1.45);
        let m = fitting_minimum(&d3, &fit);
        assert!(m.contains(&(cpu, 5)) && m.iter().any(|(s, l)| *s == slim && *l >= 1), "past what CPU Management gives, the other comes in: {m:?}");
        let rec = minimum(&d3, &fit);
        assert!(rec.contains(&(cpu, 5)) && rec.contains(&(crate::test_support::ids::SKILL_MF, 1)), "required and fitting together: {rec:?}");
    }

    #[test]
    fn required_skills_leave_out_fitting_skills_and_split_drones_and_cargo() {
        use crate::test_support::ids;
        let (d, cpu, _, fat) = fitting_data();
        let mut fit = Fit::new(ids::SHIP, "Rifter");
        fit.add_module(fat, crate::fit::Slot::Low);
        fit.drones.push(crate::fit::DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None });
        fit.cargo.push(crate::fit::CargoItem { type_id: ids::MISSILE, quantity: 10, hold: None });
        fit.cargo.push(crate::fit::CargoItem { type_id: ids::DRONE, quantity: 1, hold: None });
        let r = required(&d, &fit);
        assert!(!r.iter().any(|n| n.skill == cpu), "no fitting skills");
        assert!(r.contains(&Need { skill: ids::SKILL_MF, level: 1, scope: Scope::Fit }));
        assert!(r.contains(&Need { skill: ids::SKILL_DRONES, level: 5, scope: Scope::Drones }));
        assert!(r.contains(&Need { skill: ids::SKILL_CRUISE, level: 1, scope: Scope::Cargo }));
        assert_eq!(r.iter().filter(|n| n.skill == ids::SKILL_DRONES).count(), 1, "the drone in the hold asks nothing the bay did not");
    }

    #[test]
    fn a_plan_trains_what_flies_the_fit_first_weapons_ahead_and_prerequisites_before() {
        use crate::test_support::ids;
        let (d, cpu, _, fat) = fitting_data();
        let mut d = d;
        d.types.iter_mut().find(|t| t.id == ids::SKILL_CRUISE_SPEC).unwrap().required_skills = vec![(ids::SKILL_CRUISE, 3)];
        let d = d.finish();
        let mut fit = Fit::new(ids::SHIP, "Rifter");
        fit.add_module(ids::LAUNCHER, crate::fit::Slot::High);
        fit.add_module(fat, crate::fit::Slot::Low);
        fit.drones.push(crate::fit::DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None });
        fit.cargo.push(crate::fit::CargoItem { type_id: ids::MISSILE, quantity: 10, hold: None });
        fit.milestones = vec![crate::fit::Milestone { name: "Recommended".into(), skills: vec![(ids::SKILL_MF, 4), (ids::SKILL_CRUISE_SPEC, 3)] }];
        let none = SkillSet::none();
        let p = plan(&d, &fit, &[(cpu, 2)], &none, PlanOptions::default());
        let at = |s: TypeId, l: u8| p.iter().position(|x| *x == (s, l)).unwrap_or_else(|| panic!("{s} {l} in {p:?}"));
        assert!(at(ids::SKILL_MF, 1) < at(cpu, 1), "the hull before the lows");
        assert!(at(cpu, 2) < at(ids::SKILL_DRONES, 1), "the lows before drones on a hull without drone bonuses");
        assert!(at(ids::SKILL_DRONES, 5) < at(ids::SKILL_MF, 2), "what flies it before the upgrades");
        assert!(at(ids::SKILL_MF, 2) < at(ids::SKILL_MF, 3) && at(ids::SKILL_MF, 3) < at(ids::SKILL_MF, 4));
        assert!(at(ids::SKILL_CRUISE, 1) < at(ids::SKILL_CRUISE_SPEC, 1), "a listed prerequisite comes first, wherever it sorts");
        assert!(!p.contains(&(ids::SKILL_CRUISE, 3)), "the tree only when asked");
        let tree = plan(&d, &fit, &[(cpu, 2)], &none, PlanOptions { full_tree: true, ..PlanOptions::default() });
        let at = |s: TypeId, l: u8| tree.iter().position(|x| *x == (s, l)).unwrap();
        assert!(at(ids::SKILL_CRUISE, 3) < at(ids::SKILL_CRUISE_SPEC, 1));
        let pilot = SkillSet::from_esi([(ids::SKILL_MF, 2)]);
        let left = plan(&d, &fit, &[], &pilot, PlanOptions { skip_trained: true, fitting: false, ..PlanOptions::default() });
        assert!(!left.iter().any(|(s, l)| *s == ids::SKILL_MF && *l <= 2) && left.contains(&(ids::SKILL_MF, 3)));
        let only_rec = plan(&d, &fit, &[], &none, PlanOptions { required: false, fitting: false, sort: false, ..PlanOptions::default() });
        assert_eq!(&only_rec[..4], &[(ids::SKILL_MF, 1), (ids::SKILL_MF, 2), (ids::SKILL_MF, 3), (ids::SKILL_MF, 4)], "as listed");
        assert_eq!(plan_text(&d, &only_rec[..2]), "Minmatar Frigate 1\nMinmatar Frigate 2\n");
        // A second milestone trains after the first, whatever it holds.
        fit.milestones.push(crate::fit::Milestone { name: "Guns".into(), skills: vec![(ids::SKILL_SPT, 2)] });
        let p = plan(&d, &fit, &[(cpu, 2)], &none, PlanOptions::default());
        let at = |s: TypeId, l: u8| p.iter().position(|x| *x == (s, l)).unwrap();
        assert!(at(ids::SKILL_MF, 4) < at(ids::SKILL_SPT, 1), "milestone by milestone");
        let first = plan(&d, &fit, &[(cpu, 2)], &none, PlanOptions { milestones: 1, ..PlanOptions::default() });
        assert!(!first.contains(&(ids::SKILL_SPT, 1)), "up to the first milestone only");
    }

    #[test]
    fn a_pasted_plan_reads_by_name_and_a_milestone_keeps_only_what_it_adds() {
        use crate::test_support::ids;
        let (d, cpu, _, fat) = fitting_data();
        let (got, unknown) = parse_plan(&d, "Minmatar Frigate 1\nminmatar frigate IV\n\nCPU Management 3\nNot A Skill 2\nRifter 1\nDrones x\n");
        assert_eq!(got, vec![(ids::SKILL_MF, 4), (cpu, 3)]);
        assert_eq!(unknown, vec!["Not A Skill 2", "Rifter 1", "Drones x"], "a hull is no skill");
        let mut fit = Fit::new(ids::SHIP, "Rifter");
        fit.add_module(fat, crate::fit::Slot::Low);
        fit.milestones = vec![crate::fit::Milestone { name: "Recommended".into(), skills: vec![(ids::SKILL_MF, 3)] }];
        let before = before_milestone(&d, &fit, &[(cpu, 2)], 1);
        assert!(before.contains(&(ids::SKILL_MF, 3)) && before.contains(&(cpu, 2)));
        assert_eq!(added_over(&got, &before), vec![(ids::SKILL_MF, 4), (cpu, 3)]);
        assert_eq!(added_over(&[(ids::SKILL_MF, 2), (cpu, 2), (ids::SKILL_DRONES, 1)], &before), vec![(ids::SKILL_DRONES, 1)]);
    }

    #[test]
    fn training_time_follows_the_attributes_and_alpha_trains_at_half() {
        use crate::test_support::ids;
        let mut d = crate::test_support::dataset();
        let t = d.types.iter_mut().find(|t| t.id == ids::SKILL_MF).unwrap();
        t.attrs.extend([(180, 167.0), (181, 168.0), (SKILL_RANK, 2.0)]);
        let d = d.finish();
        assert_eq!(skill_attributes(&d, ids::SKILL_MF), Some((Attr::Perception, Attr::Willpower)));
        let a = Attributes([20.0, 20.0, 20.0, 27.0, 21.0]);
        assert_eq!(sp_per_minute(&a, Attr::Perception, Attr::Willpower, true), 37.5);
        // Level II of a rank 2 skill: 2 × (1415 − 250) points at 37.5 a minute.
        assert!((step_seconds(&d, ids::SKILL_MF, 2, &a, true) - 2330.0 / 37.5 * 60.0).abs() < 1e-6);
        assert!((step_seconds(&d, ids::SKILL_MF, 2, &a, false) - 2.0 * step_seconds(&d, ids::SKILL_MF, 2, &a, true)).abs() < 1e-6);
        assert_eq!(Attributes::default().plus(&Attributes([0.0, 3.0, 0.0, 0.0, 0.0])).get(Attr::Intelligence), 23.0);
    }

    #[test]
    fn grouping_by_attributes_keeps_each_stage_apart_and_prerequisites_first() {
        use crate::test_support::ids;
        let mut d = crate::test_support::dataset();
        for (skill, p, s) in [(ids::SKILL_MF, 167.0, 168.0), (ids::SKILL_SPT, 167.0, 168.0), (ids::SKILL_DRONES, 166.0, 167.0), (ids::SKILL_CRUISE, 167.0, 168.0)] {
            d.types.iter_mut().find(|t| t.id == skill).unwrap().attrs.extend([(180, p), (181, s)]);
        }
        let d = d.finish();
        let mut fit = Fit::new(ids::SHIP, "Rifter");
        fit.add_module(ids::GUN, crate::fit::Slot::High);
        fit.drones.push(crate::fit::DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None });
        fit.cargo.push(crate::fit::CargoItem { type_id: ids::MISSILE, quantity: 10, hold: None });
        fit.milestones = vec![crate::fit::Milestone { name: "More".into(), skills: vec![(ids::SKILL_DRONES, 5), (ids::SKILL_MF, 3)] }];
        let steps = plan_staged(&d, &fit, &[], &SkillSet::none(), PlanOptions { by_attributes: true, ..PlanOptions::default() });
        let pairs: Vec<(usize, Option<(Attr, Attr)>)> = steps.iter().map(|(s, _, st)| (*st, skill_attributes(&d, *s))).collect();
        let mut runs = pairs.clone();
        runs.dedup();
        let mut distinct = pairs.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(runs.len(), distinct.len(), "each pair in one run per stage: {pairs:?}");
        assert!(steps.windows(2).all(|w| w[0].2 <= w[1].2), "stage by stage");
    }

    #[test]
    fn stages_stay_in_order_when_an_earlier_one_needs_a_prerequisite_a_later_one_lists() {
        use crate::test_support::ids;
        let mut d = crate::test_support::dataset();
        d.types.iter_mut().find(|t| t.id == ids::SKILL_CRUISE_SPEC).unwrap().required_skills = vec![(ids::SKILL_CRUISE, 3)];
        for (skill, p) in [(ids::SKILL_CRUISE, 165.0), (ids::SKILL_CRUISE_SPEC, 167.0), (ids::SKILL_MF, 166.0)] {
            d.types.iter_mut().find(|t| t.id == skill).unwrap().attrs.extend([(180, p), (181, 168.0)]);
        }
        let d = d.finish();
        let mut fit = Fit::new(ids::SHIP, "Rifter");
        fit.add_module(ids::LAUNCHER, crate::fit::Slot::High);
        fit.milestones = vec![
            crate::fit::Milestone { name: "First".into(), skills: vec![(ids::SKILL_MF, 3)] },
            crate::fit::Milestone { name: "Second".into(), skills: vec![(ids::SKILL_CRUISE, 4)] },
        ];
        for sort in [true, false] {
            for by_attributes in [true, false] {
                let steps = plan_staged(&d, &fit, &[], &SkillSet::none(), PlanOptions { sort, by_attributes, ..PlanOptions::default() });
                assert!(steps.windows(2).all(|w| w[0].2 <= w[1].2), "sort {sort}, attributes {by_attributes}: {steps:?}");
                let at = |s: TypeId, l: u8| steps.iter().position(|x| x.0 == s && x.1 == l).unwrap();
                assert!(at(ids::SKILL_CRUISE, 3) < at(ids::SKILL_CRUISE_SPEC, 1), "prerequisites still first: {steps:?}");
                assert_eq!(steps[at(ids::SKILL_CRUISE, 3)].2, 0, "pulled into the stage that needs it: {steps:?}");
                assert_eq!(steps[at(ids::SKILL_CRUISE, 4)].2, 2, "the level only the later milestone wants stays there: {steps:?}");
            }
        }
    }

    #[test]
    fn a_written_plan_becomes_milestones_that_only_add_to_the_minimum() {
        use crate::test_support::ids;
        let (d, cpu, _, fat) = fitting_data();
        let (d, fit) = with_cpu(&d, fat, d.type_(ids::SHIP).unwrap().attr(attr::CPU_OUTPUT).unwrap() * 1.08);
        // The hull asks Minmatar Frigate I and it takes CPU Management II to fit.
        let m = milestones_over_minimum(&d, &fit, &[(ids::SKILL_MF, 1), (cpu, 2), (ids::SKILL_SPT, 3)], &[(ids::SKILL_SPT, 3), (ids::SKILL_MF, 4), (ids::SKILL_DRONES, 1)], ["Required", "Recommended"]);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].skills, vec![(ids::SKILL_SPT, 3)], "what the minimum has is stripped");
        assert_eq!(m[1].skills, vec![(ids::SKILL_MF, 4), (ids::SKILL_DRONES, 1)], "and what the milestone before has");
        let none = milestones_over_minimum(&d, &fit, &[(ids::SKILL_MF, 1)], &[(cpu, 1)], ["Required", "Recommended"]);
        assert!(none.is_empty(), "nothing added, no milestone");
    }
}
