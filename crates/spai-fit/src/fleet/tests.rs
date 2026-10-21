use super::*;
use crate::fit::{Module, Slot};
use crate::test_support::{dataset, ids};

fn fleet(size: u32, slots: Vec<FleetSlot>) -> Fleet {
    Fleet { size, slots, ..Fleet::default() }
}

fn slot(fit: &str, rule: CountRule) -> FleetSlot {
    FleetSlot { fit_id: fit.into(), rule, override_count: None, profile: None, variation: None }
}

#[test]
fn counts_follow_fixed_percent_and_fill_rules_and_typed_numbers() {
    let f = fleet(100, vec![slot("logi", CountRule::Percent(10.0)), slot("boost", CountRule::Fixed(2)), slot("dps", CountRule::Fill { min: 10, max: 200 }), slot("tackle", CountRule::Fill { min: 0, max: 5 })]);
    assert_eq!(resolve_counts(&f), vec![10, 2, 44, 5], "the two fill slots share the 88 left, the tackle capped at 5");
    let f = fleet(10, vec![slot("logi", CountRule::Percent(10.0)), slot("dps", CountRule::Fill { min: 20, max: 200 })]);
    assert_eq!(resolve_counts(&f), vec![1, 20], "a fill slot's minimum holds even over the fleet size");
    let mut f = fleet(40, vec![slot("dps", CountRule::Fill { min: 0, max: 200 }), slot("logi", CountRule::Percent(25.0))]);
    assert_eq!(resolve_counts(&f), vec![30, 10]);
    f.slots[1].override_count = Some(3);
    assert_eq!(resolve_counts(&f), vec![37, 3], "a typed number wins and the fill takes the rest");
    f.slots[0].override_count = Some(1);
    assert_eq!(resolve_counts(&f), vec![1, 3]);
    assert_eq!(resolve_counts(&fleet(0, vec![slot("x", CountRule::Percent(50.0))])), vec![0]);
}

fn gun_fit(name: &str, guns: u8) -> Fit {
    let mut f = Fit::new(ids::SHIP, name);
    for i in 0..guns {
        f.modules.push(Module::new(ids::GUN, Slot::High, i).with_charge(ids::AMMO).with_state(State::Active));
    }
    f.modules.push(Module::new(ids::DC, Slot::Low, 0));
    f
}

fn library() -> Vec<Fit> {
    let mut logi = Fit::new(ids::SHIP, "logi");
    logi.modules.push(Module::new(ids::REMOTE_REP, Slot::High, 0).with_state(State::Active));
    let mut web = gun_fit("webber", 1);
    web.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
    vec![gun_fit("brawler", 3), gun_fit("kiter", 1), logi, web]
}

fn lookup(lib: &[Fit]) -> impl Fn(&str) -> Option<Fit> + '_ {
    move |id| lib.iter().find(|f| f.name == id).cloned()
}

#[test]
fn fleet_stats_sum_per_ship_numbers_by_count() {
    let d = dataset();
    let lib = library();
    let f = fleet(20, vec![slot("brawler", CountRule::Fill { min: 1, max: 100 }), slot("logi", CountRule::Fixed(4))]);
    let s = stats(&d, &f, &lookup(&lib), &|_| SkillSet::all_v());
    assert_eq!(s.ships, 20);
    assert_eq!(s.slots.iter().map(|x| x.count).collect::<Vec<_>>(), vec![16, 4]);
    let one = s.slots[0].computed.dps();
    assert!((s.dps - one * 16.0).abs() < 1e-6, "{} vs {}", s.dps, one * 16.0);
    assert!((s.logi_rep - 4.0 * 60.0 / 6.0).abs() < 1e-6, "{}: four remote reps of 60 per 6 s", s.logi_rep);
    assert!(s.speed_min > 0.0 && s.speed_min <= s.speed_max);
    assert_eq!(s.main().unwrap().name, "brawler");
}

