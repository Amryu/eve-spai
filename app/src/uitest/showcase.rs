//! Scenes for the website: full windows with enough in them to show what each view is for. Every
//! name of a pilot, scout or channel member is made up; systems and ships are EVE's own.

use super::fixtures::{self, INSMOTHER};
use super::harness::{self, Scene};
use crate::intel::{DetectedShip, DetectedSystem, IntelReport};
use crate::nav::View;

/// Where the player sits in every showcase.
pub(crate) const HOME: &str = "A24L-V";
/// The signed-in character in every showcase.
pub(crate) const PILOT: &str = "Kestrel Vane";
const SIZE: [f32; 2] = [1280.0, 800.0];
const SHARP: f32 = 1.5;

fn sys(region: &crate::geo::Systems, name: &str) -> DetectedSystem {
    let i = region.lookup(name).unwrap_or_else(|| panic!("{name} is not in the Insmother fixture"));
    DetectedSystem { id: i.id, name: i.name.clone(), security: i.security }
}

/// A report `ago` seconds old, `text` as a scout typed it.
#[allow(clippy::too_many_arguments)]
fn report(region: &crate::geo::Systems, id: u64, ago: i64, reporter: &str, text: &str, systems: &[&str], ships: &[(i64, &str)], pilots: &[&str], count: Option<u32>) -> IntelReport {
    IntelReport {
        id,
        received: fixtures::now() - ago,
        channel: "insmother.imperium".into(),
        reporter: reporter.into(),
        text: text.into(),
        systems: systems.iter().map(|n| sys(region, n)).collect(),
        ships: ships.iter().map(|(id, n)| DetectedShip { id: *id, name: (*n).into() }).collect(),
        pilots: pilots.iter().map(|p| (*p).to_owned()).collect(),
        count,
        count_ships: ships.len() as u32,
        ..Default::default()
    }
}

/// The feed a quiet evening in Insmother might read, newest first.
pub(crate) fn home_intel(region: &crate::geo::Systems) -> Vec<IntelReport> {
    let mut clear = report(region, 107, 610, "Scout Ilsa Varn", "F39H-1 clr", &["F39H-1"], &[], &[], None);
    clear.clear = true;
    vec![
        report(region, 101, 25, "Scout Ilsa Varn", "Vex Harrow Kaelen Dray  Sabre Loki  EKPB-3 gate", &["EKPB-3"], &[(22456, "Sabre"), (29990, "Loki")], &["Vex Harrow", "Kaelen Dray"], None),
        report(region, 102, 70, "Scout Tomas Reyl", "12 hostile 4M-QXK  Eris Cerberus", &["4M-QXK"], &[(22460, "Eris"), (11993, "Cerberus")], &[], Some(12)),
        report(region, 103, 140, "Scout Ilsa Varn", "Mira Solvane  Stiletto  WF4C-8", &["WF4C-8"], &[(11198, "Stiletto")], &["Mira Solvane"], None),
        report(region, 104, 260, "Scout Aven Kordt", "Ostrik Vale  Astero  cloaked  8G-2FP", &["8G-2FP"], &[(33468, "Astero")], &["Ostrik Vale"], None),
        report(region, 105, 390, "Scout Tomas Reyl", "5 red 7-JT09 Ishtar gang", &["7-JT09"], &[(12005, "Ishtar")], &[], Some(5)),
        report(region, 106, 480, "Scout Aven Kordt", "Jannik Holt  Heron  RQN-OO", &["RQN-OO"], &[(605, "Heron")], &["Jannik Holt"], None),
        clear,
        report(region, 108, 780, "Scout Ilsa Varn", "Seren Aldous  Hurricane  +3  GK5Z-T", &["GK5Z-T"], &[(24702, "Hurricane")], &["Seren Aldous"], Some(4)),
    ]
}

fn pilot_ids() -> std::collections::HashMap<String, i64> {
    ["Vex Harrow", "Kaelen Dray", "Mira Solvane", "Ostrik Vale", "Jannik Holt", "Seren Aldous"]
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_lowercase(), 2_112_100_000 + i as i64))
        .collect()
}

