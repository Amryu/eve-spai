use super::*;
use super::tests_support::{close, dataset, effect, ids::*, modifier, type_def};
use crate::fit::{Booster, DroneStack, Module, Projected};

fn fit() -> Fit {
    let mut f = Fit::new(SHIP, "test");
    f.modules.push(Module::new(GUN, Slot::High, 0).with_charge(AMMO).with_state(State::Active));
    f.modules.push(Module::new(DC, Slot::Low, 0).with_state(State::Active));
    f
}

#[test]
fn skill_levels_scale_ship_bonuses_that_reach_the_right_modules() {
    let d = dataset();
    let c = Calc::new(&d, &fit(), &SkillSet::all_v(), &Env::default());
    let gun = c.module(Slot::High, 0).unwrap();
    assert!(close(c.value(gun, attr::FALLOFF), 6000.0 * 1.375), "7.5% × 5 levels = 37.5% falloff");
    assert!(close(c.value(gun, attr::DAMAGE_MULTIPLIER), 3.0 * 1.05), "the turret skill adds 1% per level through its own type");
    let c = Calc::new(&d, &fit(), &SkillSet::from_esi([(SKILL_MF, 2), (SKILL_SPT, 0)]), &Env::default());
    let gun = c.module(Slot::High, 0).unwrap();
    assert!(close(c.value(gun, attr::FALLOFF), 6000.0 * 1.15));
    assert!(close(c.value(gun, attr::DAMAGE_MULTIPLIER), 3.0));
    assert!(close(c.ship(attr::MAX_VELOCITY), 365.0), "a System-category effect never runs");
}

#[test]
fn a_charge_reaches_its_module_through_other_id() {
    let d = dataset();
    let c = Calc::new(&d, &fit(), &SkillSet::all_v(), &Env::default());
    let gun = c.module(Slot::High, 0).unwrap();
    assert!(close(c.value(gun, attr::MAX_RANGE), 600.0));
    let mut f = fit();
    f.modules[0].charge = None;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(close(c.value(c.module(Slot::High, 0).unwrap(), attr::MAX_RANGE), 1200.0));
}

#[test]
fn resistance_modules_stack_as_in_eve() {
    let d = dataset();
    let mut f = fit();
    f.modules.push(Module::new(HARDENER, Slot::Low, 1).with_state(State::Active));
    f.modules.push(Module::new(HARDENER, Slot::Low, 2).with_state(State::Active));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let want = 0.4 * 0.85 * 0.7 * (1.0 - 0.3 * PENALTY_FACTOR);
    assert!(close(c.ship(attr::ARMOR_EM_RESONANCE), want), "DC in its own PreMul group, hardeners penalised among themselves");
    f.modules[2].state = State::Overload;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let want = 0.4 * 0.85 * (1.0 - 0.36) * (1.0 - 0.3 * PENALTY_FACTOR);
    assert!(close(c.ship(attr::ARMOR_EM_RESONANCE), want), "the overloaded hardener is 20% stronger and sorts first");
    f.modules[1].state = State::Offline;
    f.modules[2].state = State::Online;
    f.modules[3].state = State::Online;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(close(c.ship(attr::ARMOR_EM_RESONANCE), 0.4), "an offline DC and online-only hardeners do nothing");
}

#[test]
fn a_propulsion_module_adds_mass_signature_and_speed_while_active() {
    let d = dataset();
    let mut f = fit();
    f.modules.push(Module::new(MWD, Slot::Mid, 0).with_state(State::Online));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(close(c.ship(attr::MAX_VELOCITY), 365.0));
    assert!(close(c.ship(attr::MASS), 1_067_000.0));
    f.modules[2].state = State::Active;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let mass = 1_067_000.0 + 500_000.0;
    assert!(close(c.ship(attr::MASS), mass));
    assert!(close(c.ship(attr::SIGNATURE_RADIUS), 35.0 * 6.0));
    let want = 365.0 * (1.0 + 500.0 * 1_500_000.0 / mass / 100.0);
    assert!(close(c.ship(attr::MAX_VELOCITY), want), "{} vs {want}", c.ship(attr::MAX_VELOCITY));
    f.modules[2].state = State::Overload;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let want = 365.0 * (1.0 + 750.0 * 1_500_000.0 / mass / 100.0);
    assert!(close(c.ship(attr::MAX_VELOCITY), want));
}

#[test]
fn implants_are_exempt_from_the_penalty_and_booster_side_effects_are_opt_in() {
    let d = dataset();
    let mut f = fit();
    f.implants.push(IMPLANT);
    f.modules.push(Module::new(MWD, Slot::Mid, 0).with_state(State::Active));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let mods = c.modifiers_of(c.ship_idx(), attr::MAX_VELOCITY);
    assert_eq!(mods.iter().filter(|(_, _, p)| !*p).count(), 1, "the implant's 5% is unpenalised");
    assert_eq!(mods.iter().filter(|(_, _, p)| *p).count(), 1, "the MWD's bonus is penalised");
    f.boosters.push(Booster { type_id: BOOSTER, side_effects: false });
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert_eq!(c.modifiers_of(c.ship_idx(), attr::MAX_VELOCITY).len(), 2);
    f.boosters[0].side_effects = true;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert_eq!(c.modifiers_of(c.ship_idx(), attr::MAX_VELOCITY).len(), 3, "the side effect's velocity penalty joins in");
}

