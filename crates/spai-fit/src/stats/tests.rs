use super::*;
use crate::test_support::{close, dataset, ids};
use crate::fit::{DroneStack, Module};

fn env() -> Env {
    Env::default()
}

#[test]
fn fitting_room_counts_online_modules_rigs_and_hardpoints() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    f.modules.push(Module::new(ids::GUN, Slot::High, 1).with_state(State::Offline));
    f.modules.push(Module::new(ids::RIG, Slot::Rig, 0));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.fitting.cpu_used, 25.0), "{}: the offline gun costs nothing", c.fitting.cpu_used);
    assert!(close(c.fitting.power_used, 15.0));
    assert!(close(c.fitting.cpu_total, 130.0));
    assert!(close(c.fitting.calibration_used, 50.0));
    assert_eq!(c.fitting.slots[0], (2, 3), "high slots used/total");
    assert_eq!(c.fitting.slots[3], (1, 3));
    assert_eq!((c.fitting.turrets_used, c.fitting.turrets_total), (1, 3));
}

#[test]
fn effective_hit_points_follow_the_damage_profile() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.tank.armor.hp, 350.0));
    assert!(close(c.tank.armor.resonance[0], 0.4));
    let uniform_through = 0.25 * (0.4 + 1.0 + 1.0 + 1.0);
    assert!(close(c.tank.armor.ehp, 350.0 / uniform_through));
    assert!(close(c.tank.shield.ehp, 200.0), "no resists on the shield");
    assert!(close(c.tank.ehp, c.tank.shield.ehp + c.tank.armor.ehp + c.tank.hull.ehp));
    f.damage_profile = DamageProfile { em: 1.0, thermal: 0.0, kinetic: 0.0, explosive: 0.0 };
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.tank.armor.ehp, 350.0 / 0.4));
    assert!(close(c.tank.shield_peak_regen, 2.5 * 200.0 / 500.0));
}

#[test]
fn repairers_count_only_when_active_and_paste_triples_an_ancillary() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 0).with_state(State::Online));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.tank.reps.armor, 0.0));
    f.modules[0].state = State::Active;
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.tank.reps.armor, 60.0 / 6.0));
    f.modules.push(Module::new(ids::AAR, Slot::Low, 1).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.tank.reps.armor, 10.0 + 40.0 / 6.0), "an unfuelled ancillary repairs its base amount");
    f.modules[1].charge = Some(ids::PASTE);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.tank.reps.armor_burst, 10.0 + 120.0 / 6.0), "{}", c.tank.reps.armor_burst);
    // 8 cycles of 6 s at 120, then a 60 s reload.
    let sustained = 120.0 * 8.0 / (8.0 * 6.0 + 60.0);
    assert!(close(c.tank.reps.armor, 10.0 + sustained), "{} vs {}", c.tank.reps.armor, 10.0 + sustained);
}

#[test]
fn turret_groups_split_by_charge_and_count_reloads() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    for i in 0..3 {
        f.modules.push(Module::new(ids::GUN, Slot::High, i).with_charge(ids::AMMO).with_state(State::Active));
    }
    f.modules[2].charge = Some(ids::AMMO_LONG);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert_eq!(c.weapons.len(), 2, "two groups: EMP and Barrage");
    let emp = c.weapons.iter().find(|w| w.charge == Some(ids::AMMO)).unwrap();
    assert_eq!((emp.count, emp.kind), (2, WeaponKind::Turret));
    // 3.0 × 1.05 (skill) damage multiplier, 9 em per shot, 2 guns.
    assert!(close(emp.volley.em, 2.0 * 3.15 * 9.0), "{}", emp.volley.em);
    assert!(close(emp.cycle_s, 3.0));
    assert_eq!(emp.shots, Some(20), "capacity 2 / volume 0.05 = 40 rounds, 2 per shot");
    assert!(close(emp.reload_s, 10.0));
    assert!(close(emp.dps_burst, emp.volley.total() / 3.0));
    assert!(close(emp.dps, emp.volley.total() * 20.0 / (20.0 * 3.0 + 10.0)));
    assert!(close(emp.optimal, 600.0) && close(emp.falloff, 6000.0 * 1.375));
    let barrage = c.weapons.iter().find(|w| w.charge == Some(ids::AMMO_LONG)).unwrap();
    assert!(close(barrage.optimal, 1200.0 * 1.5));
    assert!(close(c.volley(), emp.volley.total() + barrage.volley.total()));
    f.modules[0].charge = None;
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert_eq!(c.weapons.iter().map(|w| w.count).sum::<u32>(), 2, "an empty gun shoots nothing");
}

#[test]
fn missiles_drones_and_smartbombs_are_weapons_too() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::LAUNCHER, Slot::High, 0).with_charge(ids::MISSILE).with_state(State::Active));
    f.modules.push(Module::new(ids::SMARTBOMB, Slot::High, 1).with_state(State::Active));
    f.drones.push(DroneStack { type_id: ids::DRONE, count: 3, active: true, launched: None, mutation: None });
    f.drones.push(DroneStack { type_id: ids::DRONE, count: 2, active: false, launched: None, mutation: None });
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let missile = c.weapons.iter().find(|w| w.kind == WeaponKind::Missile).unwrap();
    assert!(close(missile.volley.kinetic, 300.0 * 1.25), "{}", missile.volley.kinetic);
    assert!(close(missile.cycle_s, 9.0));
    let m = missile.missile.as_ref().unwrap();
    assert!(close(m.range, 4000.0 * 6.0) && close(missile.optimal, m.range));
    assert!(close(m.explosion_radius, 100.0) && close(m.damage_reduction_factor, 0.7));
    let drones = c.weapons.iter().find(|w| w.kind == WeaponKind::Drone).unwrap();
    assert_eq!(drones.count, 3, "only the drones in space");
    assert!(close(drones.volley.thermal, 3.0 * 1.1 * 20.0));
    assert!(close(drones.cycle_s, 4.0));
    let bomb = c.weapons.iter().find(|w| w.kind == WeaponKind::Smartbomb).unwrap();
    assert!(close(bomb.volley.em, 100.0) && close(bomb.optimal, 5000.0));
    assert_eq!(c.drones.active, 3);
    assert!(close(c.drones.bandwidth_used, 15.0) && close(c.drones.bay_used, 25.0));
    assert!(close(c.drones.control_range, 20_000.0));
}

