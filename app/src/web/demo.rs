//! A snapshot built entirely from fixtures.
//!
//! Every web ticket after this one needs a browser screenshot in its `after/` folder, and those
//! folders are committed and pushed to a public repo. Alliance chat is operational information, so a
//! screenshot taken against the running app would publish room names, pilots and fleet movements.
//! This is the same rule `harness::assert_no_live_profile` enforces for the egui renders, applied to
//! the web surface: **the demo serves fixtures and nothing else.**

use std::collections::{BTreeMap, HashMap};

use super::snapshot::*;
use super::state::{hash_of, Pane, SharedWeb};

/// 1DQ1-A in the fixture graph, where the fixture player sits.
const HOME: i64 = 30_004_759;

pub fn seed(web: &SharedWeb, tick: u64) {
    let reports = reports_for(tick);
    let cards = cards(&reports);
    let alerts = alerts(&cards);
    let pings = pings(tick);
    let meta = meta();
    let map = map(&cards);

    let mut st = web.lock().unwrap_or_else(|e| e.into_inner());
    let lookups = Lookups {
        resolved_pilots: crate::uitest::fixtures::resolved_pilots().into_iter().collect(),
        uncertain: crate::uitest::fixtures::uncertain(),
        ..Default::default()
    };
    if let Some(rev) = st.changed(Pane::Intel, hash_of(&(&cards, &lookups))) {
        st.put_intel(IntelPane { rev, cards, lookups });
    }
    if let Some(rev) = st.changed(Pane::Alerts, hash_of(&alerts.feed)) {
        st.put_alerts(AlertPane { rev, msg: alerts });
    }
    if let Some(rev) = st.changed(Pane::Pings, hash_of(&pings)) {
        let systems = crate::uitest::fixtures::systems();
        let names = systems
            .info_of(HOME)
            .map(|i| BTreeMap::from([(HOME, i.name.clone())]))
            .unwrap_or_default();
        st.put_pings(PingPane { rev, pings, systems: names });
    }
    if let Some(rev) = st.changed(Pane::Map, hash_of(&map)) {
        st.put_map(MapLive { rev, ..map });
    }
    if let Some(rev) = st.changed(Pane::Meta, hash_of(&meta)) {
        st.put_meta(Meta { rev, ..meta });
    }
    let sys = BTreeMap::from([
        (
            HOME,
            super::snapshot::SysInfo {
                adm: Some(5.8),
                k: 3,
                n: 140,
                j: 62,
                sov: Some("#9b6fd8".to_owned()),
                coal: Some("#4f9bd8".to_owned()),
                ..Default::default()
            },
        ),
        (
            30_004_608,
            super::snapshot::SysInfo { adm: Some(2.1), p: 2, j: 18, ..Default::default() },
        ),
    ]);
    if let Some(rev) = st.changed(Pane::Status, hash_of(&sys)) {
        st.put_status(super::snapshot::StatusPane { rev, systems: sys });
    }
}

/// Grows by one report per tick, then wraps. A static page proves the layout and nothing else: a
/// changing one proves the push channel, the age clock and, once WEB-009 lands, the sound.
fn reports_for(tick: u64) -> Vec<crate::intel::IntelReport> {
    use crate::uitest::fixtures as f;
    let all = [
        f::intel_typical(),
        f::intel_next_door(),
        f::intel_across_the_bridge(),
        f::intel_torture(),
        f::intel_clear(),
        f::intel_two_celestials(),
    ];
    let n = (tick as usize % all.len()) + 1;
    all[..n].to_vec()
}

fn cards(reports: &[crate::intel::IntelReport]) -> Vec<IntelCard> {
    let systems = Some(crate::uitest::fixtures::systems());
    let locations = HashMap::from([("Amryu".to_owned(), (HOME, false))]);
    let chars = vec![("Amryu".to_owned(), 0_i64)];
    let rings = crate::app::build_char_rings(
        &systems,
        &chars,
        &locations,
        "Amryu",
        Some(HOME),
        &[],
        false,
        false,
    );
    let severity = crate::settings::SeverityRules::default();
    let mut cards: Vec<IntelCard> = reports
        .iter()
        .map(|r| {
            let target = r.primary_system().map(|s| s.id);
            let from_you = crate::app::jumps_from_you(&systems, Some(HOME), target, false);
            IntelCard {
                severity: crate::app::severity_of(r, &severity),
                from_you,
                via: crate::app::jump_via(&systems, Some(HOME), target, false, from_you),
                chars: rings.card(target),
                report: r.clone(),
            }
        })
        .collect();
    cards.sort_by(|a, b| b.report.received.cmp(&a.report.received));
    cards
}

