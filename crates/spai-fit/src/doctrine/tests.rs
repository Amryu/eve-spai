use super::*;
use crate::fit::{CargoItem, Slot};
use crate::test_support::{dataset, ids};

fn rifter(name: &str) -> Fit {
    let mut f = Fit::new(ids::SHIP, name);
    f.add_module(ids::GYRO, Slot::Low);
    f.add_module(ids::MWD, Slot::Mid);
    f
}

fn supplied() -> Doctrine {
    let fits = vec![
        DoctrineFit { fit_id: supplied_fit_id("gsf-rifters", "tackle"), fit: Some(rifter("Tackle")), role: FitRole::Tackle, main: false, note: String::new() },
        DoctrineFit { fit_id: supplied_fit_id("gsf-rifters", "dps"), fit: Some(rifter("DPS")), role: FitRole::Line, main: true, note: "100 of these".into() },
    ];
    Doctrine { id: "gsf-rifters".into(), alliance: "Goonswarm Federation".into(), category: "Strategic".into(), name: "Rifters".into(), tags: vec!["Strategic".into()], url: Some("https://forum.example/t/1".into()), updated: Some(1_700_000_000), fits, ..Doctrine::default() }
}

#[test]
fn a_fleet_of_a_doctrine_flies_its_mainline_first_as_the_main() {
    let d = dataset();
    let fleet = supplied().fleet(&d, &|_| None, &crate::skills::SkillSet::all_v());
    assert_eq!(fleet.name, "Rifters");
    assert_eq!(fleet.slots.len(), 2);
    assert_eq!(fleet.slots[0].fit_id, "gsf-rifters~dps");
    assert_eq!(fleet.main, Some(0));
    assert!(matches!(fleet.slots[0].rule, CountRule::Fill { .. }));
    assert_eq!(fleet.slots[1].rule, CountRule::Fixed(3), "tackle by its role in the doctrine, whatever the hull");
}

#[test]
fn a_copy_moves_the_fits_into_the_library_and_remembers_where_it_came_from() {
    let mut n = 0;
    let (copy, fits) = supplied().copy_into_library(&mut || {
        n += 1;
        format!("id{n}")
    });
    assert_eq!(copy.id, "id1");
    assert_eq!(copy.origin.as_deref(), Some("gsf-rifters"));
    assert!(!copy.link);
    assert_eq!(fits.len(), 2);
    assert_eq!(copy.fits.iter().map(|f| f.fit_id.as_str()).collect::<Vec<_>>(), vec!["id2", "id3"]);
    assert!(copy.fits.iter().all(|f| f.fit.is_none()));
    assert_eq!(fits[0].id, "id2");
    assert!(fits[0].tags.contains(&"Rifters".to_owned()));
    assert!(copy.fits[1].main, "the roles come along");
    let library = |id: &str| fits.iter().find(|f| f.id == id).cloned();
    let fleet = copy.fleet(&dataset(), &library, &crate::skills::SkillSet::all_v());
    assert_eq!(fleet.slots[0].fit_id, "id3");
}

#[test]
fn a_link_carries_no_fits_of_its_own() {
    let l = supplied().link_to("mine".into());
    assert!(l.link && l.fits.is_empty());
    assert_eq!(l.origin.as_deref(), Some("gsf-rifters"));
    assert_eq!((l.alliance.as_str(), l.category.as_str(), l.name.as_str()), ("Goonswarm Federation", "Strategic", "Rifters"));
}

#[test]
fn categories_sort_strategic_first_and_unknown_ones_last() {
    assert!(category_rank("Strategic") < category_rank("peacetime"));
    assert!(category_rank("Retired") < category_rank("Something else"));
}

#[test]
fn hulls_take_their_role_from_their_group() {
    assert_eq!(FitRole::of_hull(&dataset(), ids::SHIP), FitRole::Line);
}

