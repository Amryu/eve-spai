use std::sync::Arc;

use spai_fit::fit::{DroneStack, Slot, State};
use spai_fit::history::EditKind;
use spai_fit::test_support::{close, dataset, ids};

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
    assert_eq!(s.session().unwrap().fit.module(Slot::Mid, 0).unwrap().state, State::Active, "a propulsion module comes in active");
    let boosted = s.computed().unwrap().mobility.max_velocity;
    assert!(boosted > base * 3.0, "{boosted} vs {base}");
    assert!(s.session().unwrap().dirty);
    s.undo();
    assert_eq!(s.computed().unwrap().mobility.max_velocity, base);
    s.redo();
    assert_eq!(s.computed().unwrap().mobility.max_velocity, boosted);
    s.edit(EditKind::Other, 1200, |_| {});
    assert_eq!(s.session().unwrap().history.len(), 1, "an edit that changes nothing records nothing");
}

#[test]
fn the_picker_filters_by_what_the_slot_takes() {
    let d = dataset();
    assert_eq!(results(&d, PickTarget::Ship, None, ""), vec![ids::T3C, ids::SHIP], "by name");
    let mids = results(&d, PickTarget::Module(Slot::Mid, 0), None, "");
    assert!(mids.contains(&ids::MWD) && mids.contains(&ids::WEB) && !mids.contains(&ids::GUN));
    assert_eq!(results(&d, PickTarget::Module(Slot::High, 0), None, "autocannon"), vec![ids::GUN]);
    assert_eq!(results(&d, PickTarget::Module(Slot::High, 0), None, "200mm auto"), vec![ids::GUN], "every word must match");
    let charges = results(&d, PickTarget::Charge(Slot::High, 0), Some(ids::GUN), "");
    assert_eq!(charges, vec![ids::AMMO_LONG, ids::AMMO], "by name: Barrage before EMP");
    assert_eq!(results(&d, PickTarget::Charge(Slot::High, 0), Some(ids::AAR), ""), vec![ids::PASTE]);
    assert_eq!(results(&d, PickTarget::Drone, None, ""), vec![ids::WEB_DRONE, ids::LOGI_DRONE, ids::DRONE], "by name");
    assert_eq!(results(&d, PickTarget::Implant, None, ""), vec![ids::SHIELD_MINDLINK, ids::SKIRM_MINDLINK, ids::IMPLANT], "by name");
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
    let f = &s.session().unwrap().fit;
    assert_eq!(f.module(Slot::High, 2).unwrap().charge, Some(ids::AMMO));
    assert_eq!(f.drones[0].count, 2);
    assert_eq!(f.implants, vec![ids::IMPLANT]);
    assert_eq!(f.cargo[0].type_id, ids::AMMO_LONG);
    assert_eq!(s.computed().unwrap().weapons.len(), 2, "the gun and the drones");
    apply(&mut s, PickTarget::Ship, ids::SHIP, 7);
    assert_eq!(s.session().unwrap().history.len(), 6, "the same hull again is no edit");
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
    assert!(!s.session().unwrap().missing.is_empty());
    assert!(s.session().unwrap().warnings.iter().any(|w| matches!(w.kind, spai_fit::warnings::WarningKind::MissingSkills { .. })));
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
    assert!(!s.session().unwrap().dirty);
    assert_eq!(s.take_requests(), vec![HostRequest::ListFits]);
    s.handle(HostEvent::Fits(vec![FitSummary { id: id.clone(), name: "Rifter".into(), ship: ids::SHIP, updated_at: 0 }]));
    assert_eq!(s.library.len(), 1);
    s.handle(HostEvent::Fit { id: "other".into(), doc: doc.replace("\"name\":\"Rifter\"", "\"name\":\"Other\"") });
    assert_eq!(s.session().unwrap().fit.name, "Other");
    s.handle(HostEvent::Fit { id: "bad".into(), doc: "not json".into() });
    assert_eq!(s.session().unwrap().fit.name, "Other", "a broken document leaves the fit alone");
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
    let f = &s.session().unwrap().fit;
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
    assert_eq!(s.session().unwrap().fit.variations.len(), 1);
    s.start_recording(9);
    s.keep_recording(10);
    assert_eq!(s.session().unwrap().fit.variations.len(), 1, "no change, no variation");
}

#[test]
fn with_a_variation_on_edits_land_in_it_and_leave_the_base_alone() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.start_recording(3);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO_LONG, 4);
    s.keep_recording(5);
    // Its own module: the change is the variation's.
    s.edit(EditKind::SetState, 6, |f| f.module_mut(Slot::High, 0).unwrap().state = State::Offline);
    apply(&mut s, PickTarget::Module(Slot::Low, 0), ids::DC, 7);
    let f = s.session().unwrap().fit.clone();
    assert_eq!(f.module(Slot::High, 0).map(|m| (m.charge, m.state)), Some((Some(ids::AMMO), State::Active)), "the base keeps its gun as it was");
    assert!(f.module(Slot::Low, 0).is_none(), "and gains nothing");
    let v = s.view_fit().unwrap();
    assert_eq!(v.module(Slot::High, 0).map(|m| (m.charge, m.state)), Some((Some(ids::AMMO_LONG), State::Offline)));
    assert_eq!(v.module(Slot::Low, 0).map(|m| m.type_id), Some(ids::DC), "a new module joins the variation");
    s.edit(EditKind::Variation, 8, |f| f.active_variations.clear());
    let base = s.view_fit().unwrap();
    assert_eq!(base.module(Slot::High, 0).map(|m| m.charge), Some(Some(ids::AMMO)));
    assert!(base.module(Slot::Low, 0).is_none());
    s.edit(EditKind::Variation, 9, |f| spai_fit::variations::activate(f, 1));
    assert_eq!(s.view_fit().unwrap().module(Slot::Low, 0).map(|m| m.type_id), Some(ids::DC), "kept with the variation");
}

#[test]
fn doctrines_open_as_copies_and_admins_publish_the_open_fit() {
    let mut s = sim();
    let mut fit = Fit::new(ids::SHIP, "Doctrine Rifter");
    fit.add_module(ids::GUN, Slot::High);
    fit.variations.push(spai_fit::variations::Variation { id: 1, name: "AB".into(), kind: spai_fit::variations::VariationKind::Refit, refit_key: Some("prop:ab".into()), ops: vec![], group: None });
    let doc: serde_json::Value = serde_json::from_str(&spai_fit::doc::to_json(&fit)).unwrap();
    s.handle(HostEvent::Doctrines(vec![Doctrine { id: "d1".into(), side: DoctrineSide::Imperium, name: "Alpha Rifter".into(), ship: ids::SHIP, tags: vec!["alpha".into()], refit_keys: vec!["prop:ab".into()], rev: 1, doc }]));
    s.open_doctrine("d1", 5);
    let open = s.session().unwrap();
    assert_eq!(open.fit.name, "Alpha Rifter");
    assert_ne!(open.id, "d1", "a copy with its own id");
    assert_eq!(open.fit.modules.len(), 1);
    assert!(!open.dirty, "a copy to look at is not a library entry until it is edited");
    s.edit(EditKind::Other, 6, |f| f.name.push('!'));
    assert!(s.session().unwrap().dirty);
    s.handle(HostEvent::Access { signed_in: true, imperium: true, admin: true });
    s.start_publish();
    let d = s.publish.as_mut().unwrap();
    d.name = "Alpha Rifter v2".into();
    d.side = DoctrineSide::Enemy;
    d.tags = "alpha, kiting".into();
    let (id, name, side, tags, keys, doc) = {
        let (Some(d), Some(sess)) = (s.publish.take(), s.session()) else { panic!() };
        (d.id, d.name, d.side, d.tags, sess.fit.variations.iter().filter_map(|v| v.refit_key.clone()).collect::<Vec<_>>(), spai_fit::doc::to_json(&sess.fit))
    };
    s.request(HostRequest::PublishDoctrine { id: id.clone(), side, name: name.clone(), tags: tags.split(',').map(|t| t.trim().to_owned()).collect(), refit_keys: keys.clone(), doc });
    let reqs = s.take_requests();
    assert!(matches!(&reqs[0], HostRequest::PublishDoctrine { side: DoctrineSide::Enemy, refit_keys, tags, .. } if refit_keys == &vec!["prop:ab".to_owned()] && tags == &vec!["alpha".to_owned(), "kiting".to_owned()]), "{reqs:?}");
    let json = serde_json::to_string(&s.doctrines[0]).unwrap();
    assert!(json.contains("\"side\":\"imperium\"") && json.contains("\"ship_type_id\""));
}

#[test]
fn fleets_ask_for_fit_documents_and_compute_once_they_arrive() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id: fit_id, doc, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let (fit_id, doc) = (fit_id.clone(), doc.clone());
    s.handle(HostEvent::Saved(fit_id.clone()));
    s.handle(HostEvent::Fits(vec![FitSummary { id: fit_id.clone(), name: "Rifter".into(), ship: ids::SHIP, updated_at: 0 }]));
    s.new_fleet(10);
    let f = s.fleet.as_mut().unwrap();
    f.fleet.size = 20;
    f.fleet.range = 1000.0;
    f.fleet.slots.push(FleetSlot { fit_id: fit_id.clone(), rule: CountRule::Fill { min: 0, max: 100 }, override_count: None, profile: None, variation: None, charges: Vec::new(), mode: None, splits: Vec::new(), states: Vec::new(), scripts: Vec::new(), ewar_tune: Vec::new() });
    let (mine, enemy) = s.fleet_stats_settled().unwrap();
    assert_eq!((mine.ships, mine.slots.len()), (20, 1), "the saved fit's document is already at hand");
    assert!(enemy.is_none());
    assert!(mine.dps > 0.0);
    s.forget_fit_doc(&fit_id);
    assert_eq!(s.fleet_stats_settled().unwrap().0.slots.len(), 0, "without the document the slot is left out");
    s.handle(HostEvent::FitDocs(vec![(fit_id.clone(), doc.clone())]));
    assert_eq!(s.fleet_stats_settled().unwrap().0.slots.len(), 1);
    s.save_fleet();
    let reqs = s.take_requests();
    let Some(HostRequest::SaveFleet { id, name, doc: fleet_doc }) = reqs.iter().find(|r| matches!(r, HostRequest::SaveFleet { .. })) else { panic!("{reqs:?}") };
    assert_eq!(name, "New fleet");
    s.handle(HostEvent::FleetSaved(id.clone()));
    assert!(!s.fleet.as_ref().unwrap().dirty);
    s.handle(HostEvent::Fleets(vec![FleetSummary { id: id.clone(), name: name.clone(), updated_at: 0 }]));
    s.handle(HostEvent::FleetDoc { id: "enemy".into(), doc: fleet_doc.clone() });
    s.fleet.as_mut().unwrap().enemy = Some("enemy".into());
    let (_, enemy) = s.fleet_stats_settled().unwrap();
    let (theirs, a, b) = enemy.unwrap();
    assert_eq!(theirs.ships, 20);
    assert!(a.applied_dps > 0.0 && (a.applied_dps - b.applied_dps).abs() < 1e-9, "a fleet against its own copy is symmetric: {a:?} vs {b:?}");
    assert!(a.critical_mass.is_some());
    s.handle(HostEvent::Fleet { id: "enemy".into(), doc: "nope".into() });
    assert_eq!(s.fleet.as_ref().unwrap().id, *id, "a broken fleet document changes nothing");
    // A mirror match: the fleet as edited, unsaved changes too, against itself.
    s.fleet.as_mut().unwrap().enemy = Some(crate::fleet_panel::MIRROR.into());
    s.fleet.as_mut().unwrap().fleet.size = 30;
    let (_, enemy) = s.fleet_stats_settled().unwrap();
    let (theirs, a, b) = enemy.expect("the mirror needs no document");
    assert_eq!(theirs.ships, 30, "the copy follows the edits");
    assert!((a.applied_dps - b.applied_dps).abs() < 1e-9);
    let an = s.fleet_analysis_settled().expect("the mirror is analysed");
    assert_eq!(an.sweep.a_best, an.sweep.b_best);
    assert!(an.sweep.a_favoured.is_empty() && an.sweep.b_favoured.is_empty(), "neither side is favoured against itself");
    // A knob moving keeps the last analysis on screen until it rests.
    s.now_ms = 100_000;
    s.fleet.as_mut().unwrap().fleet.range = 9_000.0;
    let held = s.fleet_analysis().unwrap();
    assert!(std::sync::Arc::ptr_eq(&held, &an), "the last one while the knob moves");
    assert!(s.fleet_analysis_pending());
    s.now_ms += crate::fleet_panel::SETTLE_MS;
    // Worked out once it rests, on its own thread: the last one until it is in.
    let mut fresh = s.fleet_analysis().unwrap();
    for _ in 0..300 {
        if !std::sync::Arc::ptr_eq(&fresh, &an) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        fresh = s.fleet_analysis().unwrap();
    }
    assert!(!std::sync::Arc::ptr_eq(&fresh, &an) && fresh.range == 9_000.0, "worked out once it rests");
    assert!(!s.fleet_analysis_pending());
    // The copy at its own numbers.
    s.fleet.as_mut().unwrap().fleet.mirror_size = Some(12);
    assert_eq!(s.fleet_stats_settled().unwrap().1.unwrap().0.ships, 12, "the mirror at its own size");
    s.fleet.as_mut().unwrap().fleet.mirror_counts = vec![Some(5)];
    let (mine, theirs) = s.fleet_stats_settled().map(|(m, e)| (m, e.unwrap().0)).unwrap();
    assert_eq!((theirs.ships, mine.ships), (5, 30), "a typed count for the copy, the fleet itself untouched");
}

#[test]
fn a_double_click_puts_each_kind_where_it_belongs() {
    use crate::browser::quick_add;
    let mut s = sim();
    assert!(!quick_add(&mut s, ids::GUN, 1), "no hull yet: a module has nowhere to go");
    assert!(quick_add(&mut s, ids::SHIP, 1));
    assert!(s.session().is_some());
    assert!(quick_add(&mut s, ids::GUN, 2) && quick_add(&mut s, ids::GUN, 3));
    let fit = |s: &SimUi| s.session().unwrap().fit.clone();
    assert_eq!(fit(&s).rack(Slot::High).map(|m| m.index).collect::<Vec<_>>(), vec![0, 1], "the first free slots");
    quick_add(&mut s, ids::AMMO_LONG, 4);
    assert!(fit(&s).rack(Slot::High).all(|m| m.charge == Some(ids::AMMO_LONG)), "a charge loads every weapon that takes it");
    assert!(fit(&s).cargo.is_empty());
    quick_add(&mut s, ids::CAP_CHARGE, 5);
    assert_eq!(fit(&s).cargo.len(), 1, "a charge nothing takes goes to the cargo");
    quick_add(&mut s, ids::DRONE, 6);
    quick_add(&mut s, ids::DRONE, 7);
    assert_eq!(fit(&s).drones[0].count, 2);
    quick_add(&mut s, ids::IMPLANT, 8);
    quick_add(&mut s, ids::IMPLANT, 9);
    assert_eq!(fit(&s).implants, vec![ids::IMPLANT], "one implant per slot: the second replaces the first");
    quick_add(&mut s, ids::BOOSTER, 10);
    assert_eq!(fit(&s).boosters.len(), 1);
    assert!(fit(&s).cargo.iter().all(|c| c.type_id != ids::IMPLANT), "an implant is worn, never carried");
    assert_eq!(fit(&s).cargo.iter().find(|c| c.type_id == ids::BOOSTER).map(|c| c.quantity), Some(1), "a booster in use comes from the cargo");
    quick_add(&mut s, ids::SHIP, 11);
    assert_eq!(fit(&s).ship, ids::SHIP);
}

#[test]
fn a_drop_lands_only_where_it_makes_sense() {
    use crate::browser::{drop_on, DropTarget};
    use crate::dock::DockTab;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    assert!(drop_on(&mut s, DropTarget::Module(Slot::High, 1), ids::GUN, 1));
    assert!(!drop_on(&mut s, DropTarget::Module(Slot::Mid, 0), ids::GUN, 2), "a high-slot module on a mid slot");
    assert!(drop_on(&mut s, DropTarget::Module(Slot::High, 1), ids::AMMO, 3), "a charge on the gun that takes it");
    assert!(!drop_on(&mut s, DropTarget::Module(Slot::High, 0), ids::AMMO, 4), "no module there");
    assert!(!drop_on(&mut s, DropTarget::Module(Slot::High, 1), ids::PASTE, 5), "paste does not go in a gun");
    let fit = |s: &SimUi| s.session().unwrap().fit.clone();
    assert_eq!(fit(&s).module(Slot::High, 1).unwrap().charge, Some(ids::AMMO));
    assert!(drop_on(&mut s, DropTarget::Dock(DockTab::Cargo), ids::IMPLANT, 6));
    assert_eq!((fit(&s).implants.len(), fit(&s).cargo.len()), (1, 0), "an implant dropped on the cargo is worn, not carried");
    assert!(drop_on(&mut s, DropTarget::Dock(DockTab::Drones), ids::BOOSTER, 7));
    assert_eq!(fit(&s).boosters.len(), 1);
    assert!(!drop_on(&mut s, DropTarget::Dock(DockTab::Drones), ids::AMMO, 8));
    assert!(drop_on(&mut s, DropTarget::Dock(DockTab::Cargo), ids::AMMO, 9));
    assert_eq!(fit(&s).cargo.iter().map(|c| c.type_id).collect::<Vec<_>>(), vec![ids::BOOSTER, ids::AMMO], "the booster in use and the ammo dropped");
    assert!(!drop_on(&mut s, DropTarget::Hull, ids::GUN, 10));
    assert!(drop_on(&mut s, DropTarget::Hull, ids::SHIP, 11));
    assert!(drop_on(&mut s, DropTarget::Module(Slot::High, 1), ids::GUN, 12), "a module on a full slot replaces what is there");
    assert_eq!(fit(&s).rack(Slot::High).count(), 1);
}

