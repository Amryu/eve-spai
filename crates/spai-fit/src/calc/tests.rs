use super::*;
use crate::dogma::{AttrDef, EffectDef, GroupDef, Modifier, TypeDef};
use crate::fit::{Booster, DroneStack, Module, Projected};

fn attr_def(id: AttrId, name: &str, stackable: bool, high_is_good: bool, default: f32) -> AttrDef {
    AttrDef { id, name: name.into(), display: String::new(), stackable, high_is_good, default, unit: None, min_attr: None, max_attr: None, category: None, published: true }
}

fn effect(id: EffectId, name: &str, category: EffectCategory, modifiers: Vec<Modifier>) -> EffectDef {
    EffectDef {
        id,
        name: name.into(),
        category,
        offensive: false,
        assistance: false,
        duration_attr: None,
        range_attr: None,
        falloff_attr: None,
        tracking_attr: None,
        discharge_attr: None,
        usage_chance_attr: None,
        resistance_attr: None,
        modifiers,
    }
}

fn modifier(domain: Domain, func: Func, modified: AttrId, modifying: AttrId, op: Op) -> Modifier {
    Modifier { domain, func, modified, modifying, op, group: None, skill: None }
}

fn type_def(id: TypeId, name: &str, group: GroupId, category: CategoryId, attrs: &[(AttrId, f32)], effects: &[EffectId], skills: &[(TypeId, u8)]) -> TypeDef {
    let mut attrs = attrs.to_vec();
    for base in [attr::MASS, attr::CAPACITY, attr::VOLUME, attr::RADIUS] {
        if !attrs.iter().any(|(a, _)| *a == base) {
            attrs.push((base, 0.0));
        }
    }
    attrs.sort_by_key(|(a, _)| *a);
    TypeDef {
        id,
        name: name.into(),
        group,
        category,
        market_group: None,
        meta_group: None,
        meta_level: None,
        tech: None,
        variation_parent: None,
        published: true,
        attrs,
        effects: effects.iter().map(|e| (*e, false)).collect(),
        required_skills: skills.to_vec(),
    }
}

const SHIP: TypeId = 587;
const SKILL_MF: TypeId = 3329;
const SKILL_SPT: TypeId = 3300;
const SKILL_DRONES: TypeId = 3436;
const GUN: TypeId = 2881;
const AMMO: TypeId = 185;
const DC: TypeId = 2048;
const HARDENER: TypeId = 11269;
const MWD: TypeId = 434;
const IMPLANT: TypeId = 10226;
const BOOSTER: TypeId = 28672;
const DRONE: TypeId = 2454;
const WEB: TypeId = 526;
const RIG: TypeId = 31055;
const SKILL_CRUISE: TypeId = 3326;
const SKILL_CRUISE_SPEC: TypeId = 20212;
const LAUNCHER: TypeId = 19739;
const MISSILE: TypeId = 2514;
const SUBSYSTEM: TypeId = 45586;

const SHIP_BONUS_MF: AttrId = 587;
const DRONE_DAMAGE_BONUS: AttrId = 1255;
const WEB_SPEED_FACTOR: AttrId = 20;
const BOOSTER_SIDE: AttrId = 1223;
const USAGE_CHANCE: AttrId = 1287;

