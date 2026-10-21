use super::*;
use crate::test_support::{dataset, ids};
use crate::fit::{DroneStack, Module};

const TEXT: &str = "[Rifter, Test fit]\nDamage Control II\nGyrostabilizer II /OFFLINE\n\n1MN Microwarpdrive II\n[Empty Med slot]\n\n200mm AutoCannon II, EMP S\n200mm AutoCannon II, Barrage S\n\nSmall Trimark Armor Pump II\n\n\nWarrior II x2\n\nZor's Custom Navigation Hyper-Link\nSynth Exile Booster\n\nEMP S x500\n";

#[test]
fn eft_imports_by_type_not_by_position_and_exports_the_same_text() {
    let d = dataset();
    let imp = eft::import(&d, TEXT).unwrap();
    assert!(imp.unknown.is_empty(), "{:?}", imp.unknown);
    let f = &imp.fit;
    assert_eq!((f.ship, f.name.as_str()), (ids::SHIP, "Test fit"));
    assert_eq!(f.rack(Slot::Low).map(|m| (m.type_id, m.state)).collect::<Vec<_>>(), vec![(ids::DC, State::Online), (ids::GYRO, State::Offline)]);
    assert_eq!(f.rack(Slot::Mid).map(|m| m.type_id).collect::<Vec<_>>(), vec![ids::MWD]);
    assert_eq!(f.rack(Slot::High).map(|m| (m.type_id, m.charge)).collect::<Vec<_>>(), vec![(ids::GUN, Some(ids::AMMO)), (ids::GUN, Some(ids::AMMO_LONG))]);
    assert_eq!(f.rack(Slot::Rig).map(|m| m.state).collect::<Vec<_>>(), vec![State::Online]);
    assert_eq!(f.drones, vec![DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None }]);
    assert_eq!(f.implants, vec![ids::IMPLANT]);
    assert_eq!(f.boosters.iter().map(|b| b.type_id).collect::<Vec<_>>(), vec![ids::BOOSTER]);
    assert_eq!(f.cargo, vec![CargoItem { type_id: ids::AMMO, quantity: 500 }]);
    let out = eft::export(&d, f);
    assert_eq!(out, TEXT.replace("[Empty Med slot]\n", "").replace("\n\n\nWarrior", "\n\nWarrior"));
    assert_eq!(eft::import(&d, &out).unwrap().fit, *f);
}

#[test]
fn eft_keeps_going_past_unknown_names_and_odd_input() {
    let d = dataset();
    let imp = eft::import(&d, "\n\n[Rifter, x]\nNope Module\n200mm AutoCannon II, Nope Ammo\nRifter\n").unwrap();
    assert_eq!(imp.unknown, vec!["Nope Module", "Nope Ammo", "Rifter"]);
    assert_eq!(imp.fit.modules.len(), 1);
    assert_eq!(imp.fit.modules[0].charge, None);
    assert!(eft::import(&d, "no header").is_none());
    let imp = eft::import(&d, "[Nope, y]").unwrap();
    assert_eq!((imp.fit.ship, imp.unknown.as_slice()), (0, &["Nope".to_owned()][..]));
    let imp = eft::import(&d, "[Rifter]").unwrap();
    assert_eq!(imp.fit.name, "Rifter");
}

#[test]
fn eft_export_names_come_from_the_caller_when_asked() {
    let mut f = Fit::new(1, "");
    f.add_module(2, Slot::High);
    let out = eft::export_named(&f, |id| format!("T{id}"));
    assert_eq!(out, "[T1, T1]\nT2\n");
}