#[test]
fn owner_required_skill_modifiers_reach_drones_and_location_group_modifiers_their_group() {
    let d = dataset();
    let mut f = fit();
    f.drones.push(DroneStack { type_id: DRONE, count: 3, active: true });
    f.boosters.push(Booster { type_id: BOOSTER, side_effects: false });
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let (drone, _) = c.drones().next().unwrap();
    assert!(close(c.value(drone, attr::DAMAGE_MULTIPLIER), 1.1), "Drones skill: +10% through the owner");
    let gun = c.module(Slot::High, 0).unwrap();
    assert!(close(c.value(gun, attr::ARMOR_DAMAGE_AMOUNT), 0.0), "the booster's group filter leaves the gun alone");
}

#[test]
fn a_projected_module_reaches_the_ship_and_nothing_else() {
    let d = dataset();
    let mut f = fit();
    f.projected.push(Projected { type_id: WEB, ..Projected::default() });
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(close(c.ship(attr::MAX_VELOCITY), 365.0 * 0.4));
    f.projected[0].state = State::Online;
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(close(c.ship(attr::MAX_VELOCITY), 365.0));
}

#[test]
fn fleet_buffs_apply_penalised_and_the_strongest_wins() {
    let mut d = dataset();
    d.buffs.push(crate::dogma::BuffDef { id: 12, name: "Shield HP".into(), aggregate: Aggregate::Max, op: Op::PostPercent, item: vec![attr::ARMOR_HP], location: vec![], location_group: vec![], location_skill: vec![] });
    let d = d.finish();
    let mut env = Env::default();
    env.offer(&d, 12, 10.0);
    env.offer(&d, 12, 15.0);
    env.offer(&d, 12, 12.0);
    assert_eq!(env.buffs, vec![ActiveBuff { id: 12, value: 15.0 }]);
    let mut f = fit();
    f.modules.push(Module::new(RIG, Slot::Rig, 0));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &env);
    let want = 350.0 * 1.2 * (1.0 + 0.15 * PENALTY_FACTOR);
    assert!(close(c.ship(attr::ARMOR_HP), want), "{} vs {want}: the rig's 20% and the buff's 15% are penalised together", c.ship(attr::ARMOR_HP));
}

#[test]
fn unknown_types_are_reported_and_left_out() {
    let d = dataset();
    let mut f = fit();
    f.modules.push(Module::new(999_999, Slot::Low, 3));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert_eq!(c.unknown_types, vec![999_999]);
    assert!(c.module(Slot::Low, 3).is_none());
    assert!(close(c.ship(attr::ARMOR_EM_RESONANCE), 0.4 * 0.85));
}

#[test]
fn clamps_follow_the_attribute_definitions_and_cycles_fall_back_to_base() {
    let mut d = dataset();
    d.attributes.iter_mut().find(|a| a.id == attr::FALLOFF).unwrap().max_attr = Some(attr::MAX_RANGE);
    let d = d.finish();
    let c = Calc::new(&d, &fit(), &SkillSet::all_v(), &Env::default());
    let gun = c.module(Slot::High, 0).unwrap();
    assert!(close(c.value(gun, attr::FALLOFF), 600.0), "falloff capped at the (ammo-halved) optimal");
    let mut d = dataset();
    d.effects.push(effect(400, "loop", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, attr::MAX_VELOCITY, Op::PostPercent)]));
    d.types.iter_mut().find(|t| t.id == SHIP).unwrap().effects.push((400, false));
    let d = d.finish();
    let c = Calc::new(&d, &fit(), &SkillSet::all_v(), &Env::default());
    let v = c.ship(attr::MAX_VELOCITY);
    assert!(v.is_finite() && close(v, 365.0 * (1.0 + 3.65)), "{v}: the self-reference reads the base value");
}

#[test]
fn a_fit_of_an_unknown_hull_still_computes() {
    let d = dataset();
    let c = Calc::new(&d, &Fit::new(1, "ghost"), &SkillSet::all_v(), &Env::default());
    assert_eq!(c.unknown_types, vec![1]);
    assert_eq!(c.ship(attr::MAX_VELOCITY), 0.0);
}

#[test]
fn skills_whose_bonus_effects_are_empty_in_the_data_still_apply() {
    let d = dataset();
    let mut f = fit();
    f.modules.push(Module::new(LAUNCHER, Slot::High, 1).with_charge(MISSILE).with_state(State::Active));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let launcher = c.module(Slot::High, 1).unwrap();
    let missile = c.items()[launcher].charge.unwrap();
    assert!(close(c.value(missile, attr::KINETIC_DAMAGE), 300.0 * 1.25), "Cruise Missiles V: +25% damage");
    assert!(close(c.value(missile, attr::EM_DAMAGE), 0.0));
    assert!(close(c.value(launcher, attr::SPEED), 10_000.0 * 0.9), "specialisation V: 10% faster");
    let c = Calc::new(&d, &f, &SkillSet::from_esi([(SKILL_CRUISE, 2)]), &Env::default());
    let missile = c.items()[c.module(Slot::High, 1).unwrap()].charge.unwrap();
    assert!(close(c.value(missile, attr::KINETIC_DAMAGE), 300.0 * 1.10));
}

