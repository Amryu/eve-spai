//! The dogma calculation: every item of a fit as a node with its base attribute values, every
//! running effect's modifiers routed to the items they reach, and attribute values resolved on
//! demand with the operator order, stacking penalties and clamps EVE applies.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::dogma::{attr, Aggregate, Domain, Dogma, EffectCategory, Func, Op, SkillRef, TypeDef};
use crate::fit::{Fit, Slot, State};

use crate::ids::{AttrId, BuffId, CategoryId, EffectId, GroupId, TypeId, CHARACTER_TYPE};
use crate::skills::SkillSet;

mod explain;
pub(crate) mod patch;
mod value;

pub use explain::{Explanation, Step};
pub use value::{combine, fraction, penalty_weights, Modification, Source, PENALTY_FACTOR};

pub type ItemIdx = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Ship,
    Char,
    Skill,
    Module { slot: Slot, index: u8 },
    /// The hull's mode: a tactical destroyer's defense, propulsion or sharpshooter.
    Mode,
    Charge,
    Drone { count: u32 },
    Fighter { count: u32 },
    Implant,
    Booster,
    /// Another ship's module acting on this one.
    Projected { count: u32 },
    /// A system effect: wormhole or abyssal weather, an incursion, a beacon.
    Environment,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub type_id: TypeId,
    pub kind: ItemKind,
    pub state: State,
    pub category: CategoryId,
    pub group: GroupId,
    /// Where it sits, for `Location*` modifiers.
    pub location: Option<ItemIdx>,
    /// Who owns it, for `OwnerRequiredSkill` modifiers.
    pub owner: Option<ItemIdx>,
    pub charge: Option<ItemIdx>,
    /// For a charge, the module holding it.
    pub module: Option<ItemIdx>,
    /// Values standing in for the type's: skill levels, mutations.
    pub overrides: Vec<(AttrId, f64)>,
    pub side_effects: bool,
    /// A booster's side effects picked one by one; none follows `side_effects`.
    pub side_on: Option<Vec<EffectId>>,
    pub effects: Vec<EffectId>,
    pub required_skills: Vec<TypeId>,
}

impl Item {
    /// How many of a projected item there are; one for anything else.
    pub(crate) fn projected_count(&self) -> u32 {
        match self.kind {
            ItemKind::Projected { count } => u32::from(count).max(1),
            _ => 1,
        }
    }
}

/// A fleet buff in effect on the ship, the strongest of its kind already chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveBuff {
    pub id: BuffId,
    pub value: f64,
}

/// What surrounds the fit: the fleet's buffs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Env {
    pub buffs: Vec<ActiveBuff>,
}

impl Env {
    /// Adds a buff, keeping the strongest per id by the buff's aggregate mode.
    pub fn offer(&mut self, dogma: &Dogma, id: BuffId, value: f64) {
        let keep_max = dogma.buff(id).is_none_or(|b| b.aggregate == Aggregate::Max);
        match self.buffs.iter_mut().find(|b| b.id == id) {
            Some(b) => {
                if (keep_max && value > b.value) || (!keep_max && value < b.value) {
                    b.value = value;
                }
            }
            None => self.buffs.push(ActiveBuff { id, value }),
        }
    }
}

pub struct Calc<'d> {
    pub dogma: &'d Dogma,
    items: Vec<Item>,
    ship: ItemIdx,
    char: ItemIdx,
    /// Per item, per attribute, what modifies it.
    mods: Vec<HashMap<AttrId, Vec<Modification>>>,
    cache: RefCell<HashMap<(ItemIdx, AttrId), f64>>,
    visiting: RefCell<HashSet<(ItemIdx, AttrId)>>,
    /// Type ids the fit names that the data does not know; those items are left out.
    pub unknown_types: Vec<TypeId>,
    /// The incoming damage's shares, for a reactive hardener to adapt to.
    pub(crate) incoming: [f64; 4],
    /// Reactive hardeners kept at their even split.
    pub(crate) fixed: Vec<ItemIdx>,
    /// The first running reactive hardener, for damage taken over time.
    pub rah: Option<Rah>,
    /// Modules held below their state by a group limit, with the fit's bonuses on the limits.
    capped: Vec<Capped>,
    /// Modules offline in the fit whose group is already as full as its limit lets it be.
    limited_offline: Vec<(crate::fit::Slot, u8)>,
}

