use std::sync::Arc;

use spai_fit::fit::{Slot, State};
use spai_fit::history::EditKind;
use spai_fit::test_support::{dataset, ids};

use super::*;
use crate::picker::{apply, results, PickTarget};

fn sim() -> SimUi {
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(Arc::new(dataset())));
    s
}

#[test]
fn a_fit_is_loading_data_until_the_host_answers() {
    let mut s = SimUi::default();
    assert!(s.computed().is_none());
    s.handle(HostEvent::DogmaFailed("offline".into()));
    assert_eq!(s.dogma_error.as_deref(), Some("offline"));
    s.handle(HostEvent::Dogma(Arc::new(dataset())));
    assert!(s.dogma_error.is_none());
}

#[test]
fn edits_recompute_and_undo_restores_the_numbers() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 1000);
    let base = s.computed().unwrap().mobility.max_velocity;
    apply(&mut s, PickTarget::Module(Slot::Mid, 0), ids::MWD, 1100);
    assert_eq!(s.session.as_ref().unwrap().fit.module(Slot::Mid, 0).unwrap().state, State::Active, "a propulsion module comes in active");
    let boosted = s.computed().unwrap().mobility.max_velocity;
    assert!(boosted > base * 3.0, "{boosted} vs {base}");
    assert!(s.session.as_ref().unwrap().dirty);
    s.undo();
    assert_eq!(s.computed().unwrap().mobility.max_velocity, base);
    s.redo();
    assert_eq!(s.computed().unwrap().mobility.max_velocity, boosted);
    s.edit(EditKind::Other, 1200, |_| {});
    assert_eq!(s.session.as_ref().unwrap().history.len(), 1, "an edit that changes nothing records nothing");
}

#[test]
fn the_picker_filters_by_what_the_slot_takes() {
    let d = dataset();
    assert_eq!(results(&d, PickTarget::Ship, None, ""), vec![ids::SHIP]);
    let mids = results(&d, PickTarget::Module(Slot::Mid, 0), None, "");
    assert!(mids.contains(&ids::MWD) && mids.contains(&ids::WEB) && !mids.contains(&ids::GUN));
    assert_eq!(results(&d, PickTarget::Module(Slot::High, 0), None, "autocannon"), vec![ids::GUN]);
    assert_eq!(results(&d, PickTarget::Module(Slot::High, 0), None, "200mm auto"), vec![ids::GUN], "every word must match");
    let charges = results(&d, PickTarget::Charge(Slot::High, 0), Some(ids::GUN), "");
    assert_eq!(charges, vec![ids::AMMO_LONG, ids::AMMO], "by name: Barrage before EMP");
    assert_eq!(results(&d, PickTarget::Charge(Slot::High, 0), Some(ids::AAR), ""), vec![ids::PASTE]);
    assert_eq!(results(&d, PickTarget::Drone, None, ""), vec![ids::DRONE]);
    assert_eq!(results(&d, PickTarget::Implant, None, ""), vec![ids::IMPLANT]);
    assert_eq!(results(&d, PickTarget::Booster, None, ""), vec![ids::BOOSTER]);
    assert!(results(&d, PickTarget::Cargo, None, "").contains(&ids::CAP_CHARGE));
    assert!(results(&d, PickTarget::Ship, None, "zzz").is_empty());
}

#[test]
fn picking_places_charges_and_bay_items_and_swaps_hulls() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 2), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 2), ids::AMMO, 2);
    apply(&mut s, PickTarget::Drone, ids::DRONE, 3);
    apply(&mut s, PickTarget::Drone, ids::DRONE, 4);
    apply(&mut s, PickTarget::Implant, ids::IMPLANT, 5);
    apply(&mut s, PickTarget::Cargo, ids::AMMO_LONG, 6);
    let f = &s.session.as_ref().unwrap().fit;
    assert_eq!(f.module(Slot::High, 2).unwrap().charge, Some(ids::AMMO));
    assert_eq!(f.drones[0].count, 2);
    assert_eq!(f.implants, vec![ids::IMPLANT]);
    assert_eq!(f.cargo[0].type_id, ids::AMMO_LONG);
    assert_eq!(s.computed().unwrap().weapons.len(), 2, "the gun and the drones");
    apply(&mut s, PickTarget::Ship, ids::SHIP, 7);
    assert_eq!(s.session.as_ref().unwrap().history.len(), 6, "the same hull again is no edit");
}

