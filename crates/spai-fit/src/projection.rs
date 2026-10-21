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
    // Boost shortcuts: their bursts as any booster's, the strongest of each buff kept.
    for b in fit.boosts.iter().filter(|b| b.active) {
        if let Some((booster, pilot)) = crate::fleet::shortcut_booster(dogma, b) {
            for (id, value) in crate::fleet::bursts_of(dogma, &booster, &pilot) {
                env.offer(dogma, id, value);
            }
        }
    }
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

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_start_online_effects() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    for t in d.types.iter().filter(|t| t.published && t.name.contains("Assault Damage Control")).take(2) {
        let e: Vec<String> = t.effects.iter().filter_map(|(e, _)| d.effect(*e)).map(|x| format!("{}:{:?}", x.name, x.category)).collect();
        println!("{} [{}] {e:?}", t.name, d.groups.iter().find(|g| g.id == t.group).map(|g| g.name.clone()).unwrap_or_default());
    }
    for n in ["Covert Ops Cloaking Device II", "Improved Cloaking Device II", "Jump Portal Generator I", "Small Assault Damage Control I", "Gyrostabilizer II", "5MN Microwarpdrive II", "Siege Module II", "Bastion Module I", "Cynosural Field Generator I", "Interdiction Nullifier I", "500MN Microwarpdrive II", "Large Micro Jump Drive", "Damage Control II"] {
        let Some(t) = d.type_named(n) else { println!("no {n}"); continue };
        let g = d.groups.iter().find(|g| g.id == t.group).map(|g| g.name.clone()).unwrap_or_default();
        let e: Vec<String> = t.effects.iter().filter_map(|(e, _)| d.effect(*e)).map(|x| format!("{}:{:?}", x.name, x.category)).collect();
        println!("{n} [{g}] {e:?}");
    }
    use crate::formats::{can_activate, default_state};
    let id = |n: &str| d.type_named(n).unwrap().id;
    for (n, active, starts) in [("Gyrostabilizer II", false, State::Online), ("Damage Control II", false, State::Online), ("5MN Microwarpdrive II", true, State::Active), ("Covert Ops Cloaking Device II", true, State::Online), ("Jump Portal Generator I", true, State::Online), ("Assault Damage Control I", true, State::Online), ("Large Micro Jump Drive", true, State::Online), ("Siege Module II", true, State::Active)] {
        assert_eq!((can_activate(&d, id(n)), default_state(&d, id(n))), (active, starts), "{n}");
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_burst_shortcut_parts() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    for k in crate::fleet::BurstKind::ALL {
        let mods: Vec<&str> = d.types.iter().filter(|t| t.published && t.effects.iter().any(|(e, _)| d.effect(*e).is_some_and(|x| x.name == k.effect()))).map(|t| t.name.as_str()).collect();
        println!("{k:?}: {mods:?}");
    }
    let skills: Vec<&str> = d.types.iter().filter(|t| t.published && t.category == crate::ids::category::SKILL && { let n = t.name.to_lowercase(); n.contains("command") || n.contains("leadership") || n.contains("burst") || n.contains("foreman") }).map(|t| t.name.as_str()).collect();
    println!("skills {skills:?}");
    let links: Vec<&str> = d.types.iter().filter(|t| t.published && t.name.contains("Mindlink")).map(|t| t.name.as_str()).collect();
    println!("mindlinks {links:?}");
    let mut hulls = Vec::new();
    for tr in &d.traits {
        let all = tr.role.iter().chain(tr.misc.iter()).chain(tr.per_skill.iter().flat_map(|(_, b)| b.iter()));
        if all.into_iter().any(|b| b.text.contains("Command Burst") || b.text.contains("Mining Foreman Burst")) {
            if let Some(t) = d.type_(tr.type_id) { hulls.push(t.name.as_str()); }
        }
    }
    println!("{} bonused hulls e.g. {:?}", hulls.len(), &hulls[..hulls.len().min(40)]);
    let shield = d.types.iter().find(|t| t.name == "Shield Command Burst II").unwrap();
    let cg: Vec<f32> = shield.attrs.iter().filter(|(a, _)| d.attr(*a).is_some_and(|x| x.name.starts_with("chargeGroup"))).map(|(_, v)| *v).collect();
    let charges: Vec<&str> = d.types.iter().filter(|t| t.published && cg.contains(&(f32::from(t.group)))).map(|t| t.name.as_str()).collect();
    println!("shield charges {charges:?}");
    for tr in d.traits.iter().filter(|t| d.type_(t.type_id).is_some_and(|x| x.name == "Vulture" || x.name == "Erebus" || x.name == "Orca")) {
        let n = d.type_(tr.type_id).unwrap().name.clone();
        for x in tr.role.iter().chain(tr.misc.iter()) { if x.text.to_lowercase().contains("burst") { println!("ROLE {n} {:?}", x.text); } }
        for (s, b) in &tr.per_skill { for x in b { if x.text.to_lowercase().contains("burst") { println!("SKILL {n} {} {:?}", d.type_(*s).map(|t| t.name.as_str()).unwrap_or("?"), x.text); } } }
    }
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_boost_shortcuts() {
    use crate::fit::BoostShortcut;
    use crate::fleet::BurstKind;
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let vulture = d.type_named("Vulture").unwrap().id;
    let hulls = crate::fleet::boosting_hulls(&d, BurstKind::Shield);
    assert!(hulls.contains(&vulture) && !hulls.contains(&d.type_named("Rifter").unwrap().id));
    assert!(!crate::fleet::boosting_hulls(&d, BurstKind::Armor).contains(&vulture), "the Vulture boosts shield and information only");
    let strength = |b: BoostShortcut| {
        let mut me = Fit::new(d.type_named("Rifter").unwrap().id, "me");
        me.boosts.push(b);
        let (_, env) = expand(&d, &me, &|_| None, &SkillSet::all_v());
        let mut v: Vec<(u32, f64)> = env.buffs.iter().map(|b| (u32::from(b.id), b.value)).collect();
        v.sort_by_key(|(id, _)| *id);
        v
    };
    let plain = strength(BoostShortcut::default());
    let hull = strength(BoostShortcut { hull: Some(vulture), ..Default::default() });
    let linked = strength(BoostShortcut { hull: Some(vulture), mindlink: true, ..Default::default() });
    println!("plain {plain:?}\nvulture {hull:?}\nlinked {linked:?}");
    assert_eq!(plain.len(), hull.len());
    assert!(hull.iter().zip(&plain).all(|(h, p)| h.1.abs() > p.1.abs()), "the hull's bonus strengthens every buff");
    assert!(linked.iter().zip(&hull).all(|(l, h)| l.1.abs() > h.1.abs()), "and the mindlink on top");
    let charges = crate::fleet::burst_charges(&d, d.type_named("Shield Command Burst II").unwrap().id);
    let fewer = strength(BoostShortcut { charges_off: vec![charges[0]], ..Default::default() });
    assert!(fewer.len() < plain.len(), "a charge switched off gives its buffs no more");
    let low = strength(BoostShortcut { skills: vec![(d.type_named("Shield Command Specialist").unwrap().id, 0)], ..Default::default() });
    assert!(low.iter().zip(&plain).any(|(l, p)| l.1.abs() < p.1.abs()), "fewer skills, weaker bursts");
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_panic_module() {
    use crate::fit::{Module, Slot};
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let panic = d.types.iter().find(|t| t.name.contains("Pulse Activated Nexus")).unwrap();
    println!("{} group {}", panic.name, d.groups.iter().find(|g| g.id == panic.group).map(|g| g.name.as_str()).unwrap_or(""));
    for (e, _) in &panic.effects {
        let x = d.effect(*e).unwrap();
        println!("effect {} {:?} mods {}", x.name, x.category, x.modifiers.len());
        for m in &x.modifiers {
            println!("   {:?}", m);
        }
    }
    for (a, v) in &panic.attrs {
        println!("  attr {} = {v}", d.attr(*a).map(|x| x.name.as_str()).unwrap_or("?"));
    }
    for b in d.buffs.iter().filter(|b| { let n = b.name.to_lowercase(); n.contains("invuln") || n.contains("panic") || n.contains("industrial") || n.contains("nexus") }) {
        println!("buff {} {} {:?} items {:?} loc {:?}", b.id, b.name, b.op, b.item, b.location);
    }
    let rorq = d.type_named("Rorqual").unwrap().id;
    let ehp = |state| {
        let mut f = Fit::new(rorq, "r");
        f.modules.push(Module::new(panic.id, Slot::High, 0).with_state(state));
        let c = crate::stats::compute(&d, &f, &SkillSet::all_v(), &Env::default());
        (c.tank.ehp, c.tank.shield.resist(0), c.tank.shield.hp)
    };
    println!("online {:?} active {:?}", ehp(State::Online), ehp(State::Active));
    let tank = |state| {
        let mut f = Fit::new(rorq, "r");
        f.modules.push(Module::new(panic.id, Slot::High, 0).with_state(state));
        crate::stats::compute(&d, &f, &SkillSet::all_v(), &Env::default()).tank.invulnerable
    };
    assert!(!tank(State::Online) && tank(State::Active), "running, the PANIC marks the ship invulnerable");
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB and SPAI_EFT"]
fn real_data_panic_back_solve() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let text = std::fs::read_to_string(std::env::var("SPAI_EFT").unwrap()).unwrap();
    let fit = crate::formats::eft::import(&d, &text).unwrap().fit;
    let game = 4_905_113_104.0f64;
    for core in [State::Online, State::Active] {
        let mut f = fit.clone();
        for m in &mut f.modules {
            let n = d.type_(m.type_id).map(|t| t.name.clone()).unwrap_or_default();
            if n.contains("Industrial Core") { m.state = core; }
            if n.contains("Nexus") { m.state = State::Online; }
        }
        let c = crate::stats::compute(&d, &f, &SkillSet::all_v(), &Env::default());
        let t = &c.tank;
        let below = t.armor.ehp + t.hull.ehp;
        let k = t.shield.ehp / (game - below);
        println!("core {core:?}: shield hp {:.0} res {:?} shield ehp {:.0} armor+hull ehp {:.0} -> PANIC multiplies shield resonance by {k:.6} ({:.4}% resist on top)", t.shield.hp, (0..4).map(|i| (t.shield.resist(i) * 1000.0).round() / 10.0).collect::<Vec<_>>(), t.shield.ehp, below, (1.0 - k) * 100.0);
    }
    // With the core running, the fit comes out at the game's figure.
    let mut f = fit.clone();
    for m in &mut f.modules {
        if d.type_(m.type_id).is_some_and(|t| t.name.contains("Nexus")) {
            m.state = State::Active;
        }
    }
    let ehp = crate::stats::compute(&d, &f, &SkillSet::all_v(), &Env::default()).tank.ehp;
    println!("with PANIC: {ehp:.0}");
    assert!((ehp / game - 1.0).abs() < 0.001, "{ehp} against the game's {game}");
}

#[cfg(test)]
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_missile_step() {
    use crate::fit::{Module, Slot};
    use crate::graphs::{sample, GraphKind, GraphSpec};
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let mut f = Fit::new(d.type_named("Drake").unwrap().id, "d");
    f.modules.push(Module::new(d.type_named("Heavy Missile Launcher II").unwrap().id, Slot::High, 0).with_charge(d.type_named("Scourge Heavy Missile").unwrap().id).with_state(State::Active));
    let c = crate::stats::compute(&d, &f, &SkillSet::all_v(), &Env::default());
    let m = c.weapons[0].missile.clone().unwrap();
    println!("flight {} s, velocity {}, short {} long {}", m.flight_time_s, m.velocity, m.short_range(), m.long_range());
    let s = sample(&c, &GraphSpec::new(GraphKind::DamageVsDistance, crate::fit::TargetProfile { signature: 400.0, ..Default::default() }), None);
    let w = &s[0];
    let at = |x: f64| crate::graphs::value_at_for_test(&w.points, x);
    let (full, step) = (at(m.short_range() - 100.0), at(m.short_range() + 100.0));
    assert!(step > 0.0 && step < full, "between the whole seconds and one more, the share that flies on: {full} then {step}");
    assert!(((step / full) - m.flight_time_s.fract()).abs() < 1e-6);
    assert_eq!(at(m.long_range() + 100.0), 0.0);
}
