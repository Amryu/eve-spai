use super::fixtures::{self, IntelArgs};
use super::harness::{self, Scene};
use crate::nav::View;

fn intel_scene(name: &'static str, report: crate::intel::IntelReport, width: f32) -> Scene {
    intel_scene_sized(name, report, [width, 520.0])
}

fn intel_scene_sized(
    name: &'static str,
    report: crate::intel::IntelReport,
    size: [f32; 2],
) -> Scene {
    let args = IntelArgs::default();
    Scene::ui(name, size, move |ui| {
        let mut tip = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut tip,
        );
    })
}

/// A card whose pilots and system carry tags and notes from two online folders, with a third
/// offline folder that must not show.
fn intel_notes_scene(name: &'static str, compact: bool) -> Scene {
    let args = IntelArgs { notes: fixtures::notebook().view(""), ..IntelArgs::default() };
    let report = fixtures::intel_typical();
    Scene::ui(name, [if compact { 360.0 } else { 620.0 }, 260.0], move |ui| {
        let mut tip = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            Some(3),
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            compact,
            &mut tip,
        );
    })
}

/// The map's right dock System tab, which is narrower than the window and lays its header out
/// differently.
fn docked_system_scene(name: &'static str) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, [270.0, 600.0], move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.seed_notes(fixtures::notebook());
            a
        });
        app.docked_system_ui(ui, 30_004_759);
    })
}

/// The Lookup view with one pilot's tab open, rendered through the same report UI as the pilot window.
fn lookup_tab_scene(name: &'static str) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, [1100.0, 760.0], move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.seed_notes(fixtures::notebook());
            a.seed_lookup_tab(fixtures::pilot_report());
            a.view = View::Lookup;
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

fn notes_dialog_scene(name: &'static str, size: [f32; 2], open: fn(&mut crate::app::SpaiApp)) -> Scene {
    dialog_scene(name, size, move |a| {
        a.seed_notes(fixtures::notebook());
        open(a);
    })
}

/// One card with per-character attribution, which is what a feed looks like once more than one
/// character has alerts on.
fn intel_chars_scene(
    name: &'static str,
    report: crate::intel::IntelReport,
    size: [f32; 2],
    chars: crate::app::CardChars,
    from_you: Option<u32>,
) -> Scene {
    let args = IntelArgs { chars, ..IntelArgs::default() };
    Scene::ui(name, size, move |ui| {
        let mut tip = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            from_you,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            size[0] < 400.0,
            &mut tip,
        );
    })
}

/// The same mid-resolution card at all three dot phases, stacked. The placeholder chip has to
/// start and end at the same x in every one of them.
fn resolving_phases_scene(name: &'static str) -> Scene {
    let args = IntelArgs::default();
    Scene::ui(name, [520.0, 520.0], move |ui| {
        for step in 0..3 {
            let clock = fixtures::now() + step;
            let mut tip = None;
            crate::app::intel_row(
                ui,
                &fixtures::intel_resolving(clock),
                clock,
                false,
                None,
                crate::app::JumpVia::Gates,
                &args.chars,
                &args.systems,
                &args.status,
                &args.ship_details,
                &args.ship_roles,
                &args.resolved_pilots,
                &args.uncertain,
                &args.last_ship,
                &args.kills,
                crate::settings::Severity::Danger,
                true,
                &args.affil,
                &args.notes,
                false,
                &mut tip,
            );
        }
    })
}

fn ping_scene(name: &'static str, ping: crate::pings::Ping) -> Scene {
    ping_scene_with_doctrine_url(name, ping, "")
}

fn ping_scene_with_doctrine_url(
    name: &'static str,
    ping: crate::pings::Ping,
    doctrine_url: &str,
) -> Scene {
    let systems = Some(fixtures::systems());
    let url = doctrine_url.to_owned();
    Scene::ui(name, [520.0, 320.0], move |ui| {
        crate::app::render_ping(ui, &ping, &systems, false, &url, &Default::default());
    })
}

fn alert_window_scene(name: &'static str, feed: Vec<crate::intel::IntelReport>) -> Scene {
    let mut st = crate::app::AlertWindowState {
        enabled: true,
        pinned: true,
        secs: 5.0,
        systems: Some(fixtures::systems()),
        resolved_pilots: fixtures::resolved_pilots(),
        uncertain: fixtures::uncertain(),
        kills: Some(fixtures::kills()),
        affil: Some(fixtures::affil()),
        ..Default::default()
    };
    st.from_you = feed.iter().map(|_| None).collect();
    st.feed = feed.into_iter().map(|r| (r, crate::settings::Severity::Danger)).collect();
    let shared: crate::app::SharedAlertWindow = std::sync::Arc::new(std::sync::Mutex::new(st));
    let cb = crate::app::build_alert_viewport_cb(shared);
    Scene::ctx(name, [560.0, 480.0], move |ctx| {
        let mut ui = harness::detached_ui(ctx);
        cb(&mut ui, egui::ViewportClass::Root);
    })
}

/// One card per bridge state, ruled on the way the main process would: 319-3D a plain gate hop,
/// 7-K5EL one bridge jump where gates take two, Jita reachable by bridge alone.
fn alert_bridge_cards() -> Vec<(crate::intel::IntelReport, Option<u32>, crate::app::JumpVia)> {
    vec![
        (fixtures::intel_next_door(), Some(1), crate::app::JumpVia::Gates),
        (fixtures::intel_across_the_bridge(), Some(1), crate::app::JumpVia::BridgeShorter(2)),
        (fixtures::intel_beyond_the_gates(), Some(1), crate::app::JumpVia::BridgeOnly),
    ]
}

/// The alert window fed the way the overlay subprocess is fed: one `AlertMsg` through the frame
/// codec and into the state the real reader fills. Nothing here knows the player's system or the
/// bridge setting, so a mark can only have come from the verdict on the wire. The subprocess itself
/// cannot run in the harness; this is the same callback reached the same way.
fn alert_window_ipc_scene(
    name: &'static str,
    cards: Vec<(crate::intel::IntelReport, Option<u32>, crate::app::JumpVia)>,
    send_via: bool,
) -> Scene {
    alert_window_ipc_chars_scene(name, cards, send_via, Vec::new())
}

/// The same frame with per-card character attribution on the wire, which is the only way to see
/// what the overlay subprocess makes of it: the subprocess itself cannot be rendered by kittest.
fn alert_window_ipc_chars_scene(
    name: &'static str,
    cards: Vec<(crate::intel::IntelReport, Option<u32>, crate::app::JumpVia)>,
    send_via: bool,
    chars: Vec<crate::app::CardChars>,
) -> Scene {
    let msg = crate::ipc::AlertMsg {
        feed: cards
            .iter()
            .map(|(r, _, _)| (r.clone(), crate::settings::Severity::Danger))
            .collect(),
        from_you: cards.iter().map(|(_, j, _)| *j).collect(),
        via: if send_via { cards.iter().map(|(_, _, v)| *v).collect() } else { Vec::new() },
        chars,
        status: Default::default(),
        resolved_pilots: fixtures::resolved_pilots(),
        uncertain: fixtures::uncertain(),
        last_ship: Default::default(),
        kills: Default::default(),
        affil: Default::default(),
        notes: Default::default(),
        secs: 5.0,
        focus: false,
    };
    let mut buf: Vec<u8> = Vec::new();
    crate::ipc::send(&mut buf, &msg).expect("frame the alert");
    let msg: crate::ipc::AlertMsg =
        crate::ipc::recv(&mut std::io::Cursor::new(buf)).expect("read the alert back");
    let mut st = crate::app::AlertWindowState {
        enabled: true,
        pinned: true,
        systems: Some(fixtures::systems()),
        kills: Some(fixtures::kills()),
        affil: Some(fixtures::affil()),
        ..Default::default()
    };
    crate::overlay::apply_alert(&mut st, msg);
    let shared: crate::app::SharedAlertWindow = std::sync::Arc::new(std::sync::Mutex::new(st));
    let cb = crate::app::build_alert_viewport_cb(shared);
    Scene::ctx(name, [560.0, 720.0], move |ctx| {
        let mut ui = harness::detached_ui(ctx);
        cb(&mut ui, egui::ViewportClass::Root);
    })
}

fn ping_window_scene(name: &'static str, pings: Vec<crate::pings::Ping>) -> Scene {
    let st = crate::app::PingWindowState {
        enabled: true,
        open: true,
        windows: pings
            .into_iter()
            .map(|p| crate::app::PingShown { ping: p, shown_at: std::time::Instant::now() })
            .collect(),
        systems: Some(fixtures::systems()),
        ..Default::default()
    };
    let shared: crate::app::SharedPingWindow = std::sync::Arc::new(std::sync::Mutex::new(st));
    let cb = crate::app::build_ping_viewport_cb(shared);
    Scene::ctx(name, [560.0, 420.0], move |ctx| {
        let mut ui = harness::detached_ui(ctx);
        cb(&mut ui, egui::ViewportClass::Root);
    })
}

fn nav_scene(name: &'static str, expanded: bool, height: f32) -> Scene {
    let mut expanded = expanded;
    let width = if expanded { crate::nav::WIDTH_EXPANDED } else { crate::nav::WIDTH_COLLAPSED };
    Scene::ui(name, [width, height], move |ui| {
        crate::nav::rail(ui, View::Intel, &mut expanded, &[View::Alerts], &[View::Jabber], crate::nav::View::primary());
    })
}

/// Settings with the web view switched on, so its controls actually render.
///
/// The plain `view_settings` scene leaves it off, which is the default and shows one checkbox. The
/// port, the LAN toggle and the three link buttons only exist once it is enabled, and those are the
/// widgets worth checking for overlap and for escaping their row.
fn web_settings_scene(name: &'static str, size: [f32; 2]) -> Scene {
    web_settings_scene_cfg(name, size, false)
}

/// `bind` opens the advanced part with an address being typed, which is where the field says whether
/// it is bound, waiting or holding something that is not an address.
fn web_settings_scene_cfg(name: &'static str, size: [f32; 2], bind: bool) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Settings;
            if bind {
                a.settings.web.advanced_ack = true;
                a.settings.web.bind_addr = "10.20.30.40".to_owned();
                a.web_bind_draft = Some(("10.20.30".to_owned(), std::time::Instant::now()));
            }
            a.settings.web.enabled = true;
            a.settings.web.token = "PAIRING-TOKEN-FOR-THE-SCENE-0123456789ab".to_owned();
            // Path detection finds the real EVE install, so a render of this view carries the
            // machine's home directory, and these renders get committed and pushed.
            a.settings.eve_logs_dir = "/fixture/EVE/logs".to_owned();
            a.settings.eve_settings_dir = "/fixture/EVE/settings".to_owned();
            // The link and QR are hidden until asked for, which a scene has to ask for.
            a.web_reveal = true;
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

fn view_scene(name: &'static str, view: View, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = view;
            // Only Intel: `battles_view` reads `chat_dir` too, and seeding it there would drop the
            // "configure intel channels" hint the battles scenes exist to cover.
            if view == View::Intel {
                a.chat_dir = Some(harness::scratch_chat_dir());
            }
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The disk-pressure banner over a real view, sized so the top bar, nav rail and status bar are
/// all in frame: an over-tall banner would displace exactly those.
fn disk_scene(
    name: &'static str,
    level: crate::disk::Level,
    free: Option<u64>,
    saw_failure: bool,
    size: [f32; 2],
) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Dashboard;
            a
        });
        // Set every frame: `root_chrome` is called directly here, so the per-frame snapshot in
        // `App::update` never runs.
        app.disk_level = level;
        app.disk_free = free;
        app.disk_saw_failure = saw_failure;
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The banner for a character whose EVE login has stopped working.
///
/// Its own scene for the same reason the disk banner has one: it is chrome that appears above every
/// view, so what it must not do is push the rest of the chrome around or overlap it.
fn auth_scene(
    name: &'static str,
    problem: crate::esi::AuthProblem,
    chars: &'static [&'static str],
    size: [f32; 2],
) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Dashboard;
            a
        });
        app.characters = chars
            .iter()
            .enumerate()
            .map(|(i, n)| crate::store::CharacterRow {
                id: 90_000_000 + i as i64,
                name: (*n).to_owned(),
                expires_at: 0,
                scopes: crate::auth::DEFAULT_SCOPES.join(" "),
            })
            .collect();
        for c in &app.characters {
            crate::esi::set_auth_problem_for_test(c.id, problem);
        }
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// 1DQ1-A, where [`intel_bridge_scene`] parks the player.
const PLAYER_SYS: i64 = 30_004_759;

/// The intel feed with the player in 1DQ1-A and one hostile in 7-K5EL, two gates away or one jump
/// over the bridge in [`fixtures::systems_bridged`].
fn intel_bridge_scene(name: &'static str, count_bridges: bool, size: [f32; 2]) -> Scene {
    intel_feed_scene(
        name,
        count_bridges,
        fixtures::systems_bridged(),
        vec![fixtures::intel_across_the_bridge()],
        size,
    )
}

/// One card per bridge state, against a graph where 319-3D is a plain gate hop, 7-K5EL is two
/// gates or one bridge, and Jita has no gate route at all.
fn intel_bridge_states_scene(name: &'static str, count_bridges: bool, size: [f32; 2]) -> Scene {
    intel_feed_scene(
        name,
        count_bridges,
        fixtures::systems_bridged_island(),
        vec![
            fixtures::intel_next_door(),
            fixtures::intel_across_the_bridge(),
            fixtures::intel_beyond_the_gates(),
        ],
        size,
    )
}

/// The intel feed with the player parked in [`PLAYER_SYS`]. Headless runs no log watcher and no
/// location poller, so the reports and the player's position are seeded.
fn intel_feed_scene(
    name: &'static str,
    count_bridges: bool,
    systems: std::sync::Arc<crate::geo::Systems>,
    reports: Vec<crate::intel::IntelReport>,
    size: [f32; 2],
) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Intel;
            a.chat_dir = Some(harness::scratch_chat_dir());
            a.systems = Some(systems.clone());
            a.settings.intel_count_bridges = count_bridges;
            a.player.lock().unwrap().system_id = Some(PLAYER_SYS);
            a.intel_state.lock().unwrap().reports = reports.clone();
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The battle detail view, which carries the second wrapping toolbar. Its cache is normally
/// filled by the brview worker, and headless starts no worker, so the scene seeds the selection
/// and the cache itself.
fn battle_detail_scene(name: &'static str, size: [f32; 2]) -> Scene {
    use br_core::battle::{Battle, Engagement, Party, PartyKind};
    harness::scratch_profile();
    let battle = Battle {
        engagements: vec![Engagement {
            kill_id: 1,
            time: 0,
            system_id: 30000142,
            system_name: "Jita".into(),
            security: 0.9,
            victim: Party { id: 99, name: "Test Alliance".into(), kind: PartyKind::Alliance },
            victim_char: 5,
            victim_pilot: "Victim".into(),
            victim_ship: 587,
            attackers: vec![],
            isk: 1.0,
            anchored: true,
        }],
        start: 0,
        end: 0,
        systems: vec![(30000142, "Jita".into(), 0.9)],
        sides: vec![],
        kills: 1,
        isk: 1.0,
        ambiguous: false,
        suggested_splits: vec![],
    };
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Battles;
            *a.battles.lock().unwrap() = vec![battle.clone()];
            a.battle_selected = Some(1);
            a.battle_detail_cache = Some(std::sync::Arc::new(crate::brview::BattleDetail {
                kid: 1,
                battle: battle.clone(),
                inv: Default::default(),
                rosters: vec![],
                condensed: vec![],
                ship_ids: vec![],
            }));
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The wormhole table with rows in it. Headless populates no cache, so `view_wormholes` only ever
/// shows the empty state and the column headers never render.
fn wormholes_rows_scene(name: &'static str, size: [f32; 2]) -> Scene {
    use crate::wormholes::{DestClass, ShipSize, Source, Wormhole};
    harness::scratch_profile();
    let now = fixtures::now();
    let holes = vec![
        Wormhole {
            id: 1,
            system_id: 30_004_759,
            signature: Some("ABC-123".into()),
            wh_type: Some("K162".into()),
            dest: DestClass::Thera,
            dest_system_id: Some(30_000_142),
            dest_signature: None,
            dest_wh_type: None,
            size: Some(ShipSize::XLarge),
            is_drifter: false,
            reported_at: now - 600,
            explicit_expiry: Some(now + 6 * 3600),
            source: Source::EveScout,
            updated_at: now - 600,
        },
        Wormhole {
            id: 2,
            system_id: 30_004_608,
            signature: Some("XYZ-987".into()),
            wh_type: Some("C729".into()),
            dest: DestClass::Wspace,
            dest_system_id: None,
            dest_signature: None,
            dest_wh_type: None,
            size: None,
            is_drifter: true,
            reported_at: now - 1_800,
            explicit_expiry: None,
            source: Source::Intel,
            updated_at: now - 1_800,
        },
        Wormhole {
            id: 3,
            system_id: 30_003_704,
            signature: None,
            wh_type: None,
            dest: DestClass::Highsec,
            dest_system_id: Some(30_004_759),
            dest_signature: None,
            dest_wh_type: None,
            size: Some(ShipSize::Frigate),
            is_drifter: false,
            reported_at: now - 90,
            explicit_expiry: Some(now + 1_800),
            source: Source::Manual,
            updated_at: now - 90,
        },
    ];
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Wormholes;
            a.systems = Some(fixtures::systems());
            a.wh_cache = holes.clone();
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

const POPOUT_ID: u64 = 1;

/// One wrapped-not-newlined line, which `desired_rows` would count as a single row.
const DRAFT_WRAPPED: &str = "hold the gate, we are waiting on the second logi wing before anyone \
    warps, and the scout says there is a bubble on the far side so bring an mjd fit if you have \
    one, otherwise burn back to the perch and hold there until the FC calls the next warp in, and \
    keep your cap booster charges for the tackle rather than dumping them on the first neut";
/// Past the composer's ten-row cap, so the field stops growing and scrolls.
const DRAFT_OVERFLOW: &str = "line one\nline two\nline three\nline four\nline five\nline six\n\
    line seven\nline eight\nline nine\nline ten\nline eleven\nline twelve\nline thirteen\n\
    line fourteen";

/// Reads the live draft back out of the scene's own `SpaiApp` after every frame, which is the only
/// handle a test has on state the scene closure owns.
type DraftProbe = std::sync::Arc<std::sync::Mutex<String>>;

/// A popped-out chat window: the tab bar, one room's history and the composer, which is what the
/// pop-out viewport puts in its central panel. `SpaiApp::build` starts no jabber session headless,
/// so the chat state and the window's tab list are seeded here.
fn jabber_popout_scene(name: &'static str, size: [f32; 2], active: &str, draft: &str) -> Scene {
    jabber_popout_probed(name, size, active, draft, None)
}

fn jabber_popout_probed(
    name: &'static str,
    size: [f32; 2],
    active: &str,
    draft: &str,
    probe: Option<DraftProbe>,
) -> Scene {
    jabber_popout_seeded(name, size, active, draft, probe, fixtures::jabber_state)
}

fn jabber_popout_seeded(
    name: &'static str,
    size: [f32; 2],
    active: &str,
    draft: &str,
    probe: Option<DraftProbe>,
    state: fn() -> crate::jabber::JabberState,
) -> Scene {
    use crate::app::ChatWinKey;
    harness::scratch_profile();
    let (active, draft) = (active.to_owned(), draft.to_owned());
    let f = fixtures::jabber_frame();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            *a.jabber.lock().unwrap() = state();
            a.jabber_popouts = vec![fixtures::jabber_popout(POPOUT_ID, &active)];
            a.jabber_drafts.insert(active.clone(), draft.clone());
            // Older than the newest messages, so the "new" divider sits inside the history.
            a.session_start = fixtures::now() - 1_000;
            a
        });
        let mut out = Vec::new();
        app.jabber_window_body(ui, ChatWinKey::Popout(POPOUT_ID), &f, &mut out);
        if let Some(p) = &probe {
            *p.lock().unwrap() = app.jabber_drafts.get(&active).cloned().unwrap_or_default();
        }
    })
}

/// The same pop-out mid-drag: the room tab picked up, the pointer holding it over the history.
/// The drag state is seeded rather than gestured, because kittest has no way to grab a tab that is
/// a painter-only `interact` rect with no AccessKit node.
fn jabber_tab_drag_scene(name: &'static str, size: [f32; 2], pointer: [f32; 2]) -> Scene {
    use crate::app::ChatWinKey;
    harness::scratch_profile();
    let f = fixtures::jabber_frame();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            *a.jabber.lock().unwrap() = fixtures::jabber_state();
            a.jabber_popouts = vec![fixtures::jabber_popout(POPOUT_ID, fixtures::JABBER_ROOM)];
            a.session_start = fixtures::now() - 1_000;
            a.seed_tab_drag(fixtures::JABBER_ROOM, ChatWinKey::Popout(POPOUT_ID));
            a
        });
        let mut out = Vec::new();
        app.jabber_window_body(ui, ChatWinKey::Popout(POPOUT_ID), &f, &mut out);
    })
    .hovered_at(pointer)
}

/// A dialog or secondary window, opened by seeding its gate field and letting
/// [`crate::app::SpaiApp::root_dialogs`] dispatch as it does in `App::ui`.
///
/// Two viewport settings make that reachable. kittest registers no immediate-viewport renderer, so
/// `dialog_viewport_ext` would otherwise take egui's embedded fallback, which wraps the dialog in a
/// stub `egui::Window` while the body's `CentralPanel` still paints full-screen on the root: an
/// empty title bar floating over the dialog it is supposed to contain. Rendering the body straight
/// onto the root drops the stub. Turning embedding off then keeps the two always-on deferred
/// viewports (the alert and fleet-ping overlays) out, which would each open a second `CentralPanel`
/// on the same context and paint egui's "double use of widget ID" error over the dialog.
fn dialog_scene(
    name: &'static str,
    size: [f32; 2],
    open: impl Fn(&mut crate::app::SpaiApp) + 'static,
) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, size, move |ctx| {
        let app = app.get_or_insert_with(|| {
            harness::render_dialogs_on_the_root(ctx);
            let mut a = crate::app::SpaiApp::build(ctx, true);
            open(&mut a);
            a
        });
        app.root_dialogs(ctx, None);
    })
}

/// The Jump Plan panel with the ship picker open, which is the only place the class list is
/// visible. `jump_plan_content` is private, so the scene drives the panel through `jump_plan_ui`
/// and clicks the combo open in the interaction test rather than here.
fn jump_plan_scene(name: &'static str, size: [f32; 2], ship: usize) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.seed_jump_ship(ship);
            a
        });
        app.jump_plan_ui(ui);
    })
}

/// The route panel with a gate route planned, which is the only state that offers the in-game and
/// save buttons.
fn route_panel_scene(name: &'static str) -> Scene {
    route_panel_scene_cfg(name, false)
}

fn route_panel_scene_cfg(name: &'static str, detail: bool) -> Scene {
    route_panel_scene_full(name, detail, false)
}

fn route_panel_scene_full(name: &'static str, detail: bool, fork: bool) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, [380.0, 620.0], move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.seed_map_route(30_004_759, 30_003_704);
            if detail {
                a.seed_route_detail();
            }
            if fork {
                a.seed_route_fork();
            }
            a
        });
        app.jump_plan_ui(ui);
    })
}

fn map_layers_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| crate::app::SpaiApp::build(ui.ctx(), true));
        egui::ScrollArea::vertical().show(ui, |ui| app.map_layers_ui(ui));
    })
}

/// The chat sidebar, one scene per pane. The Channels pane lists remembered rooms with their MOTDs,
/// the Directory pane files anything remembered but not on the roster under "Other", and both carry
/// a remove button.
fn jabber_sidebar_scene(name: &'static str, size: [f32; 2], convos: bool) -> Scene {
    jabber_sidebar_scene_cfg(name, size, convos, false)
}

fn jabber_sidebar_scene_cfg(
    name: &'static str,
    size: [f32; 2],
    convos: bool,
    rescue: bool,
) -> Scene {
    harness::scratch_profile();
    let f = fixtures::jabber_sidebar_frame();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            *a.jabber.lock().unwrap() = fixtures::jabber_state();
            a.settings.fc_rescue_enabled = rescue;
            a
        });
        app.jabber_sidebar_for_test(ui, &f, convos);
    })
}

/// A tracked fleet whose boss is one of this machine's characters, with its advert off.
#[cfg(feature = "fleet")]
fn fleet_advert_scene(name: &'static str, size: [f32; 2], up: bool) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let a = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.view = View::Fleet;
            fixtures::seed_fleet_state(&a);
            fixtures::open_first_fleet(&a);
            a.fleet_booted = true;
            {
                let mut st = a.fleet_state_for_test().lock().unwrap();
                let id = st.open.value.as_ref().unwrap().fleet.id.clone();
                st.advert = Some((id, up));
            }
            a
        });
        a.root_chrome(ui);
        a.root_central(ui, None);
    })
}

/// The Convos list before anything has been opened, which is what a first sign-in shows.
fn jabber_empty_convos_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let a = app.get_or_insert_with(|| {
            let a = crate::app::SpaiApp::build(ui.ctx(), true);
            *a.jabber.lock().unwrap() = crate::jabber::JabberState::default();
            a
        });
        let mut f = a.jabber_frame(false);
        f.configured = true;
        f.ever_online = true;
        f.connected = true;
        a.jabber_sidebar_for_test(ui, &f, true);
    })
}

/// The DM and room start dialogs, each with its own recent list. Sized to show the list scrolling
/// rather than a handful of names.
fn jabber_start_scene(name: &'static str, rooms: bool) -> Scene {
    harness::scratch_profile();
    let f = fixtures::jabber_sidebar_frame();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, [420.0, 400.0], move |ctx| {
        let app = app.get_or_insert_with(|| {
            let a = crate::app::SpaiApp::build(ctx, true);
            *a.jabber.lock().unwrap() = fixtures::jabber_state();
            a
        });
        app.jabber_join_dialog_for_test(ctx, &f, rooms);
    })
}

/// The rescue feed on its own. The rescue window itself has no scene, so this renders the chat the
/// way `uitest_rescue_chat_lines_are_one_line_tall` drives it.
#[cfg(feature = "fleet")]
fn rescue_chat_scene(name: &'static str, size: [f32; 2]) -> Scene {
    let base = fixtures::now();
    let msgs = vec![
        ("Rescue Actual".to_owned(), "titan tackled in 1DQ, need subcaps now".to_owned(), false, base - 95),
        ("Rescue Actual".to_owned(), "cyno is up, bridge is open".to_owned(), false, base - 65),
        ("Wingmate Alpha".to_owned(), "on the bridge".to_owned(), false, base - 34),
        ("me".to_owned(), "in fleet, jumping".to_owned(), true, base - 7),
    ];
    Scene::ui(name, size, move |ui| {
        let _ = crate::app::rescue_chat_feed(ui, &msgs, "shot");
    })
}

/// The clock over the checklist, at a fixed time so the render is the same every run, with the
/// checklist under it the way the ops column stacks them.
#[cfg(feature = "fleet")]
fn rescue_timer_scene(name: &'static str, secs: i64) -> Scene {
    harness::scratch_profile();
    let mut state: Option<crate::rescue::RescueState> = None;
    Scene::ui(name, [300.0, 300.0], move |ui| {
        let _ = state.get_or_insert_with(crate::rescue::RescueState::default);
        crate::app::ping_timer_row(ui, secs);
    })
}