#[test]
fn fit_names_drop_what_the_tree_already_says() {
    let said = ["GSF", "Goonswarm Federation", "HammerFleet (FNI)"];
    assert_eq!(short_fit_name("GSF FNI - DPS - V25.1", "Ferox Navy Issue", &said), "DPS - V25.1");
    assert_eq!(short_fit_name("GSF RNI/Vulture - LOGI - v25.1", "Basilisk", &said), "RNI/Vulture - LOGI - v25.1");
    assert_eq!(short_fit_name("GSF Standard v100.0", "Huginn", &said), "Standard v100.0");
    assert_eq!(short_fit_name("Rohk/RNI Command vulture", "Vulture", &said), "Rohk/RNI Command");
    assert_eq!(short_fit_name("GSF-MALLET Standard Vigil", "Vigil", &said), "MALLET Standard");
    assert_eq!(short_fit_name("GSF- SAC - SUPP - V26.1", "Confessor", &["GSF", "Sacrileges"]), "SUPP - V26.1");
    assert_eq!(short_fit_name("GSF - SACFLEET - DPS - V24.1", "Sacrilege", &["GSF", "Sacrileges"]), "DPS - V24.1");
    assert_eq!(short_fit_name("*Beehive", "Phobos", &["GSF", "Stupid Idiot Rorquals (SIR)", "Beehive"]), "Standard");
    assert_eq!(short_fit_name("T2 TFI", "Thrasher Fleet Issue", &["GSF", "Thrasher Fleet Issues (tFIs)"]), "T2");
    assert_eq!(short_fit_name("GSF - Flycatchers - Boost - V25.1", "Bifrost", &["GSF", "Svipuls"]), "Flycatchers - Boost - V25.1", "another doctrine's name is news here");
    assert_eq!(short_fit_name("GSF Retribution Fleet", "Retribution", &["GSF", "Retributions"]), "Standard");
    assert_eq!(short_fit_name("TigersClaw - DPS - V25.1", "Thanatos", &["GSF", "Tiger's Claw (Carriers)"]), "DPS - V25.1");
    assert_eq!(short_fit_name("25.01 Standard", "Zirnitra", &["GSF", "Dreadnought Fits"]), "25.01 Standard");
}

#[test]
fn a_fighter_stack_bigger_than_a_squadron_splits_into_full_ones() {
    let d = dataset();
    let mut fit = Fit::new(ids::SHIP, "Carrier");
    fit.fighters.push(crate::fit::FighterSquad { type_id: ids::SUPPORT_FIGHTER, count: 7, active: true });
    forum::launch_fighters(&d, &mut fit);
    assert_eq!(fit.fighters.iter().map(|f| f.count).collect::<Vec<_>>(), vec![3, 3, 1]);
}


fn with_cargo(mut fit: Fit, types: &[crate::ids::TypeId]) -> Fit {
    fit.cargo.extend(types.iter().map(|t| CargoItem { type_id: *t, quantity: 1, hold: None }));
    fit
}

fn flown(d: &crate::Dogma, fit: &Fit, key: &str) -> Fit {
    let v = fit.variations.iter().find(|v| v.refit_key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key} in {:?}", fit.variations));
    let mut on = fit.clone();
    crate::variations::activate(&mut on, v.id);
    crate::variations::resolve(d, &on)
}

#[test]
fn a_prop_refit_in_the_cargo_makes_a_category_and_a_fleet_swaps_on_it() {
    let d = dataset();
    let mut fit = with_cargo(rifter("DPS"), &[ids::AB, ids::WEB, ids::AMMO]);
    let made = forum::refits(&d, &mut fit);
    assert_eq!(made.left, vec![ids::WEB], "a spare with nothing of another kind to replace stays a spare; charges are no refit");
    assert_eq!(fit.variations.len(), 1);
    let v = &fit.variations[0];
    assert_eq!((v.name.as_str(), v.refit_key.as_deref(), v.group.as_deref()), ("Afterburner", Some("prop:ab"), Some("Afterburner")));
    assert_eq!(fit.base_name.as_deref(), Some("Microwarpdrive"), "the base is its propulsion's category");
    let ab = flown(&d, &fit, "prop:ab");
    assert!(ab.modules.iter().any(|m| m.type_id == ids::AB) && !ab.modules.iter().any(|m| m.type_id == ids::MWD));

    let mut doctrine = supplied();
    doctrine.fits[1].fit = Some(fit.clone());
    assert_eq!(doctrine.refit_keys(&|_| None), vec!["prop:ab".to_owned()]);
    let lookup = |id: &str| doctrine.fits.iter().find(|f| f.fit_id == id).and_then(|f| f.fit.clone());
    let mut fleet = doctrine.fleet(&d, &lookup, &crate::skills::SkillSet::all_v());
    assert_eq!(current_refit(&fleet, &lookup), None);
    swap_refit(&mut fleet, Some("prop:ab"), &lookup);
    assert_eq!(fleet.slots[0].variation, Some(v.id));
    assert_eq!(fleet.slots[1].variation, None, "a fit without the key keeps its base");
    assert_eq!(current_refit(&fleet, &lookup).as_deref(), Some("prop:ab"));
    swap_refit(&mut fleet, None, &lookup);
    assert_eq!(fleet.slots[0].variation, None);
    assert_eq!(refit_label("prop:ab"), "Afterburner");
    assert_eq!(refit_label("prop:ab|ewar:damp"), "Afterburner · Sensor dampener");
}