/// The app in `view`, the player in [`HOME`] in Insmother, the intel feed filled.
fn showcase(name: &'static str, view: View, setup: impl Fn(&mut crate::app::SpaiApp) + 'static) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, SIZE, move |ui| {
        let app = app.get_or_insert_with(|| {
            let region = fixtures::insmother();
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = view;
            a.chat_dir = Some(harness::scratch_chat_dir());
            let home = region.systems.lookup(HOME).map(|i| i.id);
            a.characters = vec![crate::store::CharacterRow { id: 2_112_000_900, name: PILOT.into(), expires_at: 0, scopes: String::new() }];
            a.active_character = PILOT.into();
            {
                let mut p = a.player.lock().unwrap();
                p.system_id = home;
                p.active_name = PILOT.into();
                if let Some(h) = home {
                    p.locations.insert(PILOT.into(), (h, false));
                }
            }
            a.intel_state.lock().unwrap().reports = home_intel(&region.systems);
            a.settings.jump_bridges = region.bridges.clone();
            a.pilots.lock().unwrap().preload(&pilot_ids());
            a.seed_map(region.systems, region.regions, INSMOTHER, region.drawn);
            setup(&mut a);
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
    .sharp(SHARP)
}

pub(crate) fn scenes() -> Vec<Scene> {
    let v = vec![
        jabber(),
        lookup(),
        showcase("showcase_map", View::Map, |_| {}),
        showcase("showcase_intel", View::Intel, |_| {}),
        // Paths as a Windows pilot's would read, not this machine's.
        showcase("showcase_settings", View::Settings, |a| {
            a.settings.eve_logs_dir = r"C:\Users\Pilot\Documents\EVE\logs".into();
            a.settings.eve_settings_dir = r"C:\Users\Pilot\AppData\Local\CCP\EVE\c_ccp_eve_tq_tranquility".into();
        }),
        showcase("showcase_battle", View::Battles, |a| {
            let (b, names) = fixtures::real_battle();
            a.seed_battle(b, names);
        }),
        // A spoken question about the gang in the intel feed, answered with links, a killmail, a
        // map action waiting for a click and a watch running.
        showcase("showcase_assistant", View::Assistant, |a| {
            use crate::ai::session::{ActionCard, CardState, Chip, Turn};
            use crate::ai::tools::{ActionKind, PendingAction};
            a.settings.ai.enabled = true;
            a.settings.ai.voice.stt = crate::ai::config::SttKind::Groq;
            a.set_ship_names_for_test(&[(22456, "Sabre"), (29990, "Loki"), (32880, "Venture")]);
            let path: Vec<i64> = ["EKPB-3", "Z182-R", "KDG-TA"].iter().filter_map(|n| a.systems.as_ref().and_then(|g| g.lookup(n)).map(|i| i.id)).collect();
            let now = crate::clock::utc().timestamp();
            {
                let mut ws = a.ai_watches.lock().unwrap();
                let mut w = crate::ai::watch::Watch::new(1, "Vex Harrow and Kaelen Dray within 3 jumps of A24L-V".into(), Default::default(), vec!["vex harrow".into()], now - 120, Some(now + 3480));
                w.hits = 1;
                ws.push(w);
            }
            let q = Turn { user: true, voice: true, text: "Where's that Sabre and Loki pair heading?".into(), ..Default::default() };
            let answer = Turn {
                text: "**Vex Harrow and Kaelen Dray**, Sabre and Loki, on the EKPB-3 gate 25 seconds ago.\n\n\
                       - 21:38 killed a Venture [killmail](spai:kill/131000002) in 4M-QXK\n\
                       - 21:52 intel in Delve.Imperium: \"Sabre Loki EKPB-3 gate\"\n\n\
                       They are working north towards KDG-TA. Highlight their path?"
                    .into(),
                chips: vec![
                    Chip { name: "track_movement".into(), args: "entity: Vex Harrow, since_minutes: 60".into(), error: None },
                    Chip { name: "route".into(), args: "from: EKPB-3, to: A24L-V".into(), error: None },
                    Chip { name: "highlight_systems".into(), args: "systems: [EKPB-3, Z182-R, KDG-TA]".into(), error: None },
                ],
                cards: vec![ActionCard {
                    action: PendingAction { id: 1, kind: ActionKind::Highlight(path), summary: "Highlight EKPB-3, Z182-R, KDG-TA on the map".into() },
                    state: CardState::Pending,
                    confirmed: false,
                }],
                ..Default::default()
            };
            let ask = Turn { user: true, text: "Tell me if they come within 3 jumps of me".into(), ..Default::default() };
            let watching = Turn {
                text: "Watching for Vex Harrow and Kaelen Dray within 3 jumps of A24L-V for the next hour.".into(),
                chips: vec![Chip { name: "create_watch".into(), args: "goal: Vex Harrow and Kaelen Dray within 3 jumps of A24L-V".into(), error: None }],
                ..Default::default()
            };
            let ctx = egui::Context::default();
            crate::app::ai_ui::seed_ai_view(a, &ctx, vec![q, answer, ask, watching]);
        }),
        // A gate route across the region, drawn on the map with the Route dock beside it.
        showcase("showcase_route", View::Map, |a| {
            let Some(g) = a.systems.clone() else { return };
            let (Some(from), Some(to)) = (g.lookup(HOME).map(|i| i.id), g.lookup("EKPB-3").map(|i| i.id)) else { return };
            a.settings.intel_count_bridges = true;
            a.map_route_start("gate", from);
            a.map_route_set_dest(to, None);
        }),
    ];
    v.into_iter().map(with_images).collect()
}

/// One of Amryu's own home defence strat op pings from mid September, dated two minutes ago for the
/// picture and with its internal Mumble link left out.
pub(crate) fn home_defence_ping() -> crate::pings::Ping {
    use crate::pings::{Comms, Formup, PapType, Ping};
    let raw = "Hostiles in our ESS, get in this fleet quick\n\nFC Name: Amryu\nFormup Location: C-J6MT\n\
               PAP Type: Strategic\nComms: Op 11\nDoctrine: Snail Fleet (Moas) (Moa > Moa > Osprey > Booster Drake)";
    Ping::Fleet {
        timestamp: fixtures::now() - 120,
        description: "Hostiles in our ESS, get in this fleet quick".into(),
        fc: "Amryu".into(),
        fleet: None,
        formup: vec![Formup::Text("C-J6MT".into())],
        pap: Some(PapType::Strategic),
        comms: Some(Comms::Mumble { channel: "Op 11".into(), link: "mumble://voice.example.invalid/Op%2011".into() }),
        doctrine: Some("Snail Fleet (Moas) (Moa > Moa > Osprey > Booster Drake)".into()),
        source: Some("coord".into()),
        target: Some("all".into()),
        raw: raw.into(),
        parts: Vec::new(),
    }
}

/// Made-up fleet pings from around the home defence op, older than it, in the same format: made-up
/// FCs, the staging system and real doctrine strings. Two, so the
/// feed, newest at the bottom, still shows the home defence op whole.
fn evening_pings() -> Vec<crate::pings::Ping> {
    use crate::pings::{Comms, Formup, PapType, Ping};
    let ping = |ago: i64, hurf: &str, fc: &str, formup: &str, pap: PapType, op: &str, doctrine: &str| {
        let kind = match &pap {
            PapType::Strategic => "Strategic",
            _ => "Peacetime",
        };
        Ping::Fleet {
            timestamp: fixtures::now() - ago,
            description: hurf.into(),
            fc: fc.into(),
            fleet: None,
            formup: vec![Formup::Text(formup.into())],
            pap: Some(pap),
            comms: Some(Comms::Mumble { channel: op.into(), link: format!("mumble://voice.example.invalid/{}", op.replace(' ', "%20")) }),
            doctrine: Some(doctrine.into()),
            source: Some("coord".into()),
            target: Some("all".into()),
            raw: format!("{hurf}\n\nFC Name: {fc}\nFormup Location: {formup}\nPAP Type: {kind}\nComms: {op}\nDoctrine: {doctrine}"),
            parts: Vec::new(),
        }
    };
    vec![
        ping(9 * 60, "Svipul roam through the backyard, Kirins and Scalpels welcome", "Mira Ostwald", "C-J6MT", PapType::Peacetime, "Op 9", "Svipul (Boosters > Kirin/Scalpel > Svipul > Else)"),
        ping(23 * 60, "Ihub timer in Z182-R, we need every Ferox", "Dain Morrow", "C-J6MT", PapType::Strategic, "Op 2", "Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)"),
    ]
}

/// Room names as they read from the new home.
fn moved_home(jid: &str) -> String {
    jid.replace("delve", "insmother")
}

const SHOWCASE_MOTD: &str = "Insmother intel. System first, then what you see.\n\
         Ping format: FC / formup / doctrine\n\
         Comms: Mumble, Home Defence";

/// The Jabber view with the fleet ping feed open: the rooms a line member sits in, no command
/// channels.
fn jabber() -> Scene {
    harness::scratch_profile();
    let mut f = fixtures::jabber_frame();
    f.rooms = f.rooms.iter().map(|r| moved_home(r)).collect();
    for c in &mut f.channels {
        c.jid = moved_home(&c.jid);
        c.name = moved_home(&c.name);
        if !c.motd.is_empty() {
            c.motd = SHOWCASE_MOTD.into();
        }
    }
    f.subjects = f.subjects.iter().map(|(k, _)| (moved_home(k), SHOWCASE_MOTD.to_owned())).collect();
    f.unread = f.unread.iter().map(|r| moved_home(r)).collect();
    // Fleet pings only: the home defence op and a few others from the same evening.
    f.pings = std::iter::once(home_defence_ping()).chain(evening_pings()).collect();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui("showcase_jabber", SIZE, move |ui| {
        let app = app.get_or_insert_with(|| {
            let a = crate::app::SpaiApp::build(ui.ctx(), true);
            let mut st = fixtures::jabber_state();
            st.rooms = st.rooms.iter().map(|r| moved_home(r)).collect();
            st.chats = std::mem::take(&mut st.chats).into_iter().map(|(k, v)| (moved_home(&k), v)).collect();
            st.unread = st.unread.iter().map(|r| moved_home(r)).collect();
            st.room_subjects = st.room_subjects.keys().map(|k| (moved_home(k), SHOWCASE_MOTD.to_owned())).collect();
            *a.jabber.lock().unwrap() = st;
            a
        });
        app.jabber_sidebar_for_test(ui, &f, true);
    })
    .sharp(SHARP)
}

/// A pasted local list, looked up: made-up pilots, three in Goonswarm, three without an alliance and
/// one in Fraternity., each with their own record, hulls and habits.
fn lookup() -> Scene {
    use crate::localscan::{Bait, Bar, Cyno, Fc, Org, Row, Ship, Summary, Tag};
    harness::scratch_profile();
    const GOONS: i64 = 1_354_830_081;
    const FRAT: i64 = 99_003_581;
    const HORDE: i64 = 99_005_338;
    const INIT: i64 = 1_900_696_668;
    let bars = |v: &[(u32, u32)]| v.iter().map(|&(kills, losses)| Bar { kills, losses }).collect::<Vec<_>>();
    #[allow(clippy::too_many_arguments)]
    let pilot = |id: i64, name: &str, corp: i64, alliance: i64, days: i64, sec: f64, danger: u32, gang: u32, avg: f64, solo: u32, kills: u32, losses: u32, isk: (f64, f64)| Summary {
        id,
        name: name.into(),
        birthday: Some(fixtures::now() - days * 86_400),
        security: Some(sec),
        corp_id: corp,
        alliance_id: alliance,
        danger,
        gang,
        avg_gang: avg,
        solo,
        kills,
        losses,
        isk_destroyed: isk.0,
        isk_lost: isk.1,
        ..Default::default()
    };
    let fill = |mut s: Summary, groups: &[(u32, u32)], space: &[(u32, u32)], isk: &[(u32, u32)], ships: &[(i64, u32, u32)]| {
        s.groups.iter_mut().zip(bars(groups)).for_each(|(a, b)| *a = b);
        s.space.iter_mut().zip(bars(space)).for_each(|(a, b)| *a = b);
        s.isk.iter_mut().zip(bars(isk)).for_each(|(a, b)| *a = b);
        s.ships = ships.iter().map(|&(type_id, kills, losses)| Ship { type_id, group_id: 0, kills, losses }).collect();
        s
    };
    let mut line = fill(
        pilot(2_120_000_101, "Brannoc Velt", 98_400_001, GOONS, 2_262, 2.1, 38, 98, 61.0, 1, 1_284, 212, (410e9, 38e9)),
        &[(1, 2), (8, 6), (40, 11), (190, 38), (420, 70), (430, 55), (195, 30), (0, 0)],
        &[(12, 4), (40, 9), (1_210, 196), (22, 3), (0, 0), (0, 0)],
        &[(980, 160), (230, 40), (60, 10), (14, 2)],
        &[(12015, 640, 31), (11987, 210, 9), (22456, 180, 12)],
    );
    line.affiliates = vec![(GOONS, 1_120), (HORDE, 41)];
    line.tags = vec![(Tag::Logi, 9)];
    let mut fc = fill(
        pilot(2_120_000_102, "Iska Morrow", 98_400_002, GOONS, 3_420, 4.8, 64, 99, 142.0, 0, 4_920, 388, (2.1e12, 96e9)),
        &[(0, 0), (12, 8), (60, 20), (410, 60), (1_380, 120), (2_100, 130), (958, 50), (0, 0)],
        &[(30, 6), (210, 22), (4_500, 352), (180, 8), (0, 0), (0, 0)],
        &[(3_100, 250), (1_200, 90), (480, 38), (140, 10)],
        &[(22460, 1_830, 96), (29990, 1_120, 44), (11978, 610, 30)],
    );
    fc.affiliates = vec![(GOONS, 4_600), (INIT, 180), (HORDE, 120)];
    fc.tags = vec![(Tag::Fc, 0), (Tag::Cyno, 4)];
    fc.fc = Some(Fc { level: "high".into(), score: 92, monitor: 0, command: 210, large_fleet: 880 });
    fc.cyno = Some(Cyno { standard: 4, covert: 0, industrial: 0 });
    let newbro = fill(
        pilot(2_120_000_103, "Pell Arnault", 98_400_001, GOONS, 65, 0.6, 9, 100, 38.0, 0, 14, 9, (2.1e9, 0.4e9)),
        &[(0, 0), (0, 1), (0, 0), (6, 3), (8, 4), (0, 1), (0, 0), (0, 0)],
        &[(0, 0), (0, 0), (14, 9), (0, 0), (0, 0), (0, 0)],
        &[(14, 9), (0, 0), (0, 0), (0, 0)],
        &[(620, 11, 5), (32880, 0, 2), (608, 3, 2)],
    );
    let mut explorer = fill(
        pilot(2_120_000_104, "Odrin Saak", 98_400_011, 0, 1_130, 3.4, 21, 40, 2.1, 19, 31, 44, (5.6e9, 7.2e9)),
        &[(19, 20), (9, 14), (3, 8), (0, 2), (0, 0), (0, 0), (0, 0), (0, 0)],
        &[(2, 3), (1, 6), (4, 9), (24, 26), (0, 0), (0, 0)],
        &[(29, 40), (2, 4), (0, 0), (0, 0)],
        &[(33468, 12, 9), (11192, 2, 6), (605, 0, 11)],
    );
    explorer.affiliates = vec![(0, 31)];
    let mut camper = fill(
        pilot(2_120_000_105, "Veyra Tolune", 98_400_012, 0, 2_040, -7.9, 91, 71, 5.4, 143, 2_206, 118, (640e9, 31e9)),
        &[(143, 21), (1_100, 52), (720, 30), (230, 12), (13, 3), (0, 0), (0, 0), (0, 0)],
        &[(310, 12), (1_840, 98), (52, 6), (4, 2), (0, 0), (0, 0)],
        &[(1_790, 100), (330, 15), (70, 3), (16, 0)],
        &[(33157, 1_210, 61), (22456, 640, 33), (11198, 356, 24)],
    );
    camper.faction_id = 500_002;
    camper.affiliates = vec![(0, 2_100), (99_003_214, 90)];
    camper.tags = vec![(Tag::Ganker, 26), (Tag::Bait, 4)];
    camper.ganker = 26;
    camper.bait = Some(Bait { level: "low".into(), count: 4 });
    let hauler = fill(
        pilot(2_120_000_106, "Kett Marrow", 98_400_013, 0, 610, 5.0, 0, 0, 0.0, 0, 0, 6, (0.0, 3.1e9)),
        &[(0, 4), (0, 2), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
        &[(0, 2), (0, 3), (0, 1), (0, 0), (0, 0), (0, 0)],
        &[(0, 5), (0, 1), (0, 0), (0, 0)],
        &[(12733, 0, 3), (29248, 0, 2), (11129, 0, 1)],
    );
    let mut blops = fill(
        pilot(2_120_000_107, "Saro Dvelin", 98_400_021, FRAT, 1_560, -2.6, 94, 89, 18.0, 27, 3_870, 402, (1.4e12, 88e9)),
        &[(27, 6), (380, 40), (1_240, 120), (1_610, 170), (520, 52), (73, 14), (0, 0), (0, 0)],
        &[(60, 8), (420, 51), (3_310, 336), (80, 7), (0, 0), (0, 0)],
        &[(2_910, 330), (680, 52), (220, 16), (60, 4)],
        &[(12038, 1_210, 96), (12032, 640, 51), (29986, 310, 22)],
    );
    blops.affiliates = vec![(FRAT, 3_400), (HORDE, 260), (INIT, 140)];
    blops.tags = vec![(Tag::Blops, 7), (Tag::Cyno, 3)];
    blops.cyno = Some(Cyno { standard: 0, covert: 3, industrial: 0 });
    let rows: Vec<(String, Row)> = [line, fc, newbro, explorer, camper, hauler, blops].into_iter().map(|s| (s.name.clone(), Row::Done(Box::new(s)))).collect();
    let org = |name: &str, ticker: &str| Org { name: name.to_owned(), ticker: ticker.to_owned() };
    let orgs = vec![
        (98_400_001, org("KarmaFleet", "KF")),
        (98_400_002, org("GoonWaffe", "GEWNS")),
        (98_400_011, org("Hollow Reach Salvage", "HRSV")),
        (98_400_012, org("Ashen Veil", "ASHV")),
        (98_400_013, org("Tidewalker Freight", "TDWF")),
        (98_400_021, org("Hellfire Collective", "HFC")),
        (GOONS, org("Goonswarm Federation", "CONDI")),
        (FRAT, org("Fraternity.", "FRT")),
        (HORDE, org("Pandemic Horde", "REKTD")),
        (INIT, org("The Initiative.", "INIT")),
        (99_003_214, org("Brave Collective", "BRAVE")),
    ];
    let mut app: Option<crate::app::SpaiApp> = None;
    // Wider than the rest: the table's full set of columns needs a little over 1280.
    Scene::ui("showcase_lookup", [1440.0, 800.0], move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.seed_lookup(rows.clone(), orgs.clone());
            // Blue Goonswarm, red Fraternity., the rest without a standing.
            a.seed_standings([(GOONS, 10.0), (FRAT, -10.0)]);
            a.view = View::Lookup;
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
    .sharp(SHARP)
}

/// Answers the EVE image server's URLs from bundled files: a ship icon by type id, one default
/// portrait for every character and one default logo for every group. Headless loads no images,
/// and the grey squares it leaves read as broken on a website.
struct Images(std::collections::HashMap<String, std::sync::Arc<[u8]>>);

impl Images {
    fn bundled() -> Self {
        use base64::Engine as _;
        #[derive(serde::Deserialize)]
        struct Raw {
            images: std::collections::HashMap<String, String>,
        }
        let mut json = String::new();
        std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&include_bytes!("data/images.json.gz")[..]), &mut json).expect("image fixture");
        let raw: Raw = serde_json::from_str(&json).expect("image fixture");
        let engine = base64::engine::general_purpose::STANDARD;
        Images(raw.images.into_iter().filter_map(|(k, v)| Some((k, engine.decode(v).ok()?.into()))).collect())
    }
}

impl egui::load::BytesLoader for Images {
    fn id(&self) -> &str {
        "showcase_images"
    }

    fn load(&self, _ctx: &egui::Context, uri: &str) -> egui::load::BytesLoadResult {
        use egui::load::{Bytes, BytesPoll, LoadError};
        let Some(rest) = uri.strip_prefix("https://images.evetech.net/") else { return Err(LoadError::NotSupported) };
        let key = match rest.split('/').collect::<Vec<_>>().as_slice() {
            ["types", id, ..] => (*id).to_owned(),
            ["characters", ..] => "portrait".to_owned(),
            _ => "corp".to_owned(),
        };
        match self.0.get(&key) {
            Some(b) => Ok(BytesPoll::Ready { size: None, bytes: Bytes::Shared(b.clone()), mime: None }),
            None => Err(LoadError::NotSupported),
        }
    }

    fn forget(&self, _uri: &str) {}

    fn forget_all(&self) {}

    fn byte_size(&self) -> usize {
        self.0.values().map(|b| b.len()).sum()
    }
}

/// `scene` with ship icons, portraits and logos drawn from the bundled images.
pub(crate) fn with_images(mut scene: Scene) -> Scene {
    use super::harness::Draw;
    fn install(ctx: &egui::Context) {
        let id = egui::Id::new("showcase_images_installed");
        if ctx.data(|d| d.get_temp::<bool>(id)).is_none() {
            egui_extras::install_image_loaders(ctx);
            ctx.add_bytes_loader(std::sync::Arc::new(Images::bundled()));
            ctx.data_mut(|d| d.insert_temp(id, true));
        }
    }
    scene.draw = match scene.draw {
        Draw::Ui(mut f) => Draw::Ui(Box::new(move |ui| {
            install(ui.ctx());
            f(ui)
        })),
        Draw::Ctx(mut f) => Draw::Ctx(Box::new(move |ctx| {
            install(ctx);
            f(ctx)
        })),
    };
    scene
}