#[test]
fn a_disintegrator_spools_to_its_maximum_unless_told_otherwise() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::DISINTEGRATOR, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let w = &c.weapons[0];
    assert!(close(w.spool_max, 2.0));
    assert!(close(w.volley.em, 9.0 * 3.0), "fully spooled: ×(1 + 2.0)");
    f.modules[0].spool = Some(0.0);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.weapons[0].volley.em, 9.0));
    f.modules[0].spool = Some(0.5);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.weapons[0].volley.em, 9.0 * 2.0));
}

#[test]
fn capacitor_usage_comes_from_the_effects_discharge_attributes() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.cap.capacity, 300.0) && close(c.cap.recharge_s, 100.0));
    assert!(close(c.cap.peak_regen, 7.5));
    assert!(close(c.cap.usage, 24.0 / 6.0), "{}: the repairer draws, the gun has no capacitor need", c.cap.usage);
    assert!(c.cap.stable_at.is_some() && c.cap.lasts_s.is_none());
    let est = c.cap.estimate_stable_at.unwrap();
    assert!((c.cap.stable_at.unwrap() - est).abs() < 0.05, "{:?} vs {est}", c.cap.stable_at);
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 1).with_state(State::Active));
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 2).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(c.cap.lasts_s.is_some_and(|t| t > 0.0 && t < 600.0), "{:?}", c.cap);
    assert_eq!(c.cap.estimate_stable_at, None);
    f.modules.push(Module::new(ids::CAP_BOOSTER, Slot::Mid, 0).with_charge(ids::CAP_CHARGE).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(c.cap.usage < 0.0, "{}: the booster gives more than the repairers take", c.cap.usage);
    assert!(c.cap.lasts_s.is_none(), "{:?}", c.cap);
}

#[test]
fn a_running_nosferatu_feeds_the_capacitor() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 0).with_state(State::Active));
    let without = compute(&d, &f, &SkillSet::all_v(), &env()).cap;
    f.modules.push(Module::new(ids::NOS, Slot::High, 0).with_state(State::Online));
    assert_eq!(compute(&d, &f, &SkillSet::all_v(), &env()).cap.usage, without.usage, "online, it gives nothing");
    f.modules[1].state = State::Active;
    let with = compute(&d, &f, &SkillSet::all_v(), &env()).cap;
    assert!(close(with.usage, without.usage - 12.0 / 6.0), "{} vs {}: 12 GJ every 6 s", with.usage, without.usage);
    assert!(with.stable_at.unwrap() > without.stable_at.unwrap());
}

#[test]
fn mobility_and_sensors_use_the_published_formulas() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::MWD, Slot::Mid, 0).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let mass = 1_067_000.0 + 500_000.0;
    assert!(close(c.mobility.mass, mass));
    assert!(close(c.mobility.align_s, std::f64::consts::LN_2 * 3.0 * mass / 500_000.0));
    assert!(close(c.mobility.warp_speed_au, 5.0));
    assert!(close(c.mobility.signature, 210.0));
    assert!(close(c.sensors.scan_resolution, 600.0) && close(c.sensors.lock_range, 30_000.0));
    assert_eq!(c.sensors.max_targets, 2, "the pilot's limit is the lower one");
    assert_eq!((c.sensors.kind, c.sensors.strength), (SensorKind::Ladar, 9.0));
    let lock = c.sensors.lock_time_s(100.0);
    assert!(close(lock, 40_000.0 / (600.0 * 100.0f64.asinh().powi(2))));
    assert!(close(lock_time(0.0, 100.0), 1800.0));
}

/// `SPAI_DOGMA_BLOB`: a classic autocannon Rifter against the figures fitting tools agree on.
#[test]
#[ignore]
fn the_real_blob_computes_a_known_rifter() {
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let text = "[Rifter, Known Rifter]\nIFFA Compact Damage Control\nCounterbalanced Compact Gyrostabilizer\n200mm Steel Plates II\n\n5MN Microwarpdrive II\nFaint Epsilon Scoped Warp Scrambler\nFleeting Compact Stasis Webifier\n\n200mm AutoCannon II, Republic Fleet EMP S\n200mm AutoCannon II, Republic Fleet EMP S\n200mm AutoCannon II, Republic Fleet EMP S\n\nSmall Projectile Burst Aerator I\nSmall Projectile Collision Accelerator I\nSmall Trimark Armor Pump I\n\nRepublic Fleet EMP S x1000\nRepublic Fleet Phased Plasma S x1000\n";
    let imported = crate::formats::eft::import(&d, text).unwrap();
    assert!(imported.unknown.is_empty(), "{:?}", imported.unknown);
    let fit = imported.fit;
    let started = std::time::Instant::now();
    let c = compute(&d, &fit, &SkillSet::all_v(), &env());
    eprintln!("computed in {:?}", started.elapsed());
    eprintln!("dps {:.1} volley {:.1} ehp {:.0} (shield {:.0} armor {:.0} hull {:.0}) speed {:.0} align {:.2} sig {:.0} cap {:?} cpu {:.1}/{:.1} pg {:.1}/{:.1}",
        c.dps(), c.volley(), c.tank.ehp, c.tank.shield.ehp, c.tank.armor.ehp, c.tank.hull.ehp, c.mobility.max_velocity, c.mobility.align_s, c.mobility.signature, c.cap, c.fitting.cpu_used, c.fitting.cpu_total, c.fitting.power_used, c.fitting.power_total);
    let calc = crate::calc::Calc::new(&d, &fit, &SkillSet::all_v(), &env());
    for (i, it) in calc.modules() {
        eprintln!("  {:40} cpu {:6.1} pg {:6.1}", d.type_(it.type_id).unwrap().name, calc.value(i, crate::dogma::attr::CPU), calc.value(i, crate::dogma::attr::POWER));
    }
    let w = &c.weapons[0];
    eprintln!("guns: {} × volley {:.1} cycle {:.2}s optimal {:.0} falloff {:.0} tracking {:.3} shots {:?}", w.count, w.volley.total(), w.cycle_s, w.optimal, w.falloff, w.tracking, w.shots);
    assert_eq!(c.weapons.len(), 1);
    assert_eq!(w.count, 3);
    assert!(c.dps() > 150.0 && c.dps() < 260.0, "{}", c.dps());
    assert!(c.tank.ehp > 3500.0 && c.tank.ehp < 7000.0, "{}", c.tank.ehp);
    assert!(c.mobility.max_velocity > 2500.0 && c.mobility.max_velocity < 4000.0);
    assert!(c.fitting.cpu_used <= c.fitting.cpu_total && c.fitting.power_used <= c.fitting.power_total, "{:?}", c.fitting);
    let back = crate::formats::eft::export(&d, &fit);
    let again = crate::formats::eft::import(&d, &back).unwrap().fit;
    assert_eq!(again.modules.len(), fit.modules.len());
    assert_eq!(crate::formats::eft::export(&d, &again), back);
}

