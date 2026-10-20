use super::*;
use crate::fit::{Module, Slot};
use crate::test_support::{dataset, ids};

fn fleet(size: u32, slots: Vec<FleetSlot>) -> Fleet {
    Fleet { size, slots, ..Fleet::default() }
}

fn slot(fit: &str, rule: CountRule) -> FleetSlot {
    FleetSlot { fit_id: fit.into(), rule, override_count: None, profile: None }
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
    assert!((close.break_point - (sb.logi_rep + sb.main().unwrap().computed.tank.reps.armor)).abs() < 1e-9);
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