/// The fleet tab in the shell, so the rail entry and the sub-nav are checked like any other view.
///
/// Headless starts no workers, so the state is filled from the dry-run backend here rather than
/// left on a permanent loading state.
#[cfg(feature = "fleet")]
fn fleet_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.view = View::Fleet;
            fixtures::seed_fleet_state(&a);
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The start form, filled from a preset so every control has something in it, with the ping the
/// dry run renders beside it.
#[cfg(feature = "fleet")]
fn fleet_start_scene(name: &'static str, size: [f32; 2]) -> Scene {
    fleet_start_scene_with(name, size, false)
}

/// `tracked` leaves the fixture's FC boss of an active fleet, which is what the double-track
/// guard is for. Off, the form is in its normal ready-to-track state.
#[cfg(feature = "fleet")]
fn fleet_start_scene_with(name: &'static str, size: [f32; 2], tracked: bool) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_presets = fixtures::fleet_presets();
            a.view = View::Fleet;
            // Traffic in both docked rooms, or the chat renders an empty box and its spacing is
            // never actually looked at.
            *a.jabber.lock().unwrap() = fixtures::jabber_state_fleet_rooms();
            fixtures::seed_fleet_state(&a);
            fixtures::open_fleet_start(&a);
            if !tracked {
                // The spoof's active fleets are led by the fixture's own FC, so without this the
                // guard fires and the form never shows the state it exists for.
                let mut st = a.fleet_state_for_test().lock().unwrap();
                st.active_strat.put(Vec::new());
                st.active_pct.put(Vec::new());
            }
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// A tracked fleet, in either of its two halves: who is in it, and what they are flying.
#[cfg(feature = "fleet")]
fn fleet_detail_scene(
    name: &'static str,
    size: [f32; 2],
    tab: crate::app::fleet_ui::DetailTab,
) -> Scene {
    fleet_detail_scene_with(name, size, tab, false)
}

/// `closed` swaps the tree for the flat participant list a finished fleet leaves behind.
#[cfg(feature = "fleet")]
fn fleet_detail_scene_with(
    name: &'static str,
    size: [f32; 2],
    tab: crate::app::fleet_ui::DetailTab,
    closed: bool,
) -> Scene {
    fleet_detail_scene_closed(name, size, tab, closed, true)
}

/// `via_history` decides whether the page was reached through the history or closed underneath the
/// tracking page. Both have to render the same.
#[cfg(feature = "fleet")]
fn fleet_detail_scene_closed(
    name: &'static str,
    size: [f32; 2],
    tab: crate::app::fleet_ui::DetailTab,
    closed: bool,
    via_history: bool,
) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.view = View::Fleet;
            a.fleet_detail_tab = tab;
            // The app opens it by default, and the composition tab now leans on it for
            // everything above the tables.
            a.fleet_sidebar_open = true;
            a.settings.fleet_boost_requirements = fixtures::fleet_boost_rules();
            fixtures::seed_fleet_state(&a);
            if closed {
                fixtures::open_closed_fleet_as(&a, via_history);
            } else {
                fixtures::open_first_fleet(&a);
            }
            a.fleet_booted = true;
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The composition of a fleet that is short of everything, so every check renders in its own
/// colour instead of the quiet line.
#[cfg(feature = "fleet")]
fn fleet_thin_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_boost_requirements = fixtures::fleet_boost_rules();
            a.view = View::Fleet;
            a.fleet_detail_tab = crate::app::fleet_ui::DetailTab::Composition;
            fixtures::seed_fleet_state(&a);
            fixtures::open_thin_fleet(&a);
            // Booting would re-open the fleet from the spoof and put back what the fixture took
            // out.
            a.fleet_booted = true;
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The dialog behind the worst button on the page, which has to be typed out rather than clicked
/// through. On its own, because a modal painted over the page reads to the overlap check as text
/// on top of text.
#[cfg(feature = "fleet")]
fn fleet_confirm_scene(
    name: &'static str,
    size: [f32; 2],
    action: crate::fleets::backend::Action,
    question: &'static str,
) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, size, move |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ctx, true);
            a.settings.fleet_enabled = true;
            a.fleet_booted = true;
            a.fleet_confirm = Some((
                crate::fleets::model::FleetId("fixture-fleet".to_owned()),
                action.clone(),
                question,
                24,
            ));
            a
        });
        app.fleet_confirm_modal(ctx);
    })
}

/// The rescue panel with a ping in it, which is the only way to see the ops column at all.
#[cfg(feature = "fleet")]
fn rescue_panel_scene(name: &'static str, size: [f32; 2]) -> Scene {
    rescue_panel_scene_with(name, size, None)
}

/// `in_use` names the dashboard's channels "Op N" the way the real list does, with that op taken,
/// and picks it. The invented table's "Comms N" names resolve to no op at all.
#[cfg(feature = "fleet")]
fn rescue_panel_scene_with(name: &'static str, size: [f32; 2], in_use: Option<u8>) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fc_rescue_enabled = true;
            a.settings.fleet_enabled = true;
            a.settings.fleet_presets = fixtures::rescue_presets();
            a.settings.rescue_preset = "Capital Save".to_owned();
            fixtures::seed_fleet_state(&a);
            a.fleet_booted = true;
            fixtures::seed_rescue_ping(&a);
            if let Some(op) = in_use {
                a.fleet_state_for_test().lock().unwrap().seed.mumble_channels = (1..=12)
                    .map(|n| crate::fleets::model::ChannelItem {
                        id: crate::fleets::model::ChannelId(n),
                        name: format!("Op {n}"),
                        is_in_use: n == i32::from(op),
                    })
                    .collect();
                a.rescue_state_for_test().lock().unwrap().op_channel = op;
            }
            a
        });
        app.rescue_window_body(ui);
    })
}

/// The Quick Fleet picker, which is the whole of what the button does.
#[cfg(feature = "fleet")]
fn fleet_quick_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, size, move |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ctx, true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_presets = fixtures::fleet_presets();
            a.fleet_booted = true;
            a.fleet_quick_open = true;
            a
        });
        let presets = app.settings.fleet_presets.clone();
        let mut act = crate::app::fleet_ui::FormAct::default();
        app.quick_fleet_window(ctx, &presets, &mut act);
    })
}

/// The same window on its Ships tab, which is what stops every hull reading as out of doctrine.
#[cfg(feature = "fleet")]
fn fleet_hull_editor_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, size, move |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ctx, true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_hulls = fixtures::fleet_hulls();
            fixtures::seed_fleet_state(&a);
            a.fleet_booted = true;
            a.fleet_boost_editor = true;
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("fleet_boost_editor_pick"), 46_i32);
                // The window remembers which tab it was on, so the scene asks for the Ships half.
                d.insert_temp(egui::Id::new("fleet_doctrine_tab"), 1_u8);
            });
            a
        });
        app.fleet_boost_editor(ctx);
    })
}

/// The per-doctrine boost editor, filled for one doctrine so both halves have something in them.
#[cfg(feature = "fleet")]
fn fleet_boost_editor_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, size, move |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ctx, true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_boost_requirements =
                crate::fleets::boosts::default_rules(46, false);
            fixtures::seed_fleet_state(&a);
            a.fleet_booted = true;
            a.settings.fleet_hulls = fixtures::fleet_hulls();
            // More hulls than fit, which is what used to push the window past the screen.
            a.settings.fleet_hulls.extend((0..40).map(|i| crate::settings::FleetHull {
                setup_id: 46,
                type_id: 900_000 + i,
                name: format!("Hull {i}"),
                main: false,
            }));
            a.fleet_boost_editor = true;
            // Land on the doctrine that has rules, or the right half renders its empty state.
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("fleet_boost_editor_pick"), 46_i32));
            a
        });
        app.fleet_boost_editor(ctx);
    })
}

/// The two dialogs the start form and readiness pane now open instead of growing inline.
#[cfg(feature = "fleet")]
fn fleet_dialog_scene(name: &'static str, size: [f32; 2], which: u8) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ctx(name, size, move |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ctx, true);
            a.settings.fleet_enabled = true;
            // The ping window is its own viewport in the app; rendered inline beside a dialog it
            // lands on top of it and this scene is about the dialog.
            a.settings.fleet_ping_window = false;
            a.settings.fleet_presets = fixtures::fleet_presets();
            fixtures::seed_fleet_state(&a);
            // A fleet open behind the dialog, so the breakdown can name what each booster flies.
            fixtures::open_first_fleet(&a);
            a.fleet_booted = true;
            match which {
                0 => a.fleet_snowflakes_open = Some(crate::app::fleet_ui::SnowflakeTarget::Draft),
                2 => {
                    a.fleet_migrate_open = true;
                    // A character picked and checked, which is the state the button unlocks in.
                    // One of the account's own characters, which is the only pool offered without
                    // the startFleetOther permission.
                    let who = {
                        let st = a.fleet_state_for_test().lock().unwrap();
                        let c = st.seed.characters.first().expect("a character").clone();
                        crate::fleets::model::Labelled { id: c.id, label: c.name }
                    };
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("fleet_migrate_pick"), who.clone())
                    });
                    let mut st = a.fleet_state_for_test().lock().unwrap();
                    st.migrate_boss = Some((
                        who.id,
                        crate::fleets::model::BossCheck {
                            is_fleet_boss: true,
                            backup_available: false,
                            error_message: None,
                        },
                    ));
                }
                _ => {
                    {
                        let mut st = a.fleet_state_for_test().lock().unwrap();
                        st.boosts = fixtures::fleet_coverage();
                        st.boost_lines = fixtures::fleet_boost_lines();
                    }
                    a.fleet_boost_detail = Some("Shield Harmonizing".to_owned());
                }
            }
            a
        });
        // The specific window, not `root_dialogs`: that also renders the fleet ping viewport,
        // which lands on top of the dialog this scene is about.
        match which {
            0 => app.fleet_snowflakes_window(ctx),
            2 => app.fleet_migrate_window(ctx),
            _ => app.fleet_boost_detail_window(ctx),
        }
    })
}

/// The start form's sidebar on its Search tab, which the Presets scene never reaches.
#[cfg(feature = "fleet")]
fn fleet_side_search_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_presets = fixtures::fleet_presets();
            a.view = View::Fleet;
            fixtures::seed_fleet_state(&a);
            fixtures::open_fleet_start(&a);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("fleet_side_tab"), 1_u8));
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The fleet settings section on its own: the whole settings page would need a canvas thousands of
/// pixels tall to reach it.
#[cfg(feature = "fleet")]
fn fleet_settings_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            // Otherwise the hint carries this machine's home directory into a committed PNG.
            std::env::set_var("EVE_SPAI_FLEET_SEED", "/fixture/EVE/fleet-seed.json");
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.settings.fleet_boost_requirements = fixtures::fleet_boost_rules();
            fixtures::seed_fleet_state(&a);
            a.fleet_booted = true;
            a
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            app.fleet_settings_section(ui);
        });
    })
}

/// The journal, with the requests a dry run recorded rather than sent.
#[cfg(feature = "fleet")]
fn fleet_journal_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.view = View::Fleet;
            fixtures::seed_fleet_state(&a);
            fixtures::record_fleet_requests(&a);
            a.fleet_journal_open = true;
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// Clicking a fleet has to open it: the rows are painted frames, so nothing but a real click
/// through the tree proves they are reachable.
#[cfg(feature = "fleet")]
#[test]
fn uitest_fleet_rows_open_the_fleet() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = fleet_scene("fleet_click", [1280.0, 800.0]);
    let mut h = harness::build(&mut scene, false);
    let row = h
        .query_all_by_label_contains("Home Defence")
        .next()
        .expect("the seeded fleet is on screen")
        .rect();
    harness::click_at(&h, row.center());
    h.run();
    // The fleet loads on a worker, so either its page or the loading state proves the click landed.
    let opened = h.query_by_label_contains("Members").is_some()
        || h.query_by_label_contains("Loading the fleet").is_some();
    assert!(opened, "clicking a fleet did not open it");
}

/// The tag field is a badge field plus a dropdown, so nothing about the list is on screen until
/// the caret is clicked. Only a real click through the tree proves it opens and filters.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_tag_field_opens_and_filters() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = fleet_start_scene("fleet_tag_popup", [1440.0, 900.0]);
    let mut h = harness::build(&mut scene, false);

    // Two carets, one per tag row. The second is the secondary tags, which take more than one.
    let carets: Vec<egui::Rect> = h
        .query_all_by_label_contains(egui_phosphor::regular::CARET_DOWN)
        .map(|n| n.rect())
        .collect();
    assert_eq!(carets.len(), 2, "one caret per tag row");
    harness::click_at(&h, carets[1].center());
    h.run();

    let listed = |h: &egui_kittest::Harness<'_>, name: &str| {
        h.query_all_by_label_contains(name).count()
    };
    assert!(listed(&h, "Structure Bash") > 0, "the secondary tags are not listed");
    assert!(listed(&h, "Roam") > 0);

    // What typing does to the list is `tag_matches`, unit-tested beside it.
}

/// The screenshot path must not be one forgotten override away from painting a real alliance's
/// rooms, contacts or messages into a PNG that gets committed to a ticket folder.
#[test]
fn uitest_a_live_profile_is_refused() {
    let want = std::path::Path::new("/w/target/uitest-profile");
    // The two ways a render could reach live data: no override at all, and an override pointing
    // somewhere real.
    assert!(harness::profile_objection(None, want).is_some(), "an unset override was accepted");
    let live = std::path::Path::new("/home/someone/.local/share/eve-spai");
    assert!(
        harness::profile_objection(Some(live), want).is_some(),
        "a live profile path was accepted"
    );
    assert!(harness::profile_objection(Some(want), want).is_none(), "the scratch profile was refused");
}