/// A running reactive hardener: the armor resonances it adapts against, its starting resistances,
/// how much it shifts a cycle (zero kept even) and how long a cycle lasts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rah {
    pub armor_before: [f64; 4],
    pub start: [f64; 4],
    pub shift: f64,
    pub cycle_s: f64,
}

/// Source item categories whose modifiers are never stacking-penalised.
const EXEMPT_CATEGORIES: [CategoryId; 6] = [6, 8, 16, 20, 32, 65];

/// Effects the data describes in a way that means nothing to a fit, left out whole.
const SKIPPED_EFFECTS: &[&str] = &["skillEffect"];

impl<'d> Calc<'d> {
    pub fn new(dogma: &'d Dogma, fit: &Fit, skills: &SkillSet, env: &Env) -> Self {
        let mut c = Calc {
            dogma,
            items: Vec::new(),
            ship: 0,
            char: 0,
            mods: Vec::new(),
            cache: RefCell::new(HashMap::new()),
            visiting: RefCell::new(HashSet::new()),
            unknown_types: Vec::new(),
            incoming: fit.damage_profile.weights(),
            fixed: Vec::new(),
            rah: None,
            capped: Vec::new(),
            limited_offline: Vec::new(),
        };
        // The group limits carry the fit's own bonuses (a Command Processor rig, a command ship's
        // role bonus), so they are read off a first pass before any module is held back by them.
        c.build(fit, skills, &[]);
        c.collect();
        let (capped, limited_offline) = c.group_caps(fit);
        if !capped.is_empty() {
            c.reset();
            c.build(fit, skills, &capped);
            c.collect();
        }
        c.capped = capped;
        c.limited_offline = limited_offline;
        c.buffs(env);
        patch::apply(&mut c);
        c
    }

    fn reset(&mut self) {
        self.items.clear();
        self.mods.clear();
        self.cache.borrow_mut().clear();
        self.visiting.borrow_mut().clear();
        self.unknown_types.clear();
        self.fixed.clear();
        self.rah = None;
        self.ship = 0;
        self.char = 0;
    }

    pub fn capped(&self) -> &[Capped] {
        &self.capped
    }

    pub fn limited_offline(&self) -> &[(crate::fit::Slot, u8)] {
        &self.limited_offline
    }

