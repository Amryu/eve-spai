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
fn doctrines_open_as_copies_and_admins_publish_the_open_fit() {
    let mut s = sim();
    let mut fit = Fit::new(ids::SHIP, "Doctrine Rifter");
    fit.add_module(ids::GUN, Slot::High);
    fit.variations.push(spai_fit::variations::Variation { id: 1, name: "AB".into(), kind: spai_fit::variations::VariationKind::Refit, refit_key: Some("prop:ab".into()), ops: vec![] });
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
    f.fleet.slots.push(FleetSlot { fit_id: fit_id.clone(), rule: CountRule::Fill { min: 0, max: 100 }, override_count: None, profile: None });
    let (mine, enemy) = s.fleet_stats().unwrap();
    assert_eq!((mine.ships, mine.slots.len()), (20, 1), "the saved fit's document is already at hand");
    assert!(enemy.is_none());
    assert!(mine.dps > 0.0);
    s.forget_fit_doc(&fit_id);
    assert_eq!(s.fleet_stats().unwrap().0.slots.len(), 0, "without the document the slot is left out");
    s.handle(HostEvent::FitDocs(vec![(fit_id.clone(), doc.clone())]));
    assert_eq!(s.fleet_stats().unwrap().0.slots.len(), 1);
    s.save_fleet();
    let reqs = s.take_requests();
    let Some(HostRequest::SaveFleet { id, name, doc: fleet_doc }) = reqs.iter().find(|r| matches!(r, HostRequest::SaveFleet { .. })) else { panic!("{reqs:?}") };
    assert_eq!(name, "New fleet");
    s.handle(HostEvent::FleetSaved(id.clone()));
    assert!(!s.fleet.as_ref().unwrap().dirty);
    s.handle(HostEvent::Fleets(vec![FleetSummary { id: id.clone(), name: name.clone(), updated_at: 0 }]));
    s.handle(HostEvent::FleetDoc { id: "enemy".into(), doc: fleet_doc.clone() });
    s.fleet.as_mut().unwrap().enemy = Some("enemy".into());
    let (_, enemy) = s.fleet_stats().unwrap();
    let (theirs, a, b) = enemy.unwrap();
    assert_eq!(theirs.ships, 20);
    assert!(a.applied_dps > 0.0 && (a.applied_dps - b.applied_dps).abs() < 1e-9, "a fleet against its own copy is symmetric: {a:?} vs {b:?}");
    assert!(a.critical_mass.is_some());
    s.handle(HostEvent::Fleet { id: "enemy".into(), doc: "nope".into() });
    assert_eq!(s.fleet.as_ref().unwrap().id, *id, "a broken fleet document changes nothing");
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
    assert!(fit(&s).cargo.iter().all(|c| c.type_id != ids::IMPLANT && c.type_id != ids::BOOSTER), "worn things never land in the cargo");
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
    assert_eq!(fit(&s).cargo.len(), 1);
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
    let name = |_t: u32| "Rifter".to_owned();
    let none = std::collections::BTreeSet::new();
    let by_hull = lines(&all, &name, Grouping::Hull, "", &none);
    assert!(matches!(&by_hull[0], Line::Group { n: 3, .. }), "one hull, three fits");
    let by_tag = lines(&all, &name, Grouping::Tag, "", &none);
    let alpha = by_tag.iter().find_map(|l| match l { Line::Group { title, n, .. } if title == "Alpha" => Some(*n), _ => None });
    assert_eq!(alpha, Some(2), "a fit and a doctrine both tagged Alpha");
    assert!(matches!(by_tag.iter().rev().find(|l| matches!(l, Line::Group { .. })), Some(Line::Group { key, .. }) if key == "tag:"), "the untagged come last");
    let found = lines(&all, &name, Grouping::Flat, "kit", &none);
    assert_eq!(found.len(), 1, "the search reads tags as well as names");
    let mut shut = std::collections::BTreeSet::new();
    shut.insert(format!("hull:{}", ids::SHIP));
    assert_eq!(lines(&all, &name, Grouping::Hull, "", &shut).len(), 1, "a folded group shows its header only");
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
