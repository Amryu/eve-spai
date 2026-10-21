//! What a fit does, read off the resolved item tree: fitting room, tank, weapons, drones,
//! capacitor, mobility and sensors. Pure functions of a [`Calc`]; formulas are in
//! docs/FITTING_MECHANICS.md.

use crate::calc::{Calc, Env, ItemIdx, ItemKind};
use crate::dogma::{attr, Dogma, EffectCategory};
use crate::fit::{DamageProfile, Fit, Slot, State};
use crate::ids::{AttrId, GroupId, TypeId};
use crate::skills::SkillSet;

pub mod application;
pub mod cap_sim;

/// The four damage types, in EVE's order: em, thermal, kinetic, explosive.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Damage {
    pub em: f64,
    pub thermal: f64,
    pub kinetic: f64,
    pub explosive: f64,
}

impl Damage {
    pub fn total(&self) -> f64 {
        self.em + self.thermal + self.kinetic + self.explosive
    }

    pub fn scaled(&self, k: f64) -> Damage {
        Damage { em: self.em * k, thermal: self.thermal * k, kinetic: self.kinetic * k, explosive: self.explosive * k }
    }

    pub fn add(&mut self, other: Damage) {
        self.em += other.em;
        self.thermal += other.thermal;
        self.kinetic += other.kinetic;
        self.explosive += other.explosive;
    }

    /// What gets through resonances (em, thermal, kinetic, explosive).
    pub fn through(&self, resonance: [f64; 4]) -> f64 {
        self.em * resonance[0] + self.thermal * resonance[1] + self.kinetic * resonance[2] + self.explosive * resonance[3]
    }