    /// `group_limits` on this pass's values: `maxGroupFitted`, `maxGroupOnline` and
    /// `maxGroupActive` as the fit modifies them. With them, the offline modules whose group is
    /// full at Online or Active, which are offline for want of room and no cause for a warning.
    fn group_caps(&self, fit: &Fit) -> (Vec<Capped>, Vec<(crate::fit::Slot, u8)>) {
        let online_attr = self.dogma.attr_named("maxGroupOnline").map(|a| a.id);
        let mut order: Vec<&crate::fit::Module> = fit.modules.iter().collect();
        order.sort_by_key(|m| (m.slot, m.index));
        let max_of = |m: &crate::fit::Module, a: Option<AttrId>| -> Option<u32> {
            let a = a?;
            let t = self.dogma.type_(m.type_id)?;
            t.attr(a)?;
            let i = self.items.iter().position(|i| i.kind == ItemKind::Module { slot: m.slot, index: m.index })?;
            Some(self.value(i, a).round()).filter(|v| *v > 0.0).map(|v| v as u32)
        };
        let limits = |m: &crate::fit::Module| {
            [
                (GroupLimit::Fitted, max_of(m, Some(attr::MAX_GROUP_FITTED)), State::Offline, None),
                (GroupLimit::Online, max_of(m, online_attr), State::Online, Some(State::Offline)),
                (GroupLimit::Active, max_of(m, Some(attr::MAX_GROUP_ACTIVE)), State::Active, Some(State::Online)),
            ]
        };
        let mut counts: HashMap<(crate::ids::GroupId, GroupLimit), u32> = HashMap::new();
        let mut capped = Vec::new();
        for m in &order {
            let Some(t) = self.dogma.type_(m.type_id) else { continue };
            let mut state = m.state;
            let mut hit: Option<(GroupLimit, u32)> = None;
            for (limit, cap, from, to) in limits(m) {
                let Some(cap) = cap else { continue };
                if state < from {
                    continue;
                }
                let n = counts.entry((t.group, limit)).or_insert(0);
                *n += 1;
                if *n > cap {
                    hit = Some((limit, cap));
                    state = to.unwrap_or(State::Offline);
                    if to.is_none() {
                        break;
                    }
                }
            }
            if let Some((limit, max)) = hit {
                capped.push(Capped { slot: m.slot, index: m.index, state: state.min(m.state), limit, max });
            }
        }
        let mut limited_offline = Vec::new();
        for m in order.iter().filter(|m| m.state == State::Offline) {
            let Some(t) = self.dogma.type_(m.type_id) else { continue };
            let full = limits(m).into_iter().filter(|(limit, _, _, _)| *limit != GroupLimit::Fitted).any(|(limit, cap, _, _)| cap.is_some_and(|cap| counts.get(&(t.group, limit)).copied().unwrap_or(0) >= cap));
            if full {
                limited_offline.push((m.slot, m.index));
            }
        }
        (capped, limited_offline)
    }

    fn item_for(&mut self, type_id: TypeId, kind: ItemKind, state: State) -> Option<ItemIdx> {
        let Some(t) = self.dogma.type_(type_id) else {
            self.unknown_types.push(type_id);
            return None;
        };
        let (category, group) = (t.category, t.group);
        let effects = t.effects.iter().map(|(e, _)| *e).collect();
        let required_skills = t.required_skills.iter().map(|(s, _)| *s).collect();
        self.items.push(Item {
            type_id,
            kind,
            state,
            category,
            group,
            location: None,
            owner: None,
            charge: None,
            module: None,
            overrides: Vec::new(),
            side_effects: false,
            side_on: None,
            effects,
            required_skills,
        });
        Some(self.items.len() - 1)
    }