pub(crate) fn all() -> Vec<Scene> {
    let mut v = vec![
        // The timestamp is the subject, so the messages sit seconds apart.
        jabber_popout_scene("jabber_popout_stamps", [520.0, 480.0], fixtures::JABBER_ROOM, ""),
        // Both panes, because the remove button has to read the same in each.
        jabber_sidebar_scene("jabber_sidebar_convos", [900.0, 560.0], true),
        jabber_empty_convos_scene("jabber_sidebar_convos_empty", [420.0, 400.0]),
        jabber_sidebar_scene("jabber_sidebar_directory", [900.0, 560.0], false),
        // One dialog per kind, and neither offering the other kind's conversations.
        jabber_start_scene("jabber_start_dm", false),
        jabber_start_scene("jabber_join_room", true),
        // The class list is the subject, so the panel is sized to show the picker and the
        // range readout that moves with it.
        route_panel_scene("route_panel_gate"),
        route_panel_scene_cfg("route_panel_costs_and_warnings", true),
        route_panel_scene_full("route_panel_fork", false, true),
        jump_plan_scene("jump_plan_command_carrier", [360.0, 560.0], 5),
        jump_plan_scene("jump_plan_capital", [360.0, 560.0], 0),
        docked_system_scene("map_dock_system_notes"),
        notes_dialog_scene("dialog_system_window_notes", [470.0, 620.0], |a| {
            a.open_system(30_004_759);
        }),
        lookup_tab_scene("view_lookup_tab"),
        notes_dialog_scene("dialog_pilot_window", [440.0, 580.0], |a| {
            a.seed_pilot_report(fixtures::pilot_report());
        }),
        intel_notes_scene("intel_card_notes", false),
        intel_notes_scene("intel_card_notes_compact", true),
        notes_dialog_scene("dialog_note_editor_pilot", [480.0, 640.0], |a| {
            let id = fixtures::resolved_pilots()["Hostile Pilot"];
            a.open_note_editor(crate::notes::Subject::Pilot { id, name: "Hostile Pilot".into() });
        }),
        notes_dialog_scene("dialog_note_editor_system", [480.0, 640.0], |a| {
            a.open_note_editor(crate::notes::Subject::System(30_004_759));
        }),
        notes_dialog_scene("dialog_notes_manager_pilot", [900.0, 660.0], |a| {
            a.open_notes_manager(crate::notes::NoteKind::Pilot);
        }),
        notes_dialog_scene("dialog_notes_manager_system", [900.0, 660.0], |a| {
            a.open_notes_manager(crate::notes::NoteKind::System);
        }),
        alert_window_scene("alert_window_typical", vec![fixtures::intel_typical()]),
        alert_window_scene(
            "alert_window_torture",
            vec![fixtures::intel_torture(), fixtures::intel_typical(), fixtures::intel_clear()],
        ),
        alert_window_ipc_scene("alert_window_bridged", alert_bridge_cards(), true),
        // The attribution has to survive the wire, since the overlay subprocess holds
        // neither the roster nor anyone's location and cannot derive it.
        alert_window_ipc_chars_scene(
            "alert_window_chars",
            alert_bridge_cards(),
            true,
            vec![fixtures::card_chars_two(); alert_bridge_cards().len()],
        ),
        ping_window_scene("ping_window_fleet", vec![fixtures::ping_fleet()]),
        ping_window_scene(
            "ping_window_mixed",
            vec![fixtures::ping_fleet(), fixtures::ping_plain()],
        ),
        // A gigabyte free is a warning; a quarter of one has already stopped the archive.
        // The narrow pair proves the wording wraps rather than clipping.
        disk_scene(
            "disk_banner_low",
            crate::disk::Level::Low,
            Some(780 * 1024 * 1024),
            false,
            [1280.0, 800.0],
        ),
        disk_scene(
            "disk_banner_low_narrow",
            crate::disk::Level::Low,
            Some(780 * 1024 * 1024),
            false,
            [720.0, 600.0],
        ),
        disk_scene(
            "disk_banner_critical",
            crate::disk::Level::Critical,
            Some(190 * 1024 * 1024),
            false,
            [1280.0, 800.0],
        ),
        disk_scene(
            "disk_banner_critical_after_a_failed_write",
            crate::disk::Level::Critical,
            None,
            true,
            [1280.0, 800.0],
        ),
        auth_scene(
            "auth_banner_expired",
            crate::esi::AuthProblem::LoggedOut,
            &["Amryu"],
            [1280.0, 800.0],
        ),
        // Narrow, because the remedy is three sentences and it has to wrap rather than clip; and
        // two characters, because the headline changes shape when it is not naming one.
        auth_scene(
            "auth_banner_keychain_narrow",
            crate::esi::AuthProblem::NoKeychain,
            &["Amryu", "Scout"],
            [720.0, 600.0],
        ),
        intel_scene("intel_row_typical", fixtures::intel_typical(), 520.0),
        // Two badges and two numbers when the nearest alerting character is not the one
        // you are looking through, one badge and one number when it is, and a compact card that
        // has room for neither the second badge nor the second number.
        intel_chars_scene(
            "intel_row_two_characters",
            fixtures::intel_typical(),
            [520.0, 520.0],
            fixtures::card_chars_two(),
            Some(4),
        ),
        intel_chars_scene(
            "intel_row_two_characters_narrow",
            fixtures::intel_typical(),
            [320.0, 520.0],
            fixtures::card_chars_two(),
            Some(4),
        ),
        intel_chars_scene(
            "intel_row_nearest_is_selected",
            fixtures::intel_typical(),
            [520.0, 520.0],
            fixtures::card_chars_nearest_is_selected(),
            Some(1),
        ),
        intel_chars_scene(
            "intel_row_two_characters_bridged",
            fixtures::intel_across_the_bridge(),
            [520.0, 520.0],
            fixtures::card_chars_bridged(),
            Some(3),
        ),
        intel_scene("intel_row_clear", fixtures::intel_clear(), 520.0),
        intel_scene("intel_row_torture", fixtures::intel_torture(), 520.0),
        intel_scene("intel_row_two_celestials", fixtures::intel_two_celestials(), 520.0),
        resolving_phases_scene("intel_row_resolving_phases"),
        // The feed is resizable, so the same card has to survive a narrow dock too.
        intel_scene("intel_row_torture_narrow", fixtures::intel_torture(), 320.0),
        // The 520-tall torture scenes cut off below the pilot chips, so the reporter footer is
        // only visible with room for the whole card.
        intel_scene_sized("intel_row_torture_full", fixtures::intel_torture(), [520.0, 1000.0]),
        intel_scene_sized(
            "intel_row_torture_narrow_full",
            fixtures::intel_torture(),
            [320.0, 1400.0],
        ),
        ping_scene("ping_fleet", fixtures::ping_fleet()),
        // The doctrine row carries a second link only when a doctrine URL is configured, which is
        // the case that decides whether the row may hold more than one item.
        ping_scene_with_doctrine_url(
            "ping_fleet_doctrine_link",
            fixtures::ping_fleet(),
            "https://example.invalid/doctrines",
        ),
        ping_scene("ping_fleet_no_doctrine", fixtures::ping_fleet_no_doctrine()),
        ping_scene("ping_plain", fixtures::ping_plain()),
        ping_scene("ping_plain_multiline", fixtures::ping_plain_multiline()),
        nav_scene("nav_rail_collapsed", false, 560.0),
        nav_scene("nav_rail_expanded", true, 560.0),
        // 460 is the app's minimum window height (main.rs), where the rail runs out of room for
        // its own rows and every item still has to stay reachable.
        nav_scene("nav_rail_collapsed_short", false, 460.0),
        nav_scene("nav_rail_expanded_short", true, 460.0),
        nav_scene("nav_rail_expanded_tall", true, 800.0),
    ];
    for (name, view) in [
        ("view_dashboard", View::Dashboard),
        ("view_intel", View::Intel),
        ("view_battles", View::Battles),
        ("view_characters", View::Characters),
        ("view_wormholes", View::Wormholes),
        ("view_lookup", View::Lookup),
        ("view_alerts", View::Alerts),
        ("view_settings", View::Settings),
        ("view_map", View::Map),
    ] {
        v.push(view_scene(name, view, [1280.0, 800.0]));
    }
    // 720 is the app's minimum window width (main.rs), where the settings path fields and their
    // Browse buttons have the least room to share.
    v.push(view_scene("view_settings_narrow", View::Settings, [720.0, 800.0]));
    // Tall enough to hold the whole settings column: the web section sits below Alerts, and a
    // scene that crops its own subject reads as coverage without being any.
    v.push(web_settings_scene("web_settings", [980.0, 2600.0]));
    v.push(web_settings_scene("web_settings_narrow", [720.0, 2600.0]));
    v.push(web_settings_scene_cfg("web_settings_bind_address", [980.0, 2600.0], true));
    // Both battle toolbars are one wrapping row of groups, so where they break moves with the
    // window. 720 breaks them into the most rows, which is where a divider is most likely to end
    // up at a row edge.
    v.push(view_scene("view_battles_narrow", View::Battles, [720.0, 800.0]));
    // 1440 is a break point where the throttle picker lands last on its row with the least space
    // left, next to the panel edge.
    v.push(view_scene("view_battles_wide", View::Battles, [1440.0, 800.0]));
    v.push(battle_detail_scene("view_battle_detail_narrow", [720.0, 800.0]));
    // 720 is the app's minimum window width (main.rs), where the intel toolbar has to wrap.
    v.push(view_scene("view_intel_narrow", View::Intel, [720.0, 800.0]));
    v.push(intel_bridge_scene("view_intel_feed", false, [1280.0, 800.0]));
    v.push(intel_bridge_states_scene("view_intel_feed_bridged", true, [1280.0, 800.0]));
    v.push(wormholes_rows_scene("view_wormholes_rows", [1280.0, 800.0]));
    // 720 is the app's minimum window width, where the eight-column table has the least room.
    v.push(wormholes_rows_scene("view_wormholes_rows_narrow", [720.0, 800.0]));
    v.push(map_layers_scene("map_layers", [320.0, 800.0]));
    // 520x480 is what `jabber_popout_windows` opens a new window at.
    v.push(jabber_popout_scene("jabber_popout", [520.0, 480.0], fixtures::JABBER_ROOM, ""));
    v.push(jabber_popout_scene(
        "jabber_popout_drafting",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        "reshipping, back in two\nbring a second logi\nand a scout for the gate",
    ));
    v.push(jabber_popout_scene(
        "jabber_popout_wrapped",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        DRAFT_WRAPPED,
    ));
    v.push(jabber_popout_scene(
        "jabber_popout_overflow",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        DRAFT_OVERFLOW,
    ));
    // A DM, which is the branch that draws the contact star and no room controls.
    v.push(jabber_popout_scene("jabber_popout_dm", [520.0, 480.0], fixtures::JABBER_DM, ""));
    // 360x260 is the pop-out's minimum inner size, where the tab bar overflows and the composer
    // and history have the least room.
    v.push(jabber_popout_scene("jabber_popout_min", [360.0, 260.0], fixtures::JABBER_ROOM, ""));
    v.push(jabber_popout_scene(
        "jabber_popout_min_overflow",
        [360.0, 260.0],
        fixtures::JABBER_ROOM,
        DRAFT_OVERFLOW,
    ));
    v.push(jabber_popout_seeded(
        "jabber_popout_long",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        "",
        None,
        fixtures::jabber_state_long,
    ));
    v.push(jabber_tab_drag_scene("jabber_popout_tab_drag", [520.0, 480.0], [200.0, 150.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_scene("fleet_list", [1280.0, 800.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_start_scene("fleet_start_form", [1440.0, 820.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_start_scene("fleet_start_form_narrow", [820.0, 1200.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_start_scene_with("fleet_start_already_tracked", [1440.0, 820.0], true));
    #[cfg(feature = "fleet")]
    v.push(fleet_advert_scene("fleet_advert_off", [1280.0, 1420.0], false));
    #[cfg(feature = "fleet")]
    v.push(fleet_side_search_scene("fleet_side_search", [1440.0, 820.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_dialog_scene("fleet_snowflakes", [700.0, 480.0], 0));
    #[cfg(feature = "fleet")]
    v.push(fleet_dialog_scene("fleet_migrate", [700.0, 460.0], 2));
    #[cfg(feature = "fleet")]
    v.push(fleet_dialog_scene("fleet_boost_detail", [760.0, 560.0], 1));
    #[cfg(feature = "fleet")]
    v.push(fleet_journal_scene("fleet_journal", [1280.0, 800.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_detail_scene("fleet_members", [1280.0, 1420.0], crate::app::fleet_ui::DetailTab::Members));
    #[cfg(feature = "fleet")]
    v.push(fleet_detail_scene_with(
        "fleet_members_closed",
        [1280.0, 1000.0],
        crate::app::fleet_ui::DetailTab::Members,
        true,
    ));
    #[cfg(feature = "fleet")]
    v.push(fleet_detail_scene(
        "fleet_composition",
        [1280.0, 1120.0],
        crate::app::fleet_ui::DetailTab::Composition,
    ));
    #[cfg(feature = "fleet")]
    v.push(fleet_thin_scene("fleet_composition_thin", [1280.0, 820.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_settings_scene("fleet_settings", [900.0, 620.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_boost_editor_scene("fleet_boost_editor", [860.0, 660.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_hull_editor_scene("fleet_hull_editor", [860.0, 660.0]));
    #[cfg(feature = "fleet")]
    v.push(fleet_quick_scene("fleet_quick", [520.0, 460.0]));
    #[cfg(feature = "fleet")]
    v.push(rescue_panel_scene("rescue_panel", [1100.0, 700.0]));
    #[cfg(feature = "fleet")]
    v.push(rescue_panel_scene_with("rescue_panel_op_in_use", [1100.0, 700.0], Some(3)));
    #[cfg(feature = "fleet")]
    v.push(fleet_confirm_scene(
        "fleet_confirm_kick_all",
        [560.0, 340.0],
        crate::fleets::backend::Action::KickAll,
        "Kick everyone out of the fleet?",
    ));
    #[cfg(feature = "fleet")]
    v.push(fleet_confirm_scene(
        "fleet_confirm_kick_pods",
        [560.0, 300.0],
        crate::fleets::backend::Action::KickCapsules,
        "Kick every pod out of the fleet?",
    ));
    #[cfg(feature = "fleet")]
    v.push(fleet_scene("fleet_list_narrow", [720.0, 700.0]));
    #[cfg(feature = "fleet")]
    v.push(rescue_chat_scene("rescue_chat_stamps", [420.0, 260.0]));
    #[cfg(feature = "fleet")]
    v.push(rescue_timer_scene("rescue_ping_timer", 372));
    #[cfg(feature = "fleet")]
    v.push(rescue_timer_scene("rescue_ping_timer_late", 931));
    // With Rescue Mode on, delve911's remove button is disabled while every other room
    // keeps its own. Only meaningful in a build that has the feature.
    #[cfg(feature = "fleet")]
    v.push(jabber_sidebar_scene_cfg("jabber_sidebar_rescue_pinned", [900.0, 560.0], true, true));
    v.push(characters_rows_scene("view_characters_rows", [1280.0, 800.0]));
    v.push(alert_rules_scene("view_alert_rules", [1280.0, 800.0], None));
    // The rule panel's 180px drag minimum, the least room a rule name ever gets.
    v.push(alert_rules_scene("view_alert_rules_narrow", [1280.0, 800.0], Some(180.0)));
    // Dialog sizes are the ones each dialog asks for in `dialog_viewport`, so a scene lays out at
    // the width the real window opens at. The three that are plain `egui::Window`s or `Modal`s get
    // room around them instead, since those float rather than fill.
    v.push(dialog_scene("dialog_severity", [620.0, 480.0], |a| a.severity_open = true));
    v.push(dialog_scene("dialog_intel_channels", [420.0, 480.0], |a| {
        a.intel_channels_open = true;
        a.settings.intel_channels =
            ["Delve Intel", "Querious Intel", "corp"].map(str::to_owned).into();
    }));
    // Capital 1DQ1-A at Zone 1: 7-K5EL sits 5.3 LY out (Zone 2) and Jita far beyond, so the
    // rows show a one-way bridge, an excluded one and an unresolved one.
    v.push(dialog_scene("dialog_jump_bridges", [460.0, 560.0], |a| {
        a.jump_bridges_open = true;
        a.systems = Some(fixtures::systems());
        a.settings.ansiblex_capital = "1DQ1-A".to_owned();
        a.settings.ansiblex_max_zone = 1;
        a.settings.jump_bridges = [("1DQ1-A", "O-EIMK"), ("319-3D", "7-K5EL"), ("7-K5EL", "Jita")]
            .map(|(from, to)| crate::settings::JumpBridge {
                from: from.to_owned(),
                to: to.to_owned(),
            })
            .into();
    }));
    // `coal_edit` is the dialog's edit buffer, filled by the settings button that opens it, so the
    // scene has to fill it the same way or the coalition list renders empty.
    v.push(dialog_scene("dialog_coalitions", [520.0, 680.0], |a| {
        a.coalitions_open = true;
        a.coal_edit =
            a.settings.coalitions.iter().map(|c| (c.name.clone(), c.alliances.join("\n"))).collect();
        a.settings.alliances = ["Goonswarm Federation", "Pandemic Horde"]
            .map(|name| crate::settings::AllianceConfig { name: name.to_owned(), color: None })
            .into();
    }));
    v.push(dialog_scene("dialog_battle_filter", [580.0, 620.0], |a| {
        use crate::settings::{BattleCond, BattleRule, RuleAction, ShipSize};
        a.battle_filter_open = true;
        a.settings.battles.rules = vec![
            BattleRule {
                action: RuleAction::Include,
                match_all: true,
                conditions: vec![
                    BattleCond::Region("Delve".into()),
                    BattleCond::HullSizeAtLeast(ShipSize::Capital),
                ],
                expanded: true,
            },
            BattleRule {
                action: RuleAction::Exclude,
                match_all: false,
                conditions: vec![BattleCond::IskAtMost(500_000_000.0)],
                expanded: true,
            },
        ];
    }));
    v.push(dialog_scene("dialog_routes", [640.0, 620.0], |a| {
        a.routes_dialog_open = true;
        a.systems = Some(fixtures::systems());
        a.settings.route_folders = vec!["Deployments".into()];
        a.settings.saved_routes = vec![
            crate::settings::SavedRoute {
                name: "Home run".into(),
                folder: "Deployments".into(),
                start: 30_004_759,
                end: 30_000_142,
                waypoints: vec![30_004_608],
                jumps: 12,
                constraints: None,
            },
            crate::settings::SavedRoute {
                name: "Staging".into(),
                folder: String::new(),
                start: 30_004_608,
                end: 30_003_704,
                waypoints: vec![],
                jumps: 3,
                constraints: None,
            },
        ];
    }));
    v.push(dialog_scene("dialog_filter_picker", [520.0, 620.0], |a| {
        let mut p = crate::pickers::FilterPicker::new(crate::pickers::PickerKind::Ships, 0);
        p.data = crate::pickers::PickerData::List(
            ["Kikimora", "Cenotaph", "Muninn", "Eagle", "Nightmare", "Revelation"]
                .map(str::to_owned)
                .into(),
        );
        p.selected = ["Muninn".to_owned(), "Revelation".to_owned()].into_iter().collect();
        a.filter_picker = Some(p);
    }));
    v.push(dialog_scene("dialog_verdict_explainer", [520.0, 400.0], |a| {
        a.verdict_explainer_open = true;
    }));
    v
}

/// How many click targets each scene actually exposes. A scene near zero is not being checked in
/// any meaningful sense, so run this before trusting a clean layout result.
#[test]
#[ignore = "coverage census; run with --ignored --nocapture"]
fn uitest_census() {
    for mut scene in all() {
        let size = scene.size;
        let name = scene.name;
        let mut harness = harness::build(&mut scene, false);
        let report = super::checks::inspect(&mut harness, size);
        let tightest = report
            .smallest
            .first()
            .map(|(d, w)| format!("{d:.0}px  {w}"))
            .unwrap_or_else(|| "-".into());
        let roles: Vec<String> =
            report.roles.iter().map(|(r, n)| format!("{r}:{n}")).collect();
        println!("{:>28}  {:>3} hit targets   smallest: {}", name, report.hit_targets, tightest);
        println!("{:>28}  roles: {}", "", roles.join(" "));
    }
}

#[test]
fn uitest_layout() {
    let mut failures = Vec::new();
    for mut scene in all() {
        let size = scene.size;
        let name = scene.name;
        let mut harness = harness::build(&mut scene, false);
        let report = super::checks::inspect(&mut harness, size);
        if !report.is_empty() {
            failures.push(report.render(name));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
#[ignore = "renders every scene to target/uishots; run with --ignored"]
fn uitest_screenshots() {
    for mut scene in all() {
        let name = scene.name;
        let mut harness = harness::build(&mut scene, true);
        harness::shot(&mut harness, name);
        harness::shot_debug(&mut harness, name);
    }
}

/// The pin sits at the right end of the tab-bar row, so it shares that row with the overflow caret,
/// follows it rather than covering it, and stays inside the window at both sizes.
#[test]
fn uitest_jabber_popout_pin_is_in_the_tab_bar() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT as _;

    for name in ["jabber_popout", "jabber_popout_min"] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let size = scene.size;
        let harness = harness::build(&mut scene, false);
        let mut pin = None;
        let mut caret = None;
        let mut others = Vec::new();
        for node in harness.root().children_recursive() {
            let n = node.accesskit_node();
            if n.is_hidden() || n.role() != Role::Button {
                continue;
            }
            let Some(b) = n.bounding_box() else { continue };
            let r = egui::Rect {
                min: egui::pos2(b.x0 as f32, b.y0 as f32),
                max: egui::pos2(b.x1 as f32, b.y1 as f32),
            };
            let label = n.label().unwrap_or_default().to_string();
            if label.contains(egui_phosphor::regular::PUSH_PIN) {
                pin = Some(r);
            } else if label.contains(egui_phosphor::regular::CARET_DOWN) {
                caret = Some(r);
            } else {
                others.push((label, r));
            }
        }
        let pin = pin.unwrap_or_else(|| panic!("{name}: no always-on-top pin"));
        let caret = caret.unwrap_or_else(|| panic!("{name}: no overflow caret"));
        assert!(
            pin.max.x <= size.x && pin.min.x >= 0.0 && pin.min.y >= 0.0,
            "{name}: the pin left the window: {pin:?} in {size:?}"
        );
        assert!(
            pin.min.x >= caret.max.x,
            "{name}: the pin is not clear of the overflow caret: {pin:?} vs {caret:?}"
        );
        assert!(
            pin.min.y < caret.max.y && caret.min.y < pin.max.y,
            "{name}: the pin left the tab-bar row: {pin:?} vs {caret:?}"
        );
        for (label, r) in others {
            assert!(
                !r.intersects(pin),
                "{name}: the pin covers {label:?}: {pin:?} over {r:?}"
            );
        }
    }
}

/// Rects of everything the toolbar puts in the flow. Dividers are painted decoration and emit no
/// node of their own, so this is what has to sit on either side of one.
fn content_rects(harness: &egui_kittest::Harness<'_>) -> Vec<egui::Rect> {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT as _;

    let mut out = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden()
            || !matches!(
                n.role(),
                Role::Button
                    | Role::CheckBox
                    | Role::ComboBox
                    | Role::TextInput
                    | Role::SpinButton
                    | Role::Label
                    | Role::Image
            )
        {
            continue;
        }
        let Some(b) = n.bounding_box() else { continue };
        out.push(egui::Rect {
            min: egui::pos2(b.x0 as f32, b.y0 as f32),
            max: egui::pos2(b.x1 as f32, b.y1 as f32),
        });
    }
    out
}

/// A divider is a group boundary, so one at the start or the end of a row divides nothing. The
/// battles toolbar is a single wrapping row and its break points move with the window, so sweep
/// the whole range from the app's minimum width up.
#[test]
fn uitest_toolbar_dividers_keep_content_on_both_sides() {
    for (name, view) in
        [("battles_divider_probe", View::Battles), ("intel_divider_probe", View::Intel)]
    {
        for w in (720..=1600).step_by(40).map(|w| w as f32) {
            let mut scene = view_scene(name, view, [w, 800.0]);
            let harness = harness::build(&mut scene, false);
            let seps = crate::app::painted_toolbar_seps(&harness.ctx);
            assert!(!seps.is_empty(), "no toolbar divider painted at all at {w}px in {name}");
            let content = content_rects(&harness);
            for sep in &seps {
                let row = |r: &&egui::Rect| {
                    let y = r.center().y;
                    sep.top() - 2.0 < y && y < sep.bottom() + 2.0
                };
                assert!(
                    content.iter().filter(row).any(|r| r.right() <= sep.left() + 0.5),
                    "divider at {sep:?} starts a row at {w}px in {name}"
                );
                assert!(
                    content.iter().filter(row).any(|r| r.left() >= sep.right() - 0.5),
                    "divider at {sep:?} ends a row at {w}px in {name}"
                );
            }
        }
    }
}

/// A `ComboBox` that claims only the row space left over instead of the width it paints overflows
/// the window wherever the toolbar breaks just before one. Failing widths sit between widths that
/// look fine, so sweep rather than spot-check.
#[test]
fn uitest_battles_toolbar_stays_inside_the_window() {
    let mut failures = Vec::new();
    for w in (720..=1600).step_by(40).map(|w| w as f32) {
        let size = egui::vec2(w, 800.0);
        let mut scene = view_scene("battles_width_probe", View::Battles, [w, 800.0]);
        let mut harness = harness::build(&mut scene, false);
        let report = super::checks::inspect(&mut harness, size);
        for esc in &report.offscreen {
            failures.push(format!("  {w}px: {esc}"));
        }
        if let Some(o) = &report.overflow {
            failures.push(format!("  {w}px: {o}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The resolving placeholder is the only chip whose whole label is the pilot icon plus dots.
fn resolving_chip(harness: &egui_kittest::Harness<'_>) -> (String, egui::Rect) {
    use egui_kittest::kittest::NodeT as _;

    let mut found = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Button {
            continue;
        }
        let label = n.label().unwrap_or_default();
        let rest = label.trim_start_matches(egui_phosphor::regular::USER);
        if rest.is_empty() || !rest.chars().all(|c| c == '.' || c == ' ') {
            continue;
        }
        let b = n.bounding_box().expect("chip has a bounding box");
        let rect = egui::Rect {
            min: egui::pos2(b.x0 as f32, b.y0 as f32),
            max: egui::pos2(b.x1 as f32, b.y1 as f32),
        };
        found.push((label, rect));
    }
    assert_eq!(found.len(), 1, "expected exactly one resolving chip, got {found:?}");
    found.remove(0)
}

/// The chip animates its dots, and it sits in a wrapped flow, so any width it gains with the
/// phase shifts every chip after it and can bounce one across the line break twice a second.
#[test]
fn uitest_intel_row_resolving_chip_holds_its_width() {
    let mut seen: Vec<(String, egui::Rect)> = Vec::new();
    // Consecutive seconds hit all three phases, since the phase is `now * 2 % 3`.
    for step in 0..3 {
        let clock = fixtures::now() + step;
        let args = IntelArgs::default();
        let report = fixtures::intel_resolving(clock);
        let mut scene = Scene::ui("resolving_probe", [520.0, 520.0], move |ui| {
            let mut t = None;
            crate::app::intel_row(
                ui,
                &report,
                clock,
                false,
                None,
                crate::app::JumpVia::Gates,
                &args.chars,
                &args.systems,
                &args.status,
                &args.ship_details,
                &args.ship_roles,
                &args.resolved_pilots,
                &args.uncertain,
                &args.last_ship,
                &args.kills,
                crate::settings::Severity::Danger,
                true,
                &args.affil,
                &args.notes,
                false,
                &mut t,
            );
        });
        let harness = harness::build(&mut scene, false);
        seen.push(resolving_chip(&harness));
    }

    let phases: std::collections::BTreeSet<&str> = seen.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(phases.len(), 3, "the three clocks did not produce three phases: {seen:?}");
    let first = seen[0].1;
    for (label, rect) in &seen {
        assert_eq!(*rect, first, "phase {label:?} moved or resized the chip: {seen:?}");
    }
}

/// The chip is disabled, so its explanation only reaches the user through the disabled-hover
/// tooltip. `on_hover_text` on a disabled widget is silently dropped by egui.
#[test]
fn uitest_intel_row_resolving_chip_explains_itself_on_hover() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_resolving(fixtures::now());
    let mut scene = Scene::ui("resolving_hover_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
    });
    let mut harness = harness::build(&mut scene, false);
    assert!(harness.query_by_label_contains("Resolving pilot").is_none());
    let at = resolving_chip(&harness).1.center();
    harness.event(egui::Event::PointerMoved(at));
    harness.run_steps(3);
    assert!(
        harness.query_by_label_contains("Resolving pilot").is_some(),
        "hovering the resolving chip at {at:?} showed no tooltip"
    );
}

/// Compact mode routes tooltips through the `tip` out-param instead of egui, because the alert
/// overlay draws them itself. Hovering a pilot chip has to fill it in.
#[test]
fn uitest_intel_row_hover_sets_tip_when_compact() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let tip = std::rc::Rc::new(std::cell::RefCell::new(None));
    let sink = tip.clone();
    let mut scene = Scene::ui("hover_probe_compact", [520.0, 520.0], move |ui| {
        let mut t = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            true,
            &mut t,
        );
        if t.is_some() {
            *sink.borrow_mut() = t;
        }
    });
    let mut harness = harness::build(&mut scene, false);
    harness.get_by_label_contains("Hostile Pilot").hover();
    harness.run_steps(2);
    assert!(tip.borrow().is_some(), "hovering a pilot chip in compact mode produced no tooltip");
}

/// The full-size card uses egui's own tooltip, so the hint has to show up in the tree on hover
/// and must not be there before it.
#[test]
fn uitest_intel_row_hover_shows_tooltip() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let mut scene = Scene::ui("hover_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
    });
    let mut harness = harness::build(&mut scene, false);
    assert!(harness.query_by_label_contains("Click to look up").is_none());
    harness.get_by_label_contains("Hostile Pilot").hover();
    harness.run_steps(3);
    assert!(
        harness.query_by_label_contains("Click to look up").is_some(),
        "hovering a pilot chip showed no tooltip"
    );
}

#[test]
fn uitest_system_badge_tooltip_shows_light_years() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let locations = std::collections::HashMap::from([("Amryu".to_owned(), (30_003_704, false))]);
    let chars = crate::app::build_char_rings(
        &args.systems,
        &[("Amryu".to_owned(), 0)],
        &locations,
        "Amryu",
        None,
        &[],
        false,
        false,
    )
    .with_staging(Some("319-3D"))
    .card_for(&report);
    let mut scene = Scene::ui("ly_hover_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
    });
    let mut harness = harness::build(&mut scene, false);
    assert!(harness.query_by_label_contains("ly from").is_none());
    let badge = format!("{} 1DQ1-A", egui_phosphor::regular::PLANET);
    harness.get_by_label(&badge).hover();
    harness.run_steps(3);
    assert!(harness.query_by_label("2.10 ly from staging 319-3D").is_some());
    assert!(harness.query_by_label("5.33 ly from Amryu").is_some());
}

/// The quick menu edits the quick-edit folder only: Hunter is set in another folder, so it shows
/// unticked with that folder named, and ticking it adds it to the target.
#[test]
fn uitest_pilot_chip_menu_toggles_a_tag_in_the_target_folder() {
    use egui_kittest::kittest::Queryable as _;

    let book = fixtures::notebook();
    let view = book.view("");
    let target = view.target.clone();
    let hunter = view.pilot_tags.iter().find(|t| t.name == "Hunter").unwrap().id.clone();
    let args = IntelArgs { notes: view, ..IntelArgs::default() };
    let report = fixtures::intel_typical();
    let clicks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = clicks.clone();
    let mut scene = Scene::ui("notes_menu_probe", [620.0, 700.0], move |ui| {
        let mut t = None;
        let hit = crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
        if let Some(hit) = hit {
            sink.borrow_mut().push(hit);
        }
    });
    let mut harness = harness::build(&mut scene, false);
    harness.get_by_label_contains("Hostile Pilot").click_secondary();
    harness.run_steps(3);
    assert!(harness.query_all_by_label_contains("also in Coalition intel / Delve").next().is_some());
    // User tags follow the fourteen built-in pilot tags, below the menu's scroll fold.
    harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Hunter").scroll_to_me();
    harness.run_steps(3);
    harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Hunter").click();
    harness.run_steps(2);
    let got = clicks.borrow();
    assert!(
        got.iter().any(|c| matches!(
            c,
            crate::app::IntelClick::Notes(crate::notes::NotesOp::SetTag { folder, tag, on: true, .. })
                if *folder == target && *tag == hunter
        )),
        "ticking Hunter yielded {got:?}"
    );
}

/// Draws one card and reports its height, so two `show_reporter` settings can be compared.
fn intel_card_height(name: &'static str, show_reporter: bool) -> f32 {
    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let height = std::rc::Rc::new(std::cell::Cell::new(0.0));
    let sink = height.clone();
    let mut scene = Scene::ui(name, [520.0, 520.0], move |ui| {
        let mut t = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            show_reporter,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
        sink.set(ui.min_rect().height());
    });
    let _ = harness::build(&mut scene, false);
    height.get()
}

/// The reporter and channel are a footer, so they start their own row below every chip instead of
/// trailing the last badge into the wrapped flow, and they leave no row behind when hidden.
#[test]
fn uitest_intel_row_reporter_is_a_footer() {
    use egui_kittest::kittest::NodeT as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let mut scene = Scene::ui("footer_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
    });
    let harness = harness::build(&mut scene, false);

    let rect = |b: egui::accesskit::Rect| egui::Rect {
        min: egui::pos2(b.x0 as f32, b.y0 as f32),
        max: egui::pos2(b.x1 as f32, b.y1 as f32),
    };
    let mut footer = None;
    let mut chips: Vec<egui::Rect> = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        let Some(b) = n.bounding_box() else { continue };
        match n.role() {
            egui::accesskit::Role::Button => chips.push(rect(b)),
            egui::accesskit::Role::Label if label.contains("Scout Alpha") => {
                footer = Some(rect(b))
            }
            _ => {}
        }
    }
    let footer = footer.expect("the card drew no reporter footer");
    assert!(!chips.is_empty(), "the card drew no chips to place the footer under");
    let lowest = chips.iter().fold(f32::MIN, |acc, r| acc.max(r.max.y));
    assert!(
        footer.min.y >= lowest - 0.5,
        "footer at {footer:?} shares a row with a chip reaching {lowest}"
    );
    let leftmost = chips.iter().fold(f32::MAX, |acc, r| acc.min(r.min.x));
    assert!(
        footer.min.x <= leftmost + 0.5,
        "footer at {footer:?} is indented past the leftmost chip at x={leftmost}"
    );

    let with = intel_card_height("footer_height_on", true);
    let without = intel_card_height("footer_height_off", false);
    assert!(
        without < with - 10.0,
        "hiding the reporter saved {:.1}px, so a row was left behind (on {with:.1}, off {without:.1})",
        with - without
    );
}

/// Height of the first `Label` node whose text contains `needle`. Text rendered at a reduced size
/// lands in a shorter box, which is the only signal for font size the AccessKit tree carries.
fn label_height(harness: &egui_kittest::Harness<'_>, needle: &str) -> f32 {
    use egui_kittest::kittest::NodeT as _;

    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label {
            continue;
        }
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        if !label.contains(needle) {
            continue;
        }
        if let Some(b) = n.bounding_box() {
            return (b.y1 - b.y0) as f32;
        }
    }
    panic!("no label containing {needle:?}");
}

/// The column headers and the drifter warning are read to pick a hole, so they carry the same body
/// size as the cells under them.
#[test]
fn uitest_wormhole_table_text_is_body_size() {
    let mut scene = wormholes_rows_scene("wormhole_text_probe", [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);

    let cell = label_height(&harness, "O-EImg");
    for header in ["Constellation", "Region", "Life"] {
        let h = label_height(&harness, header);
        assert!(
            h >= cell - 0.5,
            "column header {header:?} is {h:.1}px tall against a {cell:.1}px cell"
        );
    }
    let drifter = label_height(&harness, "drifter");
    assert!(
        drifter >= cell - 0.5,
        "the drifter tag is {drifter:.1}px tall against a {cell:.1}px cell"
    );
}

/// Who sent a ping and who it went to decides whether it applies to you, so the footer stays at
/// body size instead of shrinking below the call text.
#[test]
fn uitest_ping_footer_is_body_size() {
    let mut scene = ping_scene("ping_footer_probe", fixtures::ping_fleet());
    let harness = harness::build(&mut scene, false);

    let body = label_height(&harness, "FC: Fleet Commander");
    let footer = label_height(&harness, "goonfleet");
    assert!(
        footer >= body - 0.5,
        "the ping footer is {footer:.1}px tall against a {body:.1}px metadata row"
    );
}

/// Every jump-distance chip on screen, in tree order. The card pads the text to four monospace
/// columns, so the label carries leading spaces.
fn jump_chips(harness: &egui_kittest::Harness<'_>) -> Vec<String> {
    use egui_kittest::kittest::NodeT as _;

    let mut out = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label || n.is_hidden() {
            continue;
        }
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        let t = label.trim();
        if t == "here"
            || (t.ends_with('j') && t.len() > 1 && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit()))
        {
            out.push(t.to_owned());
        }
    }
    out
}

/// Every character badge on the card, by the portrait's alt text, in tree order.
fn char_badges(harness: &egui_kittest::Harness<'_>) -> Vec<String> {
    use egui_kittest::kittest::NodeT as _;

    harness
        .root()
        .children_recursive()
        .map(|node| node.accesskit_node())
        .filter(|n| n.role() == egui::accesskit::Role::Button && !n.is_hidden())
        .filter_map(|n| n.label().map(|l| l.to_owned()))
        .filter(|l| l == "Amryu" || l == "Scout Alt")
        .collect()
}

/// The left edge of the first jump number, a column that has to hold across cards.
fn first_jump_x(harness: &egui_kittest::Harness<'_>) -> Option<f32> {
    use egui_kittest::kittest::NodeT as _;

    harness
        .root()
        .children_recursive()
        .map(|node| node.accesskit_node())
        .filter(|n| n.role() == egui::accesskit::Role::Label && !n.is_hidden())
        .find(|n| {
            let l = n.label().or_else(|| n.value()).unwrap_or_default();
            let t = l.trim();
            t == "here" || (t.ends_with('j') && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit()))
        })
        .and_then(|n| n.bounding_box())
        .map(|b| b.x0 as f32)
}

/// The alert engine fires on the nearest alert-enabled character, so a card quoting only the
/// selected one could read four jumps for a report that alerted at one.
#[test]
fn uitest_intel_card_attributes_the_nearest_character() {
    let mut scene = all().into_iter().find(|s| s.name == "intel_row_two_characters").expect("scene");
    let harness = harness::build(&mut scene, false);
    assert_eq!(
        jump_chips(&harness),
        ["1j", "4j"],
        "the nearest character's distance comes first, the selected one's second"
    );
    assert_eq!(
        char_badges(&harness),
        ["Scout Alt", "Amryu"],
        "each number carries the portrait of the character it belongs to"
    );
}

/// The portrait reads as a player, so it wants the height the row already has, not the height of
/// the text beside it.
#[test]
fn uitest_intel_card_badge_is_as_tall_as_the_chips_beside_it() {
    use egui_kittest::kittest::NodeT as _;

    let mut scene = all().into_iter().find(|s| s.name == "intel_row_two_characters").expect("scene");
    let harness = harness::build(&mut scene, false);
    let mut badge: Option<f32> = None;
    let mut chip: Option<f32> = None;
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Button || n.is_hidden() {
            continue;
        }
        let label = n.label().unwrap_or_default().to_owned();
        let Some(b) = n.bounding_box() else { continue };
        let h = (b.y1 - b.y0) as f32;
        if label == "Scout Alt" {
            badge = Some(h);
        } else if label.contains("1DQ1-A") {
            chip = Some(h);
        }
    }
    let badge = badge.expect("no portrait button");
    let chip = chip.expect("no system chip");
    assert!(
        (badge - chip).abs() < 0.5,
        "the portrait stands {badge:.1}px against a {chip:.1}px chip on the same row"
    );
    let line = theme_body_line();
    assert!(
        badge > line + 4.0,
        "the portrait is {badge:.1}px against {line:.1}px of body text, so it shrank back to \
         text size"
    );
}

/// A compact card carries both slots too: the number you are used to reading disappearing when the
/// feed is docked narrow is more confusing than the width it costs.
#[test]
fn uitest_intel_card_compact_shows_both_characters() {
    let mut scene =
        all().into_iter().find(|s| s.name == "intel_row_two_characters_narrow").expect("scene");
    let harness = harness::build(&mut scene, false);
    assert_eq!(jump_chips(&harness), ["1j", "4j"]);
    assert_eq!(char_badges(&harness), ["Scout Alt", "Amryu"]);
}

/// Nothing to disambiguate, so one number, but the portrait stays: the card should not change
/// shape as an alt moves in and out of being the nearest.
#[test]
fn uitest_intel_card_keeps_the_badge_when_nearest_is_selected() {
    let mut scene =
        all().into_iter().find(|s| s.name == "intel_row_nearest_is_selected").expect("scene");
    let harness = harness::build(&mut scene, false);
    assert_eq!(jump_chips(&harness), ["1j"]);
    assert_eq!(char_badges(&harness), ["Amryu"]);
}

/// One character is the case that must look exactly like it always did.
#[test]
fn uitest_intel_card_draws_no_badge_for_one_character() {
    let mut scene = intel_chars_scene(
        "intel_row_one_character",
        fixtures::intel_typical(),
        [520.0, 520.0],
        crate::app::CardChars::default(),
        Some(1),
    );
    let harness = harness::build(&mut scene, false);
    assert_eq!(jump_chips(&harness), ["1j"]);
    assert!(char_badges(&harness).is_empty(), "a lone character has nobody to be told apart from");
}

/// The badge is on every card in a multi-character feed, so the first number's column holds whether
/// or not a card also carries a second slot.
#[test]
fn uitest_intel_card_jump_column_holds_its_x() {
    let x = |name: &str| {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let harness = harness::build(&mut scene, false);
        first_jump_x(&harness)
    };
    let two = x("intel_row_two_characters").expect("two-slot card");
    let one = x("intel_row_nearest_is_selected").expect("one-slot card");
    assert!(
        (two - one).abs() < 0.5,
        "the first jump number moved {:.1}px between a card with a second slot and one without",
        two - one
    );
}

/// A per-character verdict is the thing one shared `JumpVia` could not express: the nearest
/// character rides a bridge here and the other one does not. The mark is a colour, which the
/// AccessKit tree cannot see, so the two halves are asserted where each is observable.
#[test]
fn uitest_intel_card_marks_a_bridge_per_character() {
    use crate::app::jump_chip_style;

    let chars = fixtures::card_chars_bridged();
    assert_ne!(
        jump_chip_style(chars.hops[0].via).0,
        jump_chip_style(chars.hops[1].via).0,
        "both characters' numbers would be drawn in the same colour"
    );
    assert_eq!(jump_chip_style(chars.hops[0].via).0, crate::theme::standing::ALLIANCE);

    let mut scene =
        all().into_iter().find(|s| s.name == "intel_row_two_characters_bridged").expect("scene");
    let harness = harness::build(&mut scene, false);
    assert_eq!(jump_chips(&harness), ["1j", "3j"]);
    assert!(
        bridge_marks(&harness).is_empty(),
        "a shortcut is purple and wordless, so neither slot writes anything beside its number"
    );
}

/// Clicking a badge opens the whole roster, which is where a compact card's second distance lives
/// and where a third character shows up at all.
#[test]
fn uitest_intel_card_badge_opens_the_character_roster() {
    use egui_kittest::kittest::NodeT as _;

    let mut scene = all().into_iter().find(|s| s.name == "intel_row_two_characters").expect("scene");
    let mut harness = harness::build(&mut scene, false);

    let names = |h: &egui_kittest::Harness<'_>| -> Vec<String> {
        h.root()
            .children_recursive()
            .map(|node| node.accesskit_node())
            .filter(|n| n.role() == egui::accesskit::Role::Label)
            .filter_map(|n| n.label().or_else(|| n.value()).map(|l| l.to_owned()))
            .filter(|l| l == "Amryu" || l == "Scout Alt")
            .collect()
    };
    assert!(names(&harness).is_empty(), "the roster is closed until the badge is clicked");

    use egui_kittest::kittest::Queryable as _;
    harness.get_by_label("Scout Alt").click();
    harness.run_steps(2);

    let mut rows = names(&harness);
    rows.sort();
    assert_eq!(rows, ["Amryu", "Scout Alt"], "the roster lists every alert-enabled character");
    let chips = jump_chips(&harness);
    assert!(
        chips.iter().filter(|c| *c == "1j").count() >= 2
            && chips.iter().filter(|c| *c == "4j").count() >= 2,
        "each roster row carries that character's own distance: {chips:?}"
    );
}

/// The attribution has to survive the IPC frame, because the overlay subprocess holds neither the
/// roster nor anyone's location and cannot derive it.
#[test]
fn uitest_alert_window_shows_attribution_sent_over_ipc() {
    let mut scene = all().into_iter().find(|s| s.name == "alert_window_chars").expect("scene");
    let harness = harness::build(&mut scene, false);
    let badges = char_badges(&harness);
    assert!(
        badges.iter().any(|b| b == "Scout Alt") && badges.iter().any(|b| b == "Amryu"),
        "the wire dropped the attribution: {badges:?}"
    );
    assert!(jump_chips(&harness).iter().any(|c| c == "1j"), "the nearest number crossed too");
}

/// The banner has to say the archive stopped, not just that space is low, and it must not
/// offer a Dismiss for a state that is still degrading the app.
#[test]
fn uitest_disk_banner_says_what_stopped_and_cannot_be_dismissed_when_critical() {
    use egui_kittest::kittest::NodeT as _;

    let text = |name: &str| -> String {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let harness = harness::build(&mut scene, false);
        harness
            .root()
            .children_recursive()
            .map(|node| node.accesskit_node())
            .filter(|n| !n.is_hidden())
            .filter_map(|n| n.label().or_else(|| n.value()).map(|l| l.to_owned()))
            .collect::<Vec<_>>()
            .join(" | ")
    };

    let low = text("disk_banner_low");
    assert!(low.contains("Low disk space"), "{low}");
    assert!(low.contains("780 MB free"), "the reading is not on screen: {low}");
    assert!(low.contains("Dismiss"), "a warning with room to act should be dismissible");

    let crit = text("disk_banner_critical");
    assert!(crit.contains("Disk almost full"), "{crit}");
    assert!(
        crit.contains("stopped recording history") && crit.contains("alerts still work"),
        "the banner must say what stopped and what did not: {crit}"
    );
    assert!(
        !crit.contains("Dismiss"),
        "a live degradation must not be dismissible, it clears itself: {crit}"
    );

    // A stale number is a worse answer than what actually happened.
    let failed = text("disk_banner_critical_after_a_failed_write");
    assert!(failed.contains("A save just failed"), "{failed}");
}

/// The banner sits between the top bar and the nav rail, so an over-tall one pushes the status bar
/// out of the window. Both have to stay in frame at both levels.
#[test]
fn uitest_disk_banner_does_not_displace_the_chrome() {
    use egui_kittest::kittest::NodeT as _;

    for name in ["disk_banner_low", "disk_banner_critical"] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let harness = harness::build(&mut scene, false);
        let mut seen_status = false;
        let mut lowest = 0.0f32;
        for node in harness.root().children_recursive() {
            let n = node.accesskit_node();
            if n.is_hidden() {
                continue;
            }
            let label = n.label().or_else(|| n.value()).unwrap_or_default();
            if let Some(b) = n.bounding_box() {
                lowest = lowest.max(b.y1 as f32);
            }
            // The status bar's own text, which is the thing a tall banner would push off.
            if label.starts_with("Intel:") {
                seen_status = true;
            }
        }
        assert!(seen_status, "{name}: the status bar left the window");
        assert!(lowest <= 800.5, "{name}: content reached {lowest:.1}px in an 800px window");
    }
}

/// The card's jump distance follows the bridge setting, or a hostile who cannot use your bridges
/// reads as closer than the alert that fired on it. Both answers come from one fixture, since a
/// single number proves nothing about which graph was used.
#[test]
fn uitest_intel_card_jumps_follow_the_bridge_setting() {
    let read = |count_bridges: bool| {
        let mut scene = intel_bridge_scene("bridge_probe", count_bridges, [1280.0, 800.0]);
        let harness = harness::build(&mut scene, false);
        jump_chips(&harness)
    };
    assert_eq!(read(false), ["2j"], "gate-only did not walk the gates");
    assert_eq!(read(true), ["1j"], "counting bridges did not take the bridge");
}

/// Every jump-bridge marker the feed drew, in tree order.
fn bridge_marks(harness: &egui_kittest::Harness<'_>) -> Vec<String> {
    use egui_kittest::kittest::NodeT as _;

    let mut out = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label || n.is_hidden() {
            continue;
        }
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        // A bridge that merely shortens the trip is purple and nothing else, so the only mark
        // still written in words is the route gates cannot reach at all.
        if label.trim() == "bridge only" || label.contains(egui_phosphor::regular::ARROWS_LEFT_RIGHT)
        {
            out.push(label.trim().to_owned());
        }
    }
    out
}

/// With the setting on, a card's distance can be short by a few jumps or exist only
/// because of a bridge, and a hostile can use neither. All three states share one feed, since a
/// mark means nothing without an unmarked card beside it.
#[test]
fn uitest_intel_card_marks_a_bridge_dependent_range() {
    let read = |count_bridges: bool| {
        let mut scene =
            intel_bridge_states_scene("bridge_states_probe", count_bridges, [1280.0, 800.0]);
        let harness = harness::build(&mut scene, false);
        (jump_chips(&harness), bridge_marks(&harness))
    };

    let (chips, marks) = read(true);
    assert_eq!(chips, ["1j", "1j", "1j"], "counting bridges did not take both bridges");
    assert_eq!(
        marks,
        ["bridge only"],
        "a shortcut is purple and wordless now, and only an unreachable-by-gate route says so"
    );

    let (chips, marks) = read(false);
    assert_eq!(chips, ["2j", "1j"], "gate-only did not walk the gates, or Jita gained a route");
    assert!(marks.is_empty(), "gate-only distances were marked as bridged: {marks:?}");
}

/// The overlay is handed a jump number over IPC and has no graph to work out what it rests on, so
/// the verdict has to travel with it. The same feed without `via` draws no mark at all.
#[test]
fn uitest_alert_window_marks_a_bridge_dependent_range() {
    let read = |send_via: bool| {
        let mut scene =
            alert_window_ipc_scene("alert_bridge_probe", alert_bridge_cards(), send_via);
        let harness = harness::build(&mut scene, false);
        (jump_chips(&harness), bridge_marks(&harness))
    };

    let (chips, marks) = read(true);
    assert_eq!(chips, ["1j", "1j", "1j"], "the overlay lost the jump numbers");
    assert_eq!(
        marks,
        ["bridge only"],
        "the overlay lost the one route gates cannot reach, or marked a card that has one"
    );

    let (chips, marks) = read(false);
    assert_eq!(chips, ["1j", "1j", "1j"], "the numbers depend on the verdict");
    assert!(marks.is_empty(), "a mark appeared with no verdict on the wire: {marks:?}");
}

/// The gap between a card's jump number and its first system chip, so an extra widget wedged
/// between them shows up as width rather than having to be found by name.
fn jump_to_chip_gap(harness: &egui_kittest::Harness<'_>, system: &str) -> f32 {
    use egui_kittest::kittest::NodeT as _;

    let mut numbers: Vec<(f32, f32, f32)> = Vec::new();
    let mut chip: Option<(f32, f32, f32)> = None;
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden() {
            continue;
        }
        let Some(b) = n.bounding_box() else { continue };
        let r = (b.x0 as f32, b.x1 as f32, b.y0 as f32);
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        match n.role() {
            egui::accesskit::Role::Label if label.trim().ends_with('j') => numbers.push(r),
            egui::accesskit::Role::Button if label.contains(system) => chip = Some(r),
            _ => {}
        }
    }
    let chip = chip.unwrap_or_else(|| panic!("no {system} chip on screen"));
    // Same row as the chip, and the nearest number to its left.
    numbers
        .into_iter()
        .filter(|n| (n.2 - chip.2).abs() < 6.0 && n.1 <= chip.0 + 0.5)
        .map(|n| chip.0 - n.1)
        .fold(f32::MAX, f32::min)
}

/// The mark is an extra widget on the row, so a card that did not earn one must carry nothing at
/// all between its number and its first chip, not even reserved width.
#[test]
fn uitest_bridge_mark_only_takes_width_on_the_card_that_earned_it() {
    let mut scene = intel_bridge_states_scene("bridge_align_probe", true, [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);
    let plain = jump_to_chip_gap(&harness, "319-3D");
    let shortcut = jump_to_chip_gap(&harness, "7-K5EL");
    let bridge_only = jump_to_chip_gap(&harness, "Jita");
    let spacing = harness.ctx.global_style().spacing.item_spacing.x;
    assert!(
        plain <= spacing + 0.5,
        "an unbridged card holds {plain:.1}px between its number and its chip, \
         against {spacing:.1}px of plain item spacing"
    );
    assert!(
        (shortcut - plain).abs() < 0.5,
        "a bridge shortcut is colour only, so it must cost the same width as no mark at all: \
         {shortcut:.1}px against {plain:.1}px"
    );
    assert!(
        bridge_only > plain + 4.0,
        "the bridge-only card lost its words: {bridge_only:.1}px against {plain:.1}px"
    );
}

/// The number column holds: the mark rides after the number, never in front of it.
#[test]
fn uitest_bridge_mark_holds_the_jump_column() {
    use egui_kittest::kittest::NodeT as _;

    let mut scene = intel_bridge_states_scene("bridge_column_probe", true, [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);
    let mut xs: Vec<f32> = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label || n.is_hidden() {
            continue;
        }
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        if label.trim() == "1j" {
            if let Some(b) = n.bounding_box() {
                xs.push(b.x0 as f32);
            }
        }
    }
    assert_eq!(xs.len(), 3, "expected three jump numbers, got {xs:?}");
    let first = xs[0];
    assert!(
        xs.iter().all(|x| (x - first).abs() < 0.5),
        "the jump numbers no longer share a column: {xs:?}"
    );
}

/// The mark is a glyph, so the whole explanation lives in the tooltip, and both the number and
/// the glyph have to carry it: the number is the part a user reaches for.
#[test]
fn uitest_bridge_mark_explains_itself_on_hover() {
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    let probe = |at: egui::Pos2| {
        let mut scene = intel_bridge_states_scene("bridge_hover_probe", true, [1280.0, 800.0]);
        let mut harness = harness::build(&mut scene, false);
        assert!(harness.query_by_label_contains("by gate").is_none());
        harness.event(egui::Event::PointerMoved(at));
        harness.run_steps(3);
        harness.query_by_label_contains("2j by gate").is_some()
    };

    let mut scene = intel_bridge_states_scene("bridge_hover_probe", true, [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);
    // The shortcut draws no widget of its own, so the number is the only thing to reach for. Find
    // it by the row its system chip sits on.
    let mut row_y: Option<f32> = None;
    let mut spot: Option<egui::Pos2> = None;
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden() {
            continue;
        }
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        if n.role() == egui::accesskit::Role::Button && label.contains("7-K5EL") {
            row_y = n.bounding_box().map(|b| (b.y0 as f32 + b.y1 as f32) / 2.0);
        }
    }
    let row_y = row_y.expect("no 7-K5EL chip on screen");
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label || n.is_hidden() {
            continue;
        }
        let label = n.label().or_else(|| n.value()).unwrap_or_default();
        if label.trim() != "1j" {
            continue;
        }
        if let Some(b) = n.bounding_box() {
            let cy = (b.y0 as f32 + b.y1 as f32) / 2.0;
            if (cy - row_y).abs() < 6.0 {
                spot = Some(egui::pos2((b.x0 as f32 + b.x1 as f32) / 2.0, cy));
            }
        }
    }
    let spot = spot.expect("no jump number on the bridged card's row");
    drop(harness);
    assert!(probe(spot), "hovering the bridged number at {spot:?} showed no gate distance");
}

/// The flag answers "does the number on screen rest on a bridge", not "do you own a bridge". With
/// the setting off the number is already the gate answer, so nothing is marked however many
/// bridges would have shortened the trip.
#[test]
fn uitest_bridge_flag_reads_the_number_that_is_shown() {
    use crate::app::{jump_via, JumpVia};

    const SEVEN_K5EL: i64 = 30_003_704;
    const JITA: i64 = 30_000_142;
    let sys = Some(fixtures::systems_bridged_island());
    let via = |target: i64, use_bridges: bool, shown: u32| {
        jump_via(&sys, Some(PLAYER_SYS), Some(target), use_bridges, Some(shown))
    };

    assert_eq!(via(SEVEN_K5EL, false, 2), JumpVia::Gates, "a gate-only number was marked");
    assert_eq!(via(SEVEN_K5EL, true, 1), JumpVia::BridgeShorter(2), "the shortcut went unnoticed");
    assert_eq!(via(JITA, true, 1), JumpVia::BridgeOnly, "a gateless target read as a shortcut");
    assert_eq!(via(30_004_608, true, 1), JumpVia::Gates, "a plain gate hop was marked");
    assert_eq!(
        jump_via(&sys, Some(PLAYER_SYS), Some(JITA), true, None),
        JumpVia::Gates,
        "a card with no number still asked about bridges"
    );
}

/// The colour has to be legible as "informational" beside the row's own vocabulary, where green
/// means cleared and amber and red mean threat.
#[test]
fn uitest_a_bridged_range_is_marked_by_colour_not_a_glyph() {
    use crate::app::{jump_chip_style, jump_chip_tip, JumpVia};
    use crate::theme::standing;

    let (col, mark) = jump_chip_style(JumpVia::Gates);
    assert_eq!(col, standing::CORP);
    assert!(mark.is_none() && jump_chip_tip(JumpVia::Gates, 3).is_none());

    for via in [JumpVia::BridgeShorter(9), JumpVia::BridgeOnly] {
        let (col, mark) = jump_chip_style(via);
        assert_eq!(col, standing::ALLIANCE, "{via:?} is not purple");
        for (name, other) in
            [("CORP", standing::CORP), ("HOSTILE", standing::HOSTILE), ("WARNING", standing::WARNING)]
        {
            assert_ne!(col, other, "{via:?} reuses {name}");
        }
        assert!(
            !mark.as_deref().unwrap_or_default().contains(egui_phosphor::regular::ARROWS_LEFT_RIGHT),
            "{via:?} still draws the arrow glyph: {mark:?}"
        );
        let tip = jump_chip_tip(via, 2).unwrap_or_else(|| panic!("{via:?} has no tooltip"));
        assert!(tip.contains("hostile") && tip.contains("bridge"), "{tip:?}");
    }
    assert_eq!(
        jump_chip_style(JumpVia::BridgeShorter(9)).1,
        None,
        "a shortcut says nothing in words, the colour is the whole mark"
    );
    assert_eq!(
        jump_chip_style(JumpVia::BridgeOnly).1.as_deref(),
        Some("bridge only"),
        "colour alone cannot say gates do not reach at all, so this case keeps its words"
    );
}

/// Detection is a second BFS per card per frame, on a graph the size of k-space, so the whole
/// 250-card cap is what decides whether it needs caching. Two feeds: one whose reports sit within
/// a few jumps, which is what intel channels actually carry, and one spread over the whole map.
#[test]
#[ignore = "timing, not an assertion; run with --ignored --nocapture"]
fn uitest_bench_intel_bridge_detection() {
    use crate::geo::{SystemInfo, Systems};

    const N: i64 = 5200;
    const CARDS: usize = 250;
    let id = |k: i64| 30_000_000 + k.rem_euclid(N);
    let mut by_name = std::collections::HashMap::new();
    let mut adjacency = std::collections::HashMap::new();
    for i in 0..N {
        by_name.insert(
            format!("s{i}"),
            SystemInfo {
                id: id(i),
                name: format!("s{i}"),
                security: -0.5,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        );
        // A ring plus one long chord per system: four neighbours, the branching k-space has.
        adjacency.insert(id(i), vec![id(i - 1), id(i + 1), id(i + 37), id(i - 37)]);
    }
    let mut sys = Systems::new(by_name, adjacency);
    sys.add_bridges(&[(id(0), id(N / 2)), (id(0), id(120))]);
    let sys = Some(std::sync::Arc::new(sys));

    let feeds = [
        ("home", (0..CARDS as i64).map(|i| id((i % 40) - 20)).collect::<Vec<_>>()),
        ("map-wide", (0..CARDS as i64).map(|i| id(i * 17)).collect::<Vec<_>>()),
    ];
    for (name, targets) in feeds {
        // `moving` walks the player one system per pass, which is what invalidates the memo, so it
        // times the cold pass the feed pays when the player jumps. Standing still times a redraw.
        let time = |detect: bool, moving: bool| {
            let mut passes = 0i64;
            let mut sink = 0u64;
            if !moving {
                // One untimed pass, or the standing-still figure carries the cold pass it exists
                // to exclude.
                for &target in &targets {
                    let shown = crate::app::jumps_from_you(&sys, Some(id(0)), Some(target), true);
                    crate::app::jump_via(&sys, Some(id(0)), Some(target), true, shown);
                }
            }
            let t = std::time::Instant::now();
            while t.elapsed() < std::time::Duration::from_millis(500) {
                let me = Some(id(if moving { passes } else { 0 }));
                for &target in &targets {
                    let shown = crate::app::jumps_from_you(&sys, me, Some(target), true);
                    sink += shown.unwrap_or(0) as u64;
                    if detect {
                        let via = crate::app::jump_via(&sys, me, Some(target), true, shown);
                        sink += matches!(std::hint::black_box(via), crate::app::JumpVia::Gates)
                            as u64;
                    }
                }
                passes += 1;
            }
            assert!(sink > 0);
            t.elapsed().as_secs_f64() * 1000.0 / passes as f64
        };
        let me = Some(id(0));
        let flagged = targets
            .iter()
            .filter(|&&target| {
                let shown = crate::app::jumps_from_you(&sys, me, Some(target), true);
                crate::app::jump_via(&sys, me, Some(target), true, shown)
                    != crate::app::JumpVia::Gates
            })
            .count();
        let per = |detect, moving| (time(detect, moving) * 1000.0) / CARDS as f64;
        println!(
            "{name} feed, {CARDS} cards over {N} systems, {flagged} bridge-dependent: \
             distances alone {:.0} us/card, \
             detection adds {:.0} us/card cold and {:.0} us/card warm",
            per(false, false),
            per(true, true) - per(false, true),
            per(true, false) - per(false, false),
        );
    }
}

/// The setting is only reachable from the alert rules editor unless the intel toolbar carries its
/// own control. This asserts a labelled, ticked control rather than exact wording: an icon or a
/// menu entry would pass a looser check while losing the point.
#[test]
fn uitest_intel_toolbar_carries_the_bridge_toggle() {
    use egui_kittest::kittest::NodeT as _;

    let mut scene = view_scene("intel_toolbar_probe", View::Intel, [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);
    let found = harness.root().children_recursive().any(|node| {
        let n = node.accesskit_node();
        n.role() == egui::accesskit::Role::CheckBox
            && n.label().unwrap_or_default().contains("bridges")
    });
    assert!(found, "the intel toolbar has no jump-bridge toggle");
}

/// The toolbar is one wrapping row ending in the search field, so every pixel a control spends is
/// a pixel the field does not get.
///
/// The budget is three quarters of the window for the fixed controls, a quarter for the field. The
/// field then has to still sit on that row and still be able to show its own hint, since a
/// placeholder it cannot render says nothing about what it filters.
#[test]
fn uitest_intel_toolbar_leaves_room_for_the_search_field() {
    use egui_kittest::kittest::NodeT as _;

    let w = 1280.0;
    let mut scene = view_scene("intel_budget_probe", View::Intel, [w, 800.0]);
    let harness = harness::build(&mut scene, false);
    let mut nodes = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden() {
            continue;
        }
        let Some(b) = n.bounding_box() else { continue };
        nodes.push((
            n.role(),
            n.label().unwrap_or_default().to_string(),
            egui::Rect {
                min: egui::pos2(b.x0 as f32, b.y0 as f32),
                max: egui::pos2(b.x1 as f32, b.y1 as f32),
            },
        ));
    }
    let first = nodes
        .iter()
        .find(|(role, label, _)| *role == egui::accesskit::Role::Button && label == "All")
        .map(|(_, _, r)| *r)
        .expect("no type filter in the intel toolbar");
    let field = nodes
        .iter()
        .find(|(role, _, _)| *role == egui::accesskit::Role::TextInput)
        .map(|(_, _, r)| *r)
        .expect("no search field in the intel toolbar");
    let row = |r: &egui::Rect| (r.center().y - first.center().y).abs() < 8.0;
    // The field's own text run sits inside it, so it is excluded by containment rather than role.
    let controls = nodes
        .iter()
        .filter(|(_, _, r)| row(r) && !field.contains_rect(*r))
        .fold(f32::MIN, |acc, (_, _, r)| acc.max(r.right()));
    let used = controls - first.left();
    assert!(
        used <= w * 0.75,
        "the intel toolbar's controls take {used:.0}px of a {w:.0}px window, over the {:.0}px budget",
        w * 0.75
    );
    assert!(row(&field), "the search field was pushed off the toolbar's row: {field:?}");
    let font = egui::TextStyle::Body.resolve(&harness.ctx.global_style());
    let hint = harness
        .ctx
        .fonts_mut(|f| {
            f.layout_no_wrap(
                crate::app::SpaiApp::INTEL_FILTER_HINT.to_owned(),
                font,
                egui::Color32::WHITE,
            )
        })
        .size()
        .x;
    assert!(
        field.width() >= hint,
        "the search field is {:.0}px wide, too narrow for its own {hint:.0}px hint",
        field.width()
    );
}

/// Labels of every chip the card drew.
fn chip_labels(harness: &egui_kittest::Harness<'_>) -> Vec<String> {
    use egui_kittest::kittest::NodeT as _;

    let mut out = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Button || n.is_hidden() {
            continue;
        }
        out.push(n.label().unwrap_or_default().to_string());
    }
    out
}

/// The torture card carries the same moon twice, once from `near_celestial` and once from
/// `celestials`. One chip has to absorb the other, and it has to be the one with the distance.
#[test]
fn uitest_intel_row_folds_the_duplicate_celestial() {
    let mut scene =
        intel_scene_sized("celestial_merge_probe", fixtures::intel_torture(), [520.0, 1000.0]);
    let harness = harness::build(&mut scene, false);
    let cels: Vec<String> = chip_labels(&harness)
        .into_iter()
        .filter(|l| l.contains("Chemical Laboratory"))
        .collect();
    assert_eq!(cels.len(), 1, "the card drew the same moon on two chips: {cels:?}");
    assert!(cels[0].contains("0 km"), "the surviving chip lost the distance: {cels:?}");
}

/// The other direction, and the expensive one to get wrong: hostiles at a different moon than the
/// kill must keep their own chip.
#[test]
fn uitest_intel_row_keeps_a_second_celestial() {
    let mut scene =
        intel_scene("celestial_distinct_probe", fixtures::intel_two_celestials(), 520.0);
    let harness = harness::build(&mut scene, false);
    let labels = chip_labels(&harness);
    assert!(
        labels.iter().any(|l| l.contains("Moon 6-3") && l.contains("0 km")),
        "the near-celestial chip is gone: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.contains("Moon 6-4")),
        "a different moon was suppressed as a duplicate: {labels:?}"
    );
}

/// Clicking a pilot chip has to return that pilot, not the card's system or ship.
#[test]
fn uitest_intel_row_click_returns_pilot() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let clicks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = clicks.clone();
    let mut scene = Scene::ui("click_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        let hit = crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
        if let Some(hit) = hit {
            sink.borrow_mut().push(hit);
        }
    });
    let mut harness = harness::build(&mut scene, false);
    harness.get_by_label_contains("Hostile Pilot").click();
    harness.run_steps(2);
    let got = clicks.borrow();
    assert!(
        got.iter().any(|c| matches!(c, crate::app::IntelClick::Pilot(p) if p == "Hostile Pilot")),
        "clicking a pilot chip yielded {got:?}"
    );
}

/// Coordinate clicking, the fallback for the custom-painted widgets that emit no AccessKit node.
/// Driven here against a chip that does have one, so the coordinate path itself is what is tested.
#[test]
fn uitest_click_at_hits_the_system_chip() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs::default();
    let report = fixtures::intel_typical();
    let clicks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = clicks.clone();
    let mut scene = Scene::ui("click_at_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        let hit = crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
        if let Some(hit) = hit {
            sink.borrow_mut().push(hit);
        }
    });
    let mut harness = harness::build(&mut scene, false);
    let at = harness.get_by_label_contains("1DQ1-A").rect().center();
    harness::click_at(&harness, at);
    harness.run_steps(2);
    let got = clicks.borrow();
    assert!(
        got.iter().any(|c| matches!(c, crate::app::IntelClick::System(30_004_759))),
        "clicking the system chip at {at:?} yielded {got:?}"
    );
}

