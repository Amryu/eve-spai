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
    assert_eq!(f.drones, vec![DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None }]);
    assert_eq!(f.implants, vec![ids::IMPLANT]);
    assert_eq!(f.boosters.iter().map(|b| b.type_id).collect::<Vec<_>>(), vec![ids::BOOSTER]);
    assert_eq!(f.cargo, vec![CargoItem { type_id: ids::AMMO, quantity: 500, hold: None }]);
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
    assert_eq!(f.cargo, vec![CargoItem { type_id: ids::AMMO, quantity: 10, hold: None }]);
    assert_eq!(f.drones, vec![DroneStack { type_id: ids::DRONE, count: 3, active: true, launched: None, mutation: None }]);
    assert_eq!(place_of(&d, d.type_(ids::BOOSTER).unwrap()), Place::Booster);
    assert_eq!(place_of(&d, d.type_(ids::RIG).unwrap()), Place::Module(Slot::Rig));
    assert_eq!(default_state(&d, ids::GUN), State::Active, "it has a target effect");
    assert_eq!(default_state(&d, ids::MWD), State::Active, "it needs capacitor");
    assert_eq!(default_state(&d, ids::DC), State::Online);
    let _ = Module::new(1, Slot::Low, 0);
}

/// The racks and bays of a fit, without states, which some formats do not carry.
fn contents(f: &Fit) -> (TypeId, Vec<(Slot, TypeId)>, Vec<(TypeId, u32)>) {
    let mut m: Vec<(Slot, TypeId)> = f.modules.iter().map(|m| (m.slot, m.type_id)).collect();
    m.sort_by_key(|(s, t)| (*s as u8, *t));
    (f.ship, m, f.drones.iter().map(|d| (d.type_id, d.count)).collect())
}

#[test]
fn every_format_comes_back_through_the_one_import() {
    let d = dataset();
    let f = eft::import(&d, TEXT).unwrap().fit;
    let eft_text = eft::export(&d, &f);
    let esi = serde_json::to_string(&esi_json::export(&f, "")).unwrap();
    let xml_text = xml::export(&d, &[&f, &f]);
    let dna_text = dna::export(&f);
    for (what, text, n) in [("EFT", eft_text.clone(), 1), ("ESI", esi.clone(), 1), ("ESI list", format!("[{esi}, {esi}]"), 2), ("XML", xml_text, 2), ("DNA", dna_text.clone(), 1), ("game link", format!("<url=fitting:{dna_text}>Test</url>"), 1), ("two EFT", format!("{eft_text}\n{eft_text}"), 2)] {
        let got = import_any(&d, &text);
        assert_eq!(got.len(), n, "{what}: {} fits", got.len());
        assert_eq!(contents(&got[0].fit), contents(&f), "{what}");
    }
    let named = esi.replace("\"flag\":11", "\"flag\":\"LoSlot0\"").replace("\"flag\":27", "\"flag\":\"HiSlot0\"");
    assert_eq!(contents(&import_any(&d, &named)[0].fit), contents(&f), "named flags");
    assert!(import_any(&d, "hello").is_empty());
}

#[test]
fn xml_carries_names_quantities_and_cargo() {
    let d = dataset();
    let f = eft::import(&d, TEXT).unwrap().fit;
    let x = xml::export(&d, &[&f]);
    assert!(x.contains("<shipType value=\"Rifter\"/>") && x.contains("slot=\"hi slot 0\"") && x.contains("qty=\"2\" slot=\"drone bay\""), "{x}");
    let back = &xml::import(&d, &x)[0].fit;
    assert_eq!(back.name, "Test fit");
    let emp = back.cargo.iter().find(|c| c.type_id == ids::AMMO).map(|c| c.quantity);
    assert_eq!(emp, Some(501), "the cargo's EMP and the loaded one together");
}

#[test]
fn multibuy_counts_everything_once() {
    let d = dataset();
    let f = eft::import(&d, TEXT).unwrap().fit;
    let m = multibuy::export(&d, &f);
    assert!(m.starts_with("Rifter x1\n"), "{m}");
    assert!(m.contains("200mm AutoCannon II x2") && m.contains("Warrior II x2") && m.contains("EMP S x501"), "{m}");
}

#[test]
fn a_link_carries_a_fit_packed_or_plain() {
    let text = "[Rifter, Link]\nDamage Control II\n\n200mm AutoCannon II, EMP S\n";
    let packed = link::encode(text);
    assert!(packed.starts_with("z:") && !packed.contains(['+', '/', '=', '\n']), "{packed}");
    assert_eq!(link::decode(&packed).as_deref(), Some(text));
    assert_eq!(link::decode("%5BRifter%2C%20Link%5D%0ADamage%20Control%20II%0A").as_deref(), Some("[Rifter, Link]\nDamage Control II"));
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, text);
    assert_eq!(link::decode(&b64).as_deref(), Some(text), "plain base64 too");
    assert_eq!(link::param("#fit=z:abc&x=1"), Some("z:abc"));
    assert_eq!(link::param("?a=1&fit=hello"), Some("hello"));
}

#[test]
fn a_fit_document_comes_back_whole_with_its_variations() {
    let d = dataset();
    let mut f = eft::import(&d, TEXT).unwrap().fit;
    f.variations.push(crate::variations::Variation { id: 4, name: "Long range".into(), ..Default::default() });
    let text = link::decode(&link::encode(&crate::doc::to_json(&f))).unwrap();
    let back = import_any(&d, &text);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].fit.variations.len(), 1, "the variation travels");
    assert_eq!(contents(&back[0].fit), contents(&f));
}

#[test]
fn boosters_in_use_follow_the_cargo() {
    let mut d = dataset();
    let mut other = d.type_(ids::BOOSTER).unwrap().clone();
    other.id = 99_001;
    other.name = "Aaa Booster".into();
    d.types.push(other);
    let d = d.finish();
    let mut f = Fit::new(ids::SHIP, "b");
    f.boosters.push(crate::fit::Booster { type_id: ids::BOOSTER, side_effects: false, side_on: None });
    sync_boosters(&d, &mut f, None);
    assert_eq!(f.cargo.iter().find(|c| c.type_id == ids::BOOSTER).map(|c| c.quantity), Some(1), "a booster in use is carried");
    let before = f.clone();
    f.cargo.retain(|c| c.type_id != ids::BOOSTER);
    sync_boosters(&d, &mut f, Some(&before));
    assert!(f.boosters.is_empty(), "taken out of the cargo, it is no longer in use");
    f.cargo.push(CargoItem { type_id: ids::BOOSTER, quantity: 3, hold: None });
    f.cargo.push(CargoItem { type_id: 99_001, quantity: 1, hold: None });
    sync_boosters(&d, &mut f, None);
    assert_eq!(f.boosters.iter().map(|b| b.type_id).collect::<Vec<_>>(), vec![99_001], "one per slot, the first carried by name");
    let slot = d.type_(ids::BOOSTER).unwrap().attr(crate::dogma::attr::BOOSTERNESS).unwrap() as u8;
    assert_eq!(boosters_for_slot(&d, &f, slot), vec![99_001, ids::BOOSTER]);
    f.boosters.push(crate::fit::Booster { type_id: ids::BOOSTER, side_effects: false, side_on: None });
    sync_boosters(&d, &mut f, None);
    assert_eq!(f.boosters.iter().map(|b| b.type_id).collect::<Vec<_>>(), vec![ids::BOOSTER], "the latest picked for the slot is the one used");
}