    fn build(&mut self, fit: &Fit, skills: &SkillSet, capped: &[Capped]) {
        self.char = match self.item_for(CHARACTER_TYPE, ItemKind::Char, State::Online) {
            Some(i) => i,
            None => {
                self.unknown_types.clear();
                self.items.push(Item {
                    type_id: 0,
                    kind: ItemKind::Char,
                    state: State::Online,
                    category: 0,
                    group: 0,
                    location: None,
                    owner: None,
                    charge: None,
                    module: None,
                    overrides: Vec::new(),
                    side_effects: false,
                    side_on: None,
                    effects: Vec::new(),
                    required_skills: Vec::new(),
                });
                self.items.len() - 1
            }
        };
        self.ship = match self.item_for(fit.ship, ItemKind::Ship, State::Online) {
            Some(i) => i,
            None => {
                self.items.push(Item { kind: ItemKind::Ship, ..self.items[0].clone() });
                self.items.len() - 1
            }
        };
        self.items[self.ship].owner = Some(self.char);
        let (ship, char) = (self.ship, self.char);

        let skill_types: Vec<(TypeId, bool)> = self.dogma.types.iter().filter(|t| t.category == crate::ids::category::SKILL).map(|t| (t.id, t.published)).collect();
        for (s, published) in skill_types {
            // An unpublished skill cannot be trained, so "All V" leaves it untrained; it stays an
            // item because modules may still name it as required.
            let level = if published { f64::from(skills.level(s)) } else { 0.0 };
            if let Some(i) = self.item_for(s, ItemKind::Skill, State::Online) {
                self.items[i].owner = Some(char);
                self.items[i].location = Some(char);
                self.items[i].overrides.push((attr::SKILL_LEVEL, level));
            }
        }
        if let Some(mode) = self.dogma.mode_for(fit.ship, fit.mode) {
            if let Some(i) = self.item_for(mode, ItemKind::Mode, State::Online) {
                self.items[i].location = Some(ship);
                self.items[i].owner = Some(char);
            }
        }
        for m in &fit.modules {
            let state = capped.iter().find(|c| (c.slot, c.index) == (m.slot, m.index)).map_or(m.state, |c| c.state);
            let Some(i) = self.item_for(m.type_id, ItemKind::Module { slot: m.slot, index: m.index }, state) else { continue };
            if m.fixed {
                self.fixed.push(i);
            }
            self.items[i].location = Some(ship);
            self.items[i].owner = Some(char);
            if let Some(mu) = &m.mutation {
                self.items[i].overrides.extend(mu.attrs.iter().map(|(a, v)| (*a, f64::from(*v))));
            }
            // A charge the module cannot take stays out of it: the game would not load it.
            if let Some(c) = m.charge.filter(|c| self.dogma.takes_charge(m.type_id, *c)) {
                if let Some(ci) = self.item_for(c, ItemKind::Charge, state) {
                    self.items[ci].location = Some(ship);
                    self.items[ci].owner = Some(char);
                    self.items[ci].module = Some(i);
                    self.items[i].charge = Some(ci);
                }
            }
        }
        for d in &fit.drones {
            // The launched ones and the rest of the stack in the bay are two items: only the first
            // fights, both take bay room.
            let out = d.in_space();
            for (n, state) in [(out, State::Active), (d.count.saturating_sub(out), State::Offline)] {
                if n == 0 {
                    continue;
                }
                if let Some(i) = self.item_for(d.type_id, ItemKind::Drone { count: n }, state) {
                    self.items[i].location = Some(ship);
                    self.items[i].owner = Some(char);
                    if let Some(mu) = &d.mutation {
                        self.items[i].overrides.extend(mu.attrs.iter().map(|(a, v)| (*a, f64::from(*v))));
                    }
                }
            }
        }
        for f in &fit.fighters {
            let state = if f.active { State::Active } else { State::Offline };
            // A squadron holds no more than its type's size.
            let count = self.dogma.attr_named("fighterSquadronMaxSize").and_then(|a| self.dogma.type_(f.type_id).and_then(|t| t.attr(a.id))).filter(|m| *m > 0.0).map_or(f.count, |m| f.count.min(m as u32));
            if let Some(i) = self.item_for(f.type_id, ItemKind::Fighter { count }, state) {
                self.items[i].location = Some(ship);
                self.items[i].owner = Some(char);
            }
        }
        for &imp in fit.implants.iter().filter(|t| !fit.disabled.contains(t)) {
            if let Some(i) = self.item_for(imp, ItemKind::Implant, State::Online) {
                self.items[i].location = Some(char);
                self.items[i].owner = Some(char);
            }
        }
        for b in fit.boosters.iter().filter(|b| !fit.disabled.contains(&b.type_id)) {
            if let Some(i) = self.item_for(b.type_id, ItemKind::Booster, State::Online) {
                self.items[i].location = Some(char);
                self.items[i].owner = Some(char);
                self.items[i].side_effects = b.side_effects;
                self.items[i].side_on = b.side_on.clone();
            }
        }
        for p in &fit.projected {
            let beacon = self.dogma.type_(p.type_id).is_some_and(|t| t.group == crate::ids::group::EFFECT_BEACON);
            if beacon {
                // Its effects reach the ship as if fitted to the system; off is its only other state.
                if p.state == State::Offline {
                    continue;
                }
                if let Some(i) = self.item_for(p.type_id, ItemKind::Environment, State::Online) {
                    self.items[i].location = Some(ship);
                }
                continue;
            }
            let Some(i) = self.item_for(p.type_id, ItemKind::Projected { count: p.count }, p.state) else { continue };
            // Figured on its own fit already, its charge's effects included.
            if !p.values.is_empty() {
                self.items[i].overrides.extend(p.values.iter().copied());
                continue;
            }
            if let Some(c) = p.charge {
                if let Some(ci) = self.item_for(c, ItemKind::Charge, p.state) {
                    self.items[ci].module = Some(i);
                    self.items[i].charge = Some(ci);
                }
            }
        }
        self.mods = vec![HashMap::new(); self.items.len()];
    }