/// At the app's minimum window height the rail cannot show all ten rows, so the ones below the
/// fold have to come back on a scroll. Settings is pinned and is checked without scrolling.
#[test]
fn uitest_nav_rail_short_reaches_every_item() {
    use egui_kittest::kittest::Queryable as _;

    let selected = std::rc::Rc::new(std::cell::RefCell::new(View::Intel));
    let sink = selected.clone();
    let mut expanded = true;
    let mut scene = Scene::ui("nav_probe_short", [crate::nav::WIDTH_EXPANDED, 460.0], move |ui| {
        let got = crate::nav::rail(ui, *sink.borrow(), &mut expanded, &[], &[], crate::nav::View::primary());
        *sink.borrow_mut() = got;
    });
    let mut harness = harness::build(&mut scene, false);

    harness.get_by_label_contains("Settings").click();
    harness.run_steps(2);
    assert_eq!(*selected.borrow(), View::Settings, "Settings is pinned and must always be hittable");

    assert!(
        harness.query_by_label_contains("Jabber").is_none(),
        "Jabber sits below the fold at 460px, so it should not be laid out until scrolled to"
    );
    harness.event(egui::Event::PointerMoved(egui::pos2(crate::nav::WIDTH_EXPANDED / 2.0, 200.0)));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -200.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(3);
    harness.get_by_label_contains("Jabber").click();
    harness.run_steps(2);
    assert_eq!(*selected.borrow(), View::Jabber, "scrolling must bring the tail of the list back");
}

