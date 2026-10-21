//! What a pilot would want pointed out about a fit before undocking.

use crate::dogma::{attr, Dogma};
use crate::fit::{Fit, Slot, State};
use crate::ids::{GroupId, TypeId};
use crate::skills::MissingSkill;
use crate::stats::{Computed, WeaponKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageType {
    Em,
    Thermal,
    Kinetic,
    Explosive,
}

impl DamageType {
    pub const ALL: [DamageType; 4] = [DamageType::Em, DamageType::Thermal, DamageType::Kinetic, DamageType::Explosive];
}

#[derive(Clone, Debug, PartialEq)]
pub enum WarningKind {
    CpuOver { used: f64, total: f64 },
    PowerOver { used: f64, total: f64 },
    CalibrationOver { used: f64, total: f64 },
    SlotsOver { slot: Slot, used: u32, total: u32 },
    TurretsOver { used: u32, total: u32 },
    LaunchersOver { used: u32, total: u32 },
    DroneBandwidthOver { used: f64, total: f64 },
    DroneBayOver { used: f64, total: f64 },
    /// More carried than a hold takes: the cargo hold, or another by name.
    HoldOver { hold: String, used: f64, total: f64 },
    TooManyDronesOut { active: u32, max: u32 },
    EmptySlots { slot: Slot, free: u32 },
    NoPropMod,
    OfflineModule { slot: Slot, index: u8 },
    /// An ancillary armour repairer without Nanite Repair Paste.
    AncillaryArmorNoPaste { slot: Slot, index: u8 },
    /// A capacitor booster or ancillary shield booster without charges.
    NoBoosterCharges { slot: Slot, index: u8 },
    NoAmmo { slot: Slot, index: u8 },
    /// Nothing carried for a weapon deals these types, though ammo that does exists.
    AmmoDamageGap { weapon: TypeId, missing: Vec<DamageType> },
    /// The carried ammo leaves the weapon's short or long end uncovered.
    AmmoRangeGap { weapon: TypeId, short: bool, long: bool },
    CapUnstable { lasts_s: f64 },
    MissingSkills { count: usize },
    UnknownTypes(Vec<TypeId>),
    /// Two switched-on variations change the same thing; the later one wins.
    VariationConflict { first: String, second: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub severity: Severity,
    pub kind: WarningKind,
}

fn warn(severity: Severity, kind: WarningKind) -> Warning {
    Warning { severity, kind }
}

/// The charge groups a module takes, from `chargeGroup1..5`.
fn charge_groups(dogma: &Dogma, module: TypeId) -> Vec<GroupId> {
    let Some(t) = dogma.type_(module) else { return Vec::new() };
    [attr::CHARGE_GROUP_1, attr::CHARGE_GROUP_2, attr::CHARGE_GROUP_3, attr::CHARGE_GROUP_4, attr::CHARGE_GROUP_5]
        .iter()
        .filter_map(|a| t.attr(*a))
        .filter(|g| *g > 0.0)
        .map(|g| g as GroupId)
        .collect()
}

/// Charges the module can load: of its charge groups and its charge size.
fn compatible_charges(dogma: &Dogma, module: TypeId) -> Vec<TypeId> {
    let size = dogma.type_(module).and_then(|t| t.attr(attr::CHARGE_SIZE));
    charge_groups(dogma, module)
        .iter()
        .flat_map(|g| dogma.types_in_group(*g).iter().copied())
        .filter(|c| dogma.type_(*c).is_some_and(|t| t.published && (size.is_none() || t.attr(attr::CHARGE_SIZE) == size)))
        .collect()
}

fn damage_types(dogma: &Dogma, charge: TypeId) -> Vec<DamageType> {
    let Some(t) = dogma.type_(charge) else { return Vec::new() };
    [(attr::EM_DAMAGE, DamageType::Em), (attr::THERMAL_DAMAGE, DamageType::Thermal), (attr::KINETIC_DAMAGE, DamageType::Kinetic), (attr::EXPLOSIVE_DAMAGE, DamageType::Explosive)]
        .iter()
        .filter(|(a, _)| t.attr(*a).unwrap_or(0.0) > 0.0)
        .map(|(_, d)| *d)
        .collect()
}

/// A charge's reach relative to its kind: the range multiplier for turret ammo, flight distance
/// for missiles.
fn reach(dogma: &Dogma, charge: TypeId) -> Option<f64> {
    let t = dogma.type_(charge)?;
    if let Some(m) = t.attr(attr::WEAPON_RANGE_MULTIPLIER) {
        return Some(f64::from(m));
    }
    let v = t.attr(attr::MAX_VELOCITY)?;
    let f = t.attr(attr::EXPLOSION_DELAY)?;
    Some(f64::from(v) * f64::from(f))
}

pub fn check(dogma: &Dogma, fit: &Fit, computed: &Computed, missing_skills: &[MissingSkill]) -> Vec<Warning> {
    let mut out = Vec::new();
    let f = &computed.fitting;
    if f.cpu_used > f.cpu_total + 1e-6 {
        out.push(warn(Severity::Error, WarningKind::CpuOver { used: f.cpu_used, total: f.cpu_total }));
    }
    if f.power_used > f.power_total + 1e-6 {
        out.push(warn(Severity::Error, WarningKind::PowerOver { used: f.power_used, total: f.power_total }));
    }
    if f.calibration_used > f.calibration_total + 1e-6 {
        out.push(warn(Severity::Error, WarningKind::CalibrationOver { used: f.calibration_used, total: f.calibration_total }));
    }
    for (i, slot) in Slot::ALL.iter().enumerate() {
        let (used, total) = f.slots[i];
        if used > total {
            out.push(warn(Severity::Error, WarningKind::SlotsOver { slot: *slot, used, total }));
        }
    }
    if f.turrets_used > f.turrets_total {
        out.push(warn(Severity::Error, WarningKind::TurretsOver { used: f.turrets_used, total: f.turrets_total }));
    }
    if f.launchers_used > f.launchers_total {
        out.push(warn(Severity::Error, WarningKind::LaunchersOver { used: f.launchers_used, total: f.launchers_total }));
    }
    let d = &computed.drones;
    if d.bandwidth_used > d.bandwidth_total + 1e-6 {
        out.push(warn(Severity::Error, WarningKind::DroneBandwidthOver { used: d.bandwidth_used, total: d.bandwidth_total }));
    }
    if d.bay_used > d.bay_total + 1e-6 {
        out.push(warn(Severity::Error, WarningKind::DroneBayOver { used: d.bay_used, total: d.bay_total }));
    }
    for h in computed.holds.iter().filter(|h| h.over()) {
        out.push(warn(Severity::Error, WarningKind::HoldOver { hold: h.name.clone(), used: h.used, total: h.capacity }));
    }
    if d.active > d.max_active {
        out.push(warn(Severity::Error, WarningKind::TooManyDronesOut { active: d.active, max: d.max_active }));
    }
    if !computed.unknown_types.is_empty() {
        out.push(warn(Severity::Error, WarningKind::UnknownTypes(computed.unknown_types.clone())));
    }
    for (i, slot) in [Slot::High, Slot::Mid, Slot::Low, Slot::Rig].iter().enumerate() {
        let (used, total) = f.slots[i];
        if total > used {
            out.push(warn(Severity::Info, WarningKind::EmptySlots { slot: *slot, free: total - used }));
        }
    }
    let has_prop = fit.modules.iter().any(|m| dogma.type_(m.type_id).is_some_and(|t| t.group == 46));
    if !has_prop && !fit.modules.is_empty() {
        out.push(warn(Severity::Warn, WarningKind::NoPropMod));
    }
    for m in &fit.modules {
        if m.state == State::Offline {
            out.push(warn(Severity::Warn, WarningKind::OfflineModule { slot: m.slot, index: m.index }));
        }
        let Some(t) = dogma.type_(m.type_id) else { continue };
        let effects: Vec<&str> = t.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).map(|e| e.name.as_str()).collect();
        let charged = m.charge.is_some();
        if effects.contains(&"fueledArmorRepair") && !charged {
            out.push(warn(Severity::Warn, WarningKind::AncillaryArmorNoPaste { slot: m.slot, index: m.index }));
        }
        if (effects.contains(&"fueledShieldBoosting") || effects.contains(&"powerBooster")) && !charged {
            out.push(warn(Severity::Warn, WarningKind::NoBoosterCharges { slot: m.slot, index: m.index }));
        }
        let fires = effects.iter().any(|e| ["projectileFired", "targetAttack", "useMissiles", "targetDisintegratorAttack", "ChainLightning"].contains(e));
        if fires && !charged && !charge_groups(dogma, m.type_id).is_empty() {
            out.push(warn(Severity::Warn, WarningKind::NoAmmo { slot: m.slot, index: m.index }));
        }
    }
    for w in &computed.weapons {
        if !matches!(w.kind, WeaponKind::Turret | WeaponKind::Missile) {
            continue;
        }
        let possible = compatible_charges(dogma, w.type_id);
        if possible.len() < 2 {
            continue;
        }
        let mut carried: Vec<TypeId> = w.charge.into_iter().collect();
        carried.extend(fit.cargo.iter().map(|c| c.type_id).filter(|c| possible.contains(c)));
        if carried.is_empty() {
            continue;
        }
        let possible_types: Vec<DamageType> = DamageType::ALL.into_iter().filter(|d| possible.iter().any(|c| damage_types(dogma, *c).contains(d))).collect();
        let missing: Vec<DamageType> = possible_types.into_iter().filter(|d| !carried.iter().any(|c| damage_types(dogma, *c).contains(d))).collect();
        if !missing.is_empty() {
            out.push(warn(Severity::Info, WarningKind::AmmoDamageGap { weapon: w.type_id, missing }));
        }
        let reaches = |ids: &[TypeId]| -> Option<(f64, f64)> {
            let r: Vec<f64> = ids.iter().filter_map(|c| reach(dogma, *c)).collect();
            let min = r.iter().copied().fold(f64::INFINITY, f64::min);
            let max = r.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (!r.is_empty()).then_some((min, max))
        };
        if let (Some((pmin, pmax)), Some((cmin, cmax))) = (reaches(&possible), reaches(&carried)) {
            let short = cmin > pmin * 1.01;
            let long = cmax < pmax * 0.99;
            if short || long {
                out.push(warn(Severity::Info, WarningKind::AmmoRangeGap { weapon: w.type_id, short, long }));
            }
        }
    }
    if let Some(lasts) = computed.cap.lasts_s {
        out.push(warn(Severity::Warn, WarningKind::CapUnstable { lasts_s: lasts }));
    }
    if !missing_skills.is_empty() {
        out.push(warn(Severity::Warn, WarningKind::MissingSkills { count: missing_skills.len() }));
    }
    for (a, b) in crate::variations::conflicts(dogma, fit) {
        let name = |id: u32| fit.variations.iter().find(|v| v.id == id).map_or(String::new(), |v| v.name.clone());
        out.push(warn(Severity::Warn, WarningKind::VariationConflict { first: name(a), second: name(b) }));
    }
    out.sort_by_key(|w| w.severity);
    out
}
#[cfg(test)]
mod tests;