#[test]
fn remote_modules_are_listed_with_their_reach_and_fighters_shoot_like_missiles() {
    use crate::stats::{range_factor, RemoteKind};
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "logi");
    f.modules.push(Module::new(ids::REMOTE_REP, Slot::High, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::REMOTE_REP, Slot::High, 1).with_state(State::Online));
    f.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
    f.fighters.push(crate::fit::FighterSquad { type_id: ids::FIGHTER, count: 9, active: true });
    let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
    let m = &c.remote.modules;
    assert_eq!(m.len(), 2, "the online repairer does nothing: {m:?}");
    let rep = m.iter().find(|x| x.kind == RemoteKind::ArmorRep).unwrap();
    assert!((rep.amount - 10.0).abs() < 1e-9 && rep.optimal == 7000.0 && rep.falloff == 2000.0);
    let web = m.iter().find(|x| x.kind == RemoteKind::Web).unwrap();
    assert_eq!((web.amount, web.optimal, web.falloff), (60.0, 10_000.0, 5000.0), "a web's strength reads as a magnitude");
    assert_eq!((c.remote.armor_rep, c.remote.rep_range), (10.0, 9000.0));
    assert_eq!(range_factor(5000.0, 7000.0, 2000.0), 1.0);
    assert!((range_factor(9000.0, 7000.0, 2000.0) - 0.5).abs() < 1e-12);
    assert_eq!(range_factor(13_001.0, 7000.0, 2000.0), 0.0);
    let fighters = c.weapons.iter().find(|w| w.kind == crate::stats::WeaponKind::Fighter).expect("a fighter group");
    assert_eq!(fighters.count, 9);
    assert!((fighters.volley.total() - 30.0 * 1.5 * 9.0).abs() < 1e-9, "{}", fighters.volley.total());
    assert!((fighters.dps - fighters.volley.total() / 4.0).abs() < 1e-9);
    let big = crate::fit::TargetProfile { signature: 400.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0, resist: [0.0; 4] };
    let at = |dist: f64, t: &crate::fit::TargetProfile| crate::stats::application::applied_factor(fighters, dist, t, 0.0, 0.0);
    assert_eq!(at(6000.0, &big), 1.0);
    assert!((at(9000.0, &big) - 0.5).abs() < 1e-12, "half at one falloff");
    assert_eq!(at(20_000.0, &big), 0.0);
    let small = crate::fit::TargetProfile { signature: 20.0, velocity: 1000.0, ..big };
    assert!(at(1000.0, &small) < 0.5, "a small fast target takes a fraction, as from a missile");
    f.fighters[0].active = false;
    let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
    assert!(c.weapons.iter().all(|w| w.kind != crate::stats::WeaponKind::Fighter));
}

#[test]
fn module_stats_give_each_module_its_fitting_draw_and_damage() {
    use crate::stats::module_stats;
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    f.modules.push(Module::new(ids::GUN, Slot::High, 1).with_charge(ids::AMMO).with_state(State::Active));
    f.modules.push(Module::new(ids::DC, Slot::Low, 0).with_state(State::Offline));
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 1).with_state(State::Active));
    let calc = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let c = compute_with(&calc, &f);
    let m = module_stats(&calc, &c);
    assert_eq!(m.len(), 4);
    let gun = m.iter().find(|x| x.slot == Slot::High && x.index == 0).unwrap();
    let group = c.weapons.iter().find(|w| w.type_id == ids::GUN).unwrap();
    assert!((gun.dps - group.dps / 2.0).abs() < 1e-9, "each gun carries half the group's DPS");
    assert!(gun.cpu > 0.0 && gun.power > 0.0 && gun.cap_per_s == 0.0, "{gun:?}: autocannons draw no capacitor");
    let dc = m.iter().find(|x| x.slot == Slot::Low && x.index == 0).unwrap();
    assert_eq!((dc.cpu, dc.power, dc.cap_per_s, dc.dps), (0.0, 0.0, 0.0, 0.0), "offline: nothing drawn");
    let rep = m.iter().find(|x| x.slot == Slot::Low && x.index == 1).unwrap();
    assert!(rep.cap_per_s > 0.0 && rep.dps == 0.0);
    let total: f64 = m.iter().map(|x| x.cpu).sum();
    assert!((total - c.fitting.cpu_used).abs() < 1e-6);
}

#[test]
fn holds_fill_from_the_cargo_and_overfill_warns() {
    use crate::fit::CargoItem;
    use crate::test_support::attr_def;
    let mut d = dataset();
    let mut fleet = attr_def(912, "fleetHangarCapacity", true, true, 0.0);
    fleet.display = "Fleet Hangar Capacity".into();
    d.attributes.push(fleet);
    let mut bonus = attr_def(913, "shipBonusFleetHangarCapacity", true, true, 0.0);
    bonus.display = String::new();
    d.attributes.push(bonus);
    let ship = d.types.iter_mut().find(|t| t.id == ids::SHIP).unwrap();
    ship.attrs.push((912, 50.0));
    ship.attrs.push((913, 5.0));
    ship.attrs.sort_by_key(|(a, _)| *a);
    let d = d.finish();
    let mut f = Fit::new(ids::SHIP, "t");
    let ammo_volume = f64::from(d.type_(ids::AMMO).unwrap().attr(attr::VOLUME).unwrap_or(0.0));
    f.cargo.push(CargoItem { type_id: ids::AMMO, quantity: 10, hold: None });
    f.cargo.push(CargoItem { type_id: ids::AMMO, quantity: 4, hold: Some(912) });
    let c = compute(&d, &f, &SkillSet::all_v(), &Env::default());
    assert_eq!(c.holds.len(), 2, "the cargo hold and the fleet hangar, not the bonus to it: {:?}", c.holds);
    assert!((c.holds[0].used - 10.0 * ammo_volume).abs() < 1e-9);
    assert_eq!(c.holds[1].name, "Fleet Hangar Capacity");
    assert!((c.holds[1].used - 4.0 * ammo_volume).abs() < 1e-9);
    let cap = c.holds[0].capacity;
    let mut over = f.clone();
    over.cargo[0].quantity = (cap / ammo_volume.max(1e-9)) as u32 + 10;
    let c = compute(&d, &over, &SkillSet::all_v(), &Env::default());
    let w = crate::warnings::check(&d, &over, &c, &[]);
    assert!(w.iter().any(|w| matches!(&w.kind, crate::warnings::WarningKind::HoldOver { hold, .. } if hold == "Cargo hold")), "{w:?}");
}

