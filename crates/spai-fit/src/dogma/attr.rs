//! Attribute ids the engine reads by number. `NAMED` pairs each with its static-data name so a
//! bake of the real data can confirm the numbers (`the_real_blob_agrees_with_the_named_attributes`).

use crate::ids::AttrId;

pub const MASS: AttrId = 4;
pub const CAPACITY: AttrId = 38;
pub const VOLUME: AttrId = 161;
pub const RADIUS: AttrId = 162;
pub const AGILITY: AttrId = 70;
pub const SKILL_LEVEL: AttrId = 280;
pub const DAMAGE_MULTIPLIER_BONUS: AttrId = 292;
pub const ROF_BONUS: AttrId = 293;
pub const TURRET_HARDPOINT_MODIFIER: AttrId = 1368;
pub const LAUNCHER_HARDPOINT_MODIFIER: AttrId = 1369;
pub const HI_SLOT_MODIFIER: AttrId = 1374;
pub const MED_SLOT_MODIFIER: AttrId = 1375;
pub const LOW_SLOT_MODIFIER: AttrId = 1376;
pub const IS_ONLINE: AttrId = 2;
pub const DAMAGE: AttrId = 3;
pub const HP: AttrId = 9;
pub const POWER_OUTPUT: AttrId = 11;
pub const LOW_SLOTS: AttrId = 12;
pub const MED_SLOTS: AttrId = 13;
pub const HI_SLOTS: AttrId = 14;
pub const SPEED_FACTOR: AttrId = 20;
pub const POWER: AttrId = 30;
pub const MAX_VELOCITY: AttrId = 37;
pub const CPU_OUTPUT: AttrId = 48;
pub const CPU: AttrId = 50;
pub const SPEED: AttrId = 51;
pub const MAX_RANGE: AttrId = 54;
pub const RECHARGE_RATE: AttrId = 55;
pub const CHARGE_RATE: AttrId = 56;
pub const DAMAGE_MULTIPLIER: AttrId = 64;
pub const SHIELD_BONUS: AttrId = 68;
pub const DURATION: AttrId = 73;
pub const MAX_TARGET_RANGE: AttrId = 76;
pub const STRUCTURE_DAMAGE_AMOUNT: AttrId = 83;
pub const ARMOR_DAMAGE_AMOUNT: AttrId = 84;
pub const POWER_TRANSFER_AMOUNT: AttrId = 90;
pub const ENERGY_NEUTRALIZER_AMOUNT: AttrId = 97;
pub const LAUNCHER_SLOTS_LEFT: AttrId = 101;
pub const TURRET_SLOTS_LEFT: AttrId = 102;
pub const KINETIC_RESONANCE: AttrId = 109;
pub const THERMAL_RESONANCE: AttrId = 110;
pub const EXPLOSIVE_RESONANCE: AttrId = 111;
pub const EM_RESONANCE: AttrId = 113;
pub const EM_DAMAGE: AttrId = 114;
pub const EXPLOSIVE_DAMAGE: AttrId = 116;
pub const KINETIC_DAMAGE: AttrId = 117;
pub const THERMAL_DAMAGE: AttrId = 118;
pub const AMMO_LOADED: AttrId = 127;
pub const CHARGE_SIZE: AttrId = 128;
pub const LAUNCHER_GROUP: AttrId = 137;
pub const FALLOFF: AttrId = 158;
pub const TRACKING_SPEED: AttrId = 160;
pub const MAX_LOCKED_TARGETS: AttrId = 192;
pub const SCAN_RADAR_STRENGTH: AttrId = 208;
pub const SCAN_LADAR_STRENGTH: AttrId = 209;
pub const SCAN_MAGNETOMETRIC_STRENGTH: AttrId = 210;
pub const SCAN_GRAVIMETRIC_STRENGTH: AttrId = 211;
pub const MISSILE_DAMAGE_MULTIPLIER: AttrId = 212;
pub const MAX_LOCKED_TARGETS_BONUS: AttrId = 235;
pub const SHIELD_CAPACITY: AttrId = 263;
pub const ARMOR_HP: AttrId = 265;
pub const ARMOR_EM_RESONANCE: AttrId = 267;
pub const ARMOR_EXPLOSIVE_RESONANCE: AttrId = 268;
pub const ARMOR_KINETIC_RESONANCE: AttrId = 269;
pub const ARMOR_THERMAL_RESONANCE: AttrId = 270;
pub const SHIELD_EM_RESONANCE: AttrId = 271;
pub const SHIELD_EXPLOSIVE_RESONANCE: AttrId = 272;
pub const SHIELD_KINETIC_RESONANCE: AttrId = 273;
pub const SHIELD_THERMAL_RESONANCE: AttrId = 274;
pub const EXPLOSION_DELAY: AttrId = 281;
pub const DRONE_CAPACITY: AttrId = 283;
pub const IMPLANTNESS: AttrId = 331;
pub const MAX_ACTIVE_DRONES: AttrId = 352;
pub const DRONE_CONTROL_DISTANCE: AttrId = 458;
pub const SHIELD_RECHARGE_RATE: AttrId = 479;
pub const CAPACITOR_CAPACITY: AttrId = 482;
pub const SIGNATURE_RADIUS: AttrId = 552;
pub const SIGNATURE_RADIUS_BONUS: AttrId = 554;
pub const SCAN_RESOLUTION: AttrId = 564;
pub const SPEED_BOOST_FACTOR: AttrId = 567;
pub const WARP_SPEED_MULTIPLIER: AttrId = 600;
pub const CHARGE_GROUP_1: AttrId = 604;
pub const OPTIMAL_SIG_RADIUS: AttrId = 620;
pub const AOE_VELOCITY: AttrId = 653;
pub const AOE_CLOUD_SIZE: AttrId = 654;
pub const MODULE_REACTIVATION_DELAY: AttrId = 669;
pub const MAX_GROUP_ACTIVE: AttrId = 763;
pub const CRYSTAL_VOLATILITY_CHANCE: AttrId = 783;
pub const CRYSTAL_VOLATILITY_DAMAGE: AttrId = 784;
pub const CRYSTALS_GET_DAMAGED: AttrId = 786;
pub const MASS_ADDITION: AttrId = 796;
pub const BOOSTERNESS: AttrId = 1087;
pub const UPGRADE_CAPACITY: AttrId = 1132;
pub const RIG_SLOTS: AttrId = 1137;
pub const UPGRADE_COST: AttrId = 1153;
pub const OVERLOAD_ROF_BONUS: AttrId = 1205;
pub const OVERLOAD_SELF_DURATION_BONUS: AttrId = 1206;
pub const OVERLOAD_HARDENING_BONUS: AttrId = 1208;
pub const OVERLOAD_DAMAGE_MODIFIER: AttrId = 1210;
pub const HEAT_DAMAGE: AttrId = 1211;
pub const OVERLOAD_SPEED_FACTOR_BONUS: AttrId = 1223;
pub const OVERLOAD_ARMOR_DAMAGE_AMOUNT: AttrId = 1230;
pub const OVERLOAD_SHIELD_BONUS: AttrId = 1231;
pub const DRONE_BANDWIDTH: AttrId = 1271;
pub const BASE_WARP_SPEED: AttrId = 1281;
pub const AOE_DAMAGE_REDUCTION_FACTOR: AttrId = 1353;
pub const SUBSYSTEM_SLOT: AttrId = 1366;
pub const MAX_SUBSYSTEMS: AttrId = 1367;
pub const MAX_GROUP_FITTED: AttrId = 1544;
pub const RIG_SIZE: AttrId = 1547;
pub const RELOAD_TIME: AttrId = 1795;
pub const CHARGED_ARMOR_DAMAGE_MULTIPLIER: AttrId = 1886;
pub const SERVICE_SLOTS: AttrId = 2056;
pub const FIGHTER_SQUADRON_MAX_SIZE: AttrId = 2215;
pub const ENERGY_NEUTRALIZER_SIGNATURE_RESOLUTION: AttrId = 2451;
pub const WARFARE_BUFF_1_ID: AttrId = 2468;
pub const WARFARE_BUFF_1_VALUE: AttrId = 2469;
pub const WARFARE_BUFF_2_ID: AttrId = 2470;
pub const WARFARE_BUFF_2_VALUE: AttrId = 2471;
pub const WARFARE_BUFF_3_ID: AttrId = 2472;
pub const WARFARE_BUFF_3_VALUE: AttrId = 2473;
pub const BUFF_DURATION: AttrId = 2535;
pub const WARFARE_BUFF_4_ID: AttrId = 2536;
pub const WARFARE_BUFF_4_VALUE: AttrId = 2537;

