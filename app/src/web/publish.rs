//! The publisher thread.
//!
//! Its own thread because egui parks while the window is minimized, and the phone must keep
//! updating. Not on the alert daemon, to keep an optional feature off the alert critical path.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use super::facts::SharedFacts;
use super::snapshot::*;
use super::state::{hash_of, Pane, SharedWeb};

const TICK: std::time::Duration = std::time::Duration::from_millis(500);

/// Newest reports carried to the page. Matches the app's own card cap.
const CARD_CAP: usize = 250;

/// Fleet pings carried to the page. The app keeps its whole history.
const PING_CAP: usize = 80;

fn ping_time(p: &crate::pings::Ping) -> i64 {
    match p {
        crate::pings::Ping::Fleet { timestamp, .. } => *timestamp,
        crate::pings::Ping::Plain { timestamp, .. } => *timestamp,
    }
}

pub struct Deps {
    pub facts: SharedFacts,
    pub web: SharedWeb,
    pub intel_state: Arc<Mutex<crate::intel::IntelState>>,
    pub pilots: crate::pilot::SharedPilots,
    pub player: crate::esi::SharedPlayer,
    pub system_status: crate::systemstatus::SharedStatus,
    pub jabber: crate::jabber::SharedJabber,
}

pub fn spawn(deps: Deps, alerts: impl Fn() -> crate::ipc::AlertMsg + Send + 'static) {
    let _ = std::thread::Builder::new().name("web-publish".into()).spawn(move || {
        let mut last_notes = 0usize;
        let mut last_inputs: Option<(u64, std::time::Instant)> = None;
        loop {
            // A notes edit publishes on the next slice, since the page is waiting for it.
            const SLICE: std::time::Duration = std::time::Duration::from_millis(40);
            let mut waited = std::time::Duration::ZERO;
            while waited < TICK {
                std::thread::sleep(SLICE);
                waited += SLICE;
                let notes = Arc::as_ptr(&deps.facts.lock().unwrap_or_else(|e| e.into_inner()).notes_view) as usize;
                if notes != last_notes {
                    break;
                }
            }
            let facts = deps.facts.lock().unwrap_or_else(|e| e.into_inner()).clone();
            last_notes = Arc::as_ptr(&facts.notes_view) as usize;
            if !should_publish(&facts) {
                continue;
            }
            // Building the payload walks every card, ring and system. This runs first and only
            // touches counters, so a quiet minute costs nothing.
            let inputs = inputs_hash(&deps, &facts);
            let unchanged = last_inputs
                .as_ref()
                .is_some_and(|(h, at)| *h == inputs && at.elapsed().as_secs_f32() < 3.0);
            if unchanged {
                continue;
            }
            last_inputs = Some((inputs, std::time::Instant::now()));
            tick(&deps, &facts, &alerts());
        }
    });
}

/// What the published payload is built from, cheaply. Resolved names and affiliations are not in
/// it, which is why the publisher also refreshes on a timer.
fn inputs_hash(deps: &Deps, facts: &super::facts::UiFacts) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    {
        let st = deps.intel_state.lock().unwrap_or_else(|e| e.into_inner());
        st.reports.len().hash(&mut h);
        for r in &st.reports {
            r.id.hash(&mut h);
            r.received.hash(&mut h);
        }
    }
    {
        let p = deps.player.lock().unwrap_or_else(|e| e.into_inner());
        p.system_id.hash(&mut h);
        p.locations.len().hash(&mut h);
    }
    deps.system_status.lock().unwrap_or_else(|e| e.into_inner()).len().hash(&mut h);
    deps.jabber.lock().unwrap_or_else(|e| e.into_inner()).pings.len().hash(&mut h);
    std::sync::Arc::as_ptr(&facts.notes_view).hash(&mut h);
    facts.active_character.hash(&mut h);
    h.finish()
}

/// Split out so it is testable, since the loop never returns. The SDE graph arrives well after
/// startup.
fn should_publish(facts: &super::facts::UiFacts) -> bool {
    facts.web_enabled && facts.systems.is_some()
}

