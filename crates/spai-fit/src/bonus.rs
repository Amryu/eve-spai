//! Which hulls, subsystems and skills change a module, read from their effects' modifiers: what a
//! projected module can be flown from, and what its pilot's training does to it.

use crate::dogma::{attr, Domain, Dogma, Func, Modifier, SkillRef, TypeDef};
use crate::ids::{category, TypeId};

/// What it costs to fit, how many may run, what heat does to it and how much it takes, none of
/// which reaches the ship it is pointed at.
fn own_business(dogma: &Dogma, a: crate::ids::AttrId) -> bool {
    [attr::CPU, attr::POWER, attr::CAPACITOR_NEED].contains(&a) || dogma.attr(a).is_some_and(|d| d.name == "hp" || d.name.starts_with("heat") || d.name.starts_with("overload") || d.name.starts_with("maxGroup") || d.name.contains("CPU"))
}

/// Whether a modifier on `owner`'s effect changes what `target` does to another ship: an attribute
/// it has, located on the ship or owned by the pilot.
pub fn reaches(dogma: &Dogma, m: &Modifier, owner: TypeId, target: &TypeDef) -> bool {
    if !matches!(m.domain, Domain::Ship | Domain::Char) || target.attr(m.modified).is_none() || own_business(dogma, m.modified) {
        return false;
    }
    let needs = |skill: Option<&SkillRef>| match skill {
        Some(SkillRef::Of(s)) => target.required_skills.iter().any(|(r, _)| r == s),
        Some(SkillRef::Own) => target.required_skills.iter().any(|(r, _)| *r == owner),
        None => false,
    };
    match m.func {
        Func::Location => m.domain == Domain::Ship,
        Func::LocationGroup => m.group == Some(target.group),
        Func::LocationRequiredSkill | Func::OwnerRequiredSkill => needs(m.skill.as_ref()),
        Func::Item => false,
    }
}

/// Whether any effect of `owner` changes `target` or its charge.
pub fn changes(dogma: &Dogma, owner: &TypeDef, targets: &[&TypeDef]) -> bool {
    owner.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).flat_map(|e| e.modifiers.iter()).any(|m| targets.iter().any(|t| reaches(dogma, m, owner.id, t)))
}

/// A hull that bonuses a module: on its own, or through one of its subsystems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BonusSource {
    pub hull: TypeId,
    pub subsystem: Option<TypeId>,
}

fn targets(dogma: &Dogma, type_id: TypeId, charge: Option<TypeId>) -> Vec<&TypeDef> {
    [Some(type_id), charge].into_iter().flatten().filter_map(|t| dogma.type_(t)).collect()
}

/// The subsystem's hull.
pub fn subsystem_hull(dogma: &Dogma, subsystem: TypeId) -> Option<TypeId> {
    let fits = dogma.attr_named("fitsToShipType")?.id;
    dogma.type_(subsystem).and_then(|t| t.attr(fits)).map(|v| v as TypeId)
}

/// Every hull whose bonuses change the module (or its charge), by name; a strategic cruiser once
/// for each subsystem that does.
pub fn bonus_sources(dogma: &Dogma, type_id: TypeId, charge: Option<TypeId>) -> Vec<BonusSource> {
    let targets = targets(dogma, type_id, charge);
    let mut out: Vec<(String, BonusSource)> = Vec::new();
    for t in dogma.types.iter().filter(|t| t.published && matches!(t.category, category::SHIP | category::SUBSYSTEM)) {
        if !changes(dogma, t, &targets) {
            continue;
        }
        let source = if t.category == category::SHIP {
            BonusSource { hull: t.id, subsystem: None }
        } else {
            let Some(hull) = subsystem_hull(dogma, t.id) else { continue };
            BonusSource { hull, subsystem: Some(t.id) }
        };
        let name = format!("{} {}", crate::formats::name_of(dogma, source.hull), source.subsystem.map(|s| crate::formats::name_of(dogma, s)).unwrap_or_default());
        out.push((name, source));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, s)| s).collect()
}

/// The skills that matter to the module flown from this hull and these subsystems: those it needs,
/// those whose effects change it or its charge, and those its hull's and subsystems' bonuses grow
/// with. By name.
pub fn relevant_skills(dogma: &Dogma, type_id: TypeId, charge: Option<TypeId>, hull: Option<TypeId>, subsystems: &[TypeId]) -> Vec<TypeId> {
    let targets = targets(dogma, type_id, charge);
    let mut out: Vec<TypeId> = targets.iter().flat_map(|t| t.required_skills.iter().map(|(s, _)| *s)).collect();
    for t in dogma.types.iter().filter(|t| t.published && t.category == category::SKILL) {
        if changes(dogma, t, &targets) {
            out.push(t.id);
        }
    }
    for owner in hull.into_iter().chain(subsystems.iter().copied()) {
        let bonuses = dogma.type_(owner).is_some_and(|t| changes(dogma, t, &targets));
        if !bonuses {
            continue;
        }
        // A hull's bonuses grow with the skills its traits name, a subsystem's with the one it needs.
        if let Some(traits) = dogma.traits_of(owner) {
            out.extend(traits.per_skill.iter().map(|(s, _)| *s));
        }
        if dogma.type_(owner).is_some_and(|t| t.category == category::SUBSYSTEM) {
            out.extend(dogma.type_(owner).into_iter().flat_map(|t| t.required_skills.iter().map(|(s, _)| *s)));
        }
    }
    let mut named: Vec<(String, TypeId)> = out.into_iter().filter(|s| dogma.type_(*s).is_some_and(|t| t.published)).map(|s| (crate::formats::name_of(dogma, s), s)).collect();
    named.sort();
    named.dedup();
    named.into_iter().map(|(_, s)| s).collect()
}