#[test]
fn profiles_change_the_numbers_and_survive_a_refresh() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    let all_v = s.computed().unwrap().dps();
    s.handle(HostEvent::Profiles(vec![Profile { id: "c1".into(), name: "Newbro".into(), skills: SkillSet::none() }]));
    assert_eq!(s.profiles.len(), 2);
    s.profile = 1;
    let newbro = s.computed().unwrap().dps();
    assert!(newbro < all_v, "{newbro} vs {all_v}");
    assert!(!s.session.as_ref().unwrap().missing.is_empty());
    assert!(s.session.as_ref().unwrap().warnings.iter().any(|w| matches!(w.kind, spai_fit::warnings::WarningKind::MissingSkills { .. })));
    s.handle(HostEvent::Profiles(vec![Profile { id: "c1".into(), name: "Newbro".into(), skills: SkillSet::none() }, Profile { id: "c2".into(), name: "Vet".into(), skills: SkillSet::all_v() }]));
    assert_eq!(s.profile, 1, "the chosen profile is kept by id across a refresh");
}

#[test]
fn saving_loading_and_the_library_go_through_the_host() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id, name, ship, doc } = &reqs[0] else { panic!("{reqs:?}") };
    assert_eq!((name.as_str(), *ship), ("Rifter", ids::SHIP));
    assert!(doc.starts_with("{\"v\":1"));
    s.handle(HostEvent::Saved(id.clone()));
    assert!(!s.session.as_ref().unwrap().dirty);
    assert_eq!(s.take_requests(), vec![HostRequest::ListFits]);
    s.handle(HostEvent::Fits(vec![FitSummary { id: id.clone(), name: "Rifter".into(), ship: ids::SHIP, updated_at: 0 }]));
    assert_eq!(s.library.len(), 1);
    s.handle(HostEvent::Fit { id: "other".into(), doc: doc.replace("\"name\":\"Rifter\"", "\"name\":\"Other\"") });
    assert_eq!(s.session.as_ref().unwrap().fit.name, "Other");
    s.handle(HostEvent::Fit { id: "bad".into(), doc: "not json".into() });
    assert_eq!(s.session.as_ref().unwrap().fit.name, "Other", "a broken document leaves the fit alone");
}

#[test]
fn module_states_cycle_through_what_the_module_can_do() {
    use crate::editor::next_state;
    let d = dataset();
    assert_eq!(next_state(&d, ids::GYRO, State::Online), State::Offline, "a passive module only toggles");
    assert_eq!(next_state(&d, ids::GYRO, State::Offline), State::Online);
    assert_eq!(next_state(&d, ids::MWD, State::Online), State::Active);
    assert_eq!(next_state(&d, ids::MWD, State::Active), State::Offline, "no heat damage figure: no overload");
    assert_eq!(next_state(&d, ids::GUN, State::Active), State::Offline);
}

#[test]
fn warnings_read_as_sentences() {
    use spai_fit::warnings::{Severity, Warning, WarningKind};
    let d = dataset();
    let w = Warning { severity: Severity::Info, kind: WarningKind::AmmoDamageGap { weapon: ids::GUN, missing: vec![spai_fit::warnings::DamageType::Explosive] } };
    assert_eq!(crate::warnings_panel::describe(&d, &w), "200mm AutoCannon II: no ammo carried for explosive damage");
    let w = Warning { severity: Severity::Warn, kind: WarningKind::NoAmmo { slot: Slot::High, index: 1 } };
    assert_eq!(crate::warnings_panel::describe(&d, &w), "No ammunition loaded in high slot 2");
}