fn tick(deps: &Deps, facts: &super::facts::UiFacts, alerts: &crate::ipc::AlertMsg) {
    // Phase 1, copy out. Each lock is dropped before the next, so the `intel_state -> pilots` order
    // cannot be violated.
    let reports: Vec<crate::intel::IntelReport> = {
        let st = deps.intel_state.lock().unwrap_or_else(|e| e.into_inner());
        let n = st.reports.len();
        st.reports[n.saturating_sub(CARD_CAP)..].to_vec()
    };
    let last_ship = crate::app::build_last_ship(&reports);
    let (resolved_pilots, uncertain) = {
        let mut cache = deps.pilots.lock().unwrap_or_else(|e| e.into_inner());
        let rp =
            cache.display_ids(reports.iter().flat_map(|r| r.pilots.iter()).map(|s| s.as_str()));
        let un = crate::app::uncertain_set(&cache, &rp);
        (rp, un)
    };
    let status = deps.system_status.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (player_sys, locations) = {
        let p = deps.player.lock().unwrap_or_else(|e| e.into_inner());
        let sys = p.locations.get(&facts.active_character).map(|(s, _)| *s).or(p.system_id);
        (sys, p.locations.clone())
    };
    let pings = {
        let j = deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        j.pings.clone()
    };

    // Phase 2, enrich with nothing held.
    let systems = facts.systems.clone();
    let rings = crate::app::build_char_rings(
        &systems,
        &facts.chars,
        &locations,
        &facts.active_character,
        player_sys,
        &facts.disabled,
        facts.only_undocked,
        facts.count_bridges,
    )
    .with_staging(facts.staging.as_deref());
    let mut cards: Vec<IntelCard> = reports
        .iter()
        .map(|r| {
            let target = r.primary_system().map(|s| s.id);
            let from_you =
                crate::app::jumps_from_you(&systems, player_sys, target, facts.count_bridges);
            IntelCard {
                severity: crate::app::severity_of(r, &facts.severity),
                from_you,
                via: crate::app::jump_via(
                    &systems,
                    player_sys,
                    target,
                    facts.count_bridges,
                    from_you,
                ),
                chars: rings.card_for(r),
                report: r.clone(),
            }
        })
        .collect();
    // Newest first, like `intel_view`. `IntelState` is in arrival order and an amended report keeps
    // its slot.
    cards.sort_by(|a, b| b.report.received.cmp(&a.report.received));

    // Capped, because the jabber state holds every ping it has seen.
    let mut pings = pings;
    pings.sort_by_key(|p| std::cmp::Reverse(ping_time(p)));
    pings.truncate(PING_CAP);
    let ping_cards: Vec<PingCard> = pings
        .iter()
        .map(|p| {
            let m = crate::pings::match_ping_rule(&facts.ping_rules, p);
            PingCard {
                ping: p.clone(),
                rule: m.map(|r| r.name.clone()),
                suppressed: m.is_some_and(|r| r.suppress),
            }
        })
        .collect();

    let formup_names = formup_names(&ping_cards, &systems);
    let map = map_live(&cards, player_sys, &locations, facts);
    let sysinfo = status_pane(&status, facts);

    // Phase 3, publish. Only the hash comparison happens under the web lock.
    let mut st = deps.web.lock().unwrap_or_else(|e| e.into_inner());
    let lookups = Lookups {
        resolved_pilots: resolved_pilots.into_iter().collect(),
        uncertain,
        last_ship: last_ship.into_iter().collect(),
        kills: alerts.kills.clone().into_iter().collect(),
        affil: alerts.affil.clone().into_iter().collect(),
    };
    if let Some(rev) = st.changed(Pane::Intel, hash_of(&(&cards, &lookups))) {
        st.put_intel(IntelPane { rev, cards, lookups });
    }
    if let Some(rev) = st.changed(Pane::Alerts, hash_of(&alerts.feed)) {
        // The page reads the status and notes panes, so drop the overlay's copies.
        let mut msg = alerts.clone();
        msg.status = Default::default();
        msg.notes = Default::default();
        st.put_alerts(AlertPane { rev, msg });
    }
    if let Some(rev) = st.changed(Pane::Pings, hash_of(&(&ping_cards, &formup_names))) {
        st.put_pings(PingPane { rev, pings: ping_cards, systems: formup_names });
    }
    if let Some(rev) = st.changed(Pane::Map, hash_of(&map)) {
        st.put_map(MapLive { rev, ..map });
    }
    if let Some(rev) = st.changed(Pane::Status, hash_of(&sysinfo)) {
        st.put_status(StatusPane { rev, systems: sysinfo });
    }
    if let Some(rev) = st.changed(Pane::Rescue, hash_of(&facts.rescue)) {
        if let Some(side) = facts.rescue.clone() {
            st.put_rescue(crate::web::rescue::RescuePane { rev, side });
        }
    }
    if let Some(rev) = st.changed(Pane::Notes, hash_of(&(&*facts.notes_view, &*facts.notes))) {
        st.put_notes(NotesPane {
            rev,
            view: (*facts.notes_view).clone(),
            book: (*facts.notes).clone(),
        });
    }
    if let Some(rev) = st.changed(Pane::Jabber, hash_of(&facts.jabber)) {
        st.put_jabber(crate::web::jabber::JabberPane { rev, side: facts.jabber.clone() });
    }
    let meta = Meta {
        rev: 0,
        version: env!("CARGO_PKG_VERSION"),
        theme: facts.theme.clone(),
        compact: facts.compact,
        intel_ttl_secs: facts.intel_ttl_secs,
        intel_max_jumps: facts.intel_max_jumps,
        count_bridges: facts.count_bridges,
        allow_writeback: facts.allow_writeback,
        active_character: facts.active_character.clone(),
        chars: facts.chars.clone(),
        player_system: player_sys,
        sounds: sound_map(&facts.sounds),
        rescue: facts.rescue.is_some(),
        avoid_gate: facts.avoid_gate.clone(),
        avoid_jump: facts.avoid_jump.clone(),
        sound_rev: crate::sound::SYNTH_REV,
    };
    if let Some(rev) = st.changed(Pane::Meta, hash_of(&meta)) {
        st.put_meta(Meta { rev, ..meta });
    }
}