    /// Which types carry damage.
    pub fn types(&self) -> [bool; 4] {
        [self.em > 0.0, self.thermal > 0.0, self.kinetic > 0.0, self.explosive > 0.0]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fitting {
    pub cpu_used: f64,
    pub cpu_total: f64,
    pub power_used: f64,
    pub power_total: f64,
    pub calibration_used: f64,
    pub calibration_total: f64,
    /// Used and available per rack, in `Slot::ALL` order.
    pub slots: [(u32, u32); 6],
    pub turrets_used: u32,
    pub turrets_total: u32,
    pub launchers_used: u32,
    pub launchers_total: u32,
}

/// One of shield, armour and hull.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layer {
    pub hp: f64,
    /// em, thermal, kinetic, explosive; a resonance of 0.6 is 40% resistance.
    pub resonance: [f64; 4],
    pub ehp: f64,
}

impl Layer {
    pub fn resist(&self, i: usize) -> f64 {
        1.0 - self.resonance[i]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reps {
    /// Hit points per second, reloads counted.
    pub shield: f64,
    pub armor: f64,
    pub hull: f64,
    /// Hit points per second while charged or before the first reload.
    pub shield_burst: f64,
    pub armor_burst: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tank {
    pub shield: Layer,
    pub armor: Layer,
    pub hull: Layer,
    pub ehp: f64,
    /// Shield regenerated per second at the 25% peak.
    pub shield_peak_regen: f64,
    /// Seconds for the shield to recharge in full.
    pub shield_recharge_s: f64,
    pub reps: Reps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponKind {
    Turret,
    Missile,
    Drone,
    Smartbomb,
    Fighter,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MissileProfile {
    pub velocity: f64,
    pub flight_time_s: f64,
    pub range: f64,
    pub explosion_radius: f64,
    pub explosion_velocity: f64,
    pub damage_reduction_factor: f64,
}

impl MissileProfile {
    /// How far a missile surely flies: the server moves it in whole seconds, so it flies the
    /// whole seconds of its flight time.
    pub fn short_range(&self) -> f64 {
        self.velocity * self.flight_time_s.floor()
    }

    /// How far it may fly: one second more, as often as the flight time's fraction of a second.
    pub fn long_range(&self) -> f64 {
        self.velocity * self.flight_time_s.ceil()
    }

    /// The share of missiles that reach `distance`.
    pub fn reach(&self, distance: f64) -> f64 {
        if distance <= self.short_range() {
            1.0
        } else if distance <= self.long_range() {
            self.flight_time_s.fract()
        } else {
            0.0
        }
    }
}

/// Identical weapons with the same charge, counted together.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponGroup {
    pub kind: WeaponKind,
    pub type_id: TypeId,
    pub charge: Option<TypeId>,
    pub count: u32,
    pub items: Vec<ItemIdx>,
    /// Of the whole group, per cycle.
    pub volley: Damage,
    pub cycle_s: f64,
    pub reload_s: f64,
    /// Cycles per load; `None` when nothing runs out.
    pub shots: Option<u32>,
    /// Damage per second with reloads counted.
    pub dps: f64,
    /// Damage per second within one load.
    pub dps_burst: f64,
    pub optimal: f64,
    pub falloff: f64,
    pub tracking: f64,
    pub signature_resolution: f64,
    pub missile: Option<MissileProfile>,
    /// Capacitor drawn per second.
    pub cap_per_s: f64,
    /// Overheated and still cold damage multipliers are already in `volley`; this is the extra a
    /// fully spooled disintegrator reaches over its cold shot.
    pub spool_max: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drones {
    pub control_range: f64,
    pub bandwidth_used: f64,
    pub bandwidth_total: f64,
    pub bay_used: f64,
    pub bay_total: f64,
    pub active: u32,
    pub max_active: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Capacitor {
    pub capacity: f64,
    /// The recharge time constant in seconds.
    pub recharge_s: f64,
    /// GJ per second at the 25% peak.
    pub peak_regen: f64,
    /// GJ per second the active modules draw, boosters subtracted.
    pub usage: f64,
    /// Where the capacitor settles, as a fraction, when it does.
    pub stable_at: Option<f64>,
    /// Seconds until empty when it does not.
    pub lasts_s: Option<f64>,
    /// The closed-form estimate EVE's fitting window shows, when the usage allows one.
    pub estimate_stable_at: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mobility {
    pub max_velocity: f64,
    pub mass: f64,
    pub agility: f64,
    pub align_s: f64,
    pub warp_speed_au: f64,
    pub signature: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SensorKind {
    #[default]
    Radar,
    Ladar,
    Magnetometric,
    Gravimetric,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sensors {
    pub scan_resolution: f64,
    pub lock_range: f64,
    pub max_targets: u32,
    pub strength: f64,
    pub kind: SensorKind,
}

impl Sensors {
    /// Seconds to lock a target of the signature radius.
    pub fn lock_time_s(&self, signature: f64) -> f64 {
        lock_time(self.scan_resolution, signature)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RemoteKind {
    ArmorRep,
    ShieldRep,
    HullRep,
    CapTransfer,
    Neut,
    Nos,
    Web,
    Painter,
    Damp,
    TrackingDisruptor,
    GuidanceDisruptor,
    Ecm,
    Scram,
    Point,
}

impl RemoteKind {
    /// Repairs and capacitor given, as opposed to ewar done.
    pub fn is_rep(self) -> bool {
        matches!(self, RemoteKind::ArmorRep | RemoteKind::ShieldRep | RemoteKind::HullRep | RemoteKind::CapTransfer)
    }
}

/// One active module acting on another ship.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteModule {
    pub type_id: TypeId,
    pub kind: RemoteKind,
    /// Repairs and capacitor per second; ewar as its strength: percent for webs, painters, damps
    /// and disruptors, sensor points for ECM, warp points for tackle.
    pub amount: f64,
    pub optimal: f64,
    pub falloff: f64,
}

/// What the fit does to others per second: repairs, capacitor and neuts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Remote {
    pub armor_rep: f64,
    pub shield_rep: f64,
    pub hull_rep: f64,
    pub cap_transfer: f64,
    pub neut: f64,
    /// The longest reach of the remote repairers, optimal plus falloff.
    pub rep_range: f64,
    pub modules: Vec<RemoteModule>,
}

/// How much of a remote effect lands at `distance`: all of it inside the optimal, half at one
/// falloff, nothing past three.
pub fn range_factor(distance: f64, optimal: f64, falloff: f64) -> f64 {
    let over = (distance - optimal).max(0.0);
    if over <= 0.0 {
        1.0
    } else if falloff <= 0.0 || over > 3.0 * falloff {
        0.0
    } else {
        0.5f64.powf((over / falloff).powi(2))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Computed {
    /// The fastest drone in space, for how well drones keep up.
    pub drone_speed: Option<f64>,
    pub remote: Remote,
    pub fitting: Fitting,
    pub tank: Tank,
    pub weapons: Vec<WeaponGroup>,
    pub drones: Drones,
    pub cap: Capacitor,
    pub mobility: Mobility,
    pub sensors: Sensors,
    /// The cargo hold first, then every other hold the hull has.
    pub holds: Vec<Hold>,
    pub unknown_types: Vec<TypeId>,
}

impl Computed {
    pub fn dps(&self) -> f64 {
        self.weapons.iter().map(|w| w.dps).sum()
    }

    pub fn volley(&self) -> f64 {
        self.weapons.iter().map(|w| w.volley.total()).sum()
    }
}

pub fn compute(dogma: &Dogma, fit: &Fit, skills: &SkillSet, env: &Env) -> Computed {
    let calc = Calc::new(dogma, fit, skills, env);
    compute_with(&calc, fit)
}

pub fn compute_with(calc: &Calc, fit: &Fit) -> Computed {
    let weapons = weapons(calc, fit);
    let drone_speed = calc.drones().filter(|(_, it)| it.state >= State::Active).map(|(i, _)| calc.value(i, attr::MAX_VELOCITY)).fold(None, |best: Option<f64>, v| Some(best.map_or(v, |b| b.max(v))));
    Computed {
        drone_speed,
        remote: remote(calc),
        fitting: fitting(calc, fit),
        tank: tank(calc, fit),
        drones: drones(calc),
        cap: capacitor(calc, &weapons),
        mobility: mobility(calc),
        sensors: sensors(calc),
        holds: holds(calc, fit),
        weapons,
        unknown_types: calc.unknown_types.clone(),
    }
}

/// A place things are carried: its capacity with skills and modules counted, and what fills it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hold {
    /// The ship attribute giving its capacity; none is the cargo hold.
    pub attr: Option<AttrId>,
    pub name: String,
    pub capacity: f64,
    pub used: f64,
}

impl Hold {
    pub fn fraction(&self) -> f64 {
        if self.capacity > 0.0 {
            self.used / self.capacity
        } else if self.used > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    }

    pub fn over(&self) -> bool {
        self.used > self.capacity + 1e-9
    }
}

/// Whether an attribute is a hold's capacity: named for a hold, bay or hangar, and shown to the
/// pilot, which leaves out the bonuses to them. The drone and fighter bays have their own screens.
pub fn is_hold_attr(a: &crate::dogma::AttrDef) -> bool {
    let n = a.name.to_lowercase();
    !a.display.is_empty() && n.ends_with("capacity") && (n.contains("hold") || n.contains("bay") || n.contains("hangar")) && !n.starts_with("drone") && !n.starts_with("fighter")
}

/// The cargo hold and the hull's other holds, with what the fit carries in each.
pub fn holds(calc: &Calc, fit: &Fit) -> Vec<Hold> {
    let dogma = calc.dogma;
    let volume = |t: TypeId| dogma.type_(t).and_then(|d| d.attr(attr::VOLUME)).map_or(0.0, f64::from);
    let used = |hold: Option<AttrId>| fit.cargo.iter().filter(|c| c.hold == hold).map(|c| volume(c.type_id) * f64::from(c.quantity)).sum::<f64>();
    let mut out = vec![Hold { attr: None, name: "Cargo hold".into(), capacity: calc.ship(attr::CAPACITY), used: used(None) }];
    for a in dogma.attributes.iter().filter(|a| is_hold_attr(a)) {
        let capacity = calc.ship(a.id);
        let carried = used(Some(a.id));
        // A hold the hull has, or one something was put in before the hull changed.
        if capacity > 0.0 || carried > 0.0 {
            out.push(Hold { attr: Some(a.id), name: a.display.clone(), capacity, used: carried });
        }
    }
    out
}

/// `min(40000 / (scanRes × asinh(sig)²), 1800)` seconds.
pub fn lock_time(scan_resolution: f64, signature: f64) -> f64 {
    if scan_resolution <= 0.0 || signature <= 0.0 {
        return 1800.0;
    }
    (40_000.0 / (scan_resolution * signature.asinh().powi(2))).min(1800.0)
}

fn has_effect(calc: &Calc, item: ItemIdx, name: &str) -> bool {
    calc.effect_names(item).any(|n| n == name)
}

fn fitting(calc: &Calc, fit: &Fit) -> Fitting {
    let mut f = Fitting {
        cpu_total: calc.ship(attr::CPU_OUTPUT),
        power_total: calc.ship(attr::POWER_OUTPUT),
        calibration_total: calc.ship(attr::UPGRADE_CAPACITY),
        turrets_total: calc.ship(attr::TURRET_SLOTS_LEFT).round().max(0.0) as u32,
        launchers_total: calc.ship(attr::LAUNCHER_SLOTS_LEFT).round().max(0.0) as u32,
        ..Fitting::default()
    };
    let totals = [attr::HI_SLOTS, attr::MED_SLOTS, attr::LOW_SLOTS, attr::RIG_SLOTS, attr::MAX_SUBSYSTEMS, attr::SERVICE_SLOTS];
    for (i, a) in totals.iter().enumerate() {
        f.slots[i].1 = calc.ship(*a).round().max(0.0) as u32;
    }
    for m in &fit.modules {
        let i = Slot::ALL.iter().position(|s| *s == m.slot).unwrap_or(0);
        f.slots[i].0 += 1;
    }
    for (i, it) in calc.modules() {
        if it.state == State::Offline {
            continue;
        }
        f.cpu_used += calc.value(i, attr::CPU);
        f.power_used += calc.value(i, attr::POWER);
        if matches!(it.kind, ItemKind::Module { slot: Slot::Rig, .. }) {
            f.calibration_used += calc.value(i, attr::UPGRADE_COST);
        }
        if has_effect(calc, i, "turretFitted") {
            f.turrets_used += 1;
        }
        if has_effect(calc, i, "launcherFitted") {
            f.launchers_used += 1;
        }
    }
    f
}

fn layer(calc: &Calc, hp: crate::ids::AttrId, resonances: [crate::ids::AttrId; 4], profile: &DamageProfile) -> Layer {
    let hp = calc.ship(hp);
    let resonance = resonances.map(|a| calc.ship(a));
    let w = profile.weights();
    let through: f64 = (0..4).map(|i| w[i] * resonance[i]).sum();
    Layer { hp, resonance, ehp: if through > 0.0 { hp / through } else { hp } }
}

/// Shield peak: like the capacitor, `2.5 × capacity / τ`.
fn tank(calc: &Calc, fit: &Fit) -> Tank {
    let p = &fit.damage_profile;
    let shield = layer(calc, attr::SHIELD_CAPACITY, [attr::SHIELD_EM_RESONANCE, attr::SHIELD_THERMAL_RESONANCE, attr::SHIELD_KINETIC_RESONANCE, attr::SHIELD_EXPLOSIVE_RESONANCE], p);
    let armor = layer(calc, attr::ARMOR_HP, [attr::ARMOR_EM_RESONANCE, attr::ARMOR_THERMAL_RESONANCE, attr::ARMOR_KINETIC_RESONANCE, attr::ARMOR_EXPLOSIVE_RESONANCE], p);
    let hull = layer(calc, attr::HP, [attr::EM_RESONANCE, attr::THERMAL_RESONANCE, attr::KINETIC_RESONANCE, attr::EXPLOSIVE_RESONANCE], p);
    let recharge_s = calc.ship(attr::SHIELD_RECHARGE_RATE) / 1000.0;
    let shield_peak_regen = if recharge_s > 0.0 { 2.5 * shield.hp / recharge_s } else { 0.0 };
    let reps = reps(calc);
    Tank { ehp: shield.ehp + armor.ehp + hull.ehp, shield, armor, hull, shield_peak_regen, shield_recharge_s: recharge_s, reps }
}

/// Local repairs per second from the active repairers. An ancillary armour repairer with Nanite
/// Repair Paste loaded repairs `chargedArmorDamageMultiplier` times as much for its load, then
/// reloads; an ancillary shield booster runs its load, then reloads.
fn reps(calc: &Calc) -> Reps {
    const PASTE_GROUP: GroupId = 916;
    let mut r = Reps::default();
    for (i, it) in calc.modules() {
        if it.state < State::Active {
            continue;
        }
        let cycle = calc.value(i, attr::DURATION) / 1000.0;
        if cycle <= 0.0 {
            continue;
        }
        let charged = it.charge.map(|c| calc.items()[c].group);
        if has_effect(calc, i, "armorRepair") {
            let per_s = calc.value(i, attr::ARMOR_DAMAGE_AMOUNT) / cycle;
            r.armor += per_s;
            r.armor_burst += per_s;
        } else if has_effect(calc, i, "fueledArmorRepair") {
            let base = calc.value(i, attr::ARMOR_DAMAGE_AMOUNT);
            let (burst, sustained) = if charged == Some(PASTE_GROUP) {
                let mult = calc.value(i, attr::CHARGED_ARMOR_DAMAGE_MULTIPLIER).max(1.0);
                let load = load_cycles(calc, i).unwrap_or(1).max(1) as f64;
                let reload = calc.value(i, attr::RELOAD_TIME) / 1000.0;
                let burst = base * mult / cycle;
                (burst, base * mult * load / (load * cycle + reload))
            } else {
                (base / cycle, base / cycle)
            };
            r.armor += sustained;
            r.armor_burst += burst;
        } else if has_effect(calc, i, "shieldBoosting") {
            let per_s = calc.value(i, attr::SHIELD_BONUS) / cycle;
            r.shield += per_s;
            r.shield_burst += per_s;
        } else if has_effect(calc, i, "fueledShieldBoosting") {
            let per_s = calc.value(i, attr::SHIELD_BONUS) / cycle;
            r.shield_burst += per_s;
            r.shield += match (charged, load_cycles(calc, i)) {
                (Some(_), Some(load)) if load > 0 => {
                    let reload = calc.value(i, attr::RELOAD_TIME) / 1000.0;
                    per_s * (load as f64 * cycle) / (load as f64 * cycle + reload)
                }
                _ => per_s,
            };
        } else if has_effect(calc, i, "structureRepair") {
            r.hull += calc.value(i, attr::STRUCTURE_DAMAGE_AMOUNT) / cycle;
        }
    }
    r
}

/// Remote effects of the active modules by effect name: repairs and capacitor per second, ewar
/// as its strength.
fn remote(calc: &Calc) -> Remote {
    let mut r = Remote::default();
    for (i, it) in calc.modules() {
        if it.state < State::Active {
            continue;
        }
        let cycle = calc.value(i, attr::DURATION) / 1000.0;
        let names: Vec<&str> = calc.effect_names(i).collect();
        let has = |list: &[&str]| names.iter().any(|n| list.contains(n));
        let per_s = |a: crate::ids::AttrId| if cycle > 0.0 { calc.value(i, a) / cycle } else { 0.0 };
        let strongest = |attrs: &[crate::ids::AttrId]| attrs.iter().map(|&a| calc.value(i, a).abs()).fold(0.0, f64::max);
        let (kind, amount) = if has(&["shipModuleRemoteArmorRepairer", "shipModuleAncillaryRemoteArmorRepairer", "ShipModuleRemoteArmorMutadaptiveRepairer"]) {
            (RemoteKind::ArmorRep, per_s(attr::ARMOR_DAMAGE_AMOUNT))
        } else if has(&["shipModuleRemoteShieldBooster", "shipModuleAncillaryRemoteShieldBooster"]) {
            (RemoteKind::ShieldRep, per_s(attr::SHIELD_BONUS))
        } else if has(&["shipModuleRemoteHullRepairer"]) {
            (RemoteKind::HullRep, per_s(attr::STRUCTURE_DAMAGE_AMOUNT))
        } else if has(&["shipModuleRemoteCapacitorTransmitter"]) {
            (RemoteKind::CapTransfer, per_s(attr::POWER_TRANSFER_AMOUNT))
        } else if has(&["energyNeutralizerFalloff"]) {
            (RemoteKind::Neut, per_s(attr::ENERGY_NEUTRALIZER_AMOUNT))
        } else if has(&["energyNosferatuFalloff"]) {
            (RemoteKind::Nos, per_s(attr::POWER_TRANSFER_AMOUNT))
        } else if has(&["remoteWebifierFalloff"]) {
            (RemoteKind::Web, strongest(&[attr::SPEED_FACTOR]))
        } else if has(&["remoteTargetPaintFalloff"]) {
            (RemoteKind::Painter, strongest(&[attr::SIGNATURE_RADIUS_BONUS]))
        } else if has(&["remoteSensorDampFalloff"]) {
            (RemoteKind::Damp, strongest(&[attr::MAX_TARGET_RANGE_BONUS, attr::SCAN_RESOLUTION_BONUS]))
        } else if has(&["shipModuleTrackingDisruptor"]) {
            (RemoteKind::TrackingDisruptor, strongest(&[attr::TRACKING_SPEED_BONUS, attr::MAX_RANGE_BONUS, attr::FALLOFF_BONUS]))
        } else if has(&["shipModuleGuidanceDisruptor"]) {
            (RemoteKind::GuidanceDisruptor, strongest(&[attr::AOE_CLOUD_SIZE_BONUS, attr::AOE_VELOCITY_BONUS]))
        } else if has(&["remoteECMFalloff"]) {
            (RemoteKind::Ecm, strongest(&[attr::SCAN_GRAVIMETRIC_STRENGTH_BONUS, attr::SCAN_LADAR_STRENGTH_BONUS, attr::SCAN_MAGNETOMETRIC_STRENGTH_BONUS, attr::SCAN_RADAR_STRENGTH_BONUS]))
        } else if names.iter().any(|n| n.starts_with("warpScramble")) {
            (RemoteKind::Scram, calc.value(i, attr::WARP_SCRAMBLE_STRENGTH))
        } else if has(&["warpDisrupt"]) {
            (RemoteKind::Point, calc.value(i, attr::WARP_SCRAMBLE_STRENGTH))
        } else {
            continue;
        };
        let mut optimal = calc.value(i, attr::MAX_RANGE);
        if matches!(kind, RemoteKind::Scram | RemoteKind::Point) {
            optimal = calc.value(i, attr::WARP_SCRAMBLE_RANGE);
        }
        let mut falloff = calc.value(i, attr::FALLOFF_EFFECTIVENESS);
        if falloff <= 0.0 {
            falloff = calc.value(i, attr::FALLOFF);
        }
        match kind {
            RemoteKind::ArmorRep => r.armor_rep += amount,
            RemoteKind::ShieldRep => r.shield_rep += amount,
            RemoteKind::HullRep => r.hull_rep += amount,
            RemoteKind::CapTransfer => r.cap_transfer += amount,
            RemoteKind::Neut => r.neut += amount,
            _ => {}
        }
        if kind.is_rep() {
            r.rep_range = r.rep_range.max(optimal + falloff);
        }
        r.modules.push(RemoteModule { type_id: it.type_id, kind, amount, optimal, falloff });
    }
    r
}

/// Cycles one load of charges lasts: `floor(floor(capacity / volume) / chargeRate)`; for crystals
/// that wear, `floor(hp / (volatilityDamage × volatilityChance))` per crystal carried.
fn load_cycles(calc: &Calc, module: ItemIdx) -> Option<u32> {
    let charge = calc.items()[module].charge?;
    let capacity = calc.value(module, attr::CAPACITY);
    let volume = calc.value(charge, attr::VOLUME);
    if capacity <= 0.0 || volume <= 0.0 {
        return None;
    }
    let carried = (capacity / volume + 1e-6).floor();
    if calc.value(charge, attr::CRYSTALS_GET_DAMAGED) > 0.0 {
        let wear = calc.value(charge, attr::CRYSTAL_VOLATILITY_DAMAGE) * calc.value(charge, attr::CRYSTAL_VOLATILITY_CHANCE);
        if wear <= 0.0 {
            return None;
        }
        return Some((carried * (calc.value(charge, attr::HP) / wear).floor()).max(0.0) as u32);
    }
    if calc.has(charge, attr::CRYSTALS_GET_DAMAGED) || calc.items()[charge].group == 907 {
        return None;
    }
    let rate = calc.value(module, attr::CHARGE_RATE).max(1.0);
    Some((carried / rate).floor().max(0.0) as u32)
}

/// A fighter squadron's primary attack and its heavy missile volley, each a weapon of the squad:
/// the ability's four damages times its multiplier times the squad, applied like missiles within
/// the ability's optimal and falloff. The attributes are found by name: `fighterAbility…`.
fn fighter_weapons(calc: &Calc, i: ItemIdx, count: u32) -> Vec<Weapon> {
    let it = &calc.items()[i];
    if it.state < State::Active {
        return Vec::new();
    }
    let names: Vec<&str> = calc.effect_names(i).collect();
    let v = |name: &str| calc.dogma.attr_named(name).map_or(0.0, |a| calc.value(i, a.id));
    let mut out = Vec::new();
    for (effect, prefix, optimal, falloff, drf) in [
        ("fighterAbilityAttackM", "fighterAbilityAttackMissile", "fighterAbilityAttackMissileRangeOptimal", Some("fighterAbilityAttackMissileRangeFalloff"), "fighterAbilityAttackMissileReductionFactor"),
        ("fighterAbilityMissiles", "fighterAbilityMissiles", "fighterAbilityMissilesRange", None, "fighterAbilityMissilesDamageReductionFactor"),
    ] {
        if !names.contains(&effect) {
            continue;
        }
        let mult = v(&format!("{prefix}DamageMultiplier")) * f64::from(count);
        let dmg = Damage { em: v(&format!("{prefix}DamageEM")), thermal: v(&format!("{prefix}DamageTherm")), kinetic: v(&format!("{prefix}DamageKin")), explosive: v(&format!("{prefix}DamageExp")) }.scaled(mult);
        let cycle = v(&format!("{prefix}Duration")) / 1000.0;
        let (optimal, falloff) = (v(optimal), falloff.map_or(0.0, v));
        out.push(Weapon {
            kind: WeaponKind::Fighter,
            volley: dmg,
            cycle_s: cycle,
            reload_s: 0.0,
            shots: None,
            cap_per_cycle: 0.0,
            optimal,
            falloff,
            tracking: 0.0,
            signature_resolution: 0.0,
            missile: Some(MissileProfile {
                velocity: 0.0,
                flight_time_s: 0.0,
                range: optimal + falloff,
                explosion_radius: v(&format!("{prefix}ExplosionRadius")),
                explosion_velocity: v(&format!("{prefix}ExplosionVelocity")),
                damage_reduction_factor: v(drf),
            }),
            spool_max: 0.0,
        });
    }
    out
}

fn charge_damage(calc: &Calc, item: ItemIdx) -> Damage {
    Damage {
        em: calc.value(item, attr::EM_DAMAGE),
        thermal: calc.value(item, attr::THERMAL_DAMAGE),
        kinetic: calc.value(item, attr::KINETIC_DAMAGE),
        explosive: calc.value(item, attr::EXPLOSIVE_DAMAGE),
    }
}

const TURRET_EFFECTS: [&str; 3] = ["projectileFired", "targetAttack", "ChainLightning"];
const DISINTEGRATOR_EFFECT: &str = "targetDisintegratorAttack";
const MISSILE_EFFECTS: [&str; 3] = ["missileLaunching", "fofMissileLaunching", "dotMissileLaunching"];

/// One weapon of a group with its damage and timing, before counting.
struct Weapon {
    kind: WeaponKind,
    volley: Damage,
    cycle_s: f64,
    reload_s: f64,
    shots: Option<u32>,
    cap_per_cycle: f64,
    optimal: f64,
    falloff: f64,
    tracking: f64,
    signature_resolution: f64,
    missile: Option<MissileProfile>,
    spool_max: f64,
}

fn weapon(calc: &Calc, i: ItemIdx, fit: &Fit) -> Option<Weapon> {
    let it = &calc.items()[i];
    if it.state < State::Active {
        return None;
    }
    let names: Vec<&str> = calc.effect_names(i).collect();
    let charge = it.charge;
    let cycle = |a| calc.value(i, a) / 1000.0;
    match it.kind {
        ItemKind::Drone { count } if names.contains(&"targetAttack") => {
            let dmg = charge_damage(calc, i).scaled(calc.value(i, attr::DAMAGE_MULTIPLIER) * f64::from(count));
            Some(Weapon {
                kind: WeaponKind::Drone,
                volley: dmg,
                cycle_s: cycle(attr::SPEED),
                reload_s: 0.0,
                shots: None,
                cap_per_cycle: 0.0,
                optimal: calc.value(i, attr::MAX_RANGE),
                falloff: calc.value(i, attr::FALLOFF),
                tracking: calc.value(i, attr::TRACKING_SPEED),
                signature_resolution: calc.value(i, attr::OPTIMAL_SIG_RADIUS),
                missile: None,
                spool_max: 0.0,
            })
        }
        ItemKind::Module { .. } if names.iter().any(|n| TURRET_EFFECTS.contains(n) || *n == DISINTEGRATOR_EFFECT) => {
            let c = charge?;
            let mut mult = calc.value(i, attr::DAMAGE_MULTIPLIER);
            let spool_max = if names.contains(&DISINTEGRATOR_EFFECT) { calc.value(i, attr::DAMAGE_MULTIPLIER_BONUS_MAX) } else { 0.0 };
            if spool_max > 0.0 {
                let fraction = fit.modules.iter().find(|m| matches!(it.kind, ItemKind::Module { slot, index } if m.slot == slot && m.index == index)).and_then(|m| m.spool).unwrap_or(1.0);
                mult *= 1.0 + spool_max * f64::from(fraction.clamp(0.0, 1.0));
            }
            Some(Weapon {
                kind: WeaponKind::Turret,
                volley: charge_damage(calc, c).scaled(mult),
                cycle_s: cycle(attr::SPEED) + cycle(attr::MODULE_REACTIVATION_DELAY),
                reload_s: cycle(attr::RELOAD_TIME),
                shots: load_cycles(calc, i),
                cap_per_cycle: calc.value(i, attr::CAPACITOR_NEED),
                optimal: calc.value(i, attr::MAX_RANGE),
                falloff: calc.value(i, attr::FALLOFF),
                tracking: calc.value(i, attr::TRACKING_SPEED),
                signature_resolution: calc.value(i, attr::OPTIMAL_SIG_RADIUS),
                missile: None,
                spool_max,
            })
        }
        ItemKind::Module { .. } if names.contains(&"useMissiles") => {
            let c = charge?;
            if !calc.effect_names(c).any(|n| MISSILE_EFFECTS.contains(&n)) {
                return None;
            }
            let mult = calc.char(attr::MISSILE_DAMAGE_MULTIPLIER).max(0.0);
            let velocity = calc.value(c, attr::MAX_VELOCITY);
            let flight = calc.value(c, attr::EXPLOSION_DELAY) / 1000.0;
            Some(Weapon {
                kind: WeaponKind::Missile,
                volley: charge_damage(calc, c).scaled(mult),
                cycle_s: cycle(attr::SPEED) + cycle(attr::MODULE_REACTIVATION_DELAY),
                reload_s: cycle(attr::RELOAD_TIME),
                shots: load_cycles(calc, i),
                cap_per_cycle: calc.value(i, attr::CAPACITOR_NEED),
                optimal: velocity * flight,
                falloff: 0.0,
                tracking: 0.0,
                signature_resolution: 0.0,
                missile: Some(MissileProfile {
                    velocity,
                    flight_time_s: flight,
                    range: velocity * flight,
                    explosion_radius: calc.value(c, attr::AOE_CLOUD_SIZE),
                    explosion_velocity: calc.value(c, attr::AOE_VELOCITY),
                    damage_reduction_factor: calc.value(c, attr::AOE_DAMAGE_REDUCTION_FACTOR),
                }),
                spool_max: 0.0,
            })
        }
        ItemKind::Module { .. } if names.contains(&"empWave") => Some(Weapon {
            kind: WeaponKind::Smartbomb,
            volley: charge_damage(calc, i),
            cycle_s: cycle(attr::DURATION),
            reload_s: 0.0,
            shots: None,
            cap_per_cycle: calc.value(i, attr::CAPACITOR_NEED),
            optimal: calc.value(i, attr::EMP_FIELD_RANGE),
            falloff: 0.0,
            tracking: 0.0,
            signature_resolution: 0.0,
            missile: None,
            spool_max: 0.0,
        }),
        _ => None,
    }
}

fn weapons(calc: &Calc, fit: &Fit) -> Vec<WeaponGroup> {
    let mut groups: Vec<WeaponGroup> = Vec::new();
    for (i, it) in calc.items().iter().enumerate() {
        let ws = match it.kind {
            ItemKind::Fighter { count } => fighter_weapons(calc, i, count),
            _ => weapon(calc, i, fit).into_iter().collect(),
        };
        for w in ws {
        let charge = it.charge.map(|c| calc.items()[c].type_id);
        let count = match it.kind {
            ItemKind::Drone { count } | ItemKind::Fighter { count } => count,
            _ => 1,
        };
        let key = (w.kind, it.type_id, charge);
        if let Some(g) = groups.iter_mut().find(|g| (g.kind, g.type_id, g.charge) == key && g.cycle_s == w.cycle_s) {
            g.count += count;
            g.items.push(i);
            g.volley.add(w.volley);
            g.cap_per_s += if w.cycle_s > 0.0 { w.cap_per_cycle / w.cycle_s } else { 0.0 };
            continue;
        }
        groups.push(WeaponGroup {
            kind: w.kind,
            type_id: it.type_id,
            charge,
            count,
            items: vec![i],
            volley: w.volley,
            cycle_s: w.cycle_s,
            reload_s: w.reload_s,
            shots: w.shots,
            dps: 0.0,
            dps_burst: 0.0,
            optimal: w.optimal,
            falloff: w.falloff,
            tracking: w.tracking,
            signature_resolution: w.signature_resolution,
            missile: w.missile,
            cap_per_s: if w.cycle_s > 0.0 { w.cap_per_cycle / w.cycle_s } else { 0.0 },
            spool_max: w.spool_max,
        });
        }
    }
    for g in &mut groups {
        let volley = g.volley.total();
        g.dps_burst = if g.cycle_s > 0.0 { volley / g.cycle_s } else { 0.0 };
        g.dps = match g.shots {
            Some(n) if n > 0 && g.cycle_s > 0.0 => volley * f64::from(n) / (f64::from(n) * g.cycle_s + g.reload_s),
            Some(_) => 0.0,
            None => g.dps_burst,
        };
    }
    groups.sort_by(|a, b| b.dps.partial_cmp(&a.dps).unwrap_or(std::cmp::Ordering::Equal));
    groups
}

fn drones(calc: &Calc) -> Drones {
    let mut d = Drones {
        control_range: calc.char(attr::DRONE_CONTROL_DISTANCE),
        bandwidth_total: calc.ship(attr::DRONE_BANDWIDTH),
        bay_total: calc.ship(attr::DRONE_CAPACITY),
        max_active: calc.char(attr::MAX_ACTIVE_DRONES).round().max(0.0) as u32,
        ..Drones::default()
    };
    for (i, it) in calc.drones() {
        let ItemKind::Drone { count } = it.kind else { continue };
        d.bay_used += calc.value(i, attr::VOLUME) * f64::from(count);
        if it.state >= State::Active {
            d.active += count;
            d.bandwidth_used += calc.value(i, attr::DRONE_BANDWIDTH_USED) * f64::from(count);
        }
    }
    d
}

/// Every active effect with a discharge attribute draws that much per cycle of its duration
/// attribute; capacitor boosters give their charge's `capacitorBonus` per cycle.
pub fn cap_users(calc: &Calc) -> Vec<cap_sim::User> {
    cap_users_by_item(calc).into_iter().map(|(_, u)| u).collect()
}

/// The same, each user with the module it belongs to.
pub fn cap_users_by_item(calc: &Calc) -> Vec<(ItemIdx, cap_sim::User)> {
    let mut users = Vec::new();
    for (i, it) in calc.modules() {
        if it.state < State::Active {
            continue;
        }
        let charged = it.charge.is_some();
        for e in it.effects.iter().filter_map(|e| calc.dogma.effect(*e)) {
            if !matches!(e.category, EffectCategory::Active | EffectCategory::Target) {
                continue;
            }
            let cycle_ms = e.duration_attr.map_or(calc.value(i, attr::DURATION), |a| calc.value(i, a));
            if cycle_ms <= 0.0 {
                continue;
            }
            let mut need = e.discharge_attr.map_or(0.0, |a| calc.value(i, a));
            if e.name == "fueledShieldBoosting" && charged {
                need = 0.0;
            }
            let mut give = 0.0;
            if e.name == "powerBooster" {
                if let Some(c) = it.charge {
                    give = calc.value(c, attr::CAPACITOR_BONUS);
                }
            }
            if need == 0.0 && give == 0.0 {
                continue;
            }
            users.push((i, cap_sim::User {
                cycle_ms: cycle_ms + calc.value(i, attr::MODULE_REACTIVATION_DELAY),
                need,
                give,
                shots: load_cycles(calc, i),
                reload_ms: calc.value(i, attr::RELOAD_TIME),
            }));
        }
    }
    users
}

/// One fitted module's own figures, for the columns beside its name.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleStat {
    pub slot: Slot,
    pub index: u8,
    pub cpu: f64,
    pub power: f64,
    /// Capacitor drawn per second while running, after boosters' injections.
    pub cap_per_s: f64,
    pub dps: f64,
    /// Seconds per cycle, or per shot for a weapon; none for what does not cycle.
    pub cycle_s: Option<f64>,
    /// Seconds of overloading before heat burns it out, at worst: damaged every cycle.
    pub burnout_s: Option<f64>,
    /// What one cycle does, the figure the module is fitted for.
    pub info: Option<ModuleInfo>,
}

/// What a module does in one cycle, in its own terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModuleInfo {
    pub kind: InfoKind,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoKind {
    /// Hit points repaired, here or on another ship.
    ShieldBoost,
    ArmorRepair,
    HullRepair,
    RemoteShield,
    RemoteArmor,
    RemoteHull,
    /// Capacitor, in GJ: injected, drained, sent or drawn.
    CapInjected,
    Neut,
    Nos,
    CapTransfer,
    /// Percent changes.
    Speed,
    Web,
    Paint,
    Damp,
    Disrupt,
    /// Points of jam strength or warp disruption.
    Ecm,
    Tackle,
}

/// The one figure a module is fitted for, whatever its state.
pub fn module_info(calc: &Calc, i: ItemIdx) -> Option<ModuleInfo> {
    let names: Vec<&str> = calc.effect_names(i).collect();
    let has = |list: &[&str]| names.iter().any(|n| list.contains(n));
    let v = |a: AttrId| calc.value(i, a);
    let strongest = |attrs: &[AttrId]| attrs.iter().map(|&a| v(a)).fold(0.0, |m: f64, x| if x.abs() > m.abs() { x } else { m });
    let charged = calc.items()[i].charge;
    let (kind, value) = if has(&["shieldBoosting", "fueledShieldBoosting"]) {
        (InfoKind::ShieldBoost, v(attr::SHIELD_BONUS))
    } else if has(&["fueledArmorRepair"]) {
        let paste = charged.is_some_and(|c| calc.items()[c].group == 916);
        (InfoKind::ArmorRepair, v(attr::ARMOR_DAMAGE_AMOUNT) * if paste { v(attr::CHARGED_ARMOR_DAMAGE_MULTIPLIER).max(1.0) } else { 1.0 })
    } else if has(&["armorRepair"]) {
        (InfoKind::ArmorRepair, v(attr::ARMOR_DAMAGE_AMOUNT))
    } else if has(&["structureRepair"]) {
        (InfoKind::HullRepair, v(attr::STRUCTURE_DAMAGE_AMOUNT))
    } else if has(&["shipModuleRemoteArmorRepairer", "shipModuleAncillaryRemoteArmorRepairer", "ShipModuleRemoteArmorMutadaptiveRepairer"]) {
        (InfoKind::RemoteArmor, v(attr::ARMOR_DAMAGE_AMOUNT))
    } else if has(&["shipModuleRemoteShieldBooster", "shipModuleAncillaryRemoteShieldBooster"]) {
        (InfoKind::RemoteShield, v(attr::SHIELD_BONUS))
    } else if has(&["shipModuleRemoteHullRepairer"]) {
        (InfoKind::RemoteHull, v(attr::STRUCTURE_DAMAGE_AMOUNT))
    } else if has(&["shipModuleRemoteCapacitorTransmitter"]) {
        (InfoKind::CapTransfer, v(attr::POWER_TRANSFER_AMOUNT))
    } else if has(&["energyNeutralizerFalloff"]) {
        (InfoKind::Neut, v(attr::ENERGY_NEUTRALIZER_AMOUNT))
    } else if has(&["energyNosferatuFalloff"]) {
        (InfoKind::Nos, v(attr::POWER_TRANSFER_AMOUNT))
    } else if has(&["powerBooster"]) {
        (InfoKind::CapInjected, charged.map_or(0.0, |c| calc.value(c, attr::CAPACITOR_BONUS)))
    } else if has(&["moduleBonusAfterburner", "moduleBonusMicrowarpdrive"]) {
        (InfoKind::Speed, v(attr::SPEED_FACTOR))
    } else if has(&["remoteWebifierFalloff"]) {
        (InfoKind::Web, v(attr::SPEED_FACTOR))
    } else if has(&["remoteTargetPaintFalloff"]) {
        (InfoKind::Paint, v(attr::SIGNATURE_RADIUS_BONUS))
    } else if has(&["remoteSensorDampFalloff"]) {
        (InfoKind::Damp, strongest(&[attr::MAX_TARGET_RANGE_BONUS, attr::SCAN_RESOLUTION_BONUS]))
    } else if has(&["shipModuleTrackingDisruptor"]) {
        (InfoKind::Disrupt, strongest(&[attr::TRACKING_SPEED_BONUS, attr::MAX_RANGE_BONUS, attr::FALLOFF_BONUS]))
    } else if has(&["shipModuleGuidanceDisruptor"]) {
        (InfoKind::Disrupt, strongest(&[attr::AOE_CLOUD_SIZE_BONUS, attr::AOE_VELOCITY_BONUS]))
    } else if has(&["remoteECMFalloff"]) {
        (InfoKind::Ecm, strongest(&[attr::SCAN_GRAVIMETRIC_STRENGTH_BONUS, attr::SCAN_LADAR_STRENGTH_BONUS, attr::SCAN_MAGNETOMETRIC_STRENGTH_BONUS, attr::SCAN_RADAR_STRENGTH_BONUS]))
    } else if names.iter().any(|n| n.starts_with("warpScramble")) || has(&["warpDisrupt"]) {
        (InfoKind::Tackle, v(attr::WARP_SCRAMBLE_STRENGTH))
    } else {
        return None;
    };
    (value != 0.0).then_some(ModuleInfo { kind, value })
}

/// Seconds of overload before it burns out, if every overloaded cycle damages it: its hit points
/// over its heat damage per cycle, at the overloaded cycle time. A cooler rack damages it less
/// often, so this is how long it lasts at the worst.
fn burnout_of(calc: &Calc, i: ItemIdx) -> Option<f64> {
    let dogma = calc.dogma;
    let id = |n: &str| dogma.attr_named(n).map(|a| a.id);
    let heat = id("heatDamage").filter(|a| calc.has(i, *a)).map(|a| calc.value(i, a)).filter(|h| *h > 0.0)?;
    let hp = id("hp").map_or(0.0, |a| calc.value(i, a));
    if hp <= 0.0 {
        return None;
    }
    let mut cycle = cycle_of(calc, i)?;
    // Not overloaded now: the cycle it would run at overloaded.
    if calc.items()[i].state != State::Overload {
        for bonus in ["overloadSelfDurationBonus", "overloadRofBonus"] {
            if let Some(a) = id(bonus).filter(|a| calc.has(i, *a)) {
                cycle *= 1.0 + calc.value(i, a) / 100.0;
            }
        }
    }
    Some((hp / heat).ceil() * cycle)
}

/// Seconds per cycle: the module's duration, or for a weapon its rate of fire.
fn cycle_of(calc: &Calc, i: ItemIdx) -> Option<f64> {
    [attr::DURATION, attr::SPEED].into_iter().filter(|a| calc.has(i, *a)).map(|a| calc.value(i, a) / 1000.0).find(|s| *s > 0.0)
}

/// Every fitted module's CPU, powergrid, capacitor draw and damage, in the fit's order.
pub fn module_stats(calc: &Calc, c: &Computed) -> Vec<ModuleStat> {
    let cap = cap_users_by_item(calc);
    let mut out = Vec::new();
    for (i, it) in calc.modules() {
        let ItemKind::Module { slot, index } = it.kind else { continue };
        let cap_per_s: f64 = cap.iter().filter(|(j, _)| *j == i).map(|(_, u)| u.average_per_s()).sum();
        let dps: f64 = c.weapons.iter().filter(|w| w.items.contains(&i)).map(|w| w.dps / w.items.len().max(1) as f64).sum();
        let (cpu, power) = if it.state >= State::Online { (calc.value(i, attr::CPU), calc.value(i, attr::POWER)) } else { (0.0, 0.0) };
        out.push(ModuleStat { slot, index, cpu, power, cap_per_s, dps, cycle_s: cycle_of(calc, i), burnout_s: burnout_of(calc, i), info: module_info(calc, i) });
    }
    out
}

fn capacitor(calc: &Calc, _weapons: &[WeaponGroup]) -> Capacitor {
    let capacity = calc.ship(attr::CAPACITOR_CAPACITY);
    let recharge_s = calc.ship(attr::RECHARGE_RATE) / 1000.0;
    let peak_regen = if recharge_s > 0.0 { 2.5 * capacity / recharge_s } else { 0.0 };
    let users = cap_users(calc);
    let usage: f64 = users.iter().map(|u| u.average_per_s()).sum();
    let estimate_stable_at = if capacity > 0.0 && recharge_s > 0.0 { cap_sim::stable_estimate(capacity, recharge_s, usage) } else { None };
    let sim = cap_sim::simulate(capacity, recharge_s, &users, cap_sim::DEFAULT_HORIZON_S);
    Capacitor { capacity, recharge_s, peak_regen, usage, stable_at: sim.stable_at, lasts_s: sim.lasts_s, estimate_stable_at }
}

/// Align: `ln 2 × agility × mass / 500 000` seconds.
fn mobility(calc: &Calc) -> Mobility {
    let mass = calc.ship(attr::MASS);
    let agility = calc.ship(attr::AGILITY);
    Mobility {
        max_velocity: calc.ship(attr::MAX_VELOCITY),
        mass,
        agility,
        align_s: std::f64::consts::LN_2 * agility * mass / 500_000.0,
        warp_speed_au: calc.ship(attr::WARP_SPEED_MULTIPLIER) * calc.ship(attr::BASE_WARP_SPEED),
        signature: calc.ship(attr::SIGNATURE_RADIUS),
    }
}

fn sensors(calc: &Calc) -> Sensors {
    let strengths = [
        (SensorKind::Radar, calc.ship(attr::SCAN_RADAR_STRENGTH)),
        (SensorKind::Ladar, calc.ship(attr::SCAN_LADAR_STRENGTH)),
        (SensorKind::Magnetometric, calc.ship(attr::SCAN_MAGNETOMETRIC_STRENGTH)),
        (SensorKind::Gravimetric, calc.ship(attr::SCAN_GRAVIMETRIC_STRENGTH)),
    ];
    let (kind, strength) = strengths.into_iter().fold((SensorKind::Radar, 0.0), |best, s| if s.1 > best.1 { s } else { best });
    let ship_targets = calc.ship(attr::MAX_LOCKED_TARGETS);
    let char_targets = calc.char(attr::MAX_LOCKED_TARGETS);
    Sensors {
        scan_resolution: calc.ship(attr::SCAN_RESOLUTION),
        lock_range: calc.ship(attr::MAX_TARGET_RANGE),
        max_targets: ship_targets.min(char_targets).round().max(0.0) as u32,
        strength,
        kind,
    }
}

#[cfg(test)]
mod tests;