#[test]
fn an_mwd_beside_a_fitted_mwd_is_a_spare_not_a_refit() {
    let d = dataset();
    let mut fit = with_cargo(rifter("DPS"), &[ids::MWD]);
    assert_eq!(forum::refits(&d, &mut fit).left, vec![ids::MWD]);
    assert!(fit.variations.is_empty() && fit.base_name.is_none());
}

#[test]
fn an_afterburner_beside_a_microwarpdrive_waits_online_and_a_variation_runs_it() {
    let d = dataset();
    let mut fit = rifter("Logi");
    fit.add_module(ids::AB, Slot::Mid);
    forum::refits(&d, &mut fit);
    assert_eq!(fit.modules.iter().find(|m| m.type_id == ids::AB).unwrap().state, crate::fit::State::Online);
    let ab = flown(&d, &fit, "prop:ab");
    let state = |t| ab.modules.iter().find(|m| m.type_id == t).unwrap().state;
    assert_eq!((state(ids::AB), state(ids::MWD)), (crate::fit::State::Active, crate::fit::State::Online));
}

#[test]
fn a_spare_scrambler_swaps_for_the_fitted_disruptor_and_nothing_else() {
    let d = dataset();
    let mut fit = rifter("Tackle");
    fit.add_module(ids::POINT, Slot::Mid);
    fit.add_module(ids::WEB, Slot::Mid);
    let mut fit = with_cargo(fit, &[ids::SCRAM, ids::GYRO]);
    let made = forum::refits(&d, &mut fit);
    assert_eq!((made.left, made.unfit), (vec![ids::GYRO], vec![]));
    let v = &fit.variations[0];
    assert_eq!((v.refit_key.as_deref(), v.group.as_deref()), (Some("point:scram"), None), "no propulsion refit, no categories");
    let scram = flown(&d, &fit, "point:scram");
    assert!(scram.modules.iter().any(|m| m.type_id == ids::SCRAM) && !scram.modules.iter().any(|m| m.type_id == ids::POINT));
    assert!(scram.modules.iter().any(|m| m.type_id == ids::WEB), "the web stays: a scram replaces a point only");
}

#[test]
fn an_ewar_spare_replaces_another_kind_and_is_left_out_where_it_does_not_fit() {
    let d = dataset();
    let mut fit = rifter("Support");
    fit.add_module(ids::SCRAM, Slot::Mid);
    fit.add_module(ids::DAMP, Slot::Mid);
    let mut fit = with_cargo(fit, &[ids::WEB]);
    let made = forum::refits(&d, &mut fit);
    assert!(made.left.is_empty() && made.unfit.is_empty(), "{made:?}");
    let web = flown(&d, &fit, "ewar:web");
    assert!(web.modules.iter().any(|m| m.type_id == ids::WEB) && !web.modules.iter().any(|m| m.type_id == ids::DAMP), "the damp makes room, not the scram");
    assert!(web.modules.iter().any(|m| m.type_id == ids::SCRAM));

    let mut light = rifter("Support");
    light.add_module(ids::WEB, Slot::Mid);
    let mut light = with_cargo(light, &[ids::DAMP]);
    let made = forum::refits(&d, &mut light);
    assert_eq!((made.left, made.unfit), (vec![ids::DAMP], vec!["Sensor dampener".to_owned()]), "a refit over CPU is left out");
    assert!(light.variations.is_empty());
}