#[test]
fn the_browser_tree_holds_only_groups_with_something_in_them() {
    use crate::browser::Tree;
    let d = dataset();
    let t = Tree::build(&d);
    let roots: Vec<&str> = t.roots().iter().map(|g| t.name(*g)).collect();
    assert_eq!(roots, vec!["Ammunition & Charges", "Drones", "Fighters", "Implants & Boosters", "Ship Equipment", "Ship Modifications", "Ships"]);
    let equipment = t.roots().iter().copied().find(|g| t.name(*g) == "Ship Equipment").unwrap();
    let children: Vec<&str> = t.children(equipment).iter().map(|g| t.name(*g)).collect();
    assert!(children.contains(&"Turrets & Launchers") && children.contains(&"Propulsion"), "{children:?}");
    assert!(!children.contains(&"Hull & Armor") || t.children(equipment).iter().any(|g| !t.types(*g).is_empty()));
    let turrets = t.children(equipment).iter().copied().find(|g| t.name(*g) == "Turrets & Launchers").unwrap();
    let projectile = t.children(turrets).iter().copied().find(|g| t.name(*g) == "Projectile Turrets").unwrap();
    assert!(t.types(projectile).contains(&ids::GUN));
    assert_eq!(t.path(&d, projectile), "Ship Equipment › Turrets & Launchers › Projectile Turrets");
    let mut b = crate::browser::Browser::default();
    b.query = "auto".into();
    assert_eq!(b.search(&d), &[ids::GUN]);
    b.query = "II".into();
    assert!(b.search(&d).len() > 3);
    b.query = String::new();
    assert!(b.search(&d).is_empty(), "no query, no hits: the tree shows instead");
}

#[test]
fn the_dock_shows_as_many_panels_as_fit_from_the_chosen_tab() {
    use crate::dock::{visible, DockTab, PANEL_W};
    let tabs = [DockTab::Drones, DockTab::Implants, DockTab::Boosters, DockTab::Cargo];
    assert_eq!(visible(PANEL_W - 1.0, &tabs, 0), vec![DockTab::Drones], "too narrow for two: one panel");
    assert_eq!(visible(PANEL_W * 2.5, &tabs, 0), vec![DockTab::Drones, DockTab::Implants]);
    assert_eq!(visible(PANEL_W * 2.5, &tabs, 3), vec![DockTab::Boosters, DockTab::Cargo], "the last tab chosen: the window slides back so two still show");
    assert_eq!(visible(PANEL_W * 10.0, &tabs, 2), tabs.to_vec());
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    assert_eq!(crate::dock::tabs(&s), vec![DockTab::Drones, DockTab::Implants, DockTab::Boosters, DockTab::Cargo, DockTab::Projected], "a frigate has no fighter bay");
    s.edit(EditKind::Drones, 1, |f| f.fighters.push(spai_fit::fit::FighterSquad { type_id: ids::FIGHTER, count: 1, active: true }));
    assert!(crate::dock::tabs(&s).contains(&DockTab::Fighters));
}

#[test]
fn module_rows_carry_their_own_draw_and_the_states_they_can_take() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Module(Slot::Low, 0), ids::DC, 2);
    s.computed();
    let stats = s.session().unwrap().module_stats.clone();
    assert_eq!(stats.len(), 2);
    assert!(stats.iter().any(|m| m.slot == Slot::High && m.cpu > 0.0));
    let d = dataset();
    assert_eq!(crate::editor::states_of(&d, ids::DC), vec![State::Offline, State::Online], "a damage control is never active");
    assert_eq!(crate::editor::next_state(&d, ids::DC, State::Online), State::Offline);
    assert_eq!(crate::editor::next_state(&d, ids::GUN, State::Online), State::Active);
    assert_eq!(crate::editor::next_state(&d, ids::GUN, State::Offline), State::Online);
}

#[test]
fn rows_of_a_list_must_share_one_height_and_pitch() {
    use crate::rows::uneven_lists;
    let id = egui::Id::new("list");
    let r = |y: f32, h: f32| egui::Rect::from_min_size(egui::pos2(0.0, y), egui::vec2(100.0, h));
    assert!(uneven_lists(&[(id, r(0.0, 24.0)), (id, r(26.0, 24.0)), (id, r(52.0, 24.0))]).is_empty());
    assert!(uneven_lists(&[(id, r(0.0, 24.0)), (id, r(0.0, 24.0)), (id, r(26.0, 24.0))]).is_empty(), "two panels side by side");
    assert_eq!(uneven_lists(&[(id, r(0.0, 24.0)), (id, r(26.0, 30.0))]).len(), 1, "a taller row");
    assert_eq!(uneven_lists(&[(id, r(0.0, 24.0)), (id, r(26.0, 24.0)), (id, r(60.0, 24.0))]).len(), 1, "a gap");
    let other = egui::Id::new("other");
    assert!(uneven_lists(&[(id, r(0.0, 24.0)), (other, r(0.0, 30.0))]).is_empty(), "lists are judged apart");
}

#[test]
fn fits_open_in_tabs_and_save_themselves_after_a_pause() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.new_fit(ids::SHIP, 1);
    assert_eq!(s.sessions().len(), 2);
    assert_eq!(s.active_tab(), 1);
    s.activate(0);
    s.edit(EditKind::Other, 100, |f| f.name = "First".into());
    s.autosave(100 + AUTOSAVE_MS - 1);
    assert!(s.take_requests().is_empty(), "nothing goes out while the user is still typing");
    s.autosave(100 + AUTOSAVE_MS);
    let reqs = s.take_requests();
    let [HostRequest::SaveFit { id, name, .. }] = reqs.as_slice() else { panic!("{reqs:?}") };
    assert_eq!(name, "First");
    s.autosave(100 + 10 * AUTOSAVE_MS);
    assert!(s.take_requests().is_empty(), "one save per change, until the host answers");
    s.handle(HostEvent::Saved(id.clone()));
    assert!(!s.session().unwrap().dirty);
    assert_eq!(s.take_requests(), vec![HostRequest::ListFits]);
    // A fit already open comes to the front instead of opening twice.
    let doc = spai_fit::doc::to_json(&s.session().unwrap().fit);
    s.activate(1);
    s.handle(HostEvent::Fit { id: id.clone(), doc });
    assert_eq!(s.sessions().len(), 2);
    assert_eq!(s.active_tab(), 0);
    // Closing a changed tab saves it first; closing the other leaves the right one showing.
    s.edit(EditKind::Other, 200, |f| f.name = "First again".into());
    s.close_tab(0);
    assert!(matches!(s.take_requests().as_slice(), [HostRequest::SaveFit { name, .. }] if name == "First again"));
    assert_eq!(s.sessions().len(), 1);
    assert_eq!(s.active_tab(), 0);
    s.close_id(&s.sessions()[0].id.clone());
    assert!(s.session().is_none());
}

#[test]
fn clicks_pick_rows_alone_toggle_with_ctrl_and_stretch_with_shift() {
    use crate::editor::pick;
    let order = vec![(Slot::High, 0), (Slot::High, 1), (Slot::High, 2), (Slot::Mid, 0)];
    let (mut sel, mut anchor) = (Vec::new(), None);
    pick(&mut sel, &mut anchor, &order, (Slot::High, 1), false, false);
    assert_eq!(sel, vec![(Slot::High, 1)]);
    pick(&mut sel, &mut anchor, &order, (Slot::Mid, 0), true, false);
    assert_eq!(sel, vec![(Slot::High, 1), (Slot::Mid, 0)], "ctrl adds");
    pick(&mut sel, &mut anchor, &order, (Slot::Mid, 0), true, false);
    assert_eq!(sel, vec![(Slot::High, 1)], "ctrl again takes it out");
    pick(&mut sel, &mut anchor, &order, (Slot::High, 0), false, false);
    pick(&mut sel, &mut anchor, &order, (Slot::High, 2), false, true);
    assert_eq!(sel, vec![(Slot::High, 0), (Slot::High, 1), (Slot::High, 2)], "shift takes the stretch from the anchor");
    pick(&mut sel, &mut anchor, &order, (Slot::Mid, 0), false, false);
    assert_eq!(sel, vec![(Slot::Mid, 0)], "a plain click starts over");
}

#[test]
fn a_shared_menu_offers_only_what_every_picked_module_can_do() {
    use crate::editor::{common_charges, common_states};
    let d = dataset();
    assert_eq!(common_states(&d, &[ids::GUN, ids::GUN]), crate::editor::states_of(&d, ids::GUN));
    assert_eq!(common_states(&d, &[ids::GUN, ids::DC]), vec![State::Offline, State::Online], "a damage control never activates");
    assert_eq!(common_charges(&d, &[ids::GUN, ids::GUN]), crate::picker::compatible_charges(&d, ids::GUN));
    assert!(common_charges(&d, &[ids::GUN, ids::DC]).is_empty(), "no charge fits both");
    let any = crate::editor::any_charges(&d, &[ids::GUN, ids::LAUNCHER]);
    assert!(any.contains(&ids::AMMO) && any.contains(&ids::MISSILE), "a mixed selection lists what either takes");
    assert!(common_states(&d, &[]).is_empty());
}

#[test]
fn details_list_the_raw_and_fitted_numbers_with_who_changed_them() {
    use crate::details::{rows_for_item, rows_for_type, DetailsTarget};
    use spai_fit::calc::{Calc, Env};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    let d = s.dogma.clone().unwrap();
    let fit = s.session().unwrap().fit.clone();
    let calc = Calc::new(&d, &fit, s.skills(), &Env::default());
    let gun = calc.module(Slot::High, 0).unwrap();
    let rows = rows_for_item(&d, &calc, gun, true);
    let falloff = rows.iter().find(|r| r.attr == spai_fit::dogma::attr::FALLOFF).expect("falloff is listed");
    assert!(falloff.changed && falloff.fitted.is_some());
    assert!(!falloff.steps.is_empty(), "the skills that raise it are named");
    assert!(falloff.steps.iter().all(|st| st.good == Some(true) && st.who.contains("skill") || st.who.contains("hull")), "{:?}", falloff.steps);
    let cpu = rows.iter().find(|r| r.attr == spai_fit::dogma::attr::CPU).unwrap();
    assert!(!cpu.changed && cpu.steps.is_empty());
    let raw = rows_for_type(&d, ids::GUN, true);
    assert!(raw.iter().all(|r| r.fitted.is_none()) && raw.len() <= rows.len());
    s.open_details(DetailsTarget::Module(Slot::High, 0));
    s.open_details(DetailsTarget::Module(Slot::High, 0));
    assert_eq!(s.details.len(), 1, "one window per item");
}

#[test]
fn a_charge_or_drone_off_the_fit_shows_its_numbers_as_the_fit_would_use_it() {
    use crate::details::{build, DetailsTarget};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    let ammo = build(&s, DetailsTarget::Type(ids::AMMO), false).unwrap();
    assert!(ammo.fitted && ammo.trial.as_deref().is_some_and(|n| n.contains("Gun") || n.contains(&spai_fit::formats::name_of(s.dogma.as_ref().unwrap(), ids::GUN))), "{:?}", ammo.trial);
    let missile = build(&s, DetailsTarget::Type(ids::MISSILE), false).unwrap();
    let launcher = spai_fit::formats::name_of(s.dogma.as_ref().unwrap(), ids::LAUNCHER);
    assert!(missile.fitted && missile.trial.as_deref().is_some_and(|n| n.contains(&launcher)), "none fitted: the game's launcher: {:?}", missile.trial);
    let drone = build(&s, DetailsTarget::Type(ids::DRONE), true).unwrap();
    assert!(drone.fitted && drone.trial.is_some() && drone.rows.iter().any(|r| r.changed), "skills raise the drone's numbers");
    let gun = build(&s, DetailsTarget::Type(ids::GUN), false).unwrap();
    assert!(!gun.fitted && gun.trial.is_none(), "a module off the fit shows the data's numbers");
}

#[test]
fn a_full_rack_takes_no_more_from_a_double_click() {
    use crate::browser::quick_add;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    let total = s.computed().unwrap().fitting.slots[0].1;
    for i in 0..total {
        assert!(quick_add(&mut s, ids::GUN, 10 + u64::from(i)));
    }
    assert!(!quick_add(&mut s, ids::GUN, 99), "nothing hangs past the last slot");
    assert_eq!(s.session().unwrap().fit.rack(Slot::High).count(), total as usize);
}

#[test]
fn an_import_lands_where_it_belongs() {
    let mut s = sim();
    let eft = |name: &str, extra: &str| format!("[Rifter, {name}]\nDamage Control II\n\n1MN Microwarpdrive II\n\n200mm AutoCannon II\n200mm AutoCannon II{extra}");
    // Into the untouched tab on screen rather than a new one.
    s.new_fit(ids::SHIP, 0);
    assert!(s.import_text(&eft("Brawler", ""), 1));
    assert_eq!(s.sessions().len(), 1);
    assert_eq!(s.session().unwrap().fit.name, "Brawler");
    assert!(s.session().unwrap().dirty, "an import is worth keeping");
    // The same fitting again brings the open tab forward instead of a copy.
    s.new_fit(ids::SHIP, 2);
    assert!(s.import_text(&eft("Brawler copy", ""), 3));
    assert_eq!(s.sessions().len(), 2);
    assert_eq!(s.active_tab(), 0);
    // A near copy of the hull asks instead of opening blind.
    s.activate(1);
    let one_module_off = "[Rifter, Kite]\nDamage Control II\n\n1MN Microwarpdrive II\n\n200mm AutoCannon II\nSmall Armor Repairer II";
    assert!(s.import_text(one_module_off, 5));
    let ask = s.import_ask.clone().expect("a question");
    assert_eq!(ask.base_name, "Brawler");
    s.import_as_variation(6);
    assert!(s.import_ask.is_none());
    let base = s.session().unwrap();
    assert_eq!(base.fit.name, "Brawler");
    assert_eq!(base.fit.variations.len(), 1);
    assert_eq!(base.fit.variations[0].name, "Kite");
    assert!(!base.fit.variations[0].ops.is_empty());
    // Not a fit at all: refused, nothing opened.
    let before = s.sessions().len();
    assert!(!s.import_text("hello", 7));
    assert_eq!(s.sessions().len(), before);
    // The Import button's clipboard read: a fit imports, anything else lands in the paste box.
    s.handle(HostEvent::Clipboard("junk".into()));
    assert!(s.import.is_none(), "an unasked paste does not open the dialog");
    s.request(HostRequest::ReadClipboard);
    let _ = s.take_requests();
    s.import_from_button_for_test();
    s.handle(HostEvent::Clipboard("junk".into()));
    assert!(s.import.as_ref().is_some_and(|d| d.text == "junk" && d.error.is_some()));
}

#[test]
fn a_dragged_module_moves_or_with_shift_copies_within_its_rack() {
    use crate::editor::move_module;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Module(Slot::High, 1), ids::LAUNCHER, 2);
    let at = |s: &SimUi, i: u8| s.session().unwrap().fit.module(Slot::High, i).map(|m| m.type_id);
    move_module(&mut s, Slot::High, 0, 1, false, 3);
    assert_eq!((at(&s, 0), at(&s, 1)), (Some(ids::LAUNCHER), Some(ids::GUN)), "a move onto a fitted slot trades places");
    move_module(&mut s, Slot::High, 1, 2, true, 4);
    assert_eq!((at(&s, 1), at(&s, 2)), (Some(ids::GUN), Some(ids::GUN)), "shift copies");
    assert_eq!(s.session().unwrap().fit.rack(Slot::High).count(), 3);
    move_module(&mut s, Slot::High, 0, 3, false, 5);
    assert_eq!((at(&s, 0), at(&s, 3)), (None, Some(ids::LAUNCHER)), "onto an empty slot it simply moves");
}

#[test]
fn what_acts_from_outside_goes_to_projected() {
    use crate::browser::{add_projected, drop_on, DropTarget};
    use crate::DockTab;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    assert!(add_projected(&mut s, ids::WEB, 1));
    assert!(add_projected(&mut s, ids::WEB, 2));
    assert!(drop_on(&mut s, DropTarget::Dock(DockTab::Projected), ids::BEACON, 3));
    assert!(add_projected(&mut s, ids::BEACON, 4));
    let p = &s.session().unwrap().fit.projected;
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].count, 2, "a second web is one more of it");
    assert_eq!(p[1].count, 1, "one weather at a time");
    assert!(!add_projected(&mut s, ids::DC, 5), "a damage control acts on nothing outside its own ship");
    let slow = s.computed().unwrap().mobility.max_velocity;
    s.edit(EditKind::Other, 6, |f| f.projected.clear());
    assert!(s.computed().unwrap().mobility.max_velocity > slow);
}