#[test]
fn esi_json_round_trips_with_charges_in_cargo() {
    let d = dataset();
    let mut f = eft::import(&d, TEXT).unwrap().fit;
    f.modules.iter_mut().for_each(|m| if m.state == State::Offline { m.state = State::Online });
    let doc = esi_json::export(&f, "desc");
    assert_eq!(doc.ship_type_id, ids::SHIP);
    assert_eq!(doc.items.iter().filter(|i| i.flag == esi_json::FLAG_CARGO).map(|i| (i.type_id, i.quantity)).collect::<Vec<_>>(), vec![(ids::AMMO, 501), (ids::AMMO_LONG, 1)]);
    assert!(doc.items.iter().any(|i| i.flag == esi_json::FLAG_DRONE_BAY && i.quantity == 2));
    assert_eq!(esi_json::flag_of(Slot::High, 1), 28);
    assert_eq!(esi_json::slot_of_flag(28), Some((Slot::High, 1)));
    assert_eq!(esi_json::slot_of_flag(5), None);
    let back = esi_json::import(&d, &doc);
    assert_eq!(back.modules.iter().map(|m| (m.slot, m.index, m.type_id)).collect::<Vec<_>>(), f.modules.iter().map(|m| (m.slot, m.index, m.type_id)).collect::<Vec<_>>());
    assert_eq!(back.drones, f.drones);
    assert!(back.modules.iter().all(|m| m.charge.is_none()), "ESI knows no loaded charges");
    let json = serde_json::to_string(&doc).unwrap();
    assert_eq!(serde_json::from_str::<esi_json::EsiFitting>(&json).unwrap(), doc);
}

#[test]
fn dna_strings_carry_counts_and_read_back() {
    let d = dataset();
    let f = eft::import(&d, TEXT).unwrap().fit;
    let dna = dna::export(&f);
    assert!(dna.starts_with(&format!("{}:{};1:{};1:{};1:{};2:{};1:{};2:", ids::SHIP, ids::DC, ids::GYRO, ids::MWD, ids::GUN, ids::RIG, ids::DRONE)), "{dna}");
    assert!(dna.ends_with(&format!(":{};501:{};1::", ids::AMMO, ids::AMMO_LONG)), "{dna}: the loaded EMP joins the cargo EMP");
    let back = dna::import(&d, &dna).unwrap();
    assert_eq!(back.ship, ids::SHIP);
    assert_eq!(back.modules.len(), f.modules.len());
    assert_eq!(back.drones, f.drones);
    assert!(dna::import(&d, "999999:1;1::").is_none());
    assert!(dna::import(&d, &format!("fitting:{}:{};2::", ids::SHIP, ids::GUN)).unwrap().modules.len() == 2);
}

#[test]
fn items_are_placed_by_their_type() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "");
    assert!(add_item(&d, &mut f, ids::GUN, 2));
    assert!(add_item(&d, &mut f, ids::AMMO, 10));
    assert!(add_item(&d, &mut f, ids::DRONE, 1));
    assert!(add_item(&d, &mut f, ids::DRONE, 2));
    assert!(add_item(&d, &mut f, ids::IMPLANT, 1));
    assert!(add_item(&d, &mut f, ids::BOOSTER, 1));
    assert!(!add_item(&d, &mut f, ids::SKILL_MF, 1));
    assert!(!add_item(&d, &mut f, 424242, 1));
    assert_eq!(f.modules.len(), 2);
    assert_eq!(f.modules[0].state, State::Active, "a gun comes in active");
    assert_eq!(f.cargo, vec![CargoItem { type_id: ids::AMMO, quantity: 10 }]);
    assert_eq!(f.drones, vec![DroneStack { type_id: ids::DRONE, count: 3, active: true, launched: None }]);
    assert_eq!(place_of(&d, d.type_(ids::BOOSTER).unwrap()), Place::Booster);
    assert_eq!(place_of(&d, d.type_(ids::RIG).unwrap()), Place::Module(Slot::Rig));
    assert_eq!(default_state(&d, ids::GUN), State::Active, "it has a target effect");
    assert_eq!(default_state(&d, ids::MWD), State::Active, "it needs capacitor");
    assert_eq!(default_state(&d, ids::DC), State::Online);
    let _ = Module::new(1, Slot::Low, 0);
}
