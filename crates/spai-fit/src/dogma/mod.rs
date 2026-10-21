//! The static data a fit is calculated from: types with their attribute values and effects,
//! the attributes' and effects' definitions, groups, market groups and fleet buffs. English names
//! only. Built by the baker from EVE's JSONL static-data zip and carried as a blob (`blob`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ids::{AttrId, BuffId, CategoryId, EffectId, GroupId, MarketGroupId, TypeId};

pub mod blob;

pub mod attr;

/// How a modifier changes the attribute, in the order they are applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Op {
    PreAssign,
    PreMul,
    PreDiv,
    ModAdd,
    ModSub,
    PostMul,
    PostDiv,
    PostPercent,
    PostAssign,
}

impl Op {
    /// The static data's `operation` number. 9 (skill level from skill points) has no fitting
    /// meaning and gives `None`.
    pub fn from_sde(n: i64) -> Option<Op> {
        Some(match n {
            -1 => Op::PreAssign,
            0 => Op::PreMul,
            1 => Op::PreDiv,
            2 => Op::ModAdd,
            3 => Op::ModSub,
            4 => Op::PostMul,
            5 => Op::PostDiv,
            6 => Op::PostPercent,
            7 => Op::PostAssign,
            _ => return None,
        })
    }

    /// The buff collections name the operation instead of numbering it.
    pub fn from_name(name: &str) -> Option<Op> {
        Some(match name {
            "PreAssignment" | "PreAssign" => Op::PreAssign,
            "PreMul" => Op::PreMul,
            "PreDiv" => Op::PreDiv,
            "ModAdd" => Op::ModAdd,
            "ModSub" => Op::ModSub,
            "PostMul" => Op::PostMul,
            "PostDiv" => Op::PostDiv,
            "PostPercent" => Op::PostPercent,
            "PostAssignment" | "PostAssign" => Op::PostAssign,
            _ => return None,
        })
    }
}

/// Whose attributes a modifier reaches, relative to the item carrying the effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Domain {
    /// The ship the item is fitted to.
    Ship,
    /// The pilot.
    Char,
    /// The item itself.
    Item,
    /// A module's charge, or a charge's module.
    Other,
    /// The ship the effect is projected on.
    Target,
    /// The structure the item is fitted to.
    Structure,
}

impl Domain {
    pub fn from_sde(name: &str) -> Option<Domain> {
        Some(match name {
            "shipID" => Domain::Ship,
            "charID" => Domain::Char,
            "itemID" => Domain::Item,
            "otherID" => Domain::Other,
            "targetID" | "target" => Domain::Target,
            "structureID" => Domain::Structure,
            _ => return None,
        })
    }
}

/// Which items in the domain a modifier picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Func {
    /// The domain item itself.
    Item,
    /// Everything located in the domain.
    Location,
    /// Everything located in the domain of the given group.
    LocationGroup,
    /// Everything located in the domain that requires the given skill.
    LocationRequiredSkill,
    /// Everything the domain owns that requires the given skill.
    OwnerRequiredSkill,
}

impl Func {
    /// `EffectStopper` changes no attribute and gives `None`.
    pub fn from_sde(name: &str) -> Option<Func> {
        Some(match name {
            "ItemModifier" => Func::Item,
            "LocationModifier" => Func::Location,
            "LocationGroupModifier" => Func::LocationGroup,
            "LocationRequiredSkillModifier" => Func::LocationRequiredSkill,
            "OwnerRequiredSkillModifier" => Func::OwnerRequiredSkill,
            _ => return None,
        })
    }
}

/// The skill a `*RequiredSkill` modifier filters by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillRef {
    /// The item carrying the effect (a skill's own effects say "whatever needs me").
    Own,
    Of(TypeId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub domain: Domain,
    pub func: Func,
    pub modified: AttrId,
    pub modifying: AttrId,
    pub op: Op,
    pub group: Option<GroupId>,
    pub skill: Option<SkillRef>,
}

/// `effectCategoryID`: when an effect runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectCategory {
    Passive,
    Active,
    Target,
    Area,
    Online,
    Overload,
    Dungeon,
    System,
}