#[test]
fn fits_group_by_hull_or_tag_with_the_doctrines_among_them() {
    use crate::library_panel::{entries, lines, parse_tags, Grouping, Line};
    let mut s = sim();
    s.handle(HostEvent::Fits(vec![
        FitSummary { id: "a".into(), name: "Kiter".into(), ship: ids::SHIP, updated_at: 0 },
        FitSummary { id: "b".into(), name: "Brawler".into(), ship: ids::SHIP, updated_at: 0 },
    ]));
    let mut kiter = Fit::new(ids::SHIP, "Kiter");
    kiter.tags = vec!["kiting".into(), "Alpha".into()];
    s.fit_docs.insert("a".into(), kiter.clone());
    let doc: serde_json::Value = serde_json::from_str(&spai_fit::doc::to_json(&kiter)).unwrap();
    s.handle(HostEvent::Doctrines(vec![Doctrine { id: "d".into(), side: DoctrineSide::Imperium, name: "Alpha Rifter".into(), ship: ids::SHIP, tags: vec!["Alpha".into()], refit_keys: vec![], rev: 1, doc }]));
    let all = entries(&s);
    assert_eq!(all.len(), 3, "the doctrine sits among the fits");
    assert!(all.iter().any(|e| e.doctrine.is_some() && e.tags.iter().any(|t| t.contains("doctrine"))), "pre-tagged as a doctrine");
    let name = |_t: u32| crate::library_panel::HullPath { class: (0, "Frigate".into()), group: "Frigate".into(), hull: "Rifter".into() };
    let none = std::collections::BTreeSet::new();
    let by_hull = lines(&all, &name, Grouping::Hull, "", &none);
    assert!(matches!(&by_hull[0], Line::Group { n: 3, depth: 0, .. }), "one class, three fits");
    assert!(matches!(&by_hull[2], Line::Group { n: 3, depth: 2, title, .. } if title == "Rifter"), "then its group, then the hull");
    let by_tag = lines(&all, &name, Grouping::Tag, "", &none);
    let alpha = by_tag.iter().find_map(|l| match l { Line::Group { title, n, .. } if title == "Alpha" => Some(*n), _ => None });
    assert_eq!(alpha, Some(2), "a fit and a doctrine both tagged Alpha");
    assert!(matches!(by_tag.iter().rev().find(|l| matches!(l, Line::Group { .. })), Some(Line::Group { key, .. }) if key == "tag:"), "the untagged come last");
    let found = lines(&all, &name, Grouping::Flat, "kit", &none);
    assert_eq!(found.len(), 1, "the search reads tags as well as names");
    let mut shut = std::collections::BTreeSet::new();
    shut.insert(format!("hull:{}", ids::SHIP));
    assert_eq!(lines(&all, &name, Grouping::Hull, "", &shut).len(), 3, "a folded hull shows its header only, under its class and group");
    shut.insert("class:0".into());
    assert_eq!(lines(&all, &name, Grouping::Hull, "", &shut).len(), 1, "a folded class hides all under it");
    assert_eq!(parse_tags(" kiting, Alpha ,alpha,, "), vec!["kiting".to_owned(), "Alpha".to_owned()]);
    s.set_fit_tags("a", vec!["solo".into()], 9);
    assert!(matches!(s.take_requests().iter().find(|r| matches!(r, HostRequest::SaveFit { .. })), Some(HostRequest::SaveFit { id, doc, .. }) if id == "a" && doc.contains("solo")));
}

#[test]
fn the_new_fit_picker_opens_a_tab_and_the_hull_picker_changes_the_hull() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::NewFit, ids::SHIP, 1);
    assert_eq!(s.sessions().len(), 2, "the plus opens another tab");
    assert_eq!(s.active_tab(), 1);
    apply(&mut s, PickTarget::Ship, ids::SHIP, 2);
    assert_eq!(s.sessions().len(), 2, "the hull picker changes the fit on screen");
}

#[test]
fn the_plus_opens_an_empty_tab_that_a_hull_or_an_import_fills() {
    use crate::browser::quick_add;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.new_empty_tab(1);
    assert_eq!(s.sessions().len(), 2);
    assert!(!s.has_hull() && s.computed().is_none(), "nothing to compute without a hull");
    assert!(quick_add(&mut s, ids::SHIP, 2));
    assert_eq!(s.sessions().len(), 2, "the hull goes into the empty tab");
    assert!(s.has_hull() && s.session().unwrap().fit.name == "Rifter", "named after its hull");
    s.new_empty_tab(3);
    assert!(s.import_text("[Rifter, Pasted]\nDamage Control II", 4));
    assert_eq!(s.sessions().len(), 3);
    assert_eq!(s.session().unwrap().fit.name, "Pasted", "an import fills the empty tab");
}

#[test]
fn a_projected_fit_slows_this_one_and_follows_its_edits() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    let support = s.session().unwrap().id.clone();
    apply(&mut s, PickTarget::Module(Slot::Mid, 0), ids::WEB, 1);
    s.edit(EditKind::SetState, 2, |f| f.module_mut(Slot::Mid, 0).unwrap().state = State::Active);
    s.new_fit(ids::SHIP, 3);
    let free = s.computed().unwrap().mobility.max_velocity;
    let id = support.clone();
    s.edit(EditKind::Other, 4, move |f| f.projected_fits.push(spai_fit::fit::ProjectedFit { fit_id: id, ..Default::default() }));
    let webbed = s.computed().unwrap().mobility.max_velocity;
    assert!(webbed < free * 0.5, "{webbed} vs {free}: the other tab's web lands");
    // Switching the web off in its own tab frees this one again.
    s.activate(0);
    s.edit(EditKind::SetState, 5, |f| f.module_mut(Slot::Mid, 0).unwrap().state = State::Online);
    s.activate(1);
    assert!((s.computed().unwrap().mobility.max_velocity - free).abs() < 1e-6, "the numbers follow the projected fit");
}

#[test]
fn the_cargo_takes_anything_without_fitting_it() {
    use crate::browser::{drop_on, DropTarget};
    use crate::DockTab;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    assert!(drop_on(&mut s, DropTarget::Dock(DockTab::Cargo), ids::GUN, 1));
    assert!(drop_on(&mut s, DropTarget::Dock(DockTab::Cargo), ids::GUN, 2));
    apply(&mut s, PickTarget::Cargo, ids::SHIP, 3);
    let f = &s.session().unwrap().fit;
    assert!(f.modules.is_empty(), "a module dropped on the cargo is carried, not fitted");
    assert_eq!(f.cargo.iter().find(|c| c.type_id == ids::GUN).map(|c| c.quantity), Some(2));
    assert!(f.cargo.iter().any(|c| c.type_id == ids::SHIP));
    assert!(results(&s.dogma.clone().unwrap(), PickTarget::Cargo, None, "").contains(&ids::DC));
}

#[test]
fn bay_pickers_stay_open_for_more_and_slot_pickers_close() {
    use crate::picker::stays_open;
    assert!(stays_open(PickTarget::Drone) && stays_open(PickTarget::Cargo) && stays_open(PickTarget::Projected) && stays_open(PickTarget::Environment));
    assert!(!stays_open(PickTarget::Module(Slot::High, 0)) && !stays_open(PickTarget::Ship) && !stays_open(PickTarget::NewFit) && !stays_open(PickTarget::Charge(Slot::High, 0)));
}

#[test]
fn a_press_is_a_drag_only_once_it_travels_and_lasts() {
    use crate::rows::is_drag;
    let o = egui::pos2(100.0, 100.0);
    assert!(!is_drag(o, egui::pos2(103.0, 101.0), 1.0), "a slipping click stays a click");
    assert!(!is_drag(o, egui::pos2(140.0, 100.0), 0.05), "a flick too quick to mean it");
    assert!(is_drag(o, egui::pos2(110.0, 104.0), 0.2));
}

#[test]
fn a_sized_module_takes_only_charges_of_its_size_that_fit_in_it() {
    use crate::picker::compatible_charges;
    use spai_fit::dogma::attr;
    use spai_fit::test_support::type_def;
    let mut d = dataset();
    let asb = 90_001;
    d.types.push(type_def(asb, "X-Large Ancillary Shield Booster", 40, 7, &[(attr::CHARGE_GROUP_1, 87.0), (attr::CHARGE_SIZE, 3.0), (attr::CAPACITY, 112.0)], &[], &[]));
    let charge = |id: u32, name: &str, size: Option<f32>, volume: f32| {
        let mut a = vec![(attr::VOLUME, volume)];
        if let Some(s) = size {
            a.push((attr::CHARGE_SIZE, s));
        }
        type_def(id, name, 87, 8, &a, &[], &[])
    };
    d.types.push(charge(90_002, "Cap Booster 25", None, 1.0));
    d.types.push(charge(90_003, "Cap Booster 400", Some(3.0), 16.0));
    d.types.push(charge(90_004, "Cap Booster 3200", Some(4.0), 128.0));
    d.types.push(charge(90_005, "Cap Booster Huge", Some(3.0), 200.0));
    d.groups.push(spai_fit::dogma::GroupDef { id: 87, name: "Capacitor Booster Charge".into(), category: 8 });
    let d = d.finish();
    assert_eq!(compatible_charges(&d, asb), vec![90_003], "size 3 only, and no bigger than the module holds");
}

#[test]
fn charges_sort_into_tech_and_each_faction() {
    use crate::picker::charge_sections;
    use spai_fit::test_support::type_def;
    let mut d = dataset();
    let mut t1 = type_def(91_001, "EMP S", 83, 8, &[], &[], &[]);
    t1.meta_group = Some(1);
    let mut t2 = type_def(91_002, "Barrage S", 83, 8, &[], &[], &[]);
    t2.meta_group = Some(2);
    let mut rf = type_def(91_003, "Republic Fleet EMP S", 83, 8, &[], &[], &[]);
    rf.meta_group = Some(4);
    rf.variation_parent = Some(91_001);
    let mut cn = type_def(91_004, "Caldari Navy EMP S", 83, 8, &[], &[], &[]);
    cn.meta_group = Some(4);
    cn.variation_parent = Some(91_001);
    d.types.extend([t1, t2, rf, cn]);
    let d = d.finish();
    let s = charge_sections(&d, &[91_001, 91_002, 91_003, 91_004]);
    let titles: Vec<&str> = s.iter().map(|x| x.title.as_str()).collect();
    assert_eq!(titles, vec!["Tech I", "Tech II", "Faction: Caldari Navy", "Faction: Republic Fleet"]);
    assert_eq!(s.iter().map(|x| x.plain).collect::<Vec<_>>(), vec![true, true, false, false]);
}

#[test]
fn a_short_menu_folds_nothing_and_a_long_one_folds_all_but_tech() {
    use crate::picker::{fold_sections, MenuSection, FOLD_FROM};
    let section = |title: &str, n: usize, plain: bool| MenuSection { title: title.to_owned(), items: (0..n as u32).collect(), plain };
    let short = vec![section("Tech I", 4, true), section("Tech II", 2, true), section("Faction: Republic Fleet", 3, false)];
    assert_eq!(short.iter().map(|s| s.items.len()).sum::<usize>(), FOLD_FROM - 1);
    let (flat, folded) = fold_sections(short.clone());
    assert_eq!((flat.len(), folded.len()), (3, 0), "under ten entries, everything is in the one menu");
    let mut long = short;
    long.push(section("Faction: Caldari Navy", 1, false));
    let (flat, folded) = fold_sections(long);
    assert_eq!(flat.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), vec!["Tech I", "Tech II"], "tech stays in the menu itself");
    assert_eq!(folded.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), vec!["Faction: Republic Fleet", "Faction: Caldari Navy"]);
}

#[test]
fn module_variants_section_by_meta_group_with_tech_first() {
    use spai_fit::ids::meta_group;
    use spai_fit::test_support::type_def;
    let mut d = dataset();
    let gun = d.types.iter().find(|t| t.id == ids::GUN).unwrap().clone();
    let mut named = |id, name: &str, group, level| {
        let mut t = type_def(id, name, gun.group, gun.category, &[], &[], &[]);
        t.meta_group = Some(group);
        t.meta_level = Some(level);
        t.variation_parent = Some(ids::GUN);
        d.types.push(t);
    };
    named(92_001, "Gun II", meta_group::TECH_II, 5);
    named(92_002, "Republic Fleet Gun", meta_group::FACTION, 8);
    named(92_003, "Gun I upgraded", meta_group::TECH_I, 1);
    named(92_004, "Domination Gun", meta_group::FACTION, 9);
    named(92_005, "Tobias' Gun", meta_group::OFFICER, 14);
    let d = d.finish();
    let s = crate::editor::variant_sections(&d, vec![92_001, 92_002, 92_003, 92_004, 92_005]);
    assert_eq!(s.iter().map(|x| (x.title.as_str(), x.items.clone(), x.plain)).collect::<Vec<_>>(), vec![("Tech", vec![92_003, 92_001], true), ("Faction", vec![92_002, 92_004], false), ("Officer", vec![92_005], false)]);
}

#[test]
fn a_charge_picked_on_a_selected_row_loads_every_selected_module_that_takes_it() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Module(Slot::High, 1), ids::GUN, 2);
    apply(&mut s, PickTarget::Module(Slot::Low, 0), ids::DC, 3);
    s.selection = vec![(Slot::High, 0), (Slot::High, 1), (Slot::Low, 0)];
    apply(&mut s, PickTarget::Charge(Slot::High, 1), ids::AMMO_LONG, 4);
    let f = &s.session().unwrap().fit;
    assert!(f.rack(Slot::High).all(|m| m.charge == Some(ids::AMMO_LONG)), "both guns");
    assert_eq!(f.module(Slot::Low, 0).unwrap().charge, None, "the damage control takes none");
}

#[test]
fn the_layout_is_handed_over_when_it_changes_and_read_back() {
    let mut s = sim();
    s.left = LeftTab::Doctrines;
    s.dock_h = Some(240.0);
    s.library_state.grouping = crate::library_panel::Grouping::Tag;
    let json = serde_json::to_string(&s.layout()).unwrap();
    let mut t = sim();
    t.handle(HostEvent::Layout(json));
    assert_eq!(t.left, LeftTab::Doctrines);
    assert_eq!(t.dock_h, Some(240.0));
    assert_eq!(t.library_state.grouping, crate::library_panel::Grouping::Tag);
    t.handle(HostEvent::Layout("{\"left_w\": 333.0, \"something_new\": 1}".into()));
    assert_eq!(t.layout().left_w, Some(333.0), "a newer copy with fields this build does not know still reads");
    assert_eq!(t.left, LeftTab::Doctrines, "and what it leaves out stays");
}

#[test]
fn fitted_tooltips_carry_the_values_the_fit_gives() {
    use crate::details::DetailsTarget;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    let tips = s.tip_values();
    let gun = tips.get(&DetailsTarget::Module(Slot::High, 0)).expect("the gun has fitted values");
    let base = f64::from(s.dogma.as_ref().unwrap().type_(ids::GUN).unwrap().attr(spai_fit::dogma::attr::FALLOFF).unwrap());
    assert!(gun[&spai_fit::dogma::attr::FALLOFF] > base, "skills lengthen the falloff in the tooltip too");
    assert!(std::sync::Arc::ptr_eq(&tips, &s.tip_values()), "figured once per change");
    apply(&mut s, PickTarget::Module(Slot::High, 1), ids::GUN, 2);
    assert!(!std::sync::Arc::ptr_eq(&tips, &s.tip_values()), "and again after one");
}

#[test]
fn the_slot_picker_offers_what_the_hull_and_the_fit_leave_room_for() {
    use crate::picker::{fits, FitFilter, FitRoom};
    let d = dataset();
    let gun = d.type_(ids::GUN).unwrap();
    let room = |turrets_free: u32, cpu_left: f64| FitRoom { ship: ids::SHIP, ship_group: 25, rig_size: None, cpu_total: 130.0, power_total: 40.0, cpu_left, power_left: 40.0, turrets_total: 3, launchers_total: 2, turrets_free, launchers_free: 2, costs: Default::default(), slot_group: None };
    assert!(fits(&d, gun, &room(1, 100.0), FitFilter::Left));
    assert!(!fits(&d, gun, &room(0, 100.0), FitFilter::Left), "no turret hardpoint free");
    assert!(fits(&d, gun, &room(0, 100.0), FitFilter::Totals), "but it fits an empty hull");
    assert!(!fits(&d, gun, &room(1, 1.0), FitFilter::Left), "not enough CPU left");
    // A reactor control unit adds a share of the powergrid: offered though the fit is over by
    // less than that, and on a fit far over it costs no powergrid, so it is kept too.
    let rcu = d.type_(ids::RCU).unwrap();
    let mut over = room(1, 100.0);
    over.power_left = -5.0;
    assert!(fits(&d, rcu, &over, FitFilter::Left), "20% of 40 MW makes up 5 MW over");
    over.power_left = -50.0;
    assert!(fits(&d, rcu, &over, FitFilter::Left), "it takes no powergrid, so it makes nothing worse");
    assert!(!fits(&d, rcu, &room(1, 0.5), FitFilter::Left), "its own CPU still has to fit");
    let mut no_turrets = room(1, 100.0);
    no_turrets.turrets_total = 0;
    assert!(!fits(&d, gun, &no_turrets, FitFilter::Hull) && fits(&d, gun, &no_turrets, FitFilter::All), "a hull without turret hardpoints takes no gun at all");
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    let r = s.fit_room(Slot::High, 0).expect("room on a fitted hull");
    assert!(r.cpu_left <= r.cpu_total && r.turrets_free <= r.turrets_total);
    // A hull that halves its guns' CPU, as role bonuses cut bombers' launchers.
    use spai_fit::dogma::{attr, Domain, EffectCategory, Func, Modifier, Op};
    let mut hd = dataset();
    let gun_group = hd.type_(ids::GUN).unwrap().group;
    hd.attributes.push(spai_fit::test_support::attr_def(9990, "shipGunCpuBonus", true, false, 0.0));
    hd.effects.push(spai_fit::test_support::effect(9990, "shipGunCpuBonus", EffectCategory::Passive, vec![Modifier { domain: Domain::Ship, func: Func::LocationGroup, modified: attr::CPU, modifying: 9990, op: Op::PostPercent, group: Some(gun_group), skill: None }]));
    let ship = hd.types.iter_mut().find(|t| t.id == ids::SHIP).unwrap();
    ship.attrs.push((9990, -50.0));
    ship.effects.push((9990, false));
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(Arc::new(hd.finish())));
    s.new_fit(ids::SHIP, 0);
    let r = s.fit_room(Slot::High, 0).unwrap();
    let raw_cpu = f64::from(gun.attr(attr::CPU).unwrap());
    let (cpu, _) = r.costs[&ids::GUN];
    assert!(cpu > 0.0 && cpu < raw_cpu, "skills cut its CPU: {cpu} of {raw_cpu}");
    let mut tight = room(1, (cpu + raw_cpu) / 2.0);
    assert!(!fits(&d, gun, &tight, FitFilter::Left), "by its own CPU it does not fit");
    tight.costs = r.costs.clone();
    assert!(fits(&d, gun, &tight, FitFilter::Left), "with what the skills leave of it, it does");
}