    /// The state an effect of this category needs its item to be in; `None` never runs. Passive
    /// effects need the item online: an offline plate holds no armour.
    fn required_state(category: EffectCategory) -> Option<State> {
        Some(match category {
            EffectCategory::Passive | EffectCategory::Online => State::Online,
            EffectCategory::Active | EffectCategory::Target | EffectCategory::Area => State::Active,
            EffectCategory::Overload => State::Overload,
            EffectCategory::Dungeon | EffectCategory::System => return None,
        })
    }

    fn collect(&mut self) {
        for src in 0..self.items.len() {
            let effects = self.items[src].effects.clone();
            for eid in effects {
                let Some(effect) = self.dogma.effect(eid) else { continue };
                if SKIPPED_EFFECTS.contains(&effect.name.as_str()) {
                    continue;
                }
                let environment = self.items[src].kind == ItemKind::Environment;
                let needed = match Self::required_state(effect.category) {
                    Some(s) => s,
                    // System effects are what a beacon is for.
                    None if environment && effect.category == EffectCategory::System => State::Online,
                    None => continue,
                };
                if self.items[src].state < needed {
                    continue;
                }
                if effect.usage_chance_attr.is_some() {
                    let on = match &self.items[src].side_on {
                        Some(list) => list.contains(&eid),
                        None => self.items[src].side_effects,
                    };
                    if !on {
                        continue;
                    }
                }
                let projected = matches!(self.items[src].kind, ItemKind::Projected { .. }) || self.items[src].module.is_some_and(|m| matches!(self.items[m].kind, ItemKind::Projected { .. }));
                for m in &effect.modifiers {
                    if projected && m.domain != Domain::Target {
                        continue;
                    }
                    if !projected && m.domain == Domain::Target {
                        continue;
                    }
                    // Several of one projected module each apply, penalised against each other.
                    let times = match self.items[src].kind {
                        ItemKind::Projected { count } if projected => count.max(1),
                        _ => 1,
                    };
                    for target in self.targets(src, m.domain, m.func, m.group, m.skill) {
                        for _ in 0..times {
                            self.add_mod(target, m.modified, m.op, src, m.modifying);
                        }
                    }
                }
            }
        }
    }

    fn root(&self, src: ItemIdx, domain: Domain) -> Option<ItemIdx> {
        match domain {
            Domain::Ship | Domain::Structure | Domain::Target => Some(self.ship),
            Domain::Char => Some(self.char),
            Domain::Item => Some(src),
            Domain::Other => self.items[src].charge.or(self.items[src].module),
        }
    }

    fn targets(&self, src: ItemIdx, domain: Domain, func: Func, group: Option<GroupId>, skill: Option<SkillRef>) -> Vec<ItemIdx> {
        let Some(root) = self.root(src, domain) else { return Vec::new() };
        let skill = match skill {
            Some(SkillRef::Own) => Some(self.items[src].type_id),
            Some(SkillRef::Of(s)) => Some(s),
            None => None,
        };
        match func {
            Func::Item => vec![root],
            Func::Location => (0..self.items.len()).filter(|&i| self.items[i].location == Some(root)).collect(),
            Func::LocationGroup => (0..self.items.len()).filter(|&i| self.items[i].location == Some(root) && Some(self.items[i].group) == group).collect(),
            Func::LocationRequiredSkill => (0..self.items.len())
                .filter(|&i| self.items[i].location == Some(root) && skill.is_some_and(|s| self.items[i].required_skills.contains(&s)))
                .collect(),
            Func::OwnerRequiredSkill => (0..self.items.len())
                .filter(|&i| self.items[i].owner == Some(root) && skill.is_some_and(|s| self.items[i].required_skills.contains(&s)))
                .collect(),
        }
    }