/// The subsystems a strategic cruiser flies a module with by default: in each of its subsystem
/// slots, the first that bonuses the module, if any does.
pub fn default_subsystems(dogma: &Dogma, hull: TypeId, type_id: TypeId, charge: Option<TypeId>) -> Vec<TypeId> {
    let targets = targets(dogma, type_id, charge);
    let fits = dogma.attr_named("fitsToShipType").map(|a| a.id);
    dogma
        .subsystem_groups(hull)
        .into_iter()
        .filter_map(|g| {
            dogma.types_in_group(g).iter().filter_map(|t| dogma.type_(*t)).filter(|t| t.published && fits.and_then(|f| t.attr(f)).is_some_and(|v| v as TypeId == hull)).find(|t| changes(dogma, t, &targets)).map(|t| t.id)
        })
        .collect()
}

/// A strategic cruiser's subsystems for one slot, by name.
pub fn subsystems_for(dogma: &Dogma, hull: TypeId, group: crate::ids::GroupId) -> Vec<TypeId> {
    let fits = dogma.attr_named("fitsToShipType").map(|a| a.id);
    let mut v: Vec<&TypeDef> = dogma.types_in_group(group).iter().filter_map(|t| dogma.type_(*t)).filter(|t| t.published && fits.and_then(|f| t.attr(f)).is_some_and(|v| v as TypeId == hull)).collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v.into_iter().map(|t| t.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::{BoostShortcut, Projected, ProjectedSource};
    use crate::fleet::BurstKind;
    use crate::test_support::{dataset, ids};

    /// A strategic cruiser bonuses a web through its core subsystem, which it picks by default,
    /// and the web from it reaches further than from a hull without bonuses.
    #[test]
    fn a_web_is_flown_from_the_subsystem_that_bonuses_it() {
        let d = dataset();
        let sources = bonus_sources(&d, ids::WEB, None);
        assert!(sources.contains(&BonusSource { hull: ids::T3C, subsystem: Some(ids::WEB_SUB) }), "{sources:?}");
        assert!(!sources.iter().any(|s| s.subsystem == Some(ids::BOOST_SUB)), "the burst subsystem does nothing to a web");
        assert_eq!(default_subsystems(&d, ids::T3C, ids::WEB, None), [ids::WEB_SUB]);
        let range = |source: ProjectedSource| {
            let p = Projected { type_id: ids::WEB, source, ..Default::default() };
            crate::projection::source_values(&d, &p).into_iter().find(|(a, _)| *a == attr::MAX_RANGE).map(|(_, v)| v)
        };
        assert_eq!(range(ProjectedSource::default()), Some(10_000.0));
        assert_eq!(range(ProjectedSource { hull: Some(ids::T3C), subsystems: vec![ids::WEB_SUB], skills: vec![] }), Some(22_500.0));
        assert_eq!(range(ProjectedSource { hull: Some(ids::T3C), subsystems: vec![], skills: vec![] }), Some(10_000.0), "the hull alone gives nothing");
    }

    /// Heat and fitting costs reach a module too, but do nothing to the ship it is pointed at.
    #[test]
    fn what_only_the_fitting_pilot_feels_is_no_bonus_to_offer() {
        let d = dataset();
        let web = d.type_(ids::WEB).unwrap();
        let cpu = crate::dogma::Modifier { domain: Domain::Ship, func: Func::Location, modified: attr::CPU, modifying: attr::CPU, op: crate::dogma::Op::PostPercent, group: None, skill: None };
        assert!(!reaches(&d, &cpu, ids::SHIP, web));
        assert!(reaches(&d, &crate::dogma::Modifier { modified: attr::MAX_RANGE, ..cpu }, ids::SHIP, web));
    }

    /// A strategic cruiser boosts through its burst subsystem: offered as a booster hull, flown
    /// with that subsystem, stronger than a hull without bonuses.
    #[test]
    fn a_strategic_cruiser_boosts_through_its_subsystem() {
        let d = dataset();
        assert_eq!(crate::fleet::boost_subsystem(&d, ids::T3C, BurstKind::Shield), Some(ids::BOOST_SUB));
        assert!(crate::fleet::boosting_hulls(&d, BurstKind::Shield).contains(&ids::T3C));
        let strength = |hull| crate::fleet::shortcut_bursts(&d, &BoostShortcut { hull, ..Default::default() }).first().map(|(_, v)| *v).unwrap_or(0.0);
        let (plain, t3c) = (strength(None), strength(Some(ids::T3C)));
        assert!(t3c > plain * 1.05, "{plain} -> {t3c}");
    }
}