#[test]
fn the_slot_picker_lists_a_modules_cpu_powergrid_and_capacitor() {
    let d = dataset();
    assert_eq!(crate::picker::costs(&d, ids::GUN), [25.0, 15.0, 0.0], "a projectile turret needs no capacitor");
    let [cpu, power, cap] = crate::picker::costs(&d, ids::WEB);
    assert_eq!((cpu, power), (30.0, 1.0));
    assert!((cap - 6.0 / 5.0).abs() < 1e-9, "its need over its cycle: {cap}");
}

#[test]
fn a_fit_costs_what_its_parts_cost_at_the_market() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    let prices = br#"[{"type_id": 587, "average_price": 500000.0}, {"type_id": 2873, "average_price": 100000.0, "adjusted_price": 1.0}, {"type_id": 99999, "adjusted_price": 5.0}]"#;
    let parsed = crate::parse_prices(prices);
    assert!(parsed.contains(&(99_999, 5.0)), "the adjusted price stands in for a missing average");
    s.handle(HostEvent::Prices(parsed));
    let c = s.cost().unwrap();
    assert_eq!(c.hull, if ids::SHIP == 587 { 500_000.0 } else { 0.0 });
    assert!(c.missing > 0, "the ammo has no price in this list");
    assert!((c.total() - c.hull - c.modules - c.charges - c.drones - c.worn - c.cargo).abs() < 1e-6);
}

#[test]
fn a_roll_lands_within_the_mutaplasmid_and_best_is_best_for_each_attribute() {
    use crate::mutation::{icon_of, roll, MutTarget};
    use spai_fit::dogma::attr;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::Mid, 0), ids::MWD, 1);
    let d = s.dogma.clone().unwrap();
    let base = |a| f64::from(d.type_(ids::MWD).unwrap().attr(a).unwrap());
    let best = roll(&d, ids::MWD, ids::MUTAPLASMID, |lo, hi, high| if high { hi } else { lo }).unwrap();
    let get = |m: &spai_fit::fit::Mutation, a| f64::from(m.attrs.iter().find(|(x, _)| *x == a).unwrap().1);
    assert!((get(&best, attr::SPEED_FACTOR) - base(attr::SPEED_FACTOR) * 1.1).abs() < 1e-3, "more speed is better: the top of its range");
    assert!((get(&best, attr::CPU) - base(attr::CPU) * 0.8).abs() < 1e-3, "less CPU is better: the bottom of its range");
    s.set_mutation(MutTarget::Module(Slot::Mid, 0), Some(best.clone()), 2);
    let m = s.session().unwrap().fit.module(Slot::Mid, 0).unwrap().mutation.clone();
    assert_eq!(m.as_ref(), Some(&best));
    assert_eq!(icon_of(&d, ids::MWD, m.as_ref()), ids::ABYSSAL_MWD, "the rolled item shows the abyssal icon");
    assert!(s.computed().is_some());
    s.set_mutation(MutTarget::Module(Slot::Mid, 0), None, 3);
    assert!(s.session().unwrap().fit.module(Slot::Mid, 0).unwrap().mutation.is_none(), "None takes the roll off");
}

#[test]
fn the_incoming_damage_pattern_moves_the_effective_hp() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::Low, 0), ids::HARDENER, 1);
    s.edit(EditKind::SetState, 2, |f| f.module_mut(Slot::Low, 0).unwrap().state = State::Active);
    let uniform = s.computed().unwrap().tank.ehp;
    s.set_damage_profile(spai_fit::fit::DamageProfile { em: 1.0, thermal: 0.0, kinetic: 0.0, explosive: 0.0 });
    let em = s.computed().unwrap().tank.ehp;
    assert!(em > uniform, "all EM into an EM hardener: {em} against {uniform}");
    assert!(s.session().unwrap().fit.damage_profile.em == 1.0, "kept on the fit");
}

#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_every_damaging_charge_is_offered_as_a_profile() {
    use spai_fit::dogma::attr;
    let d = std::sync::Arc::new(spai_fit::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap());
    let tree = crate::browser::Tree::build(&d);
    let mut reachable = std::collections::HashSet::new();
    fn walk(tree: &crate::browser::Tree, g: spai_fit::ids::MarketGroupId, out: &mut std::collections::HashSet<u32>) {
        out.extend(tree.types(g).iter().copied());
        for &c in tree.children(g) {
            walk(tree, c, out);
        }
    }
    for &r in tree.roots() {
        walk(&tree, r, &mut reachable);
    }
    let damaging: Vec<&spai_fit::dogma::TypeDef> = d.types.iter().filter(|t| t.published && t.category == spai_fit::ids::category::CHARGE && [attr::EM_DAMAGE, attr::THERMAL_DAMAGE, attr::KINETIC_DAMAGE, attr::EXPLOSIVE_DAMAGE].iter().any(|a| t.attr(*a).unwrap_or(0.0) > 0.0)).collect();
    let missing: Vec<&str> = damaging.iter().filter(|t| !reachable.contains(&t.id)).map(|t| t.name.as_str()).collect();
    println!("{} damaging charges, {} not on the market: {:?}", damaging.len(), missing.len(), &missing[..missing.len().min(12)]);
}

#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_incoming_damage_menu_is_one_level_per_family() {
    let d = std::sync::Arc::new(spai_fit::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap());
    let tree = crate::browser::Tree::build(&d);
    let families = crate::stats_panel::profile_families(&d, &tree);
    for (f, e) in &families {
        println!("{f}: {} entries, e.g. {:?}", e.len(), e.iter().take(8).map(|x| x.label.as_str()).collect::<Vec<_>>());
    }
    let labels: Vec<&str> = families.iter().flat_map(|(_, e)| e.iter().map(|x| x.label.as_str())).collect();
    for want in ["Berserker", "Hammerhead", "Ogre", "Hobgoblin", "Antimatter Charge", "EMP", "Scourge"] {
        assert!(labels.contains(&want), "{want} offered: {labels:?}");
    }
    for copy in ["Caldari Navy Antimatter Charge", "Republic Fleet EMP", "Caldari Navy Scourge"] {
        assert!(!labels.iter().any(|l| l.starts_with(copy)), "{copy} deals what its plain line does: folded in");
    }
}

/// Drags the edges of the dock and the left bar like a user, and reads back what is kept.
#[test]
fn resizing_the_dock_and_the_left_bar_is_kept() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    let ctx = egui::Context::default();
    let mut t = 0.0;
    let mut frame = |s: &mut SimUi, events: Vec<egui::Event>| {
        t += 0.05;
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(input, |ui| {
            s.show(ui);
            s.show_dialogs(&ui.ctx().clone(), 0);
        });
    };
    for _ in 0..3 {
        frame(&mut s, vec![]);
    }
    let rect = |id: &str| egui::PanelState::load(&ctx, egui::Id::new(id)).map(|p| p.rect).expect(id);
    let drag = |s: &mut SimUi, frame: &mut dyn FnMut(&mut SimUi, Vec<egui::Event>), from: egui::Pos2, by: egui::Vec2| {
        let b = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::default() };
        frame(s, vec![egui::Event::PointerMoved(from)]);
        frame(s, vec![b(from, true)]);
        for k in 1..=8 {
            frame(s, vec![egui::Event::PointerMoved(from + by * (k as f32 / 8.0))]);
        }
        frame(s, vec![b(from + by, false)]);
        frame(s, vec![]);
        frame(s, vec![]);
    };
    let dock = rect("sim_dock");
    drag(&mut s, &mut frame, egui::pos2(dock.center().x, dock.min.y), egui::vec2(0.0, -120.0));
    let left = rect("sim_left");
    drag(&mut s, &mut frame, egui::pos2(left.max.x, left.center().y), egui::vec2(90.0, 0.0));
    let (dock2, left2) = (rect("sim_dock"), rect("sim_left"));
    assert!(dock2.height() > dock.height() + 100.0, "the dock grew: {} to {}", dock.height(), dock2.height());
    assert!(left2.width() > left.width() + 70.0, "the left bar grew: {} to {}", left.width(), left2.width());
    let kept = s.take_requests().into_iter().filter_map(|r| if let HostRequest::SaveLayout(j) = r { Some(j) } else { None }).last().expect("the layout was handed over");
    let l: SimLayout = serde_json::from_str(&kept).unwrap();
    assert!((l.dock_h.unwrap() - dock2.height()).abs() < 1.0, "kept dock {:?} against {}", l.dock_h, dock2.height());
    assert!((l.left_w.unwrap() - left2.width()).abs() < 1.0, "kept left {:?} against {}", l.left_w, left2.width());
}

/// The plot keeps its place and size whatever the controls hold: another kind of damage graph,
/// another bottom, a dial turned, or a long list of ships.
#[test]
fn the_graph_keeps_its_size_whatever_the_controls() {
    use spai_fit::graphs::GraphKind;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    s.edit(EditKind::SetState, 2, |f| {
        let m = f.module_mut(Slot::High, 0).unwrap();
        m.state = State::Active;
        m.charge = Some(ids::AMMO);
    });
    s.graph_window = true;
    s.pane = crate::Pane::Graphs;
    for w in [1440.0f32, 760.0] {
        let ctx = egui::Context::default();
        let mut t = 0.0;
        let mut rects = Vec::new();
        for (i, kind) in [GraphKind::DamageVsDistance, GraphKind::DamageVsTargetSpeed, GraphKind::DamageVsTargetSignature, GraphKind::VolleyVsDistance].into_iter().enumerate() {
            s.graph = kind;
            s.graph_motion.target = (0.25 * i as f32, 30.0 * i as f32);
            for _ in 0..3 {
                t += 0.05;
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, 900.0))), time: Some(t), ..Default::default() };
                let _ = ctx.run_ui(input, |ui| s.show(ui));
            }
            rects.push(crate::plot::last_rect(&ctx).expect("a plot was drawn"));
        }
        assert!(rects.windows(2).all(|p| (p[0].min - p[1].min).length() < 0.5 && (p[0].size() - p[1].size()).length() < 0.5), "at {w}: {rects:?}");
    }
}

#[test]
fn the_motion_circles_and_the_angular_velocity_agree() {
    use crate::graphs_panel::Motion;
    let t = spai_fit::fit::TargetProfile { velocity: 1000.0, distance: 10_000.0, ..Default::default() };
    let mut m = Motion::default();
    assert!((f64::from(m.apply(&t, 500.0).transversal) - 1000.0).abs() < 1e-3, "still shooter, target across at full speed");
    m.shooter = (1.0, 90.0);
    assert!((f64::from(m.apply(&t, 500.0).transversal) - 500.0).abs() < 1e-3, "both across the same way: the difference");
    m.solve_target_share(&t, 500.0, 0.02);
    let back = f64::from(m.apply(&t, 500.0).transversal) / 10_000.0;
    assert!((back - 0.02).abs() < 1e-6, "the share found gives the angular asked for: {back}");
    m.target.1 = 0.0;
    assert!((f64::from(m.apply(&t, 500.0).transversal) - 500.0).abs() < 1e-3, "a target heading straight away adds nothing across");
}

#[test]
fn an_axis_end_set_by_hand_is_where_the_graph_stops() {
    use spai_fit::graphs::{GraphKind, GraphSpec, GraphX};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    s.edit(EditKind::SetState, 2, |f| {
        let m = f.module_mut(Slot::High, 0).unwrap();
        m.state = State::Active;
        m.charge = Some(ids::AMMO);
    });
    s.graph_x_to.insert(GraphX::Distance, 100_000.0);
    let c = s.computed().unwrap();
    let spec = GraphSpec { x_to: s.graph_x_to.get(&GraphKind::DamageVsDistance.axes().1).copied(), ..GraphSpec::new(GraphKind::DamageVsDistance, Default::default()) };
    let series = spai_fit::graphs::sample(&c, &spec);
    let last = series.iter().filter_map(|s| s.points.last()).map(|p| p[0]).fold(0.0, f64::max);
    assert!((last - 100_000.0).abs() < 1.0, "the bottom ends at 100 km: {last}");
}

#[test]
fn the_open_tabs_come_back_in_order_with_the_same_one_on_screen() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.new_fit(ids::SHIP, 1);
    s.new_fit(ids::SHIP, 2);
    let ids_open: Vec<String> = s.sessions().iter().map(|x| x.id.clone()).collect();
    s.handle(HostEvent::Fits(ids_open.iter().map(|id| FitSummary { id: id.clone(), name: "Rifter".into(), ship: ids::SHIP, updated_at: 0 }).collect()));
    s.activate(1);
    let docs: Vec<(String, String)> = s.sessions().iter().map(|x| (x.id.clone(), spai_fit::doc::to_json(&x.fit))).collect();
    let layout = serde_json::to_string(&s.layout()).unwrap();

    let mut back = sim();
    back.apply_layout(&layout);
    let asked: Vec<String> = back.take_requests().into_iter().filter_map(|r| if let HostRequest::LoadFit(id) = r { Some(id) } else { None }).collect();
    assert_eq!(asked, ids_open, "each tab asked for, in order");
    for (id, doc) in docs {
        back.handle(HostEvent::Fit { id, doc });
    }
    let order: Vec<String> = back.sessions().iter().map(|x| x.id.clone()).collect();
    assert_eq!(order, ids_open);
    assert_eq!(back.session().unwrap().id, ids_open[1], "the tab that was on screen is on screen");
}

#[test]
fn a_linked_fit_waits_for_the_data_and_then_opens() {
    let mut s = SimUi::default();
    let text = spai_fit::formats::link::decode(&spai_fit::formats::link::encode("[Rifter, From a link]\nDamage Control II\n")).unwrap();
    s.import_when_ready(text);
    assert!(s.sessions().is_empty(), "nothing without the data");
    s.handle(HostEvent::Dogma(std::sync::Arc::new(spai_fit::test_support::dataset())));
    assert_eq!(s.session().map(|x| x.fit.name.as_str()), Some("From a link"));
}

#[test]
fn several_fits_pasted_at_once_open_in_tabs() {
    let mut s = sim();
    assert!(s.import_text("[Rifter, One]\nDamage Control II\n\n[Rifter, Two]\n200mm AutoCannon II\n", 1));
    let names: Vec<&str> = s.sessions().iter().map(|x| x.fit.name.as_str()).collect();
    assert_eq!(names, vec!["One", "Two"]);
}

#[test]
fn a_copied_fit_keeps_its_variations_under_a_new_id() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::Other, 1, |f| f.variations.push(spai_fit::variations::Variation { id: 3, name: "Kite".into(), ..Default::default() }));
    let id = s.session().unwrap().id.clone();
    s.copy_fit(&id, 2);
    let copy = s.session().unwrap();
    assert_ne!(copy.id, id);
    assert!(copy.fit.name.ends_with("(copy)"), "{}", copy.fit.name);
    assert_eq!(copy.fit.variations.len(), 1);
    assert!(copy.dirty, "saved on its own as a new fit");
}

#[test]
fn a_hull_search_without_fits_offers_new_ones() {
    let s = sim();
    let found = crate::library_panel::hulls_matching_for_test(&s, "rif");
    assert!(found.iter().any(|(t, _)| *t == ids::SHIP), "{found:?}");
    assert!(crate::library_panel::hulls_matching_for_test(&s, "nothing like it").is_empty());
}

#[test]
fn rolled_items_are_not_offered_on_their_own() {
    let d = sim().dogma.clone().unwrap();
    assert!(d.rolled_types().contains(&ids::ABYSSAL_MWD));
    let tree = crate::browser::Tree::build(&d);
    fn all(tree: &crate::browser::Tree, g: spai_fit::ids::MarketGroupId, out: &mut Vec<u32>) {
        out.extend(tree.types(g).iter().copied());
        for &c in tree.children(g) {
            all(tree, c, out);
        }
    }
    let mut listed = Vec::new();
    for &r in tree.roots() {
        all(&tree, r, &mut listed);
    }
    assert!(listed.contains(&ids::MWD) && !listed.contains(&ids::ABYSSAL_MWD), "the base item is offered, its abyssal version is not");
}

