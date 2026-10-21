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

/// A titan's phenomena generator: no charge, its buffs written on the module itself.
const GENERATOR: &str = "moduleTitanEffectGenerator";

pub fn is_generator(dogma: &Dogma, type_id: TypeId) -> bool {
    dogma.type_(type_id).is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.name == GENERATOR)))
}

/// The buffs a phenomena generator gives, as the module carries them.
pub fn generator_buffs(dogma: &Dogma, type_id: TypeId) -> Vec<(crate::ids::BuffId, f64)> {
    use crate::dogma::attr;
    let Some(t) = dogma.type_(type_id) else { return Vec::new() };
    [
        (attr::WARFARE_BUFF_1_ID, attr::WARFARE_BUFF_1_VALUE),
        (attr::WARFARE_BUFF_2_ID, attr::WARFARE_BUFF_2_VALUE),
        (attr::WARFARE_BUFF_3_ID, attr::WARFARE_BUFF_3_VALUE),
        (attr::WARFARE_BUFF_4_ID, attr::WARFARE_BUFF_4_VALUE),
    ]
    .into_iter()
    .filter_map(|(id, value)| {
        let id = t.attr(id).filter(|v| *v > 0.0)?;
        Some((id as crate::ids::BuffId, f64::from(t.attr(value).unwrap_or(0.0))))
    })
    .collect()
}

/// Whether the type has an effect that reaches another ship.
fn reaches_out(dogma: &Dogma, type_id: TypeId) -> bool {
    dogma.type_(type_id).is_some_and(|t| t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.category == EffectCategory::Target)))
}

/// Whether projecting the fit would do anything: a running remote module or generator, a launched
/// drone that reaches out, or a command burst with a charge.
pub fn acts_on_others(dogma: &Dogma, fit: &Fit) -> bool {
    let fit = crate::variations::resolve(dogma, fit);
    fit.modules.iter().any(|m| m.state >= State::Active && (reaches_out(dogma, m.type_id) || is_generator(dogma, m.type_id)))
        || fit.drones.iter().any(|d| d.in_space() > 0 && reaches_out(dogma, d.type_id))
        || !crate::fleet::bursts_of(dogma, &fit, &SkillSet::all_v()).is_empty()
}

/// The fit with every switched-on projected fit folded into its `projected` list, and the buffs
/// their bursts give. `lookup` finds a fit by library id.
pub fn expand(dogma: &Dogma, fit: &Fit, lookup: &dyn Fn(&str) -> Option<Fit>, skills: &SkillSet) -> (Fit, Env) {
    let mut out = fit.clone();
    let mut env = Env::default();
    // A generator acts through the system's buffs, not as a module on the ship; off is off.
    for p in fit.projected.iter().filter(|p| p.state != State::Offline && is_generator(dogma, p.type_id)) {
        for (id, value) in generator_buffs(dogma, p.type_id) {
            env.offer(dogma, id, value);
        }
    }
    out.projected.retain(|p| !is_generator(dogma, p.type_id));
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
        for m in src.modules.iter().filter(|m| m.state >= State::Active && is_generator(dogma, m.type_id)) {
            for (id, value) in generator_buffs(dogma, m.type_id) {
                env.offer(dogma, id, value);
            }
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
        assert!(acts_on_others(&d, &support));
        assert!(!acts_on_others(&d, &Fit::new(ids::SHIP, "plain")), "a fit with nothing that reaches out is not offered");
    }

    #[test]
    fn a_phenomena_generator_gives_its_buffs_alone_or_from_a_titan_fit() {
        let mut d = dataset();
        d.buffs.push(crate::dogma::BuffDef { id: 12, name: "Armor HP".into(), aggregate: crate::dogma::Aggregate::Max, op: crate::dogma::Op::PostPercent, item: vec![attr::ARMOR_HP], location: vec![], location_group: vec![], location_skill: vec![] });
        let d = d.finish();
        let mut me = Fit::new(ids::SHIP, "me");
        me.projected.push(crate::fit::Projected { type_id: ids::GENERATOR, ..Default::default() });
        let (fit, env) = expand(&d, &me, &|_| None, &SkillSet::all_v());
        assert!(fit.projected.is_empty(), "the generator is a buff, not an item on the ship");
        assert_eq!(env.buffs.len(), 1);
        let plain = Calc::new(&d, &Fit::new(ids::SHIP, "x"), &SkillSet::all_v(), &Env::default());
        let boosted = Calc::new(&d, &fit, &SkillSet::all_v(), &env);
        assert!(close(boosted.ship(attr::ARMOR_HP), plain.ship(attr::ARMOR_HP) * 1.15));
        me.projected[0].state = State::Offline;
        assert!(expand(&d, &me, &|_| None, &SkillSet::all_v()).1.buffs.is_empty(), "switched off");
        let mut titan = Fit::new(ids::SHIP, "titan");
        titan.modules.push(Module::new(ids::GENERATOR, Slot::High, 0).with_state(State::Active));
        let mut me = Fit::new(ids::SHIP, "me");
        me.projected_fits.push(ProjectedFit { fit_id: "titan".into(), count: 1, active: true });
        let (_, env) = expand(&d, &me, &|id| (id == "titan").then(|| titan.clone()), &SkillSet::all_v());
        assert_eq!(env.buffs.len(), 1, "the titan's running generator reaches the fit it is projected onto");
    }
}

/// What the real data offers as projected effects. `SPAI_DOGMA_BLOB=<path> cargo test -p spai-fit
/// real_data_has_weather -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_has_weather_and_generators() {
    let path = std::env::var("SPAI_DOGMA_BLOB").expect("SPAI_DOGMA_BLOB");
    let d = crate::dogma::blob::decode(&std::fs::read(path).unwrap()).unwrap();
    let beacons: Vec<&str> = d.types.iter().filter(|t| t.group == crate::ids::group::EFFECT_BEACON).map(|t| t.name.as_str()).collect();
    let generators: Vec<&str> = d.types.iter().filter(|t| is_generator(&d, t.id)).map(|t| t.name.as_str()).collect();
    println!("{} beacons, e.g. {:?}", beacons.len(), &beacons[..beacons.len().min(8)]);
    println!("{} generators: {:?}", generators.len(), generators);
    let with_effects = d.types.iter().filter(|t| t.group == crate::ids::group::EFFECT_BEACON && !t.effects.is_empty()).count();
    println!("{with_effects} beacons carry effects");
    assert!(!beacons.is_empty() && !generators.is_empty());
}
