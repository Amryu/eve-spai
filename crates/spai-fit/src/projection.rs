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
        let Some(mut src) = lookup(&pf.fit_id) else { continue };
        // The variation picked for it, or its base fit: not whatever was last switched on there.
        src.active_variations = pf.variation.filter(|v| src.variations.iter().any(|x| x.id == *v)).into_iter().collect();
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
    fn a_projected_fit_flies_its_base_unless_a_variation_is_picked() {
        use crate::variations::{RefitOp, Variation};
        let d = dataset();
        let mut support = Fit::new(ids::SHIP, "support");
        support.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Online));
        support.variations.push(Variation { id: 7, name: "Webbing".into(), ops: vec![RefitOp::SetState { slot: Slot::Mid, index: 0, state: State::Active }], ..Default::default() });
        // Switched on in its own tab: that does not decide how it flies here.
        support.active_variations = vec![7];
        let lookup = |id: &str| (id == "support").then(|| support.clone());
        let mut me = Fit::new(ids::SHIP, "me");
        me.projected_fits.push(ProjectedFit { fit_id: "support".into(), ..Default::default() });
        assert!(expand(&d, &me, &lookup, &SkillSet::all_v()).0.projected.is_empty(), "the base fit's web is only online");
        me.projected_fits[0].variation = Some(7);
        assert_eq!(expand(&d, &me, &lookup, &SkillSet::all_v()).0.projected.len(), 1, "the picked variation switches it on");
    }

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
        me.projected_fits.push(ProjectedFit { fit_id: "support".into(), count: 2, active: true, variation: None });
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
        me.projected_fits.push(ProjectedFit { fit_id: "titan".into(), count: 1, active: true, variation: None });
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

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_hold_attributes() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    for a in &d.attributes {
        let n = a.name.to_lowercase();
        if (n.contains("hold") || n.contains("bay") || n.contains("hangar")) && n.contains("capacity") {
            let ships = d.types.iter().filter(|t| t.category == crate::ids::category::SHIP && t.attr(a.id).is_some_and(|v| v > 0.0)).count();
            println!("{} {} {:?} ships={ships}", a.id, a.name, a.display);
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_asb_charges() {
    use crate::dogma::attr;
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    for name in ["X-Large Ancillary Shield Booster", "Large Ancillary Shield Booster", "Medium Ancillary Shield Booster", "Heavy Capacitor Booster II"] {
        let Some(m) = d.type_named(name) else { println!("no {name}"); continue };
        println!("{name}: capacity {:?} chargeSize {:?} groups {:?}", m.attr(attr::CAPACITY), m.attr(128), [604, 605, 606, 609, 610].iter().map(|a| m.attr(*a)).collect::<Vec<_>>());
    }
    for t in d.types.iter().filter(|t| t.name.contains("Cap Booster") && t.published) {
        println!("  {} group {} volume {:?} chargeSize {:?}", t.name, t.group, t.attr(attr::VOLUME), t.attr(128));
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_booster_side_effects() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    for name in ["Strong Blue Pill Booster", "Strong Exile Booster", "Improved Crash Booster"] {
        let Some(t) = d.type_named(name) else { println!("no {name}"); continue };
        println!("{name}");
        for (e, _) in &t.effects {
            let Some(def) = d.effect(*e) else { continue };
            if def.usage_chance_attr.is_none() {
                continue;
            }
            println!("  effect {} {} chance attr {:?}", e, def.name, def.usage_chance_attr.and_then(|a| d.attr(a)).map(|a| a.name.clone()));
            for m in &def.modifiers {
                println!("    {:?} {:?} modified {} by {} ({:?}) group {:?} skill {:?}", m.domain, m.func, d.attr(m.modified).map_or("?".into(), |a| format!("{}|{}", a.name, a.display)), d.attr(m.modifying).map_or("?".into(), |a| a.name.clone()), t.attr(m.modifying), m.group, m.skill);
            }
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_mutaplasmids() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    println!("{} mutaplasmids", d.mutaplasmids.len());
    for m in d.mutaplasmids.iter().take(3) {
        println!("  {} {} attrs {} maps {}", m.id, m.name, m.attrs.len(), m.mapping.len());
    }
    if let Some(mwd) = d.type_named("50MN Microwarpdrive II") {
        let names: Vec<&str> = d.mutaplasmids_for(mwd.id).iter().map(|m| m.name.as_str()).collect();
        println!("50MN MWD II takes {names:?}");
    }
    if let Some(drone) = d.type_named("Hammerhead II") {
        println!("Hammerhead II takes {}", d.mutaplasmids_for(drone.id).len());
    }
    println!("meta groups {:?}", d.meta_groups.iter().map(|m| (m.id, m.name.as_str())).collect::<Vec<_>>());
    assert!(d.mutaplasmids.iter().all(|m| !m.name.starts_with("Mutaplasmid ")), "every one named");
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_a_projected_web_slows_the_ship() {
    use crate::fit::{Module, ProjectedFit, Slot};
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let rifter = d.type_named("Rifter").unwrap().id;
    let web = d.type_named("Stasis Webifier II").unwrap().id;
    let mut webber = Fit::new(rifter, "Webber");
    webber.modules.push(Module::new(web, Slot::Mid, 0).with_state(State::Active));
    let mut me = Fit::new(rifter, "Me");
    let skills = SkillSet::all_v();
    let alone = crate::stats::compute(&d, &me, &skills, &Env::default()).mobility.max_velocity;
    me.projected_fits.push(ProjectedFit { fit_id: "w".into(), count: 1, active: true, variation: None });
    let lookup = |id: &str| (id == "w").then(|| webber.clone());
    let (expanded, env) = expand(&d, &me, &lookup, &skills);
    println!("projected: {:?}", expanded.projected);
    let webbed = crate::stats::compute(&d, &expanded, &skills, &env).mobility.max_velocity;
    let effects: Vec<String> = d.type_(web).unwrap().effects.iter().filter_map(|(e, _)| d.effect(*e).map(|x| format!("{} cat {:?} mods {}", x.name, x.category, x.modifiers.len()))).collect();
    println!("web effects {effects:?}; {alone} -> {webbed}");
    assert!(webbed < alone * 0.6, "a web halves the speed: {alone} -> {webbed}");
    let painter = d.type_named("Target Painter II").unwrap().id;
    let td = d.type_named("Tracking Disruptor II").unwrap().id;
    let gun = d.type_named("200mm AutoCannon II").unwrap().id;
    webber.modules.push(Module::new(painter, Slot::Mid, 1).with_state(State::Active));
    webber.modules.push(Module::new(td, Slot::Mid, 2).with_state(State::Active));
    me.modules.push(Module::new(gun, Slot::High, 0).with_state(State::Active).with_charge(d.type_named("EMP S").unwrap().id));
    let plain = crate::stats::compute(&d, &Fit { projected_fits: vec![], ..me.clone() }, &skills, &Env::default());
    let lookup = |id: &str| (id == "w").then(|| webber.clone());
    let (expanded, env) = expand(&d, &me, &lookup, &skills);
    let hit = crate::stats::compute(&d, &expanded, &skills, &env);
    println!("sig {} -> {}, optimal {} -> {}", plain.mobility.signature, hit.mobility.signature, plain.weapons[0].optimal, hit.weapons[0].optimal);
    assert!(hit.mobility.signature > plain.mobility.signature * 1.2, "painted");
    assert!(hit.weapons[0].optimal < plain.weapons[0].optimal * 0.9, "disrupted");
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_empty_target_effects() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let mut seen = std::collections::BTreeMap::new();
    for t in d.types.iter().filter(|t| t.published && matches!(t.category, crate::ids::category::MODULE | crate::ids::category::DRONE)) {
        for (e, _) in &t.effects {
            let Some(x) = d.effect(*e) else { continue };
            if x.category == crate::dogma::EffectCategory::Target && x.modifiers.is_empty() {
                seen.entry(x.name.clone()).or_insert_with(|| (t.name.clone(), 0)).1 += 1;
            }
        }
    }
    for (n, (ex, k)) in seen {
        println!("{n}: {k} types, e.g. {ex}");
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_escape_bay() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let a = d.attr_named("frigateEscapeBayCapacity").unwrap();
    println!("unit {:?} default {}", a.unit, a.default);
    for t in d.types.iter().filter(|t| t.attr(a.id).is_some_and(|v| v > 0.0)).take(5) {
        println!("{} = {:?}", t.name, t.attr(a.id));
    }
    for n in ["nullifier", "warp core", "interdiction nullifier"] {
        for x in d.attributes.iter().filter(|x| x.name.to_lowercase().contains(&n.replace(' ', ""))).take(5) {
            println!("attr {} {}", x.id, x.name);
        }
    }
    for x in d.attributes.iter().filter(|x| { let n = x.name.to_lowercase(); n.contains("escape") || n.contains("canfitship") || n.contains("fitsto") }) {
        let n = d.types.iter().filter(|t| t.attr(x.id).is_some()).count();
        println!("attr {} {} on {n} types", x.id, x.name);
    }
    let null = d.types.iter().find(|t| t.group == 4117 && t.published).unwrap();
    println!("nullifier {}: {:?}", null.name, null.attrs.iter().filter(|(a, _)| d.attr(*a).is_some_and(|x| x.name.to_lowercase().contains("canfit"))).map(|(a, v)| (d.attr(*a).unwrap().name.clone(), *v)).collect::<Vec<_>>());
    for g in d.groups.iter().filter(|g| g.name.to_lowercase().contains("nullifier")) {
        println!("group {} {}", g.id, g.name);
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_escape_bay_hulls() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let id = |n: &str| d.type_named(n).unwrap().id;
    for (hull, fits) in [("Rifter", true), ("Wolf", true), ("Hound", true), ("Hawk", true), ("Kestrel", true), ("Malediction", false), ("Helios", false), ("Thrasher", false), ("Sabre", false), ("Raven", false), ("Hecate", false)] {
        assert_eq!(crate::stats::fits_escape_bay(&d, id(hull)), fits, "{hull}");
    }
    let n = d.types.iter().filter(|t| t.published && crate::stats::fits_escape_bay(&d, t.id)).count();
    println!("{n} hulls fit an escape bay");
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_burnout_depends_on_the_rack() {
    use crate::fit::{Module, Slot};
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    for n in ["heatAbsorbtionRateModifier", "heatGenerationMultiplier", "heatAttenuationHi", "heatAttenuationMed", "heatAttenuationLow", "hp", "heatDamage", "hiSlots", "rigSlots"] {
        assert!(d.attr_named(n).is_some(), "{n}");
    }
    let rifter = d.type_named("Rifter").unwrap().id;
    let gun = d.type_named("200mm AutoCannon II").unwrap().id;
    let emp = d.type_named("EMP S").unwrap().id;
    let burn = |overloaded: usize| {
        let mut f = Fit::new(rifter, "x");
        for i in 0..3 {
            f.modules.push(Module::new(gun, Slot::High, i).with_charge(emp).with_state(if (i as usize) < overloaded { State::Overload } else { State::Active }));
        }
        let c = crate::stats::compute(&d, &f, &SkillSet::all_v(), &Env::default());
        let calc = crate::calc::Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
        crate::stats::module_stats(&calc, &c).into_iter().find(|m| m.slot == Slot::High && m.index == 0).and_then(|m| m.burnout_s).unwrap()
    };
    let (one, three) = (burn(1), burn(3));
    println!("one overloaded {one:.0}s, all three {three:.0}s");
    assert!(three < one * 0.6, "three heating the rack burn it much sooner: {one} against {three}");
}