#[test]
fn cpu_and_grid_show_decimals_only_when_close_to_the_limit() {
    use crate::stats_panel::resource_text;
    assert_eq!(resource_text(399.6, 400.0, true), ("399.60".to_owned(), "400.00".to_owned()), "within 1%");
    assert_eq!(resource_text(40.4, 40.0, true), ("40.40".to_owned(), "40.00".to_owned()), "within one unit");
    assert_eq!(resource_text(300.0, 400.0, true), (crate::text::num(300.0), crate::text::num(400.0)), "far from it");
    assert_eq!(resource_text(399.6, 400.0, false), (crate::text::num(399.6), crate::text::num(400.0)), "only where asked");
}

#[test]
fn another_ship_added_to_a_fit_rides_in_the_cargo() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    let hull = s.session().unwrap().fit.ship;
    assert!(crate::browser::quick_add(&mut s, ids::SHIP, 1));
    let f = &s.session().unwrap().fit;
    assert_eq!(f.ship, hull, "the hull stays");
    assert!(f.cargo.iter().any(|c| c.type_id == ids::SHIP && c.hold.is_none()), "{:?}", f.cargo);
    s.new_empty_tab(2);
    assert!(crate::browser::quick_add(&mut s, ids::SHIP, 3));
    assert_eq!(s.session().unwrap().fit.ship, ids::SHIP, "a tab without a hull takes it as the hull");
}

#[test]
fn a_module_state_set_by_hand_is_saved_and_comes_back() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::Mid, 0), ids::MWD, 1);
    s.edit(EditKind::SetState, 2, |f| f.module_mut(Slot::Mid, 0).unwrap().state = State::Online);
    s.autosave_for_test(100_000);
    let (id, doc) = s.take_requests().into_iter().find_map(|r| if let HostRequest::SaveFit { id, doc, .. } = r { Some((id, doc)) } else { None }).expect("saved");
    let mut back = sim();
    back.handle(HostEvent::Fit { id, doc });
    assert_eq!(back.session().unwrap().fit.module(Slot::Mid, 0).unwrap().state, State::Online, "the state as it was set, not the default");
}

#[test]
fn clicks_on_the_state_button_toggle_overload_and_offline() {
    use crate::editor::{gesture_state, Gesture};
    let d = sim().dogma.clone().unwrap();
    assert_eq!(gesture_state(&d, ids::MWD, State::Online, Gesture::Toggle), State::Active);
    assert_eq!(gesture_state(&d, ids::MWD, State::Active, Gesture::Toggle), State::Online);
    assert_eq!(gesture_state(&d, ids::MWD, State::Offline, Gesture::Toggle), State::Active, "from offline a click switches it on");
    assert_eq!(gesture_state(&d, ids::GYRO, State::Online, Gesture::Toggle), State::Offline, "a passive module never goes active");
    assert_eq!(gesture_state(&d, ids::MWD, State::Active, Gesture::Overload), State::Active, "no heat figure: no overload");
    assert_eq!(gesture_state(&d, ids::MWD, State::Active, Gesture::Offline), State::Offline);
    assert_eq!(gesture_state(&d, ids::MWD, State::Offline, Gesture::Offline), State::Online);
}

#[test]
fn no_more_than_five_drones_are_ever_out_across_stacks() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::Drones, 1, |f| {
        f.drones.push(DroneStack { type_id: ids::DRONE, count: 7, active: true, launched: None, mutation: None });
        f.drones.push(DroneStack { type_id: ids::DRONE + 1, count: 3, active: true, launched: None, mutation: None });
    });
    let out: Vec<u32> = s.session().unwrap().fit.drones.iter().map(|d| d.in_space()).collect();
    assert_eq!(out, vec![5, 0], "the first stack takes all five, the second waits in the bay");
    s.edit(EditKind::Drones, 2, |f| f.drones[0].launched = Some(2));
    s.edit(EditKind::Drones, 3, |f| {
        f.drones[1].active = true;
        f.drones[1].launched = None;
    });
    let out: Vec<u32> = s.session().unwrap().fit.drones.iter().map(|d| d.in_space()).collect();
    assert_eq!(out, vec![2, 3], "with two out, three of the other stack may go");
    assert_eq!(s.session().unwrap().fit.drones[0].count, 7, "the bay keeps all seven");
}

/// Drones launched with a variation on land in the variation, so the bay reads the fit as shown.
#[test]
fn drones_launched_with_a_variation_on_count_and_show() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::Drones, 1, |f| f.drones.push(DroneStack { type_id: ids::DRONE, count: 5, active: true, launched: None, mutation: None }));
    let five = s.computed().unwrap().drones.clone();
    assert_eq!(five.active, 5);
    s.start_recording(2);
    s.edit(EditKind::Drones, 3, |f| f.drones[0].launched = Some(2));
    s.keep_recording(4);
    assert_eq!(s.session().unwrap().fit.active_variations.len(), 1, "the launch is a variation");
    assert_eq!(s.session().unwrap().fit.drones[0].in_space(), 5, "the base keeps all five out");
    assert_eq!(s.view_fit().unwrap().drones[0].in_space(), 2, "the fit as shown has two");
    let two = s.computed().unwrap().drones.clone();
    assert_eq!(two.active, 2);
    assert!(close(two.bandwidth_used, five.bandwidth_used / 5.0 * 2.0), "{two:?}");
    s.edit(EditKind::Drones, 5, |f| f.drones[0].launched = Some(4));
    assert_eq!(s.view_fit().unwrap().drones[0].in_space(), 4);
    assert_eq!(s.computed().unwrap().drones.active, 4);
}

/// Past a group limit the editor puts the extra module offline, as the game would refuse it, and
/// the module the pilot just set is the one that stays.
#[test]
fn a_group_limit_puts_the_extra_module_offline_and_spares_the_one_just_set() {
    let mut d = dataset();
    d.types.iter_mut().find(|t| t.id == ids::HARDENER).unwrap().attrs.push((ids::MAX_GROUP_ACTIVE, 1.0));
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(Arc::new(d.finish())));
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::AddModule, 1, |f| {
        f.modules.push(spai_fit::fit::Module::new(ids::HARDENER, Slot::Mid, 0).with_state(State::Active));
        f.modules.push(spai_fit::fit::Module::new(ids::HARDENER, Slot::Mid, 1).with_state(State::Active));
    });
    let states = |s: &mut SimUi| {
        s.computed();
        (0..2).map(|i| s.session().unwrap().fit.module(Slot::Mid, i).unwrap().state).collect::<Vec<_>>()
    };
    assert_eq!(states(&mut s), vec![State::Active, State::Offline]);
    assert!(s.session().unwrap().warnings.iter().all(|w| !matches!(w.kind, spai_fit::warnings::WarningKind::OfflineModule { .. } | spai_fit::warnings::WarningKind::GroupLimit { .. })), "{:?}", s.session().unwrap().warnings);
    s.edit(EditKind::SetState, 2, |f| f.module_mut(Slot::Mid, 1).unwrap().state = State::Active);
    s.touch(Slot::Mid, 1);
    assert_eq!(states(&mut s), vec![State::Offline, State::Active], "the one just set stays, the other yields");
    s.undo();
    assert_eq!(states(&mut s), vec![State::Active, State::Offline]);
}

#[test]
fn a_group_limit_holds_through_a_variation_and_leaves_the_base_alone() {
    use spai_fit::variations::{RefitOp, Variation};
    let mut d = dataset();
    d.types.iter_mut().find(|t| t.id == ids::HARDENER).unwrap().attrs.push((ids::MAX_GROUP_ACTIVE, 1.0));
    let d = Arc::new(d.finish());
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(d.clone()));
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::AddModule, 1, |f| {
        f.modules.push(spai_fit::fit::Module::new(ids::HARDENER, Slot::Mid, 0).with_state(State::Active));
        f.modules.push(spai_fit::fit::Module::new(ids::HARDENER, Slot::Mid, 1).with_state(State::Online));
        f.variations.push(Variation { id: 7, name: "Both".into(), ops: vec![RefitOp::SetState { slot: Slot::Mid, index: 1, state: State::Active }], ..Default::default() });
    });
    let states = |f: &spai_fit::fit::Fit| (0..2).map(|i| f.module(Slot::Mid, i).unwrap().state).collect::<Vec<_>>();
    s.computed();
    assert_eq!(states(&s.session().unwrap().fit), vec![State::Active, State::Online], "within the limit as saved");
    s.edit(EditKind::Variation, 2, |f| spai_fit::variations::activate(f, 7));
    s.computed();
    let fit = s.session().unwrap().fit.clone();
    assert_eq!(states(&spai_fit::variations::resolve(&d, &fit)), vec![State::Active, State::Offline], "the variation's second running one yields");
    assert_eq!(states(&fit), vec![State::Active, State::Online], "the base fit is not what changed");
    assert!(s.session().unwrap().warnings.iter().all(|w| !matches!(w.kind, spai_fit::warnings::WarningKind::GroupLimit { .. })), "{:?}", s.session().unwrap().warnings);
    s.edit(EditKind::Variation, 3, |f| f.active_variations.clear());
    s.computed();
    assert_eq!(states(&s.session().unwrap().fit), vec![State::Active, State::Online]);
}

#[test]
fn a_fleet_page_switching_one_more_on_than_the_limit_allows_puts_another_off() {
    use crate::fleet_ships::page_states_after;
    let mut d = dataset();
    d.types.iter_mut().find(|t| t.id == ids::HARDENER).unwrap().attrs.push((ids::MAX_GROUP_ACTIVE, 1.0));
    let d = d.finish();
    let mut base = spai_fit::fit::Fit::new(ids::SHIP, "base");
    base.modules.push(spai_fit::fit::Module::new(ids::HARDENER, Slot::Mid, 0).with_state(State::Active));
    base.modules.push(spai_fit::fit::Module::new(ids::HARDENER, Slot::Mid, 1).with_state(State::Online));
    base.modules.push(spai_fit::fit::Module::new(ids::GUN, Slot::High, 0).with_state(State::Active));
    let now = base.clone();
    assert_eq!(page_states_after(&d, &base, &now, (Slot::High, 0, State::Offline)), vec![(Slot::High, 0, State::Offline)], "an unrelated switch changes that one alone");
    let after = page_states_after(&d, &base, &now, (Slot::Mid, 1, State::Active));
    assert_eq!(after, vec![(Slot::Mid, 0, State::Offline), (Slot::Mid, 1, State::Active)], "the one just switched on runs, the other yields");
    let mut now = base.clone();
    for (sl, i, st) in &after {
        now.module_mut(*sl, *i).unwrap().state = *st;
    }
    assert_eq!(page_states_after(&d, &base, &now, (Slot::Mid, 1, State::Online)), vec![(Slot::Mid, 0, State::Offline)], "switching it back down leaves the rest as set");
}

#[test]
fn the_charge_picker_serves_the_module_the_variation_put_in_the_slot() {
    use crate::picker::charge_module;
    use spai_fit::variations::{RefitOp, Variation};
    let d = Arc::new(dataset());
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(d.clone()));
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::AddModule, 1, |f| {
        f.modules.push(spai_fit::fit::Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO));
        f.variations.push(Variation { id: 7, name: "Boost".into(), ops: vec![RefitOp::Replace { slot: Slot::High, index: 0, with: Some(ids::BURST) }], ..Default::default() });
    });
    let target = PickTarget::Charge(Slot::High, 0);
    assert_eq!(charge_module(&s, target), Some(ids::GUN));
    assert!(results(&d, target, charge_module(&s, target), "").contains(&ids::AMMO));
    s.edit(EditKind::Variation, 2, |f| spai_fit::variations::activate(f, 7));
    assert_eq!(charge_module(&s, target), Some(ids::BURST), "the burst the variation fitted, not the gun under it");
    let offered = results(&d, target, charge_module(&s, target), "");
    assert!(offered.contains(&ids::BURST_CHARGE) && !offered.contains(&ids::AMMO), "{offered:?}");
}

#[test]
fn a_charge_from_the_browser_loads_the_module_the_variation_fitted_and_the_cost_counts_it() {
    use crate::browser::quick_add;
    use spai_fit::variations::{RefitOp, Variation};
    let d = Arc::new(dataset());
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(d.clone()));
    s.new_fit(ids::SHIP, 0);
    s.edit(EditKind::AddModule, 1, |f| {
        f.modules.push(spai_fit::fit::Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO));
        f.variations.push(Variation { id: 7, name: "Boost".into(), ops: vec![RefitOp::Replace { slot: Slot::High, index: 0, with: Some(ids::BURST) }], ..Default::default() });
    });
    s.edit(EditKind::Variation, 2, |f| spai_fit::variations::activate(f, 7));
    quick_add(&mut s, ids::BURST_CHARGE, 3);
    let shown = s.shown_fit().unwrap();
    assert_eq!(shown.module(Slot::High, 0).map(|m| (m.type_id, m.charge)), Some((ids::BURST, Some(ids::BURST_CHARGE))), "loaded into the burst the variation fitted");
    assert!(shown.cargo.is_empty(), "not dropped into the cargo for want of a taker: {:?}", shown.cargo);
    assert_eq!(s.session().unwrap().fit.module(Slot::High, 0).map(|m| (m.type_id, m.charge)), Some((ids::GUN, Some(ids::AMMO))), "the base keeps its gun and ammo");
    s.prices.insert(ids::BURST, 100.0);
    s.prices.insert(ids::GUN, 1.0);
    s.prices.insert(ids::SHIP, 0.0);
    s.prices.insert(ids::BURST_CHARGE, 0.0);
    s.prices.insert(ids::AMMO, 0.0);
    assert_eq!(s.cost().map(|c| c.modules), Some(100.0), "the cost is of the fit as shown");
}

#[test]
fn fit_tabs_fill_the_bar_and_the_one_on_screen_is_always_on_it() {
    use crate::editor::tabs_that_fit;
    let w = [100.0, 100.0, 100.0, 100.0, 100.0];
    assert_eq!(tabs_that_fit(&w, 320.0, 0), vec![0, 1, 2]);
    assert_eq!(tabs_that_fit(&w, 320.0, 4), vec![0, 1, 4], "the active one takes the last place");
    assert_eq!(tabs_that_fit(&w, 1000.0, 2), vec![0, 1, 2, 3, 4]);
    assert_eq!(tabs_that_fit(&w, 50.0, 3), vec![3], "even when nothing fits, the active one shows");
}

#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_consumables_cover_charges_paste_and_fuel() {
    let d = spai_fit::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let id = |n: &str| d.type_named(n).unwrap_or_else(|| panic!("{n}")).id;
    let mut f = spai_fit::fit::Fit::new(id("Revelation"), "x");
    f.modules.push(spai_fit::fit::Module::new(id("Siege Module II"), Slot::High, 0));
    f.cargo.push(spai_fit::fit::CargoItem { type_id: id("200mm AutoCannon II"), quantity: 1, hold: None });
    let c = crate::picker::consumables_for(&d, &f);
    let names: Vec<&str> = c.iter().filter_map(|t| d.type_(*t)).map(|t| t.name.as_str()).filter(|n| !n.contains(" S")).collect();
    println!("{} consumables, besides small ammo: {names:?}", c.len());
    for want in ["Nanite Repair Paste", "Strontium Clathrates", "Helium Isotopes", "EMP S"] {
        assert!(c.contains(&id(want)), "{want}");
    }
}

#[test]
fn the_fit_on_screen_can_be_taken_off_the_graph() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    s.edit(EditKind::SetState, 2, |f| {
        let m = f.module_mut(Slot::High, 0).unwrap();
        m.state = State::Active;
        m.charge = Some(ids::AMMO);
    });
    s.new_fit(ids::SHIP, 3);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 4);
    s.edit(EditKind::SetState, 5, |f| {
        let m = f.module_mut(Slot::High, 0).unwrap();
        m.state = State::Active;
        m.charge = Some(ids::AMMO);
    });
    let other = s.sessions()[0].id.clone();
    s.graph_attackers = vec![other];
    s.graph_current = false;
    s.graph_window = true;
    s.pane = crate::Pane::Graphs;
    let ctx = egui::Context::default();
    let mut legend = 0;
    for t in 0..3 {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), time: Some(f64::from(t) * 0.05), ..Default::default() };
        let _ = ctx.run_ui(input, |ui| s.show(ui));
        legend = crate::graphs_panel::last_line_count(&ctx);
    }
    assert_eq!(legend, 1, "only the other fit is drawn");
}

