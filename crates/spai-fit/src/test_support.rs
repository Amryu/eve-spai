//! A synthetic dataset for the engine's and the screens' tests: a frigate, its skills and enough modules, charges
//! and drones to exercise every modifier route and every stat. Numbers are round, not EVE's.

use crate::dogma::{attr, AttrDef, Dogma, Domain, EffectCategory, EffectDef, Func, GroupDef, MarketGroup, Modifier, Op, SkillRef, TypeDef};
use crate::ids::{AttrId, CategoryId, EffectId, GroupId, MarketGroupId, TypeId, CHARACTER_TYPE};

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
        packaged: None,
    }
}

pub mod ids {
    use crate::ids::{AttrId, TypeId};

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
    pub const AB: TypeId = 438;
    pub const DAMP: TypeId = 1968;
    pub const DAMP_LIGHT: TypeId = 5299;
    pub const DIAGNOSTIC: TypeId = 1541;
    pub const RCU: TypeId = 11640;
    pub const AB_BIG: TypeId = 12066;
    pub const AB_MID: TypeId = 12058;
    pub const RCU_BONUS: AttrId = 9004;
    pub const ISOTOPES: TypeId = 16274;
    pub const JUMP_FUEL: AttrId = 866;
    pub const FUEL_BAY: AttrId = 1549;
    pub const FLEET_HANGAR: AttrId = 912;
    pub const TITAN: TypeId = 11567;
    pub const IMPLANT: TypeId = 10226;
    pub const BOOSTER: TypeId = 28672;
    /// A wormhole weather beacon in the synthetic data.
    pub const BEACON: TypeId = 30_844;
    pub const BEACON_SPEED: AttrId = 9001;
    /// A titan's phenomena generator in the synthetic data.
    pub const GENERATOR: TypeId = 43_658;
    pub const MUTAPLASMID: TypeId = 47_297;
    /// What the mutaplasmid turns the microwarpdrive into.
    pub const ABYSSAL_MWD: TypeId = 47_408;
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
    pub const NOS: TypeId = 530;
    pub const DEFENSE_MODE: TypeId = 34319;
    pub const PROPULSION_MODE: TypeId = 34323;
    pub const SCRAM: TypeId = 448;
    pub const POINT: TypeId = 3244;
    pub const FIELD: TypeId = 4248;
    pub const FOCUSED_SCRIPT: TypeId = 29003;
    pub const BURST: TypeId = 43552;
    pub const BURST_CHARGE: TypeId = 42830;
    pub const FIGHTER: TypeId = 40354;
    pub const VORTON: TypeId = 54753;
    pub const VORTON_CHARGE: TypeId = 54783;
    pub const BOMB_LAUNCHER: TypeId = 27914;
    pub const BOMB: TypeId = 27912;
    pub const BREACHER_LAUNCHER: TypeId = 85084;
    pub const BREACHER_POD: TypeId = 85088;
    pub const DOOMSDAY: TypeId = 24550;
    pub const LANCE: TypeId = 40631;
    pub const LOGI_DRONE: TypeId = 28197;
    pub const WEB_DRONE: TypeId = 23536;
    pub const SUPPORT_FIGHTER: TypeId = 40347;
    pub const MUTA_REP: TypeId = 49773;
    pub const ARAR: TypeId = 41478;
    pub const MINER: TypeId = 17912;
    pub const SENSOR_BOOST: TypeId = 1969;
    pub const TRACKING_BOOST: TypeId = 2104;
    pub const ECM_BURST: TypeId = 22551;
    pub const SKIRM_BURST: TypeId = 43554;
    /// A strategic cruiser, a core subsystem with a web range bonus and an offensive one with a
    /// burst strength bonus.
    pub const T3C: TypeId = 29990;
    pub const WEB_SUB: TypeId = 45620;
    pub const BOOST_SUB: TypeId = 45600;
    pub const SKIRM_CHARGE: TypeId = 42836;
    pub const SHIELD_MINDLINK: TypeId = 21888;
    pub const SKIRM_MINDLINK: TypeId = 22798;
    /// Named attributes of the synthetic data with ids of its own.
    pub const ARC_TARGETS: AttrId = 2950;
    pub const ARC_RANGE: AttrId = 2951;
    pub const EXPLOSION_RANGE: AttrId = 2952;
    pub const DOT_PER_TICK: AttrId = 2953;
    pub const DOT_HP_PCT: AttrId = 2954;
    pub const DOT_DURATION: AttrId = 2955;
    pub const DD_TICK: AttrId = 2956;
    pub const DD_DURATION: AttrId = 2957;
    pub const DD_RADIUS: AttrId = 2958;
    pub const FIGHTER_WEB: AttrId = 2959;
    pub const FIGHTER_WEB_RANGE: AttrId = 2960;
    pub const FIGHTER_WEB_DURATION: AttrId = 2961;
    pub const SQUADRON_MAX: AttrId = 2962;
    pub const REP_SPOOL_MAX: AttrId = 2963;
    pub const REP_SPOOL_STEP: AttrId = 2964;
    pub const CRIT_CHANCE: AttrId = 2965;
    pub const CRIT_YIELD: AttrId = 2966;
    pub const MAX_GROUP_ACTIVE: AttrId = 763;
}

