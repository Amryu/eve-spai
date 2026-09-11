//! A snapshot built entirely from fixtures.
//!
//! Every web ticket after this one needs a browser screenshot in its `after/` folder, and those
//! folders are committed and pushed to a public repo. Alliance chat is operational information, so a
//! screenshot taken against the running app would publish room names, pilots and fleet movements.
//! This is the same rule `harness::assert_no_live_profile` enforces for the egui renders, applied to
//! the web surface: **the demo serves fixtures and nothing else.**

use std::collections::HashMap;

use super::snapshot::*;
use super::state::{hash_of, Pane, SharedWeb};

/// 1DQ1-A in the fixture graph, where the fixture player sits.
const HOME: i64 = 30_004_759;

pub fn seed(web: &SharedWeb, tick: u64) {
    let reports = reports_for(tick);
    let cards = cards(&reports);
    let pings = pings(tick);
    let meta = meta();
    let map = map(&cards);

    let mut st = web.lock().unwrap_or_else(|e| e.into_inner());
    let lookups = Lookups {
        resolved_pilots: crate::uitest::fixtures::resolved_pilots(),
        uncertain: crate::uitest::fixtures::uncertain(),
        ..Default::default()
    };
    if let Some(rev) = st.changed(Pane::Intel, hash_of(&(&cards, &lookups))) {
        st.put_intel(IntelPane { rev, cards, lookups });
    }
    if let Some(rev) = st.changed(Pane::Pings, hash_of(&pings)) {
        st.put_pings(PingPane { rev, pings });
    }
    if let Some(rev) = st.changed(Pane::Map, hash_of(&map)) {
        st.put_map(MapLive { rev, ..map });
    }
    if let Some(rev) = st.changed(Pane::Meta, hash_of(&meta)) {
        st.put_meta(Meta { rev, ..meta });
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
    MapLive { rev: 0, you: Some(HOME), chars: vec![(HOME, 1)], intel }
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
    }
}
