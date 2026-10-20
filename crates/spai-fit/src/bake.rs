//! Reads EVE's JSONL static-data zip into a [`Dogma`] dataset. Each member is streamed line by
//! line, so the 150 MB `types.jsonl` never sits in memory whole. Native only: the web gets the
//! baked blob.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Seek};

use anyhow::{bail, Context as _, Result};
use serde::Deserialize;

use crate::dogma::{
    attr, Aggregate, AttrDef, Bonus, BuffDef, CategoryDef, Dogma, Domain, EffectCategory, EffectDef, Func, GroupDef,
    MarketGroup, MetaGroup, Modifier, Op, SkillRef, TypeDef, TypeTraits, UnitDef,
};
use crate::ids::{category, AttrId, EffectId, GroupId, MarketGroupId, TypeId, CHARACTER_TYPE};

#[derive(Deserialize, Default)]
struct Loc {
    #[serde(default)]
    en: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdeRec {
    #[serde(rename = "_key")]
    key: String,
    build_number: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CategoryRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    name: Loc,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(rename = "categoryID", default)]
    category_id: u32,
    #[serde(default)]
    name: Loc,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(rename = "groupID")]
    group_id: u32,
    #[serde(default)]
    name: Loc,
    #[serde(default)]
    published: bool,
    #[serde(rename = "marketGroupID")]
    market_group_id: Option<u32>,
    #[serde(rename = "metaGroupID")]
    meta_group_id: Option<u32>,
    meta_level: Option<f64>,
    tech_level: Option<f64>,
    #[serde(rename = "variationParentTypeID")]
    variation_parent_type_id: Option<u32>,
    mass: Option<f64>,
    volume: Option<f64>,
    capacity: Option<f64>,
    radius: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttrVal {
    #[serde(rename = "attributeID")]
    attribute_id: u32,
    value: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EffectRef {
    #[serde(rename = "effectID")]
    effect_id: u32,
    #[serde(default)]
    is_default: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeDogmaRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    dogma_attributes: Vec<AttrVal>,
    #[serde(default)]
    dogma_effects: Vec<EffectRef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttrRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    name: String,
    display_name: Option<Loc>,
    #[serde(default)]
    stackable: bool,
    #[serde(default)]
    high_is_good: bool,
    #[serde(default)]
    default_value: f64,
    #[serde(rename = "unitID")]
    unit_id: Option<u32>,
    #[serde(rename = "minAttributeID")]
    min_attribute_id: Option<u32>,
    #[serde(rename = "maxAttributeID")]
    max_attribute_id: Option<u32>,
    #[serde(rename = "attributeCategoryID")]
    attribute_category_id: Option<u32>,
    #[serde(default)]
    published: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModifierRec {
    #[serde(default)]
    domain: String,
    #[serde(default)]
    func: String,
    #[serde(rename = "modifiedAttributeID")]
    modified_attribute_id: Option<u32>,
    #[serde(rename = "modifyingAttributeID")]
    modifying_attribute_id: Option<u32>,
    operation: Option<i64>,
    #[serde(rename = "groupID")]
    group_id: Option<u32>,
    #[serde(rename = "skillTypeID")]
    skill_type_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EffectRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    name: String,
    #[serde(rename = "effectCategoryID", default)]
    effect_category_id: i64,
    #[serde(default)]
    is_offensive: bool,
    #[serde(default)]
    is_assistance: bool,
    #[serde(rename = "durationAttributeID")]
    duration_attribute_id: Option<u32>,
    #[serde(rename = "rangeAttributeID")]
    range_attribute_id: Option<u32>,
    #[serde(rename = "falloffAttributeID")]
    falloff_attribute_id: Option<u32>,
    #[serde(rename = "trackingSpeedAttributeID")]
    tracking_speed_attribute_id: Option<u32>,
    #[serde(rename = "dischargeAttributeID")]
    discharge_attribute_id: Option<u32>,
    #[serde(rename = "fittingUsageChanceAttributeID")]
    fitting_usage_chance_attribute_id: Option<u32>,
    #[serde(rename = "resistanceAttributeID")]
    resistance_attribute_id: Option<u32>,
    #[serde(default)]
    modifier_info: Vec<ModifierRec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    name: Loc,
    #[serde(rename = "parentGroupID")]
    parent_group_id: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NamedRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    name: Loc,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnitRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    name: String,
    display_name: Option<Loc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuffModRec {
    #[serde(rename = "dogmaAttributeID")]
    dogma_attribute_id: u32,
    #[serde(rename = "groupID")]
    group_id: Option<u32>,
    #[serde(rename = "skillID")]
    skill_id: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuffRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    aggregate_mode: String,
    #[serde(default)]
    operation_name: String,
    display_name: Option<Loc>,
    developer_description: Option<String>,
    #[serde(default)]
    item_modifiers: Vec<BuffModRec>,
    #[serde(default)]
    location_modifiers: Vec<BuffModRec>,
    #[serde(default)]
    location_group_modifiers: Vec<BuffModRec>,
    #[serde(default)]
    location_required_skill_modifiers: Vec<BuffModRec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BonusRec {
    bonus: Option<f64>,
    bonus_text: Option<Loc>,
    #[serde(default)]
    importance: i64,
    #[serde(rename = "unitID")]
    unit_id: Option<u32>,
}

#[derive(Deserialize)]
struct SkillBonusRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(rename = "_value", default)]
    value: Vec<BonusRec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TraitsRec {
    #[serde(rename = "_key")]
    key: u32,
    #[serde(default)]
    role_bonuses: Vec<BonusRec>,
    #[serde(default)]
    misc_bonuses: Vec<BonusRec>,
    #[serde(default)]
    types: Vec<SkillBonusRec>,
}

fn id16(n: u32, what: &str) -> Result<u16> {
    u16::try_from(n).with_context(|| format!("{what} id {n} does not fit 16 bits"))
}

fn opt16(n: Option<u32>, what: &str) -> Result<Option<u16>> {
    n.map(|n| id16(n, what)).transpose()
}

fn small(n: Option<f64>) -> Option<u8> {
    n.map(|n| n.round().clamp(0.0, 255.0) as u8)
}

/// The sentence without its `<a href=showinfo:...>` and other tags.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn bonus(b: BonusRec) -> Bonus {
    Bonus {
        amount: b.bonus.map(|v| v as f32),
        unit: b.unit_id.and_then(|u| u8::try_from(u).ok()),
        text: strip_tags(&b.bonus_text.map(|l| l.en).unwrap_or_default()),
    }
}

/// Streams the lines of one zip member, in order, through `each`.
fn lines<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    required: bool,
    mut each: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let entry = match archive.by_name(name) {
        Ok(e) => e,
        Err(_) if !required => return Ok(()),
        Err(e) => bail!("{name}: {e}"),
    };
    for (n, line) in BufReader::with_capacity(1 << 16, entry).lines().enumerate() {
        let line = line.with_context(|| format!("reading {name}"))?;
        if line.trim().is_empty() {
            continue;
        }
        each(&line).with_context(|| format!("{name} line {}", n + 1))?;
    }
    Ok(())
}

fn record<T: for<'a> Deserialize<'a>>(line: &str) -> Result<T> {
    serde_json::from_str(line).map_err(|e| anyhow::anyhow!("{e}"))
}

pub fn from_zip(bytes: &[u8]) -> Result<Dogma> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("opening the static-data zip")?;
    from_archive(&mut archive)
}