/// A spinner is a promise that something is happening. Headless starts no worker, which is also
/// what a user sees with no static data downloaded, so the view has to say why instead of spinning.
#[test]
fn uitest_battles_view_settles_without_a_worker() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = view_scene("battles_probe", View::Battles, [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);
    assert!(
        harness.query_by_label_contains("have not started").is_some(),
        "battles view should state why no report is coming"
    );
    assert!(
        harness.query_by_label_contains("Loading battles").is_none(),
        "battles view should not spin when no worker was ever started"
    );
}

/// Every rail entry must be reachable, and picking one must return it.
#[test]
fn uitest_nav_rail_click_selects() {
    use egui_kittest::kittest::Queryable as _;

    let selected = std::rc::Rc::new(std::cell::RefCell::new(View::Intel));
    let sink = selected.clone();
    let mut expanded = true;
    let mut scene = Scene::ui("nav_probe", [crate::nav::WIDTH_EXPANDED, 560.0], move |ui| {
        let got = crate::nav::rail(ui, *sink.borrow(), &mut expanded, &[], &[], crate::nav::View::primary());
        *sink.borrow_mut() = got;
    });
    let mut harness = harness::build(&mut scene, false);
    harness.get_by_label_contains("Battles").click();
    harness.run_steps(2);
    assert_eq!(*selected.borrow(), View::Battles);
}

/// Presses at `at`, drags right, and reports whether the window asked the OS to move it.
fn drags_the_alert_window(harness: &mut egui_kittest::Harness<'_>, at: egui::Pos2) -> bool {
    let mut started = false;
    let pump = |harness: &mut egui_kittest::Harness<'_>, started: &mut bool| {
        harness.run_steps(1);
        *started |= harness.output().viewport_output.values().any(|v| {
            v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::StartDrag))
        });
    };
    harness.event(egui::Event::PointerMoved(at));
    pump(harness, &mut started);
    harness.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    pump(harness, &mut started);
    for dx in [4.0, 12.0] {
        harness.event(egui::Event::PointerMoved(at + egui::vec2(dx, 0.0)));
        pump(harness, &mut started);
    }
    harness.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
    started
}

/// The alert window has no OS title bar, so the only way to move it is the drag rect behind its
/// title row. egui hands a drag to a small widget sitting on a bigger drag rect, so any label in
/// there that senses click or drag takes the grab and the window stops moving.
#[test]
fn uitest_alert_titlebar_has_no_competing_grab_target() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = alert_window_scene("titlebar_drag_probe", vec![fixtures::intel_typical()]);
    let mut harness = harness::build(&mut scene, false);
    let title = harness.get_by_label_contains("Intel alerts").rect();
    let secs = harness.get_by_label("5s").rect();
    let y = title.center().y;
    for (what, at) in [
        ("title text", title.center()),
        ("gap between the title and the counter", egui::pos2((title.max.x + secs.min.x) / 2.0, y)),
        ("seconds counter", secs.center()),
        ("empty stretch of the bar", egui::pos2(250.0, y)),
    ] {
        assert!(
            drags_the_alert_window(&mut harness, at),
            "dragging the {what} at {at:?} did not start a window drag"
        );
    }
}

/// Allocated rect of the first node whose label starts with `prefix`. Row heights, not ink, are
/// what the vertical rhythm is made of: in a wrapping horizontal layout egui pushes the row height
/// into the galley as `first_row_min_height`, so a label reports the row it sits in.
fn ping_label_rect(harness: &egui_kittest::Harness<'_>, prefix: &str) -> Option<egui::Rect> {
    ping_node_rect(harness, prefix, |_| true)
}

/// Same, but only nodes that answer a click, which is how a hyperlink is told apart from the text
/// beside it: egui's selectable-label pass leaves the node reporting `Role::Label`.
fn ping_click_rect(harness: &egui_kittest::Harness<'_>, prefix: &str) -> Option<egui::Rect> {
    ping_node_rect(harness, prefix, |n| n.data().supports_action(egui::accesskit::Action::Click))
}

fn ping_node_rect(
    harness: &egui_kittest::Harness<'_>,
    prefix: &str,
    keep: impl Fn(&egui_kittest::kittest::AccessKitNode<'_>) -> bool,
) -> Option<egui::Rect> {
    use egui_kittest::kittest::NodeT as _;

    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label {
            continue;
        }
        if !n.label().or_else(|| n.value()).unwrap_or_default().starts_with(prefix) {
            continue;
        }
        if !keep(&n) {
            continue;
        }
        let b = n.bounding_box()?;
        return Some(egui::Rect {
            min: egui::pos2(b.x0 as f32, b.y0 as f32),
            max: egui::pos2(b.x1 as f32, b.y1 as f32),
        });
    }
    None
}

/// FC, Formup and Doctrine are the same thing: one line of text, so they stand the same height. A
/// `horizontal_wrapped` row is floored at `interact_size.y` whether or not anything interactive is
/// in it, which would make one 11px taller than its neighbours.
#[test]
fn uitest_ping_metadata_rows_share_a_rhythm() {
    for url in ["", "https://example.invalid/doctrines"] {
        let mut scene = ping_scene_with_doctrine_url("rhythm_probe", fixtures::ping_fleet(), url);
        let harness = harness::build(&mut scene, false);
        let row = |p: &str| ping_label_rect(&harness, p).unwrap_or_else(|| panic!("no {p} row"));
        let fc = row("FC:");
        let formup = row("Formup:");
        let doctrine = row("Doctrine:");
        assert!(
            (fc.height() - formup.height()).abs() < 0.5,
            "FC {:.1} and Formup {:.1} already disagree",
            fc.height(),
            formup.height()
        );
        assert!(
            (doctrine.height() - fc.height()).abs() < 0.5,
            "doctrine_url {url:?}: Doctrine row is {:.1}px against {:.1}px for FC",
            doctrine.height(),
            fc.height()
        );
        // Comms is allowed its extra height because it hosts the Join Mumble button.
        let comms = row("Comms:");
        assert!(
            comms.height() > fc.height(),
            "Comms holds a button, so {:.1}px is too short for it",
            comms.height()
        );
    }
}

/// The doctrine URL adds a second link to the same row rather than a row of its own.
#[test]
fn uitest_ping_doctrine_link_shares_the_doctrine_row() {
    let mut scene = ping_scene_with_doctrine_url(
        "doctrine_link_probe",
        fixtures::ping_fleet(),
        "https://example.invalid/doctrines",
    );
    let harness = harness::build(&mut scene, false);
    let doctrine = ping_label_rect(&harness, "Doctrine:").expect("no Doctrine row");
    let chip = ping_label_rect(&harness, "Doctrines").expect("no doctrine link chip");
    assert_eq!(chip.top(), doctrine.top(), "the chip started its own row: {chip:?}");
    assert_eq!(chip.height(), doctrine.height(), "the chip is taller than its row: {chip:?}");
    assert!(chip.left() > doctrine.right(), "the chip sits on the Doctrine text: {chip:?}");
}

/// A fleet ping with no doctrine and no configured URL has nothing to put on the row, so it must
/// not leave an empty one behind.
#[test]
fn uitest_ping_without_a_doctrine_leaves_no_row() {
    let bottom = |ping: crate::pings::Ping| {
        let mut scene = ping_scene("doctrine_gap_probe", ping);
        let harness = harness::build(&mut scene, false);
        ping_label_rect(&harness, "goonfleet").expect("no ping footer").bottom()
    };
    let with = bottom(fixtures::ping_fleet());
    let without = bottom(fixtures::ping_fleet_no_doctrine());

    let mut scene = ping_scene("doctrine_row_probe", fixtures::ping_fleet());
    let harness = harness::build(&mut scene, false);
    let doctrine = ping_label_rect(&harness, "Doctrine:").expect("no Doctrine row");
    let fc = ping_label_rect(&harness, "FC:").expect("no FC row");
    let formup = ping_label_rect(&harness, "Formup:").expect("no Formup row");
    let want = doctrine.height() + (formup.top() - fc.bottom());
    assert!(
        (with - without - want).abs() < 1.0,
        "dropping the doctrine saved {:.1}px, not the {want:.1}px its row plus spacing occupies",
        with - without
    );
}

/// The prefixes of `fixtures::ping_plain_multiline`'s three body lines, in order.
const BODY_LINES: [&str; 3] =
    ["Sov timer in 68FT-6", "Fits and doctrine:", "Bring a mobile depot"];

/// One line of body text, resolved the way the app resolves it, so a row height can be checked
/// against the ink it holds rather than against a number written down here.
fn theme_body_line() -> f32 {
    let ctx = egui::Context::default();
    harness::prepare(&ctx);
    let mut h = 0.0;
    // Twice: `install_fonts_opts` only stashes the definitions, so the first pass still measures
    // egui's defaults (see `harness::build`).
    for _ in 0..2 {
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            h = ui.text_style_height(&egui::TextStyle::Body);
        });
    }
    h
}

fn theme_spacing() -> egui::style::Spacing {
    let ctx = egui::Context::default();
    harness::prepare(&ctx);
    ctx.global_style().spacing.clone()
}

/// A body line holds one line of text, so it must stand one line tall. A `horizontal_wrapped` row
/// is floored at `interact_size.y` whether or not anything interactive is on it, which allocates
/// 26px for 15px of ink.
#[test]
fn uitest_ping_body_lines_are_one_line_tall() {
    let spacing = theme_spacing();
    let mut scene = ping_scene("body_leading_probe", fixtures::ping_plain_multiline());
    let harness = harness::build(&mut scene, false);
    let rects: Vec<egui::Rect> = BODY_LINES
        .iter()
        .map(|p| ping_label_rect(&harness, p).unwrap_or_else(|| panic!("no body line {p:?}")))
        .collect();
    for (p, r) in BODY_LINES.iter().zip(&rects) {
        assert!(
            r.height() < spacing.interact_size.y - 1.0,
            "body line {p:?} is {:.1}px tall, still floored at interact_size {:.1}",
            r.height(),
            spacing.interact_size.y
        );
    }
    assert!(
        (rects[0].height() - rects[2].height()).abs() < 0.5,
        "body lines disagree: {:.1}px against {:.1}px",
        rects[0].height(),
        rects[2].height()
    );
    for (w, p) in rects.windows(2).zip(BODY_LINES.iter().skip(1)) {
        let gap = w[1].top() - w[0].bottom();
        assert!(
            (gap - spacing.item_spacing.y).abs() < 1.0,
            "{p:?} sits {gap:.1}px below the line above, not the usual {:.1}px",
            spacing.item_spacing.y
        );
    }
}

/// A blank line is the author's paragraph break, and a tight row allocates nothing for it.
#[test]
fn uitest_ping_body_keeps_a_blank_line_as_a_break() {
    let measure = |text: &str| {
        let mut ping = fixtures::ping_plain();
        if let crate::pings::Ping::Plain { text: t, .. } = &mut ping {
            *t = text.to_owned();
        }
        let mut scene = ping_scene("blank_line_probe", ping);
        let harness = harness::build(&mut scene, false);
        let over = ping_label_rect(&harness, "over").expect("no first line");
        let under = ping_label_rect(&harness, "under").expect("no second line");
        (under.top() - over.bottom(), over.height())
    };
    let (tight, line) = measure("over\nunder");
    let (broken, _) = measure("over\n\nunder");
    assert!(
        (broken - tight - line).abs() < 2.0,
        "a blank line opened {:.1}px, not the {line:.1}px of the line it stands for",
        broken - tight
    );
}

/// A link is the one genuinely interactive thing a body line can hold, and the reason the row
/// height was floored at all. It has to stay on its own line, beside the text it follows.
#[test]
fn uitest_ping_body_link_stays_on_its_line() {
    let mut scene = ping_scene("body_link_probe", fixtures::ping_plain_multiline());
    let harness = harness::build(&mut scene, false);
    let text = ping_label_rect(&harness, BODY_LINES[1]).expect("no link line");
    let link = ping_click_rect(&harness, "https://example.invalid").expect("no body link");
    assert_eq!(link.top(), text.top(), "the link started its own row: {link:?}");
    assert_eq!(link.height(), text.height(), "the link is taller than its line: {link:?}");
    assert!(link.left() > text.right(), "the link sits on the text: {link:?}");
    let after = ping_label_rect(&harness, BODY_LINES[2]).expect("no line after the link");
    assert!(link.bottom() <= after.top(), "the link overruns the next line: {link:?}");
}

/// The Copy button keeps the `interact_size` floor: a `small_button` would be a 17px target next to
/// the 27px Join Mumble in the same card.
#[test]
fn uitest_ping_copy_matches_the_other_buttons() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = ping_scene("copy_size_probe", fixtures::ping_fleet());
    let harness = harness::build(&mut scene, false);
    let copy = harness.get_by_label_contains("Copy").rect();
    let mumble = harness.get_by_label_contains("Join Mumble").rect();
    assert!(
        (copy.height() - mumble.height()).abs() < 1.0,
        "Copy is {:.1}px tall against {:.1}px for Join Mumble",
        copy.height(),
        mumble.height()
    );
    let ago = ping_label_rect(&harness, "2m ago").expect("no timestamp");
    assert!(
        (copy.center().y - ago.center().y).abs() < 1.0,
        "Copy at {:.1} and the timestamp at {:.1} no longer share the header row",
        copy.center().y,
        ago.center().y
    );
}

/// The uncertain set is built from display-cased pilot names here. `UncertainPilots` normalizes on
/// the way in, so the marker and the verdict click both have to survive it.
#[test]
fn uitest_intel_row_marks_uncertain_pilot_from_display_cased_set() {
    use egui_kittest::kittest::Queryable as _;

    let args = IntelArgs {
        uncertain: ["Second Target"].into_iter().collect(),
        ..IntelArgs::default()
    };
    let report = fixtures::intel_typical();
    let clicks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = clicks.clone();
    let mut scene = Scene::ui("uncertain_case_probe", [520.0, 520.0], move |ui| {
        let mut t = None;
        let hit = crate::app::intel_row(
            ui,
            &report,
            fixtures::now(),
            false,
            None,
            crate::app::JumpVia::Gates,
            &args.chars,
            &args.systems,
            &args.status,
            &args.ship_details,
            &args.ship_roles,
            &args.resolved_pilots,
            &args.uncertain,
            &args.last_ship,
            &args.kills,
            crate::settings::Severity::Danger,
            true,
            &args.affil,
            &args.notes,
            false,
            &mut t,
        );
        if let Some(hit) = hit {
            sink.borrow_mut().push(hit);
        }
    });
    let mut harness = harness::build(&mut scene, false);
    let labels = chip_labels(&harness);
    assert!(
        labels.iter().any(|l| l.contains("Second Target") && l.contains('?')),
        "no uncertain marker on the flagged pilot: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.contains("Hostile Pilot") && !l.contains('?')),
        "an unflagged pilot picked up the marker: {labels:?}"
    );
    harness.get_by_label_contains("Second Target").click();
    harness.run_steps(2);
    let got = clicks.borrow();
    assert!(
        got.iter().any(
            |c| matches!(c, crate::app::IntelClick::PilotVerdict(p) if p == "Second Target")
        ),
        "clicking the uncertain pilot did not open the verdict: {got:?}"
    );
}

/// A frame with `configured` false renders the login form instead of a chat, and a pop-out whose
/// tab list never landed renders "No conversations in this window". Both leave a scene that looks
/// like coverage and inspects nothing, so the pop-out states what it must actually contain.
#[test]
fn uitest_jabber_popout_renders_a_conversation() {
    use egui_kittest::kittest::NodeT as _;

    let mut scene = all().into_iter().find(|s| s.name == "jabber_popout").expect("scene");
    let harness = harness::build(&mut scene, false);
    let mut labels = Vec::new();
    let mut composer = false;
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        composer |= n.role() == egui::accesskit::Role::MultilineTextInput;
        if n.role() == egui::accesskit::Role::Label {
            labels.push(n.label().or_else(|| n.value()).unwrap_or_default().to_string());
        }
    }
    assert!(composer, "no composer in the pop-out: {labels:?}");
    for needle in ["delve.imperium", "in fleet, in position", "— new —"] {
        assert!(
            labels.iter().any(|l| l.contains(needle)),
            "the pop-out drew no {needle:?}: {labels:?}"
        );
    }
}

/// Each conversation in a window needs its own `ScrollArea` id. With a shared one, a short DM's
/// tab-switch press clears the stick-to-bottom flag (`selecting` drops `stick_to_bottom` for that
/// frame, and egui only re-enters sticky mode from a short body while sticking is asked for), so
/// the next, longer conversation opens at offset 0, the top of a 1000-message history.
#[test]
fn uitest_jabber_tab_switch_opens_at_the_newest_message() {
    use crate::app::ChatWinKey;
    use egui_kittest::kittest::NodeT as _;

    let mut scene = jabber_popout_seeded(
        "jabber_tab_switch_scroll",
        [520.0, 480.0],
        fixtures::JABBER_DM,
        "",
        None,
        fixtures::jabber_state_long,
    );
    let mut harness = harness::build(&mut scene, false);
    let tab = harness
        .ctx
        .read_response(egui::Id::new((
            "jtab",
            ChatWinKey::Popout(POPOUT_ID),
            fixtures::JABBER_ROOM,
        )))
        .expect("the room tab is not in the pop-out's bar")
        .rect;
    harness::click_at(&harness, tab.center());
    harness.run_steps(4);

    let bodies: Vec<String> = harness
        .root()
        .children_recursive()
        .map(|node| node.accesskit_node())
        .filter(|n| n.role() == egui::accesskit::Role::Label)
        .map(|n| n.label().or_else(|| n.value()).unwrap_or_default().to_string())
        .collect();
    let newest = format!("#{}", fixtures::JABBER_LONG_LEN - 1);
    assert!(
        bodies.iter().any(|b| b.contains(&newest)),
        "switching to the long room did not open on its newest message ({newest}); drew: {:?}",
        bodies.iter().filter(|b| b.contains('#')).take(6).collect::<Vec<_>>()
    );
    assert!(
        !bodies.iter().any(|b| b.contains("#0 ") || b.ends_with("#0")),
        "switching to the long room opened at the top of its history: {:?}",
        bodies.iter().filter(|b| b.contains('#')).take(6).collect::<Vec<_>>()
    );
}

/// The other half: per-conversation state has to mean the scrollback is kept, not that
/// every arrival is forced to the bottom. Scroll the room up, leave, come back, land where you
/// left. A fix that simply snapped to the newest message on every switch would pass
/// [`uitest_jabber_tab_switch_opens_at_the_newest_message`] and fail this.
#[test]
fn uitest_jabber_tab_switch_keeps_a_scrolled_back_conversation() {
    use crate::app::ChatWinKey;

    let mut scene = jabber_popout_seeded(
        "jabber_tab_switch_scrollback",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        "",
        None,
        fixtures::jabber_state_long,
    );
    let mut harness = harness::build(&mut scene, false);
    let tab = |harness: &egui_kittest::Harness<'_>, jid: &str| {
        harness
            .ctx
            .read_response(egui::Id::new(("jtab", ChatWinKey::Popout(POPOUT_ID), jid)))
            .unwrap_or_else(|| panic!("{jid} is not a tab in the pop-out's bar"))
            .rect
    };
    // Over the history, below the tab bar and the room header, above the composer.
    let body = egui::pos2(260.0, 240.0);
    harness.event(egui::Event::PointerMoved(body));
    for _ in 0..6 {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 400.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        harness.run_steps(2);
    }
    // egui animates a wheel scroll, so the offset is still moving for a few frames after the last
    // event. Reading the landing position early makes the comparison below drift by a message.
    harness.run_steps(16);
    let scrolled_back = jabber_top_body(&harness).expect("no message bodies in the room");
    assert!(
        !scrolled_back.contains(&format!("#{}", fixtures::JABBER_LONG_LEN - 1)),
        "the wheel did not move the history, so this asserts nothing: {scrolled_back:?}"
    );

    let dm = tab(&harness, fixtures::JABBER_DM);
    harness::click_at(&harness, dm.center());
    harness.run_steps(4);
    let room = tab(&harness, fixtures::JABBER_ROOM);
    harness::click_at(&harness, room.center());
    harness.run_steps(4);

    assert_eq!(
        jabber_top_body(&harness).as_deref(),
        Some(scrolled_back.as_str()),
        "coming back to the room did not restore where it was scrolled to"
    );
}

/// The first message body in the AccessKit tree, which is the top of the history as drawn. Nicks
/// and timestamps are labels too, so bodies are picked out by the marker every fixture body
/// carries.
fn jabber_top_body(harness: &egui_kittest::Harness<'_>) -> Option<String> {
    use egui_kittest::kittest::NodeT as _;

    harness
        .root()
        .children_recursive()
        .map(|node| node.accesskit_node())
        .filter(|n| n.role() == egui::accesskit::Role::Label)
        .map(|n| n.label().or_else(|| n.value()).unwrap_or_default().to_string())
        .find(|l| l.contains('#'))
}

/// A chat body stands one line tall. A `horizontal_wrapped` row is floored at `interact_size.y`
/// whether or not anything interactive is on it, which allocates 26px for 15px of ink.
#[test]
fn uitest_jabber_message_body_is_one_line_tall() {
    let (line, spacing) = (theme_body_line(), theme_spacing());
    let mut scene = all().into_iter().find(|s| s.name == "jabber_popout").expect("scene");
    let harness = harness::build(&mut scene, false);
    let body = ping_label_rect(&harness, "in fleet, in position").expect("no one-line body");
    // That nick appears on more than one message, so pick the one on this body's row.
    let top = body.top();
    let nick = ping_node_rect(&harness, "Wingmate Alpha:", |n| {
        n.bounding_box().is_some_and(|b| (b.y0 as f32 - top).abs() < 0.5)
    })
    .expect("no nick beside it");
    assert!(
        body.height() < spacing.interact_size.y - 1.0,
        "a one-line body is {:.1}px tall, still floored at interact_size {:.1}",
        body.height(),
        spacing.interact_size.y
    );
    assert!(
        (body.height() - line).abs() < 0.5,
        "a one-line body stands {:.1}px against {line:.1}px of text",
        body.height()
    );
    assert!(
        (nick.height() - line).abs() < 0.5,
        "the nick stands {:.1}px against {line:.1}px of text",
        nick.height()
    );
}

/// Every row of a wrapped body has to be the same height, so a two-row message measures 30px, not
/// 41px with a floored first row.
#[test]
fn uitest_jabber_wrapped_body_rows_are_all_one_line() {
    let line = theme_body_line();
    for name in ["jabber_popout", "jabber_popout_min"] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let harness = harness::build(&mut scene, false);
        let body = ping_label_rect(&harness, "hostiles moved off gate")
            .unwrap_or_else(|| panic!("no wrapped body in {name}"));
        assert!(
            body.height() > line + 0.5,
            "{name}: the body did not wrap, so this asserts nothing"
        );
        let rows = (body.height() / line).round();
        assert!(
            (body.height() - rows * line).abs() < 0.5,
            "{name}: a wrapped body is {:.1}px tall, not a whole number of {line:.1}px rows, so \
             its first row is still floored",
            body.height()
        );
    }
}

/// A message with an empty body has nothing in its row to hold it open, and a tight row is then
/// literally 0.0px, which folds the message into the one above it.
#[test]
fn uitest_jabber_blank_body_keeps_its_row() {
    let line = theme_body_line();
    let mut scene = jabber_popout_seeded(
        "blank_body_probe",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        "",
        None,
        fixtures::jabber_state_blank_body,
    );
    let harness = harness::build(&mut scene, false);
    let before = ping_label_rect(&harness, "cyno up on the Keepstar").expect("no message before");
    let after =
        ping_label_rect(&harness, "and the second one just lit").expect("no message after");
    let gap = after.top() - before.bottom();
    assert!(
        gap > line,
        "the empty message opened {gap:.1}px between its neighbours, less than the {line:.1}px of \
         the line it stands for"
    );
}

/// The rescue window's chat lines stand one line tall too. That window has no scene, so this drives
/// the feed directly rather than through the window around it.
#[cfg(feature = "fleet")]
#[test]
fn uitest_rescue_chat_lines_are_one_line_tall() {
    let (line, spacing) = (theme_body_line(), theme_spacing());
    let now = chrono::Utc::now().timestamp();
    let msgs = vec![
        ("Rescue Actual".to_owned(), "form up now".to_owned(), false, now - 60),
        (
            "Rescue Actual".to_owned(),
            "hostiles moved off the gate and are burning back to the keepstar right now"
                .to_owned(),
            false,
            now - 30,
        ),
    ];
    let mut scene = Scene::ui("rescue_chat_probe", [360.0, 240.0], move |ui| {
        assert!(crate::app::rescue_chat_feed(ui, &msgs, "probe").is_none());
    });
    let harness = harness::build(&mut scene, false);
    let nick = ping_label_rect(&harness, "Rescue Actual:").expect("no nick");
    let body = ping_label_rect(&harness, "form up now").expect("no one-line body");
    let wrapped = ping_label_rect(&harness, "hostiles moved off the gate").expect("no long body");
    assert!(
        body.height() < spacing.interact_size.y - 1.0,
        "a rescue chat body is {:.1}px tall, still floored at interact_size {:.1}",
        body.height(),
        spacing.interact_size.y
    );
    for (what, r) in [("body", body), ("nick", nick)] {
        assert!(
            (r.height() - line).abs() < 0.5,
            "the rescue chat {what} stands {:.1}px against {line:.1}px of text",
            r.height()
        );
    }
    assert!(wrapped.height() > line + 0.5, "the long body did not wrap, so this asserts nothing");
    let rows = (wrapped.height() / line).round();
    assert!(
        (wrapped.height() - rows * line).abs() < 0.5,
        "a wrapped rescue body is {:.1}px tall, not a whole number of {line:.1}px rows, so its \
         first row is still floored",
        wrapped.height()
    );
}