#[test]
fn boosters_bursts_reach_every_ship_in_the_fleet() {
    let mut d = dataset();
    d.buffs.push(crate::dogma::BuffDef { id: 12, name: "Shield HP".into(), aggregate: crate::dogma::Aggregate::Max, op: crate::dogma::Op::PostPercent, item: vec![attr::ARMOR_HP], location: vec![], location_group: vec![], location_skill: vec![] });
    let d = d.finish();
    let mut lib = library();
    let mut booster = Fit::new(ids::SHIP, "booster");
    booster.modules.push(Module::new(ids::BURST, Slot::High, 0).with_charge(ids::BURST_CHARGE).with_state(State::Active));
    lib.push(booster.clone());
    assert_eq!(bursts_of(&d, &booster, &SkillSet::all_v()), vec![(12, 25.0)]);
    booster.modules[0].charge = None;
    assert!(bursts_of(&d, &booster, &SkillSet::all_v()).is_empty(), "no charge, no burst");
    let plain = fleet(10, vec![slot("brawler", CountRule::Fill { min: 1, max: 100 })]);
    let mut boosted = plain.clone();
    boosted.slots.push(slot("booster", CountRule::Fixed(1)));
    boosted.boosters.push((BurstKind::Shield, "booster".into()));
    let a = stats(&d, &plain, &lookup(&lib), &|_| SkillSet::all_v());
    let b = stats(&d, &boosted, &lookup(&lib), &|_| SkillSet::all_v());
    let (ehp_a, ehp_b) = (a.slots[0].computed.tank.armor.hp, b.slots[0].computed.tank.armor.hp);
    assert!((ehp_b - ehp_a * 1.25).abs() < 1e-6, "{ehp_b} vs {ehp_a}: the one 25% buff lands in full, the penalty only bites the second of a kind");
}

#[test]
fn a_fleet_against_another_lands_less_at_range_and_more_with_ewar() {
    let d = dataset();
    let lib = library();
    let mut a = fleet(30, vec![slot("brawler", CountRule::Fill { min: 1, max: 100 })]);
    let b = fleet(30, vec![slot("kiter", CountRule::Fill { min: 1, max: 100 }), slot("logi", CountRule::Fixed(5))]);
    let (sa, mut sb) = (stats(&d, &a, &lookup(&lib), &|_| SkillSet::all_v()), stats(&d, &b, &lookup(&lib), &|_| SkillSet::all_v()));
    // The enemy main hull sits still for the baseline; its own speed is tried below.
    sb.slots[0].computed.mobility.max_velocity = 0.0;
    a.range = 500.0;
    let close = against(&d, &a, &sa, &b, &sb);
    assert!(close.applied_dps > 0.0);
    assert!((close.applied_dps - sa.dps * 0.5 * (1.0 + 0.98 + 0.0501)).abs() < 1e-6, "still target in optimal: every shot hits");
    let eff = f64::from(a.volley_efficiency);
    assert_eq!(close.critical_mass, Some((sb.main().unwrap().computed.tank.ehp / (sa.slots[0].computed.volley() * eff)).ceil() as u32));
    let landing: f64 = reps_on_one(&sb, &|_, s| s.count).iter().sum();
    assert!(landing < sb.logi_rep, "five logi on one ship land less than their sum");
    assert!((close.break_point - (landing * f64::from(b.logi_efficiency) + sb.main().unwrap().computed.tank.reps.armor)).abs() < 1e-9, "their logistics at their efficiency, lessened as they pile up");
    assert!((close.margin - (close.applied_dps * eff - close.break_point)).abs() < 1e-9);
    a.range = 20_000.0;
    let far = against(&d, &a, &sa, &b, &sb);
    assert!(far.applied_dps < close.applied_dps * 0.2, "{} vs {}", far.applied_dps, close.applied_dps);
    let mut b_fast = b.clone();
    b_fast.ewar.tracking_disruptors = 2;
    b_fast.ewar_auto = false;
    let disrupted = against(&d, &a, &sa, &b_fast, &sb);
    assert!(disrupted.applied_dps < far.applied_dps);
    a.range = 500.0;
    let mut moving = sb.clone();
    moving.slots[0].computed.mobility.max_velocity = 2000.0;
    let fast = against(&d, &a, &sa, &b, &moving);
    assert!(fast.applied_dps < close.applied_dps, "a fast target takes less");
    a.ewar.webs = 2;
    a.ewar.painters = 1;
    a.ewar_auto = false;
    let webbed = against(&d, &a, &sa, &b, &moving);
    assert!(webbed.applied_dps > fast.applied_dps);
    let (v, sig) = ewar_on_target(&a.ewar, 2000.0, 100.0);
    assert!((v - 2000.0 * 0.4 * (1.0 - 0.6 * crate::calc::PENALTY_FACTOR)).abs() < 1e-6);
    assert!((sig - 130.0).abs() < 1e-9);
    assert_eq!(webbed.target_velocity, v);
    a.volley_efficiency = 1.0;
    let full = against(&d, &a, &sa, &b, &sb);
    assert!(full.critical_mass.unwrap() <= close.critical_mass.unwrap());
    let json = serde_json::to_string(&a).unwrap();
    assert_eq!(serde_json::from_str::<Fleet>(&json).unwrap(), a);
}