#[test]
fn modules_tell_their_cycle_and_what_one_cycle_does() {
    use crate::fit::{Module, Slot, State};
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
    f.modules.push(Module::new(ids::DC, Slot::Low, 1).with_state(State::Online));
    let calc = crate::calc::Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let c = compute_with(&calc, &f);
    let stats = module_stats(&calc, &c);
    let of = |slot, index| stats.iter().find(|m| m.slot == slot && m.index == index).unwrap();
    let rep = of(Slot::Low, 0);
    assert!(rep.cycle_s.is_some_and(|s| s > 0.0));
    assert!(matches!(rep.info, Some(ModuleInfo { kind: InfoKind::ArmorRepair, value, .. }) if value > 0.0));
    assert!(matches!(of(Slot::Mid, 0).info, Some(ModuleInfo { kind: InfoKind::Web, value, .. }) if value < 0.0));
    assert!(of(Slot::High, 0).cycle_s.is_some(), "a gun's cycle is its rate of fire");
    assert!(of(Slot::Low, 1).info.is_none() && of(Slot::Low, 1).cycle_s.is_none(), "a damage control does neither");
}

#[test]
fn a_module_that_heats_tells_how_long_it_lasts_overloaded() {
    use crate::fit::{Module, Slot, State};
    use crate::test_support::attr_def;
    let mut d = dataset();
    d.attributes.push(attr_def(attr::HEAT_DAMAGE, "heatDamage", true, false, 0.0));
    d.attributes.push(attr_def(9001, "heatAbsorbtionRateModifier", true, false, 0.0));
    d.attributes.push(attr_def(9002, "heatAttenuationHi", true, false, 0.0));
    let gun = d.types.iter_mut().find(|t| t.id == ids::GUN).unwrap();
    gun.attrs.extend([(attr::HEAT_DAMAGE, 10.0), (attr::HP, 40.0), (9001, 0.02)]);
    gun.attrs.sort_by_key(|(a, _)| *a);
    let ship = d.types.iter_mut().find(|t| t.id == ids::SHIP).unwrap();
    ship.attrs.push((9002, 0.5));
    ship.attrs.sort_by_key(|(a, _)| *a);
    let d = d.finish();
    let burnout = |second: State| {
        let mut f = Fit::new(ids::SHIP, "t");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Overload));
        f.modules.push(Module::new(ids::GUN, Slot::High, 1).with_charge(ids::AMMO).with_state(second));
        f.modules.push(Module::new(ids::DC, Slot::Low, 0).with_state(State::Online));
        let calc = crate::calc::Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
        let c = compute_with(&calc, &f);
        let stats = module_stats(&calc, &c);
        assert!(stats.iter().find(|m| m.slot == Slot::Low).unwrap().burnout_s.is_none(), "a module that takes no heat never burns out");
        let gun = stats.iter().find(|m| m.slot == Slot::High && m.index == 0).unwrap();
        (gun.burnout_s.unwrap(), gun.cycle_s.unwrap())
    };
    let (alone, cycle) = burnout(State::Active);
    assert!(alone > 4.0 * cycle, "40 HP at 10 a hit takes longer than four cycles: the rack warms first and not every cycle hits");
    let (beside, _) = burnout(State::Overload);
    assert!(beside < alone, "a second overloaded gun beside it heats the rack and hits it too: {beside} against {alone}");
}

#[test]
fn a_neighbour_left_cool_outlasts_the_overloaded_module() {
    use crate::fit::{Module, Slot, State};
    use crate::test_support::attr_def;
    let mut d = dataset();
    d.attributes.push(attr_def(attr::HEAT_DAMAGE, "heatDamage", true, false, 0.0));
    d.attributes.push(attr_def(9001, "heatAbsorbtionRateModifier", true, false, 0.0));
    d.attributes.push(attr_def(9002, "heatAttenuationHi", true, false, 0.0));
    let gun = d.types.iter_mut().find(|t| t.id == ids::GUN).unwrap();
    gun.attrs.extend([(attr::HEAT_DAMAGE, 10.0), (attr::HP, 40.0), (9001, 0.02)]);
    gun.attrs.sort_by_key(|(a, _)| *a);
    let ship = d.types.iter_mut().find(|t| t.id == ids::SHIP).unwrap();
    ship.attrs.push((9002, 0.5));
    ship.attrs.sort_by_key(|(a, _)| *a);
    let d = d.finish();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Overload));
    f.modules.push(Module::new(ids::GUN, Slot::High, 1).with_charge(ids::AMMO).with_state(State::Active));
    let calc = crate::calc::Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let c = compute_with(&calc, &f);
    let stats = module_stats(&calc, &c);
    let at = |i: u8| stats.iter().find(|m| m.slot == Slot::High && m.index == i).unwrap().burnout_s.unwrap();
    assert!(at(1) > at(0), "the cool neighbour only takes the spill-over: {} against {}", at(1), at(0));
    let mut cool = f.clone();
    cool.modules[0].state = State::Active;
    let calc = crate::calc::Calc::new(&d, &cool, &SkillSet::all_v(), &Env::default());
    let c = compute_with(&calc, &cool);
    assert!(module_stats(&calc, &c).iter().all(|m| m.burnout_s.is_none()), "nothing overloaded, no burnout figures");
}

