use std::collections::HashMap;
use std::sync::Arc;

use crate::geo::{SystemInfo, Systems};
use crate::intel::{DetectedShip, DetectedSystem, IntelReport};

/// Clock for every scene, stamped once per run. Most drawing functions take `now` as a parameter,
/// but the alert and ping windows read the wall clock themselves, so a fixed epoch would render
/// every age as several thousand hours. Snapshots are inspected, never diffed, so drift is fine.
pub(crate) fn now() -> i64 {
    static NOW: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *NOW.get_or_init(|| chrono::Utc::now().timestamp())
}

pub(crate) fn systems() -> Arc<Systems> {
    let mut by_name = HashMap::new();
    for (id, name, security, region) in [
        (30_004_759_i64, "1DQ1-A", -0.36_f64, "Delve"),
        (30_004_608, "319-3D", -0.41, "Delve"),
        (30_003_704, "7-K5EL", -0.29, "Fountain"),
        (30_000_142, "Jita", 0.95, "The Forge"),
    ] {
        by_name.insert(
            name.to_owned(),
            SystemInfo {
                id,
                name: name.to_owned(),
                security,
                constellation: "O-EImg".into(),
                region: region.to_owned(),
                faction: String::new(),
            },
        );
    }
    let adjacency = HashMap::from([
        (30_004_759, vec![30_004_608]),
        (30_004_608, vec![30_004_759, 30_003_704]),
        (30_003_704, vec![30_004_608]),
    ]);
    Arc::new(Systems::new(by_name, adjacency))
}

fn ship(id: i64, name: &str) -> DetectedShip {
    DetectedShip { id, name: name.to_owned() }
}

fn detected(name: &str) -> DetectedSystem {
    let (id, security) = match name {
        "1DQ1-A" => (30_004_759, -0.36),
        "319-3D" => (30_004_608, -0.41),
        "7-K5EL" => (30_003_704, -0.29),
        _ => (30_000_142, 0.95),
    };
    DetectedSystem { id, name: name.to_owned(), security }
}

/// A routine hostile report: one system, a couple of hulls, a handful of named pilots.
pub(crate) fn intel_typical() -> IntelReport {
    IntelReport {
        id: 1,
        received: now() - 45,
        channel: "delve.imperium".into(),
        reporter: "Scout Alpha".into(),
        text: "1DQ1-A  Muninn Loki x3  hostile".into(),
        systems: vec![detected("1DQ1-A")],
        ships: vec![ship(12_005, "Muninn"), ship(29_990, "Loki")],
        pilots: vec!["Hostile Pilot".into(), "Second Target".into()],
        count: Some(3),
        count_ships: 3,
        ..Default::default()
    }
}

/// A clear report, the other end of the visual range: mostly empty.
pub(crate) fn intel_clear() -> IntelReport {
    IntelReport {
        id: 2,
        received: now() - 5,
        channel: "delve.imperium".into(),
        reporter: "Scout Bravo".into(),
        text: "319-3D clr".into(),
        systems: vec![detected("319-3D")],
        clear: true,
        ..Default::default()
    }
}

/// Everything at once, at its longest. This is the fixture that surfaces overlap and overflow:
/// maximal names, every badge flag set, a long pilot list and a large count.
pub(crate) fn intel_torture() -> IntelReport {
    IntelReport {
        id: 3,
        received: now() - 3_600,
        channel: "coalition.intel.broadcast.northern-front".into(),
        reporter: "Reporter With An Extremely Long Character Name".into(),
        text: "7-K5EL  Titan Supercarrier Dreadnought Force Auxiliary  cyno up  camp on gate \
               bubbles up  x150  help needed immediately"
            .into(),
        systems: vec![detected("7-K5EL"), detected("1DQ1-A"), detected("319-3D")],
        ships: vec![
            ship(23_773, "Ragnarok"),
            ship(23_913, "Nyx"),
            ship(19_720, "Revelation"),
            ship(37_604, "Apostle"),
            ship(12_005, "Muninn"),
            ship(29_990, "Loki"),
        ],
        classes: vec!["Supercapital".into(), "Capital".into()],
        pilots: (0..12)
            .map(|i| format!("Very Long Hostile Pilot Name Number {i:02}"))
            .collect(),
        count: Some(150),
        count_extra: Some(150),
        isk: Some(412_000_000_000),
        celestials: vec!["Planet VI - Moon 3 - Blood Raider Chemical Laboratory".into()],
        near_celestial: Some(("Planet VI - Moon 3 - Chemical Laboratory".into(), 42.7)),
        camp: true,
        bubble: true,
        cyno: true,
        help: true,
        spike: true,
        dropper: true,
        cap_tackled: true,
        ..Default::default()
    }
}

/// A kill beside one moon while the same report names a different moon in the same system. The
/// two chips have to both survive, since which moon the hostiles sit at is the point of the badge.
pub(crate) fn intel_two_celestials() -> IntelReport {
    IntelReport {
        id: 5,
        received: now() - 90,
        channel: "delve.imperium".into(),
        reporter: "Scout Delta".into(),
        text: "7-K5EL  Planet VI - Moon 4  3 hostiles".into(),
        systems: vec![detected("7-K5EL")],
        celestials: vec!["Moon 6-4".into()],
        near_celestial: Some(("Planet VI - Moon 3 - Chemical Laboratory".into(), 42.7)),
        count: Some(3),
        ..Default::default()
    }
}