#[test]
fn ewar_refits_sit_under_each_propulsion_and_fit_where_that_propulsion_leaves_room() {
    let d = dataset();
    // CPU 130: gyro 30 and the microwarpdrive 25 leave no room for an 80 damp; the afterburner's 20 does.
    let mut fit = rifter("Support");
    fit.add_module(ids::WEB, Slot::Mid);
    let mut fit = with_cargo(fit, &[ids::AB, ids::DAMP_LIGHT]);
    let made = forum::refits(&d, &mut fit);
    assert_eq!(made.unfit, vec!["Microwarpdrive · Sensor dampener".to_owned()]);
    assert!(made.left.is_empty(), "the damp made a variation under one of them");
    let keys: Vec<(Option<&str>, Option<&str>)> = fit.variations.iter().map(|v| (v.group.as_deref(), v.refit_key.as_deref())).collect();
    assert_eq!(keys, vec![(Some("Afterburner"), Some("prop:ab")), (Some("Afterburner"), Some("prop:ab|ewar:damp"))]);
    let both = flown(&d, &fit, "prop:ab|ewar:damp");
    assert!(both.modules.iter().any(|m| m.type_id == ids::AB) && both.modules.iter().any(|m| m.type_id == ids::DAMP_LIGHT) && !both.modules.iter().any(|m| m.type_id == ids::WEB));
}
#[test]
fn a_refit_that_needs_room_takes_the_carried_fitting_modules_in_place_of_diagnostics() {
    let d = dataset();
    // Powergrid 40: the 45 afterburner needs a reactor control unit's 20% over a diagnostic system.
    let mut fit = rifter("Heavy tackle");
    fit.add_module(ids::DIAGNOSTIC, Slot::Low);
    let mut fit = with_cargo(fit, &[ids::AB_BIG, ids::RCU]);
    let made = forum::refits(&d, &mut fit);
    assert!(made.unfit.is_empty(), "{made:?}");
    let ab = flown(&d, &fit, "prop:ab");
    assert!(ab.modules.iter().any(|m| m.type_id == ids::RCU) && !ab.modules.iter().any(|m| m.type_id == ids::DIAGNOSTIC));
    assert!(ab.modules.iter().any(|m| m.type_id == ids::GYRO), "the damage module stays");

    let mut bare = with_cargo(rifter("Heavy tackle"), &[ids::AB_BIG, ids::RCU]);
    assert_eq!(forum::refits(&d, &mut bare).unfit, vec!["Afterburner".to_owned()], "no diagnostic to give up, no room");
}
#[test]
fn a_refit_that_needs_room_offlines_smartbombs_one_at_a_time() {
    let d = dataset();
    // Powergrid 40, three 10 smartbombs and the 10 microwarpdrive: the 25 afterburner needs two offline.
    let mut fit = rifter("Huginn-like");
    fit.modules.retain(|m| m.type_id != ids::GYRO);
    for _ in 0..3 {
        fit.add_module(ids::SMARTBOMB, Slot::High);
    }
    let mut fit = with_cargo(fit, &[ids::AB_MID]);
    let made = forum::refits(&d, &mut fit);
    assert!(made.unfit.is_empty(), "{made:?}");
    let ab = flown(&d, &fit, "prop:ab");
    let off = ab.modules.iter().filter(|m| m.type_id == ids::SMARTBOMB && m.state == crate::fit::State::Offline).count();
    assert_eq!(off, 2, "as few offline as it takes");
    let base_off = fit.modules.iter().filter(|m| m.state == crate::fit::State::Offline).count();
    assert_eq!(base_off, 0, "the base keeps its smartbombs online");
}

fn booster(name: &str, bursts: &[crate::ids::TypeId]) -> Fit {
    let mut f = Fit::new(ids::SHIP, name);
    for (i, b) in bursts.iter().enumerate() {
        f.modules.push(crate::fit::Module::new(*b, Slot::High, i as u8).with_state(crate::fit::State::Active));
    }
    f.implants.push(ids::IMPLANT);
    f
}