#[test]
fn missiles_fly_whole_seconds_and_one_more_by_chance() {
    let m = crate::stats::MissileProfile { velocity: 1000.0, flight_time_s: 6.3, range: 6300.0, explosion_radius: 0.0, explosion_velocity: 0.0, damage_reduction_factor: 0.0 };
    assert_eq!((m.short_range(), m.long_range()), (6000.0, 7000.0));
    assert_eq!(m.reach(5999.0), 1.0);
    assert!((m.reach(6500.0) - 0.3).abs() < 1e-9, "past the whole seconds, three in ten fly the extra one");
    assert_eq!(m.reach(7001.0), 0.0);
    let whole = crate::stats::MissileProfile { flight_time_s: 6.0, ..m };
    assert_eq!((whole.reach(6000.0), whole.reach(6000.1)), (1.0, 0.0), "a whole flight time has a single wall");
}

#[test]
fn an_empty_hold_holds_a_plain_zero() {
    let d = dataset();
    let c = compute(&d, &Fit::new(ids::SHIP, "x"), &SkillSet::all_v(), &Env::default());
    for h in &c.holds {
        assert!(h.used == 0.0 && h.used.is_sign_positive(), "{} uses {:?}", h.name, h.used);
    }
}

#[test]
fn burnout_waits_for_the_rack_to_heat() {
    use crate::stats::burn_time;
    // A rack that heats at once: hp over the full rate.
    assert!((burn_time(2.0, 1e6, 40.0).unwrap() - 20.0).abs() < 1e-3);
    // A slow rack: a full second of the rate is lost to every 1/k of warming up.
    let t = burn_time(2.0, 0.05, 40.0).unwrap();
    let dealt = 2.0 * (t - (1.0 - (-0.05 * t).exp()) / 0.05);
    assert!((dealt - 40.0).abs() < 1e-6 && t > 20.0 + 10.0, "{t}");
    assert_eq!(burn_time(0.0, 0.05, 40.0), None, "no damage, no burnout");
}

#[test]
fn tackle_reaches_its_max_range_and_a_field_its_radius() {
    use crate::stats::RemoteKind;
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "tackle");
    f.modules.push(Module::new(ids::SCRAM, Slot::Mid, 0).with_state(State::Active));
    f.modules.push(Module::new(ids::POINT, Slot::Mid, 1).with_state(State::Active));
    f.modules.push(Module::new(ids::FIELD, Slot::High, 0).with_state(State::Active));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let of = |k: RemoteKind| c.remote.modules.iter().find(|m| m.kind == k).map(|m| (m.amount, m.optimal, m.falloff));
    assert_eq!(of(RemoteKind::Scram), Some((2.0, 9000.0, 0.0)), "a scrambler holds to its maxRange");
    assert_eq!(of(RemoteKind::Point), Some((1.0, 24_000.0, 0.0)));
    assert_eq!(of(RemoteKind::Bubble), Some((100.0, 20_000.0, 0.0)), "an unscripted field is a bubble of its warpScrambleRange");
    f.modules[2] = Module::new(ids::FIELD, Slot::High, 0).with_charge(ids::FOCUSED_SCRIPT).with_state(State::Active);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let points: Vec<f64> = c.remote.modules.iter().filter(|m| m.kind == RemoteKind::Point).map(|m| m.optimal).collect();
    assert_eq!(points, vec![24_000.0, 30_000.0], "the focused script points one target at the raised range");
}

fn running(t: crate::ids::TypeId, slot: Slot, k: u8) -> Module {
    Module::new(t, slot, k).with_state(State::Active)
}

#[test]
fn a_vorton_hits_within_its_range_like_a_missile_and_arcs_on() {
    use crate::stats::application::applied_factor;
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "v");
    f.modules.push(running(ids::VORTON, Slot::High, 0).with_charge(ids::VORTON_CHARGE));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let w = c.weapons.iter().find(|w| w.kind == WeaponKind::Vorton).expect("a vorton group");
    assert!(close(w.volley.total(), 200.0) && close(w.optimal, 20_000.0));
    assert_eq!((w.arc_targets, w.arc_range), (3, 10_000.0));
    let big = crate::fit::TargetProfile { signature: 400.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0, resist: [0.0; 4] };
    assert_eq!(applied_factor(w, 15_000.0, &big, 0.0, 0.0), 1.0, "no wrecking shots, no tracking");
    assert_eq!(applied_factor(w, 20_001.0, &big, 0.0, 0.0), 0.0, "nothing past its range");
    let crossing = crate::fit::TargetProfile { signature: 400.0, velocity: 300.0, transversal: 300.0, ..big.clone() };
    let f = applied_factor(w, 10_000.0, &crossing, 0.0, 0.0);
    assert!(f > 0.2 && f < 1.0, "a crossing target takes a missile's share, not nothing: {f}");
}

#[test]
fn a_bomb_bursts_at_the_end_of_its_flight_and_a_breacher_pod_eats_hit_points() {
    use crate::stats::application::applied_factor;
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "b");
    f.modules.push(running(ids::BOMB_LAUNCHER, Slot::High, 0).with_charge(ids::BOMB));
    f.modules.push(running(ids::BREACHER_LAUNCHER, Slot::High, 1).with_charge(ids::BREACHER_POD));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let bomb = c.weapons.iter().find(|w| w.kind == WeaponKind::Bomb).expect("a bomb");
    assert!(close(bomb.volley.total(), 5800.0), "no missile skill multiplies a bomb: {}", bomb.volley.total());
    assert!(close(bomb.cycle_s, 90.0), "its cycle and reactivation delay: {}", bomb.cycle_s);
    assert!(close(bomb.splash, 15_000.0));
    let big = crate::fit::TargetProfile { signature: 800.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0, resist: [0.0; 4] };
    let small = crate::fit::TargetProfile { signature: 100.0, ..big.clone() };
    assert_eq!(applied_factor(bomb, 30_000.0, &big, 0.0, 0.0), 1.0, "it bursts at 30 km");
    assert_eq!(applied_factor(bomb, 5_000.0, &big, 0.0, 0.0), 0.0, "and not where it is launched");
    assert!(close(applied_factor(bomb, 40_000.0, &small, 0.0, 0.0), 0.25), "a quarter of its explosion takes a quarter");
    let pod = c.weapons.iter().find(|w| w.kind == WeaponKind::Breacher).expect("a breacher pod");
    let dot = pod.dot.expect("its damage over time");
    assert_eq!((dot.per_tick, dot.duration_s), (200.0, 40.0));
    assert!(close(pod.dps, 200.0) && pod.volley.total() == 0.0);
    assert!(close(dot.on(10_000.0), 60.0), "0.6% of a small target's hit points");
    assert!(close(dot.on(1_000_000.0), 200.0), "capped on a big one");
}