/// Mid-resolution: one pilot already carries a character id, the other is still queued, which is
/// the only state that draws the "Resolving pilot" placeholder chip. `clock` is the `now` the row
/// is drawn with, so the age chip reads the same however far the animation phase has advanced.
pub(crate) fn intel_resolving(clock: i64) -> IntelReport {
    IntelReport {
        id: 4,
        received: clock - 20,
        channel: "delve.imperium".into(),
        reporter: "Scout Charlie".into(),
        text: "1DQ1-A  Muninn  Hostile Pilot  Unresolved Pilot".into(),
        systems: vec![detected("1DQ1-A")],
        ships: vec![ship(12_005, "Muninn")],
        pilots: vec!["Hostile Pilot".into(), "Unresolved Pilot".into()],
        count: Some(2),
        count_ships: 1,
        ..Default::default()
    }
}

pub(crate) fn ship_details() -> HashMap<i64, crate::store::ShipDetails> {
    HashMap::new()
}

pub(crate) fn kills() -> crate::kills::KillCache {
    Arc::new(std::sync::Mutex::new(HashMap::new()))
}

pub(crate) fn affil() -> crate::affiliation::SharedAffil {
    Arc::new(std::sync::Mutex::new(crate::affiliation::AffilCache::default()))
}

/// `intel_row` skips any pilot missing from this map (`app.rs:22099`), so every fixture name has
/// to appear here or the pilot chips never render and the torture case stops being one.
pub(crate) fn resolved_pilots() -> HashMap<String, i64> {
    let mut m = HashMap::from([
        ("Hostile Pilot".to_owned(), 2_112_000_001_i64),
        ("Second Target".to_owned(), 2_112_000_002_i64),
    ]);
    for i in 0..12 {
        m.insert(format!("Very Long Hostile Pilot Name Number {i:02}"), 2_112_001_000 + i);
    }
    m
}

pub(crate) fn uncertain() -> crate::pilot::UncertainPilots {
    ["Second Target", "Very Long Hostile Pilot Name Number 03"].into_iter().collect()
}

/// Args for [`crate::app::intel_row`] that are the same in every scene, so a scene only has to say
/// which report it draws.
pub(crate) struct IntelArgs {
    pub(crate) systems: Option<Arc<Systems>>,
    pub(crate) status: HashMap<i64, crate::systemstatus::SysFlags>,
    pub(crate) ship_details: HashMap<i64, crate::store::ShipDetails>,
    pub(crate) ship_roles: HashMap<i64, Vec<(&'static str, &'static str)>>,
    pub(crate) resolved_pilots: HashMap<String, i64>,
    pub(crate) uncertain: crate::pilot::UncertainPilots,
    pub(crate) last_ship: HashMap<String, (i64, String, i64)>,
    pub(crate) kills: crate::kills::KillCache,
    pub(crate) affil: crate::affiliation::SharedAffil,
}

impl Default for IntelArgs {
    fn default() -> Self {
        Self {
            systems: Some(systems()),
            status: HashMap::new(),
            ship_details: ship_details(),
            ship_roles: HashMap::new(),
            resolved_pilots: resolved_pilots(),
            uncertain: uncertain(),
            last_ship: HashMap::new(),
            kills: kills(),
            affil: affil(),
        }
    }
}

pub(crate) fn ping_fleet() -> crate::pings::Ping {
    crate::pings::Ping::Fleet {
        timestamp: now() - 120,
        description: "Strat op, capitals staged. Bring your own Muninn, logi welcome. \
                      Undock in 5."
            .into(),
        fc: "Fleet Commander".into(),
        fleet: Some("Home Defence".into()),
        formup: vec![
            crate::pings::Formup::System(30_004_759),
            crate::pings::Formup::Text("Keepstar, undock and warp to FC".into()),
        ],
        pap: Some(crate::pings::PapType::Strategic),
        comms: Some(crate::pings::Comms::Mumble {
            channel: "Home Defence".into(),
            link: "mumble://voice.example.invalid/Home%20Defence".into(),
        }),
        doctrine: Some("Muninn".into()),
        source: Some("goonfleet".into()),
        target: Some("all".into()),
        raw: "raw ping body".into(),
    }
}

/// A fleet ping that names no doctrine, which is the case where the doctrine row has nothing to
/// hold at all.
pub(crate) fn ping_fleet_no_doctrine() -> crate::pings::Ping {
    let mut p = ping_fleet();
    if let crate::pings::Ping::Fleet { doctrine, .. } = &mut p {
        *doctrine = None;
    }
    p
}

pub(crate) fn ping_plain() -> crate::pings::Ping {
    crate::pings::Ping::Plain {
        timestamp: now() - 30,
        text: "Reminder: skill queue check before the weekend.".into(),
        sender: Some("Director".into()),
        target: Some("corp".into()),
        raw: "raw plain body".into(),
    }
}
