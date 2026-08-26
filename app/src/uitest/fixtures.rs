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
    Arc::new(build_systems())
}

/// The same graph with one jump bridge folded in. 1DQ1-A to 7-K5EL is two gates or one bridge, so
/// the gate-only and bridged answers differ for a pair the other fixtures already use.
pub(crate) fn systems_bridged() -> Arc<Systems> {
    let mut s = build_systems();
    s.add_bridges(&[(30_004_759, 30_003_704)]);
    Arc::new(s)
}

/// [`systems_bridged`] plus a bridge to Jita, which has no gate adjacency in this graph at all, so
/// gates cannot reach it by any route and only the bridged answer exists.
pub(crate) fn systems_bridged_island() -> Arc<Systems> {
    let mut s = build_systems();
    s.add_bridges(&[(30_004_759, 30_003_704), (30_004_759, 30_000_142)]);
    Arc::new(s)
}

fn build_systems() -> Systems {
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
    Systems::new(by_name, adjacency)
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

/// A hostile in 7-K5EL, the far end of the bridge in [`systems_bridged`], with the player in
/// 1DQ1-A. Its jump chip reads a different number under each setting.
pub(crate) fn intel_across_the_bridge() -> IntelReport {
    IntelReport {
        id: 9,
        received: now() - 20,
        channel: "delve.imperium".into(),
        reporter: "Scout Charlie".into(),
        text: "7-K5EL  Jackdaw  hostile".into(),
        systems: vec![detected("7-K5EL")],
        ships: vec![ship(34_317, "Jackdaw")],
        count: Some(1),
        count_ships: 1,
        ..Default::default()
    }
}

/// A hostile in Jita, the far end of the gateless bridge in [`systems_bridged_island`].
pub(crate) fn intel_beyond_the_gates() -> IntelReport {
    IntelReport {
        id: 11,
        received: now() - 30,
        channel: "delve.imperium".into(),
        reporter: "Scout Delta".into(),
        text: "Jita  Rifter  hostile".into(),
        systems: vec![detected("Jita")],
        ships: vec![ship(587, "Rifter")],
        count: Some(1),
        count_ships: 1,
        ..Default::default()
    }
}

/// A hostile one gate from the player, where no bridge changes the answer.
pub(crate) fn intel_next_door() -> IntelReport {
    IntelReport {
        id: 10,
        received: now() - 25,
        channel: "delve.imperium".into(),
        reporter: "Scout Bravo".into(),
        text: "319-3D  Cerberus  hostile".into(),
        systems: vec![detected("319-3D")],
        ships: vec![ship(11_993, "Cerberus")],
        count: Some(1),
        count_ships: 1,
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

/// A plain ping whose body runs to several lines, with a link on the middle one. A body line is
/// the only place `render_ping_body`'s row height is visible, and a link is the one genuinely
/// interactive thing that can sit on such a line.
pub(crate) fn ping_plain_multiline() -> crate::pings::Ping {
    crate::pings::Ping::Plain {
        timestamp: now() - 30,
        text: "Sov timer in 68FT-6 at 19:40.\n\
               Fits and doctrine: https://example.invalid/doctrines\n\
               Bring a mobile depot, we refit on grid."
            .into(),
        sender: Some("Director".into()),
        target: Some("corp".into()),
        raw: "raw plain body".into(),
    }
}

pub(crate) const JABBER_ROOM: &str = "delve.imperium@conference.goonfleet.com";
pub(crate) const JABBER_ROOM_QUIET: &str = "corp.chat@conference.goonfleet.com";
pub(crate) const JABBER_DM: &str = "wingmate@goonfleet.com";

fn chat_msg(from: &str, body: &str, age: i64, outgoing: bool) -> crate::jabber::ChatMsg {
    crate::jabber::ChatMsg {
        from: from.to_owned(),
        body: body.to_owned(),
        time: now() - age,
        outgoing,
    }
}

/// Long enough to fill a default-sized pop-out and keep text under the top-right corner, where the
/// always-on-top pin floats.
fn room_history() -> Vec<crate::jabber::ChatMsg> {
    let mut v = vec![
        chat_msg("Scout Alpha", "gate camp still up in 319-3D, about 20 in bubbles", 5400, false),
        chat_msg("Scout Alpha", "mostly Muninns and a couple of Loki", 5395, false),
        chat_msg("Fleet Commander", "forming home defence in 1DQ1-A, Muninn doctrine", 5100, false),
        chat_msg("Fleet Commander", "undock and warp to me, logi first", 5090, false),
        chat_msg("Wingmate Alpha", "on my way, 3 jumps out", 4800, false),
        chat_msg("me", "grabbing a Muninn, need 2 minutes", 4700, true),
        chat_msg("Logi Lead", "logi channel is up, join before you undock", 4200, false),
        chat_msg(
            "Scout Bravo",
            "hostiles moved off gate, they are burning towards the Keepstar undock and \
             holding at about 60km, watch the bubbles on the way in",
            3000,
            false,
        ),
        chat_msg("Fleet Commander", "hold cloak until I call it", 2400, false),
        chat_msg("Wingmate Alpha", "in fleet, in position", 1200, false),
        chat_msg("me", "same, sitting on the FC", 900, true),
        chat_msg("Scout Alpha", "second wave landing, 40+ now", 420, false),
        chat_msg("Fleet Commander", "primary is the Loki on grid, broadcast for reps", 300, false),
        chat_msg("Logi Lead", "reps landing, hold your cap", 120, false),
        chat_msg("Wingmate Alpha", "nice, tackle is holding", 40, false),
    ];
    v.extend([chat_msg("Scout Bravo", "cyno up on the Keepstar", 10, false)]);
    v
}

/// The cap `jabber.rs` drains a conversation back to, which is the case the user reported.
pub(crate) const JABBER_LONG_LEN: usize = 1000;

/// A conversation at the cap, generated rather than written out. Senders run in threes so grouping
/// engages, bodies vary in length so rows are variable height, and the last six land inside the
/// session window so the "new" divider sits near the bottom rather than off the top.
fn room_history_long() -> Vec<crate::jabber::ChatMsg> {
    const SENDERS: [&str; 5] =
        ["Scout Alpha", "Fleet Commander", "Wingmate Alpha", "Logi Lead", "Scout Bravo"];
    const BODIES: [&str; 4] = [
        "gate is clear",
        "holding at 60km off the undock, watch the bubbles",
        "primary is the Loki on grid, broadcast for reps and keep transversal up while the \
         second wave lands",
        "warp to me",
    ];
    (0..JABBER_LONG_LEN)
        .map(|i| {
            let age = if i >= JABBER_LONG_LEN - 6 {
                300 - (i - (JABBER_LONG_LEN - 6)) as i64 * 40
            } else {
                1200 + (JABBER_LONG_LEN - 6 - i) as i64 * 5
            };
            let outgoing = i % 17 == 0;
            let from = if outgoing { "me" } else { SENDERS[(i / 3) % SENDERS.len()] };
            chat_msg(from, &format!("{} #{i}", BODIES[i % BODIES.len()]), age, outgoing)
        })
        .collect()
}

/// [`jabber_state`] with an empty-bodied message grouped under the last one, which is what an
/// `<body/>` with no text produces. Nothing else holds that row open, so it is the case where a
/// body row of literally nothing shows up as a collapsed message.
pub(crate) fn jabber_state_blank_body() -> crate::jabber::JabberState {
    let mut st = jabber_state();
    let v = st.chats.entry(JABBER_ROOM.to_owned()).or_default();
    v.push(chat_msg("Scout Bravo", "", 8, false));
    v.push(chat_msg("Scout Bravo", "and the second one just lit", 6, false));
    st
}

/// [`jabber_state`] with the room history swapped for a full-cap one.
pub(crate) fn jabber_state_long() -> crate::jabber::JabberState {
    let mut st = jabber_state();
    st.chats.insert(JABBER_ROOM.to_owned(), room_history_long());
    st
}

pub(crate) fn jabber_state() -> crate::jabber::JabberState {
    let mut st = crate::jabber::JabberState {
        enabled: true,
        running: true,
        connected: true,
        status: "Online".into(),
        ever_online: true,
        ..Default::default()
    };
    st.rooms.insert(JABBER_ROOM.to_owned());
    st.rooms.insert(JABBER_ROOM_QUIET.to_owned());
    st.chats.insert(JABBER_ROOM.to_owned(), room_history());
    st.chats.insert(
        JABBER_ROOM_QUIET.to_owned(),
        vec![chat_msg("Director", "sov timer tonight, sign up on the forum", 7200, false)],
    );
    st.chats.insert(
        JABBER_DM.to_owned(),
        vec![
            chat_msg("Wingmate Alpha", "you flying tonight?", 900, false),
            chat_msg("me", "yes, staging in 1DQ", 880, true),
        ],
    );
    st.unread.insert(JABBER_ROOM_QUIET.to_owned());
    st.unread.insert(JABBER_DM.to_owned());
    st.mentions.insert(JABBER_DM.to_owned());
    st.room_subjects.insert(
        JABBER_ROOM.to_owned(),
        "Delve intel. Report hostiles with system first.".to_owned(),
    );
    st
}

fn convo(
    jid: &str,
    name: &str,
    presence: crate::jabber::Presence,
    unread: bool,
) -> crate::app::Convo {
    crate::app::Convo {
        jid: jid.to_owned(),
        name: name.to_owned(),
        unread,
        group: "Fleet".to_owned(),
        presence,
        status_text: String::new(),
    }
}

fn channel(jid: &str, unread: bool, motd: &str) -> crate::app::ChannelRow {
    crate::app::ChannelRow {
        jid: jid.to_owned(),
        name: jid.split('@').next().unwrap_or(jid).to_owned(),
        unread,
        inaccessible: false,
        motd: motd.to_owned(),
    }
}

/// The per-frame snapshot `jabber_frame` normally builds off `JabberState` and the keyring. Built
/// here directly: the real builder reads `has_password`, which needs an OS keyring no test machine
/// has, and would leave `configured` false and the view stuck on its login form.
pub(crate) fn jabber_frame() -> crate::app::JabberFrame {
    let st = jabber_state();
    crate::app::JabberFrame {
        configured: true,
        ever_online: true,
        connected: true,
        status: "Online".into(),
        convos: vec![
            convo(JABBER_DM, "Wingmate Alpha", crate::jabber::Presence::Online, true),
            convo("logilead@goonfleet.com", "Logi Lead", crate::jabber::Presence::Away, false),
        ],
        pings: vec![ping_fleet(), ping_plain()],
        rooms: vec![JABBER_ROOM.to_owned(), JABBER_ROOM_QUIET.to_owned()],
        dm_keys: vec![JABBER_DM.to_owned()],
        unread: st.unread.clone(),
        mentions: st.mentions.clone(),
        pings_unread: true,
        channels: vec![
            channel(JABBER_ROOM, false, "Delve intel. Report hostiles with system first."),
            channel(JABBER_ROOM_QUIET, true, ""),
        ],
        inaccessible: Vec::new(),
        subjects: st.room_subjects.clone(),
    }
}

/// One pop-out holding all three conversations, `active` on top. `id` is the window id the scene
/// addresses it by.
pub(crate) fn jabber_popout(id: u64, active: &str) -> crate::app::ChatWindow {
    crate::app::ChatWindow {
        id,
        tabs: vec![JABBER_ROOM.to_owned(), JABBER_ROOM_QUIET.to_owned(), JABBER_DM.to_owned()],
        active: Some(active.to_owned()),
        ..Default::default()
    }
}