/// The composer's text band, which is the height the galley *wants*. It lives inside a scroll
/// area, so past the ten-row cap this rect keeps growing while the visible band stops. The frame
/// margin is outside the field, so this is the text alone.
fn composer_rect(harness: &egui_kittest::Harness<'_>) -> egui::Rect {
    use egui_kittest::kittest::NodeT as _;
    harness
        .root()
        .children_recursive()
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::MultilineTextInput)
        .map(|n| {
            let b = n.accesskit_node().bounding_box().expect("composer bounds");
            egui::Rect {
                min: egui::pos2(b.x0 as f32, b.y0 as f32),
                max: egui::pos2(b.x1 as f32, b.y1 as f32),
            }
        })
        .expect("no composer in the pop-out")
}

struct Composer {
    /// The scrolling text band, from the AccessKit tree.
    field: egui::Rect,
    /// The painted border box, and the clip rect it was painted under.
    border: egui::Rect,
    clip: egui::Rect,
    stroke: egui::Stroke,
    /// What a focused `TextEdit` outlines itself with, read off the scene's own theme.
    selection: egui::Stroke,
    row_h: f32,
}

/// The composer's border emits no AccessKit node, so the frame's shape list is the only handle on
/// the box the user actually sees, and on the clip rect that decides whether it survives whole.
fn composer_border(harness: &egui_kittest::Harness<'_>) -> (egui::Rect, egui::Rect, egui::Stroke) {
    let field = composer_rect(harness);
    let probe = egui::pos2(field.center().x, field.top() + 1.0);
    let mut found = Vec::new();
    for clipped in &harness.output().shapes {
        if let egui::Shape::Rect(r) = &clipped.shape {
            let spans = r.rect.width() >= field.width() - 0.5;
            if r.stroke.width > 0.0 && spans && r.rect.contains(probe) {
                found.push((r.rect, clipped.clip_rect, r.stroke));
            }
        }
    }
    assert_eq!(found.len(), 1, "expected one composer border, got {found:?}");
    found[0]
}

fn composer_metrics(name: &'static str) -> Composer {
    composer_metrics_maybe_focused(name, false)
}

fn composer_metrics_maybe_focused(name: &'static str, focus: bool) -> Composer {
    let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
    let mut harness = harness::build(&mut scene, false);
    if focus {
        use egui_kittest::kittest::Queryable as _;
        harness.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
        harness.run_steps(2);
    }
    let font = egui::TextStyle::Body.resolve(&harness.ctx.global_style());
    let row_h = harness.ctx.fonts_mut(|f| f.row_height(&font));
    let (border, clip, stroke) = composer_border(&harness);
    let selection = harness.ctx.global_style().visuals.selection.stroke;
    Composer { field: composer_rect(&harness), border, clip, stroke, selection, row_h }
}

/// The composer grows with the draft to ten rows and then scrolls. The scenes cover empty, three
/// newlines, one long line that only wraps, and a draft past the cap, so both the growth and the
/// boundary are pinned.
#[test]
fn uitest_jabber_composer_grows_then_caps() {
    let empty = composer_metrics("jabber_popout");
    let three = composer_metrics("jabber_popout_drafting");
    let wrapped = composer_metrics("jabber_popout_wrapped");
    let over = composer_metrics("jabber_popout_overflow");
    let row_h = empty.row_h;
    let rows = |c: &Composer| c.field.height() / row_h;

    assert!((rows(&empty) - 2.0).abs() < 0.1, "empty composer is {} rows", rows(&empty));
    assert!((rows(&three) - 3.0).abs() < 0.1, "3-line draft is {} rows", rows(&three));
    assert!(
        rows(&wrapped) > 3.5,
        "one wrapped line counted as {} rows, so wrapping is not being measured",
        rows(&wrapped)
    );
    assert!(rows(&over) > 10.0, "the overflow draft should want more than ten rows");

    // Every pop-out scene is the same width and pins the composer to the same bottom edge, so the
    // band the window actually shows is that bottom minus the field's top.
    let shown = empty.field.bottom() - over.field.top();
    assert!(
        (shown - 10.0 * row_h).abs() < 1.0,
        "past the cap the window shows {shown}px of text, not ten rows ({}px)",
        10.0 * row_h
    );
    // The box the user sees: the text band plus the frame margin, and the cap holds it there.
    for (c, want_rows) in [(&empty, 2.0), (&three, 3.0), (&wrapped, 5.0), (&over, 10.0)] {
        assert!(
            (c.border.height() - (want_rows * row_h + 4.0)).abs() < 0.5,
            "the visible box is {}px, not {want_rows} rows ({}px)",
            c.border.height(),
            want_rows * row_h + 4.0
        );
    }
}

/// A ten-row composer is taller than the whole body of a 360x260 pop-out, so the cap has to yield
/// to the window before it pushes the history off the bottom edge.
#[test]
fn uitest_jabber_composer_yields_to_a_small_window() {
    let empty = composer_metrics("jabber_popout_min");
    let over = composer_metrics("jabber_popout_min_overflow");
    let row_h = empty.row_h;
    let shown = over.border.height();
    assert!(
        shown >= 2.0 * row_h + 4.0 && shown < 10.0 * row_h + 4.0,
        "small-window composer shows {shown}px, outside the 2..10 row range"
    );
    assert!(
        over.field.height() > shown,
        "the small-window draft is not overflowing its {shown}px box"
    );
    assert!(
        empty.border.height() < shown,
        "an empty small-window composer is {}px, no smaller than a full one",
        empty.border.height()
    );
}

/// Enter is the only way to send, so both halves of the return-key split
/// have to be driven, not reasoned about.
#[test]
fn uitest_jabber_composer_enter_sends_shift_enter_wraps() {
    use egui_kittest::kittest::Queryable as _;
    let probe: DraftProbe = Default::default();
    let mut scene = jabber_popout_probed(
        "composer_keys",
        [520.0, 480.0],
        fixtures::JABBER_ROOM,
        "reshipping",
        Some(probe.clone()),
    );
    let mut harness = harness::build(&mut scene, false);
    let draft = || probe.lock().unwrap().clone();

    assert!(
        harness.query_by_label("Send").is_none(),
        "the composer still draws a Send button"
    );

    harness.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
    harness.run_steps(2);

    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Enter);
    harness.run_steps(2);
    let after_shift = draft();
    assert!(
        after_shift.contains('\n') && after_shift.contains("reshipping"),
        "Shift+Enter did not add a line: {after_shift:?}"
    );

    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert_eq!(draft(), "", "Enter did not send and clear the draft");
}

/// Where a string is actually painted this pass. Tabs and the drag ghost emit no AccessKit node,
/// so the shape list is the only place they exist.
fn painted_text_rects(harness: &egui_kittest::Harness<'_>, text: &str) -> Vec<egui::Rect> {
    fn walk(shape: &egui::Shape, text: &str, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                out.push(egui::Rect::from_min_size(t.pos, t.galley.size()));
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk(s, text, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &harness.output().shapes {
        walk(&c.shape, text, &mut out);
    }
    out
}

/// A tab drag has to show at the pointer, or the user cannot tell one has begun. The gesture
/// itself is not driven here (the tab is a painter-only `interact` rect kittest cannot grab), so
/// the drag state is seeded and the painted output is what gets checked, against
/// the same window under the same pointer with no drag running.
#[test]
fn uitest_jabber_tab_drag_paints_a_ghost_at_the_pointer() {
    use egui_kittest::kittest::NodeT as _;

    const LABEL: &str = "delve.imperium";
    let pointer = egui::pos2(200.0, 150.0);
    let node_rects = |h: &egui_kittest::Harness<'_>| -> Vec<egui::Rect> {
        h.root()
            .children_recursive()
            .filter_map(|n| n.accesskit_node().bounding_box())
            .map(|b| egui::Rect {
                min: egui::pos2(b.x0 as f32, b.y0 as f32),
                max: egui::pos2(b.x1 as f32, b.y1 as f32),
            })
            .collect()
    };

    let mut idle = jabber_popout_scene("tab_drag_control", [520.0, 480.0], fixtures::JABBER_ROOM, "")
        .hovered_at(pointer);
    let idle = harness::build(&mut idle, false);
    assert_eq!(
        painted_text_rects(&idle, LABEL).len(),
        1,
        "with no drag running, only the tab itself names the conversation"
    );

    let mut scene = all().into_iter().find(|s| s.name == "jabber_popout_tab_drag").expect("scene");
    let harness = harness::build(&mut scene, false);
    let painted = painted_text_rects(&harness, LABEL);
    assert_eq!(
        painted.len(),
        2,
        "mid-drag the label should be painted twice, on the tab and on the ghost: {painted:?}"
    );
    let ghost = painted
        .into_iter()
        .max_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .expect("ghost");
    assert!(
        ghost.distance_to_pos(pointer) < 40.0,
        "the ghost is at {ghost:?}, nowhere near the pointer at {pointer:?}"
    );

    // An interactive ghost would sit on the history as a click target. It is painted into a layer,
    // so it owns no node at all.
    assert_eq!(
        node_rects(&harness),
        node_rects(&idle),
        "the drag ghost changed the AccessKit tree"
    );
}

/// The other half: the ghost has to appear on a real press-and-move and be gone the moment the
/// button comes up. The press goes in by coordinate, off the tab's own painted label, because the
/// tab is an `interact` rect with no AccessKit node to click.
#[test]
fn uitest_jabber_tab_drag_ghost_comes_and_goes_with_the_gesture() {
    const LABEL: &str = "delve.imperium";
    let mut scene =
        jabber_popout_scene("tab_drag_gesture", [520.0, 480.0], fixtures::JABBER_ROOM, "");
    let mut harness = harness::build(&mut scene, false);
    let from = painted_text_rects(&harness, LABEL)[0].center();
    let to = egui::pos2(200.0, 150.0);
    let btn = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };

    harness.event(egui::Event::PointerMoved(from));
    harness.run_steps(2);
    harness.event(btn(from, true));
    harness.run_steps(2);
    // Two moves: the first crosses egui's drag threshold, the second carries the tab off the bar.
    for p in [from + egui::vec2(8.0, 4.0), to] {
        harness.event(egui::Event::PointerMoved(p));
        harness.run_steps(2);
    }
    let painted = painted_text_rects(&harness, LABEL);
    assert_eq!(painted.len(), 2, "the drag painted no ghost: {painted:?}");
    let ghost = painted.into_iter().max_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("ghost");
    assert!(
        ghost.distance_to_pos(to) < 40.0,
        "the ghost is at {ghost:?}, nowhere near the pointer at {to:?}"
    );

    harness.event(btn(to, false));
    harness.run_steps(2);
    assert_eq!(
        painted_text_rects(&harness, LABEL).len(),
        1,
        "the ghost outlived the drag"
    );
}

/// Past the ten-row cap only the text scrolls. The border is painted outside the scrolling region,
/// so its clip rect has to hold all of it.
#[test]
fn uitest_jabber_composer_border_does_not_scroll() {
    for name in ["jabber_popout_overflow", "jabber_popout_min_overflow"] {
        let c = composer_metrics(name);
        assert!(
            c.clip.contains_rect(c.border),
            "{name} clips the composer border: {:?} painted under {:?}",
            c.border,
            c.clip
        );
        assert!(
            c.field.height() > c.border.height(),
            "{name} is not overflowing: a {}px field in a {}px box",
            c.field.height(),
            c.border.height()
        );
    }
}

/// The border carries the focus ring, since the field itself is drawn frameless. A focused
/// composer that outlines itself like an idle one reads as the odd widget out.
#[test]
fn uitest_jabber_composer_focused_border_is_the_focus_ring() {
    for name in ["jabber_popout", "jabber_popout_overflow"] {
        let idle = composer_metrics(name);
        let focused = composer_metrics_maybe_focused(name, true);
        assert_eq!(
            focused.stroke.color, focused.selection.color,
            "{name} focused draws a {:?} border, not the selection stroke",
            focused.stroke
        );
        assert_ne!(
            idle.stroke.color, focused.selection.color,
            "{name} idle already draws the focus ring, so the test proves nothing"
        );
        assert!(
            (focused.border.height() - idle.border.height()).abs() < 0.5,
            "{name} changes height on focus: {} to {}",
            idle.border.height(),
            focused.border.height()
        );
        assert!(
            focused.clip.contains_rect(focused.border),
            "{name} clips the focused border: {:?} under {:?}",
            focused.border,
            focused.clip
        );
    }
}

/// The focus ring is painted, not exposed, so the overflowing focused composer needs a shot of its
/// own. `all()` has no way to focus a widget, hence a scene built here.
#[test]
#[ignore = "renders to target/uishots; run with --ignored"]
fn uitest_screenshots_composer_focused() {
    use egui_kittest::kittest::Queryable as _;
    for (name, size) in
        [("jabber_popout_overflow", [520.0, 480.0]), ("jabber_popout_min_overflow", [360.0, 260.0])]
    {
        let mut scene = jabber_popout_scene(name, size, fixtures::JABBER_ROOM, DRAFT_OVERFLOW);
        let mut harness = harness::build(&mut scene, true);
        harness.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
        harness.run_steps(2);
        harness::shot(&mut harness, &format!("{name}_focused"));
    }
}

/// A tab switch needs both tabs in one gesture, and `all()` scenes cannot click, so the switch is
/// rendered here: the pop-out opens on the short DM, the room tab is clicked, and the shot is what
/// the long conversation looks like on arrival.
#[test]
#[ignore = "renders to target/uishots; run with --ignored"]
fn uitest_screenshots_tab_switch() {
    use crate::app::ChatWinKey;

    let mut scene = jabber_popout_seeded(
        "jabber_tab_switch",
        [520.0, 480.0],
        fixtures::JABBER_DM,
        "",
        None,
        fixtures::jabber_state_long,
    );
    let mut harness = harness::build(&mut scene, true);
    harness::shot(&mut harness, "jabber_tab_switch_from_dm");
    let tab = harness
        .ctx
        .read_response(egui::Id::new((
            "jtab",
            ChatWinKey::Popout(POPOUT_ID),
            fixtures::JABBER_ROOM,
        )))
        .expect("the room tab is not in the pop-out's bar")
        .rect;
    harness::click_at(&harness, tab.center());
    harness.run_steps(4);
    harness::shot(&mut harness, "jabber_tab_switch_to_room");
}

/// The four attribution layouts side by side, since the same report reads differently depending on
/// who is nearest and how wide the card is.
#[test]
#[ignore = "renders to target/uishots; run with --ignored"]
fn uitest_screenshots_char_attribution() {
    for name in [
        "intel_row_two_characters",
        "intel_row_two_characters_narrow",
        "intel_row_nearest_is_selected",
        "intel_row_two_characters_bridged",
    ] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let mut harness = harness::build(&mut scene, true);
        harness::shot(&mut harness, name);
    }
}

/// The same low-disk situation with and without anything on screen saying
/// so. Sized to the whole window, since the point is that the banner does not displace the chrome.
#[test]
#[ignore = "renders to target/uishots; run with --ignored"]
fn uitest_screenshots_disk_banner() {
    for name in [
        "disk_banner_low",
        "disk_banner_low_narrow",
        "disk_banner_critical",
        "disk_banner_critical_after_a_failed_write",
    ] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let mut harness = harness::build(&mut scene, true);
        harness::shot(&mut harness, name);
    }
}

/// The harness asserts layout, not frame time, so the only way to judge
/// whether a change to the history loop paid for itself is to time repeated passes over a
/// full-cap conversation and quote the number.
#[test]
#[ignore = "timing, not an assertion; run with --ignored --nocapture"]
fn uitest_bench_jabber_long_history() {
    const FRAMES: usize = 120;
    let mut scene = all().into_iter().find(|s| s.name == "jabber_popout_long").expect("scene");
    let mut harness = harness::build(&mut scene, false);
    harness.run_steps(10);
    let t = std::time::Instant::now();
    harness.run_steps(FRAMES);
    let per = t.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64;

    let msgs = fixtures::jabber_state_long();
    let conv = msgs.chats.get(fixtures::JABBER_ROOM).expect("room history");
    let t = std::time::Instant::now();
    let mut sink = 0usize;
    for _ in 0..FRAMES {
        sink += std::hint::black_box(conv.clone()).len();
    }
    let clone_us = t.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
    assert_eq!(sink, FRAMES * fixtures::JABBER_LONG_LEN);

    println!(
        "jabber_popout_long: {:.2} ms/frame over {FRAMES} frames, \
         a bare {}-message clone costs {clone_us:.0} us",
        per,
        fixtures::JABBER_LONG_LEN
    );
}

/// The history skips over anything more than [`MSG_OVERDRAW`] outside the viewport, so what a
/// 1000-message conversation builds per pass tracks the window rather than the history. The upper
/// bound is loose on purpose: the point is the shape of the growth, not a pixel-exact row count.
#[test]
fn uitest_jabber_long_history_builds_only_what_is_near_the_viewport() {
    let mut scene = all().into_iter().find(|s| s.name == "jabber_popout_long").expect("scene");
    let mut harness = harness::build(&mut scene, false);
    harness.run_steps(4);
    let built = crate::app::built_msg_rows(&harness.ctx);
    assert!(built > 0, "no chat row was built at all, so this asserts nothing");
    assert!(
        built * 4 < fixtures::JABBER_LONG_LEN,
        "{built} of {} rows built into a 480px window, the history is not virtualized",
        fixtures::JABBER_LONG_LEN
    );
}

/// Virtualizing must break none of the three things that read the whole history: the newest
/// message is still what a fresh window lands on, the unread divider still sits where a scan from
/// message zero puts it, and grouping still suppresses the repeated nick.
#[test]
fn uitest_jabber_long_history_keeps_its_tail_divider_and_grouping() {
    use egui_kittest::kittest::NodeT as _;

    let mut scene = all().into_iter().find(|s| s.name == "jabber_popout_long").expect("scene");
    let harness = harness::build(&mut scene, false);
    let mut labels = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() == egui::accesskit::Role::Label {
            labels.push(n.label().or_else(|| n.value()).unwrap_or_default().to_string());
        }
    }
    let last = format!("#{}", fixtures::JABBER_LONG_LEN - 1);
    for needle in [last.as_str(), "— new —"] {
        assert!(
            labels.iter().any(|l| l.contains(needle)),
            "the long history drew no {needle:?}: {labels:?}"
        );
    }
    assert!(
        !labels.iter().any(|l| l.contains("#0 ") || l.ends_with("#0")),
        "the oldest message was built while the window sits at the newest end: {labels:?}"
    );
    let nicks = labels.iter().filter(|l| l.ends_with(':')).count();
    let bodies = labels.iter().filter(|l| l.contains('#')).count();
    assert!(
        nicks < bodies,
        "every visible message drew its own nick ({nicks} nicks, {bodies} bodies), so grouping \
         stopped working"
    );
}

/// Scrolling a virtualized history has to behave like scrolling a built one: the window opens on
/// the newest message, moves off it, keeps a contiguous run of messages under the pointer, comes
/// back to the same place, and reaches the tail again. A skipped row that reserves the wrong space
/// breaks the middle of that; one that reserves none breaks all of it.
///
/// Blind to a spacer that is wrong by a constant factor, which stays self-consistent because the
/// set of skipped rows is a function of the offset. That is the one error this cannot see.
#[test]
fn uitest_jabber_long_history_survives_a_scroll_round_trip() {
    use egui_kittest::kittest::NodeT as _;

    fn shown(harness: &egui_kittest::Harness<'_>) -> Vec<usize> {
        let mut v: Vec<usize> = harness
            .root()
            .children_recursive()
            .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Label)
            .filter_map(|n| {
                let l = n.accesskit_node().label().or_else(|| n.accesskit_node().value())?;
                l.rsplit_once('#').and_then(|(_, i)| i.parse::<usize>().ok())
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    fn wheel(harness: &egui_kittest::Harness<'_>, dy: f32) {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
    }

    let newest = fixtures::JABBER_LONG_LEN - 1;
    let scene = all().into_iter().find(|s| s.name == "jabber_popout_long").expect("scene");
    let mut scene = scene.hovered_at([260.0, 200.0]);
    let mut harness = harness::build(&mut scene, false);
    harness.run_steps(20);
    let fresh = shown(&harness);
    assert!(
        fresh.contains(&newest),
        "a fresh window did not land on the newest message: {fresh:?}"
    );

    // Creep up through the band this test then scrolls over, rather than jumping to it. A row's
    // height only becomes trustworthy once it has been built, and the harness lays out its first
    // pass with egui's defaults (see `harness::build`), which leaves anything never rebuilt short.
    for _ in 0..12 {
        wheel(&harness, 1000.0);
        harness.run_steps(8);
    }
    wheel(&harness, -4000.0);
    harness.run_steps(20);
    let mid = shown(&harness);
    assert!(!mid.is_empty(), "scrolling up emptied the history");
    assert!(
        mid.iter().max() < fresh.iter().min(),
        "scrolling up did not move off the tail: {mid:?} against {fresh:?}"
    );
    assert!(
        mid.windows(2).all(|w| w[1] == w[0] + 1),
        "the messages built while scrolled up are not contiguous: {mid:?}"
    );

    wheel(&harness, 3000.0);
    harness.run_steps(20);
    wheel(&harness, -3000.0);
    harness.run_steps(20);
    assert_eq!(
        shown(&harness),
        mid,
        "3000pt up and back landed somewhere else, so a spacer is not the height of the row it \
         stands in for"
    );

    wheel(&harness, -20_000.0);
    harness.run_steps(20);
    let back = shown(&harness);
    assert!(back.contains(&newest), "scrolling back to the end lost the tail: {back:?}");
    assert!(
        back.windows(2).all(|w| w[1] == w[0] + 1),
        "the messages built back at the tail are not contiguous: {back:?}"
    );
}

/// [`alert_window_scene`] with the pin and the countdown under the test's control. Every scene in
/// `all()` pins the window, which makes `active` permanently true and leaves the expiry path
/// unrendered.
fn alert_countdown_scene(name: &'static str, pinned: bool, secs: f32) -> Scene {
    let mut st = crate::app::AlertWindowState {
        enabled: true,
        pinned,
        secs,
        systems: Some(fixtures::systems()),
        resolved_pilots: fixtures::resolved_pilots(),
        uncertain: fixtures::uncertain(),
        kills: Some(fixtures::kills()),
        affil: Some(fixtures::affil()),
        ..Default::default()
    };
    st.from_you = vec![None];
    st.feed = vec![(fixtures::intel_typical(), crate::settings::Severity::Danger)];
    let shared: crate::app::SharedAlertWindow = std::sync::Arc::new(std::sync::Mutex::new(st));
    let cb = crate::app::build_alert_viewport_cb(shared);
    Scene::ctx(name, [560.0, 480.0], move |ctx| {
        let mut ui = harness::detached_ui(ctx);
        cb(&mut ui, egui::ViewportClass::Root);
    })
}

/// Everything the viewports asked the windowing system for in the last pass. Visibility and
/// click-through leave no other trace, so the command stream is the only place the alert window's
/// expiry is observable.
fn viewport_commands(harness: &egui_kittest::Harness<'_>) -> Vec<egui::ViewportCommand> {
    harness.output().viewport_output.values().flat_map(|v| v.commands.iter().cloned()).collect()
}

/// An unpinned alert window has to stop absorbing clicks when its countdown runs out, or it sits
/// over the EVE client eating every click aimed at the game. `secs` decays by `unstable_dt`, which
/// kittest sets to `step_dt` (0.25s), so stepping drives it; `Instant::now()` would not.
#[test]
fn uitest_alert_window_auto_dismiss_hands_clicks_back_to_the_game() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = alert_countdown_scene("alert_countdown_probe", false, 5.0);
    let mut harness = harness::build(&mut scene, false);
    assert!(
        harness.query_by_label_contains("Intel alerts").is_some(),
        "the alert window painted no title while its countdown was still running"
    );
    assert!(
        harness.query_by_label_contains("Hostile Pilot").is_some(),
        "the alert window painted no feed while its countdown was still running"
    );

    let mut steps = 0;
    let mut expired_at = None;
    let mut passthrough_at = None;
    let mut hidden_at = None;
    while expired_at.is_none() {
        steps += 1;
        assert!(steps <= 60, "the countdown had not expired after {steps} passes");
        harness.run_steps(1);
        for cmd in viewport_commands(&harness) {
            match cmd {
                egui::ViewportCommand::MousePassthrough(true) => passthrough_at = Some(steps),
                egui::ViewportCommand::Visible(false) => hidden_at = Some(steps),
                _ => {}
            }
        }
        if harness.query_by_label_contains("Intel alerts").is_none() {
            expired_at = Some(steps);
        }
    }

    assert!(
        harness.query_by_label_contains("Hostile Pilot").is_none(),
        "the feed is still painted after the countdown expired"
    );
    assert_eq!(
        passthrough_at, expired_at,
        "the window stopped painting on pass {expired_at:?} but handed clicks back on \
         {passthrough_at:?}"
    );
    // The alert window only unmaps on Windows; elsewhere it stays mapped and click-through, so a
    // `Visible(false)` here would be a regression.
    let expect_hidden = if cfg!(target_os = "windows") { expired_at } else { None };
    assert_eq!(
        hidden_at, expect_hidden,
        "visibility on expiry was wrong: hid on pass {hidden_at:?}, wanted {expect_hidden:?}"
    );
}

/// The other half of the same branch, and what every scene in `all()` leans on: a pin holds the
/// window open with the countdown frozen, however long the app runs.
#[test]
fn uitest_alert_window_pin_survives_the_countdown() {
    use egui_kittest::kittest::Queryable as _;

    let mut scene = alert_countdown_scene("alert_pinned_probe", true, 5.0);
    let mut harness = harness::build(&mut scene, false);
    let mut fired: Vec<egui::ViewportCommand> = Vec::new();
    for _ in 0..60 {
        harness.run_steps(1);
        fired.extend(viewport_commands(&harness));
    }
    assert!(
        harness.query_by_label_contains("Intel alerts").is_some(),
        "the pinned alert window closed itself"
    );
    assert!(
        harness.query_by_label("5s").is_some(),
        "the pinned alert window ran its countdown down instead of holding it"
    );
    assert!(
        !fired.contains(&egui::ViewportCommand::MousePassthrough(true)),
        "the pinned alert window handed clicks back to the game: {fired:?}"
    );
}

/// The characters list with two rows, which is where `Remove` and `Re-auth` sit. Headless runs no
/// SSO, so the rows are seeded rather than logged in; the second is missing scopes and has an
/// expired token, which is the branch that draws `Re-auth`.
fn characters_rows_scene(name: &'static str, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Characters;
            a.characters = vec![
                crate::store::CharacterRow {
                    id: 90_000_001,
                    name: "Amryu".to_owned(),
                    expires_at: fixtures::now() + 1_200,
                    scopes: crate::auth::DEFAULT_SCOPES.join(" "),
                },
                crate::store::CharacterRow {
                    id: 90_000_002,
                    name: "Scout Alt".to_owned(),
                    expires_at: fixtures::now() - 60,
                    scopes: crate::auth::DEFAULT_SCOPES[0].to_owned(),
                },
            ];
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// The alert rules editor with three rules, which is the only place the reorder arrows and the
/// four condition `Edit` buttons draw. The editor auto-selects the first rule, so the right pane
/// renders without seeding a selection.
fn alert_rules_scene(name: &'static str, size: [f32; 2], panel_w: Option<f32>) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        // A resizable `Panel` takes its width from persisted state, so pinning that is the only
        // way to reach the drag minimum with no pointer to drag the separator.
        if let Some(w) = panel_w {
            ui.ctx().data_mut(|d| {
                d.insert_persisted(
                    egui::Id::new("alert_rules_split"),
                    egui::PanelState {
                        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, 100.0)),
                    },
                );
            });
        }
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = View::Alerts;
            a.alert_rules_open = true;
            a.settings.alerts.rules = ["Hostiles near home", "Cyno in Delve", "Quiet hours"]
                .into_iter()
                .map(|name| crate::settings::AlertRule {
                    name: name.to_owned(),
                    ..Default::default()
                })
                .collect();
            crate::settings::ensure_rule_ids(&mut a.settings.alerts.rules);
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    })
}