#[test]
fn the_effective_graphs_shoot_through_the_test_target_or_the_target_fit() {
    use spai_fit::graphs::{GraphKind, GraphSpec, SeriesLabel};
    let mut s = sim();
    for t in [1, 3] {
        s.new_fit(ids::SHIP, t);
        apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, t + 1);
        s.edit(EditKind::SetState, t + 1, |f| {
            let m = f.module_mut(Slot::High, 0).unwrap();
            m.state = State::Active;
            m.charge = Some(ids::AMMO);
        });
    }
    s.edit(EditKind::Target, 9, |f| f.target.resist = [0.5; 4]);
    s.graph = GraphKind::EffectiveDamageVsDistance;
    s.graph_window = true;
    s.pane = crate::Pane::Graphs;
    let ctx = egui::Context::default();
    let frames = |s: &mut SimUi, w: f32| {
        for t in 0..3 {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, 900.0))), time: Some(f64::from(t) * 0.05), ..Default::default() };
            let _ = ctx.run_ui(input, |ui| s.show(ui));
        }
    };
    for w in [1440.0, 760.0] {
        frames(&mut s, w);
        assert!(crate::plot::last_rect(&ctx).is_some(), "the fit alone against the test target draws at {w}");
    }
    // The resistances set on the test target halve the fit's line.
    let c = s.computed().unwrap();
    let target = s.session().unwrap().fit.target.clone();
    let at_zero = |kind| spai_fit::graphs::sample(&c, &GraphSpec::new(kind, target.clone()))[0].points[0][1];
    assert!((at_zero(GraphKind::EffectiveDamageVsDistance) * 2.0 - at_zero(GraphKind::DamageVsDistance)).abs() < 1e-9);
    // The other tab as a target brings its own tank and stands beside the test target.
    let other = s.sessions()[0].id.clone();
    s.graph_targets = vec![other];
    frames(&mut s, 1440.0);
    assert_eq!(crate::graphs_panel::last_line_count(&ctx), 2, "one line per target");
    s.graph_ideal = false;
    frames(&mut s, 1440.0);
    assert_eq!(crate::graphs_panel::last_line_count(&ctx), 1);
    let series = spai_fit::graphs::sample(&c, &GraphSpec::new(GraphKind::EffectiveDamageOverTime, target.clone()));
    assert!(series.iter().any(|x| x.label != SeriesLabel::Total), "over time, the weapons draw by name as the other damage graphs do");
}

#[test]
fn a_pinned_attacker_stays_whichever_tab_is_on_screen() {
    let mut s = sim();
    for t in [1, 3] {
        s.new_fit(ids::SHIP, t);
        apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, t + 1);
        s.edit(EditKind::SetState, t + 1, |f| {
            let m = f.module_mut(Slot::High, 0).unwrap();
            m.state = State::Active;
            m.charge = Some(ids::AMMO);
        });
    }
    // The second tab pinned while on screen, beside the fit on screen.
    let pinned = s.sessions()[1].id.clone();
    s.graph_attackers = vec![pinned.clone()];
    let ctx = egui::Context::default();
    let lines = |s: &mut SimUi| {
        s.graph_window = true;
        s.pane = crate::Pane::Graphs;
        let mut n = 0;
        for t in 0..3 {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), time: Some(f64::from(t) * 0.05), ..Default::default() };
            let _ = ctx.run_ui(input, |ui| s.show(ui));
            n = crate::graphs_panel::last_line_count(&ctx);
        }
        n
    };
    assert_eq!(lines(&mut s), 2, "the pinned copy and the fit on screen, though they are one tab");
    s.activate(0);
    assert_eq!(lines(&mut s), 2);
    assert_eq!(s.graph_attackers, vec![pinned], "switching tabs neither drops nor adds attackers");
}

#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_what_the_dialogs_leave_out() {
    let d = std::sync::Arc::new(spai_fit::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap());
    let rolled = d.rolled_types();
    for n in ["Berserker I", "Berserker II", "Hammerhead I", "Warrior I", "EMP S", "Scourge Heavy Missile", "Antimatter Charge M"] {
        let t = d.type_named(n);
        println!("{n}: present {} published {:?} market {:?} meta {:?} rolled {}", t.is_some(), t.map(|t| t.published), t.and_then(|t| t.market_group), t.and_then(|t| t.meta_group), t.is_some_and(|t| rolled.contains(&t.id)));
    }
    let published_drones = d.types.iter().filter(|t| t.published && t.category == spai_fit::ids::category::DRONE).count();
    let rolled_drones = d.types.iter().filter(|t| t.published && t.category == spai_fit::ids::category::DRONE && rolled.contains(&t.id)).count();
    let rolled_charges = d.types.iter().filter(|t| t.published && t.category == spai_fit::ids::category::CHARGE && rolled.contains(&t.id)).count();
    println!("drones {published_drones}, rolled away {rolled_drones}; charges rolled away {rolled_charges}; rolled set {}", rolled.len());
    let all = crate::picker::results(&d, crate::picker::PickTarget::Drone, None, "");
    let b = d.type_named("Berserker I").unwrap().id;
    println!("drone picker offers {} types, Berserker I {}", all.len(), all.contains(&b));
    let tree = crate::browser::Tree::build(&d);
    let mut g = d.type_(b).unwrap().market_group;
    while let Some(id) = g {
        println!("  group {id} {} kept-types {} children {}", tree.name(id), tree.types(id).len(), tree.children(id).len());
        g = d.market_group(id).and_then(|m| m.parent);
    }
    println!("in its group's list: {}", tree.types(d.type_(b).unwrap().market_group.unwrap()).contains(&b));
}

#[test]
fn a_fleet_autosaves_once_its_edits_rest_and_an_edit_after_sending_stays_unsaved() {
    let mut s = sim();
    s.new_fleet(10);
    s.take_requests();
    let saves = |s: &mut SimUi| s.take_requests().into_iter().filter(|r| matches!(r, HostRequest::SaveFleet { .. })).count();
    s.fleet.as_mut().unwrap().fleet.size = 80;
    s.autosave_for_test(1_000);
    assert_eq!(saves(&mut s), 0, "still being edited");
    s.autosave_for_test(1_000 + crate::AUTOSAVE_MS);
    assert_eq!(saves(&mut s), 1, "saved once the edits rest");
    s.autosave_for_test(10_000);
    assert_eq!(saves(&mut s), 0, "not sent twice");
    let id = s.fleet.as_ref().unwrap().id.clone();
    s.fleet.as_mut().unwrap().fleet.size = 90;
    s.handle(HostEvent::FleetSaved(id.clone()));
    assert!(s.fleet.as_ref().unwrap().dirty, "edited after it was sent: still to save");
    s.autosave_for_test(20_000);
    s.autosave_for_test(20_000 + crate::AUTOSAVE_MS);
    assert_eq!(saves(&mut s), 1);
    s.handle(HostEvent::FleetSaved(id));
    assert!(!s.fleet.as_ref().unwrap().dirty);
}

#[test]
fn charge_names_shorten_to_what_tells_them_apart() {
    use crate::fleet_panel::short_charge;
    assert_eq!(short_charge("Antimatter Charge L"), "Antimatter");
    assert_eq!(short_charge("Caldari Navy Antimatter Charge L"), "CN Antimatter");
    assert_eq!(short_charge("Scourge Heavy Missile"), "Scourge Heavy");
    assert_eq!(short_charge("Hail M"), "Hail");
    assert_eq!(short_charge("Republic Fleet EMP XL"), "RF EMP");
    assert_eq!(short_charge("Multifrequency L"), "Multifrequency");
}

#[test]
fn fits_now_leaves_out_the_module_being_swapped() {
    use crate::picker::{fits, FitFilter};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    for i in 0..2 {
        apply(&mut s, PickTarget::Module(Slot::High, i), ids::GUN, 1 + u64::from(i));
    }
    let d = dataset();
    let gun = d.type_(ids::GUN).unwrap();
    let full = s.fit_room(Slot::High, 2).unwrap();
    assert!(!fits(&d, gun, &full, FitFilter::Left), "a third gun overloads the powergrid: {full:?}");
    let swap = s.fit_room(Slot::High, 1).unwrap();
    assert_eq!((swap.turrets_free, swap.cpu_left, swap.power_left), (full.turrets_free + 1, full.cpu_left + 25.0, full.power_left + 15.0), "the gun there gives back its hardpoint, CPU and powergrid");
    assert!(fits(&d, gun, &swap, FitFilter::Left));
}

/// The gun with a faction and an officer variant beside it.
fn gun_family() -> spai_fit::Dogma {
    let mut d = dataset();
    let gun = d.type_(ids::GUN).unwrap().clone();
    let set = |t: &mut spai_fit::dogma::TypeDef, a: spai_fit::ids::AttrId, v: f32| match t.attrs.iter_mut().find(|(x, _)| *x == a) {
        Some(e) => e.1 = v,
        None => t.attrs.push((a, v)),
    };
    let mut faction = spai_fit::dogma::TypeDef { id: 90_001, name: "Republic Fleet 200mm AutoCannon".into(), variation_parent: Some(ids::GUN), meta_group: Some(4), meta_level: Some(8), ..gun.clone() };
    set(&mut faction, spai_fit::dogma::attr::CPU, 20.0);
    let mut officer = spai_fit::dogma::TypeDef { id: 90_002, name: "Officer 200mm AutoCannon".into(), variation_parent: Some(ids::GUN), meta_group: Some(5), meta_level: Some(14), ..gun.clone() };
    set(&mut officer, spai_fit::dogma::attr::CPU, 18.0);
    d.types.push(faction);
    d.types.push(officer);
    d.finish()
}

#[test]
fn compare_variants_brings_the_whole_family_and_the_filters_narrow_it() {
    use crate::compare::{columns, variants, visible, CompareWindow};
    let d = gun_family();
    let family = variants(&d, 90_002);
    assert_eq!(family, vec![ids::GUN, 90_001, 90_002], "from any member, its Tech I parent first");
    assert_eq!(variants(&d, ids::WEB), vec![ids::WEB], "an item without variants compares alone");
    let mut w = CompareWindow { items: family.clone(), ..Default::default() };
    assert_eq!(visible(&d, &w), family, "by meta level when unsorted");
    let cols = columns(&d, &family, true, true);
    assert_eq!(cols, vec![spai_fit::dogma::attr::CPU], "only what differs: {cols:?}");
    assert!(columns(&d, &family, true, false).len() > 5);
    w.sort = Some((spai_fit::dogma::attr::CPU, false));
    assert_eq!(visible(&d, &w), vec![90_002, 90_001, ids::GUN], "lowest CPU first");
    w.hidden_meta.insert(Some(5));
    assert_eq!(visible(&d, &w), vec![90_001, ids::GUN], "the officer one hidden");
    w.hidden_meta.clear();
    w.meta_levels = (5, 10);
    assert_eq!(visible(&d, &w), vec![90_001], "meta 5 to 10");
}

#[test]
fn compare_menus_open_the_window_and_add_without_repeats() {
    let ctx = egui::Context::default();
    let mut s = sim();
    crate::compare::request(&ctx, &[ids::GUN, ids::WEB]);
    let _ = ctx.run_ui(Default::default(), |ui| crate::compare::window(&mut s, ui.ctx()));
    assert_eq!(s.compare.as_ref().map(|w| w.items.clone()), Some(vec![ids::GUN, ids::WEB]), "unlike items side by side");
    crate::compare::request(&ctx, &[ids::WEB, ids::DC]);
    let _ = ctx.run_ui(Default::default(), |ui| crate::compare::window(&mut s, ui.ctx()));
    assert_eq!(s.compare.as_ref().map(|w| w.items.clone()), Some(vec![ids::GUN, ids::WEB, ids::DC]));
}

#[test]
fn a_double_click_on_an_empty_slot_opens_the_picker() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    let ctx = egui::Context::default();
    let screen = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0)));
    let frame = |s: &mut SimUi, t: f64, events: Vec<egui::Event>| {
        let input = egui::RawInput { screen_rect: screen, time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(input, |ui| s.show(ui));
    };
    frame(&mut s, 0.0, vec![]);
    frame(&mut s, 0.05, vec![]);
    let log = ctx.data(|d| d.get_temp::<crate::rows::RowLog>(egui::Id::new("sim_row_log"))).unwrap_or_default();
    let rect = log.0.iter().find(|(l, _)| *l == egui::Id::new("rack/mid")).map(|(_, r)| *r).expect("an empty mid slot");
    let at = rect.center();
    let press = |down| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    frame(&mut s, 0.1, vec![egui::Event::PointerMoved(at)]);
    frame(&mut s, 0.15, vec![press(true)]);
    frame(&mut s, 0.2, vec![press(false)]);
    frame(&mut s, 0.25, vec![press(true)]);
    frame(&mut s, 0.3, vec![press(false)]);
    frame(&mut s, 0.35, vec![]);
    assert!(s.picker.is_some(), "the module picker opens");
}


#[test]
fn the_graphs_open_in_a_window_dock_beside_the_fit_and_come_back_where_they_were() {
    let mut s = sim();
    assert!(!s.graphs_shown());
    s.toggle_graphs();
    assert!(s.graph_window && !s.graph_docked, "first in a window");
    s.dock_graphs(true);
    assert!(s.graph_docked && !s.graph_window);
    s.graph_collapsed = true;
    s.toggle_graphs();
    assert!(!s.graphs_shown(), "closed");
    s.toggle_graphs();
    assert!(s.graph_docked && !s.graph_collapsed, "back in the dock, open");
    let json = serde_json::to_string(&s.layout()).unwrap();
    let mut t = sim();
    t.apply_layout(&json);
    assert!(t.graph_docked, "the dock is kept between starts");
    s.dock_graphs(false);
    assert!(s.graph_window && !s.graph_docked);
    s.toggle_graphs();
    s.toggle_graphs();
    assert!(s.graph_window, "a window again once floated");
}


fn drag_window(s: &mut SimUi, window: &str, delta: egui::Vec2) -> (egui::Rect, egui::Rect) {
    let ctx = egui::Context::default();
    let screen = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0)));
    let mut t = 0.0;
    let mut frame = |s: &mut SimUi, events: Vec<egui::Event>| {
        t += 0.05;
        let input = egui::RawInput { screen_rect: screen, time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(input, |ui| s.show(ui));
    };
    for _ in 0..3 {
        frame(s, vec![]);
    }
    let rect = |ctx: &egui::Context| ctx.memory(|m| m.area_rect(egui::Id::new(window))).unwrap();
    let before = rect(&ctx);
    let corner = before.max - egui::vec2(2.0, 2.0);
    let to = corner + delta;
    let press = |p, down| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    frame(s, vec![egui::Event::PointerMoved(corner)]);
    frame(s, vec![press(corner, true)]);
    for k in 1..=10 {
        frame(s, vec![egui::Event::PointerMoved(corner + delta * (k as f32 / 10.0))]);
    }
    frame(s, vec![press(to, false)]);
    for _ in 0..3 {
        frame(s, vec![]);
    }
    (before, rect(&ctx))
}

/// Drags from `from` by `delta` over frames of the simulator at 1600 by 1000.
fn drag(s: &mut SimUi, ctx: &egui::Context, from: egui::Pos2, delta: egui::Vec2) {
    let screen = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0)));
    let mut t = ctx.input(|i| i.time) + 0.05;
    let mut frame = |s: &mut SimUi, events: Vec<egui::Event>| {
        t += 0.05;
        let input = egui::RawInput { screen_rect: screen, time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(input, |ui| s.show(ui));
    };
    let press = |p, down| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    frame(s, vec![egui::Event::PointerMoved(from)]);
    frame(s, vec![press(from, true)]);
    for k in 1..=10 {
        frame(s, vec![egui::Event::PointerMoved(from + delta * (k as f32 / 10.0))]);
    }
    frame(s, vec![press(from + delta, false)]);
    for _ in 0..3 {
        frame(s, vec![egui::Event::PointerMoved(egui::pos2(5.0, 5.0))]);
    }
}

fn settle(s: &mut SimUi, ctx: &egui::Context) {
    let screen = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0)));
    for k in 0..3 {
        let input = egui::RawInput { screen_rect: screen, time: Some(f64::from(k) * 0.05), ..Default::default() };
        let _ = ctx.run_ui(input, |ui| s.show(ui));
    }
}

#[test]
fn the_graph_windows_and_docks_follow_a_drag_both_ways() {
    let wider = |(a, b): (egui::Rect, egui::Rect)| (b.width() - a.width(), b.height() - a.height());
    for fleets in [false, true] {
        let open = |s: &mut SimUi| {
            if fleets {
                s.new_fleet(0);
                s.mode = Mode::Fleets;
                s.fleet_graph_open = true;
                s.fleet_graph_place = crate::GraphPlace::Window;
            } else {
                s.new_fit(ids::SHIP, 0);
                s.graph_window = true;
            }
        };
        let id = if fleets { "sim_fleet_graph_window" } else { "sim_graph_window" };
        let mut s = sim();
        open(&mut s);
        let (w, h) = wider(drag_window(&mut s, id, egui::vec2(150.0, 80.0)));
        assert!((w - 150.0).abs() < 2.0 && (h - 80.0).abs() < 2.0, "{id} grows: {w} {h}");
        let mut s = sim();
        open(&mut s);
        let (w, h) = wider(drag_window(&mut s, id, egui::vec2(-200.0, -150.0)));
        assert!((w + 200.0).abs() < 2.0 && (h + 150.0).abs() < 2.0, "{id} shrinks: {w} {h}");
    }
    // A dock's inner edge, dragged in and out.
    let check = |s: &mut SimUi, id: egui::Id, side: crate::DockSide| {
        let ctx = egui::Context::default();
        settle(s, &ctx);
        for d in [-120.0f32, 90.0] {
            let r = egui::PanelState::load(&ctx, id).unwrap().rect;
            let (from, delta, along): (egui::Pos2, egui::Vec2, fn(egui::Rect) -> f32) = match side {
                crate::DockSide::Right => (egui::pos2(r.min.x, r.center().y), egui::vec2(d, 0.0), |r| r.width()),
                crate::DockSide::Left => (egui::pos2(r.max.x, r.center().y), egui::vec2(-d, 0.0), |r| r.width()),
                crate::DockSide::Bottom => (egui::pos2(r.center().x, r.min.y), egui::vec2(0.0, d), |r| r.height()),
            };
            drag(s, &ctx, from, delta);
            let after = egui::PanelState::load(&ctx, id).unwrap().rect;
            assert!((along(after) - along(r) + d).abs() < 2.0, "{side:?} dock by {d}: {r:?} -> {after:?}");
        }
    };
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.dock_graphs(true);
    check(&mut s, egui::Id::new("sim_graph_dock"), crate::DockSide::Right);
    for (place, side) in [(crate::GraphPlace::Bottom, crate::DockSide::Bottom), (crate::GraphPlace::Right, crate::DockSide::Right), (crate::GraphPlace::Left, crate::DockSide::Left)] {
        let mut s = sim();
        s.new_fleet(0);
        s.mode = Mode::Fleets;
        s.fleet_graph_open = true;
        s.fleet_graph_place = place;
        check(&mut s, egui::Id::new(("sim_fleet_graph_dock", side as u8)), side);
    }
}