/// `requiredSkill1..6` with their `requiredSkill1Level..` partners.
pub const REQUIRED_SKILLS: [(AttrId, AttrId); 6] = [(182, 277), (183, 278), (184, 279), (1285, 1286), (1289, 1287), (1290, 1288)];

pub const NAMED: &[(AttrId, &str)] = &[
    (MASS, "mass"),
    (CAPACITY, "capacity"),
    (VOLUME, "volume"),
    (RADIUS, "radius"),
    (AGILITY, "agility"),
    (SKILL_LEVEL, "skillLevel"),
    (DAMAGE_MULTIPLIER_BONUS, "damageMultiplierBonus"),
    (ROF_BONUS, "rofBonus"),
    (TURRET_HARDPOINT_MODIFIER, "turretHardPointModifier"),
    (LAUNCHER_HARDPOINT_MODIFIER, "launcherHardPointModifier"),
    (HI_SLOT_MODIFIER, "hiSlotModifier"),
    (MED_SLOT_MODIFIER, "medSlotModifier"),
    (LOW_SLOT_MODIFIER, "lowSlotModifier"),
    (IS_ONLINE, "isOnline"),
    (DAMAGE, "damage"),
    (HP, "hp"),
    (POWER_OUTPUT, "powerOutput"),
    (LOW_SLOTS, "lowSlots"),
    (MED_SLOTS, "medSlots"),
    (HI_SLOTS, "hiSlots"),
    (SPEED_FACTOR, "speedFactor"),
    (POWER, "power"),
    (MAX_VELOCITY, "maxVelocity"),
    (CPU_OUTPUT, "cpuOutput"),
    (CPU, "cpu"),
    (SPEED, "speed"),
    (MAX_RANGE, "maxRange"),
    (RECHARGE_RATE, "rechargeRate"),
    (CHARGE_RATE, "chargeRate"),
    (DAMAGE_MULTIPLIER, "damageMultiplier"),
    (SHIELD_BONUS, "shieldBonus"),
    (DURATION, "duration"),
    (MAX_TARGET_RANGE, "maxTargetRange"),
    (STRUCTURE_DAMAGE_AMOUNT, "structureDamageAmount"),
    (ARMOR_DAMAGE_AMOUNT, "armorDamageAmount"),
    (POWER_TRANSFER_AMOUNT, "powerTransferAmount"),
    (ENERGY_NEUTRALIZER_AMOUNT, "energyNeutralizerAmount"),
    (LAUNCHER_SLOTS_LEFT, "launcherSlotsLeft"),
    (TURRET_SLOTS_LEFT, "turretSlotsLeft"),
    (KINETIC_RESONANCE, "kineticDamageResonance"),
    (THERMAL_RESONANCE, "thermalDamageResonance"),
    (EXPLOSIVE_RESONANCE, "explosiveDamageResonance"),
    (EM_RESONANCE, "emDamageResonance"),
    (EM_DAMAGE, "emDamage"),
    (EXPLOSIVE_DAMAGE, "explosiveDamage"),
    (KINETIC_DAMAGE, "kineticDamage"),
    (THERMAL_DAMAGE, "thermalDamage"),
    (AMMO_LOADED, "ammoLoaded"),
    (CHARGE_SIZE, "chargeSize"),
    (LAUNCHER_GROUP, "launcherGroup"),
    (FALLOFF, "falloff"),
    (TRACKING_SPEED, "trackingSpeed"),
    (MAX_LOCKED_TARGETS, "maxLockedTargets"),
    (SCAN_RADAR_STRENGTH, "scanRadarStrength"),
    (SCAN_LADAR_STRENGTH, "scanLadarStrength"),
    (SCAN_MAGNETOMETRIC_STRENGTH, "scanMagnetometricStrength"),
    (SCAN_GRAVIMETRIC_STRENGTH, "scanGravimetricStrength"),
    (MISSILE_DAMAGE_MULTIPLIER, "missileDamageMultiplier"),
    (MAX_LOCKED_TARGETS_BONUS, "maxLockedTargetsBonus"),
    (SHIELD_CAPACITY, "shieldCapacity"),
    (ARMOR_HP, "armorHP"),
    (ARMOR_EM_RESONANCE, "armorEmDamageResonance"),
    (ARMOR_EXPLOSIVE_RESONANCE, "armorExplosiveDamageResonance"),
    (ARMOR_KINETIC_RESONANCE, "armorKineticDamageResonance"),
    (ARMOR_THERMAL_RESONANCE, "armorThermalDamageResonance"),
    (SHIELD_EM_RESONANCE, "shieldEmDamageResonance"),
    (SHIELD_EXPLOSIVE_RESONANCE, "shieldExplosiveDamageResonance"),
    (SHIELD_KINETIC_RESONANCE, "shieldKineticDamageResonance"),
    (SHIELD_THERMAL_RESONANCE, "shieldThermalDamageResonance"),
    (EXPLOSION_DELAY, "explosionDelay"),
    (DRONE_CAPACITY, "droneCapacity"),
    (IMPLANTNESS, "implantness"),
    (MAX_ACTIVE_DRONES, "maxActiveDrones"),
    (DRONE_CONTROL_DISTANCE, "droneControlDistance"),
    (SHIELD_RECHARGE_RATE, "shieldRechargeRate"),
    (CAPACITOR_CAPACITY, "capacitorCapacity"),
    (SIGNATURE_RADIUS, "signatureRadius"),
    (SIGNATURE_RADIUS_BONUS, "signatureRadiusBonus"),
    (SCAN_RESOLUTION, "scanResolution"),
    (SPEED_BOOST_FACTOR, "speedBoostFactor"),
    (WARP_SPEED_MULTIPLIER, "warpSpeedMultiplier"),
    (CHARGE_GROUP_1, "chargeGroup1"),
    (OPTIMAL_SIG_RADIUS, "optimalSigRadius"),
    (AOE_VELOCITY, "aoeVelocity"),
    (AOE_CLOUD_SIZE, "aoeCloudSize"),
    (MODULE_REACTIVATION_DELAY, "moduleReactivationDelay"),
    (MAX_GROUP_ACTIVE, "maxGroupActive"),
    (CRYSTAL_VOLATILITY_CHANCE, "crystalVolatilityChance"),
    (CRYSTAL_VOLATILITY_DAMAGE, "crystalVolatilityDamage"),
    (CRYSTALS_GET_DAMAGED, "crystalsGetDamaged"),
    (MASS_ADDITION, "massAddition"),
    (BOOSTERNESS, "boosterness"),
    (UPGRADE_CAPACITY, "upgradeCapacity"),
    (RIG_SLOTS, "rigSlots"),
    (UPGRADE_COST, "upgradeCost"),
    (OVERLOAD_ROF_BONUS, "overloadRofBonus"),
    (OVERLOAD_SELF_DURATION_BONUS, "overloadSelfDurationBonus"),
    (OVERLOAD_HARDENING_BONUS, "overloadHardeningBonus"),
    (OVERLOAD_DAMAGE_MODIFIER, "overloadDamageModifier"),
    (HEAT_DAMAGE, "heatDamage"),
    (OVERLOAD_SPEED_FACTOR_BONUS, "overloadSpeedFactorBonus"),
    (OVERLOAD_ARMOR_DAMAGE_AMOUNT, "overloadArmorDamageAmount"),
    (OVERLOAD_SHIELD_BONUS, "overloadShieldBonus"),
    (DRONE_BANDWIDTH, "droneBandwidth"),
    (BASE_WARP_SPEED, "baseWarpSpeed"),
    (AOE_DAMAGE_REDUCTION_FACTOR, "aoeDamageReductionFactor"),
    (SUBSYSTEM_SLOT, "subSystemSlot"),
    (MAX_SUBSYSTEMS, "maxSubSystems"),
    (MAX_GROUP_FITTED, "maxGroupFitted"),
    (RIG_SIZE, "rigSize"),
    (RELOAD_TIME, "reloadTime"),
    (CHARGED_ARMOR_DAMAGE_MULTIPLIER, "chargedArmorDamageMultiplier"),
    (SERVICE_SLOTS, "serviceSlots"),
    (FIGHTER_SQUADRON_MAX_SIZE, "fighterSquadronMaxSize"),
    (ENERGY_NEUTRALIZER_SIGNATURE_RESOLUTION, "energyNeutralizerSignatureResolution"),
    (WARFARE_BUFF_1_ID, "warfareBuff1ID"),
    (WARFARE_BUFF_1_VALUE, "warfareBuff1Value"),
    (WARFARE_BUFF_2_ID, "warfareBuff2ID"),
    (WARFARE_BUFF_2_VALUE, "warfareBuff2Value"),
    (WARFARE_BUFF_3_ID, "warfareBuff3ID"),
    (WARFARE_BUFF_3_VALUE, "warfareBuff3Value"),
    (BUFF_DURATION, "buffDuration"),
    (WARFARE_BUFF_4_ID, "warfareBuff4ID"),
    (WARFARE_BUFF_4_VALUE, "warfareBuff4Value"),
    (182, "requiredSkill1"),
    (277, "requiredSkill1Level"),
    (1290, "requiredSkill6"),
    (1288, "requiredSkill6Level"),
];