/// Boosters get their mindlinks and burst swaps; the doctrine projects as few as cover its boost
/// kinds, one each, the same on every fit.
#[test]
fn a_doctrine_projects_one_booster_for_each_boost_kind() {
    let d = dataset();
    let fit = |key: &str, fit: Fit, role: FitRole| DoctrineFit { fit_id: supplied_fit_id("gsf-x", key), fit: Some(fit), role, main: role == FitRole::Line, note: String::new() };
    let mut doc = Doctrine {
        id: "gsf-x".into(),
        fits: vec![
            fit("dps", rifter("DPS"), FitRole::Line),
            fit("both", booster("Both", &[ids::BURST, ids::SKIRM_BURST]), FitRole::Command),
            fit("shield", booster("Shield", &[ids::BURST, ids::BURST]), FitRole::Command),
            fit("spare", booster("Spare", &[ids::BURST]), FitRole::Command),
        ],
        ..Doctrine::default()
    };
    project_boosters(&d, &mut doc);
    let get = |i: usize| doc.fits[i].fit.clone().unwrap();
    let both = get(1);
    let names: Vec<String> = both.variations.iter().map(|v| v.full_name()).collect();
    assert_eq!(names, ["Mindlink · Shield", "Mindlink · Skirmish"], "one charge a kind here: a swap would swap nothing");
    assert_eq!(both.modules.iter().map(|m| m.charge).collect::<Vec<_>>(), [Some(ids::BURST_CHARGE), Some(ids::SKIRM_CHARGE)], "empty bursts loaded");
    let shield = get(2);
    let want = vec![("gsf-x~shield".to_owned(), shield.variations[0].id, vec![ids::BURST_CHARGE]), ("gsf-x~both".to_owned(), both.variations[1].id, vec![ids::BURST_CHARGE, ids::SKIRM_CHARGE])];
    let projected = |i: usize| get(i).projected_fits.into_iter().map(|p| (p.fit_id, p.variation.unwrap(), p.charges)).collect::<Vec<_>>();
    assert_eq!(projected(0), want, "two shield bursts beat one; the mixed booster takes skirmish and keeps its shield burst; the spare is not needed");
    assert_eq!(projected(3), want, "the same on every fit");
    assert_eq!(projected(1), want[..1], "not itself");
    let mut again = doc.clone();
    project_boosters(&d, &mut again);
    assert_eq!(again.fits[1].fit.as_ref().unwrap().variations.len(), 2, "running it again adds nothing");
    assert_eq!(again.fits[0].fit.as_ref().unwrap().projected_fits.len(), 2);
}

/// A hull that strengthens a kind its fit carries no burst for still gets the swap to it: the
/// kind's bursts, charges and mindlink.
#[test]
fn a_booster_swaps_to_every_kind_its_hull_strengthens() {
    let d = dataset();
    let mut fit = Fit::new(ids::T3C, "T3C");
    fit.modules.push(crate::fit::Module::new(ids::BOOST_SUB, Slot::Subsystem, 2).with_state(crate::fit::State::Online));
    fit.modules.push(crate::fit::Module::new(ids::BURST, Slot::High, 0).with_state(crate::fit::State::Active));
    booster_variations(&d, &mut fit);
    let names: Vec<String> = fit.variations.iter().map(|v| v.full_name()).collect();
    assert_eq!(names, ["Mindlink · Shield", "Bursts · Skirmish"]);
    fit.active_variations = vec![fit.variations[1].id];
    let flown = crate::variations::resolve(&d, &fit);
    assert_eq!(flown.module(Slot::High, 0).map(|m| (m.type_id, m.charge)), Some((ids::SKIRM_BURST, Some(ids::SKIRM_CHARGE))));
    assert!(flown.implants.contains(&ids::SKIRM_MINDLINK));
}

/// A projected booster covering charges runs each of them, loaded or not; none covered is what
/// it has loaded.
#[test]
fn a_booster_covers_the_charges_picked_for_it() {
    let d = dataset();
    let skills = crate::skills::SkillSet::all_v();
    let fit = booster("B", &[ids::BURST, ids::SKIRM_BURST]);
    assert!(crate::fleet::covered_bursts(&d, &fit, &[], &skills).is_empty(), "nothing loaded");
    let buffs: Vec<u32> = crate::fleet::covered_bursts(&d, &fit, &[ids::BURST_CHARGE, ids::SKIRM_CHARGE], &skills).into_iter().map(|(id, _)| id as u32).collect();
    assert_eq!(buffs, [12, 60]);
}
