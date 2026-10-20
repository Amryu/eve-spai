//! A synthetic dataset for the engine's and the screens' tests: a frigate, its skills and enough modules, charges
//! and drones to exercise every modifier route and every stat. Numbers are round, not EVE's.

use crate::dogma::{attr, AttrDef, Dogma, Domain, EffectCategory, EffectDef, Func, GroupDef, Modifier, Op, SkillRef, TypeDef};
use crate::ids::{AttrId, CategoryId, EffectId, GroupId, TypeId, CHARACTER_TYPE};

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
}

pub fn attr_def(id: AttrId, name: &str, stackable: bool, high_is_good: bool, default: f32) -> AttrDef {
    AttrDef { id, name: name.into(), display: String::new(), stackable, high_is_good, default, unit: None, min_attr: None, max_attr: None, category: None, published: true }
}

pub fn effect(id: EffectId, name: &str, category: EffectCategory, modifiers: Vec<Modifier>) -> EffectDef {
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

pub fn modifier(domain: Domain, func: Func, modified: AttrId, modifying: AttrId, op: Op) -> Modifier {
    Modifier { domain, func, modified, modifying, op, group: None, skill: None }
}

pub fn type_def(id: TypeId, name: &str, group: GroupId, category: CategoryId, attrs: &[(AttrId, f32)], effects: &[EffectId], skills: &[(TypeId, u8)]) -> TypeDef {
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

pub mod ids {
    use crate::ids::TypeId;

    pub const SHIP: TypeId = 587;
    pub const SKILL_MF: TypeId = 3329;
    pub const SKILL_SPT: TypeId = 3300;
    pub const SKILL_DRONES: TypeId = 3436;
    pub const SKILL_CRUISE: TypeId = 3326;
    pub const SKILL_CRUISE_SPEC: TypeId = 20212;
    pub const GUN: TypeId = 2881;
    pub const AMMO: TypeId = 185;
    pub const AMMO_LONG: TypeId = 186;
    pub const DC: TypeId = 2048;
    pub const GYRO: TypeId = 519;
    pub const HARDENER: TypeId = 11269;
    pub const MWD: TypeId = 434;
    pub const IMPLANT: TypeId = 10226;
    pub const BOOSTER: TypeId = 28672;
    pub const DRONE: TypeId = 2454;
    pub const WEB: TypeId = 526;
    pub const RIG: TypeId = 31055;
    pub const LAUNCHER: TypeId = 19739;
    pub const MISSILE: TypeId = 2514;
    pub const SUBSYSTEM: TypeId = 45586;
    pub const REPAIRER: TypeId = 3530;
    pub const AAR: TypeId = 33101;
    pub const PASTE: TypeId = 28668;
    pub const SMARTBOMB: TypeId = 9680;
    pub const DISINTEGRATOR: TypeId = 47911;
    pub const CAP_BOOSTER: TypeId = 3568;
    pub const CAP_CHARGE: TypeId = 11283;
    pub const HEAVY: TypeId = 99901;
    pub const REMOTE_REP: TypeId = 3538;
    pub const BURST: TypeId = 43552;
    pub const BURST_CHARGE: TypeId = 42830;
}

pub const SHIP_BONUS_MF: AttrId = 587;
pub const DRONE_DAMAGE_BONUS: AttrId = 1255;
pub const WEB_SPEED_FACTOR: AttrId = 20;
pub const BOOSTER_SIDE: AttrId = 1223;
pub const USAGE_CHANCE: AttrId = 1287;

pub const GROUP_AMMO: GroupId = 83;
pub const GROUP_PASTE: GroupId = 916;
pub const GROUP_CAP_CHARGE: GroupId = 87;

pub fn dataset() -> Dogma {
    use ids::*;
    let types = vec![
        type_def(
            SHIP,
            "Rifter",
            25,
            6,
            &[
                (attr::MAX_VELOCITY, 365.0),
                (attr::MASS, 1_067_000.0),
                (attr::SIGNATURE_RADIUS, 35.0),
                (attr::ARMOR_EM_RESONANCE, 0.4),
                (SHIP_BONUS_MF, 7.5),
                (attr::HI_SLOTS, 3.0),
                (attr::MED_SLOTS, 3.0),
                (attr::LOW_SLOTS, 4.0),
                (attr::RIG_SLOTS, 3.0),
                (attr::UPGRADE_CAPACITY, 400.0),
                (attr::TURRET_SLOTS_LEFT, 3.0),
                (attr::LAUNCHER_SLOTS_LEFT, 2.0),
                (attr::CPU_OUTPUT, 130.0),
                (attr::POWER_OUTPUT, 40.0),
                (attr::ARMOR_HP, 350.0),
                (attr::SHIELD_CAPACITY, 200.0),
                (attr::SHIELD_RECHARGE_RATE, 500_000.0),
                (attr::HP, 300.0),
                (attr::CAPACITOR_CAPACITY, 300.0),
                (attr::RECHARGE_RATE, 100_000.0),
                (attr::AGILITY, 3.0),
                (attr::WARP_SPEED_MULTIPLIER, 1.0),
                (attr::BASE_WARP_SPEED, 5.0),
                (attr::SCAN_RESOLUTION, 600.0),
                (attr::MAX_TARGET_RANGE, 30_000.0),
                (attr::MAX_LOCKED_TARGETS, 4.0),
                (attr::SCAN_LADAR_STRENGTH, 9.0),
                (attr::DRONE_BANDWIDTH, 15.0),
                (attr::DRONE_CAPACITY, 25.0),
            ],
            &[101, 102],
            &[(SKILL_MF, 1)],
        ),
        type_def(CHARACTER_TYPE, "CharacterType", 1, 1, &[(attr::DRONE_CONTROL_DISTANCE, 20_000.0), (attr::MAX_LOCKED_TARGETS, 2.0), (attr::MAX_ACTIVE_DRONES, 5.0), (attr::MISSILE_DAMAGE_MULTIPLIER, 1.0)], &[], &[]),
        type_def(SKILL_MF, "Minmatar Frigate", 257, 16, &[], &[201], &[]),
        type_def(SKILL_SPT, "Small Projectile Turret", 255, 16, &[], &[202], &[]),
        type_def(SKILL_DRONES, "Drones", 273, 16, &[], &[203], &[]),
        type_def(SKILL_CRUISE, "Cruise Missiles", 256, 16, &[(attr::DAMAGE_MULTIPLIER_BONUS, 5.0)], &[204, 205, 206], &[]),
        type_def(SKILL_CRUISE_SPEC, "Cruise Missile Specialization", 256, 16, &[(attr::ROF_BONUS, -2.0)], &[207, 208], &[]),
        type_def(
            GUN,
            "200mm AutoCannon II",
            55,
            7,
            &[
                (attr::FALLOFF, 6000.0),
                (attr::MAX_RANGE, 1200.0),
                (attr::DAMAGE_MULTIPLIER, 3.0),
                (attr::SPEED, 3000.0),
                (attr::TRACKING_SPEED, 400.0),
                (attr::OPTIMAL_SIG_RADIUS, 40_000.0),
                (attr::CPU, 25.0),
                (attr::POWER, 15.0),
                (attr::CAPACITY, 2.0),
                (attr::CHARGE_RATE, 2.0),
                (attr::RELOAD_TIME, 10_000.0),
                (attr::CHARGE_GROUP_1, GROUP_AMMO as f32),
                (attr::CHARGE_SIZE, 1.0),
            ],
            &[301, 320, 321, 330],
            &[(SKILL_SPT, 1)],
        ),
        type_def(AMMO, "EMP S", GROUP_AMMO, 8, &[(attr::WEAPON_RANGE_MULTIPLIER, 0.5), (attr::EM_DAMAGE, 9.0), (attr::VOLUME, 0.05), (attr::CHARGE_SIZE, 1.0)], &[302], &[]),
        type_def(AMMO_LONG, "Barrage S", GROUP_AMMO, 8, &[(attr::WEAPON_RANGE_MULTIPLIER, 1.5), (attr::EXPLOSIVE_DAMAGE, 10.0), (attr::VOLUME, 0.05), (attr::CHARGE_SIZE, 1.0)], &[302], &[]),
        type_def(DC, "Damage Control II", 60, 7, &[(attr::ARMOR_EM_RESONANCE, 0.85), (attr::CPU, 20.0), (attr::POWER, 1.0)], &[303, 322, 330], &[]),
        type_def(GYRO, "Gyrostabilizer II", 59, 7, &[(attr::CPU, 30.0), (attr::POWER, 1.0)], &[322, 330], &[]),
        type_def(HARDENER, "EM Armor Hardener", 328, 7, &[(attr::ARMOR_EM_RESONANCE, -30.0), (attr::OVERLOAD_HARDENING_BONUS, 20.0), (attr::CPU, 10.0), (attr::POWER, 1.0), (attr::CAPACITOR_NEED, 1.0), (attr::DURATION, 20_000.0)], &[304, 305, 322, 330], &[]),
        type_def(
            MWD,
            "1MN Microwarpdrive II",
            46,
            7,
            &[
                (attr::SPEED_FACTOR, 500.0),
                (attr::SPEED_BOOST_FACTOR, 1_500_000.0),
                (attr::MASS_ADDITION, 500_000.0),
                (attr::SIGNATURE_RADIUS_BONUS, 500.0),
                (attr::OVERLOAD_SPEED_FACTOR_BONUS, 50.0),
                (attr::CPU, 25.0),
                (attr::POWER, 10.0),
                (attr::CAPACITOR_NEED, 30.0),
                (attr::DURATION, 10_000.0),
            ],
            &[306, 307, 323, 330],
            &[],
        ),
        type_def(IMPLANT, "Zor's Custom Navigation Hyper-Link", 300, 20, &[(attr::MAX_VELOCITY, 5.0), (attr::IMPLANTNESS, 6.0)], &[308], &[]),
        type_def(BOOSTER, "Synth Exile Booster", 303, 20, &[(attr::ARMOR_DAMAGE_AMOUNT, 3.0), (BOOSTER_SIDE, -10.0), (USAGE_CHANCE, 0.3), (attr::BOOSTERNESS, 1.0)], &[309, 310], &[]),
        type_def(DRONE, "Warrior II", 100, 18, &[(attr::DAMAGE_MULTIPLIER, 1.0), (attr::THERMAL_DAMAGE, 20.0), (attr::SPEED, 4000.0), (attr::VOLUME, 5.0), (attr::DRONE_BANDWIDTH_USED, 5.0), (attr::MAX_RANGE, 2000.0), (attr::FALLOFF, 1000.0), (attr::TRACKING_SPEED, 2.0), (attr::OPTIMAL_SIG_RADIUS, 25.0)], &[301], &[(SKILL_DRONES, 5)]),
        type_def(WEB, "Stasis Webifier II", 65, 7, &[(WEB_SPEED_FACTOR, -60.0), (attr::CPU, 30.0), (attr::POWER, 1.0), (attr::CAPACITOR_NEED, 6.0), (attr::DURATION, 5000.0)], &[311, 323, 330], &[]),
        type_def(RIG, "Small Trimark Armor Pump II", 773, 7, &[(attr::ARMOR_HP, 20.0), (attr::UPGRADE_COST, 50.0)], &[312, 324], &[]),
        type_def(LAUNCHER, "Cruise Missile Launcher II", 506, 7, &[(attr::SPEED, 10_000.0), (attr::CPU, 40.0), (attr::POWER, 10.0), (attr::CAPACITY, 1.0), (attr::RELOAD_TIME, 10_000.0), (attr::CHARGE_GROUP_1, 386.0)], &[331, 320, 332, 330], &[(SKILL_CRUISE_SPEC, 1)]),
        type_def(MISSILE, "Scourge Cruise Missile", 386, 8, &[(attr::KINETIC_DAMAGE, 300.0), (attr::EM_DAMAGE, 0.0), (attr::MAX_VELOCITY, 4000.0), (attr::EXPLOSION_DELAY, 6000.0), (attr::AOE_CLOUD_SIZE, 100.0), (attr::AOE_VELOCITY, 50.0), (attr::AOE_DAMAGE_REDUCTION_FACTOR, 0.7), (attr::VOLUME, 0.1)], &[333], &[(SKILL_CRUISE, 1)]),
        type_def(SUBSYSTEM, "Loki Core - Augmented Nuclear Reactor", 1101, 32, &[(attr::HI_SLOT_MODIFIER, 2.0), (attr::TURRET_HARDPOINT_MODIFIER, 3.0)], &[313, 314, 325], &[]),
        type_def(REPAIRER, "Small Armor Repairer II", 62, 7, &[(attr::ARMOR_DAMAGE_AMOUNT, 60.0), (attr::DURATION, 6000.0), (attr::CAPACITOR_NEED, 24.0), (attr::CPU, 10.0), (attr::POWER, 5.0)], &[340, 322, 330], &[]),
        type_def(AAR, "Small Ancillary Armor Repairer", 1199, 7, &[(attr::ARMOR_DAMAGE_AMOUNT, 40.0), (attr::DURATION, 6000.0), (attr::CAPACITOR_NEED, 24.0), (attr::CPU, 10.0), (attr::POWER, 5.0), (attr::CAPACITY, 0.08), (attr::CHARGE_RATE, 1.0), (attr::RELOAD_TIME, 60_000.0), (attr::CHARGED_ARMOR_DAMAGE_MULTIPLIER, 3.0), (attr::CHARGE_GROUP_2, GROUP_PASTE as f32)], &[341, 322, 330], &[]),
        type_def(PASTE, "Nanite Repair Paste", GROUP_PASTE, 8, &[(attr::HP, 1.0), (attr::VOLUME, 0.01)], &[], &[]),
        type_def(SMARTBOMB, "Small EMP Smartbomb II", 72, 7, &[(attr::EM_DAMAGE, 100.0), (attr::DURATION, 5000.0), (attr::EMP_FIELD_RANGE, 5000.0), (attr::CAPACITOR_NEED, 20.0), (attr::CPU, 10.0), (attr::POWER, 10.0)], &[342, 320, 330], &[]),
        type_def(DISINTEGRATOR, "Light Entropic Disintegrator II", 1986, 7, &[(attr::DAMAGE_MULTIPLIER, 1.0), (attr::DAMAGE_MULTIPLIER_BONUS_MAX, 2.0), (attr::SPEED, 4000.0), (attr::CPU, 20.0), (attr::POWER, 10.0), (attr::CAPACITY, 1.0), (attr::RELOAD_TIME, 0.01), (attr::CHARGE_GROUP_1, GROUP_AMMO as f32), (attr::CHARGE_SIZE, 1.0), (attr::CAPACITOR_NEED, 5.0)], &[343, 320, 321, 330], &[]),
        type_def(CAP_BOOSTER, "Small Capacitor Booster II", 76, 7, &[(attr::DURATION, 12_000.0), (attr::CAPACITY, 40.0), (attr::RELOAD_TIME, 10_000.0), (attr::CHARGE_RATE, 1.0), (attr::CPU, 10.0), (attr::POWER, 5.0), (attr::CHARGE_GROUP_1, GROUP_CAP_CHARGE as f32)], &[344, 323, 330], &[]),
        type_def(CAP_CHARGE, "Cap Booster 400", GROUP_CAP_CHARGE, 8, &[(attr::CAPACITOR_BONUS, 400.0), (attr::VOLUME, 10.0)], &[], &[]),
        type_def(HEAVY, "Oversized Turret", 55, 7, &[(attr::CPU, 100.0), (attr::POWER, 50.0), (attr::SPEED, 3000.0), (attr::CHARGE_GROUP_1, GROUP_AMMO as f32)], &[301, 320, 321, 330], &[]),
        type_def(REMOTE_REP, "Small Remote Armor Repairer II", 325, 7, &[(attr::ARMOR_DAMAGE_AMOUNT, 60.0), (attr::DURATION, 6000.0), (attr::CAPACITOR_NEED, 20.0), (attr::MAX_RANGE, 7000.0), (attr::FALLOFF_EFFECTIVENESS, 2000.0), (attr::CPU, 20.0), (attr::POWER, 10.0)], &[345, 320, 330], &[]),
        type_def(BURST, "Shield Command Burst II", 316, 7, &[(attr::DURATION, 60_000.0), (attr::CAPACITOR_NEED, 25.0), (attr::CPU, 55.0), (attr::POWER, 110.0), (attr::WARFARE_BUFF_1_VALUE, 1.25), (attr::CHARGE_GROUP_1, 1769.0), (attr::CAPACITY, 10.0)], &[346, 320, 330], &[]),
        type_def(BURST_CHARGE, "Shield Harmonizing Charge", 1769, 8, &[(attr::WARFARE_BUFF_1_ID, 12.0), (2596, 20.0), (attr::VOLUME, 1.0)], &[347], &[]),
    ];
    let effects = vec![
        effect(101, "shipBonusSPTFalloffMF2", EffectCategory::Passive, vec![Modifier { skill: Some(SkillRef::Of(SKILL_SPT)), ..modifier(Domain::Ship, Func::LocationRequiredSkill, attr::FALLOFF, SHIP_BONUS_MF, Op::PostPercent) }]),
        effect(102, "shipNothing", EffectCategory::System, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, SHIP_BONUS_MF, Op::PostPercent)]),
        effect(201, "minmatarFrigateSkillLevelPreMul", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, SHIP_BONUS_MF, attr::SKILL_LEVEL, Op::PreMul)]),
        effect(202, "smallProjectileTurretDamage", EffectCategory::Passive, vec![Modifier { skill: Some(SkillRef::Own), ..modifier(Domain::Ship, Func::LocationRequiredSkill, attr::DAMAGE_MULTIPLIER, attr::SKILL_LEVEL, Op::PostPercent) }]),
        effect(203, "dronesDamage", EffectCategory::Passive, vec![Modifier { skill: Some(SkillRef::Own), ..modifier(Domain::Char, Func::OwnerRequiredSkill, attr::DAMAGE_MULTIPLIER, DRONE_DAMAGE_BONUS, Op::PostPercent) }]),
        effect(204, "skillBoostDamageMultiplierBonus", EffectCategory::Passive, vec![modifier(Domain::Item, Func::Item, attr::DAMAGE_MULTIPLIER_BONUS, attr::SKILL_LEVEL, Op::PreMul)]),
        effect(205, "missileKineticDmgBonus2", EffectCategory::Passive, vec![]),
        effect(206, "missileEMDmgBonus", EffectCategory::Passive, vec![]),
        effect(207, "rapidFiringSkillBoostRofBonus", EffectCategory::Passive, vec![modifier(Domain::Item, Func::Item, attr::ROF_BONUS, attr::SKILL_LEVEL, Op::PreMul)]),
        effect(208, "selfRof", EffectCategory::Passive, vec![]),
        EffectDef { duration_attr: Some(attr::SPEED), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(301, "targetAttack", EffectCategory::Target, vec![]) },
        effect(302, "ammoInfluenceRange", EffectCategory::Passive, vec![modifier(Domain::Other, Func::Item, attr::MAX_RANGE, attr::WEAPON_RANGE_MULTIPLIER, Op::PreMul)]),
        effect(303, "damageControl", EffectCategory::Online, vec![modifier(Domain::Ship, Func::Item, attr::ARMOR_EM_RESONANCE, attr::ARMOR_EM_RESONANCE, Op::PreMul)]),
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(304, "armorHardener", EffectCategory::Active, vec![modifier(Domain::Ship, Func::Item, attr::ARMOR_EM_RESONANCE, attr::ARMOR_EM_RESONANCE, Op::PostPercent)]) },
        effect(305, "overloadSelfHardening", EffectCategory::Overload, vec![modifier(Domain::Item, Func::Item, attr::ARMOR_EM_RESONANCE, attr::OVERLOAD_HARDENING_BONUS, Op::PostPercent)]),
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(306, "moduleBonusMicrowarpdrive", EffectCategory::Active, vec![]) },
        effect(307, "overloadSelfSpeedBonus", EffectCategory::Overload, vec![modifier(Domain::Item, Func::Item, attr::SPEED_FACTOR, attr::OVERLOAD_SPEED_FACTOR_BONUS, Op::PostPercent)]),
        effect(308, "implantVelocity", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, attr::MAX_VELOCITY, Op::PostPercent)]),
        effect(309, "boosterArmorRep", EffectCategory::Passive, vec![Modifier { group: Some(62), ..modifier(Domain::Ship, Func::LocationGroup, attr::ARMOR_DAMAGE_AMOUNT, attr::ARMOR_DAMAGE_AMOUNT, Op::PostPercent) }]),
        EffectDef { usage_chance_attr: Some(USAGE_CHANCE), ..effect(310, "boosterSideEffect", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, BOOSTER_SIDE, Op::PostPercent)]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(311, "remoteWebifier", EffectCategory::Target, vec![modifier(Domain::Target, Func::Item, attr::MAX_VELOCITY, WEB_SPEED_FACTOR, Op::PostPercent)]) },
        effect(312, "rigArmorHp", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::ARMOR_HP, attr::ARMOR_HP, Op::PostPercent)]),
        effect(313, "slotModifier", EffectCategory::Passive, vec![]),
        effect(314, "hardPointModifierEffect", EffectCategory::Passive, vec![]),
        effect(320, "hiPower", EffectCategory::Passive, vec![]),
        effect(321, "turretFitted", EffectCategory::Passive, vec![]),
        effect(322, "loPower", EffectCategory::Passive, vec![]),
        effect(323, "medPower", EffectCategory::Passive, vec![]),
        effect(324, "rigSlot", EffectCategory::Passive, vec![]),
        effect(325, "subSystem", EffectCategory::Passive, vec![]),
        effect(330, "online", EffectCategory::Online, vec![]),
        EffectDef { duration_attr: Some(attr::SPEED), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(331, "useMissiles", EffectCategory::Active, vec![]) },
        effect(332, "launcherFitted", EffectCategory::Passive, vec![]),
        effect(333, "missileLaunching", EffectCategory::Target, vec![]),
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(340, "armorRepair", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(341, "fueledArmorRepair", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(342, "empWave", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::SPEED), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(343, "targetDisintegratorAttack", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(344, "powerBooster", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(345, "shipModuleRemoteArmorRepairer", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(346, "moduleBonusWarfareLinkShield", EffectCategory::Active, vec![]) },
        // The charge names the buff and multiplies the module's strength into its value.
        effect(347, "chargeBonusWarfareCharge", EffectCategory::Passive, vec![modifier(Domain::Other, Func::Item, attr::WARFARE_BUFF_1_ID, attr::WARFARE_BUFF_1_ID, Op::PostAssign), modifier(Domain::Other, Func::Item, attr::WARFARE_BUFF_1_VALUE, 2596, Op::PostMul)]),
    ];
    let attributes = vec![
        attr_def(attr::MAX_VELOCITY, "maxVelocity", false, true, 0.0),
        attr_def(attr::MASS, "mass", true, true, 0.0),
        attr_def(attr::SIGNATURE_RADIUS, "signatureRadius", false, true, 0.0),
        attr_def(attr::ARMOR_EM_RESONANCE, "armorEmDamageResonance", false, false, 1.0),
        attr_def(attr::ARMOR_THERMAL_RESONANCE, "armorThermalDamageResonance", false, false, 1.0),
        attr_def(attr::ARMOR_KINETIC_RESONANCE, "armorKineticDamageResonance", false, false, 1.0),
        attr_def(attr::ARMOR_EXPLOSIVE_RESONANCE, "armorExplosiveDamageResonance", false, false, 1.0),
        attr_def(attr::SHIELD_EM_RESONANCE, "shieldEmDamageResonance", false, false, 1.0),
        attr_def(attr::SHIELD_THERMAL_RESONANCE, "shieldThermalDamageResonance", false, false, 1.0),
        attr_def(attr::SHIELD_KINETIC_RESONANCE, "shieldKineticDamageResonance", false, false, 1.0),
        attr_def(attr::SHIELD_EXPLOSIVE_RESONANCE, "shieldExplosiveDamageResonance", false, false, 1.0),
        attr_def(attr::EM_RESONANCE, "emDamageResonance", false, false, 1.0),
        attr_def(attr::THERMAL_RESONANCE, "thermalDamageResonance", false, false, 1.0),
        attr_def(attr::KINETIC_RESONANCE, "kineticDamageResonance", false, false, 1.0),
        attr_def(attr::EXPLOSIVE_RESONANCE, "explosiveDamageResonance", false, false, 1.0),
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
        attr_def(attr::WEAPON_RANGE_MULTIPLIER, "weaponRangeMultiplier", true, true, 1.0),
        attr_def(BOOSTER_SIDE, "boosterVelocityPenalty", true, true, 0.0),
        attr_def(USAGE_CHANCE, "boosterChance", true, true, 0.0),
        attr_def(attr::CAPACITY, "capacity", true, true, 0.0),
        attr_def(attr::VOLUME, "volume", true, true, 0.0),
        attr_def(attr::RADIUS, "radius", true, true, 0.0),
        attr_def(attr::EM_DAMAGE, "emDamage", true, true, 0.0),
        attr_def(attr::THERMAL_DAMAGE, "thermalDamage", true, true, 0.0),
        attr_def(attr::KINETIC_DAMAGE, "kineticDamage", true, true, 0.0),
        attr_def(attr::EXPLOSIVE_DAMAGE, "explosiveDamage", true, true, 0.0),
        attr_def(attr::DAMAGE_MULTIPLIER_BONUS, "damageMultiplierBonus", true, true, 0.0),
        attr_def(attr::ROF_BONUS, "rofBonus", true, true, 0.0),
        attr_def(attr::SPEED, "speed", false, false, 0.0),
        attr_def(attr::HI_SLOTS, "hiSlots", true, true, 0.0),
        attr_def(attr::MED_SLOTS, "medSlots", true, true, 0.0),
        attr_def(attr::RIG_SLOTS, "rigSlots", true, true, 0.0),
        attr_def(attr::TURRET_SLOTS_LEFT, "turretSlotsLeft", true, true, 0.0),
        attr_def(attr::LAUNCHER_SLOTS_LEFT, "launcherSlotsLeft", true, true, 0.0),
        attr_def(attr::HI_SLOT_MODIFIER, "hiSlotModifier", true, true, 0.0),
        attr_def(attr::TURRET_HARDPOINT_MODIFIER, "turretHardPointModifier", true, true, 0.0),
        attr_def(attr::DRONE_CONTROL_DISTANCE, "droneControlDistance", true, true, 20000.0),
        attr_def(attr::CPU, "cpu", true, true, 0.0),
        attr_def(attr::POWER, "power", true, true, 0.0),
        attr_def(attr::CPU_OUTPUT, "cpuOutput", true, true, 0.0),
        attr_def(attr::POWER_OUTPUT, "powerOutput", true, true, 0.0),
        attr_def(attr::UPGRADE_COST, "upgradeCost", true, true, 0.0),
        attr_def(attr::UPGRADE_CAPACITY, "upgradeCapacity", true, true, 0.0),
        attr_def(attr::SHIELD_CAPACITY, "shieldCapacity", false, true, 0.0),
        attr_def(attr::SHIELD_RECHARGE_RATE, "shieldRechargeRate", false, true, 0.0),
        attr_def(attr::HP, "hp", false, true, 0.0),
        attr_def(attr::DURATION, "duration", false, false, 0.0),
        attr_def(attr::CAPACITOR_NEED, "capacitorNeed", false, false, 0.0),
        attr_def(attr::RELOAD_TIME, "reloadTime", true, false, 10_000.0),
        attr_def(attr::CHARGE_RATE, "chargeRate", true, true, 1.0),
        attr_def(attr::CHARGED_ARMOR_DAMAGE_MULTIPLIER, "chargedArmorDamageMultiplier", true, true, 1.0),
        attr_def(attr::SHIELD_BONUS, "shieldBonus", false, true, 0.0),
        attr_def(attr::TRACKING_SPEED, "trackingSpeed", false, true, 0.0),
        attr_def(attr::OPTIMAL_SIG_RADIUS, "optimalSigRadius", true, true, 0.0),
        attr_def(attr::CHARGE_GROUP_1, "chargeGroup1", true, true, 0.0),
        attr_def(attr::CHARGE_GROUP_2, "chargeGroup2", true, true, 0.0),
        attr_def(attr::CHARGE_SIZE, "chargeSize", true, true, 0.0),
        attr_def(attr::EXPLOSION_DELAY, "explosionDelay", true, true, 0.0),
        attr_def(attr::AOE_CLOUD_SIZE, "aoeCloudSize", false, false, 0.0),
        attr_def(attr::AOE_VELOCITY, "aoeVelocity", false, true, 0.0),
        attr_def(attr::AOE_DAMAGE_REDUCTION_FACTOR, "aoeDamageReductionFactor", true, true, 0.0),
        attr_def(attr::MISSILE_DAMAGE_MULTIPLIER, "missileDamageMultiplier", false, true, 1.0),
        attr_def(attr::EMP_FIELD_RANGE, "empFieldRange", true, true, 0.0),
        attr_def(attr::DAMAGE_MULTIPLIER_BONUS_MAX, "damageMultiplierBonusMax", true, true, 0.0),
        attr_def(attr::CAPACITOR_BONUS, "capacitorBonus", true, true, 0.0),
        attr_def(attr::CAPACITOR_CAPACITY, "capacitorCapacity", false, true, 0.0),
        attr_def(attr::RECHARGE_RATE, "rechargeRate", false, false, 0.0),
        attr_def(attr::AGILITY, "agility", false, false, 0.0),
        attr_def(attr::WARP_SPEED_MULTIPLIER, "warpSpeedMultiplier", true, true, 1.0),
        attr_def(attr::BASE_WARP_SPEED, "baseWarpSpeed", true, true, 1.0),
        attr_def(attr::SCAN_RESOLUTION, "scanResolution", false, true, 0.0),
        attr_def(attr::MAX_TARGET_RANGE, "maxTargetRange", false, true, 0.0),
        attr_def(attr::MAX_LOCKED_TARGETS, "maxLockedTargets", true, true, 0.0),
        attr_def(attr::SCAN_LADAR_STRENGTH, "scanLadarStrength", false, true, 0.0),
        attr_def(attr::MAX_ACTIVE_DRONES, "maxActiveDrones", true, true, 0.0),
        attr_def(attr::DRONE_BANDWIDTH, "droneBandwidth", true, true, 0.0),
        attr_def(attr::DRONE_BANDWIDTH_USED, "droneBandwidthUsed", true, true, 0.0),
        attr_def(attr::DRONE_CAPACITY, "droneCapacity", true, true, 0.0),
        attr_def(attr::IMPLANTNESS, "implantness", true, true, 0.0),
        attr_def(attr::BOOSTERNESS, "boosterness", true, true, 0.0),
        attr_def(attr::FALLOFF_EFFECTIVENESS, "falloffEffectiveness", true, true, 0.0),
        attr_def(attr::WARFARE_BUFF_1_ID, "warfareBuff1ID", true, true, 0.0),
        attr_def(attr::WARFARE_BUFF_1_VALUE, "warfareBuff1Value", true, true, 0.0),
        attr_def(2596, "warfareBuff1Multiplier", true, true, 0.0),
    ];
    let groups = vec![
        GroupDef { id: 1, name: "Character".into(), category: 1 },
        GroupDef { id: 25, name: "Frigate".into(), category: 6 },
        GroupDef { id: 46, name: "Propulsion Module".into(), category: 7 },
        GroupDef { id: 62, name: "Armor Repair Unit".into(), category: 7 },
        GroupDef { id: 55, name: "Projectile Weapon".into(), category: 7 },
        GroupDef { id: GROUP_AMMO, name: "Projectile Ammo".into(), category: 8 },
        GroupDef { id: GROUP_PASTE, name: "Nanite Repair Paste".into(), category: 8 },
        GroupDef { id: GROUP_CAP_CHARGE, name: "Capacitor Booster Charge".into(), category: 8 },
        GroupDef { id: 386, name: "Cruise Missile".into(), category: 8 },
    ];
    Dogma { sde_build: 1, types, attributes, effects, groups, ..Dogma::default() }.finish()
}