#[test]
fn a_fleets_ewar_comes_from_its_ships_unless_typed() {
    let d = dataset();
    let lib = library();
    let mut a = fleet(30, vec![slot("brawler", CountRule::Fill { min: 1, max: 100 }), slot("webber", CountRule::Fixed(4))]);
    let b = fleet(30, vec![slot("kiter", CountRule::Fill { min: 1, max: 100 })]);
    let (sa, mut sb) = (stats(&d, &a, &lookup(&lib), &|_| SkillSet::all_v()), stats(&d, &b, &lookup(&lib), &|_| SkillSet::all_v()));
    assert_eq!(sa.projected, Ewar { webs: 4, painters: 0, tracking_disruptors: 0 }, "four webbers, one web each");
    assert_eq!(ewar_of(&a, &sa), sa.projected);
    sb.slots[0].computed.mobility.max_velocity = 2000.0;
    a.range = 500.0;
    let auto = against(&d, &a, &sa, &b, &sb);
    assert!(auto.target_velocity < 2000.0 * 0.5, "{}: the fleet's webs slow the target without being typed", auto.target_velocity);
    a.ewar_auto = false;
    let typed = against(&d, &a, &sa, &b, &sb);
    assert_eq!(typed.target_velocity, 2000.0, "typed ewar, none typed: nothing on the target");
    a.ewar.webs = 1;
    assert_eq!(ewar_of(&a, &sa).webs, 1);
}

#[test]
fn a_fleet_from_a_doctrine_takes_roles_from_the_hulls_and_boosters_from_the_bursts() {
    let d = dataset();
    let mut booster = Fit::new(ids::SHIP, "booster");
    booster.modules.push(Module::new(ids::BURST, Slot::High, 0).with_charge(ids::BURST_CHARGE).with_state(State::Active));
    let fits = vec![("d-logi".to_owned(), Fit::new(ids::SHIP, "logi")), ("d-dps".to_owned(), gun_fit("dps", 2)), ("d-boost".to_owned(), booster)];
    let f = from_doctrine(&d, "Hammer", &fits, &SkillSet::all_v());
    assert_eq!(f.name, "Hammer");
    assert_eq!(f.slots.iter().map(|s| s.fit_id.as_str()).collect::<Vec<_>>(), vec!["d-logi", "d-dps", "d-boost"]);
    assert!(f.slots.iter().all(|s| matches!(s.rule, CountRule::Fill { .. })), "the synthetic hull is no logi or command ship: every slot fills");
    assert_eq!(f.main, Some(0));
    assert_eq!(f.boosters, vec![(BurstKind::Shield, "d-boost".to_owned())]);
    assert!(f.ewar_auto);
    assert_eq!(role_rule(&d, 0), CountRule::Fill { min: 0, max: 1000 });
}