impl EffectCategory {
    pub fn from_sde(n: i64) -> Option<EffectCategory> {
        Some(match n {
            0 => EffectCategory::Passive,
            1 => EffectCategory::Active,
            2 => EffectCategory::Target,
            3 => EffectCategory::Area,
            4 => EffectCategory::Online,
            5 => EffectCategory::Overload,
            6 => EffectCategory::Dungeon,
            7 => EffectCategory::System,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectDef {
    pub id: EffectId,
    pub name: String,
    pub category: EffectCategory,
    pub offensive: bool,
    pub assistance: bool,
    pub duration_attr: Option<AttrId>,
    pub range_attr: Option<AttrId>,
    pub falloff_attr: Option<AttrId>,
    pub tracking_attr: Option<AttrId>,
    pub discharge_attr: Option<AttrId>,
    /// Set on booster side effects, which only apply when the pilot opts in.
    pub usage_chance_attr: Option<AttrId>,
    pub resistance_attr: Option<AttrId>,
    pub modifiers: Vec<Modifier>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttrDef {
    pub id: AttrId,
    pub name: String,
    /// The pilot-facing name, empty when the attribute has none.
    pub display: String,
    /// False: identical modifiers of this attribute are stacking-penalised.
    pub stackable: bool,
    pub high_is_good: bool,
    pub default: f32,
    pub unit: Option<u8>,
    pub min_attr: Option<AttrId>,
    pub max_attr: Option<AttrId>,
    pub category: Option<u8>,
    pub published: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeDef {
    pub id: TypeId,
    pub name: String,
    pub group: GroupId,
    pub category: CategoryId,
    pub market_group: Option<MarketGroupId>,
    pub meta_group: Option<u8>,
    pub meta_level: Option<u8>,
    pub tech: Option<u8>,
    /// The Tech I item this one is a variant of, for the "variants" list.
    pub variation_parent: Option<TypeId>,
    pub published: bool,
    /// Sorted by attribute id. Mass, capacity, volume and radius are always present.
    pub attrs: Vec<(AttrId, f32)>,
    /// `(effect, is_default)`.
    pub effects: Vec<(EffectId, bool)>,
    pub required_skills: Vec<(TypeId, u8)>,
    /// Its volume packaged, where that differs: a ship folded up for a hold.
    #[serde(default)]
    pub packaged: Option<f32>,
}

impl TypeDef {
    pub fn attr(&self, id: AttrId) -> Option<f32> {
        self.attrs.binary_search_by_key(&id, |(a, _)| *a).ok().map(|i| self.attrs[i].1)
    }

    pub fn has_effect(&self, id: EffectId) -> bool {
        self.effects.iter().any(|(e, _)| *e == id)
    }

    pub fn mass(&self) -> f32 {
        self.attr(attr::MASS).unwrap_or(0.0)
    }

    pub fn volume(&self) -> f32 {
        self.attr(attr::VOLUME).unwrap_or(0.0)
    }

    /// The room it takes in a hold: packaged.
    pub fn cargo_volume(&self) -> f32 {
        self.packaged.unwrap_or_else(|| self.volume())
    }

    pub fn capacity(&self) -> f32 {
        self.attr(attr::CAPACITY).unwrap_or(0.0)
    }

    pub fn radius(&self) -> f32 {
        self.attr(attr::RADIUS).unwrap_or(0.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupDef {
    pub id: GroupId,
    pub name: String,
    pub category: CategoryId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CategoryDef {
    pub id: CategoryId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketGroup {
    pub id: MarketGroupId,
    pub name: String,
    pub parent: Option<MarketGroupId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetaGroup {
    pub id: u8,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: u8,
    pub name: String,
    /// The suffix shown after a value ("m", "%", "GJ").
    pub display: String,
}

/// How several boosters offering the same buff combine: only one applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregate {
    Max,
    Min,
}

/// A fleet buff (command bursts and the like): one operation over a set of attributes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuffDef {
    pub id: BuffId,
    pub name: String,
    pub aggregate: Aggregate,
    pub op: Op,
    pub item: Vec<AttrId>,
    pub location: Vec<AttrId>,
    pub location_group: Vec<(GroupId, AttrId)>,
    pub location_skill: Vec<(TypeId, AttrId)>,
}

/// One line of a ship's traits panel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bonus {
    pub amount: Option<f32>,
    pub unit: Option<u8>,
    /// The sentence after the amount, tags stripped.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeTraits {
    pub type_id: TypeId,
    pub role: Vec<Bonus>,
    pub misc: Vec<Bonus>,
    /// Per skill, in the order the panel shows them.
    pub per_skill: Vec<(TypeId, Vec<Bonus>)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dogma {
    pub sde_build: u32,
    pub types: Vec<TypeDef>,
    pub attributes: Vec<AttrDef>,
    pub effects: Vec<EffectDef>,
    pub groups: Vec<GroupDef>,
    pub categories: Vec<CategoryDef>,
    pub market_groups: Vec<MarketGroup>,
    pub meta_groups: Vec<MetaGroup>,
    pub units: Vec<UnitDef>,
    pub buffs: Vec<BuffDef>,
    pub traits: Vec<TypeTraits>,
    /// Abyssal mutaplasmids: what each may roll, and what it turns which items into.
    pub mutaplasmids: Vec<Mutaplasmid>,
    #[serde(skip)]
    pub(crate) index: Index,
}

/// A mutaplasmid: each attribute it rolls, between a lowest and highest multiplier of the item's
/// own value, and the abyssal item each item it takes becomes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mutaplasmid {
    pub id: TypeId,
    pub name: String,
    pub attrs: Vec<(AttrId, f32, f32)>,
    /// (the items it takes, what they become).
    pub mapping: Vec<(Vec<TypeId>, TypeId)>,
}

impl Mutaplasmid {
    /// What the item becomes under this mutaplasmid, if it takes it.
    pub fn result_for(&self, item: TypeId) -> Option<TypeId> {
        self.mapping.iter().find(|(from, _)| from.contains(&item)).map(|(_, to)| *to)
    }
}

impl Dogma {
    /// The mutaplasmids that take this item, by name.
    pub fn mutaplasmids_for(&self, item: TypeId) -> Vec<&Mutaplasmid> {
        let mut v: Vec<&Mutaplasmid> = self.mutaplasmids.iter().filter(|m| m.result_for(item).is_some()).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// The rolled items: abyssal versions, made by a mutaplasmid from a base item, so never picked
    /// as such.
    pub fn rolled_types(&self) -> std::collections::HashSet<TypeId> {
        let mut out: std::collections::HashSet<TypeId> = self.mutaplasmids.iter().flat_map(|m| m.mapping.iter().map(|(_, to)| *to)).collect();
        out.extend(self.types.iter().filter(|t| t.meta_group == Some(crate::ids::meta_group::ABYSSAL)).map(|t| t.id));
        out
    }

    /// The charge groups a module loads and the charge size it takes.
    pub fn charge_filter(&self, module: TypeId) -> (Vec<GroupId>, Option<f32>) {
        let Some(t) = self.type_(module) else { return (Vec::new(), None) };
        let groups = [attr::CHARGE_GROUP_1, attr::CHARGE_GROUP_2, attr::CHARGE_GROUP_3, attr::CHARGE_GROUP_4, attr::CHARGE_GROUP_5]
            .iter()
            .filter_map(|a| t.attr(*a))
            .filter(|g| *g > 0.0)
            .map(|g| g as GroupId)
            .collect();
        (groups, t.attr(attr::CHARGE_SIZE))
    }

    /// The charges a module can load, by name.
    pub fn compatible_charges(&self, module: TypeId) -> Vec<TypeId> {
        let (groups, size) = self.charge_filter(module);
        let capacity = self.type_(module).and_then(|t| t.attr(attr::CAPACITY)).filter(|c| *c > 0.0);
        let mut out: Vec<TypeId> = groups
            .iter()
            .flat_map(|g| self.types_in_group(*g).iter().copied())
            .filter(|c| {
                self.type_(*c).is_some_and(|t| {
                    // A sized module takes its own size only: Cap Booster 25 has no size at all and
                    // does not go into an ancillary shield booster. Nor does anything bigger than it holds.
                    let sized = size.is_none() || t.attr(attr::CHARGE_SIZE) == size;
                    let fits = capacity.is_none_or(|cap| t.attr(attr::VOLUME).is_none_or(|v| v <= cap));
                    t.published && sized && fits
                })
            })
            .collect();
        out.sort_by_cached_key(|c| self.type_(*c).map(|t| t.name.to_lowercase()).unwrap_or_default());
        out.dedup();
        out
    }

    pub fn mutaplasmid(&self, id: TypeId) -> Option<&Mutaplasmid> {
        self.mutaplasmids.iter().find(|m| m.id == id)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Index {
    type_by_id: HashMap<TypeId, u32>,
    type_by_name: HashMap<String, TypeId>,
    attr_by_id: HashMap<AttrId, u32>,
    attr_by_name: HashMap<String, AttrId>,
    effect_by_id: HashMap<EffectId, u32>,
    effect_by_name: HashMap<String, EffectId>,
    group_by_id: HashMap<GroupId, u32>,
    market_by_id: HashMap<MarketGroupId, u32>,
    buff_by_id: HashMap<BuffId, u32>,
    traits_by_type: HashMap<TypeId, u32>,
    types_by_group: HashMap<GroupId, Vec<TypeId>>,
    /// Parent type to every type in its family, the parent first.
    variants: HashMap<TypeId, Vec<TypeId>>,
}

impl Dogma {
    /// Sorts what the lookups rely on and builds the indexes. The baker and the blob reader both
    /// call it, so a dataset is ready as soon as it exists.
    pub fn finish(mut self) -> Self {
        self.types.sort_by_key(|t| t.id);
        self.attributes.sort_by_key(|a| a.id);
        self.effects.sort_by_key(|e| e.id);
        self.groups.sort_by_key(|g| g.id);
        self.market_groups.sort_by_key(|m| m.id);
        self.buffs.sort_by_key(|b| b.id);
        self.traits.sort_by_key(|t| t.type_id);
        for t in &mut self.types {
            t.attrs.sort_by_key(|(a, _)| *a);
        }
        let mut ix = Index::default();
        for (i, t) in self.types.iter().enumerate() {
            ix.type_by_id.insert(t.id, i as u32);
            ix.type_by_name.insert(t.name.to_lowercase(), t.id);
            ix.types_by_group.entry(t.group).or_default().push(t.id);
        }
        for (i, a) in self.attributes.iter().enumerate() {
            ix.attr_by_id.insert(a.id, i as u32);
            ix.attr_by_name.insert(a.name.clone(), a.id);
        }
        for (i, e) in self.effects.iter().enumerate() {
            ix.effect_by_id.insert(e.id, i as u32);
            ix.effect_by_name.insert(e.name.clone(), e.id);
        }
        for (i, g) in self.groups.iter().enumerate() {
            ix.group_by_id.insert(g.id, i as u32);
        }
        for (i, m) in self.market_groups.iter().enumerate() {
            ix.market_by_id.insert(m.id, i as u32);
        }
        for (i, b) in self.buffs.iter().enumerate() {
            ix.buff_by_id.insert(b.id, i as u32);
        }
        for (i, t) in self.traits.iter().enumerate() {
            ix.traits_by_type.insert(t.type_id, i as u32);
        }
        for t in &self.types {
            let parent = t.variation_parent.unwrap_or(t.id);
            ix.variants.entry(parent).or_default().push(t.id);
        }
        for (parent, family) in ix.variants.iter_mut() {
            family.sort_by_key(|id| (*id != *parent, *id));
        }
        self.index = ix;
        self
    }

    pub fn type_(&self, id: TypeId) -> Option<&TypeDef> {
        self.index.type_by_id.get(&id).map(|&i| &self.types[i as usize])
    }

    /// By English name, case-insensitively; the last of several identically named types wins.
    pub fn type_named(&self, name: &str) -> Option<&TypeDef> {
        self.index.type_by_name.get(&name.trim().to_lowercase()).and_then(|&id| self.type_(id))
    }

    /// The rack a module goes in, from the slot effect its type carries.
    pub fn slot_of(&self, type_id: TypeId) -> Option<crate::fit::Slot> {
        use crate::fit::Slot;
        let t = self.type_(type_id)?;
        t.effects.iter().find_map(|(e, _)| match self.effect(*e)?.name.as_str() {
            "hiPower" => Some(Slot::High),
            "medPower" => Some(Slot::Mid),
            "loPower" => Some(Slot::Low),
            "rigSlot" => Some(Slot::Rig),
            "subSystem" => Some(Slot::Subsystem),
            "serviceSlot" => Some(Slot::Service),
            _ => None,
        })
    }

    pub fn attr(&self, id: AttrId) -> Option<&AttrDef> {
        self.index.attr_by_id.get(&id).map(|&i| &self.attributes[i as usize])
    }

    pub fn attr_named(&self, name: &str) -> Option<&AttrDef> {
        self.index.attr_by_name.get(name).and_then(|&id| self.attr(id))
    }

    pub fn effect(&self, id: EffectId) -> Option<&EffectDef> {
        self.index.effect_by_id.get(&id).map(|&i| &self.effects[i as usize])
    }

    pub fn effect_named(&self, name: &str) -> Option<&EffectDef> {
        self.index.effect_by_name.get(name).and_then(|&id| self.effect(id))
    }

    pub fn group(&self, id: GroupId) -> Option<&GroupDef> {
        self.index.group_by_id.get(&id).map(|&i| &self.groups[i as usize])
    }

    pub fn market_group(&self, id: MarketGroupId) -> Option<&MarketGroup> {
        self.index.market_by_id.get(&id).map(|&i| &self.market_groups[i as usize])
    }

    pub fn buff(&self, id: BuffId) -> Option<&BuffDef> {
        self.index.buff_by_id.get(&id).map(|&i| &self.buffs[i as usize])
    }

    pub fn traits_of(&self, type_id: TypeId) -> Option<&TypeTraits> {
        self.index.traits_by_type.get(&type_id).map(|&i| &self.traits[i as usize])
    }

    pub fn types_in_group(&self, group: GroupId) -> &[TypeId] {
        self.index.types_by_group.get(&group).map_or(&[], Vec::as_slice)
    }

    /// The type's family: its Tech I parent first, then every variant, including itself.
    pub fn variants_of(&self, type_id: TypeId) -> &[TypeId] {
        let parent = self.type_(type_id).and_then(|t| t.variation_parent).unwrap_or(type_id);
        self.index.variants.get(&parent).map_or(&[], Vec::as_slice)
    }

    pub fn category_name(&self, id: CategoryId) -> &str {
        self.categories.iter().find(|c| c.id == id).map_or("", |c| c.name.as_str())
    }

    pub fn unit(&self, id: u8) -> Option<&UnitDef> {
        self.units.iter().find(|u| u.id == id)
    }
}

#[cfg(test)]
pub(crate) mod test_data {
    use super::*;

    /// A dataset of a frigate, a skill and a module with one effect, enough to exercise the
    /// lookups and the blob.
    pub fn small() -> Dogma {
        Dogma {
            sde_build: 42,
            types: vec![
                TypeDef {
                    id: 587,
                    name: "Rifter".into(),
                    group: 25,
                    category: 6,
                    market_group: Some(64),
                    meta_group: Some(1),
                    meta_level: Some(0),
                    tech: Some(1),
                    variation_parent: None,
                    published: true,
                    attrs: vec![(37, 365.0), (4, 1_067_000.0), (161, 27_289.0), (38, 140.0), (162, 35.0)],
                    effects: vec![],
                    required_skills: vec![(3329, 1)],
                    packaged: None,
                },
                TypeDef {
                    id: 3329,
                    name: "Minmatar Frigate".into(),
                    group: 257,
                    category: 16,
                    market_group: None,
                    meta_group: None,
                    meta_level: None,
                    tech: None,
                    variation_parent: None,
                    published: true,
                    attrs: vec![(4, 0.0), (38, 0.0), (161, 0.01), (162, 0.0)],
                    effects: vec![],
                    required_skills: vec![],
                    packaged: None,
                },
                TypeDef {
                    id: 2048,
                    name: "Damage Control II".into(),
                    group: 60,
                    category: 7,
                    market_group: Some(1),
                    meta_group: Some(2),
                    meta_level: Some(5),
                    tech: Some(2),
                    variation_parent: Some(2046),
                    published: true,
                    attrs: vec![(4, 0.0), (38, 0.0), (161, 5.0), (162, 0.0), (50, 30.0)],
                    effects: vec![(16, false)],
                    required_skills: vec![(3392, 4)],
                    packaged: None,
                },
            ],
            attributes: vec![
                AttrDef {
                    id: 37,
                    name: "maxVelocity".into(),
                    display: "Maximum Velocity".into(),
                    stackable: false,
                    high_is_good: true,
                    default: 0.0,
                    unit: Some(11),
                    min_attr: None,
                    max_attr: None,
                    category: Some(5),
                    published: true,
                },
                AttrDef {
                    id: 4,
                    name: "mass".into(),
                    display: "Mass".into(),
                    stackable: false,
                    high_is_good: true,
                    default: 0.0,
                    unit: Some(2),
                    min_attr: None,
                    max_attr: None,
                    category: Some(4),
                    published: true,
                },
            ],
            effects: vec![EffectDef {
                id: 16,
                name: "online".into(),
                category: EffectCategory::Active,
                offensive: false,
                assistance: false,
                duration_attr: None,
                range_attr: None,
                falloff_attr: None,
                tracking_attr: None,
                discharge_attr: None,
                usage_chance_attr: None,
                resistance_attr: None,
                modifiers: vec![Modifier {
                    domain: Domain::Ship,
                    func: Func::LocationRequiredSkill,
                    modified: 37,
                    modifying: 50,
                    op: Op::PostPercent,
                    group: None,
                    skill: Some(SkillRef::Own),
                }],
            }],
            groups: vec![
                GroupDef { id: 25, name: "Frigate".into(), category: 6 },
                GroupDef { id: 257, name: "Spaceship Command".into(), category: 16 },
                GroupDef { id: 60, name: "Damage Control".into(), category: 7 },
            ],
            categories: vec![
                CategoryDef { id: 6, name: "Ship".into() },
                CategoryDef { id: 7, name: "Module".into() },
                CategoryDef { id: 16, name: "Skill".into() },
            ],
            market_groups: vec![
                MarketGroup { id: 4, name: "Ships".into(), parent: None },
                MarketGroup { id: 64, name: "Minmatar".into(), parent: Some(4) },
                MarketGroup { id: 1, name: "Damage Controls".into(), parent: None },
            ],
            meta_groups: vec![MetaGroup { id: 1, name: "Tech I".into() }, MetaGroup { id: 2, name: "Tech II".into() }],
            units: vec![UnitDef { id: 11, name: "Speed".into(), display: "m/s".into() }],
            buffs: vec![BuffDef {
                id: 10,
                name: "Shield resistance".into(),
                aggregate: Aggregate::Max,
                op: Op::PostPercent,
                item: vec![],
                location: vec![],
                location_group: vec![],
                location_skill: vec![],
            }],
            traits: vec![TypeTraits {
                type_id: 587,
                role: vec![],
                misc: vec![],
                per_skill: vec![(3329, vec![Bonus { amount: Some(5.0), unit: Some(105), text: "bonus to Small Projectile Turret damage".into() }])],
            }],
            mutaplasmids: vec![],
            index: Index::default(),
        }
        .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups_find_what_the_baker_put_in() {
        let d = test_data::small();
        assert_eq!(d.type_(587).unwrap().name, "Rifter");
        assert_eq!(d.type_(587).unwrap().attr(37), Some(365.0));
        assert_eq!(d.type_(587).unwrap().mass(), 1_067_000.0);
        assert_eq!(d.attr_named("maxVelocity").unwrap().id, 37);
        assert_eq!(d.effect_named("online").unwrap().id, 16);
        assert_eq!(d.group(25).unwrap().name, "Frigate");
        assert_eq!(d.types_in_group(25), &[587]);
        assert_eq!(d.traits_of(587).unwrap().per_skill[0].0, 3329);
        assert_eq!(d.category_name(16), "Skill");
        assert!(d.type_(1).is_none());
    }

    #[test]
    fn a_variant_lists_its_family_parent_first() {
        let mut d = test_data::small();
        d.types.push(TypeDef { id: 2046, name: "Damage Control I".into(), variation_parent: None, ..d.type_(2048).unwrap().clone() });
        let d = d.finish();
        assert_eq!(d.variants_of(2048), &[2046, 2048]);
        assert_eq!(d.variants_of(2046), &[2046, 2048]);
        assert_eq!(d.variants_of(587), &[587]);
    }

    #[test]
    fn sde_numbers_map_to_the_enums() {
        assert_eq!(Op::from_sde(6), Some(Op::PostPercent));
        assert_eq!(Op::from_sde(-1), Some(Op::PreAssign));
        assert_eq!(Op::from_sde(9), None);
        assert_eq!(Op::from_name("PostAssignment"), Some(Op::PostAssign));
        assert_eq!(Domain::from_sde("target"), Some(Domain::Target));
        assert_eq!(Func::from_sde("EffectStopper"), None);
        assert_eq!(EffectCategory::from_sde(5), Some(EffectCategory::Overload));
        assert!(Op::PreAssign < Op::PostAssign);
    }
}