/// Every `Button` in a scene, as (label, rect), in tree order.
fn button_rects(harness: &egui_kittest::Harness<'_>) -> Vec<(String, egui::Rect)> {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT as _;

    let mut out = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden() || n.role() != Role::Button {
            continue;
        }
        let Some(b) = n.bounding_box() else { continue };
        out.push((
            n.label().unwrap_or_default().to_string(),
            egui::Rect {
                min: egui::pos2(b.x0 as f32, b.y0 as f32),
                max: egui::pos2(b.x1 as f32, b.y1 as f32),
            },
        ));
    }
    out
}

fn buttons_labelled(
    harness: &egui_kittest::Harness<'_>,
    label: &str,
) -> Vec<(String, egui::Rect)> {
    button_rects(harness).into_iter().filter(|(l, _)| l == label).collect()
}

/// `Remove` and `Re-auth` carry text labels and sit beside a full-height checkbox, so a
/// `small_button` would make them the shortest targets in the view at 17px against its 27px norm.
#[test]
fn uitest_character_row_buttons_match_the_view() {
    let mut scene = characters_rows_scene("character_button_probe", [1280.0, 800.0]);
    let harness = harness::build(&mut scene, false);
    let add = buttons_labelled(&harness, "Add character (EVE SSO)")
        .first()
        .expect("no Add character button")
        .1;
    let mut rows = buttons_labelled(&harness, "Remove");
    rows.extend(buttons_labelled(&harness, "Re-auth"));
    assert_eq!(rows.len(), 3, "expected two Remove and one Re-auth: {rows:?}");
    for (label, r) in rows {
        assert!(
            (r.height() - add.height()).abs() < 1.0,
            "{label} is {:.1}px tall against {:.1}px for Add character",
            r.height(),
            add.height()
        );
    }
}

/// The condition `Edit` buttons are labelled controls in a form whose every other control is
/// floored at `interact_size.y`. The `requires:` chips are the nearest peer.
#[test]
fn uitest_alert_rule_edit_buttons_match_the_condition_chips() {
    let mut scene = alert_rules_scene("alert_edit_probe", [1280.0, 800.0], None);
    let harness = harness::build(&mut scene, false);
    let chip = buttons_labelled(&harness, "bubble").first().expect("no requires chip").1;
    let edits = buttons_labelled(&harness, "Edit");
    assert_eq!(edits.len(), 6, "expected six Edit buttons: {edits:?}");
    for (_, r) in edits {
        assert!(
            (r.height() - chip.height()).abs() < 1.0,
            "Edit is {:.1}px tall against {:.1}px for the bubble chip",
            r.height(),
            chip.height()
        );
    }
}



/// With no session there is no rendered preview, and an FC still has to be able to ping. The
/// fallback renders the same template the rescue uses, from the form.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_ping_falls_back_to_the_local_template() {
    harness::scratch_profile();
    let ctx = egui::Context::default();
    let mut app = crate::app::SpaiApp::build(&ctx, true);
    app.settings.fleet_enabled = true;
    // Set to a name that is not the form's FC, so the assertion below proves which one is used.
    app.settings.fleet_character = "Stale Setting".to_owned();
    app.settings.rescue_staging_system = "Placeholder Staging".to_owned();
    app.settings.rescue_ping_template =
        "FC Name: {fc}\nFormup Location: {staging}\nComms: Op {op} {mumble}\nDoctrine: {doctrine}"
            .to_owned();
    fixtures::seed_fleet_state(&app);

    let out = app.fleet_local_ping();
    // The FC picked for this fleet. It used to be the old Acting character setting, which named
    // whoever that was set to even with someone else picked in the form.
    let fc = app.fleet_state_for_test().lock().unwrap().fc().expect("an fc").1;
    assert!(out.contains(&format!("FC Name: {fc}")), "{out}");
    assert!(!out.contains("Stale Setting"), "the retired setting still names the FC: {out}");
    assert!(out.contains("Formup Location: Placeholder Staging"), "{out}");
    // Nothing is picked in the draft yet, so the unknowns say so rather than leaking a
    // placeholder into a ping an FC is about to send.
    assert!(out.contains("Doctrine: ?"), "{out}");
    assert!(!out.contains('{'), "a placeholder survived: {out}");

    // And it is what goes to Jabber, wrapped for the bot.
    let body = crate::fleets::ping::bping("coord", &out);
    assert!(body.starts_with("!bping coord\n\n"));
}

/// The boss-check window must not grow because of what is in it, the same feedback loop the
/// Doctrines window had: a scroll area claiming `available_height` with a button underneath.
#[cfg(feature = "fleet")]
#[test]
fn uitest_boss_detail_window_height_settles() {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let mut scene = Scene::ctx("boss_detail_growth_probe", [900.0, 700.0], move |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ctx, true);
            a.settings.fleet_enabled = true;
            a.fleet_boss_detail = Some(
                (0..40).map(|i| format!("   at Fleet.Check frame {i}")).collect::<Vec<_>>().join("\n"),
            );
            a
        });
        app.fleet_boss_detail_window(ctx);
    });
    let mut harness = harness::build(&mut scene, false);
    let mut heights = Vec::new();
    for _ in 0..12 {
        harness.run();
        heights.push(
            harness
                .ctx
                .memory(|m| m.area_rect(egui::Id::new("Fleet boss check")).map(|r| r.height()))
                .expect("the window did not render"),
        );
    }
    let settled = &heights[1..];
    assert!(
        settled.iter().all(|h| (h - settled[0]).abs() < 0.5),
        "the window is still growing: {heights:?}"
    );
    assert!(settled[0] <= 660.0, "the window swallowed the canvas: {heights:?}");
}

/// A permanent tripwire. The uitest guard redirects `EVE_SPAI_DATA_DIR`, but the keychain sits
/// outside it, so a developer with a live dashboard session in their keyring must still never get
/// an `HttpBackend` under test. The headless arm of `SpaiApp::build` is what prevents it.
#[cfg(feature = "fleet")]
#[test]
fn uitest_headless_never_gets_a_live_fleet_backend() {
    use crate::fleets::backend::Mode;
    harness::scratch_profile();
    let ctx = egui::Context::default();
    let app = crate::app::SpaiApp::build(&ctx, true);
    assert_eq!(app.fleet_mode_for_test(), Mode::DryRun);
}

/// Fails if a dialog scene degrades to the empty root panel, which is what a wrong gate field or a
/// changed viewport route would look like: `uitest_layout` stays green on a blank scene.
#[test]
fn uitest_dialog_scenes_render_their_dialog() {
    use egui_kittest::kittest::NodeT as _;

    for (name, needle) in [
        ("dialog_severity", "High-threat hulls (one per line)"),
        ("dialog_note_editor_pilot", "Cloaky camper"),
        ("dialog_notes_manager_system", "Home staging, keepstar on the sun."),
        ("dialog_intel_channels", "Querious Intel"),
        ("dialog_jump_bridges", "1DQ1-A » O-EIMK"),
        ("dialog_coalitions", "Alliances (sov holders)"),
        ("dialog_battle_filter", "Add rule"),
        ("dialog_routes", "Home run"),
        ("dialog_filter_picker", "2 selected"),
        ("dialog_verdict_explainer", "Uncertain pilot (?)"),
    ] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let harness = harness::build(&mut scene, false);
        let found = harness.root().children_recursive().any(|node| {
            let n = node.accesskit_node();
            n.label().or_else(|| n.value()).is_some_and(|t| t.contains(needle))
        });
        assert!(found, "{name} rendered without {needle:?}");
    }
}

/// The reorder arrows sit under the list, so a rule name gets the whole row. Guards both halves:
/// full names at the default 240px width, and no overlap of the name's click rect with the arrows.
#[test]
fn uitest_alert_rule_names_fit_and_clear_the_arrows() {
    use egui_phosphor::regular as ic;

    for (scene_name, truncates) in
        [("view_alert_rules", false), ("view_alert_rules_narrow", true)]
    {
        let mut scene = all().into_iter().find(|s| s.name == scene_name).expect("scene");
        let harness = harness::build(&mut scene, false);
        let font = egui::TextStyle::Button.resolve(&harness.ctx.global_style());
        let arrows: Vec<egui::Rect> = button_rects(&harness)
            .into_iter()
            .filter(|(l, _)| l == ic::ARROW_UP || l == ic::ARROW_DOWN)
            .map(|(_, r)| r)
            .collect();
        assert_eq!(arrows.len(), 2, "{scene_name}: expected the two reorder arrows");

        for name in ["Hostiles near home", "Cyno in Delve", "Quiet hours"] {
            let (_, rect) = buttons_labelled(&harness, name)
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("{scene_name}: no rule button for {name:?}"));
            let full = harness.ctx.fonts_mut(|f| {
                f.layout_no_wrap(name.to_owned(), font.clone(), egui::Color32::PLACEHOLDER).size().x
            });
            // A selectable button pads its text, so a rect narrower than the bare galley means the
            // name was cut.
            if !truncates {
                assert!(
                    rect.width() >= full,
                    "{scene_name}: {name:?} is {:.1}px wide against {:.1}px of text",
                    rect.width(),
                    full
                );
            }
            for a in &arrows {
                assert!(
                    !rect.intersects(*a),
                    "{scene_name}: {name:?} {rect:?} overlaps a reorder arrow {a:?}"
                );
            }
        }
    }
}

/// Dialogs are plain windows, so none may draw an always-on-top pin, whose strip would push the
/// dialog's content down.
#[test]
fn uitest_dialogs_carry_no_pin() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT as _;

    for name in ["dialog_severity", "dialog_intel_channels", "dialog_jump_bridges", "dialog_coalitions", "dialog_battle_filter", "dialog_notes_manager_pilot", "dialog_pilot_window"] {
        let mut scene = all().into_iter().find(|s| s.name == name).expect("scene");
        let harness = harness::build(&mut scene, false);
        let pinned = harness.root().children_recursive().any(|node| {
            let n = node.accesskit_node();
            n.role() == Role::Button
                && n.label().unwrap_or_default().contains(egui_phosphor::regular::PUSH_PIN)
        });
        assert!(!pinned, "{name} still draws the always-on-top pin");
    }
}

/// The Jove observatory layer is the only way to see the baked observatory list, so the checkbox
/// has to reach the Layers panel, not just the overlay struct.
#[test]
fn uitest_map_layers_offers_the_jove_observatory_filter() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT as _;

    let mut scene = all().into_iter().find(|s| s.name == "map_layers").expect("scene");
    let harness = harness::build(&mut scene, false);
    let mut labels = Vec::new();
    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden() || n.role() != Role::CheckBox {
            continue;
        }
        labels.push(n.label().unwrap_or_default().to_string());
    }
    let want = format!("{}  Jove observatories", egui_phosphor::regular::CELL_TOWER);
    assert!(
        labels.iter().any(|l| l.contains(&want)),
        "no Jove observatory layer among the map layers: {labels:?}"
    );
}

/// The Doctrines window must not grow because of what is in it. It did: the doctrine list sized
/// itself to `available_height`, the window sized itself to that content, and every frame added a
/// little more.
#[cfg(feature = "fleet")]
#[test]
fn uitest_doctrines_window_height_settles() {
    let mut scene = fleet_boost_editor_scene("doctrines_growth_probe", [1100.0, 760.0]);
    let mut harness = harness::build(&mut scene, false);
    harness.ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new("fleet_doctrine_io_status"),
            "Exported 26 doctrines to /fixture/doctrines.json".to_owned(),
        )
    });
    let mut heights = Vec::new();
    for i in 0..18 {
        // Halfway through, switch to the hull list, which had no scroll area of its own.
        if i == 6 {
            harness.ctx.data_mut(|d| d.insert_temp(egui::Id::new("fleet_doctrine_tab"), 1_u8));
        }
        harness.run();
        let h = harness
            .ctx
            .memory(|m| m.area_rect(egui::Id::new("Doctrines")).map(|r| r.height()))
            .expect("the Doctrines window did not render");
        heights.push(h);
    }
    // The first frame may settle once; after that nothing in the window may move it.
    let settled = &heights[1..];
    assert!(
        settled.iter().all(|h| (h - settled[0]).abs() < 0.5),
        "the window is still growing: {heights:?}"
    );
    assert!(settled[0] <= 720.0, "the window swallowed the canvas: {heights:?}");
}

/// Toggles must not change size when the pointer crosses them.
///
/// egui gives an unframed button a stroke on hover and that stroke counts toward its size, so a
/// row of tabs shuffles as the mouse moves across it. `SteadySelect` exists to avoid exactly that;
/// this is the gate, because a screenshot renders one state and cannot see it.
#[test]
fn uitest_selectable_toggles_do_not_resize_on_hover() {
    use crate::app::SteadySelect as _;
    harness::scratch_profile();

    // Both kinds side by side: the steady helper, and the raw button it replaced. The raw one is
    // the control. If it ever stops growing, egui changed and this test has lost its teeth.
    let mut scene = Scene::ui("selftest_hover_size", [420.0, 120.0], |ui| {
        ui.horizontal(|ui| {
            ui.menu_label_sized([120.0, 22.0], false, "steady");
            let _ = ui.add_sized([120.0, 22.0], egui::Button::selectable(false, "raw"));
        });
    });
    let mut harness = harness::build(&mut scene, false);
    harness.run();
    let rect_of = |h: &egui_kittest::Harness<'_>, label: &str| {
        use egui_kittest::kittest::Queryable as _;
        h.get_by_label(label).rect().size()
    };
    let steady_before = rect_of(&harness, "steady");
    let raw_before = rect_of(&harness, "raw");

    {
        use egui_kittest::kittest::Queryable as _;
        harness.get_by_label("steady").hover();
    }
    harness.run();
    let steady_after = rect_of(&harness, "steady");

    {
        use egui_kittest::kittest::Queryable as _;
        harness.get_by_label("raw").hover();
    }
    harness.run();
    let raw_after = rect_of(&harness, "raw");

    assert_eq!(
        steady_before, steady_after,
        "a steady toggle moved on hover: {steady_before:?} -> {steady_after:?}"
    );
    assert_ne!(
        raw_before, raw_after,
        "the raw control stopped growing on hover, so this test no longer proves anything"
    );
}

/// Every saved-fleet row is the same width, and stays that width frame after frame.
///
/// Both halves have gone wrong here: a row sized from `available_width()` grew a few pixels per
/// row because the row before it had widened the container, and the whole column grew every frame
/// because that widening fed back in. Measured, not eyeballed: a screenshot of one frame shows
/// neither.
#[cfg(feature = "fleet")]
#[test]
fn uitest_preset_rows_are_one_width_and_stay_put() {
    use egui_kittest::kittest::Queryable as _;
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let mut scene = Scene::ui("preset_row_widths", [1440.0, 820.0], move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            // Mixed lengths and a folder: a long name must truncate rather than widen its row,
            // and an indented row must still agree with its neighbours.
            a.settings.fleet_presets = (0..6)
                .map(|i| crate::settings::FleetPreset {
                    label: if i == 2 {
                        "Preset 2 with a very long name indeed".to_owned()
                    } else {
                        format!("Preset {i}")
                    },
                    folder: if i >= 4 { "Folder".to_owned() } else { String::new() },
                    ..Default::default()
                })
                .collect();
            a.view = View::Fleet;
            fixtures::seed_fleet_state(&a);
            fixtures::open_fleet_start(&a);
            a
        });
        app.root_chrome(ui);
        app.root_central(ui, None);
    });
    let mut harness = harness::build(&mut scene, false);
    let label = |i: usize| {
        if i == 2 {
            "Preset 2 with a very long name indeed".to_owned()
        } else {
            format!("Preset {i}")
        }
    };
    let mut first: Option<Vec<f32>> = None;
    for pass in 0..4 {
        harness.run();
        let widths: Vec<f32> =
            (0..4).map(|i| harness.get_by_label(&label(i)).rect().width()).collect();
        assert!(
            widths.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5),
            "rows disagree on pass {pass}: {widths:?}"
        );
        match &first {
            None => first = Some(widths),
            Some(f) => assert!(
                f.iter().zip(&widths).all(|(a, b)| (a - b).abs() < 0.5),
                "rows grew by pass {pass}: {f:?} -> {widths:?}"
            ),
        }
    }
}


/// A closed fleet is read to settle participation, so the list leads with who got paid most.
#[cfg(feature = "fleet")]
#[test]
fn uitest_closed_fleet_sorts_by_participation() {
    let mut scene = fleet_detail_scene_with(
        "closed_fleet_paps",
        [1280.0, 1000.0],
        crate::app::fleet_ui::DetailTab::Members,
        true,
    );
    let mut harness = harness::build(&mut scene, false);
    harness.run();
    harness.run();
    use egui_kittest::kittest::Queryable as _;
    let mut seen: Vec<(i64, i64)> = Vec::new();
    for (text, paps) in
        [("no PAP", 0), ("1 PAP", 1), ("2 PAP", 2), ("3 PAP", 3)]
    {
        for n in harness.get_all_by_label(text) {
            seen.push((n.rect().top() as i64, paps));
        }
    }
    assert!(seen.len() >= 8, "the participation column did not render: {seen:?}");
    seen.sort_by_key(|(y, _)| *y);
    assert!(
        seen.windows(2).all(|w| w[0].1 >= w[1].1),
        "the list is not in descending PAP order: {seen:?}"
    );
    // A pilot who got nothing has to say so rather than leave the cell blank.
    assert!(seen.iter().any(|(_, n)| *n == 0), "no zero-PAP row rendered: {seen:?}");
}

/// A closed fleet renders as a flat participant list, and its rows must hold their columns.
///
/// They did not: a 150px group name in a 120px cell pushed every column after it 29px right, so
/// on a fleet of hictors the ship, role and kick columns stepped out of line row by row. A
/// screenshot reads that as an indent rather than as an overflow, which is why this measures.
#[cfg(feature = "fleet")]
#[test]
fn uitest_closed_fleet_rows_keep_their_columns() {
    use egui_kittest::kittest::NodeT as _;
    let mut scene = fleet_detail_scene_with(
        "closed_fleet_columns",
        [1280.0, 1000.0],
        crate::app::fleet_ui::DetailTab::Members,
        true,
    );
    let mut harness = harness::build(&mut scene, false);
    harness.run();
    harness.run();
    // The roster occupies the left of the page; the composition pane beside it has columns of its
    // own that would otherwise be read as roster rows.
    let mut rows: std::collections::BTreeMap<i64, Vec<f32>> = Default::default();
    for n in harness.root().children_recursive() {
        let a = n.accesskit_node();
        if a.is_hidden() || a.role() != egui::accesskit::Role::Label {
            continue;
        }
        let Some(b) = a.bounding_box() else { continue };
        if b.x0 < 300.0 || b.x0 > 900.0 || b.y0 < 150.0 {
            continue;
        }
        rows.entry(b.y0 as i64).or_default().push(b.x0 as f32);
    }
    let rows: Vec<Vec<f32>> = rows
        .into_values()
        .map(|mut r| {
            r.sort_by(|a, b| a.partial_cmp(b).unwrap());
            r
        })
        .filter(|r| r.len() >= 3)
        .collect();
    assert!(rows.len() >= 4, "the flat roster did not render: {rows:?}");
    let first = rows[0].clone();
    for r in &rows {
        for (i, (a, b)) in first.iter().zip(r).enumerate() {
            assert!(
                (a - b).abs() < 0.5,
                "column {i} is out of line: {a} vs {b}\nrows: {rows:?}"
            );
        }
    }
}

/// A fleet can close on the dashboard's own auto-close timer while this tab is still sitting on
/// the tracking page. Every button there would act on a fleet that no longer exists, so the page
/// has to follow the fleet's own record rather than the route the user took to it.
#[cfg(feature = "fleet")]
#[test]
fn uitest_a_fleet_that_closes_under_the_page_goes_read_only() {
    use egui_kittest::kittest::Queryable as _;
    // The control: still running, still on the tracking page, so the live controls are there.
    let mut live = fleet_detail_scene("live_under_page", [1280.0, 1000.0],
                                      crate::app::fleet_ui::DetailTab::Members);
    let mut h = harness::build(&mut live, false);
    h.run();
    assert!(h.query_by_label_contains("Close fleet").is_some(), "no live action bar to lose");

    // Same page, same route, only the fleet's own record says it is over.
    let mut scene = fleet_detail_scene_closed(
        "closed_under_page",
        [1280.0, 1000.0],
        crate::app::fleet_ui::DetailTab::Members,
        true,
        false,
    );
    let mut h = harness::build(&mut scene, false);
    h.run();
    h.run();
    assert!(
        h.query_by_label_contains("Close fleet").is_none(),
        "the live action bar survived the fleet closing"
    );
    assert!(
        h.query_by_label_contains("closed").is_some(),
        "the page does not say the fleet is closed"
    );
}

/// Closing a fleet has to land on the closed view.
///
/// It did not: the write was recorded and the page stayed on the live one, still offering to kick
/// pilots out of an in-game fleet that no longer existed. This drives the app's own backend and
/// its own dispatcher, so it covers the whole path rather than any one piece of it.
#[cfg(feature = "fleet")]
#[test]
fn uitest_closing_a_fleet_lands_on_the_closed_view() {
    use crate::fleets::state::Page;

    harness::scratch_profile();
    let ctx = egui::Context::default();
    let mut app = crate::app::SpaiApp::build(&ctx, true);
    app.settings.fleet_enabled = true;

    let backend = app.fleet_backend_for_test().clone();
    let id = backend.active(true).expect("active").first().expect("a fleet").id.clone();
    {
        let mut st = app.fleet_state_for_test().lock().unwrap();
        st.page = Page::Tracking(id.clone());
        let seed = st.seed.clone();
        st.apply(crate::fleets::state::run(
            backend.as_ref(),
            &seed,
            crate::fleets::state::Cmd::Open(id.clone()),
        ));
        let open = st.open.value.as_ref().expect("opened");
        assert!(open.fleet.closed_at.is_none(), "the fixture fleet is already closed");
        assert!(!open.composition.flat, "a live fleet has a tree");
    }

    app.fleet_act_for_test(crate::fleets::backend::Action::Close);
    // The dispatcher is a real thread and the spoof answers instantly; the bound is only here so a
    // hang fails the test instead of parking the suite.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.fleet_collect_for_test();
        let page = app.fleet_state_for_test().lock().unwrap().page.clone();
        if matches!(page, Page::Historic(ref h) if *h == id) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the page never left the live view: {page:?}");
        std::thread::yield_now();
    }

    // Navigating re-opens it, which is what turns the tree into the participant list. The first
    // read finds no report, because the dashboard writes a fleet's statistics after the close, so
    // the app has to ask again before the list is there.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut saw_empty = false;
    loop {
        app.fleet_collect_for_test();
        app.fleet_reopen_poll_for_test();
        let st = app.fleet_state_for_test().lock().unwrap();
        if let Some(open) = st.open.value.as_ref() {
            assert_eq!(open.fleet.id, id, "a different fleet was loaded");
            if open.fleet.closed_at.is_some() {
                assert!(open.composition.flat, "the closed fleet still claims a tree");
                if open.composition.total() > 0 {
                    assert!(saw_empty, "the spoof never withheld its report, so nothing was proven");
                    return;
                }
                saw_empty = true;
            }
        }
        drop(st);
        assert!(
            std::time::Instant::now() < deadline,
            "the participant list never arrived after the close"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The fleet's named roles belong to the fleet, not to the ping that started it, so the settings
/// panel lists them and can edit them. A closed fleet still shows who was on it: that record is
/// what a participation correction is made against.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_settings_panel_lists_the_snowflakes() {
    use egui_kittest::kittest::Queryable as _;
    for (what, closed) in [("live", false), ("closed", true)] {
        let mut scene = fleet_detail_scene_with(
            "snowflakes_in_settings",
            [1280.0, 1400.0],
            crate::app::fleet_ui::DetailTab::Members,
            closed,
        );
        let mut h = harness::build(&mut scene, false);
        h.run();
        h.run();
        for name in ["Second Seat", "Quiet Observer", "Anchor Pilot"] {
            assert!(
                h.query_by_label_contains(name).is_some(),
                "{what} fleet: the settings panel does not name {name}"
            );
        }
        // Two of the fixture's pilots are backseats, so the role is named twice.
        assert_eq!(
            h.query_all_by_label_contains("Backseat").count(),
            2,
            "{what} fleet: the roles are not named beside the pilots"
        );
        // Editable in both: who was FC or backseat is corrected after the fleet, which is the
        // whole reason the record exists.
        assert!(
            h.query_by_label("Edit").is_some(),
            "{what} fleet: the snowflakes cannot be edited"
        );
    }
}

/// Starting a fleet for an FC who already leads a tracked one is refused, and said in a row of its
/// own rather than only on the disabled button's hover.
///
/// The first version put that row after the right-to-left button group. That group claims the
/// panel's whole height, so the row grew the action bar over the form and left one line of it on
/// screen. The form's last control has to stay visible with the warning up.
#[cfg(feature = "fleet")]
#[test]
fn uitest_an_already_tracked_fc_cannot_start_again() {
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    let mut scene = fleet_start_scene_with("already_tracked", [1440.0, 820.0], true);
    let mut h = harness::build(&mut scene, false);
    h.run();
    h.run();
    assert!(
        h.query_by_label_contains("already tracked as").is_some(),
        "the guard does not say why tracking is refused"
    );
    // "Track fleet: not sent" is a label beside it, so the button is picked out by its role.
    let track = h
        .query_all_by_label_contains("Track fleet")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .expect("the Track fleet button");
    assert!(track.accesskit_node().is_disabled(), "a second fleet can still be started");

    // The form must survive the extra row. It was clipped rather than moved, so its controls
    // kept their natural positions; what gives it away is the action bar's top landing above the
    // form's last control.
    let manage = h.get_by_label_contains("Manage").rect();
    let save = h.get_by_label_contains("Save as preset").rect();
    assert!(
        save.top() > manage.bottom(),
        "the action bar has grown over the form: bar top {} above the form's last control {}",
        save.top(),
        manage.bottom()
    );
}

/// What the settings panel offers once a fleet has closed: its record is correctable, the rest
/// is not.
///
/// Tags and snowflakes stay live, because they describe what the fleet was and who led it. Setup
/// and comms are disabled rather than left editable behind an Apply that refuses them, and the
/// MOTD checkbox is gone, since there is no in-game fleet left to carry one.
#[cfg(feature = "fleet")]
#[test]
fn uitest_a_closed_fleet_offers_only_its_record() {
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    for closed in [false, true] {
        let mut scene = fleet_detail_scene_with(
            "closed_settings",
            [1280.0, 1400.0],
            crate::app::fleet_ui::DetailTab::Members,
            closed,
        );
        let mut h = harness::build(&mut scene, false);
        h.run();
        h.run();
        let what = if closed { "closed" } else { "live" };

        let motd = h.query_by_label_contains("Re-set the MOTD").is_some();
        assert_eq!(motd, !closed, "{what}: the MOTD checkbox");

        // A combo box carries its current value in `value`, not in its label.
        let combo = |v: &str| {
            h.root()
                .children_recursive()
                .find(|n| {
                    let a = n.accesskit_node();
                    a.role() == egui::accesskit::Role::ComboBox
                        && a.value().as_deref() == Some(v)
                })
                .unwrap_or_else(|| panic!("{what}: no combo showing {v}"))
        };
        let setup = combo("Fast Tackle");
        assert_eq!(setup.accesskit_node().is_disabled(), closed, "{what}: the setup combo");
        let comms = combo("Comms 3");
        assert_eq!(comms.accesskit_node().is_disabled(), closed, "{what}: the comms combo");


        // The snowflake editor stays offered either way.
        let edit = h.get_by_label("Edit");
        assert!(!edit.accesskit_node().is_disabled(), "{what}: snowflakes cannot be edited");
    }
}

/// Getting DM'd has to put the conversation in front of the user, whatever they did with that
/// person before.
///
/// Driven the way the worker drives it: the message goes through `jabber::receive_direct`, the frame is
/// built by `jabber_frame`, tabs by `jabber_reconcile`, and the list is the one the sidebar renders.
/// The older tests built the frame by hand, which skipped the filtering a forgotten DM goes through.
#[test]
fn uitest_getting_dmd_opens_the_conversation() {
    use egui_kittest::kittest::Queryable as _;
    const DM: &str = "stranger@goonfleet.com";

    #[derive(Clone, Copy, Debug)]
    enum Before {
        Nothing,
        Closed,
        Forgotten,
    }

    for (before, delayed) in [
        (Before::Nothing, false),
        (Before::Closed, false),
        (Before::Forgotten, false),
        // Delivered while offline: the server holds it and hands it over with a <delay/> at login.
        (Before::Nothing, true),
        (Before::Closed, true),
    ] {
        harness::scratch_profile();
        let mut app: Option<crate::app::SpaiApp> = None;
        let mut tabs: Vec<String> = Vec::new();
        let tabs_out = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let tabs_in = tabs_out.clone();
        let mut scene = Scene::ui("dm_arrives", [420.0, 560.0], move |ui| {
            let a = app.get_or_insert_with(|| {
                let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
                *a.jabber.lock().unwrap() = crate::jabber::JabberState::default();
                match before {
                    Before::Nothing => {}
                    Before::Closed => a.settings.jabber_closed_dms = vec![DM.to_owned()],
                    Before::Forgotten => a.settings.jabber_forgotten = vec![DM.to_owned()],
                }
                // The worker's own handling of a one-to-one message, live or held offline.
                crate::jabber::receive_direct(
                    &a.jabber,
                    DM,
                    "you around?".to_owned(),
                    1_700_000_000,
                    delayed,
                    None,
                );
                a
            });
            let mut f = a.jabber_frame(false);
            // Headless reports itself unconfigured, and reconcile does nothing for that.
            f.configured = true;
            f.ever_online = true;
            a.jabber_reconcile(&f);
            *tabs_in.lock().unwrap() = a.jabber_tabs_for_test();
            let mut f = a.jabber_frame(false);
            f.configured = true;
            f.ever_online = true;
            f.connected = true;
            a.jabber_sidebar_for_test(ui, &f, true);
        });
        let mut h = harness::build(&mut scene, false);
        h.run();
        h.run();
        tabs.extend(tabs_out.lock().unwrap().iter().cloned());

        let listed = h.query_all_by_label_contains("stranger").count() > 0;
        assert!(listed, "{before:?}, delayed {delayed}: the DM is not in the Convos list");
        assert!(
            tabs.iter().any(|t| t == DM),
            "{before:?}, delayed {delayed}: no tab opened for the DM (tabs: {tabs:?})"
        );
    }
}

/// Right after the first sign-in nothing is open, and the start rows are the only way in. They
/// were hidden behind "Nothing yet. Start one from the Directory." in exactly that state.
#[test]
fn uitest_an_empty_convos_list_still_offers_both_starts() {
    use egui_kittest::kittest::Queryable as _;
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let mut scene = Scene::ui("convos_empty", [420.0, 400.0], move |ui| {
        let a = app.get_or_insert_with(|| {
            let a = crate::app::SpaiApp::build(ui.ctx(), true);
            *a.jabber.lock().unwrap() = crate::jabber::JabberState::default();
            a
        });
        let mut f = a.jabber_frame(false);
        f.configured = true;
        f.ever_online = true;
        f.connected = true;
        assert!(f.convos.is_empty() && f.channels.is_empty(), "the fixture is not empty");
        a.jabber_sidebar_for_test(ui, &f, true);
    });
    let mut h = harness::build(&mut scene, false);
    h.run();
    for label in ["Direct messages", "Start a DM", "Rooms", "Join a room"] {
        assert!(h.query_by_label_contains(label).is_some(), "the empty list has no {label:?}");
    }
    assert!(h.query_by_label_contains("Nothing yet").is_none(), "the old dead end is back");
}

/// The Fleet Finder advert sits in the title row, green when it is up and yellow when it is off,
/// and never pushes the title onto a second line. Nothing at all when it cannot be read.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_advert_shows_in_the_title_row() {
    use egui_kittest::kittest::Queryable as _;
    for state in [Some(true), Some(false), None] {
        harness::scratch_profile();
        let mut app: Option<crate::app::SpaiApp> = None;
        let mut scene = Scene::ui("advert", [1280.0, 700.0], move |ui| {
            let a = app.get_or_insert_with(|| {
                let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
                a.settings.fleet_enabled = true;
                a.view = View::Fleet;
                fixtures::seed_fleet_state(&a);
                fixtures::open_first_fleet(&a);
                a.fleet_booted = true;
                {
                    let mut st = a.fleet_state_for_test().lock().unwrap();
                    let id = st.open.value.as_ref().unwrap().fleet.id.clone();
                    st.advert = state.map(|up| (id, up));
                }
                a
            });
            a.root_chrome(ui);
            a.root_central(ui, None);
        });
        let mut h = harness::build(&mut scene, false);
        h.run();
        h.run();
        let up = h.query_by_label_contains("advert up");
        let off = h.query_by_label_contains("advert off");
        match state {
            Some(true) => assert!(up.is_some() && off.is_none(), "advert up is not shown"),
            Some(false) => assert!(off.is_some() && up.is_none(), "advert off is not shown"),
            None => {
                assert!(up.is_none() && off.is_none(), "an advert state nobody could read");
                continue;
            }
        }
        let chip = up.or(off).unwrap().rect();
        // The setup's name, which sits right after the heading on the title row. The fleet's own
        // name is no anchor: it is also the breadcrumb above and one of its tags.
        let title = h.get_by_label("Fast Tackle").rect();
        assert!(
            (chip.center().y - title.center().y).abs() < title.height(),
            "{state:?}: the advert went onto another line: chip {chip:?}, title {title:?}"
        );
    }
}