#[test]
fn recording_a_variation_keeps_the_difference_and_switches_it_on() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    let short = s.computed().unwrap().weapons[0].optimal;
    s.start_recording(3);
    assert!(s.recording.is_some());
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO_LONG, 4);
    s.keep_recording(5);
    assert!(s.recording.is_none());
    let f = &s.session.as_ref().unwrap().fit;
    assert_eq!(f.variations.len(), 1);
    assert_eq!(f.variations[0].kind, spai_fit::variations::VariationKind::Ammo);
    assert_eq!(f.active_variations, vec![1]);
    assert_eq!(f.module(Slot::High, 0).unwrap().charge, Some(ids::AMMO), "the base keeps the short ammo");
    assert_eq!(s.view_fit().unwrap().module(Slot::High, 0).unwrap().charge, Some(ids::AMMO_LONG), "the view shows the variation");
    assert!(s.computed().unwrap().weapons[0].optimal > short);
    assert_eq!(s.variation_positions().into_iter().collect::<Vec<_>>(), vec![(Slot::High, 0)]);
    s.edit(EditKind::Variation, 6, |f| f.active_variations.clear());
    assert_eq!(s.computed().unwrap().weapons[0].optimal, short);
    s.start_recording(7);
    s.discard_recording(8);
    assert_eq!(s.session.as_ref().unwrap().fit.variations.len(), 1);
    s.start_recording(9);
    s.keep_recording(10);
    assert_eq!(s.session.as_ref().unwrap().fit.variations.len(), 1, "no change, no variation");
}

#[test]
fn doctrines_open_as_copies_and_admins_publish_the_open_fit() {
    let mut s = sim();
    let mut fit = Fit::new(ids::SHIP, "Doctrine Rifter");
    fit.add_module(ids::GUN, Slot::High);
    fit.variations.push(spai_fit::variations::Variation { id: 1, name: "AB".into(), kind: spai_fit::variations::VariationKind::Refit, refit_key: Some("prop:ab".into()), ops: vec![] });
    let doc: serde_json::Value = serde_json::from_str(&spai_fit::doc::to_json(&fit)).unwrap();
    s.handle(HostEvent::Doctrines(vec![Doctrine { id: "d1".into(), side: DoctrineSide::Imperium, name: "Alpha Rifter".into(), ship: ids::SHIP, tags: vec!["alpha".into()], refit_keys: vec!["prop:ab".into()], rev: 1, doc }]));
    s.open_doctrine("d1", 5);
    let open = s.session.as_ref().unwrap();
    assert_eq!(open.fit.name, "Alpha Rifter");
    assert_ne!(open.id, "d1", "a copy with its own id");
    assert_eq!(open.fit.modules.len(), 1);
    assert!(open.dirty);
    s.handle(HostEvent::Access { signed_in: true, imperium: true, admin: true });
    s.start_publish();
    let d = s.publish.as_mut().unwrap();
    d.name = "Alpha Rifter v2".into();
    d.side = DoctrineSide::Enemy;
    d.tags = "alpha, kiting".into();
    let (id, name, side, tags, keys, doc) = {
        let (Some(d), Some(sess)) = (s.publish.take(), s.session.as_ref()) else { panic!() };
        (d.id, d.name, d.side, d.tags, sess.fit.variations.iter().filter_map(|v| v.refit_key.clone()).collect::<Vec<_>>(), spai_fit::doc::to_json(&sess.fit))
    };
    s.request(HostRequest::PublishDoctrine { id: id.clone(), side, name: name.clone(), tags: tags.split(',').map(|t| t.trim().to_owned()).collect(), refit_keys: keys.clone(), doc });
    let reqs = s.take_requests();
    assert!(matches!(&reqs[0], HostRequest::PublishDoctrine { side: DoctrineSide::Enemy, refit_keys, tags, .. } if refit_keys == &vec!["prop:ab".to_owned()] && tags == &vec!["alpha".to_owned(), "kiting".to_owned()]), "{reqs:?}");
    let json = serde_json::to_string(&s.doctrines[0]).unwrap();
    assert!(json.contains("\"side\":\"imperium\"") && json.contains("\"ship_type_id\""));
}