#[test]
fn a_long_range_fleet_is_favoured_far_out_and_a_webbing_brawler_up_close() {
    let d = dataset();
    let mut sniper = gun_fit("sniper", 3);
    for m in &mut sniper.modules {
        if m.type_id == ids::GUN {
            m.charge = Some(ids::AMMO_LONG);
        }
    }
    let mut brawler = gun_fit("brawler", 3);
    brawler.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
    let lib = vec![sniper, brawler];
    let skills = |_: Option<&str>| SkillSet::all_v();
    let a = Fleet { size: 10, slots: vec![slot("sniper", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let b = Fleet { size: 10, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let (sa, sb) = (stats(&d, &a, &lookup(&lib), &skills), stats(&d, &b, &lookup(&lib), &skills));
    let side_a = sweep::Side { fleet: &a, stats: &sa, ammo: &[] };
    let side_b = sweep::Side { fleet: &b, stats: &sb, ammo: &[] };
    let ranges = sweep::ranges(&sa, &sb);
    let s = sweep::sweep(&side_a, &side_b, &ranges);
    assert!(s.a_best > s.b_best, "the long-range fleet peaks farther out: {} against {}", s.a_best, s.b_best);
    let close = s.points.iter().find(|p| p.distance > 0.0).unwrap();
    assert!(close.a.webbed, "up close the brawlers' webs reach");
    let far = s.points.last().unwrap();
    assert!(!far.a.webbed);
    assert!(s.a_favoured.iter().any(|(_, to)| *to >= far.distance - 1.0), "far out is the sniper's: {:?}", s.a_favoured);
    assert!(s.b_cutoffs.iter().any(|(k, _, _)| *k == RemoteKind::Web));
}

#[test]
fn the_primary_is_the_first_role_of_the_order_present() {
    let d = dataset();
    let lib = library();
    let skills = |_: Option<&str>| SkillSet::all_v();
    let enemy = Fleet { size: 10, main: Some(0), slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 }), slot("logi", CountRule::Fixed(2))], ..Fleet::default() };
    let se = stats(&d, &enemy, &lookup(&lib), &skills);
    let dps_first = Fleet::default();
    assert_eq!(sweep::primary(&dps_first, &se).map(|s| s.role), Some(Role::Main), "damage first by default");
    let logi_first = Fleet { target_order: vec![Role::Support, Role::Main], ..Fleet::default() };
    assert_eq!(sweep::primary(&logi_first, &se).map(|s| s.fit_id.as_str()), Some("logi"), "a non-damage slot without a logi hull is support");
}

fn race_between(a: &Fleet, b: &Fleet, lib: &[Fit], distance: f64) -> race::Race {
    let d = dataset();
    let skills = |_: Option<&str>| SkillSet::all_v();
    let (sa, sb) = (stats(&d, a, &lookup(lib), &skills), stats(&d, b, &lookup(lib), &skills));
    let side_a = sweep::Side { fleet: a, stats: &sa, ammo: &[] };
    let side_b = sweep::Side { fleet: b, stats: &sb, ammo: &[] };
    race::race(&d, &side_a, &side_b, distance, 3600.0)
}

#[test]
fn the_bigger_fleet_wins_the_race_and_logi_keep_a_fleet_standing_longer() {
    let lib = library();
    let tight = Formation::Ball { radius_m: 0.0 };
    let big = Fleet { size: 20, formation: tight, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let small = Fleet { size: 5, formation: tight, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let r = race_between(&big, &small, &lib, 500.0);
    assert_eq!(r.winner, Some(0), "twenty against five: {:?}", r.events.iter().take(5).collect::<Vec<_>>());
    assert!(r.events.iter().any(|(_, e)| matches!(e, race::Event::Kill { side: 1, .. })));
    let bare = race_between(&small, &big, &lib, 500.0);
    let with_logi = Fleet { size: 5, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 }), slot("logi", CountRule::Fixed(3))], formation: tight, ..Fleet::default() };
    let helped = race_between(&big, &with_logi, &lib, 500.0);
    let first_kill = |r: &race::Race, side: usize| r.events.iter().find(|(_, e)| matches!(e, race::Event::Kill { side: s, .. } if *s == side)).map(|(t, _)| *t);
    let unhelped = race_between(&big, &small, &lib, 500.0);
    // The logi side's brawlers last at least as long; the logi are not shot first by default.
    assert!(first_kill(&helped, 1).unwrap_or(f64::MAX) >= first_kill(&unhelped, 1).unwrap_or(f64::MAX), "logi repair the primary");
    assert!(!r.verdicts[1].is_empty(), "every target slot gets a verdict");
    let _ = bare;
}