#[test]
fn superweapons_strike_once_or_tick_and_spare_small_ships() {
    use crate::stats::application::applied_factor;
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "t");
    f.modules.push(running(ids::DOOMSDAY, Slot::High, 0));
    f.modules.push(running(ids::LANCE, Slot::High, 1));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let dd = c.weapons.iter().find(|w| w.type_id == ids::DOOMSDAY).expect("the doomsday");
    assert!(close(dd.volley.total(), 2_400_000.0) && close(dd.cycle_s, 300.0));
    let lance = c.weapons.iter().find(|w| w.type_id == ids::LANCE).expect("the lance");
    assert!(close(lance.volley.total(), 750_000.0), "fifteen ticks: {}", lance.volley.total());
    assert!(close(lance.splash, 2500.0));
    let small = crate::fit::TargetProfile { signature: 2500.0, velocity: 0.0, transversal: 0.0, distance: 0.0, radius: 0.0, resist: [0.0; 4] };
    assert!(close(applied_factor(lance, 100_000.0, &small, 0.0, 0.0), 0.5));
    assert_eq!(applied_factor(lance, 250_000.0, &small, 0.0, 0.0), 0.0);
}

#[test]
fn drones_fighters_and_charged_or_spooling_repairers_count_as_remote() {
    use crate::fit::FighterSquad;
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "logi");
    f.drones.push(DroneStack { type_id: ids::LOGI_DRONE, count: 2, active: true, launched: None, mutation: None });
    f.drones.push(DroneStack { type_id: ids::WEB_DRONE, count: 1, active: true, launched: None, mutation: None });
    f.fighters.push(FighterSquad { type_id: ids::SUPPORT_FIGHTER, count: 4, active: true });
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(c.remote.armor_rep, 2.0 * 72.0 / 5.0), "{}", c.remote.armor_rep);
    let webs: Vec<&RemoteModule> = c.remote.modules.iter().filter(|m| m.kind == RemoteKind::Web).collect();
    assert_eq!(webs.len(), 1 + 3, "the drone and each fighter of a squadron its size holds");
    assert!(webs.iter().all(|w| w.flies));
    let mut f = Fit::new(ids::SHIP, "rr");
    f.modules.push(running(ids::MUTA_REP, Slot::High, 0));
    f.modules.push(running(ids::ARAR, Slot::High, 1).with_charge(ids::PASTE));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let muta = c.remote.modules.iter().find(|m| m.type_id == ids::MUTA_REP).unwrap();
    assert!(close(muta.amount, 10.0 * 2.5), "fully spooled unless set: {}", muta.amount);
    assert!(close(muta.spool_at(0.0), 1.0 / 2.5) && close(muta.spool_at(1000.0), 1.0));
    let arar = c.remote.modules.iter().find(|m| m.type_id == ids::ARAR).unwrap();
    assert!(close(arar.amount, 60.0 * 3.0 * 8.0 / (8.0 * 6.0 + 60.0)), "paste triples it for a load, then it reloads: {}", arar.amount);
    f.modules[0].spool = Some(0.0);
    let cold = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(close(cold.remote.modules.iter().find(|m| m.type_id == ids::MUTA_REP).unwrap().amount, 10.0));
}

#[test]
fn boosts_for_friends_and_an_ecm_burst_show_as_remote_and_stay_out_of_the_enemys_ewar() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "support");
    f.modules.push(running(ids::SENSOR_BOOST, Slot::Mid, 0));
    f.modules.push(running(ids::TRACKING_BOOST, Slot::Mid, 1));
    f.modules.push(running(ids::ECM_BURST, Slot::Mid, 2));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let of = |k: RemoteKind| c.remote.modules.iter().find(|m| m.kind == k).unwrap_or_else(|| panic!("{k:?}"));
    let rsb = of(RemoteKind::SensorBoost);
    assert!(close(rsb.amount, 30.0) && close(rsb.optimal, 48_000.0) && close(rsb.falloff, 96_000.0) && close(rsb.parts[1], 30.0));
    let rtc = of(RemoteKind::TrackingBoost);
    assert!(close(rtc.amount, 25.0) && close(rtc.parts[1], 15.0));
    let burst = of(RemoteKind::EcmBurst);
    assert!(close(burst.amount, 4.0) && close(burst.optimal, 6000.0) && close(burst.falloff, 0.0), "it jams within its radius: {burst:?}");
    let cuts = crate::fleet::ewar::cutoffs(&[(1, &c)]);
    assert!(cuts.iter().all(|(k, _, _)| !k.is_assist()), "{cuts:?}");
    assert!(cuts.iter().any(|(k, _, edge)| *k == RemoteKind::EcmBurst && close(*edge, 6000.0)));
}

#[test]
fn a_group_limit_holds_the_extra_module_back_and_a_wrong_charge_stays_out() {
    let mut d = dataset();
    d.types.iter_mut().find(|t| t.id == ids::HARDENER).unwrap().attrs.push((ids::MAX_GROUP_ACTIVE, 1.0));
    let d = d.finish();
    let mut f = Fit::new(ids::SHIP, "dc");
    f.modules.push(running(ids::HARDENER, Slot::Mid, 0));
    let one = compute(&d, &f, &SkillSet::all_v(), &env()).tank.armor.resonance;
    f.modules.push(running(ids::HARDENER, Slot::Mid, 1));
    let two = compute(&d, &f, &SkillSet::all_v(), &env()).tank.armor.resonance;
    assert_eq!(one, two, "the second runs no more than online");
    let capped = compute(&d, &f, &SkillSet::all_v(), &env()).capped;
    assert_eq!(capped.len(), 1);
    assert_eq!((capped[0].index, capped[0].state, capped[0].limit), (1, State::Online, crate::calc::GroupLimit::Active));
    let mut g = Fit::new(ids::SHIP, "gun");
    g.modules.push(running(ids::GUN, Slot::High, 0).with_charge(ids::BOMB));
    assert!(compute(&d, &g, &SkillSet::all_v(), &env()).weapons.is_empty(), "a gun loads no bomb");
    let w = crate::warnings::check(&d, &g, &compute(&d, &g, &SkillSet::all_v(), &env()), &[]);
    assert!(w.iter().any(|w| matches!(w.kind, crate::warnings::WarningKind::WrongCharge { .. })));
}