/// The alert feed: the cards a rule would have fired on, which for the fixtures is anything above
/// Info. Without this the alerts pane renders empty in every screenshot, and WEB-007 would land with
/// no way to see whether it draws anything at all.
fn alerts(cards: &[IntelCard]) -> crate::ipc::AlertMsg {
    let feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> = cards
        .iter()
        .filter(|c| c.severity > crate::settings::Severity::Info)
        .map(|c| (c.report.clone(), c.severity))
        .collect();
    let n = feed.len();
    crate::ipc::AlertMsg {
        from_you: cards.iter().take(n).map(|c| c.from_you).collect(),
        via: cards.iter().take(n).map(|c| c.via).collect(),
        chars: cards.iter().take(n).map(|c| c.chars.clone()).collect(),
        feed,
        status: Default::default(),
        resolved_pilots: crate::uitest::fixtures::resolved_pilots().into_iter().collect(),
        uncertain: crate::uitest::fixtures::uncertain(),
        last_ship: Default::default(),
        kills: Default::default(),
        affil: Default::default(),
        secs: 0.0,
        focus: false,
    }
}

fn pings(tick: u64) -> Vec<PingCard> {
    use crate::uitest::fixtures as f;
    let all = [f::ping_fleet(), f::ping_plain(), f::ping_plain_multiline()];
    let n = (tick as usize % all.len()) + 1;
    all[..n]
        .iter()
        .map(|p| PingCard { ping: p.clone(), rule: None, suppressed: false })
        .collect()
}

fn map(cards: &[IntelCard]) -> MapLive {
    let mut intel: Vec<(i64, u8, i64)> = Vec::new();
    for c in cards {
        let Some(sys) = c.report.primary_system() else { continue };
        intel.push((sys.id, c.severity as u8, c.report.received));
    }
    intel.sort_unstable();
    intel.dedup_by_key(|(id, _, _)| *id);
    MapLive {
        rev: 0,
        you: Some(HOME),
        chars: vec![(HOME, 1)],
        intel,
        camps: vec![30_004_608],
        holes: vec![(30_003_704, 30_000_142)],
        cyno: vec![30_003_704],
        route: vec![HOME, 30_004_608, 30_003_704],
        upgrades: vec![(
            HOME,
            vec![
                super::snapshot::UpgradeMark { k: 0, l: 5, ore: None },
                super::snapshot::UpgradeMark { k: 2, l: 3, ore: Some(34) },
            ],
        )],
    }
}

fn meta() -> Meta {
    Meta {
        rev: 0,
        version: env!("CARGO_PKG_VERSION"),
        theme: crate::theme::Theme::caldari(),
        compact: false,
        intel_ttl_secs: 3600,
        intel_max_jumps: 0,
        count_bridges: false,
        allow_writeback: true,
        active_character: "Amryu".to_owned(),
        chars: vec![("Amryu".to_owned(), 0)],
        player_system: Some(HOME),
        sounds: BTreeMap::from([
            ("Warning".to_owned(), "warning".to_owned()),
            ("Danger".to_owned(), "danger".to_owned()),
            ("Critical".to_owned(), "critical".to_owned()),
        ]),
        sound_rev: crate::sound::SYNTH_REV,
    }
}

/// A four-system map from the same fixture graph the cards use, so the map pane has something with
/// real edges to draw.
pub fn map_geometry() -> super::map::Geometry {
    let systems = crate::uitest::fixtures::systems();
    let coords = [
        (30_004_759_i64, "1DQ1-A", -0.36, 0.0, 0.0),
        (30_004_608, "319-3D", -0.41, 120.0, 60.0),
        (30_003_704, "7-K5EL", -0.29, 240.0, 10.0),
        (30_000_142, "Jita", 0.95, 40.0, 200.0),
    ];
    let rows: Vec<crate::store::MapSystem> = coords
        .iter()
        .map(|&(id, name, sec, x, z)| crate::store::MapSystem {
            id,
            name: name.to_owned(),
            security: sec,
            region_id: 10_000_060,
            x,
            y: 0.0,
            z,
            x2d: x,
            z2d: z,
        })
        .collect();
    super::map::build(&rows, &systems)
}

/// The dialog sources, from the same fixture graph as everything else.
pub fn detail() -> super::Detail {
    let d = super::detail();
    d.lock().unwrap().graph = Some(crate::uitest::fixtures::systems());
    d.lock().unwrap().player_sys = Some(HOME);
    d
}
