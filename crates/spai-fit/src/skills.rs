//! A pilot's skill levels, and what a fit asks for that the pilot lacks.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::dogma::Dogma;
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
}
