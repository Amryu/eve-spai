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
fn a_prop_refit_in_the_cargo_becomes_a_keyed_variation_and_a_fleet_swaps_on_it() {
    let d = dataset();
    let mut fit = rifter("DPS");
    fit.cargo.push(CargoItem { type_id: ids::AB, quantity: 1, hold: None });
    fit.cargo.push(CargoItem { type_id: ids::WEB, quantity: 1, hold: None });
    fit.cargo.push(CargoItem { type_id: ids::AMMO, quantity: 100, hold: None });
    let left = forum::prop_refit(&d, &mut fit);
    assert_eq!(left, vec![ids::WEB], "a spare with no fitted twin stays a spare; charges are no refit");
    assert_eq!(fit.variations.len(), 1);
    let v = &fit.variations[0];
    assert_eq!((v.name.as_str(), v.refit_key.as_deref()), ("Afterburner", Some("prop:ab")));
    let mut on = fit.clone();
    crate::variations::activate(&mut on, v.id);
    let flown = crate::variations::resolve(&d, &on);
    assert!(flown.modules.iter().any(|m| m.type_id == ids::AB) && !flown.modules.iter().any(|m| m.type_id == ids::MWD));

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
}

#[test]
fn an_mwd_beside_a_fitted_mwd_is_a_spare_not_a_refit() {
    let d = dataset();
    let mut fit = rifter("DPS");
    fit.cargo.push(CargoItem { type_id: ids::MWD, quantity: 1, hold: None });
    assert_eq!(forum::prop_refit(&d, &mut fit), vec![ids::MWD]);
    assert!(fit.variations.is_empty());
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
fn an_afterburner_beside_a_microwarpdrive_waits_online_and_a_variation_runs_it() {
    let d = dataset();
    let mut fit = rifter("Logi");
    fit.add_module(ids::AB, Slot::Mid);
    forum::prop_refit(&d, &mut fit);
    let ab = fit.modules.iter().find(|m| m.type_id == ids::AB).unwrap();
    assert_eq!(ab.state, crate::fit::State::Online);
    assert_eq!(fit.variations.len(), 1);
    assert_eq!(fit.variations[0].refit_key.as_deref(), Some("prop:ab"));
    let mut on = fit.clone();
    crate::variations::activate(&mut on, fit.variations[0].id);
    let flown = crate::variations::resolve(&d, &on);
    let state = |t| flown.modules.iter().find(|m| m.type_id == t).unwrap().state;
    assert_eq!((state(ids::AB), state(ids::MWD)), (crate::fit::State::Active, crate::fit::State::Online));
}

#[test]
fn a_spare_scrambler_swaps_for_the_fitted_disruptor() {
    let d = dataset();
    let mut fit = rifter("Tackle");
    fit.add_module(ids::POINT, Slot::Mid);
    let left = forum::point_refit(&d, &mut fit, vec![ids::SCRAM, ids::WEB]);
    assert_eq!(left, vec![ids::WEB]);
    assert_eq!(fit.variations[0].refit_key.as_deref(), Some("point:scram"));
    let mut on = fit.clone();
    crate::variations::activate(&mut on, fit.variations[0].id);
    let flown = crate::variations::resolve(&d, &on);
    assert!(flown.modules.iter().any(|m| m.type_id == ids::SCRAM) && !flown.modules.iter().any(|m| m.type_id == ids::POINT));
    assert_eq!(refit_label("point:scram"), "Warp scrambler");
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
    assert_eq!(short_fit_name("25.01 Standard", "Zirnitra", &["GSF", "Dreadnought Fits"]), "25.01 Standard");
}