#[test]
fn a_changed_fleet_is_analysed_off_the_frame_and_the_result_comes_in() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id: fit_id, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let fit_id = fit_id.clone();
    s.handle(HostEvent::Saved(fit_id.clone()));
    s.new_fleet(10);
    let f = s.fleet.as_mut().unwrap();
    f.fleet.size = 20;
    f.fleet.slots.push(FleetSlot { fit_id, rule: CountRule::Fill { min: 0, max: 100 }, ..FleetSlot::default() });
    f.enemy = Some(crate::fleet_panel::MIRROR.to_owned());
    // The first one too is worked out off the frame: nothing on screen but its progress.
    assert!(s.fleet_analysis().is_none() && s.fleet_analysis_progress().is_some(), "the frame is not held up for the first one either");
    let first = s.fleet_analysis_settled().expect("the first analysis comes in");
    assert!(s.fleet_analysis_progress().is_none(), "no run left once it is in");
    // A change: the old one is shown while the new one is worked out on its own thread.
    s.fleet.as_mut().unwrap().fleet.slots[0].charges = vec![(ids::GUN, ids::AMMO_LONG)];
    s.now_ms = 0;
    assert!(Arc::ptr_eq(&s.fleet_analysis().unwrap(), &first), "while the knobs rest, the last one");
    s.now_ms = 10_000;
    let shown = s.fleet_analysis().unwrap();
    assert!(Arc::ptr_eq(&shown, &first) && s.fleet_analysis_pending(), "the run is under way, the frame not held up");
    let mut fresh = None;
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let a = s.fleet_analysis().unwrap();
        if !Arc::ptr_eq(&a, &first) {
            fresh = Some(a);
            break;
        }
    }
    let fresh = fresh.expect("the new analysis comes in");
    assert!(!s.fleet_analysis_pending());
    let gun = fresh.mine.slots[0].computed.weapons.iter().find(|w| w.type_id == ids::GUN).unwrap();
    assert_eq!(gun.charge, Some(ids::AMMO_LONG), "it is of the changed fleet");
}







#[test]
fn a_subsystem_lists_the_slots_it_gives() {
    let d = spai_fit::test_support::dataset();
    assert_eq!(crate::picker::slots_given(&d, ids::SUBSYSTEM), [2.0, 0.0, 0.0]);
    assert_eq!(crate::picker::slots_given(&d, ids::GUN), [0.0; 3], "a module gives none");
}

fn supplied_doctrine(id: &str, alliance: &str, category: &str, name: &str) -> spai_fit::doctrine::Doctrine {
    use spai_fit::doctrine::{supplied_fit_id, Doctrine, DoctrineFit, FitRole};
    let mut dps = Fit::new(ids::SHIP, "DPS");
    dps.add_module(ids::GUN, Slot::High);
    dps.add_module(ids::MWD, Slot::Mid);
    dps.variations.push(spai_fit::variations::Variation { id: 1, name: "Afterburner".into(), kind: spai_fit::variations::VariationKind::Refit, refit_key: Some("prop:ab".into()), ops: vec![], group: None });
    let tackle = Fit::new(ids::SHIP, "Tackle");
    let fits = vec![
        DoctrineFit { fit_id: supplied_fit_id(id, "tackle"), fit: Some(tackle), role: FitRole::Tackle, main: false, note: String::new() },
        DoctrineFit { fit_id: supplied_fit_id(id, "dps"), fit: Some(dps), role: FitRole::Line, main: true, note: "most of the fleet".into() },
    ];
    Doctrine { id: id.into(), alliance: alliance.into(), category: category.into(), name: name.into(), tags: vec![category.into()], url: Some("https://forum.example/t/1".into()), updated: Some(1_759_600_000), fits, ..Doctrine::default() }
}

fn supplied_sim() -> SimUi {
    let mut s = sim();
    let list = vec![
        SuppliedDoctrine { side: DoctrineSide::Imperium, rev: 1, doctrine: supplied_doctrine("gsf-2", "Goonswarm Federation", "Peacetime", "Kestrels") },
        SuppliedDoctrine { side: DoctrineSide::Imperium, rev: 1, doctrine: supplied_doctrine("gsf-1", "Goonswarm Federation", "Strategic", "Rifters") },
        SuppliedDoctrine { side: DoctrineSide::Enemy, rev: 1, doctrine: supplied_doctrine("horde-1", "Pandemic Horde", "Strategic", "Horde Rifters") },
    ];
    s.handle(HostEvent::Supplied(list));
    s
}

/// Adding a ship offers every fit, supplied ones too, and the doctrine the fleet already flies
/// first; a fleet of no doctrine's fits has none to put first.
#[test]
fn adding_a_ship_puts_the_fleets_own_doctrine_first() {
    let mut s = supplied_sim();
    let rifters = s.supplied.iter().find(|x| x.doctrine.name == "Rifters").unwrap().doctrine.clone();
    s.new_fleet(1);
    let all = crate::fleet_panel::fleet_fit_entries(&s);
    assert!(rifters.fits.iter().all(|f| all.iter().any(|e| e.id == f.fit_id)), "the supplied fits are offered");
    assert!(crate::fleet_panel::fleet_doctrines(&s, &all).is_empty(), "an empty fleet flies no doctrine");
    s.fleet.as_mut().unwrap().fleet.slots.push(spai_fit::fleet::FleetSlot { fit_id: rifters.fits[0].fit_id.clone(), ..Default::default() });
    let first = crate::fleet_panel::fleet_doctrines(&s, &all);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0, "Rifters");
    assert_eq!(first[0].1.len(), rifters.fits.iter().filter(|f| f.fit.is_some()).count(), "all of its fits");
}

#[test]
fn the_server_list_reads_into_supplied_doctrines_and_their_fits_reach_the_fleet_screen() {
    let d = supplied_doctrine("gsf-1", "Goonswarm Federation", "Strategic", "Rifters");
    let body = serde_json::json!([{"id": "gsf-1", "side": "imperium", "rev": 3, "updated_at": 5, "doc": d}, {"id": "bad", "side": "imperium", "doc": {"fits": "nope"}}]);
    let list = SuppliedDoctrine::parse_list(body.to_string().as_bytes());
    assert_eq!(list.len(), 1, "an entry this version cannot read is left out");
    assert_eq!((list[0].side, list[0].rev, list[0].doctrine.name.as_str()), (DoctrineSide::Imperium, 3, "Rifters"));
    let mut s = sim();
    s.handle(HostEvent::Supplied(list));
    assert!(s.fit_docs.contains_key("gsf-1~dps"), "inline fits are looked up like library fits");
    assert!(s.doctrine_fit_names().iter().any(|(id, name)| id == "gsf-1~dps" && name.contains("Rifters")));
}

#[test]
fn the_tree_nests_alliance_kind_doctrine_and_fits_with_the_mainline_first() {
    use crate::doctrine_panel::{lines_for_test, shown, Line, Source};
    let s = supplied_sim();
    let all = shown(&s);
    let none = std::collections::BTreeSet::new();
    let titles = |lines: &[Line]| -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Alliance { title, .. } => format!("A {title}"),
                Line::Category { title, .. } => format!("C {title}"),
                Line::Doctrine { at, .. } => format!("D {}", all[*at].doctrine.name),
                Line::Info { .. } => "I".to_owned(),
                Line::Fit { at, fit } => format!("F {}", all[*at].doctrine.fits[*fit].fit.as_ref().unwrap().name),
            })
            .collect()
    };
    let closed = lines_for_test(&all, Source::All, "", &none, &none);
    assert_eq!(titles(&closed), vec!["A Goonswarm Federation", "C Strategic", "D Rifters", "C Peacetime", "D Kestrels", "A Pandemic Horde", "C Strategic", "D Horde Rifters"], "strategic before peacetime, doctrines shut");
    let opened: std::collections::BTreeSet<String> = ["d:gsf-1".to_owned()].into();
    let open = lines_for_test(&all, Source::All, "", &none, &opened);
    assert_eq!(&titles(&open)[2..6], &["D Rifters", "I", "F DPS", "F Tackle"], "when it changed, then the mainline first");
    let folded: std::collections::BTreeSet<String> = ["a:goonswarm federation".to_owned()].into();
    assert_eq!(titles(&lines_for_test(&all, Source::All, "", &folded, &none))[..2], ["A Goonswarm Federation", "A Pandemic Horde"]);
    let found = lines_for_test(&all, Source::All, "tackle horde", &none, &none);
    assert_eq!(titles(&found), vec!["A Pandemic Horde", "C Strategic", "D Horde Rifters", "I", "F Tackle"], "a search opens what it finds and shows only the fits it names");
    assert!(lines_for_test(&all, Source::Mine, "", &none, &none).is_empty());
}

#[test]
fn a_copy_and_a_link_land_in_my_doctrines_and_a_link_follows_the_supplied_one() {
    use crate::doctrine_panel::{lines_for_test, shown, Kind, Line, Source};
    let mut s = supplied_sim();
    let rifters = s.supplied.iter().find(|d| d.doctrine.id == "gsf-1").unwrap().doctrine.clone();
    s.copy_doctrine(&rifters, 10);
    let reqs = s.take_requests();
    let fits_saved = reqs.iter().filter(|r| matches!(r, HostRequest::SaveFit { .. })).count();
    assert_eq!(fits_saved, 2, "the fits go into the library");
    let copy = s.own_doctrines[0].clone();
    assert!(reqs.iter().any(|r| matches!(r, HostRequest::SaveOwnDoctrine { id, .. } if *id == copy.id)));
    assert_eq!(copy.origin.as_deref(), Some("gsf-1"));
    assert!(copy.fits.iter().all(|f| f.fit.is_none() && s.fit_docs.contains_key(&f.fit_id)));
    assert_eq!(s.supplied.iter().find(|d| d.doctrine.id == "gsf-1").unwrap().doctrine, rifters, "the original stays as it is");

    s.link_doctrine(&rifters, 11);
    s.link_doctrine(&rifters, 12);
    assert_eq!(s.own_doctrines.iter().filter(|d| d.link).count(), 1, "one link per supplied doctrine");
    let mut changed = s.supplied.clone();
    changed[1].doctrine.fits.pop();
    s.handle(HostEvent::Supplied(changed));
    let all = shown(&s);
    let link = all.iter().find(|x| matches!(x.kind, Kind::Link(_))).unwrap();
    assert_eq!(link.kind, Kind::Link(true));
    assert_eq!(link.doctrine.fits.len(), 1, "a link shows the supplied doctrine as it is now");
    s.handle(HostEvent::Supplied(Vec::new()));
    let all = shown(&s);
    assert!(all.iter().any(|x| x.kind == Kind::Link(false)), "signed out, the link stays and says so");
    let none = std::collections::BTreeSet::new();
    let mine = lines_for_test(&all, Source::Mine, "", &none, &none);
    assert!(matches!(&mine[0], Line::Alliance { title, .. } if title == "Goonswarm Federation"));
    assert_eq!(mine.iter().filter(|l| matches!(l, Line::Doctrine { .. })).count(), 2);
}

#[test]
fn a_doctrine_opens_as_a_fleet_with_its_mainline_and_swaps_refits_together() {
    let mut s = supplied_sim();
    let rifters = s.supplied.iter().find(|d| d.doctrine.id == "gsf-1").unwrap().doctrine.clone();
    s.new_fleet_of(&rifters, 20);
    assert_eq!(s.mode, Mode::Fleets);
    let f = &s.fleet.as_ref().unwrap().fleet;
    assert_eq!(f.slots[0].fit_id, "gsf-1~dps");
    assert_eq!(f.main, Some(0));
    let docs = s.fit_docs.clone();
    let mut fleet = f.clone();
    spai_fit::doctrine::swap_refit(&mut fleet, Some("prop:ab"), &|id| docs.get(id).cloned());
    assert_eq!(fleet.slots[0].variation, Some(1));
    s.mode = Mode::Fits;
    s.new_fleet_from_doctrine("Horde Rifters", 21);
    assert_eq!(s.fleet.as_ref().unwrap().fleet.slots[0].fit_id, "horde-1~dps", "the fleet list's doctrine menu builds supplied ones the same way");
}

#[test]
fn an_own_doctrine_edits_its_fits_and_an_admin_publishes_it_over_the_original() {
    let mut s = supplied_sim();
    s.handle(HostEvent::Access { signed_in: true, imperium: true, admin: true });
    s.new_fit(ids::SHIP, 30);
    let fit_id = s.session().unwrap().id.clone();
    s.handle(HostEvent::Fits(vec![FitSummary { id: fit_id.clone(), name: "Rifter".into(), ship: ids::SHIP, updated_at: 0 }]));
    s.new_own_doctrine(31);
    let edit = s.doctrine_tab.edit.as_ref().unwrap();
    assert_eq!(edit.doctrine.fits.len(), 1, "the open saved fit starts the doctrine");
    assert!(edit.doctrine.fits[0].main);
    let mut d = edit.doctrine.clone();
    d.origin = Some("gsf-1".into());
    s.fit_docs.insert(fit_id.clone(), s.session().unwrap().fit.clone());
    let out = s.supplied_from_own(&d).unwrap();
    assert_eq!(out.id, "gsf-1", "publishing a copy replaces the doctrine it came from");
    assert!(out.fits[0].fit.is_some() && out.fits[0].fit_id.starts_with("gsf-1~"));
    assert!(!out.link && out.origin.is_none());
}

#[test]
fn update_times_read_as_dates_and_ages() {
    use crate::doctrine_panel::{age, date};
    assert_eq!(date(0), "1970-01-01");
    assert_eq!(date(1_759_600_000), "2025-10-04");
    assert_eq!(age(0, 3600), "today");
    assert_eq!(age(0, 86_400 * 40), "40 days ago");
    assert_eq!(age(0, 86_400 * 400), "13 months ago");
    assert_eq!(age(0, 86_400 * 800), "2 years ago");
}

/// Frame times of a big fleet after a layout change, on the real data: `SPAI_DOGMA_BLOB` and
/// `SPAI_DOCTRINES` (a supplied doctrines bundle) name the files, `SPAI_DOCTRINE` the doctrine.
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB and SPAI_DOCTRINES; prints frame times"]
fn real_data_fleet_frames_after_a_layout_change() {
    let dogma = spai_fit::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let docs: Vec<spai_fit::doctrine::Doctrine> = serde_json::from_str(&std::fs::read_to_string(std::env::var("SPAI_DOCTRINES").unwrap()).unwrap()).unwrap();
    let want = std::env::var("SPAI_DOCTRINE").unwrap_or_else(|_| "Alpha".into());
    let mut s = SimUi::default();
    s.handle(HostEvent::Dogma(Arc::new(dogma)));
    s.handle(HostEvent::Supplied(docs.into_iter().map(|doctrine| SuppliedDoctrine { side: DoctrineSide::Imperium, rev: 1, doctrine }).collect()));
    let d = s.supplied.iter().find(|x| x.doctrine.name.starts_with(&want)).unwrap().doctrine.clone();
    s.new_fleet_of(&d, 1);
    s.fleet.as_mut().unwrap().fleet.size = 250;
    s.fleet.as_mut().unwrap().enemy = Some(crate::fleet_panel::MIRROR.into());
    let ctx = egui::Context::default();
    let frame = |s: &mut SimUi, label: &str| {
        let t = std::time::Instant::now();
        let _ = ctx.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))), ..Default::default() }, |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| s.show(ui));
        });
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms > 30.0 {
            println!("{label}: {ms:.0} ms");
        }
        ms
    };
    for center in [crate::Center::Fit, crate::Center::Ships, crate::Center::Scene] {
        s.center = center;
        s.fleet_graph_open = true;
        // Settled first: everything worked out.
        for _ in 0..400 {
            frame(&mut s, "settle");
            if !s.fleet_analysis_pending() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        s.fleet.as_mut().unwrap().fleet.slots[0].override_count = Some(37 + center as u32 as u32);
        let mut worst: f64 = 0.0;
        for i in 0..200 {
            s.now_ms += 50;
            worst = worst.max(frame(&mut s, &format!("{center:?} frame {i} after the change")));
            std::thread::sleep(std::time::Duration::from_millis(10));
            if i > 20 && !s.fleet_analysis_pending() {
                break;
            }
        }
        println!("{center:?}: worst frame after a layout change {worst:.0} ms");
    }
    // The web build's frames hold one step each at most: the dearest step.
    let fleet = s.fleet.as_ref().unwrap().fleet.clone();
    let (mine, _) = s.fleet_stats_settled().unwrap();
    let mut job = crate::fleet_panel::FightJob::new(s.dogma.clone().unwrap(), fleet.clone(), fleet, mine.clone(), mine, Vec::new());
    let (mut worst, mut steps) = (0.0f64, 0);
    loop {
        let stage = job.stage();
        let t = std::time::Instant::now();
        let done = job.step();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms > 15.0 {
            println!("   step {stage}: {ms:.0} ms");
        }
        worst = worst.max(ms);
        steps += 1;
        if done {
            break;
        }
    }
    println!("fight in {steps} steps, the dearest {worst:.0} ms");
}