/// A frigate, its skills and a rack of modules whose effects cover every modifier route.
fn dataset() -> Dogma {
    let types = vec![
        type_def(SHIP, "Rifter", 25, 6, &[(attr::MAX_VELOCITY, 365.0), (attr::MASS, 1_067_000.0), (attr::SIGNATURE_RADIUS, 35.0), (attr::ARMOR_EM_RESONANCE, 0.4), (SHIP_BONUS_MF, 7.5), (attr::LOW_SLOTS, 4.0), (attr::ARMOR_HP, 350.0)], &[101, 102], &[(SKILL_MF, 1)]),
        type_def(SKILL_MF, "Minmatar Frigate", 257, 16, &[], &[201], &[]),
        type_def(SKILL_SPT, "Small Projectile Turret", 255, 16, &[], &[202], &[]),
        type_def(SKILL_DRONES, "Drones", 273, 16, &[], &[203], &[]),
        type_def(GUN, "200mm AutoCannon II", 55, 7, &[(attr::FALLOFF, 6000.0), (attr::MAX_RANGE, 1200.0), (attr::DAMAGE_MULTIPLIER, 3.0), (attr::SPEED, 3000.0)], &[301], &[(SKILL_SPT, 1)]),
        type_def(AMMO, "EMP S", 83, 8, &[(120, 0.5), (attr::EM_DAMAGE, 9.0)], &[302], &[]),
        type_def(DC, "Damage Control II", 60, 7, &[(attr::ARMOR_EM_RESONANCE, 0.85)], &[303], &[]),
        type_def(HARDENER, "EM Armor Hardener", 328, 7, &[(attr::ARMOR_EM_RESONANCE, -30.0), (attr::OVERLOAD_HARDENING_BONUS, 20.0)], &[304, 305], &[]),
        type_def(MWD, "1MN Microwarpdrive II", 46, 7, &[(attr::SPEED_FACTOR, 500.0), (attr::SPEED_BOOST_FACTOR, 1_500_000.0), (attr::MASS_ADDITION, 500_000.0), (attr::SIGNATURE_RADIUS_BONUS, 500.0), (attr::OVERLOAD_SPEED_FACTOR_BONUS, 50.0)], &[306, 307], &[]),
        type_def(IMPLANT, "Zor's Custom Navigation Hyper-Link", 300, 20, &[(attr::MAX_VELOCITY, 5.0)], &[308], &[]),
        type_def(BOOSTER, "Synth Exile Booster", 303, 20, &[(attr::ARMOR_DAMAGE_AMOUNT, 3.0), (BOOSTER_SIDE, -10.0), (USAGE_CHANCE, 0.3)], &[309, 310], &[]),
        type_def(DRONE, "Warrior II", 100, 18, &[(attr::DAMAGE_MULTIPLIER, 1.0)], &[], &[(SKILL_DRONES, 5)]),
        type_def(WEB, "Stasis Webifier II", 65, 7, &[(WEB_SPEED_FACTOR, -60.0)], &[311], &[]),
        type_def(RIG, "Small Trimark Armor Pump II", 773, 7, &[(attr::ARMOR_HP, 20.0)], &[312], &[]),
        type_def(SKILL_CRUISE, "Cruise Missiles", 256, 16, &[(attr::DAMAGE_MULTIPLIER_BONUS, 5.0)], &[204, 205, 206], &[]),
        type_def(SKILL_CRUISE_SPEC, "Cruise Missile Specialization", 256, 16, &[(attr::ROF_BONUS, -2.0)], &[207, 208], &[]),
        type_def(LAUNCHER, "Cruise Missile Launcher II", 506, 7, &[(attr::SPEED, 10_000.0)], &[], &[(SKILL_CRUISE_SPEC, 1)]),
        type_def(MISSILE, "Scourge Cruise Missile", 386, 8, &[(attr::KINETIC_DAMAGE, 300.0), (attr::EM_DAMAGE, 0.0)], &[], &[(SKILL_CRUISE, 1)]),
        type_def(SUBSYSTEM, "Loki Core - Augmented Nuclear Reactor", 1101, 32, &[(attr::HI_SLOT_MODIFIER, 2.0), (attr::TURRET_HARDPOINT_MODIFIER, 3.0)], &[313, 314], &[]),
    ];
    let effects = vec![
        // Ship: skill-scaled falloff bonus to small projectile turrets.
        effect(101, "shipBonusSPTFalloffMF2", EffectCategory::Passive, vec![Modifier { skill: Some(SkillRef::Of(SKILL_SPT)), ..modifier(Domain::Ship, Func::LocationRequiredSkill, attr::FALLOFF, SHIP_BONUS_MF, Op::PostPercent) }]),
        effect(102, "shipNothing", EffectCategory::System, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, SHIP_BONUS_MF, Op::PostPercent)]),
        // Skills: the level scales the ship bonus; a turret skill adds damage; a drone skill reaches owned drones.
        effect(201, "minmatarFrigateSkillLevelPreMul", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, SHIP_BONUS_MF, attr::SKILL_LEVEL, Op::PreMul)]),
        effect(202, "smallProjectileTurretDamage", EffectCategory::Passive, vec![Modifier { skill: Some(SkillRef::Own), ..modifier(Domain::Ship, Func::LocationRequiredSkill, attr::DAMAGE_MULTIPLIER, attr::SKILL_LEVEL, Op::PostPercent) }]),
        effect(204, "skillBoostDamageMultiplierBonus", EffectCategory::Passive, vec![modifier(Domain::Item, Func::Item, attr::DAMAGE_MULTIPLIER_BONUS, attr::SKILL_LEVEL, Op::PreMul)]),
        effect(205, "missileKineticDmgBonus2", EffectCategory::Passive, vec![]),
        effect(206, "missileEMDmgBonus", EffectCategory::Passive, vec![]),
        effect(207, "rapidFiringSkillBoostRofBonus", EffectCategory::Passive, vec![modifier(Domain::Item, Func::Item, attr::ROF_BONUS, attr::SKILL_LEVEL, Op::PreMul)]),
        effect(208, "selfRof", EffectCategory::Passive, vec![]),
        effect(313, "slotModifier", EffectCategory::Passive, vec![]),
        effect(314, "hardPointModifierEffect", EffectCategory::Passive, vec![]),
        effect(203, "dronesDamage", EffectCategory::Passive, vec![Modifier { skill: Some(SkillRef::Own), ..modifier(Domain::Char, Func::OwnerRequiredSkill, attr::DAMAGE_MULTIPLIER, DRONE_DAMAGE_BONUS, Op::PostPercent) }]),
        // Gun: its charge reaches it through otherID; the gun's own target effect does nothing here.
        effect(301, "projectileFired", EffectCategory::Target, vec![]),
        effect(302, "ammoInfluenceRange", EffectCategory::Passive, vec![modifier(Domain::Other, Func::Item, attr::MAX_RANGE, 120, Op::PreMul)]),
        // Damage control: PreMul on the ship's resonance, an online effect.
        effect(303, "damageControl", EffectCategory::Online, vec![modifier(Domain::Ship, Func::Item, attr::ARMOR_EM_RESONANCE, attr::ARMOR_EM_RESONANCE, Op::PreMul)]),
        // Hardener: active PostPercent, overloaded 20% stronger.
        effect(304, "armorHardener", EffectCategory::Active, vec![modifier(Domain::Ship, Func::Item, attr::ARMOR_EM_RESONANCE, attr::ARMOR_EM_RESONANCE, Op::PostPercent)]),
        effect(305, "overloadSelfHardening", EffectCategory::Overload, vec![modifier(Domain::Item, Func::Item, attr::ARMOR_EM_RESONANCE, attr::OVERLOAD_HARDENING_BONUS, Op::PostPercent)]),
        effect(306, "moduleBonusMicrowarpdrive", EffectCategory::Active, vec![]),
        effect(307, "overloadSelfSpeedBonus", EffectCategory::Overload, vec![modifier(Domain::Item, Func::Item, attr::SPEED_FACTOR, attr::OVERLOAD_SPEED_FACTOR_BONUS, Op::PostPercent)]),
        effect(308, "implantVelocity", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, attr::MAX_VELOCITY, Op::PostPercent)]),
        effect(309, "boosterArmorRep", EffectCategory::Passive, vec![Modifier { group: Some(62), ..modifier(Domain::Ship, Func::LocationGroup, attr::ARMOR_DAMAGE_AMOUNT, attr::ARMOR_DAMAGE_AMOUNT, Op::PostPercent) }]),
        EffectDef { usage_chance_attr: Some(USAGE_CHANCE), ..effect(310, "boosterSideEffect", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, BOOSTER_SIDE, Op::PostPercent)]) },
        effect(311, "remoteWebifier", EffectCategory::Target, vec![modifier(Domain::Target, Func::Item, attr::MAX_VELOCITY, WEB_SPEED_FACTOR, Op::PostPercent)]),
        effect(312, "rigArmorHp", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::ARMOR_HP, attr::ARMOR_HP, Op::PostPercent)]),
    ];
    let attributes = vec![
        attr_def(attr::MAX_VELOCITY, "maxVelocity", false, true, 0.0),
        attr_def(attr::MASS, "mass", true, true, 0.0),
        attr_def(attr::SIGNATURE_RADIUS, "signatureRadius", false, true, 0.0),
        attr_def(attr::ARMOR_EM_RESONANCE, "armorEmDamageResonance", false, false, 1.0),
        attr_def(attr::ARMOR_HP, "armorHP", false, true, 0.0),
        attr_def(attr::FALLOFF, "falloff", false, true, 0.0),
        attr_def(attr::MAX_RANGE, "maxRange", false, true, 0.0),
        attr_def(attr::DAMAGE_MULTIPLIER, "damageMultiplier", false, true, 1.0),
        attr_def(attr::SKILL_LEVEL, "skillLevel", true, true, 0.0),
        attr_def(SHIP_BONUS_MF, "shipBonusMF2", true, true, 0.0),
        attr_def(DRONE_DAMAGE_BONUS, "droneDamageBonus", true, true, 10.0),
        attr_def(attr::ARMOR_DAMAGE_AMOUNT, "armorDamageAmount", false, true, 0.0),
        attr_def(attr::SPEED_FACTOR, "speedFactor", false, true, 0.0),
        attr_def(attr::SPEED_BOOST_FACTOR, "speedBoostFactor", true, true, 0.0),
        attr_def(attr::MASS_ADDITION, "massAddition", true, true, 0.0),
        attr_def(attr::SIGNATURE_RADIUS_BONUS, "signatureRadiusBonus", true, true, 0.0),
        attr_def(attr::OVERLOAD_SPEED_FACTOR_BONUS, "overloadSpeedFactorBonus", true, true, 0.0),
        attr_def(attr::OVERLOAD_HARDENING_BONUS, "overloadHardeningBonus", true, true, 0.0),
        attr_def(attr::LOW_SLOTS, "lowSlots", true, true, 0.0),
        attr_def(120, "weaponRangeMultiplier", true, true, 1.0),
        attr_def(BOOSTER_SIDE, "boosterVelocityPenalty", true, true, 0.0),
        attr_def(USAGE_CHANCE, "boosterChance", true, true, 0.0),
        attr_def(attr::CAPACITY, "capacity", true, true, 0.0),
        attr_def(attr::VOLUME, "volume", true, true, 0.0),
        attr_def(attr::RADIUS, "radius", true, true, 0.0),
        attr_def(attr::EM_DAMAGE, "emDamage", true, true, 0.0),
        attr_def(attr::KINETIC_DAMAGE, "kineticDamage", true, true, 0.0),
        attr_def(attr::DAMAGE_MULTIPLIER_BONUS, "damageMultiplierBonus", true, true, 0.0),
        attr_def(attr::ROF_BONUS, "rofBonus", true, true, 0.0),
        attr_def(attr::SPEED, "speed", false, false, 0.0),
        attr_def(attr::HI_SLOTS, "hiSlots", true, true, 0.0),
        attr_def(attr::TURRET_SLOTS_LEFT, "turretSlotsLeft", true, true, 0.0),
        attr_def(attr::HI_SLOT_MODIFIER, "hiSlotModifier", true, true, 0.0),
        attr_def(attr::TURRET_HARDPOINT_MODIFIER, "turretHardPointModifier", true, true, 0.0),
        attr_def(attr::DRONE_CONTROL_DISTANCE, "droneControlDistance", true, true, 20000.0),
    ];
    let groups = vec![
        GroupDef { id: 25, name: "Frigate".into(), category: 6 },
        GroupDef { id: 62, name: "Armor Repair Unit".into(), category: 7 },
        GroupDef { id: 55, name: "Projectile Weapon".into(), category: 7 },
    ];
    Dogma { sde_build: 1, types, attributes, effects, groups, ..Dogma::default() }.finish()
}