pub const SHIP_BONUS_MF: AttrId = 587;
pub const FITS_TO_SHIP: AttrId = 1380;
pub const SUB_WEB_BONUS: AttrId = 2990;
pub const SUB_BURST_BONUS: AttrId = 2991;
pub const DRONE_DAMAGE_BONUS: AttrId = 1255;
pub const WEB_SPEED_FACTOR: AttrId = 20;
pub const BOOSTER_SIDE: AttrId = 1223;
pub const USAGE_CHANCE: AttrId = 1287;

pub const GROUP_AMMO: GroupId = 83;
pub const GROUP_PASTE: GroupId = 916;
pub const GROUP_CAP_CHARGE: GroupId = 87;

fn market(id: MarketGroupId, name: &str, parent: Option<MarketGroupId>) -> MarketGroup {
    MarketGroup { id, name: name.into(), parent }
}

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
                (JUMP_FUEL, 16274.0),
                (FUEL_BAY, 1000.0),
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
        type_def(AB, "1MN Afterburner II", 46, 7, &[(attr::SPEED_FACTOR, 135.0), (attr::SPEED_BOOST_FACTOR, 500_000.0), (attr::CPU, 20.0), (attr::POWER, 1.0), (attr::CAPACITOR_NEED, 5.0), (attr::DURATION, 10_000.0)], &[6301, 323, 330], &[]),
        type_def(DAMP_LIGHT, "Phased Muon Scoped Sensor Dampener", 208, 7, &[(attr::CPU, 80.0), (attr::POWER, 1.0)], &[323, 330], &[]),
        type_def(DIAGNOSTIC, "Power Diagnostic System II", 766, 7, &[(attr::CPU, 1.0)], &[322, 330], &[]),
        type_def(RCU, "Reactor Control Unit II", 339, 7, &[(attr::CPU, 1.0), (RCU_BONUS, 20.0)], &[6303, 322, 330], &[]),
        type_def(AB_BIG, "10MN Afterburner II", 46, 7, &[(attr::SPEED_FACTOR, 135.0), (attr::CPU, 20.0), (attr::POWER, 45.0), (attr::CAPACITOR_NEED, 5.0), (attr::DURATION, 10_000.0)], &[6301, 323, 330], &[]),
        type_def(ISOTOPES, "Helium Isotopes", crate::ids::group::ICE_PRODUCT, 4, &[(attr::VOLUME, 0.03)], &[], &[]),
        type_def(AB_MID, "1MN Monopropellant Enduring Afterburner", 46, 7, &[(attr::SPEED_FACTOR, 135.0), (attr::CPU, 20.0), (attr::POWER, 25.0), (attr::CAPACITOR_NEED, 5.0), (attr::DURATION, 10_000.0)], &[6301, 323, 330], &[]),
        // Unpublished: only the hold rules read it, the pickers keep their one hull.
        TypeDef { published: false, ..type_def(TITAN, "Avatar", 30, 6, &[(attr::HI_SLOTS, 6.0), (attr::MED_SLOTS, 4.0), (attr::LOW_SLOTS, 8.0), (FLEET_HANGAR, 50_000.0), (FUEL_BAY, 60_000.0)], &[], &[]) },
        type_def(DAMP, "Remote Sensor Dampener II", 208, 7, &[(attr::CPU, 200.0), (attr::POWER, 1.0)], &[323, 330], &[]),
        type_def(IMPLANT, "Zor's Custom Navigation Hyper-Link", 300, 20, &[(attr::MAX_VELOCITY, 5.0), (attr::IMPLANTNESS, 6.0)], &[308], &[]),
        type_def(GENERATOR, "Amarr Phenomena Generator", 1815, 7, &[(attr::WARFARE_BUFF_1_ID, 12.0), (attr::WARFARE_BUFF_1_VALUE, 15.0), (attr::CPU, 100.0)], &[9002], &[]),
        type_def(BEACON, "Pulsar Effect Beacon Class 1", crate::ids::group::EFFECT_BEACON, 2, &[(BEACON_SPEED, 10.0)], &[9001], &[]),
        type_def(BOOSTER, "Synth Exile Booster", 303, 20, &[(attr::ARMOR_DAMAGE_AMOUNT, 3.0), (BOOSTER_SIDE, -10.0), (USAGE_CHANCE, 0.3), (attr::BOOSTERNESS, 1.0)], &[309, 310], &[]),
        type_def(DRONE, "Warrior II", 100, 18, &[(attr::DAMAGE_MULTIPLIER, 1.0), (attr::THERMAL_DAMAGE, 20.0), (attr::SPEED, 4000.0), (attr::VOLUME, 5.0), (attr::DRONE_BANDWIDTH_USED, 5.0), (attr::MAX_RANGE, 2000.0), (attr::FALLOFF, 1000.0), (attr::TRACKING_SPEED, 2.0), (attr::OPTIMAL_SIG_RADIUS, 25.0)], &[301], &[(SKILL_DRONES, 5)]),
        type_def(WEB, "Stasis Webifier II", 65, 7, &[(WEB_SPEED_FACTOR, -60.0), (attr::CPU, 30.0), (attr::POWER, 1.0), (attr::CAPACITOR_NEED, 6.0), (attr::DURATION, 5000.0), (attr::MAX_RANGE, 10_000.0), (attr::FALLOFF_EFFECTIVENESS, 5000.0)], &[311, 323, 330], &[]),
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
        type_def(NOS, "Small Energy Nosferatu I", 68, 7, &[(attr::POWER_TRANSFER_AMOUNT, 12.0), (attr::DURATION, 6000.0), (attr::MAX_RANGE, 6000.0), (attr::FALLOFF_EFFECTIVENESS, 3000.0), (attr::CPU, 10.0), (attr::POWER, 5.0)], &[349, 320, 330], &[]),
        type_def(SCRAM, "Warp Scrambler II", 52, 7, &[(attr::MAX_RANGE, 9000.0), (attr::WARP_SCRAMBLE_STRENGTH, 2.0), (attr::DURATION, 5000.0)], &[350, 323, 330], &[]),
        type_def(POINT, "Warp Disruptor II", 52, 7, &[(attr::MAX_RANGE, 24_000.0), (attr::WARP_SCRAMBLE_STRENGTH, 1.0), (attr::DURATION, 5000.0)], &[351, 323, 330], &[]),
        type_def(FIELD, "Warp Disruption Field Generator I", 899, 7, &[(attr::WARP_SCRAMBLE_RANGE, 20_000.0), (attr::WARP_SCRAMBLE_STRENGTH, 100.0), (attr::DURATION, 30_000.0), (attr::CAPACITY, 1.0), (attr::CHARGE_GROUP_1, 908.0)], &[352, 320, 330], &[]),
        type_def(FOCUSED_SCRIPT, "Focused Warp Disruption Script", 908, 8, &[(1327, 50.0), (attr::VOLUME, 1.0)], &[353], &[]),
        type_def(VORTON, "Large Vorton Projector II", 4060, 7, &[(attr::DAMAGE_MULTIPLIER, 1.0), (attr::SPEED, 10_000.0), (attr::MAX_RANGE, 20_000.0), (attr::AOE_CLOUD_SIZE, 200.0), (attr::AOE_VELOCITY, 80.0), (attr::AOE_DAMAGE_REDUCTION_FACTOR, 0.7), (attr::CHARGE_GROUP_1, 4061.0), (attr::CAPACITY, 1.0), (ARC_TARGETS, 3.0), (ARC_RANGE, 10_000.0)], &[371, 320, 330], &[]),
        type_def(VORTON_CHARGE, "ElectroPunch Ultra L", 4061, 8, &[(attr::EM_DAMAGE, 100.0), (attr::KINETIC_DAMAGE, 100.0), (attr::VOLUME, 0.01)], &[], &[]),
        type_def(BOMB_LAUNCHER, "Bomb Launcher I", 862, 7, &[(attr::SPEED, 10_000.0), (669, 80_000.0), (attr::CHARGE_GROUP_1, 90.0), (attr::CAPACITY, 150.0), (attr::RELOAD_TIME, 10_000.0)], &[331, 320, 330], &[]),
        type_def(BOMB, "Concussion Bomb", 90, 8, &[(attr::KINETIC_DAMAGE, 5800.0), (attr::MAX_VELOCITY, 2500.0), (attr::EXPLOSION_DELAY, 12_000.0), (attr::AOE_CLOUD_SIZE, 400.0), (EXPLOSION_RANGE, 15_000.0), (attr::VOLUME, 75.0)], &[372], &[]),
        type_def(BREACHER_LAUNCHER, "Small Breacher Pod Launcher", 4807, 7, &[(attr::SPEED, 24_000.0), (attr::CHARGE_GROUP_1, 4808.0), (attr::CAPACITY, 4.0)], &[331, 320, 330], &[]),
        type_def(BREACHER_POD, "SCARAB Breacher Pod S", 4808, 8, &[(attr::MAX_VELOCITY, 2000.0), (attr::EXPLOSION_DELAY, 2000.0), (DOT_PER_TICK, 200.0), (DOT_HP_PCT, 0.6), (DOT_DURATION, 40_000.0), (attr::VOLUME, 0.1)], &[373], &[]),
        type_def(DOOMSDAY, "'Judgment' Electromagnetic Doomsday", 588, 7, &[(attr::EM_DAMAGE, 2_400_000.0), (attr::DURATION, 300_000.0)], &[374, 320, 330], &[]),
        type_def(LANCE, "'Holy Destiny' Electromagnetic Lance", 588, 7, &[(attr::EM_DAMAGE, 50_000.0), (attr::DURATION, 300_000.0), (attr::MAX_RANGE, 200_000.0), (attr::SIGNATURE_RADIUS, 5000.0), (DD_TICK, 1000.0), (DD_DURATION, 15_000.0), (DD_RADIUS, 2500.0)], &[375, 320, 330], &[]),
        type_def(LOGI_DRONE, "Heavy Armor Maintenance Bot II", 640, 18, &[(attr::ARMOR_DAMAGE_AMOUNT, 72.0), (attr::DURATION, 5000.0), (attr::MAX_RANGE, 10_000.0)], &[376], &[]),
        type_def(WEB_DRONE, "Berserker SW-900", 641, 18, &[(attr::SPEED_FACTOR, -20.0), (attr::DURATION, 5000.0), (attr::MAX_RANGE, 10_000.0)], &[377], &[]),
        type_def(SUPPORT_FIGHTER, "Dromi I", 1537, 87, &[(FIGHTER_WEB, -10.0), (FIGHTER_WEB_RANGE, 8000.0), (FIGHTER_WEB_DURATION, 10_000.0), (SQUADRON_MAX, 3.0)], &[378], &[]),
        type_def(MUTA_REP, "Heavy Mutadaptive Remote Armor Repairer II", 2018, 7, &[(attr::ARMOR_DAMAGE_AMOUNT, 60.0), (attr::DURATION, 6000.0), (attr::MAX_RANGE, 10_000.0), (REP_SPOOL_MAX, 1.5), (REP_SPOOL_STEP, 0.1)], &[379, 320, 330], &[]),
        type_def(ARAR, "Large Ancillary Remote Armor Repairer", 1698, 7, &[(attr::ARMOR_DAMAGE_AMOUNT, 60.0), (attr::DURATION, 6000.0), (attr::MAX_RANGE, 7000.0), (attr::CHARGE_GROUP_1, GROUP_PASTE as f32), (attr::CAPACITY, 0.08), (attr::CHARGE_RATE, 1.0), (attr::RELOAD_TIME, 60_000.0), (attr::CHARGED_ARMOR_DAMAGE_MULTIPLIER, 3.0)], &[380, 320, 330], &[]),
        type_def(SENSOR_BOOST, "Remote Sensor Booster II", 290, 7, &[(attr::MAX_TARGET_RANGE_BONUS, 30.0), (attr::SCAN_RESOLUTION_BONUS, 30.0), (attr::DURATION, 5000.0), (attr::MAX_RANGE, 48_000.0), (attr::FALLOFF_EFFECTIVENESS, 96_000.0)], &[382, 320, 330], &[]),
        type_def(TRACKING_BOOST, "Remote Tracking Computer II", 646, 7, &[(attr::TRACKING_SPEED_BONUS, 25.0), (attr::MAX_RANGE_BONUS, 15.0), (attr::FALLOFF_BONUS, 25.0), (attr::DURATION, 5000.0), (attr::MAX_RANGE, 30_000.0), (attr::FALLOFF_EFFECTIVENESS, 60_000.0)], &[383, 320, 330], &[]),
        type_def(ECM_BURST, "Burst Jammer II", 80, 7, &[(attr::SCAN_GRAVIMETRIC_STRENGTH_BONUS, 4.0), (142, 6000.0), (attr::DURATION, 30_000.0), (attr::MAX_RANGE, 1.0)], &[384, 320, 330], &[]),
        type_def(MINER, "Modulated Strip Miner II", 483, 7, &[(attr::MINING_AMOUNT, 120.0), (attr::DURATION, 45_000.0), (CRIT_CHANCE, 0.01), (CRIT_YIELD, 2.0)], &[381, 320, 330], &[]),
        type_def(DEFENSE_MODE, "Rifter Defense Mode", crate::ids::group::SHIP_MODIFIERS, 7, &[], &[], &[]),
        type_def(PROPULSION_MODE, "Rifter Propulsion Mode", crate::ids::group::SHIP_MODIFIERS, 7, &[(2003, 0.5)], &[6020], &[]),
        type_def(BURST, "Shield Command Burst II", 316, 7, &[(attr::DURATION, 60_000.0), (attr::CAPACITOR_NEED, 25.0), (attr::CPU, 55.0), (attr::POWER, 110.0), (attr::WARFARE_BUFF_1_VALUE, 1.25), (attr::CHARGE_GROUP_1, 1769.0), (attr::CAPACITY, 10.0)], &[346, 320, 330], &[]),
        type_def(FIGHTER, "Templar II", 1652, 87, &[(2901, 20.0), (2902, 10.0), (2903, 0.0), (2904, 0.0), (2905, 1.5), (2906, 4000.0), (2907, 6000.0), (2908, 3000.0), (2909, 40.0), (2910, 200.0), (2911, 0.8), (attr::VOLUME, 5000.0), (attr::MAX_VELOCITY, 4000.0)], &[348], &[]),
        type_def(BURST_CHARGE, "Shield Harmonizing Charge", 1769, 8, &[(attr::WARFARE_BUFF_1_ID, 12.0), (2596, 20.0), (attr::VOLUME, 1.0)], &[347], &[]),
        type_def(SKIRM_BURST, "Skirmish Command Burst II", 316, 7, &[(attr::DURATION, 60_000.0), (attr::CAPACITOR_NEED, 25.0), (attr::CPU, 55.0), (attr::POWER, 110.0), (attr::WARFARE_BUFF_1_VALUE, 1.25), (attr::CHARGE_GROUP_1, 1770.0), (attr::CAPACITY, 10.0)], &[9003, 320, 330], &[]),
        type_def(SKIRM_CHARGE, "Rapid Deployment Charge", 1770, 8, &[(attr::WARFARE_BUFF_1_ID, 60.0), (2596, 15.0), (attr::VOLUME, 1.0)], &[347], &[]),
        type_def(T3C, "Loki", 963, 6, &[], &[], &[]),
        type_def(WEB_SUB, "Loki Core - Immobility Drivers", 958, 32, &[(FITS_TO_SHIP, T3C as f32), (SUB_WEB_BONUS, 125.0)], &[9004], &[]),
        type_def(BOOST_SUB, "Loki Offensive - Support Processor", 956, 32, &[(FITS_TO_SHIP, T3C as f32), (SUB_BURST_BONUS, 10.0)], &[9005], &[]),
        type_def(SHIELD_MINDLINK, "Shield Command Mindlink", 1229, 20, &[(attr::IMPLANTNESS, 10.0)], &[], &[]),
        type_def(SKIRM_MINDLINK, "Skirmish Command Mindlink", 1229, 20, &[(attr::IMPLANTNESS, 10.0)], &[], &[]),
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
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(6301, "moduleBonusAfterburner", EffectCategory::Active, vec![]) },
        effect(6303, "reactorControlUnit", EffectCategory::Online, vec![modifier(Domain::Ship, Func::Item, attr::POWER_OUTPUT, RCU_BONUS, Op::PostPercent)]),
        effect(307, "overloadSelfSpeedBonus", EffectCategory::Overload, vec![modifier(Domain::Item, Func::Item, attr::SPEED_FACTOR, attr::OVERLOAD_SPEED_FACTOR_BONUS, Op::PostPercent)]),
        effect(308, "implantVelocity", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, attr::MAX_VELOCITY, Op::PostPercent)]),
        effect(309, "boosterArmorRep", EffectCategory::Passive, vec![Modifier { group: Some(62), ..modifier(Domain::Ship, Func::LocationGroup, attr::ARMOR_DAMAGE_AMOUNT, attr::ARMOR_DAMAGE_AMOUNT, Op::PostPercent) }]),
        EffectDef { usage_chance_attr: Some(USAGE_CHANCE), ..effect(310, "boosterSideEffect", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, BOOSTER_SIDE, Op::PostPercent)]) },
        effect(9002, "moduleTitanEffectGenerator", EffectCategory::Active, vec![]),
        effect(9001, "systemMaxVelocity", EffectCategory::System, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, BEACON_SPEED, Op::PostPercent)]),
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(311, "remoteWebifierFalloff", EffectCategory::Target, vec![modifier(Domain::Target, Func::Item, attr::MAX_VELOCITY, WEB_SPEED_FACTOR, Op::PostPercent)]) },
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
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(350, "warpScrambleBlockMWDWithNPCEffect", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(351, "warpDisrupt", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(352, "warpDisruptSphere", EffectCategory::Active, vec![]) },
        effect(353, "scriptWarpScrambleRangeBonus", EffectCategory::Passive, vec![modifier(Domain::Other, Func::Item, attr::WARP_SCRAMBLE_RANGE, 1327, Op::PostPercent)]),
        EffectDef { duration_attr: Some(attr::SPEED), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(371, "ChainLightning", EffectCategory::Target, vec![]) },
        effect(372, "bombLaunching", EffectCategory::Passive, vec![]),
        effect(373, "dotMissileLaunching", EffectCategory::Passive, vec![]),
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(374, "superWeaponAmarr", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(375, "doomsdayBeamDOT", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(376, "npcEntityRemoteArmorRepairer", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(377, "remoteWebifierEntity", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(FIGHTER_WEB_DURATION), ..effect(378, "fighterAbilityStasisWebifier", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(379, "ShipModuleRemoteArmorMutadaptiveRepairer", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(380, "shipModuleAncillaryRemoteArmorRepairer", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(381, "miningLaser", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(382, "remoteSensorBoostFalloff", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(383, "shipModuleRemoteTrackingComputer", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(384, "ECMBurstJammer", EffectCategory::Active, vec![]) },
        effect(6020, "modeVelocityPostDiv", EffectCategory::Passive, vec![modifier(Domain::Ship, Func::Item, attr::MAX_VELOCITY, 2003, Op::PostDiv)]),
        EffectDef { duration_attr: Some(attr::DURATION), ..effect(349, "energyNosferatuFalloff", EffectCategory::Target, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(346, "moduleBonusWarfareLinkShield", EffectCategory::Active, vec![]) },
        EffectDef { duration_attr: Some(attr::DURATION), discharge_attr: Some(attr::CAPACITOR_NEED), ..effect(9003, "moduleBonusWarfareLinkSkirmish", EffectCategory::Active, vec![]) },
        effect(9004, "subsystemBonusWebRange", EffectCategory::Passive, vec![Modifier { group: Some(65), ..modifier(Domain::Ship, Func::LocationGroup, attr::MAX_RANGE, SUB_WEB_BONUS, Op::PostPercent) }]),
        effect(9005, "subsystemBonusBursts", EffectCategory::Passive, vec![Modifier { group: Some(316), ..modifier(Domain::Ship, Func::LocationGroup, attr::WARFARE_BUFF_1_VALUE, SUB_BURST_BONUS, Op::PostPercent) }]),
        // The charge names the buff and multiplies the module's strength into its value.
        EffectDef { duration_attr: Some(2906), ..effect(348, "fighterAbilityAttackM", EffectCategory::Target, vec![]) },
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
        attr_def(attr::WARP_SCRAMBLE_RANGE, "warpScrambleRange", false, true, 0.0),
        attr_def(attr::WARP_SCRAMBLE_STRENGTH, "warpScrambleStrength", false, true, 0.0),
        attr_def(142, "ecmBurstRange", false, true, 0.0),
        attr_def(1327, "warpScrambleRangeBonus", true, true, 0.0),
        attr_def(ARC_TARGETS, "VortonArcTargets", true, true, 0.0),
        attr_def(ARC_RANGE, "VortonArcRange", true, true, 0.0),
        attr_def(EXPLOSION_RANGE, "explosionRange", true, true, 0.0),
        attr_def(DOT_PER_TICK, "dotMaxDamagePerTick", true, true, 0.0),
        attr_def(DOT_HP_PCT, "dotMaxHPPercentagePerTick", true, true, 0.0),
        attr_def(DOT_DURATION, "dotDuration", true, true, 0.0),
        attr_def(DD_TICK, "doomsdayDamageCycleTime", true, true, 0.0),
        attr_def(DD_DURATION, "doomsdayDamageDuration", true, true, 0.0),
        attr_def(DD_RADIUS, "doomsdayDamageRadius", true, true, 0.0),
        attr_def(FIGHTER_WEB, "fighterAbilityStasisWebifierSpeedPenalty", true, true, 0.0),
        attr_def(FIGHTER_WEB_RANGE, "fighterAbilityStasisWebifierOptimalRange", true, true, 0.0),
        attr_def(FIGHTER_WEB_DURATION, "fighterAbilityStasisWebifierDuration", true, true, 0.0),
        attr_def(SQUADRON_MAX, "fighterSquadronMaxSize", true, true, 0.0),
        attr_def(JUMP_FUEL, "jumpDriveConsumptionType", true, true, 0.0),
        attr_def(FUEL_BAY, "specialFuelBayCapacity", true, true, 0.0),
        attr_def(FLEET_HANGAR, "fleetHangarCapacity", true, true, 0.0),
        attr_def(REP_SPOOL_MAX, "repairMultiplierBonusMax", true, true, 0.0),
        attr_def(REP_SPOOL_STEP, "repairMultiplierBonusPerCycle", true, true, 0.0),
        attr_def(CRIT_CHANCE, "miningCritChance", true, true, 0.0),
        attr_def(CRIT_YIELD, "miningCritBonusYield", true, true, 0.0),
        attr_def(MAX_GROUP_ACTIVE, "maxGroupActive", true, true, 0.0),
        attr_def(attr::MINING_AMOUNT, "miningAmount", true, true, 0.0),
        attr_def(669, "moduleReactivationDelay", true, false, 0.0),
        attr_def(attr::DAMAGE_MULTIPLIER, "damageMultiplier", false, true, 1.0),
        attr_def(attr::SKILL_LEVEL, "skillLevel", true, true, 0.0),
        attr_def(SHIP_BONUS_MF, "shipBonusMF2", true, true, 0.0),
        attr_def(FITS_TO_SHIP, "fitsToShipType", true, true, 0.0),
        attr_def(SUB_WEB_BONUS, "subsystemBonusWebRange", true, true, 0.0),
        attr_def(SUB_BURST_BONUS, "subsystemBonusBursts", true, true, 0.0),
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
        attr_def(attr::CPU, "cpu", true, false, 0.0),
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
        attr_def(2901, "fighterAbilityAttackMissileDamageEM", true, true, 0.0),
        attr_def(2902, "fighterAbilityAttackMissileDamageTherm", true, true, 0.0),
        attr_def(2903, "fighterAbilityAttackMissileDamageKin", true, true, 0.0),
        attr_def(2904, "fighterAbilityAttackMissileDamageExp", true, true, 0.0),
        attr_def(2905, "fighterAbilityAttackMissileDamageMultiplier", true, true, 0.0),
        attr_def(2906, "fighterAbilityAttackMissileDuration", true, true, 0.0),
        attr_def(2907, "fighterAbilityAttackMissileRangeOptimal", true, true, 0.0),
        attr_def(2908, "fighterAbilityAttackMissileRangeFalloff", true, true, 0.0),
        attr_def(2909, "fighterAbilityAttackMissileExplosionRadius", true, true, 0.0),
        attr_def(2910, "fighterAbilityAttackMissileExplosionVelocity", true, true, 0.0),
        attr_def(2911, "fighterAbilityAttackMissileReductionFactor", true, true, 0.0),
    ];
    let groups = vec![
        GroupDef { id: 1, name: "Character".into(), category: 1 },
        GroupDef { id: 25, name: "Frigate".into(), category: 6 },
        GroupDef { id: 958, name: "Core Systems".into(), category: 32 },
        GroupDef { id: 956, name: "Offensive Systems".into(), category: 32 },
        GroupDef { id: 30, name: "Titan".into(), category: 6 },
        GroupDef { id: crate::ids::group::EFFECT_BEACON, name: "Effect Beacon".into(), category: 2 },
        GroupDef { id: 1815, name: "Titan Phenomena Generator".into(), category: 7 },
        GroupDef { id: 46, name: "Propulsion Module".into(), category: 7 },
        GroupDef { id: 908, name: "Warp Disruption Script".into(), category: 8 },
        GroupDef { id: 62, name: "Armor Repair Unit".into(), category: 7 },
        GroupDef { id: 55, name: "Projectile Weapon".into(), category: 7 },
        GroupDef { id: GROUP_AMMO, name: "Projectile Ammo".into(), category: 8 },
        GroupDef { id: GROUP_PASTE, name: "Nanite Repair Paste".into(), category: 8 },
        GroupDef { id: GROUP_CAP_CHARGE, name: "Capacitor Booster Charge".into(), category: 8 },
        GroupDef { id: 386, name: "Cruise Missile".into(), category: 8 },
    ];
    // A market tree like the game's, so a browser over this data has something to show.
    let market_groups = vec![
        market(4, "Ships", None),
        market(1361, "Frigates", Some(4)),
        market(9, "Ship Equipment", None),
        market(10, "Turrets & Launchers", Some(9)),
        market(11, "Projectile Turrets", Some(10)),
        market(12, "Missile Launchers", Some(10)),
        market(13, "Propulsion", Some(9)),
        market(14, "Hull & Armor", Some(9)),
        market(15, "Electronic Warfare", Some(9)),
        market(16, "Ship Modifications", None),
        market(17, "Rigs", Some(16)),
        market(18, "Subsystems", Some(16)),
        market(19, "Ammunition & Charges", None),
        market(20, "Projectile Ammo", Some(19)),
        market(21, "Missiles", Some(19)),
        market(22, "Other Charges", Some(19)),
        market(23, "Drones", None),
        market(24, "Implants & Boosters", None),
        market(25, "Fighters", None),
    ];
    let mut types = types;
    for t in &mut types {
        t.market_group = match (t.category, t.group) {
            (6, _) => Some(1361),
            (7, 55) => Some(11),
            (7, 506) => Some(12),
            (7, 46) => Some(13),
            (7, 65) => Some(15),
            (7, 773 | 774 | 775 | 776 | 777 | 778) => Some(17),
            (7, _) => Some(14),
            (32, _) => Some(18),
            (8, 83) => Some(20),
            (8, 384 | 385 | 386 | 387 | 388 | 389 | 390 | 391 | 392 | 393 | 394) => Some(21),
            (8, _) => Some(22),
            (18, _) => Some(23),
            (20, _) => Some(24),
            (87, _) => Some(25),
            _ => None,
        };
    }
    let mutaplasmids = vec![crate::dogma::Mutaplasmid {
        id: MUTAPLASMID,
        name: "Unstable Microwarpdrive Mutaplasmid".into(),
        attrs: vec![(attr::SPEED_FACTOR, 0.9, 1.1), (attr::CPU, 0.8, 1.5)],
        mapping: vec![(vec![MWD], ABYSSAL_MWD)],
    }];
    let traits = vec![crate::dogma::TypeTraits {
        type_id: SHIP,
        role: vec![crate::dogma::Bonus { amount: Some(100.0), unit: Some(105), text: "bonus to the effectiveness of something the role does, long enough to wrap onto a second line".into() }],
        misc: vec![],
        per_skill: vec![(SKILL_MF, vec![crate::dogma::Bonus { amount: Some(5.0), unit: Some(105), text: "bonus to Small Projectile Turret damage".into() }, crate::dogma::Bonus { amount: Some(7.5), unit: Some(105), text: "bonus to Small Projectile Turret falloff".into() }])],
    }];
    Dogma { sde_build: 1, types, attributes, effects, groups, market_groups, mutaplasmids, traits, ..Dogma::default() }.finish()
}