    fn penalised(&self, attr: AttrId, op: Op, src_category: CategoryId) -> bool {
        matches!(op, Op::PreMul | Op::PreDiv | Op::PostMul | Op::PostDiv | Op::PostPercent)
            && self.dogma.attr(attr).is_some_and(|a| !a.stackable)
            && !EXEMPT_CATEGORIES.contains(&src_category)
    }

    fn add_mod(&mut self, target: ItemIdx, attr: AttrId, op: Op, src: ItemIdx, src_attr: AttrId) {
        // System effects stand apart from the penalty, as implants do.
        let penalised = self.items[src].kind != ItemKind::Environment && self.penalised(attr, op, self.items[src].category);
        self.mods[target].entry(attr).or_default().push(Modification { op, source: value::Source::Attr(src, src_attr), penalised });
    }

    /// A modifier from outside the item tree, such as a fleet buff or a patch.
    pub(crate) fn add_const_mod(&mut self, target: ItemIdx, attr: AttrId, op: Op, value: f64, penalised: bool) {
        self.mods[target].entry(attr).or_default().push(Modification { op, source: value::Source::Const(value), penalised });
    }

    fn buffs(&mut self, env: &Env) {
        let ship = self.ship;
        for b in &env.buffs {
            let Some(def) = self.dogma.buff(b.id) else { continue };
            let penalised_for = |a: AttrId| matches!(def.op, Op::PreMul | Op::PreDiv | Op::PostMul | Op::PostDiv | Op::PostPercent) && self.dogma.attr(a).is_some_and(|d| !d.stackable);
            let mut targets: Vec<(ItemIdx, AttrId)> = def.item.iter().map(|a| (ship, *a)).collect();
            for a in &def.location {
                targets.extend((0..self.items.len()).filter(|&i| self.items[i].location == Some(ship)).map(|i| (i, *a)));
            }
            for (g, a) in &def.location_group {
                targets.extend((0..self.items.len()).filter(|&i| self.items[i].location == Some(ship) && self.items[i].group == *g).map(|i| (i, *a)));
            }
            for (s, a) in &def.location_skill {
                targets.extend((0..self.items.len()).filter(|&i| self.items[i].location == Some(ship) && self.items[i].required_skills.contains(s)).map(|i| (i, *a)));
            }
            for (i, a) in targets {
                let p = penalised_for(a);
                self.add_const_mod(i, a, def.op, b.value, p);
            }
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Items that require the skill, owned by the pilot (modules, charges, drones, fighters).
    pub fn requiring(&self, skill: TypeId) -> Vec<ItemIdx> {
        (0..self.items.len()).filter(|&i| self.items[i].owner == Some(self.char) && self.items[i].required_skills.contains(&skill)).collect()
    }

    pub fn skills(&self) -> impl Iterator<Item = (ItemIdx, &Item)> {
        self.items.iter().enumerate().filter(|(_, i)| i.kind == ItemKind::Skill)
    }

    pub fn subsystems(&self) -> impl Iterator<Item = (ItemIdx, &Item)> {
        self.items.iter().enumerate().filter(|(_, i)| matches!(i.kind, ItemKind::Module { slot: Slot::Subsystem, .. }))
    }

    pub(crate) fn item_effects(&self, item: ItemIdx) -> impl Iterator<Item = &'d crate::dogma::EffectDef> + '_ {
        self.items[item].effects.iter().filter_map(|e| self.dogma.effect(*e))
    }

    pub fn effect_names(&self, item: ItemIdx) -> impl Iterator<Item = &'d str> + '_ {
        self.items[item].effects.iter().filter_map(|e| self.dogma.effect(*e)).map(|e| e.name.as_str())
    }

    /// Forgets every value worked out, after a patch changed what they come from.
    pub(crate) fn forget_values(&self) {
        self.cache.borrow_mut().clear();
    }

    pub(crate) fn set_override(&mut self, item: ItemIdx, attr: AttrId, value: f64) {
        self.items[item].overrides.retain(|(a, _)| *a != attr);
        self.items[item].overrides.push((attr, value));
    }

    pub fn ship_idx(&self) -> ItemIdx {
        self.ship
    }

    pub fn char_idx(&self) -> ItemIdx {
        self.char
    }

    pub fn type_of(&self, item: ItemIdx) -> Option<&'d TypeDef> {
        self.dogma.type_(self.items[item].type_id)
    }

    pub fn module(&self, slot: Slot, index: u8) -> Option<ItemIdx> {
        self.items.iter().position(|i| i.kind == ItemKind::Module { slot, index })
    }

    /// What other ships project here: modules and drones.
    pub(crate) fn projected_items(&self) -> impl Iterator<Item = (ItemIdx, &Item)> {
        self.items.iter().enumerate().filter(|(_, it)| matches!(it.kind, ItemKind::Projected { .. }))
    }

    /// The ship's own items needing the skill.
    pub(crate) fn needing_on_ship(&self, skill: TypeId) -> Vec<ItemIdx> {
        (0..self.items.len()).filter(|&i| self.items[i].location == Some(self.ship) && !matches!(self.items[i].kind, ItemKind::Projected { .. }) && self.items[i].required_skills.contains(&skill)).collect()
    }

    pub fn modules(&self) -> impl Iterator<Item = (ItemIdx, &Item)> {
        self.items.iter().enumerate().filter(|(_, i)| matches!(i.kind, ItemKind::Module { .. }))
    }

    pub fn drones(&self) -> impl Iterator<Item = (ItemIdx, &Item)> {
        self.items.iter().enumerate().filter(|(_, i)| matches!(i.kind, ItemKind::Drone { .. }))
    }

    /// The value the item's type carries before any modifier, or the attribute's default.
    pub fn base(&self, item: ItemIdx, attr: AttrId) -> f64 {
        let it = &self.items[item];
        if let Some((_, v)) = it.overrides.iter().rev().find(|(a, _)| *a == attr) {
            return *v;
        }
        if let Some(v) = self.dogma.type_(it.type_id).and_then(|t| t.attr(attr)) {
            return f64::from(v);
        }
        self.dogma.attr(attr).map_or(0.0, |a| f64::from(a.default))
    }

    /// Whether the type carries the attribute at all (as opposed to the default standing in).
    pub fn has(&self, item: ItemIdx, attr: AttrId) -> bool {
        let it = &self.items[item];
        it.overrides.iter().any(|(a, _)| *a == attr) || self.dogma.type_(it.type_id).is_some_and(|t| t.attr(attr).is_some())
    }

    /// The attribute with every modifier applied.
    pub fn value(&self, item: ItemIdx, attr: AttrId) -> f64 {
        if let Some(v) = self.cache.borrow().get(&(item, attr)) {
            return *v;
        }
        let base = self.base(item, attr);
        if !self.visiting.borrow_mut().insert((item, attr)) {
            return base;
        }
        let def = self.dogma.attr(attr);
        let high_is_good = def.is_none_or(|a| a.high_is_good);
        let mut v = match self.mods[item].get(&attr) {
            Some(mods) if !mods.is_empty() => {
                let resolved: Vec<(Op, f64, bool)> = mods.iter().map(|m| (m.op, self.resolve(m.source), m.penalised)).collect();
                combine(base, high_is_good, &resolved)
            }
            _ => base,
        };
        if let Some(d) = def {
            if let Some(max) = d.max_attr {
                v = v.min(self.value(item, max));
            }
            if let Some(min) = d.min_attr {
                v = v.max(self.value(item, min));
            }
        }
        self.visiting.borrow_mut().remove(&(item, attr));
        self.cache.borrow_mut().insert((item, attr), v);
        v
    }

    pub fn ship(&self, attr: AttrId) -> f64 {
        self.value(self.ship, attr)
    }

    pub fn char(&self, attr: AttrId) -> f64 {
        self.value(self.char, attr)
    }

    /// What modifies the attribute on the item, resolved: `(op, value, penalised)`.
    pub fn modifiers_of(&self, item: ItemIdx, attr: AttrId) -> Vec<(Op, f64, bool)> {
        self.mods[item]
            .get(&attr)
            .map(|mods| mods.iter().map(|m| (m.op, self.resolve(m.source), m.penalised)).collect())
            .unwrap_or_default()
    }

    fn resolve(&self, source: value::Source) -> f64 {
        match source {
            value::Source::Attr(i, a) => self.value(i, a),
            value::Source::Const(c) => c,
            value::Source::PropBonus(i) => {
                let mass = self.ship(attr::MASS);
                if mass <= 0.0 {
                    0.0
                } else {
                    self.value(i, attr::SPEED_FACTOR) * self.value(i, attr::SPEED_BOOST_FACTOR) / mass
                }
            }
        }
    }

    /// A modifier whose value is computed on demand (see `Source`).
    pub(crate) fn add_source_mod(&mut self, target: ItemIdx, attr: AttrId, op: Op, source: value::Source, penalised: bool) {
        self.mods[target].entry(attr).or_default().push(Modification { op, source, penalised });
    }
}

/// Which limit of a module's group holds it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupLimit {
    Fitted,
    Online,
    Active,
}