/// A dataset where the hardener's group may have one online and one running, and a rig like the
/// Command Processor that adds `maxGangModules` to both limits of that group.
fn group_limited() -> Dogma {
    use crate::dogma::{Domain, EffectCategory, Func, Modifier, Op};
    use crate::test_support::{attr_def, effect, modifier, type_def};
    const MAX_GROUP_ONLINE: crate::ids::AttrId = 978;
    const MAX_GANG_MODULES: crate::ids::AttrId = 435;
    let mut d = dataset();
    let h = d.types.iter_mut().find(|t| t.id == ids::HARDENER).unwrap();
    h.attrs.push((ids::MAX_GROUP_ACTIVE, 1.0));
    h.attrs.push((MAX_GROUP_ONLINE, 1.0));
    d.attributes.push(attr_def(MAX_GROUP_ONLINE, "maxGroupOnline", true, true, 0.0));
    d.attributes.push(attr_def(MAX_GANG_MODULES, "maxGangModules", true, true, 0.0));
    d.effects.push(effect(
        900,
        "commandProcessorEffect",
        EffectCategory::Passive,
        vec![
            Modifier { group: Some(328), ..modifier(Domain::Ship, Func::LocationGroup, MAX_GROUP_ONLINE, MAX_GANG_MODULES, Op::ModAdd) },
            Modifier { group: Some(328), ..modifier(Domain::Ship, Func::LocationGroup, ids::MAX_GROUP_ACTIVE, MAX_GANG_MODULES, Op::ModAdd) },
        ],
    ));
    d.types.push(type_def(COMMAND_PROCESSOR, "Small Command Processor I", 773, 7, &[(MAX_GANG_MODULES, 1.0)], &[900], &[]));
    d.finish()
}

const COMMAND_PROCESSOR: crate::ids::TypeId = 9900;

#[test]
fn group_limits_carry_the_fits_bonuses_and_a_full_group_excuses_the_offline_rest() {
    use crate::calc::GroupLimit::Online;
    let d = group_limited();
    let mut f = Fit::new(ids::SHIP, "cp");
    f.modules.push(running(ids::HARDENER, Slot::Mid, 0));
    f.modules.push(running(ids::HARDENER, Slot::Mid, 1));
    f.modules.push(running(ids::HARDENER, Slot::Mid, 2));
    f.modules.push(Module::new(ids::HARDENER, Slot::Mid, 3).with_state(State::Offline));
    let one = compute(&d, &f, &SkillSet::all_v(), &env());
    let caps = |c: &Computed| c.capped.iter().map(|c| (c.index, c.limit, c.max, c.state)).collect::<Vec<_>>();
    assert_eq!(caps(&one), vec![(1, Online, 1, State::Offline), (2, Online, 1, State::Offline)]);
    assert_eq!(one.limited_offline, vec![(Slot::Mid, 3)], "the fourth is offline because the group is full");
    f.modules.push(Module::new(COMMAND_PROCESSOR, Slot::Rig, 0).with_state(State::Online));
    let two = compute(&d, &f, &SkillSet::all_v(), &env());
    assert_eq!(caps(&two), vec![(2, Online, 2, State::Offline)], "the rig makes room for one more");
    assert_eq!(two.limited_offline, vec![(Slot::Mid, 3)]);
    assert!(two.tank.armor.resonance[0] < one.tank.armor.resonance[0], "and the second hardener now works: {:?} vs {:?}", two.tank.armor.resonance, one.tank.armor.resonance);
    let w = crate::warnings::check(&d, &f, &two, &[]);
    assert!(w.iter().any(|w| matches!(w.kind, crate::warnings::WarningKind::GroupLimit { index: 2, .. })));
    assert!(!w.iter().any(|w| matches!(w.kind, crate::warnings::WarningKind::GroupLimit { index: 1, .. })));
    assert!(!w.iter().any(|w| matches!(w.kind, crate::warnings::WarningKind::OfflineModule { .. })), "offline for want of room is no warning: {w:?}");
}

#[test]
fn yielding_to_a_limit_puts_the_last_of_the_group_offline_and_spares_the_module_just_set() {
    let d = group_limited();
    let mut f = Fit::new(ids::SHIP, "cp");
    f.modules.push(running(ids::HARDENER, Slot::Mid, 0));
    f.modules.push(running(ids::HARDENER, Slot::Mid, 1));
    f.modules.push(running(ids::HARDENER, Slot::Mid, 2));
    f.modules.push(Module::new(COMMAND_PROCESSOR, Slot::Rig, 0).with_state(State::Online));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let shown = f.clone();
    assert!(crate::calc::yield_to_limits(&d, &mut f, &shown, &c.capped, None));
    let states = |f: &Fit| (0..3).map(|i| f.module(Slot::Mid, i).unwrap().state).collect::<Vec<_>>();
    assert_eq!(states(&f), vec![State::Active, State::Active, State::Offline]);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(c.capped.is_empty() && c.limited_offline == vec![(Slot::Mid, 2)]);
    let same = f.clone();
    assert!(!crate::calc::yield_to_limits(&d, &mut f, &same, &c.capped, None), "nothing left to yield");
    // The pilot turns the third on: the second, last of the others in rack order, yields.
    f.module_mut(Slot::Mid, 2).unwrap().state = State::Active;
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let shown = f.clone();
    assert!(crate::calc::yield_to_limits(&d, &mut f, &shown, &c.capped, Some((Slot::Mid, 2))));
    assert_eq!(states(&f), vec![State::Active, State::Offline, State::Active]);
    // Shown with another module in the slot (a variation), the base is left alone.
    let mut other = f.clone();
    other.module_mut(Slot::Mid, 0).unwrap().type_id = ids::MWD;
    f.module_mut(Slot::Mid, 1).unwrap().state = State::Active;
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let before = f.clone();
    let capped: Vec<_> = c.capped.iter().map(|c| crate::calc::Capped { slot: Slot::Mid, index: 0, ..*c }).collect();
    assert!(!crate::calc::yield_to_limits(&d, &mut f, &other, &capped, None));
    assert_eq!(f, before);
}

