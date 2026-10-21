use super::*;
use crate::test_support::{dataset, ids};
use crate::calc::Env;
use crate::fit::{CargoItem, DroneStack, Module};
use crate::skills::SkillSet;
use crate::stats::compute;
use crate::variations as spai_fit_variations;

fn kinds(fit: &Fit, missing: &[MissingSkill]) -> Vec<WarningKind> {
    let d = dataset();
    let c = compute(&d, fit, &SkillSet::all_v(), &Env::default());
    check(&d, fit, &c, missing).into_iter().map(|w| w.kind).collect()
}

fn has(kinds: &[WarningKind], f: impl Fn(&WarningKind) -> bool) -> bool {
    kinds.iter().any(f)
}

#[test]
fn an_empty_hull_only_notes_its_free_slots() {
    let k = kinds(&Fit::new(ids::SHIP, "t"), &[]);
    assert!(k.iter().all(|w| matches!(w, WarningKind::EmptySlots { .. })), "{k:?}");
    assert!(has(&k, |w| matches!(w, WarningKind::EmptySlots { slot: Slot::High, free: 3 })));
}

#[test]
fn fitting_overruns_are_errors_and_come_first() {
    let mut f = Fit::new(ids::SHIP, "t");
    for i in 0..4 {
        f.modules.push(Module::new(ids::HEAVY, Slot::High, i).with_state(State::Active));
    }
    let d = dataset();
    let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
    let w = check(&d, &f, &c, &[]);
    assert_eq!(w[0].severity, Severity::Error);
    let k: Vec<_> = w.into_iter().map(|w| w.kind).collect();
    assert!(has(&k, |w| matches!(w, WarningKind::CpuOver { .. })));
    assert!(has(&k, |w| matches!(w, WarningKind::PowerOver { .. })));
    assert!(has(&k, |w| matches!(w, WarningKind::SlotsOver { slot: Slot::High, used: 4, total: 3 })));
    assert!(has(&k, |w| matches!(w, WarningKind::TurretsOver { used: 4, total: 3 })));
    assert!(has(&k, |w| matches!(w, WarningKind::NoPropMod)));
}

#[test]
fn modules_without_their_charges_are_pointed_out() {
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::AAR, Slot::Low, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::CAP_BOOSTER, Slot::Mid, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_state(State::Offline));
    f.modules.push(Module::new(ids::MWD, Slot::Mid, 1).with_state(State::Active));
    let k = kinds(&f, &[]);
    assert!(has(&k, |w| matches!(w, WarningKind::AncillaryArmorNoPaste { slot: Slot::Low, index: 0 })));
    assert!(has(&k, |w| matches!(w, WarningKind::NoBoosterCharges { slot: Slot::Mid, index: 0 })));
    assert!(has(&k, |w| matches!(w, WarningKind::NoAmmo { slot: Slot::High, index: 0 })));
    assert!(has(&k, |w| matches!(w, WarningKind::OfflineModule { slot: Slot::High, index: 0 })));
    assert!(!has(&k, |w| matches!(w, WarningKind::NoPropMod)));
    f.modules[0].charge = Some(ids::PASTE);
    f.modules[1].charge = Some(ids::CAP_CHARGE);
    f.modules[2].charge = Some(ids::AMMO);
    f.modules[2].state = State::Active;
    let k = kinds(&f, &[]);
    assert!(!has(&k, |w| matches!(w, WarningKind::AncillaryArmorNoPaste { .. } | WarningKind::NoBoosterCharges { .. } | WarningKind::NoAmmo { .. } | WarningKind::OfflineModule { .. })), "{k:?}");
}

#[test]
fn ammo_coverage_looks_at_what_is_carried_against_what_exists() {
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    let k = kinds(&f, &[]);
    assert!(has(&k, |w| matches!(w, WarningKind::AmmoDamageGap { weapon, missing } if *weapon == ids::GUN && missing == &vec![DamageType::Explosive])), "{k:?}");
    assert!(has(&k, |w| matches!(w, WarningKind::AmmoRangeGap { weapon, short: false, long: true } if *weapon == ids::GUN)), "{k:?}");
    f.cargo.push(CargoItem { type_id: ids::AMMO_LONG, quantity: 100, hold: None });
    let k = kinds(&f, &[]);
    assert!(!has(&k, |w| matches!(w, WarningKind::AmmoDamageGap { .. } | WarningKind::AmmoRangeGap { .. })), "{k:?}");
    f.modules[0].charge = Some(ids::AMMO_LONG);
    f.cargo.clear();
    let k = kinds(&f, &[]);
    assert!(has(&k, |w| matches!(w, WarningKind::AmmoRangeGap { short: true, long: false, .. })), "{k:?}");
}

#[test]
fn capacitor_drones_skills_and_unknown_types_have_their_warnings() {
    let mut f = Fit::new(ids::SHIP, "t");
    for i in 0..3 {
        f.modules.push(Module::new(ids::REPAIRER, Slot::Low, i).with_state(State::Active));
    }
    f.drones.push(DroneStack { type_id: ids::DRONE, count: 9, active: true, launched: None });
    f.modules.push(Module::new(424242, Slot::Mid, 0));
    let missing = vec![MissingSkill { skill: 1, needed: 5, have: 0, for_type: ids::GUN }];
    let k = kinds(&f, &missing);
    assert!(has(&k, |w| matches!(w, WarningKind::CapUnstable { lasts_s } if *lasts_s > 0.0)), "{k:?}");
    assert!(has(&k, |w| matches!(w, WarningKind::TooManyDronesOut { active: 9, max: 5 })));
    assert!(has(&k, |w| matches!(w, WarningKind::DroneBandwidthOver { .. })));
    assert!(has(&k, |w| matches!(w, WarningKind::MissingSkills { count: 1 })));
    assert!(has(&k, |w| matches!(w, WarningKind::UnknownTypes(ids) if ids == &vec![424242])));
}

#[test]
fn clashing_variations_are_pointed_out() {
    use spai_fit_variations::*;
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    f.variations.push(Variation { id: 1, name: "Long".into(), kind: VariationKind::Ammo, refit_key: None, ops: vec![RefitOp::SetCharge { slot: Slot::High, index: 0, charge: Some(ids::AMMO_LONG) }] });
    f.variations.push(Variation { id: 2, name: "Short".into(), kind: VariationKind::Ammo, refit_key: None, ops: vec![RefitOp::SetCharge { slot: Slot::High, index: 0, charge: Some(ids::AMMO) }] });
    f.active_variations = vec![1, 2];
    let k = kinds(&f, &[]);
    assert!(has(&k, |w| matches!(w, WarningKind::VariationConflict { first, second } if first == "Long" && second == "Short")), "{k:?}");
}