/// The rescue's op picker says when the chosen op's channel is taken, before anything is pinged
/// onto it. A channel held by the fleet this app is running is expected, and says so quietly.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_rescue_op_alerts_when_its_channel_is_in_use() {
    use egui_kittest::kittest::Queryable as _;
    #[derive(Clone, Copy, Debug)]
    enum Case {
        Free,
        InUse,
        Ours,
    }
    for case in [Case::Free, Case::InUse, Case::Ours] {
        harness::scratch_profile();
        let mut app: Option<crate::app::SpaiApp> = None;
        let mut scene = Scene::ui("rescue_op_in_use", [1100.0, 700.0], move |ui| {
            let a = app.get_or_insert_with(|| {
                let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
                a.settings.fc_rescue_enabled = true;
                a.settings.fleet_enabled = true;
                a.settings.fleet_presets = fixtures::rescue_presets();
                a.settings.rescue_preset = "Capital Save".to_owned();
                fixtures::seed_fleet_state(&a);
                a.fleet_booted = true;
                fixtures::seed_rescue_ping(&a);
                {
                    let mut st = a.fleet_state_for_test().lock().unwrap();
                    // Named the way the dashboard names them, which the invented table is not.
                    st.seed.mumble_channels = (1..=12)
                        .map(|n| crate::fleets::model::ChannelItem {
                            id: crate::fleets::model::ChannelId(n),
                            name: format!("Op {n}"),
                            is_in_use: n == 3 && !matches!(case, Case::Free),
                        })
                        .collect();
                    if matches!(case, Case::Ours) {
                        st.open.put(crate::fleets::state::OpenFleet {
                            fleet: crate::fleets::model::Fleet {
                                mumble_channel_id: Some(crate::fleets::model::ChannelId(3)),
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                    }
                }
                a.rescue_state_for_test().lock().unwrap().op_channel = 3;
                a
            });
            a.rescue_window_body(ui);
        });
        let mut h = harness::build(&mut scene, false);
        // Stepped rather than run: the boss check asks to be repainted in a minute, and `run`
        // reads that as a page that never settles.
        h.run_steps(3);
        // Every case has to have a channel to judge, or a free op passes for having none.
        assert!(
            h.query_by_label_contains("no such channel").is_none(),
            "{case:?}: op 3 did not resolve to a channel"
        );
        assert!(h.query_all_by_label("Op 3").count() > 0, "{case:?}: the channel is not named");
        let alert = h.query_by_label_contains("in use").is_some();
        let ours = h.query_by_label_contains("your fleet").is_some();
        match case {
            Case::Free => assert!(!alert && !ours, "a free op raised something"),
            Case::InUse => assert!(alert && !ours, "a taken op did not alert"),
            Case::Ours => assert!(ours && !alert, "the FC's own fleet read as someone else's"),
        }
    }
}

/// Presets are identified by folder and name together, through the same actions the buttons use.
#[cfg(feature = "fleet")]
#[test]
fn uitest_a_preset_name_can_repeat_across_folders() {
    use crate::app::fleet_ui::FormAct;
    use crate::settings::preset_key;
    harness::scratch_profile();
    let ctx = egui::Context::default();
    let mut a = crate::app::SpaiApp::build(&ctx, true);
    a.settings.fleet_enabled = true;
    fixtures::seed_fleet_state(&a);
    let save = |a: &mut crate::app::SpaiApp, label: &str, folder: &str| {
        a.fleet_apply_form_for_test(FormAct {
            save_preset: Some((label.to_owned(), folder.to_owned())),
            ..Default::default()
        });
    };
    let keys = |a: &crate::app::SpaiApp| -> Vec<String> {
        a.settings.fleet_presets.iter().map(|p| p.key()).collect()
    };

    // The bug that asked for this: saving under a name used in another folder lifted that preset
    // out of its folder. Now it is a second preset and the first stays put.
    save(&mut a, "FNIs", "Rescue");
    save(&mut a, "FNIs", "");
    assert_eq!(keys(&a), vec![preset_key("Rescue", "FNIs"), preset_key("", "FNIs")]);

    // The same folder and name is the same preset, so a save there replaces it.
    save(&mut a, "FNIs", "Rescue");
    assert_eq!(a.settings.fleet_presets.len(), 2, "a re-save made a duplicate");

    // Moving one onto a folder that already has that name is refused rather than making two
    // presets nobody can tell apart.
    a.fleet_apply_form_for_test(FormAct { move_preset: Some((1, "Rescue".to_owned())), ..Default::default() });
    assert_eq!(keys(&a), vec![preset_key("Rescue", "FNIs"), preset_key("", "FNIs")]);

    // The rescue remembers its preset by key and follows it when it moves.
    a.settings.rescue_preset = preset_key("Rescue", "FNIs");
    a.rescue_state_for_test().lock().unwrap().doctrine = preset_key("Rescue", "FNIs");
    a.fleet_apply_form_for_test(FormAct { move_preset: Some((0, "Caps".to_owned())), ..Default::default() });
    assert_eq!(a.settings.rescue_preset, preset_key("Caps", "FNIs"));
    assert_eq!(a.rescue_state_for_test().lock().unwrap().doctrine, preset_key("Caps", "FNIs"));
}

/// The rescue lists every preset tagged Capital Save, whatever folder it is in, and tells two with
/// the same name apart by folder. Picking one sets the op from its channel's name: the channel id
/// the preset stores is not the op number.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_rescue_lists_every_capital_save_preset() {
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let op = std::sync::Arc::new(std::sync::Mutex::new(0u8));
    let op_out = op.clone();
    let mut scene = Scene::ui("rescue_presets", [1100.0, 700.0], move |ui| {
        let a = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fc_rescue_enabled = true;
            a.settings.fleet_enabled = true;
            let cap = |folder: &str, label: &str, channel: i32| crate::settings::FleetPreset {
                label: label.to_owned(),
                folder: folder.to_owned(),
                mumble_channel_id: Some(channel),
                tag_ids: vec![crate::settings::CAPITAL_SAVE_TAG],
                ..Default::default()
            };
            a.settings.fleet_presets = vec![
                cap("Rescue", "FNIs", 12),
                cap("Roams", "FNIs", 13),
                cap("Rescue", "Harpy", 1),
                // Not tagged, so not a rescue preset.
                crate::settings::FleetPreset { label: "Roam".to_owned(), ..Default::default() },
            ];
            fixtures::seed_fleet_state(&a);
            a.fleet_booted = true;
            fixtures::seed_rescue_ping(&a);
            // Named the way the dashboard does it: id 12 is Op 11 and id 13 is Op 12.
            a.fleet_state_for_test().lock().unwrap().seed.mumble_channels = [
                (1, "Op 1"),
                (12, "Op 11"),
                (13, "Op 12"),
            ]
            .into_iter()
            .map(|(id, name)| crate::fleets::model::ChannelItem {
                id: crate::fleets::model::ChannelId(id),
                name: name.to_owned(),
                is_in_use: false,
            })
            .collect();
            a.rescue_state_for_test().lock().unwrap().doctrine =
                crate::settings::preset_key("Rescue", "Harpy");
            a
        });
        a.rescue_window_body(ui);
        *op_out.lock().unwrap() = a.rescue_state_for_test().lock().unwrap().op_channel;
    });
    let mut h = harness::build(&mut scene, false);
    h.run_steps(3);

    let combo = h
        .root()
        .children_recursive()
        .find(|n| {
            n.accesskit_node().role() == egui::accesskit::Role::ComboBox
                && n.accesskit_node().value().as_deref() == Some("Harpy")
        })
        .expect("the rescue preset picker, showing Harpy")
        .rect();
    harness::click_at(&h, combo.center());
    h.run_steps(3);
    // Both FNIs presets, each with its folder since they share a name; Harpy alone keeps its name.
    for want in ["Rescue / FNIs", "Roams / FNIs", "Harpy"] {
        assert!(h.query_all_by_label(want).count() > 0, "the list has no {want:?}");
    }
    assert!(h.query_all_by_label("Roam").count() == 0, "an untagged preset is offered");

    // Channel id 12 is Op 11. Taking the id as the op, which the picker used to, gives 12.
    let item = h.get_by_label("Rescue / FNIs").rect();
    harness::click_at(&h, item.center());
    h.run_steps(3);
    assert_eq!(*op.lock().unwrap(), 11, "the op was taken from the channel id, not its name");
}

/// Presets and folders are ordered by dragging: a preset dropped on another goes just before it,
/// and a folder dropped on another folder's heading goes just before that folder.
#[cfg(feature = "fleet")]
#[test]
fn uitest_presets_and_folders_reorder_by_dragging() {
    use egui_kittest::kittest::Queryable as _;
    let grip = egui_phosphor::regular::DOTS_SIX_VERTICAL;
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = order.clone();
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let mut scene = Scene::ui("preset_order", [1440.0, 820.0], move |ui| {
        let a = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            let p = |folder: &str, label: &str| crate::settings::FleetPreset {
                label: label.to_owned(),
                folder: folder.to_owned(),
                ..Default::default()
            };
            a.settings.fleet_presets = vec![
                p("", "Alpha"),
                p("", "Bravo"),
                p("Caps", "Charlie"),
                p("Subs", "Delta"),
                p("Subs", "Echo"),
            ];
            a.view = View::Fleet;
            fixtures::seed_fleet_state(&a);
            fixtures::open_fleet_start(&a);
            a
        });
        a.root_chrome(ui);
        a.root_central(ui, None);
        *seen.lock().unwrap() = a
            .settings
            .fleet_presets
            .iter()
            .map(|p| format!("{}/{}", p.folder, p.label))
            .collect();
    });
    let mut h = harness::build(&mut scene, false);
    h.run();

    // The grip on the same row as a label, which is what a hand would pick up.
    // The grip on the same row as a label, which is what a hand would pick up. Found by query:
    // walking `root()` does not reach into the side panel here.
    let grip_of = |h: &egui_kittest::Harness<'_>, label: &str| -> egui::Pos2 {
        let row = h.get_by_label(label).rect();
        let grips: Vec<egui::Rect> = h.query_all_by_label(grip).map(|n| n.rect()).collect();
        grips
            .iter()
            .find(|r| (r.center().y - row.center().y).abs() < row.height() / 2.0)
            .unwrap_or_else(|| panic!("no grip on the {label} row among {grips:?}"))
            .center()
    };
    let drag = |h: &mut egui_kittest::Harness<'_>, from: egui::Pos2, to: egui::Pos2| {
        h.event(egui::Event::PointerMoved(from));
        h.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        h.step();
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            h.event(egui::Event::PointerMoved(from + (to - from) * t));
            h.step();
        }
        h.event(egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
        h.step();
        h.step();
    };

    // Bravo onto Alpha: Bravo first.
    let (from, to) = (grip_of(&h, "Bravo"), h.get_by_label("Alpha").rect().center());
    drag(&mut h, from, to);
    assert_eq!(order.lock().unwrap()[..2], ["/Bravo".to_owned(), "/Alpha".to_owned()]);

    // Delta, in Subs, onto Bravo at the top level: it comes out of its folder, before Bravo.
    let (from, to) = (grip_of(&h, "Delta"), h.get_by_label("Bravo").rect().center());
    drag(&mut h, from, to);
    assert_eq!(
        *order.lock().unwrap(),
        ["/Delta", "/Bravo", "/Alpha", "Caps/Charlie", "Subs/Echo"],
    );

    // A folder by its heading's grip: Subs dropped on Caps goes before it, Echo with it.
    let (from, to) = (grip_of(&h, "Subs"), h.get_by_label("Caps").rect().center());
    drag(&mut h, from, to);
    assert_eq!(
        *order.lock().unwrap(),
        ["/Delta", "/Bravo", "/Alpha", "Subs/Echo", "Caps/Charlie"],
    );
}

/// Changing the rescue's op has to change the comms in the ping straight away, and pick up the
/// dashboard's rendering when it lands, unless the FC has typed into the draft.
///
/// It did neither: the draft was rebuilt from the rendering still held for the old op, and when
/// the new one arrived nothing rebuilt it, so the old comms stayed until something else changed.
#[cfg(feature = "fleet")]
#[test]
fn uitest_changing_the_rescue_op_changes_the_comms_at_once() {
    let draft = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = draft.clone();
    // What the test does to the app between frames: set an op, hand in a dashboard rendering.
    let next = std::sync::Arc::new(std::sync::Mutex::new(None::<Box<dyn FnOnce(&mut crate::app::SpaiApp) + Send>>));
    let inbox = next.clone();
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let mut scene = Scene::ui("rescue_op_switch", [1100.0, 700.0], move |ui| {
        let a = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fc_rescue_enabled = true;
            a.settings.fleet_enabled = true;
            a.settings.fleet_presets = fixtures::rescue_presets();
            a.settings.rescue_preset = "Capital Save".to_owned();
            // The template names the op, so the stop-gap draft says which op it is for.
            a.settings.rescue_ping_template = "Comms: Op {op}".to_owned();
            fixtures::seed_fleet_state(&a);
            a.fleet_booted = true;
            fixtures::seed_rescue_ping(&a);
            a.fleet_state_for_test().lock().unwrap().seed.mumble_channels = (1..=12)
                .map(|n| crate::fleets::model::ChannelItem {
                    id: crate::fleets::model::ChannelId(n),
                    name: format!("Op {n}"),
                    is_in_use: false,
                })
                .collect();
            a.rescue_state_for_test().lock().unwrap().op_channel = 3;
            a
        });
        if let Some(f) = inbox.lock().unwrap().take() {
            f(a);
        }
        a.rescue_window_body(ui);
        *seen.lock().unwrap() = a.rescue_state_for_test().lock().unwrap().pending_ping.clone();
    });
    let mut h = harness::build(&mut scene, false);
    let run = |h: &mut egui_kittest::Harness<'_>, f: Box<dyn FnOnce(&mut crate::app::SpaiApp) + Send>| {
        *next.lock().unwrap() = Some(f);
        h.run_steps(4);
    };
    let dashboard = |text: &'static str| -> Box<dyn FnOnce(&mut crate::app::SpaiApp) + Send> {
        Box::new(move |a: &mut crate::app::SpaiApp| {
            a.fleet_state_for_test().lock().unwrap().rescue_preview =
                Some(crate::fleets::model::PingPreview { ping: text.to_owned(), motd: String::new() });
        })
    };

    // The dashboard's rendering for Op 3 is what the draft shows.
    run(&mut h, dashboard("DASHBOARD Op 3"));
    assert_eq!(*draft.lock().unwrap(), "DASHBOARD Op 3");

    // Op 5: the old rendering must not survive the switch, not even for a frame's worth of draft.
    run(&mut h, Box::new(|a| a.rescue_state_for_test().lock().unwrap().op_channel = 5));
    let now = draft.lock().unwrap().clone();
    assert!(!now.contains("Op 3"), "the draft still names the old comms: {now:?}");

    // The dashboard answers for Op 5: an untouched draft takes it.
    run(&mut h, dashboard("DASHBOARD Op 5"));
    assert_eq!(*draft.lock().unwrap(), "DASHBOARD Op 5", "the new rendering was not picked up");

    // Once the FC has typed, a late rendering leaves their draft alone.
    run(&mut h, Box::new(|a| {
        let mut r = a.rescue_state_for_test().lock().unwrap();
        r.pending_ping = "typed by hand".to_owned();
        r.ping_edited = true;
    }));
    run(&mut h, dashboard("DASHBOARD Op 5, again"));
    assert_eq!(*draft.lock().unwrap(), "typed by hand", "a hand-edited draft was overwritten");
}

/// Starting a fleet has to land on that fleet, loaded. It set the page and asked for nothing, so
/// the view sat on "Loading the fleet." until the FC opened it again from the list.
#[cfg(feature = "fleet")]
#[test]
fn uitest_starting_a_fleet_opens_it() {
    use crate::fleets::state::Page;
    harness::scratch_profile();
    let ctx = egui::Context::default();
    let mut a = crate::app::SpaiApp::build(&ctx, true);
    a.settings.fleet_enabled = true;
    fixtures::seed_fleet_state(&a);
    fixtures::open_fleet_start(&a);
    {
        // The fixture's FC already leads an active fleet, which the double-track guard refuses.
        let mut st = a.fleet_state_for_test().lock().unwrap();
        st.active_strat.put(Vec::new());
        st.active_pct.put(Vec::new());
        assert!(st.start_request().is_some(), "the fixture form cannot start a fleet");
    }
    a.fleet_apply_form_for_test(crate::app::fleet_ui::FormAct { start: true, ..Default::default() });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        a.fleet_collect_for_test();
        let st = a.fleet_state_for_test().lock().unwrap();
        if let Page::Tracking(id) = &st.page {
            if let Some(open) = st.open.value.as_ref() {
                assert_eq!(open.fleet.id, *id, "a different fleet is on screen");
                return;
            }
        }
        drop(st);
        assert!(std::time::Instant::now() < deadline, "the started fleet never loaded");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// "Join comms" is for someone else's fleet. On a fleet bossed by one of the account's own
/// characters, alts included, the FC set those comms and is already in them.
#[cfg(feature = "fleet")]
#[test]
fn uitest_join_comms_is_not_offered_on_your_own_fleet() {
    use egui_kittest::kittest::Queryable as _;
    for (boss, ours) in [("Placeholder Main", true), ("Placeholder Alt", true), ("Someone Else", false)] {
        harness::scratch_profile();
        let mut app: Option<crate::app::SpaiApp> = None;
        let mut scene = Scene::ui("join_comms_own", [1280.0, 1420.0], move |ui| {
            let a = app.get_or_insert_with(|| {
                let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
                a.settings.fleet_enabled = true;
                a.view = View::Fleet;
                fixtures::seed_fleet_state(&a);
                fixtures::open_first_fleet(&a);
                a.fleet_booted = true;
                {
                    let mut st = a.fleet_state_for_test().lock().unwrap();
                    let open = st.open.value.as_mut().unwrap();
                    open.fleet.commander = Some(crate::fleets::model::Labelled {
                        id: 1,
                        label: boss.to_owned(),
                    });
                    // Op 3 has a link, so the button is there to find when it is offered.
                    open.fleet.mumble_channel_id = Some(crate::fleets::model::ChannelId(3));
                    st.seed.mumble_channels = vec![crate::fleets::model::ChannelItem {
                        id: crate::fleets::model::ChannelId(3),
                        name: "Op 3".to_owned(),
                        is_in_use: true,
                    }];
                }
                a
            });
            a.root_chrome(ui);
            a.root_central(ui, None);
        });
        let mut h = harness::build(&mut scene, false);
        h.run();
        h.run();
        let offered = h.query_by_label_contains("Join comms").is_some();
        assert_eq!(offered, !ours, "{boss}: Join comms offered = {offered}");
        assert!(
            h.query_all_by_label_contains("Join Alpha command").count() > 0,
            "{boss}: the command button went too"
        );
    }
}

/// The off-doctrine clock counts on while no new snapshot arrives. It was measured against the
/// snapshot's own time, so every pilot first seen in it read "just seen" until the dashboard next
/// pushed, which on a quiet fleet is never.
#[cfg(feature = "fleet")]
#[test]
fn uitest_the_off_doctrine_clock_runs_between_snapshots() {
    use egui_kittest::kittest::Queryable as _;
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    let mut scene = Scene::ui("off_doctrine_clock", [1280.0, 1120.0], move |ui| {
        let a = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.settings.fleet_enabled = true;
            a.view = View::Fleet;
            a.fleet_detail_tab = crate::app::fleet_ui::DetailTab::Composition;
            fixtures::seed_fleet_state(&a);
            fixtures::open_first_fleet(&a);
            a.fleet_booted = true;
            // Nobody seen yet: every offender is first seen on the first frame.
            a.fleet_state_for_test().lock().unwrap().off_doctrine.clear();
            a
        });
        a.root_chrome(ui);
        a.root_central(ui, None);
    });
    let mut h = harness::build(&mut scene, false);
    h.run_steps(2);
    assert!(h.query_by_label_contains("Who is off doctrine").is_some(), "the fixture has nobody off doctrine");

    // No new snapshot in between, only time.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    h.run_steps(2);
    assert!(
        h.query_all_by_label("just seen").count() == 0,
        "the clock did not move without a new snapshot"
    );
}

/// Typing a name sends one search, for what was typed last, once the typing pauses. It sent one
/// per keystroke, each on its own thread.
#[cfg(feature = "fleet")]
#[test]
fn uitest_a_name_search_waits_for_the_typing_to_pause() {
    use crate::app::fleet_ui::FormAct;
    harness::scratch_profile();
    let ctx = egui::Context::default();
    let mut a = crate::app::SpaiApp::build(&ctx, true);
    a.settings.fleet_enabled = true;
    fixtures::seed_fleet_state(&a);
    for typed in ["amr", "amry", "amryu", "amryu a", "amryu al"] {
        a.fleet_apply_form_for_test(FormAct {
            search_character: Some(typed.to_owned()),
            ..Default::default()
        });
    }
    assert_eq!(a.fleet_search_pending_for_test().as_deref(), Some("amryu al"));
    assert!(a.fleet_state_for_test().lock().unwrap().found_characters.loading, "no looking shown");
    // Nothing goes out before the pause is up.
    a.fleet_search_poll();
    assert!(a.fleet_search_pending_for_test().is_some(), "sent before the typing paused");
    std::thread::sleep(std::time::Duration::from_millis(400));
    a.fleet_search_poll();
    assert!(a.fleet_search_pending_for_test().is_none(), "never sent once the typing paused");
}
