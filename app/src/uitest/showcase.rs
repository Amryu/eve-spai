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
    }
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
    for p in &mut f.pings {
        if matches!(p, crate::pings::Ping::Fleet { .. }) {
            *p = home_defence_ping();
        }
    }
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

/// A pasted local list, looked up: made-up pilots and groups, every row finished.
fn lookup() -> Scene {
    use crate::localscan::{Org, Row};
    harness::scratch_profile();
    fn rename(n: &str) -> &str {
        match n {
        "Fixture Pilot" => "Rhaegan Voss",
        "Sample Hunter" => "Daro Kestrin",
        "Test Logi" => "Liss Amberfall",
        "Placeholder Scout" => "Tavik Oron",
        "Imaginary Blops" => "Selune Varr",
        "Quiet Alt" => "Marek Ilyn",
        other => other,
        }
    }
    let rows: Vec<(String, Row)> = fixtures::lookup_rows()
        .into_iter()
        .filter_map(|(n, r)| match r {
            Row::Done(mut s) => {
                let name = rename(&n).to_owned();
                s.name = name.clone();
                Some((name, Row::Done(s)))
            }
            _ => None,
        })
        .collect();
    let org = |name: &str, ticker: &str| Org { name: name.to_owned(), ticker: ticker.to_owned() };
    let orgs = vec![
        (98_000_001, org("Hollow Reach Salvage", "HRSV")),
        (98_000_003, org("Ashen Veil", "ASHV")),
        (99_000_001, org("Vanta Consortium", "VANTA")),
        (99_000_002, org("Crimson Tidewalkers", "TIDE")),
    ];
    let mut app: Option<crate::app::SpaiApp> = None;
    // Wider than the rest: the table's full set of columns needs a little over 1280.
    Scene::ui("showcase_lookup", [1440.0, 800.0], move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.seed_lookup(rows.clone(), orgs.clone());
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