fn fit() -> Fit {
    let mut f = Fit::new(SHIP, "test");
    f.modules.push(Module::new(GUN, Slot::High, 0).with_charge(AMMO).with_state(State::Active));
    f.modules.push(Module::new(DC, Slot::Low, 0).with_state(State::Active));
    f
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
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
    assert!(close(c.ship(attr::HI_SLOTS), 2.0));
    assert!(close(c.ship(attr::TURRET_SLOTS_LEFT), 3.0));
    assert!(close(c.ship(attr::MED_SLOTS), 0.0));
}

#[test]
fn the_pilot_node_carries_the_character_types_attributes_when_present() {
    let mut d = dataset();
    d.types.push(type_def(crate::ids::CHARACTER_TYPE, "CharacterType", 1, 1, &[(attr::DRONE_CONTROL_DISTANCE, 20_000.0)], &[], &[]));
    let d = d.finish();
    let c = Calc::new(&d, &fit(), &SkillSet::all_v(), &Env::default());
    assert!(close(c.char(attr::DRONE_CONTROL_DISTANCE), 20_000.0));
    assert!(c.unknown_types.is_empty());
    let plain = dataset();
    let c = Calc::new(&plain, &fit(), &SkillSet::all_v(), &Env::default());
    assert!(c.unknown_types.is_empty(), "a dataset without it is not an error");
    assert!(close(c.char(attr::DRONE_CONTROL_DISTANCE), 20_000.0), "the attribute default stands in");
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