/// A module held below the state it is set to by its group's limit (`maxGroupFitted`,
/// `maxGroupOnline`, `maxGroupActive`): the first ones in rack order keep theirs, as the game
/// refuses to fit, online or activate one more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capped {
    pub slot: crate::fit::Slot,
    pub index: u8,
    pub state: State,
    pub limit: GroupLimit,
    pub max: u32,
}

/// Puts the modules a group limit holds back offline in `fit`, as the game refuses to online or
/// run one more: `capped` from the fit as shown (`shown`, variations applied). The module the pilot
/// last set, `keep`, keeps its state; an earlier one of its group yields instead. A module the base
/// fit does not hold as shown is left alone. True when something changed.
/// The one to yield is the last of its group in rack order, the one the game would hold back.
pub fn yield_to_limits(dogma: &crate::Dogma, fit: &mut Fit, shown: &Fit, capped: &[Capped], keep: Option<(crate::fit::Slot, u8)>) -> bool {
    let mut changed = false;
    let mut order: Vec<&crate::fit::Module> = shown.modules.iter().collect();
    order.sort_by_key(|m| (m.slot, m.index));
    let group_of = |m: &crate::fit::Module| dogma.type_(m.type_id).map(|t| t.group);
    for c in capped.iter().filter(|c| c.limit != GroupLimit::Fitted) {
        let Some(over) = shown.module(c.slot, c.index) else { continue };
        let victim = if keep == Some((c.slot, c.index)) {
            let group = group_of(over);
            order.iter().rev().copied().find(|m| (m.slot, m.index) != (c.slot, c.index) && group_of(m) == group && m.state > State::Offline && fit.module(m.slot, m.index).is_some_and(|b| b.state > State::Offline))
        } else {
            Some(over)
        };
        let Some(v) = victim else { continue };
        if let Some(b) = fit.module_mut(v.slot, v.index).filter(|b| b.type_id == v.type_id && b.state > State::Offline) {
            b.state = State::Offline;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