#[test]
fn subsystems_add_their_slots_and_hardpoints_to_the_hull() {
    let d = dataset();
    let mut f = fit();
    f.modules.push(Module::new(SUBSYSTEM, Slot::Subsystem, 0));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(close(c.ship(attr::HI_SLOTS), 5.0));
    assert!(close(c.ship(attr::TURRET_SLOTS_LEFT), 6.0));
    assert!(close(c.ship(attr::MED_SLOTS), 3.0));
}

#[test]
fn the_pilot_node_carries_the_character_types_attributes_when_present() {
    let d = dataset();
    let c = Calc::new(&d, &fit(), &SkillSet::all_v(), &Env::default());
    assert!(close(c.char(attr::DRONE_CONTROL_DISTANCE), 20_000.0));
    assert!(close(c.char(attr::MAX_LOCKED_TARGETS), 2.0));
    assert!(c.unknown_types.is_empty());
    let mut plain = dataset();
    plain.types.retain(|t| t.id != crate::ids::CHARACTER_TYPE);
    let plain = plain.finish();
    let c = Calc::new(&plain, &fit(), &SkillSet::all_v(), &Env::default());
    assert!(c.unknown_types.is_empty(), "a dataset without it is not an error");
    assert!(close(c.char(attr::DRONE_CONTROL_DISTANCE), 20_000.0), "the attribute default stands in");
    let _ = type_def(1, "unused", 1, 1, &[], &[], &[]);
}

/// `SPAI_DOGMA_BLOB=path/to/dogma-v1.bin.gz`: the named attributes and the patched effects exist
/// in the real data under the names the engine expects.
#[test]
#[ignore]
fn the_real_blob_agrees_with_the_named_attributes() {
    let path = std::env::var("SPAI_DOGMA_BLOB").expect("SPAI_DOGMA_BLOB");
    let d = crate::dogma::blob::decode(&std::fs::read(path).unwrap()).unwrap();
    for (id, name) in attr::NAMED {
        let a = d.attr(*id).unwrap_or_else(|| panic!("attribute {id} ({name}) missing"));
        assert_eq!(a.name, *name, "attribute {id}");
    }
    for name in ["moduleBonusAfterburner", "moduleBonusMicrowarpdrive", "online", "skillEffect"] {
        assert!(d.effect_named(name).is_some(), "{name}");
    }
    let rifter = d.types.iter().find(|t| t.name == "Rifter").unwrap();
    let mut f = Fit::new(rifter.id, "Rifter");
    let mwd = d.types.iter().find(|t| t.name == "5MN Microwarpdrive II").unwrap();
    f.modules.push(Module::new(mwd.id, Slot::Mid, 0).with_state(State::Active));
    let started = std::time::Instant::now();
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let v = c.ship(attr::MAX_VELOCITY);
    eprintln!("Rifter + 5MN MWD II at all V: {v:.1} m/s in {:?}", started.elapsed());
    assert!(v > 2000.0 && v < 4000.0, "{v}");
    assert!(c.ship(attr::SIGNATURE_RADIUS) > 150.0);
    assert!(close(c.char(attr::DRONE_CONTROL_DISTANCE), 20_000.0 + 5.0 * 5_000.0 + 5.0 * 3_000.0), "{}: Drone Avionics and Advanced Drone Avionics at V", c.char(attr::DRONE_CONTROL_DISTANCE));
    let cruise = d.types.iter().find(|t| t.name == "Scourge Cruise Missile").unwrap();
    let launcher = d.types.iter().find(|t| t.name == "Cruise Missile Launcher II").unwrap();
    let mut f = Fit::new(rifter.id, "Rifter");
    f.modules.push(Module::new(launcher.id, Slot::High, 0).with_charge(cruise.id).with_state(State::Active));
    let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let l = c.module(Slot::High, 0).unwrap();
    let m = c.items()[l].charge.unwrap();
    let base = f64::from(cruise.attr(attr::KINETIC_DAMAGE).unwrap());
    let want = base * 1.25 * 1.10;
    assert!(close(c.value(m, attr::KINETIC_DAMAGE), want), "{} vs {want}: Cruise Missiles V and Warhead Upgrades V", c.value(m, attr::KINETIC_DAMAGE));
    let rof = c.value(l, attr::SPEED) / f64::from(launcher.attr(attr::SPEED).unwrap());
    let want = 0.9 * 0.85 * 0.9;
    assert!(close(rof, want), "{rof} vs {want}: Missile Launcher Operation, Rapid Launch and the specialisation at V");
}