/// The web build takes a fight a step at a time between frames: the steps add up to the same
/// fight as one run, and each is a small part of it.
#[test]
fn a_fight_taken_step_by_step_is_the_fight_worked_out_at_once() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id: fit_id, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let fit_id = fit_id.clone();
    s.handle(HostEvent::Saved(fit_id.clone()));
    s.new_fleet(10);
    let f = s.fleet.as_mut().unwrap();
    f.fleet.size = 20;
    f.fleet.slots.push(FleetSlot { fit_id, rule: CountRule::Fill { min: 0, max: 100 }, ..FleetSlot::default() });
    let fleet = f.fleet.clone();
    let (mine, _) = s.fleet_stats_settled().unwrap();
    let job = || crate::fleet_panel::FightJob::new(s.dogma.clone().unwrap(), fleet.clone(), fleet.clone(), mine.clone(), mine.clone(), Vec::new());
    let (whole, _) = job().run(&|_| {});
    let mut stepped = job();
    let mut steps = 0;
    let mut last = 0.0;
    while !stepped.step() {
        steps += 1;
        assert!(stepped.progress() >= last, "progress only grows");
        last = stepped.progress();
    }
    let (parts, fresh) = stepped.finish();
    assert!(steps > 3, "split into steps: {steps}");
    assert_eq!(parts.sweep.points, whole.sweep.points);
    assert_eq!(parts.race.timeline, whole.race.timeline);
    assert_eq!(fresh.len(), 1, "the slot's options handed back for the cache, once for both sides");
}

/// Every knob the race reads works the fight out again: the logi's timing and the race's length
/// left the last analysis on screen as if it were still the one.
#[test]
fn the_logi_settings_and_the_race_length_redo_the_fight() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id: fit_id, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let fit_id = fit_id.clone();
    s.handle(HostEvent::Saved(fit_id.clone()));
    s.new_fleet(10);
    let f = s.fleet.as_mut().unwrap();
    f.fleet.size = 20;
    f.fleet.slots.push(FleetSlot { fit_id, rule: CountRule::Fill { min: 0, max: 100 }, ..FleetSlot::default() });
    f.enemy = Some(crate::fleet_panel::MIRROR.to_owned());
    let mut last = s.fleet_analysis_settled().expect("the first analysis");
    let changes: [fn(&mut spai_fit::fleet::Fleet); 5] = [
        |f| f.logi_spread_s = 9.0,
        |f| f.logi_reaction_s = 4.0,
        |f| f.logi_prelock = 0.2,
        |f| f.logi_efficiency = 0.4,
        |f| f.race_length_s = 120.0,
    ];
    for (i, change) in changes.into_iter().enumerate() {
        change(&mut s.fleet.as_mut().unwrap().fleet);
        s.now_ms += 10_000;
        let _ = s.fleet_analysis();
        s.now_ms += 10_000;
        let next = s.fleet_analysis_settled().unwrap();
        assert!(!Arc::ptr_eq(&next, &last), "change {i} worked the fight out again");
        last = next;
    }
}

/// A ship's ewar tuned in the fleet redoes the fight; a graph's own tune is worked out beside the
/// fleet's, which stays as it was.
#[test]
fn a_tuned_ship_redoes_the_fight_and_a_graphs_tune_keeps_to_the_graph() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    use spai_fit::stats::RemoteKind;
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id: fit_id, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let fit_id = fit_id.clone();
    s.handle(HostEvent::Saved(fit_id.clone()));
    s.new_fleet(10);
    let f = s.fleet.as_mut().unwrap();
    f.fleet.size = 20;
    f.fleet.slots.push(FleetSlot { fit_id, rule: CountRule::Fill { min: 0, max: 100 }, ..FleetSlot::default() });
    f.enemy = Some(crate::fleet_panel::MIRROR.to_owned());
    let first = s.fleet_analysis_settled().expect("the first analysis");
    let stats_before = s.stats_key(&s.fleet.as_ref().unwrap().fleet);
    {
        let f = &mut s.fleet.as_mut().unwrap().fleet;
        f.ewar_auto = false;
        f.slots[0].ewar_tune = vec![(RemoteKind::Web, 0.0)];
    }
    assert_eq!(s.stats_key(&s.fleet.as_ref().unwrap().fleet), stats_before, "a tune leaves the ships' numbers be");
    s.now_ms += 10_000;
    let _ = s.fleet_analysis();
    s.now_ms += 10_000;
    let tuned = s.fleet_analysis_settled().unwrap();
    assert!(!Arc::ptr_eq(&tuned, &first), "the tune worked the fight out again");
    assert_eq!(tuned.fleet.slots[0].ewar_tune, vec![(RemoteKind::Web, 0.0)]);
    let graph: [Option<crate::ewar_tune::SlotTunes>; 2] = [Some(vec![vec![(RemoteKind::Web, 1.0)]]), None];
    let tweak = |a: &mut spai_fit::fleet::Fleet, _: &mut spai_fit::fleet::Fleet| crate::ewar_tune::apply(a, graph[0].as_ref().unwrap());
    let mut own = None;
    for _ in 0..1000 {
        own = s.analysis(crate::fleet_panel::Run::Graph, &tweak).filter(|a| a.fleet.slots[0].ewar_tune == vec![(RemoteKind::Web, 1.0)]);
        if own.is_some() {
            break;
        }
        s.now_ms += 1_000;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(own.is_some(), "the graph's own analysis came in");
    assert!(Arc::ptr_eq(&s.fleet_analysis().unwrap(), &tuned), "the fleet's stays as it was");
}

/// The Hull tree goes class by size, then group with the class's own first, then hull, then fit.
#[test]
fn the_hull_tree_goes_by_class_group_hull_and_fit() {
    use crate::library_panel::{lines, Entry, Grouping, HullPath, Line};
    let path = |t: u32| match t {
        1 => HullPath { class: (0, "Frigate".into()), group: "Interceptor".into(), hull: "Taranis".into() },
        2 => HullPath { class: (0, "Frigate".into()), group: "Frigate".into(), hull: "Rifter".into() },
        3 => HullPath { class: (2, "Cruiser".into()), group: "Logistics".into(), hull: "Scimitar".into() },
        _ => HullPath { class: (0, "Frigate".into()), group: "Interceptor".into(), hull: "Malediction".into() },
    };
    let fit = |id: &str, name: &str, ship: u32| Entry { id: id.into(), name: name.into(), ship, tags: Vec::new(), doctrine: None };
    let all = vec![fit("a", "Logi", 3), fit("b", "Zeta tackle", 1), fit("c", "Alpha tackle", 1), fit("d", "Brawl", 2), fit("e", "Fast", 4)];
    let shown: Vec<String> = lines(&all, &path, Grouping::Hull, "", &Default::default())
        .iter()
        .map(|l| match l {
            Line::Group { title, depth, .. } => format!("{}{title}", "  ".repeat(usize::from(*depth))),
            Line::Fit(e) => format!("      {}", e.name),
        })
        .collect();
    assert_eq!(
        shown,
        ["Frigate", "  Frigate", "    Rifter", "      Brawl", "  Interceptor", "    Malediction", "      Fast", "    Taranis", "      Alpha tackle", "      Zeta tackle", "Cruiser", "  Logistics", "    Scimitar", "      Logi"]
    );
    assert_eq!(lines(&all, &path, Grouping::Flat, "intercep", &Default::default()).len(), 3, "the search reads the hull's group");
}

/// A worker in the test's own thread, every request and reply put through the bytes a web
/// worker's messages carry.
struct InlineWorker(std::cell::RefCell<crate::offload::Worker>, std::cell::Cell<usize>);

impl crate::offload::OffFrame for InlineWorker {
    fn run(&self, request: crate::offload::WorkRequest, mut reply: Box<dyn FnMut(crate::offload::WorkReply)>) {
        use crate::offload::{decode, encode};
        self.1.set(self.1.get() + 1);
        let request = decode(&encode(&request)).expect("the request reads back");
        self.0.borrow_mut().serve(request, &mut |r| reply(decode(&encode(&r)).expect("the reply reads back")));
    }
}

/// Handed to a worker, the fleet's numbers, its ammo and the fight come back as worked out at
/// home.
#[test]
fn a_worker_works_the_fleet_out_as_at_home() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    let build = |worker: bool| {
        let mut s = sim();
        let w = std::rc::Rc::new(InlineWorker(std::cell::RefCell::new(crate::offload::Worker::new(s.dogma.clone().unwrap())), std::cell::Cell::new(0)));
        if worker {
            s.set_off_frame(w.clone());
        }
        s.new_fit(ids::SHIP, 0);
        apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
        apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
        s.save();
        let reqs = s.take_requests();
        let HostRequest::SaveFit { id: fit_id, .. } = &reqs[0] else { panic!("{reqs:?}") };
        let fit_id = fit_id.clone();
        s.handle(HostEvent::Saved(fit_id.clone()));
        s.new_fleet(10);
        let f = s.fleet.as_mut().unwrap();
        f.fleet.size = 20;
        f.fleet.slots.push(FleetSlot { fit_id, rule: CountRule::Fill { min: 0, max: 100 }, ..FleetSlot::default() });
        f.enemy = Some(crate::fleet_panel::MIRROR.to_owned());
        let mut out = None;
        for _ in 0..20 {
            s.now_ms += 10_000;
            out = s.fleet_analysis_settled();
            if out.as_ref().is_some_and(|a| a.mine.ships > 0) {
                break;
            }
        }
        let stats = s.fleet_stats_settled().unwrap().0;
        let ammo = s.ammo_for(&stats);
        (out.expect("an analysis"), stats, ammo, w.1.get())
    };
    let (home, home_stats, _, _) = build(false);
    let (away, away_stats, _, asked) = build(true);
    assert!(asked >= 2, "the worker took the work: {asked} requests");
    assert_eq!(away_stats.ships, home_stats.ships);
    assert_eq!(away_stats.dps, home_stats.dps);
    assert_eq!(away.race.timeline, home.race.timeline);
    assert_eq!(away.race.winner, home.race.winner);
    assert_eq!(away.sweep.points.len(), home.sweep.points.len());
    assert_eq!(away.ammo_bands, home.ammo_bands);
}

/// A worker that answers only when told, as a web worker does a few frames later.
struct LateWorker(std::cell::RefCell<crate::offload::Worker>, std::cell::RefCell<Vec<(crate::offload::WorkRequest, Box<dyn FnMut(crate::offload::WorkReply)>)>>);

impl crate::offload::OffFrame for LateWorker {
    fn run(&self, request: crate::offload::WorkRequest, reply: Box<dyn FnMut(crate::offload::WorkReply)>) {
        self.1.borrow_mut().push((request, reply));
    }
}

impl LateWorker {
    fn answer(&self) -> usize {
        let waiting = std::mem::take(&mut *self.1.borrow_mut());
        let n = waiting.len();
        for (request, mut reply) in waiting {
            self.0.borrow_mut().serve(request, &mut |r| reply(r));
        }
        n
    }
}

/// Frames that pass while a worker works ask for nothing twice, and once its answers are in the
/// badge goes: a fleet's numbers asked for every frame left runs nobody collected.
#[test]
fn a_late_worker_is_asked_once_and_the_badge_goes_when_it_answers() {
    use spai_fit::fleet::{CountRule, FleetSlot};
    let mut s = sim();
    let w = std::rc::Rc::new(LateWorker(std::cell::RefCell::new(crate::offload::Worker::new(s.dogma.clone().unwrap())), Default::default()));
    s.set_off_frame(w.clone());
    s.new_fit(ids::SHIP, 0);
    apply(&mut s, PickTarget::Module(Slot::High, 0), ids::GUN, 1);
    apply(&mut s, PickTarget::Charge(Slot::High, 0), ids::AMMO, 2);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id: fit_id, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let fit_id = fit_id.clone();
    s.handle(HostEvent::Saved(fit_id.clone()));
    s.new_fleet(10);
    let f = s.fleet.as_mut().unwrap();
    f.fleet.size = 20;
    f.fleet.slots.push(FleetSlot { fit_id, rule: CountRule::Fill { min: 0, max: 100 }, ..FleetSlot::default() });
    f.enemy = Some(crate::fleet_panel::MIRROR.to_owned());
    let frame = |s: &mut SimUi| {
        s.now_ms += 1_000;
        let _ = s.fleet_stats();
        let stats = s.fleet_stats().map(|x| x.0);
        if let Some(st) = stats {
            let _ = s.ammo_for(&st);
        }
        s.fleet_analysis()
    };
    let mut answered = Vec::new();
    let mut done = None;
    for _ in 0..40 {
        for _ in 0..3 {
            done = frame(&mut s);
        }
        answered.push(w.answer());
        if done.as_ref().is_some_and(|a| a.mine.ships > 0) && !s.fleet_analysis_pending() {
            break;
        }
    }
    let a = done.expect("the fight came in");
    assert!(a.mine.ships > 0);
    assert!(answered.iter().all(|n| *n <= 4), "nothing asked again while it waited: {answered:?}");
    for _ in 0..3 {
        frame(&mut s);
    }
    assert!(!s.fleet_analysis_pending(), "the badge goes once the answers are in: {answered:?}");
}

/// The trash button asks first, Ctrl+click deletes at once; a fleet the same way.
#[test]
fn a_delete_asks_first_unless_ctrl_is_held() {
    let mut s = sim();
    s.new_fit(ids::SHIP, 0);
    s.save();
    let reqs = s.take_requests();
    let HostRequest::SaveFit { id, .. } = &reqs[0] else { panic!("{reqs:?}") };
    let id = id.clone();
    s.handle(HostEvent::Saved(id.clone()));
    let _ = s.take_requests();
    s.ask_delete(crate::Deleting::Fit, id.clone(), "Mine".into(), false);
    assert!(s.take_requests().iter().all(|r| !matches!(r, HostRequest::DeleteFit(_))), "nothing deleted before the answer");
    assert_eq!(s.confirm_delete.as_ref().map(|c| c.1.as_str()), Some(id.as_str()));
    s.confirm_delete = None;
    s.ask_delete(crate::Deleting::Fit, id.clone(), "Mine".into(), true);
    assert!(s.take_requests().iter().any(|r| matches!(r, HostRequest::DeleteFit(x) if *x == id)));
    assert!(s.confirm_delete.is_none());
    s.new_fleet(10);
    let fleet = s.fleet.as_ref().unwrap().id.clone();
    let _ = s.take_requests();
    s.ask_delete(crate::Deleting::Fleet, fleet.clone(), "F".into(), true);
    assert!(s.fleet.is_none());
    assert!(s.take_requests().iter().any(|r| matches!(r, HostRequest::DeleteFleet(x) if *x == fleet)));
}

/// Slots go by their hull; two of one hull by their variations, then by their fits' names.
#[test]
fn ships_go_by_hull_and_only_say_more_when_two_are_alike() {
    use spai_fit::fleet::{FleetSlot, FleetStats, SlotStats};
    let d = spai_fit::test_support::dataset();
    let slot = |slot: usize, ship, name: &str, fit: &str| SlotStats { fit_id: fit.into(), name: name.into(), count: 1, computed: Default::default(), slot, ship, role: spai_fit::fleet::Role::Mainline, fit: Default::default(), skills: spai_fit::skills::SkillSet::all_v() };
    let mut fit = spai_fit::fit::Fit { ship: ids::SHIP, name: "Brawler".into(), ..Default::default() };
    fit.variations.push(spai_fit::variations::Variation { id: 1, name: "Webs".into(), kind: spai_fit::variations::VariationKind::Refit, refit_key: None, ops: vec![], group: None });
    let fits: std::collections::HashMap<String, spai_fit::fit::Fit> = [("a".to_owned(), fit.clone()), ("b".to_owned(), fit)].into();
    let fleet = spai_fit::fleet::Fleet { slots: vec![FleetSlot { fit_id: "a".into(), ..Default::default() }, FleetSlot { fit_id: "a".into(), variation: Some(1), ..Default::default() }, FleetSlot { fit_id: "b".into(), variation: Some(1), ..Default::default() }, FleetSlot { fit_id: "c".into(), ..Default::default() }], ..Default::default() };
    let stats = FleetStats { slots: vec![slot(0, ids::SHIP, "Brawler", "a"), slot(1, ids::SHIP, "Brawler", "a"), slot(2, ids::SHIP, "Other brawler", "b"), slot(3, ids::T3C, "Loki thing", "c")], ..Default::default() };
    let hull = spai_fit::formats::name_of(&d, ids::SHIP);
    let t3c = spai_fit::formats::name_of(&d, ids::T3C);
    let labels = crate::fleet_panel::ship_labels(&d, &fleet, &stats, &|id| fits.get(id).cloned());
    assert_eq!(labels[3], t3c, "a hull of its own needs nothing more");
    assert_eq!(labels[0], hull, "no variation: the hull, the others say what sets them apart");
    assert_eq!(labels[1], format!("{hull} (Webs, Brawler)"));
    assert_eq!(labels[2], format!("{hull} (Webs, Other brawler)"));
    let unique: std::collections::HashSet<&String> = labels.iter().collect();
    assert_eq!(unique.len(), labels.len());
}