/// `AlertSettings::sounds` is positional; the page holds a severity name, not an index.
fn sound_map(sounds: &[String]) -> BTreeMap<String, String> {
    use crate::settings::Severity::*;
    [Info, Warning, Danger, Critical]
        .iter()
        .enumerate()
        .filter_map(|(i, sev)| {
            let name = sounds.get(i)?;
            (!name.is_empty() && !name.eq_ignore_ascii_case("off"))
                .then(|| (format!("{sev:?}"), name.clone()))
        })
        .collect()
}

/// Names for formup systems, since the page has no SDE.
fn formup_names(
    pings: &[PingCard],
    systems: &Option<Arc<crate::geo::Systems>>,
) -> BTreeMap<i64, String> {
    let Some(sys) = systems.as_ref() else { return BTreeMap::new() };
    let mut out = BTreeMap::new();
    for p in pings {
        let crate::pings::Ping::Fleet { formup, .. } = &p.ping else { continue };
        for f in formup {
            if let crate::pings::Formup::System(id) = f {
                if let Some(info) = sys.info_of(*id) {
                    out.insert(*id, info.name.clone());
                }
            }
        }
    }
    out
}

/// Worst severity and newest sighting per system, plus where your characters are. Ids only, the
/// geometry is served separately.
fn map_live(
    cards: &[IntelCard],
    you: Option<i64>,
    locations: &HashMap<String, (i64, bool)>,
    facts: &super::facts::UiFacts,
) -> MapLive {
    let ttl = facts.intel_ttl_secs;
    let now = chrono::Utc::now().timestamp();
    let mut per_system: HashMap<i64, (u8, i64)> = HashMap::new();
    for c in cards {
        if ttl > 0 && now - c.report.received > ttl {
            continue;
        }
        let Some(sys) = c.report.primary_system() else { continue };
        let e = per_system.entry(sys.id).or_insert((0, 0));
        e.0 = e.0.max(c.severity as u8);
        e.1 = e.1.max(c.report.received);
    }
    let mut intel: Vec<(i64, u8, i64)> =
        per_system.into_iter().map(|(id, (sev, ts))| (id, sev, ts)).collect();
    intel.sort_unstable();

    let mut counts: HashMap<i64, u32> = HashMap::new();
    for (sys, _) in locations.values() {
        *counts.entry(*sys).or_default() += 1;
    }
    let mut chars: Vec<(i64, u32)> = counts.into_iter().collect();
    chars.sort_unstable();

    MapLive {
        rev: 0,
        you,
        chars,
        intel,
        camps: facts.camps.clone(),
        holes: facts.holes.clone(),
        cyno: facts.cyno.clone(),
        route: facts.route.clone(),
        upgrades: facts
            .upgrades
            .iter()
            .map(|(id, ups)| {
                (*id, ups.iter().map(|(k, l, ore)| UpgradeMark { k: *k, l: *l, ore: *ore }).collect())
            })
            .collect(),
    }
}