#[test]
fn a_generic_booster_boosts_everyone_without_flying_in_the_fleet() {
    let d = dataset();
    let lib = library();
    let skills = |_: Option<&str>| SkillSet::all_v();
    let plain = Fleet { size: 10, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let mut boosted = plain.clone();
    boosted.boost_shortcuts.push(crate::fit::BoostShortcut { kind: BurstKind::Shield, ..Default::default() });
    let (a, b) = (stats(&d, &plain, &lookup(&lib), &skills), stats(&d, &boosted, &lookup(&lib), &skills));
    assert_eq!(a.ships, b.ships, "the generic booster is no ship of the fleet");
    assert!(crate::fleet::shortcut_booster(&d, &boosted.boost_shortcuts[0]).is_some());
    assert_ne!(a.env, b.env, "its bursts reach every ship");
    boosted.boost_shortcuts[0].active = false;
    assert_eq!(stats(&d, &boosted, &lookup(&lib), &skills).env, a.env, "switched off it gives nothing");
}

#[test]
fn drones_add_damage_per_second_but_no_alpha_and_switch_off() {
    let d = dataset();
    let mut f = gun_fit("dronie", 1);
    f.drones.push(crate::fit::DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, ..Default::default() });
    let lib = vec![f.clone(), gun_fit("plain", 1)];
    let skills = |_: Option<&str>| SkillSet::all_v();
    let with = Fleet { size: 10, slots: vec![slot("dronie", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let plain = Fleet { size: 10, slots: vec![slot("plain", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let (sw, sp) = (stats(&d, &with, &lookup(&lib), &skills), stats(&d, &plain, &lookup(&lib), &skills));
    assert!(sw.dps > sp.dps, "drones add damage per second");
    assert!((sw.volley - sp.volley).abs() < 1e-9, "but no alpha: {} against {}", sw.volley, sp.volley);
    let off = Fleet { drones: false, ..with };
    assert!((stats(&d, &off, &lookup(&lib), &skills).dps - sp.dps).abs() < 1e-9, "switched off, no drone damage");
}

#[test]
fn a_slot_flies_its_picked_variation_or_its_base() {
    use crate::variations::{RefitOp, Variation};
    let d = dataset();
    let mut f = gun_fit("v", 3);
    f.variations.push(Variation { id: 7, name: "no guns".into(), ops: (0..3).map(|i| RefitOp::Replace { slot: Slot::High, index: i, with: None }).collect(), ..Default::default() });
    f.active_variations = vec![7];
    let lib = vec![f];
    let skills = |_: Option<&str>| SkillSet::all_v();
    let mut fl = Fleet { size: 1, slots: vec![slot("v", CountRule::Fixed(1))], ..Fleet::default() };
    assert!(stats(&d, &fl, &lookup(&lib), &skills).dps > 0.0, "the base, whatever is switched on in the editor");
    fl.slots[0].variation = Some(7);
    assert_eq!(stats(&d, &fl, &lookup(&lib), &skills).dps, 0.0, "the picked variation");
}

#[test]
fn a_wider_ball_spreads_the_angular_velocities_the_guns_see() {
    let d = dataset();
    let lib = library();
    let skills = |_: Option<&str>| SkillSet::all_v();
    let mk = |r: f32| Fleet { size: 20, formation: Formation::Ball { radius_m: r }, motion: (1.0, 90.0), slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let spread = |r: f32| {
        let f = mk(r);
        let s = stats(&d, &f, &lookup(&lib), &skills);
        let side = sweep::Side { fleet: &f, stats: &s, ammo: &[] };
        let v = sweep::angular(&side, &side, 20_000.0);
        v.last().unwrap() - v.first().unwrap()
    };
    assert!(spread(8_000.0) > spread(0.0), "a wider ball, a wider spread of angles");
    let f = mk(0.0);
    let s = stats(&d, &f, &lookup(&lib), &skills);
    let side = sweep::Side { fleet: &f, stats: &s, ammo: &[] };
    let half = sweep::half_hit_angular(&side, &side, &s.slots[0], 20_000.0);
    assert!(!half.is_empty() && half.iter().all(|(_, _, a)| *a > 0.0));
}

#[test]
fn a_mirror_match_races_even() {
    let lib = library();
    let f = Fleet { size: 10, formation: Formation::Ball { radius_m: 0.0 }, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let r = race_between(&f, &f, &lib, 500.0);
    let lost = |side: usize| r.events.iter().filter(|(_, e)| matches!(e, race::Event::Kill { side: s, .. } if *s == side)).count();
    assert_eq!(lost(0), lost(1), "neither side shoots first: {:?}", r.winner);
    assert_eq!(r.winner, None);
}

#[test]
fn the_summary_can_aim_at_any_slot_of_the_target() {
    let d = dataset();
    let lib = library();
    let skills = |_: Option<&str>| SkillSet::all_v();
    let a = Fleet { size: 10, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let b = Fleet { size: 10, main: Some(0), slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 }), slot("logi", CountRule::Fixed(2))], ..Fleet::default() };
    let (sa, sb) = (stats(&d, &a, &lookup(&lib), &skills), stats(&d, &b, &lookup(&lib), &skills));
    assert_eq!(against(&d, &a, &sa, &b, &sb), against_slot(&d, &a, &sa, &b, &sb, 0), "the main hull by default");
    let on_logi = against_slot(&d, &a, &sa, &b, &sb, 1);
    let ehp = |k: usize| sb.slots[k].computed.tank.ehp;
    assert!((on_logi.kills_per_volley * ehp(1) - against_slot(&d, &a, &sa, &b, &sb, 0).kills_per_volley * ehp(0)).abs() < 1e-6, "the same volley over the other ship's hit points");
}

#[test]
fn hulls_fall_in_size_classes_by_group() {
    let d = dataset();
    assert_eq!(hull_class(&d, ids::SHIP), HullClass::Frigate, "the fixture's frigate");
    assert_eq!(hull_class(&d, 0), HullClass::Industrial, "anything unknown");
}

#[test]
fn application_gives_the_charge_the_targets_hit_points_against_it_and_the_spread() {
    let d = dataset();
    let lib = library();
    let skills = |_: Option<&str>| SkillSet::all_v();
    let a = Fleet { size: 20, formation: Formation::Flock { sigma_m: 6000.0 }, motion: (1.0, 90.0), slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let b = Fleet { size: 20, motion: (1.0, 90.0), slots: vec![slot("kiter", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let (sa, sb) = (stats(&d, &a, &lookup(&lib), &skills), stats(&d, &b, &lookup(&lib), &skills));
    let (side_a, side_b) = (sweep::Side { fleet: &a, stats: &sa, ammo: &[] }, sweep::Side { fleet: &b, stats: &sb, ammo: &[] });
    let apps = sweep::application(&side_a, &side_b, &sb.slots[0], 3_000.0);
    assert!(!apps.is_empty());
    for x in &apps {
        assert_eq!(x.weapon.charge, Some(ids::AMMO), "the loaded charge without other options");
        assert!(x.low <= x.mean && x.mean <= x.high && x.high <= 1.0 && x.low >= 0.0, "{} {} {}", x.low, x.mean, x.high);
        assert!(x.target_ehp > 0.0 && x.target_ehp.is_finite());
    }
    assert!(apps[0].high > apps[0].low, "a spread swarm crossing sees a spread of application");
    // Against damage it does not resist, the hit points are the raw ones.
    let t = &sb.slots[0].computed.tank;
    let raw = t.shield.hp + t.armor.hp + t.hull.hp;
    assert!(sweep::ehp_against(t, &crate::stats::Damage { em: 1.0, ..Default::default() }) >= raw - 1e-6);
}

#[test]
fn logi_repair_only_after_damage_reaction_lock_and_cycle_and_at_their_efficiency() {
    let lib = library();
    let tight = Formation::Ball { radius_m: 0.0 };
    let big = Fleet { size: 30, formation: tight, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 })], ..Fleet::default() };
    let with_logi = |eff: f32| Fleet { size: 8, formation: tight, logi_efficiency: eff, slots: vec![slot("brawler", CountRule::Fill { min: 0, max: 100 }), slot("logi", CountRule::Fixed(4))], ..Fleet::default() };
    let first_kill = |r: &race::Race| r.events.iter().find(|(_, e)| matches!(e, race::Event::Kill { side: 1, .. })).map(|(t, _)| *t).unwrap_or(f64::MAX);
    let lost = |r: &race::Race| r.events.iter().filter(|(_, e)| matches!(e, race::Event::Kill { side: 1, .. })).count();
    let none = race_between(&big, &with_logi(0.0), &lib, 500.0);
    let full = race_between(&big, &with_logi(1.0), &lib, 500.0);
    let usual = race_between(&big, &with_logi(0.75), &lib, 500.0);
    assert!(first_kill(&full) >= first_kill(&none), "logi slow the first kill, if at all");
    assert!(first_kill(&usual) <= first_kill(&full) + 1e-9, "at 75% they hold less than at full");
    assert!(lost(&usual) > 0, "thirty against eight with four logi still kill");
    // The first primary dies before any repair lands: reps start only once it is hurt, then each
    // logi ship needs its reaction, its lock and, for armor, a whole cycle.
    assert!(matches!(usual.verdicts[1].iter().find(|(k, _)| *k == 0).map(|(_, v)| *v), Some(race::Verdict::BeforeReps { .. }) | Some(race::Verdict::OneVolley)), "{:?}", usual.verdicts[1]);
}

#[test]
fn remote_repairs_lose_effect_as_they_pile_up_as_the_game_does() {
    let n = |count: usize, per_s: f64| vec![(per_s * 5.0, 5.0); count];
    // One repair alone lands whole.
    assert!((rep_landing(&n(1, 400.0)) - 400.0).abs() < 1e-9);
    // The 2019 figures, with the multiplier of then: fifteen 1600 HP/s repairs at about 80%,
    // half at thirty-eight.
    let then = |reps: &[(f64, f64)]| {
        let total: f64 = reps.iter().map(|(a, c)| a / c.floor()).sum();
        let (a, c) = reps[0];
        let r = a / c.floor();
        let m = REP_ADDITION + r * 10.0;
        1.0 - ((r + m) / (total + m) - 1.0).powi(2)
    };
    assert!((then(&n(15, 1600.0)) - 0.77).abs() < 0.01);
    assert!((then(&n(38, 1600.0)) - 0.50).abs() < 0.01);
    // Today's: twenty logi cruisers of four 85 HP/s repairs on one ship land about 81%.
    let cruisers = rep_landing(&n(80, 85.0)) / (80.0 * 85.0);
    assert!((cruisers - 0.81).abs() < 0.01, "{cruisers}");
    // More repairs, more landing, each worth less.
    let (a, b, c) = (rep_landing(&n(20, 400.0)), rep_landing(&n(40, 400.0)), rep_landing(&n(60, 400.0)));
    assert!(a < b && b < c && (c - b) < (b - a));
}

#[test]
fn smartbombs_count_only_on_a_ship_with_nothing_else() {
    let d = dataset();
    let mut gunner = gun_fit("gunner", 2);
    gunner.modules.push(Module::new(ids::SMARTBOMB, Slot::High, 5).with_state(State::Active));
    let mut bomber = Fit::new(ids::SHIP, "bomber");
    bomber.modules.push(Module::new(ids::SMARTBOMB, Slot::High, 0).with_state(State::Active));
    let lib = vec![gunner, bomber];
    let skills = |_: Option<&str>| SkillSet::all_v();
    let one = |id: &str| stats(&d, &Fleet { size: 1, slots: vec![slot(id, CountRule::Fixed(1))], ..Fleet::default() }, &lookup(&lib), &skills);
    let g = one("gunner");
    assert!(g.slots[0].computed.weapons.iter().all(|w| w.kind != crate::stats::WeaponKind::Smartbomb), "beside guns a smartbomb is drone defence");
    assert!(!g.slots[0].computed.weapons.is_empty());
    let b = one("bomber");
    assert!(b.slots[0].computed.weapons.iter().any(|w| w.kind == crate::stats::WeaponKind::Smartbomb), "a smartbomb boat fights with them");
}