pub fn from_archive<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Dogma> {
    let mut build = 0u32;
    lines(archive, "_sde.jsonl", false, |l| {
        let r: SdeRec = record(l)?;
        if r.key == "sde" {
            build = r.build_number.unwrap_or(0);
        }
        Ok(())
    })?;

    let mut category_names: HashMap<u32, String> = HashMap::new();
    lines(archive, "categories.jsonl", true, |l| {
        let r: CategoryRec = record(l)?;
        category_names.insert(r.key, r.name.en);
        Ok(())
    })?;

    let mut group_recs: HashMap<u32, (String, u32)> = HashMap::new();
    lines(archive, "groups.jsonl", true, |l| {
        let r: GroupRec = record(l)?;
        group_recs.insert(r.key, (r.name.en, r.category_id));
        Ok(())
    })?;

    let mut types: HashMap<TypeId, TypeDef> = HashMap::new();
    let mut base_attrs: HashMap<TypeId, [(AttrId, f32); 4]> = HashMap::new();
    lines(archive, "types.jsonl", true, |l| {
        let r: TypeRec = record(l)?;
        let Some((_, cat)) = group_recs.get(&r.group_id) else { return Ok(()) };
        let cat = u8::try_from(*cat).unwrap_or(u8::MAX);
        let wanted = r.key == CHARACTER_TYPE || (category::BAKED.contains(&cat) && (r.published || cat == category::SKILL));
        if !wanted {
            return Ok(());
        }
        base_attrs.insert(
            r.key,
            [
                (attr::MASS, r.mass.unwrap_or(0.0) as f32),
                (attr::CAPACITY, r.capacity.unwrap_or(0.0) as f32),
                (attr::VOLUME, r.volume.unwrap_or(0.0) as f32),
                (attr::RADIUS, r.radius.unwrap_or(0.0) as f32),
            ],
        );
        types.insert(
            r.key,
            TypeDef {
                id: r.key,
                name: r.name.en,
                group: id16(r.group_id, "group")?,
                category: cat,
                market_group: opt16(r.market_group_id, "market group")?,
                meta_group: r.meta_group_id.and_then(|m| u8::try_from(m).ok()),
                meta_level: small(r.meta_level),
                tech: small(r.tech_level),
                variation_parent: r.variation_parent_type_id,
                published: r.published,
                attrs: Vec::new(),
                effects: Vec::new(),
                required_skills: Vec::new(),
            },
        );
        Ok(())
    })?;
    if types.is_empty() {
        bail!("types.jsonl holds no ships or modules");
    }

    let mut used_effects: HashSet<EffectId> = HashSet::new();
    lines(archive, "typeDogma.jsonl", true, |l| {
        let r: TypeDogmaRec = record(l)?;
        let Some(t) = types.get_mut(&r.key) else { return Ok(()) };
        let mut attrs: Vec<(AttrId, f32)> =
            r.dogma_attributes.iter().map(|a| Ok((id16(a.attribute_id, "attribute")?, a.value as f32))).collect::<Result<_>>()?;
        for (id, v) in base_attrs.get(&r.key).copied().unwrap_or_default() {
            if !attrs.iter().any(|(a, _)| *a == id) {
                attrs.push((id, v));
            }
        }
        attrs.sort_by_key(|(a, _)| *a);
        let value = |id: AttrId| attrs.iter().find(|(a, _)| *a == id).map(|(_, v)| *v);
        for (skill, level) in attr::REQUIRED_SKILLS {
            if let Some(s) = value(skill) {
                let s = s as TypeId;
                let lvl = value(level).unwrap_or(0.0).round().clamp(0.0, 5.0) as u8;
                if s != 0 && !t.required_skills.iter().any(|(k, _)| *k == s) {
                    t.required_skills.push((s, lvl));
                }
            }
        }
        t.attrs = attrs;
        t.effects = r.dogma_effects.iter().map(|e| Ok((id16(e.effect_id, "effect")?, e.is_default))).collect::<Result<_>>()?;
        used_effects.extend(t.effects.iter().map(|(e, _)| *e));
        Ok(())
    })?;
    for (id, base) in &base_attrs {
        if let Some(t) = types.get_mut(id) {
            if t.attrs.is_empty() {
                t.attrs = base.to_vec();
                t.attrs.sort_by_key(|(a, _)| *a);
            }
        }
    }

    let mut attributes = Vec::new();
    lines(archive, "dogmaAttributes.jsonl", true, |l| {
        let r: AttrRec = record(l)?;
        attributes.push(AttrDef {
            id: id16(r.key, "attribute")?,
            name: r.name,
            display: r.display_name.map(|d| d.en).unwrap_or_default(),
            stackable: r.stackable,
            high_is_good: r.high_is_good,
            default: r.default_value as f32,
            unit: r.unit_id.and_then(|u| u8::try_from(u).ok()),
            min_attr: opt16(r.min_attribute_id, "attribute")?,
            max_attr: opt16(r.max_attribute_id, "attribute")?,
            category: r.attribute_category_id.and_then(|c| u8::try_from(c).ok()),
            published: r.published,
        });
        Ok(())
    })?;

    let mut referenced_groups: HashSet<GroupId> = HashSet::new();
    let mut effects = Vec::new();
    lines(archive, "dogmaEffects.jsonl", true, |l| {
        let r: EffectRec = record(l)?;
        let id = id16(r.key, "effect")?;
        if !used_effects.contains(&id) {
            return Ok(());
        }
        let mut modifiers = Vec::new();
        for m in r.modifier_info {
            let (Some(domain), Some(func), Some(op)) =
                (Domain::from_sde(&m.domain), Func::from_sde(&m.func), m.operation.and_then(Op::from_sde))
            else {
                continue;
            };
            let (Some(modified), Some(modifying)) = (m.modified_attribute_id, m.modifying_attribute_id) else { continue };
            let group = opt16(m.group_id, "group")?;
            if let Some(g) = group {
                referenced_groups.insert(g);
            }
            let skill = match m.skill_type_id {
                Some(-1) => Some(SkillRef::Own),
                Some(s) if s > 0 => Some(SkillRef::Of(s as TypeId)),
                _ => None,
            };
            modifiers.push(Modifier {
                domain,
                func,
                modified: id16(modified, "attribute")?,
                modifying: id16(modifying, "attribute")?,
                op,
                group,
                skill,
            });
        }
        effects.push(EffectDef {
            id,
            name: r.name,
            category: EffectCategory::from_sde(r.effect_category_id).unwrap_or(EffectCategory::Passive),
            offensive: r.is_offensive,
            assistance: r.is_assistance,
            duration_attr: opt16(r.duration_attribute_id, "attribute")?,
            range_attr: opt16(r.range_attribute_id, "attribute")?,
            falloff_attr: opt16(r.falloff_attribute_id, "attribute")?,
            tracking_attr: opt16(r.tracking_speed_attribute_id, "attribute")?,
            discharge_attr: opt16(r.discharge_attribute_id, "attribute")?,
            usage_chance_attr: opt16(r.fitting_usage_chance_attribute_id, "attribute")?,
            resistance_attr: opt16(r.resistance_attribute_id, "attribute")?,
            modifiers,
        });
        Ok(())
    })?;

    let mut market_recs: HashMap<MarketGroupId, MarketGroup> = HashMap::new();
    lines(archive, "marketGroups.jsonl", true, |l| {
        let r: MarketRec = record(l)?;
        let id = id16(r.key, "market group")?;
        market_recs.insert(id, MarketGroup { id, name: r.name.en, parent: opt16(r.parent_group_id, "market group")? });
        Ok(())
    })?;
    let mut market_keep: HashSet<MarketGroupId> = HashSet::new();
    for t in types.values() {
        let mut next = t.market_group;
        while let Some(id) = next {
            if !market_keep.insert(id) {
                break;
            }
            next = market_recs.get(&id).and_then(|m| m.parent);
        }
    }
    let market_groups: Vec<MarketGroup> = market_recs.into_values().filter(|m| market_keep.contains(&m.id)).collect();

    let mut meta_groups = Vec::new();
    lines(archive, "metaGroups.jsonl", false, |l| {
        let r: NamedRec = record(l)?;
        if let Ok(id) = u8::try_from(r.key) {
            meta_groups.push(MetaGroup { id, name: r.name.en });
        }
        Ok(())
    })?;

    let mut units = Vec::new();
    lines(archive, "dogmaUnits.jsonl", false, |l| {
        let r: UnitRec = record(l)?;
        if let Ok(id) = u8::try_from(r.key) {
            units.push(UnitDef { id, name: r.name, display: r.display_name.map(|d| d.en).unwrap_or_default() });
        }
        Ok(())
    })?;

    let mut buffs = Vec::new();
    lines(archive, "dbuffCollections.jsonl", false, |l| {
        let r: BuffRec = record(l)?;
        let Some(op) = Op::from_name(&r.operation_name) else { return Ok(()) };
        let aggregate = if r.aggregate_mode == "Minimum" { Aggregate::Min } else { Aggregate::Max };
        let attr_of = |m: &BuffModRec| id16(m.dogma_attribute_id, "attribute");
        let location_group = r
            .location_group_modifiers
            .iter()
            .filter_map(|m| m.group_id.map(|g| (g, m)))
            .map(|(g, m)| Ok((id16(g, "group")?, attr_of(m)?)))
            .collect::<Result<Vec<(GroupId, AttrId)>>>()?;
        for (g, _) in &location_group {
            referenced_groups.insert(*g);
        }
        let name = r
            .display_name
            .map(|d| d.en)
            .filter(|n| !n.is_empty())
            .or(r.developer_description)
            .unwrap_or_default();
        buffs.push(BuffDef {
            id: id16(r.key, "buff")?,
            name,
            aggregate,
            op,
            item: r.item_modifiers.iter().map(attr_of).collect::<Result<_>>()?,
            location: r.location_modifiers.iter().map(attr_of).collect::<Result<_>>()?,
            location_group,
            location_skill: r
                .location_required_skill_modifiers
                .iter()
                .filter_map(|m| m.skill_id.map(|s| (s, m)))
                .map(|(s, m)| Ok((s, attr_of(m)?)))
                .collect::<Result<_>>()?,
        });
        Ok(())
    })?;

    let mut traits = Vec::new();
    lines(archive, "typeBonus.jsonl", false, |l| {
        let r: TraitsRec = record(l)?;
        if !types.get(&r.key).is_some_and(|t| t.category == category::SHIP) {
            return Ok(());
        }
        let ordered = |mut v: Vec<BonusRec>| {
            v.sort_by_key(|b| b.importance);
            v.into_iter().map(bonus).collect::<Vec<_>>()
        };
        traits.push(TypeTraits {
            type_id: r.key,
            role: ordered(r.role_bonuses),
            misc: ordered(r.misc_bonuses),
            per_skill: r.types.into_iter().map(|s| (s.key, ordered(s.value))).collect(),
        });
        Ok(())
    })?;

    let mut group_keep: HashSet<GroupId> = types.values().map(|t| t.group).collect();
    group_keep.extend(referenced_groups);
    let groups: Vec<GroupDef> = group_recs
        .iter()
        .filter_map(|(id, (name, cat))| {
            let id = u16::try_from(*id).ok()?;
            group_keep.contains(&id).then(|| GroupDef { id, name: name.clone(), category: u8::try_from(*cat).unwrap_or(u8::MAX) })
        })
        .collect();
    let mut category_ids: Vec<u8> = groups.iter().map(|g| g.category).collect();
    category_ids.sort_unstable();
    category_ids.dedup();
    let categories = category_ids
        .into_iter()
        .map(|id| CategoryDef { id, name: category_names.get(&u32::from(id)).cloned().unwrap_or_default() })
        .collect();

    Ok(Dogma {
        sde_build: build,
        types: types.into_values().collect(),
        attributes,
        effects,
        groups,
        categories,
        market_groups,
        meta_groups,
        units,
        buffs,
        traits,
        ..Dogma::default()
    }
    .finish())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write as _;

    /// A zip of the members the baker reads, each holding the given lines.
    pub(crate) fn zip_of(members: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in members {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    pub(crate) fn mini_zip() -> Vec<u8> {
        zip_of(&[
            ("_sde.jsonl", r#"{"_key": "sde", "buildNumber": 3607705, "releaseDate": "2026-10-12T11:08:38Z"}"#),
            ("categories.jsonl", "{\"_key\": 1, \"name\": {\"en\": \"Owner\"}}\n{\"_key\": 6, \"name\": {\"en\": \"Ship\"}}\n{\"_key\": 7, \"name\": {\"en\": \"Module\"}}\n{\"_key\": 16, \"name\": {\"en\": \"Skill\"}}\n{\"_key\": 2, \"name\": {\"en\": \"Celestial\"}}"),
            ("groups.jsonl", "{\"_key\": 1, \"categoryID\": 1, \"name\": {\"en\": \"Character\"}}\n{\"_key\": 25, \"categoryID\": 6, \"name\": {\"en\": \"Frigate\"}}\n{\"_key\": 60, \"categoryID\": 7, \"name\": {\"en\": \"Damage Control\"}}\n{\"_key\": 257, \"categoryID\": 16, \"name\": {\"en\": \"Spaceship Command\"}}\n{\"_key\": 5, \"categoryID\": 2, \"name\": {\"en\": \"Solar System\"}}\n{\"_key\": 55, \"categoryID\": 7, \"name\": {\"en\": \"Projectile Weapon\"}}"),
            ("types.jsonl", concat!(
                "{\"_key\": 587, \"groupID\": 25, \"name\": {\"de\": \"Rifter\", \"en\": \"Rifter\"}, \"published\": true, \"marketGroupID\": 64, \"metaGroupID\": 1, \"metaLevel\": 0, \"techLevel\": 1, \"mass\": 1067000.0, \"volume\": 27289.0, \"capacity\": 140.0, \"radius\": 35.0, \"description\": {\"en\": \"long\"}}\n",
                "{\"_key\": 2048, \"groupID\": 60, \"name\": {\"en\": \"Damage Control II\"}, \"published\": true, \"marketGroupID\": 1, \"metaGroupID\": 2, \"metaLevel\": 5, \"techLevel\": 2, \"variationParentTypeID\": 2046, \"volume\": 5.0}\n",
                "{\"_key\": 2046, \"groupID\": 60, \"name\": {\"en\": \"Damage Control I\"}, \"published\": true, \"marketGroupID\": 1, \"metaGroupID\": 1, \"volume\": 5.0}\n",
                "{\"_key\": 3329, \"groupID\": 257, \"name\": {\"en\": \"Minmatar Frigate\"}, \"published\": false, \"volume\": 0.01}\n",
                "{\"_key\": 30000142, \"groupID\": 5, \"name\": {\"en\": \"Jita\"}, \"published\": true}\n",
                "{\"_key\": 999, \"groupID\": 55, \"name\": {\"en\": \"Unpublished Gun\"}, \"published\": false}\n",
                "{\"_key\": 1373, \"groupID\": 1, \"name\": {\"en\": \"CharacterType\"}, \"published\": false}\n",
                "{\"_key\": 1374, \"groupID\": 1, \"name\": {\"en\": \"CharacterLegacy10\"}, \"published\": false}\n",
            )),
            ("typeDogma.jsonl", concat!(
                "{\"_key\": 587, \"dogmaAttributes\": [{\"attributeID\": 37, \"value\": 365.0}, {\"attributeID\": 182, \"value\": 3329.0}, {\"attributeID\": 277, \"value\": 1.0}, {\"attributeID\": 4, \"value\": 1067000.0}], \"dogmaEffects\": [{\"effectID\": 16, \"isDefault\": false}]}\n",
                "{\"_key\": 2048, \"dogmaAttributes\": [{\"attributeID\": 50, \"value\": 30.0}, {\"attributeID\": 182, \"value\": 3392.0}, {\"attributeID\": 277, \"value\": 4.0}, {\"attributeID\": 183, \"value\": 3392.0}, {\"attributeID\": 278, \"value\": 1.0}], \"dogmaEffects\": [{\"effectID\": 16, \"isDefault\": false}, {\"effectID\": 2000, \"isDefault\": true}]}\n",
                "{\"_key\": 3329, \"dogmaAttributes\": [{\"attributeID\": 275, \"value\": 2.0}], \"dogmaEffects\": [{\"effectID\": 2001, \"isDefault\": false}]}\n",
                "{\"_key\": 30000142, \"dogmaAttributes\": [{\"attributeID\": 4, \"value\": 1.0}], \"dogmaEffects\": [{\"effectID\": 2002, \"isDefault\": false}]}\n",
                "{\"_key\": 1373, \"dogmaAttributes\": [{\"attributeID\": 458, \"value\": 20000.0}]}\n",
            )),
            ("dogmaAttributes.jsonl", concat!(
                "{\"_key\": 37, \"attributeCategoryID\": 5, \"defaultValue\": 0.0, \"displayName\": {\"en\": \"Maximum Velocity\"}, \"highIsGood\": true, \"name\": \"maxVelocity\", \"published\": true, \"stackable\": false, \"unitID\": 11}\n",
                "{\"_key\": 4, \"attributeCategoryID\": 4, \"defaultValue\": 0.0, \"displayName\": {\"en\": \"Mass\"}, \"highIsGood\": true, \"name\": \"mass\", \"published\": true, \"stackable\": false, \"unitID\": 2}\n",
                "{\"_key\": 50, \"defaultValue\": 0.0, \"highIsGood\": true, \"name\": \"cpu\", \"published\": true, \"stackable\": true}\n",
                "{\"_key\": 275, \"defaultValue\": 1.0, \"highIsGood\": true, \"name\": \"skillTimeConstant\", \"published\": true, \"stackable\": true, \"maxAttributeID\": 37}\n",
            )),
            ("dogmaEffects.jsonl", concat!(
                "{\"_key\": 16, \"effectCategoryID\": 4, \"guid\": \"effects.Online\", \"isAssistance\": false, \"isOffensive\": false, \"name\": \"online\"}\n",
                "{\"_key\": 2000, \"effectCategoryID\": 0, \"name\": \"damageControl\", \"modifierInfo\": [{\"domain\": \"shipID\", \"func\": \"ItemModifier\", \"modifiedAttributeID\": 37, \"modifyingAttributeID\": 50, \"operation\": 6}, {\"domain\": \"shipID\", \"func\": \"EffectStopper\", \"modifiedAttributeID\": 37, \"modifyingAttributeID\": 50, \"operation\": 6}, {\"domain\": \"shipID\", \"func\": \"LocationGroupModifier\", \"groupID\": 55, \"modifiedAttributeID\": 37, \"modifyingAttributeID\": 50, \"operation\": 9}]}\n",
                "{\"_key\": 2001, \"effectCategoryID\": 0, \"name\": \"shipBonus\", \"isOffensive\": true, \"durationAttributeID\": 73, \"modifierInfo\": [{\"domain\": \"charID\", \"func\": \"OwnerRequiredSkillModifier\", \"modifiedAttributeID\": 37, \"modifyingAttributeID\": 275, \"operation\": 6, \"skillTypeID\": -1}, {\"domain\": \"shipID\", \"func\": \"LocationRequiredSkillModifier\", \"modifiedAttributeID\": 37, \"modifyingAttributeID\": 275, \"operation\": 2, \"skillTypeID\": 3329}]}\n",
                "{\"_key\": 2002, \"effectCategoryID\": 0, \"name\": \"notFitted\"}\n",
                "{\"_key\": 2003, \"effectCategoryID\": 0, \"name\": \"unused\"}\n",
            )),
            ("marketGroups.jsonl", "{\"_key\": 4, \"name\": {\"en\": \"Ships\"}}\n{\"_key\": 64, \"name\": {\"en\": \"Minmatar\"}, \"parentGroupID\": 4}\n{\"_key\": 1, \"name\": {\"en\": \"Damage Controls\"}}\n{\"_key\": 2, \"name\": {\"en\": \"Blueprints\"}}"),
            ("metaGroups.jsonl", "{\"_key\": 1, \"name\": {\"en\": \"Tech I\"}}\n{\"_key\": 2, \"name\": {\"en\": \"Tech II\"}}"),
            ("dogmaUnits.jsonl", "{\"_key\": 11, \"displayName\": {\"en\": \"m/s\"}, \"name\": \"Speed\"}"),
            ("dbuffCollections.jsonl", concat!(
                "{\"_key\": 10, \"aggregateMode\": \"Maximum\", \"displayName\": {\"en\": \"Shield resistance\"}, \"itemModifiers\": [{\"dogmaAttributeID\": 37}], \"locationGroupModifiers\": [{\"dogmaAttributeID\": 50, \"groupID\": 60}], \"locationRequiredSkillModifiers\": [{\"dogmaAttributeID\": 4, \"skillID\": 3329}], \"operationName\": \"PostPercent\"}\n",
                "{\"_key\": 11, \"aggregateMode\": \"Minimum\", \"developerDescription\": \"Velocity penalty\", \"itemModifiers\": [{\"dogmaAttributeID\": 37}], \"operationName\": \"PostAssignment\"}\n",
                "{\"_key\": 12, \"aggregateMode\": \"Maximum\", \"operationName\": \"Weird\"}\n",
            )),
            ("typeBonus.jsonl", concat!(
                "{\"_key\": 587, \"roleBonuses\": [{\"bonus\": 50.0, \"bonusText\": {\"en\": \"bonus to <a href=showinfo:3300>Small Projectile Turret</a> falloff\"}, \"importance\": 2, \"unitID\": 105}, {\"bonusText\": {\"en\": \"Can fit  <a href=\\\"showinfo:1\\\">things</a>\"}, \"importance\": 1}], \"types\": [{\"_key\": 3329, \"_value\": [{\"bonus\": 10, \"bonusText\": {\"en\": \"bonus to damage\"}, \"importance\": 1, \"unitID\": 105}]}]}\n",
                "{\"_key\": 2048, \"roleBonuses\": [{\"bonus\": 1.0, \"bonusText\": {\"en\": \"not a ship\"}, \"importance\": 1}]}\n",
            )),
        ])
    }

    #[test]
    fn the_mini_zip_bakes_into_a_filtered_dataset() {
        let d = from_zip(&mini_zip()).unwrap();
        assert_eq!(d.sde_build, 3607705);
        let mut ids: Vec<_> = d.types.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![587, 1373, 2046, 2048, 3329], "fitting categories only, unpublished skills and the pilot's type kept");
        assert_eq!(d.type_(1373).unwrap().attr(458), Some(20000.0));
        let rifter = d.type_(587).unwrap();
        assert_eq!(rifter.category, 6);
        assert_eq!(rifter.attr(37), Some(365.0));
        assert_eq!(rifter.mass(), 1_067_000.0);
        assert_eq!(rifter.capacity(), 140.0);
        assert_eq!(rifter.radius(), 35.0);
        assert_eq!(rifter.required_skills, vec![(3329, 1)]);
        assert_eq!(rifter.tech, Some(1));
        let dc = d.type_(2048).unwrap();
        assert_eq!(dc.required_skills, vec![(3392, 4)], "a skill named twice counts once");
        assert_eq!(dc.variation_parent, Some(2046));
        assert_eq!(d.variants_of(2048), &[2046, 2048]);
        assert_eq!(d.type_(2046).unwrap().volume(), 5.0, "a type without dogma still has its base attributes");
        assert_eq!(d.type_(3329).unwrap().attr(275), Some(2.0));
    }

    #[test]
    fn effects_keep_only_real_modifiers_and_only_used_ones() {
        let d = from_zip(&mini_zip()).unwrap();
        let mut names: Vec<_> = d.effects.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["damageControl", "online", "shipBonus"]);
        let dc = d.effect_named("damageControl").unwrap();
        assert_eq!(dc.modifiers.len(), 1, "EffectStopper and operation 9 are dropped");
        assert_eq!(dc.modifiers[0], Modifier { domain: Domain::Ship, func: Func::Item, modified: 37, modifying: 50, op: Op::PostPercent, group: None, skill: None });
        let sb = d.effect_named("shipBonus").unwrap();
        assert_eq!(sb.category, EffectCategory::Passive);
        assert!(sb.offensive);
        assert_eq!(sb.duration_attr, Some(73));
        assert_eq!(sb.modifiers[0].skill, Some(SkillRef::Own));
        assert_eq!(sb.modifiers[1].skill, Some(SkillRef::Of(3329)));
        assert_eq!(sb.modifiers[1].op, Op::ModAdd);
        assert_eq!(d.effect_named("online").unwrap().category, EffectCategory::Online);
    }

    #[test]
    fn groups_markets_buffs_and_traits_are_filtered_and_cleaned() {
        let d = from_zip(&mini_zip()).unwrap();
        let mut groups: Vec<_> = d.groups.iter().map(|g| g.id).collect();
        groups.sort_unstable();
        assert_eq!(groups, vec![1, 25, 60, 257], "groups of kept types; the unused gun group has no modifier left");
        let mut cats: Vec<_> = d.categories.iter().map(|c| (c.id, c.name.as_str())).collect();
        cats.sort_unstable();
        assert_eq!(cats, vec![(1, "Owner"), (6, "Ship"), (7, "Module"), (16, "Skill")]);
        let mut markets: Vec<_> = d.market_groups.iter().map(|m| m.id).collect();
        markets.sort_unstable();
        assert_eq!(markets, vec![1, 4, 64], "ancestors kept, blueprints dropped");
        assert_eq!(d.market_group(64).unwrap().parent, Some(4));
        assert_eq!(d.attr_named("skillTimeConstant").unwrap().max_attr, Some(37));
        assert_eq!(d.unit(11).unwrap().display, "m/s");
        assert_eq!(d.meta_groups.len(), 2);
        assert_eq!(d.buffs.len(), 2, "a buff with an unknown operation is dropped");
        let b = d.buff(10).unwrap();
        assert_eq!((b.aggregate, b.op), (Aggregate::Max, Op::PostPercent));
        assert_eq!(b.item, vec![37]);
        assert_eq!(b.location_group, vec![(60, 50)]);
        assert_eq!(b.location_skill, vec![(3329, 4)]);
        let b = d.buff(11).unwrap();
        assert_eq!((b.aggregate, b.op, b.name.as_str()), (Aggregate::Min, Op::PostAssign, "Velocity penalty"));
        assert_eq!(d.traits.len(), 1, "traits of ships only");
        let t = d.traits_of(587).unwrap();
        assert_eq!(t.role[0].text, "Can fit things", "tags gone, spaces folded, importance ordered");
        assert_eq!(t.role[1], Bonus { amount: Some(50.0), unit: Some(105), text: "bonus to Small Projectile Turret falloff".into() });
        assert_eq!(t.per_skill, vec![(3329, vec![Bonus { amount: Some(10.0), unit: Some(105), text: "bonus to damage".into() }])]);
    }

    #[test]
    fn a_zip_without_the_dogma_members_is_refused() {
        let z = zip_of(&[("mapSolarSystems.jsonl", "{}\n")]);
        assert!(from_zip(&z).unwrap_err().to_string().contains("categories.jsonl"));
    }

    #[test]
    fn the_baked_dataset_round_trips_through_the_blob() {
        let d = from_zip(&mini_zip()).unwrap();
        let back = crate::dogma::blob::decode(&crate::dogma::blob::encode(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }

    /// The real zip: `SPAI_SDE_ZIP=path/to/eve-online-static-data-latest-jsonl.zip`.
    #[test]
    #[ignore]
    fn the_real_zip_bakes_small_and_consistent() {
        let path = std::env::var("SPAI_SDE_ZIP").expect("SPAI_SDE_ZIP");
        let bytes = std::fs::read(&path).unwrap();
        let started = std::time::Instant::now();
        let d = from_zip(&bytes).unwrap();
        eprintln!("baked in {:?}", started.elapsed());
        assert!(d.sde_build > 3_000_000);
        let ships = d.types.iter().filter(|t| t.category == category::SHIP).count();
        assert!(ships > 400 && d.types.len() > 7000, "{ships} ships, {} types", d.types.len());
        let rifter = d.types.iter().find(|t| t.name == "Rifter").unwrap();
        assert!(rifter.attr(37).unwrap() > 300.0);
        assert!(rifter.mass() > 1.0e6);
        assert!(!rifter.required_skills.is_empty());
        assert!(d.traits_of(rifter.id).is_some_and(|t| !t.per_skill.is_empty()));
        for t in &d.types {
            for (skill, _) in &t.required_skills {
                assert!(d.type_(*skill).is_some_and(|s| s.category == category::SKILL), "{} needs unknown skill {skill}", t.name);
            }
            for (e, _) in &t.effects {
                assert!(d.effect(*e).is_some(), "{} has unknown effect {e}", t.name);
            }
            assert!(d.group(t.group).is_some());
            if let Some(m) = t.market_group {
                assert!(d.market_group(m).is_some());
            }
        }
        for e in &d.effects {
            for m in &e.modifiers {
                assert!(d.attr(m.modified).is_some() && d.attr(m.modifying).is_some(), "{} modifies unknown attribute", e.name);
                if let Some(g) = m.group {
                    assert!(d.group(g).is_some(), "{} filters by unknown group {g}", e.name);
                }
            }
        }
        assert!(d.effect_named("online").is_some());
        assert!(d.effect_named("moduleBonusMicrowarpdrive").is_some());
        assert!(d.buff(10).is_some());
        let blob = crate::dogma::blob::encode(&d).unwrap();
        eprintln!("{} types, {} effects, {} attributes, blob {} bytes", d.types.len(), d.effects.len(), d.attributes.len(), blob.len());
        assert!(blob.len() < 3 << 20, "blob is {} bytes", blob.len());
        let started = std::time::Instant::now();
        let back = crate::dogma::blob::decode(&blob).unwrap();
        eprintln!("decoded in {:?}", started.elapsed());
        assert_eq!(back.types.len(), d.types.len());
    }
}