fn status_pane(
    status: &HashMap<i64, crate::systemstatus::SysFlags>,
    facts: &super::facts::UiFacts,
) -> BTreeMap<i64, SysInfo> {
    status
        .iter()
        .map(|(id, f)| {
            let sov = f.sov.as_ref();
            (
                *id,
                SysInfo {
                    adm: f.adm,
                    k: f.ship_kills,
                    p: f.pod_kills,
                    n: f.npc_kills,
                    j: f.jumps,
                    sov: sov.and_then(|n| facts.sov_colors.get(n).cloned()),
                    coal: sov.and_then(|n| facts.coal_colors.get(n).cloned()),
                    inc: f.incursion,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uitest::fixtures;

    /// The player's system, 1DQ1-A.
    const HOME: i64 = 30_004_759;

    fn facts() -> super::super::facts::UiFacts {
        super::super::facts::UiFacts {
            web_enabled: true,
            systems: Some(fixtures::systems()),
            chars: vec![("Amryu".to_owned(), 42)],
            active_character: "Amryu".to_owned(),
            intel_ttl_secs: 3600,
            ..Default::default()
        }
    }

    fn deps(reports: Vec<crate::intel::IntelReport>) -> Deps {
        deps_at(reports, HashMap::from([("Amryu".to_owned(), (HOME, false))]))
    }

    fn deps_at(
        reports: Vec<crate::intel::IntelReport>,
        locations: HashMap<String, (i64, bool)>,
    ) -> Deps {
        let intel_state = Arc::new(Mutex::new(crate::intel::IntelState::default()));
        intel_state.lock().unwrap().reports = reports;
        let player = Arc::new(Mutex::new(crate::esi::Player {
            active_name: "Amryu".to_owned(),
            system_id: Some(HOME),
            docked: false,
            locations,
        }));
        Deps {
            facts: super::super::facts::shared(),
            web: super::super::state::shared(),
            intel_state,
            pilots: Default::default(),
            player,
            system_status: Default::default(),
            jabber: Default::default(),
        }
    }

    fn empty_alerts() -> crate::ipc::AlertMsg {
        crate::ipc::AlertMsg {
            feed: Vec::new(),
            from_you: Vec::new(),
            via: Vec::new(),
            chars: Vec::new(),
            status: Default::default(),
            resolved_pilots: Default::default(),
            uncertain: Default::default(),
            last_ship: Default::default(),
            kills: Default::default(),
            affil: Default::default(),
            notes: Default::default(),
            secs: 0.0,
            focus: false,
        }
    }

    /// The thread is spawned regardless, and a tick is expensive for users who never enable the web
    /// view.
    #[test]
    fn a_tick_is_skipped_unless_the_feature_is_on_and_the_graph_is_loaded() {
        let ready = facts();
        assert!(should_publish(&ready));

        let off = super::super::facts::UiFacts { web_enabled: false, ..ready.clone() };
        assert!(!should_publish(&off), "off by default has to mean idle, not merely unserved");

        let no_graph = super::super::facts::UiFacts { systems: None, ..ready };
        assert!(!should_publish(&no_graph), "the SDE arrives long after the thread starts");
    }

    #[test]
    fn a_card_carries_the_distance_and_ring_the_report_does_not() {
        // Given in arrival order; `intel_typical` is the oldest of the three.
        let d = deps(vec![
            fixtures::intel_typical(),
            fixtures::intel_across_the_bridge(),
            fixtures::intel_next_door(),
        ]);
        tick(&d, &facts(), &empty_alerts());

        let st = d.web.lock().unwrap();
        let intel = st.snapshot_since(0).intel.expect("intel pane published");
        let ids: Vec<u64> = intel.cards.iter().map(|c| c.report.id).collect();
        assert_eq!(
            ids,
            vec![
                fixtures::intel_across_the_bridge().id,
                fixtures::intel_next_door().id,
                fixtures::intel_typical().id,
            ],
            "newest first, by report time rather than arrival order"
        );

        let by_id = |id: u64| intel.cards.iter().find(|c| c.report.id == id).unwrap();
        assert_eq!(by_id(fixtures::intel_typical().id).from_you, Some(0), "player is here");
        assert_eq!(by_id(fixtures::intel_next_door().id).from_you, Some(1), "one gate");
        let far = by_id(fixtures::intel_across_the_bridge().id);
        assert_eq!(far.from_you, Some(2), "two gates, through 319-3D");
        assert!(
            far.chars.hops.is_empty(),
            "one character has nothing to disambiguate, so the card draws the plain number"
        );
    }

    /// The ring says whose jump count a card quotes, so it fills in only with a second character.
    #[test]
    fn a_second_character_fills_in_the_ring() {
        let d = deps_at(
            vec![fixtures::intel_across_the_bridge()],
            HashMap::from([
                ("Amryu".to_owned(), (HOME, false)),
                ("Alt".to_owned(), (30_004_608, false)),
            ]),
        );
        let mut f = facts();
        f.chars.push(("Alt".to_owned(), 43));
        tick(&d, &f, &empty_alerts());

        let st = d.web.lock().unwrap();
        let intel = st.snapshot_since(0).intel.unwrap();
        let hops = &intel.cards[0].chars.hops;
        assert_eq!(hops.len(), 2);
        assert_eq!(hops[0].name, "Alt", "nearest first");
        assert_eq!(hops[0].jumps, Some(1));
        assert_eq!(hops[1].name, "Amryu");
        assert_eq!(hops[1].jumps, Some(2));
        assert_eq!(intel.cards[0].chars.selected, Some(1), "the active character is Amryu");
    }

    /// Jita is unconnected in the fixture graph. Zero would claim a hostile is on top of you.
    #[test]
    fn an_unreachable_system_has_no_distance() {
        let d = deps(vec![fixtures::intel_beyond_the_gates()]);
        tick(&d, &facts(), &empty_alerts());
        let st = d.web.lock().unwrap();
        let intel = st.snapshot_since(0).intel.unwrap();
        assert_eq!(intel.cards[0].from_you, None);
    }

    #[test]
    fn a_formup_system_is_named_for_the_page() {
        let d = deps(vec![]);
        d.jabber.lock().unwrap().pings = vec![fixtures::ping_fleet()];
        tick(&d, &facts(), &empty_alerts());

        let st = d.web.lock().unwrap();
        let pane = st.snapshot_since(0).pings.expect("pings pane");
        assert_eq!(pane.systems.get(&HOME).map(String::as_str), Some("1DQ1-A"));
    }

    #[test]
    fn the_configured_sounds_reach_the_page_keyed_by_severity() {
        let d = deps(vec![fixtures::intel_typical()]);
        let mut f = facts();
        f.sounds = vec!["off".into(), "warning".into(), "danger".into(), "critical".into()];
        tick(&d, &f, &empty_alerts());

        let st = d.web.lock().unwrap();
        let meta = st.snapshot_since(0).meta.expect("meta pane");
        assert_eq!(meta.sounds.get("Warning").map(String::as_str), Some("warning"));
        assert_eq!(meta.sounds.get("Critical").map(String::as_str), Some("critical"));
        assert!(meta.sounds.get("Info").is_none(), "\"off\" is not a sound to fetch");
        assert_eq!(meta.sound_rev, crate::sound::SYNTH_REV);
    }

    #[test]
    fn status_is_its_own_pane_and_is_not_duplicated() {
        let d = deps(vec![fixtures::intel_typical()]);
        {
            let mut st = d.system_status.lock().unwrap();
            st.insert(HOME, crate::systemstatus::SysFlags { npc_kills: 7, ..Default::default() });
        }
        let mut alerts = empty_alerts();
        alerts.status.insert(HOME, crate::systemstatus::SysFlags::default());
        alerts.notes = fixtures::notebook().view("").subset([], [2_112_000_001]);
        tick(&d, &facts(), &alerts);

        let st = d.web.lock().unwrap();
        let snap = st.snapshot_since(0);
        assert_eq!(snap.status.expect("status pane").systems[&HOME].n, 7);
        let alerts = snap.alerts.expect("alert pane");
        assert!(alerts.msg.status.is_empty(), "the alert pane must not carry a second copy");
        assert!(alerts.msg.notes.pilots.is_empty(), "nor a second copy of the notes");
    }

    #[test]
    fn pings_are_capped_to_the_newest() {
        let d = deps(vec![]);
        {
            let mut j = d.jabber.lock().unwrap();
            for i in 0..(PING_CAP as i64 + 40) {
                let mut p = fixtures::ping_plain();
                if let crate::pings::Ping::Plain { timestamp, .. } = &mut p {
                    *timestamp = 1_000_000 + i;
                }
                j.pings.push(p);
            }
        }
        tick(&d, &facts(), &empty_alerts());

        let st = d.web.lock().unwrap();
        let pane = st.snapshot_since(0).pings.expect("pings pane");
        assert_eq!(pane.pings.len(), PING_CAP);
        let times: Vec<i64> = pane
            .pings
            .iter()
            .map(|c| match &c.ping {
                crate::pings::Ping::Plain { timestamp, .. } => *timestamp,
                crate::pings::Ping::Fleet { timestamp, .. } => *timestamp,
            })
            .collect();
        assert_eq!(times[0], 1_000_000 + PING_CAP as i64 + 39, "newest first");
        assert!(times.windows(2).all(|w| w[0] >= w[1]), "and in order");
    }

    #[test]
    fn map_live_carries_systems_and_never_coordinates() {
        let d = deps(vec![fixtures::intel_typical()]);
        tick(&d, &facts(), &empty_alerts());

        let st = d.web.lock().unwrap();
        let map = st.snapshot_since(0).map.expect("map pane published");
        assert_eq!(map.you, Some(HOME));
        assert_eq!(map.chars, vec![(HOME, 1)]);
        assert_eq!(map.intel.len(), 1);
        assert_eq!(map.intel[0].0, HOME);
        let json = serde_json::to_string(&map).unwrap();
        assert!(!json.contains("x2d") && !json.contains("\"x\""), "geometry must not ride along");
    }

    /// Empty lookups hash stably whatever the container, so
    /// `an_idle_tick_with_real_lookups_publishes_nothing` is the stricter version.
    #[test]
    fn an_unchanged_tick_publishes_nothing() {
        let d = deps(vec![fixtures::intel_typical()]);
        let f = facts();
        tick(&d, &f, &empty_alerts());
        let first = d.web.lock().unwrap().seq;
        assert!(first > 0, "the first tick has to publish");

        tick(&d, &f, &empty_alerts());
        assert_eq!(d.web.lock().unwrap().seq, first, "nothing changed, so nothing is published");

        assert!(
            d.web.lock().unwrap().snapshot_since(first).intel.is_none(),
            "a caught-up client is sent no pane"
        );
    }

    /// Every container in the snapshot must hash the same way twice, or change detection fails.
    #[test]
    fn a_snapshot_hashes_the_same_when_rebuilt() {
        let lookups = || {
            Lookups {
                resolved_pilots: Default::default(),
                uncertain: ["Alpha Pilot", "Bravo Pilot", "Charlie Pilot", "Delta Pilot"]
                    .into_iter()
                    .collect(),
                last_ship: crate::app::build_last_ship(&[fixtures::intel_torture()])
                    .into_iter()
                    .collect(),
                kills: Default::default(),
                affil: Default::default(),
            }
        };
        let first = super::super::state::hash_of(&lookups());
        for _ in 0..30 {
            assert_eq!(
                super::super::state::hash_of(&lookups()),
                first,
                "a rebuilt snapshot hashed differently; something in it is unordered"
            );
        }
    }

    /// A republish resets the page's scroll position. Lookups are rebuilt each tick, and a `HashMap`
    /// serializes in per-instance order, so only populated lookups expose unstable hashing.
    #[test]
    fn an_idle_tick_with_real_lookups_publishes_nothing() {
        let d = deps(vec![fixtures::intel_torture()]);
        {
            let mut st = d.system_status.lock().unwrap();
            for id in 30_004_700..30_004_712 {
                st.insert(id, crate::systemstatus::SysFlags::default());
            }
        }
        let f = facts();
        let alerts = empty_alerts();
        tick(&d, &f, &alerts);
        let first = d.web.lock().unwrap().seq;

        {
            let st = d.web.lock().unwrap();
            let pane = st.snapshot_since(0).intel.expect("intel pane");
            assert!(!pane.cards.is_empty());
            let status = st.snapshot_since(0).status.expect("status pane");
            assert!(status.systems.len() >= 12, "the panes have to be worth hashing");
        }

        for _ in 0..10 {
            tick(&d, &f, &alerts);
        }
        assert_eq!(
            d.web.lock().unwrap().seq,
            first,
            "ten idle ticks published something; the panes are not comparing equal"
        );
    }

    #[test]
    fn a_notes_edit_moves_only_the_notes_pane() {
        let d = deps(vec![fixtures::intel_typical()]);
        let mut f = facts();
        tick(&d, &f, &empty_alerts());
        let seq = d.web.lock().unwrap().seq;
        assert!(d.web.lock().unwrap().snapshot_since(0).notes.is_some(), "an empty book is still published");

        let book = fixtures::notebook();
        f.notes_view = Arc::new(book.view(""));
        f.notes = Arc::new(book);
        tick(&d, &f, &empty_alerts());
        let st = d.web.lock().unwrap();
        let snap = st.snapshot_since(seq);
        let pane = snap.notes.expect("the notes pane changed");
        assert!(!pane.view.pilots.is_empty());
        assert!(!pane.book.folders.is_empty());
        assert!(snap.intel.is_none() && snap.map.is_none() && snap.meta.is_none());
        drop(st);

        let seq = d.web.lock().unwrap().seq;
        tick(&d, &f, &empty_alerts());
        assert_eq!(d.web.lock().unwrap().seq, seq, "the same book hashes the same");
    }

    #[test]
    fn a_new_report_moves_only_the_panes_it_touches() {
        let d = deps(vec![fixtures::intel_typical()]);
        let f = facts();
        tick(&d, &f, &empty_alerts());
        let (seq, meta_rev) = {
            let st = d.web.lock().unwrap();
            (st.seq, st.snapshot_since(0).meta.unwrap().rev)
        };

        d.intel_state.lock().unwrap().reports.push(fixtures::intel_clear());
        tick(&d, &f, &empty_alerts());

        let st = d.web.lock().unwrap();
        assert!(st.seq > seq, "a new report has to publish");
        let snap = st.snapshot_since(seq);
        assert!(snap.intel.is_some(), "the intel pane changed");
        assert!(snap.meta.is_none(), "nothing about the meta pane changed");
        assert_eq!(st.snapshot_since(0).meta.unwrap().rev, meta_rev);
    }

    /// Lock order is `intel_state -> pilots`. This test hangs rather than fails if the copy-out ever
    /// nests the two locks.
    #[test]
    fn the_publisher_never_holds_two_locks_at_once() {
        let d = deps(vec![fixtures::intel_typical()]);
        let f = facts();
        let pilots = d.pilots.clone();
        let intel_state = d.intel_state.clone();

        let held = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = held.clone();
        // Takes the locks in the opposite order to the publisher, and holds both.
        let other = std::thread::spawn(move || {
            let _p = pilots.lock().unwrap();
            flag.store(true, std::sync::atomic::Ordering::Release);
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _i = intel_state.lock().unwrap();
        });
        while !held.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::yield_now();
        }

        tick(&d, &f, &empty_alerts());
        other.join().expect("the opposing thread finished");
        assert!(d.web.lock().unwrap().seq > 0);
    }
}