const LEGION_BOOST: &str = "[Legion, GSF - SAC - Boost - ]
Damage Control II
Syndicate 1600mm Steel Plates
Syndicate 1600mm Steel Plates
Reactor Control Unit II
True Sansha Thermal Armor Hardener
True Sansha EM Armor Hardener
True Sansha Kinetic Armor Hardener
Imperial Navy Multispectrum Energized Membrane

Gistum B-Type 50MN Microwarpdrive
Small Cap Battery II

Information Command Burst II
Information Command Burst II
Expanded Probe Launcher I
125mm Gatling AutoCannon II
Skirmish Command Burst II
Skirmish Command Burst II

Medium Command Processor I
Medium Trimark Armor Pump II
Medium Trimark Armor Pump II

Legion Core - Augmented Antimatter Reactor
Legion Defensive - Augmented Plating
Legion Offensive - Support Processor
Legion Propulsion - Intercalated Nanofibers


Warrior II x5
Acolyte II x5
";

/// A Legion runs one command burst, plus one per Command Processor rig; the Support Processor
/// subsystem adds none. `SPAI_DOGMA_BLOB=<path> cargo test -p spai-fit real_data_legion -- --ignored`
#[test]
#[ignore = "needs SPAI_DOGMA_BLOB"]
fn real_data_legion_runs_two_bursts_with_one_command_processor() {
    use crate::calc::GroupLimit::Online;
    let d = crate::dogma::blob::decode(&std::fs::read(std::env::var("SPAI_DOGMA_BLOB").unwrap()).unwrap()).unwrap();
    let imp = crate::formats::import_any(&d, LEGION_BOOST).remove(0);
    assert!(imp.unknown.is_empty(), "{:?}", imp.unknown);
    let mut f = imp.fit;
    f.cap_drones_out(5);
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert_eq!(c.capped.iter().map(|c| (c.slot, c.index, c.limit, c.max)).collect::<Vec<_>>(), vec![(Slot::High, 4, Online, 2), (Slot::High, 5, Online, 2)]);
    assert_eq!((c.drones.active, c.drones.bandwidth_used, c.drones.max_active), (5, 25.0, 5));
    let shown = f.clone();
    assert!(crate::calc::yield_to_limits(&d, &mut f, &shown, &c.capped, None));
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert!(c.capped.is_empty());
    assert_eq!(c.limited_offline, vec![(Slot::High, 4), (Slot::High, 5)]);
    let w = crate::warnings::check(&d, &f, &c, &[]);
    assert!(!w.iter().any(|w| matches!(w.kind, crate::warnings::WarningKind::OfflineModule { .. } | crate::warnings::WarningKind::GroupLimit { .. })), "{w:?}");
    // Without the rig, one burst.
    f.modules.retain(|m| m.slot != Slot::Rig || m.index != 0);
    for m in f.modules.iter_mut().filter(|m| m.slot == Slot::High) {
        m.state = State::Active;
    }
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    assert_eq!(c.capped.iter().map(|c| (c.index, c.max)).collect::<Vec<_>>(), vec![(1, 1), (4, 1), (5, 1)]);
}

#[test]
fn a_running_bubble_blooms_the_signature_and_mining_counts_its_crits() {
    let mut d = dataset();
    d.types.iter_mut().find(|t| t.id == ids::FIELD).unwrap().attrs.push((attr::SIGNATURE_RADIUS_BONUS, 50.0));
    let d = d.finish();
    let mut f = Fit::new(ids::SHIP, "hic");
    f.modules.push(Module::new(ids::FIELD, Slot::High, 0).with_state(State::Online));
    let off = compute(&d, &f, &SkillSet::all_v(), &env()).mobility.signature;
    f.modules[0].state = State::Active;
    let on = compute(&d, &f, &SkillSet::all_v(), &env()).mobility.signature;
    assert!(close(on, off * 1.5), "{off} -> {on}");
    let mut m = Fit::new(ids::SHIP, "miner");
    m.modules.push(running(ids::MINER, Slot::High, 0));
    let c = compute(&d, &m, &SkillSet::all_v(), &env());
    assert!(close(c.mining.ore, 120.0 / 45.0 * 1.02), "{:?}", c.mining);
    assert!(close(c.mining.total(), c.mining.ore));
}

#[test]
fn an_oversized_squadron_counts_at_its_size_and_the_hull_needs_tubes_for_it() {
    use crate::warnings::{check, FighterClass, WarningKind};
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "carrier");
    f.fighters.push(crate::fit::FighterSquad { type_id: ids::SUPPORT_FIGHTER, count: 4, active: true });
    let c = compute(&d, &f, &SkillSet::all_v(), &env());
    let w = check(&d, &f, &c, &[]);
    assert!(w.iter().any(|w| w.kind == WarningKind::SquadronOver { fighter: ids::SUPPORT_FIGHTER, count: 4, max: 3 }));
    assert!(w.iter().any(|w| w.kind == WarningKind::FighterTubesOver { used: 1, total: 0 }), "a frigate has no launch tubes");
    assert!(!w.iter().any(|w| matches!(w.kind, WarningKind::FighterClassOver { class: FighterClass::Light, .. })), "it is no light fighter");
}


/// An ancillary repairer tells how long a full load of its charges lasts and the reload after it.
#[test]
fn an_ancillary_repairer_counts_its_cycles_a_load() {
    let d = dataset();
    let mut f = Fit::new(ids::SHIP, "aar");
    f.modules.push(Module::new(ids::AAR, Slot::Low, 0).with_charge(ids::PASTE).with_state(State::Active));
    f.modules.push(Module::new(ids::REPAIRER, Slot::Low, 1).with_state(State::Active));
    let calc = crate::calc::Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
    let stats = module_stats(&calc, &compute_with(&calc, &f));
    let info = |index| stats.iter().find(|m| m.slot == Slot::Low && m.index == index).and_then(|m| m.info).unwrap();
    assert_eq!(info(0).per_load, Some((8, 60.0)), "0.08 m3 of 0.01 m3 paste, one a cycle, a minute's reload");
    assert_eq!(info(1).per_load, None, "a plain repairer burns nothing");
}
