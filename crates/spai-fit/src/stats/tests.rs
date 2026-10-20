use super::*;
use crate::calc::tests_support::{close, dataset, ids};
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
    f.drones.push(DroneStack { type_id: ids::DRONE, count: 3, active: true });
    f.drones.push(DroneStack { type_id: ids::DRONE, count: 2, active: false });
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
