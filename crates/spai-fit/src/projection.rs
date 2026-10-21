//! Other fits acting on this one, turned into what the calculation already knows: their remote
//! modules and launched drones as projected items, their command bursts as fleet buffs.
//!
//! The projected modules are figured with the receiving fit's calculation, so the source pilot's
//! skills do not scale them; the bursts are figured with the source fit's own.

use crate::calc::Env;
use crate::dogma::{Dogma, EffectCategory};
use crate::fit::{Fit, Projected, State};
use crate::ids::TypeId;
use crate::skills::SkillSet;

/// Whether the type has an effect that reaches another ship.
fn reaches_out(dogma: &Dogma, type_id: TypeId) -> bool {
    dogma.type_(type_id).is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.category == EffectCategory::Target)))
}

/// The fit with every switched-on projected fit folded into its `projected` list, and the buffs
/// their bursts give. `lookup` finds a fit by library id.
pub fn expand(dogma: &Dogma, fit: &Fit, lookup: &dyn Fn(&str) -> Option<Fit>, skills: &SkillSet) -> (Fit, Env) {
    let mut out = fit.clone();
    let mut env = Env::default();
    for pf in fit.projected_fits.iter().filter(|p| p.active && p.count > 0) {
        let Some(src) = lookup(&pf.fit_id) else { continue };
        let src = crate::variations::resolve(dogma, &src);
        for m in src.modules.iter().filter(|m| m.state >= State::Active && reaches_out(dogma, m.type_id)) {
            out.projected.push(Projected { type_id: m.type_id, count: pf.count, state: m.state, charge: m.charge });
        }
        for d in src.drones.iter().filter(|d| d.in_space() > 0 && reaches_out(dogma, d.type_id)) {
            out.projected.push(Projected { type_id: d.type_id, count: d.in_space() * pf.count, state: State::Active, charge: None });
        }
        for (id, value) in crate::fleet::bursts_of(dogma, &src, skills) {
            env.offer(dogma, id, value);
        }
    }
    (out, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Calc;
    use crate::dogma::attr;
    use crate::fit::{Module, ProjectedFit, Slot};
    use crate::test_support::{close, dataset, ids};

    #[test]
    fn a_projected_fit_brings_its_webs_and_its_boosts_and_switched_off_brings_nothing() {
        let mut d = dataset();
        d.buffs.push(crate::dogma::BuffDef { id: 12, name: "Shield HP".into(), aggregate: crate::dogma::Aggregate::Max, op: crate::dogma::Op::PostPercent, item: vec![attr::ARMOR_HP], location: vec![], location_group: vec![], location_skill: vec![] });
        let d = d.finish();
        let mut support = Fit::new(ids::SHIP, "support");
        support.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
        support.modules.push(Module::new(ids::DC, Slot::Low, 0).with_state(State::Active));
        support.modules.push(Module::new(ids::BURST, Slot::High, 0).with_charge(ids::BURST_CHARGE).with_state(State::Active));
        let mut me = Fit::new(ids::SHIP, "me");
        me.projected_fits.push(ProjectedFit { fit_id: "support".into(), count: 2, active: true });
        let lookup = |id: &str| (id == "support").then(|| support.clone());
        let (fit, env) = expand(&d, &me, &lookup, &SkillSet::all_v());
        assert_eq!(fit.projected.len(), 1, "the web reaches out, the damage control does not: {:?}", fit.projected);
        assert_eq!(fit.projected[0].count, 2, "two ships of it, two webs");
        assert_eq!(env.buffs.len(), 1, "and its burst is a boost");
        let plain = Calc::new(&d, &me, &SkillSet::all_v(), &Env::default());
        let boosted = Calc::new(&d, &fit, &SkillSet::all_v(), &env);
        assert!(close(boosted.ship(attr::ARMOR_HP), plain.ship(attr::ARMOR_HP) * 1.25));
        assert!(boosted.ship(attr::MAX_VELOCITY) < plain.ship(attr::MAX_VELOCITY));
        me.projected_fits[0].active = false;
        let (off, env) = expand(&d, &me, &lookup, &SkillSet::all_v());
        assert!(off.projected.is_empty() && env.buffs.is_empty());
    }
}
