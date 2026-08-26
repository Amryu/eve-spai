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
            false,
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
        crate::nav::rail(ui, View::Intel, &mut expanded, &[View::Alerts], &[View::Jabber]);
    })
}

fn view_scene(name: &'static str, view: View, size: [f32; 2]) -> Scene {
    harness::scratch_profile();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            a.view = view;
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
    use crate::battle::{Battle, Engagement, Party, PartyKind};
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
    use crate::app::ChatWinKey;
    harness::scratch_profile();
    let (active, draft) = (active.to_owned(), draft.to_owned());
    let f = fixtures::jabber_frame();
    let mut app: Option<crate::app::SpaiApp> = None;
    Scene::ui(name, size, move |ui| {
        let app = app.get_or_insert_with(|| {
            let mut a = crate::app::SpaiApp::build(ui.ctx(), true);
            *a.jabber.lock().unwrap() = fixtures::jabber_state();
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
/// a painter-only `interact` rect with no AccessKit node (GAP-008).
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

pub(crate) fn all() -> Vec<Scene> {
    let mut v = vec![
        alert_window_scene("alert_window_typical", vec![fixtures::intel_typical()]),
        alert_window_scene(
            "alert_window_torture",
            vec![fixtures::intel_torture(), fixtures::intel_typical(), fixtures::intel_clear()],
        ),
        ping_window_scene("ping_window_fleet", vec![fixtures::ping_fleet()]),
        ping_window_scene(
            "ping_window_mixed",
            vec![fixtures::ping_fleet(), fixtures::ping_plain()],
        ),
        intel_scene("intel_row_typical", fixtures::intel_typical(), 520.0),
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
    // Both battle toolbars are one wrapping row of groups, so where they break moves with the
    // window. 720 breaks them into the most rows, which is where a divider is most likely to end
    // up at a row edge.
    v.push(view_scene("view_battles_narrow", View::Battles, [720.0, 800.0]));
    // UI-017: 1440 is a break point where the throttle picker used to land last on its row with
    // too little space left, and paint over the panel edge.
    v.push(view_scene("view_battles_wide", View::Battles, [1440.0, 800.0]));
    v.push(battle_detail_scene("view_battle_detail_narrow", [720.0, 800.0]));
    v.push(wormholes_rows_scene("view_wormholes_rows", [1280.0, 800.0]));
    // 720 is the app's minimum window width, where the eight-column table has the least room.
    v.push(wormholes_rows_scene("view_wormholes_rows_narrow", [720.0, 800.0]));
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
    v.push(jabber_tab_drag_scene("jabber_popout_tab_drag", [520.0, 480.0], [200.0, 150.0]));
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

/// UI-020: the pin used to be a floating `Area` over the central panel, which put it on top of the
/// tab bar's overflow caret. It now sits at the right end of the tab-bar row, so it shares that row
/// with the caret, follows it rather than covering it, and stays inside the window at both sizes.
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
    for w in (720..=1600).step_by(40).map(|w| w as f32) {
        let mut scene = view_scene("battles_divider_probe", View::Battles, [w, 800.0]);
        let harness = harness::build(&mut scene, false);
        let seps = crate::app::painted_toolbar_seps(&harness.ctx);
        assert!(!seps.is_empty(), "no toolbar divider painted at all at {w}px");
        let content = content_rects(&harness);
        for sep in &seps {
            let row = |r: &&egui::Rect| {
                let y = r.center().y;
                sep.top() - 2.0 < y && y < sep.bottom() + 2.0
            };
            assert!(
                content.iter().filter(row).any(|r| r.right() <= sep.left() + 0.5),
                "divider at {sep:?} starts a row at {w}px"
            );
            assert!(
                content.iter().filter(row).any(|r| r.left() >= sep.right() - 0.5),
                "divider at {sep:?} ends a row at {w}px"
            );
        }
    }
}

/// UI-017: a `ComboBox` used to claim only the row space left over instead of the width it paints,
/// which overflows the window wherever the toolbar happens to break just before one. The two
/// widths in the ticket sit between widths that both look fine, so sweep rather than spot-check.
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
        let got = crate::nav::rail(ui, *sink.borrow(), &mut expanded, &[], &[]);
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
        let got = crate::nav::rail(ui, *sink.borrow(), &mut expanded, &[], &[]);
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
/// what the vertical rhythm is made of.
fn ping_label_rect(harness: &egui_kittest::Harness<'_>, prefix: &str) -> Option<egui::Rect> {
    use egui_kittest::kittest::NodeT as _;

    for node in harness.root().children_recursive() {
        let n = node.accesskit_node();
        if n.role() != egui::accesskit::Role::Label {
            continue;
        }
        if !n.label().or_else(|| n.value()).unwrap_or_default().starts_with(prefix) {
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

/// FC, Formup and Doctrine are the same thing: one line of text. Doctrine used to sit in a
/// `horizontal_wrapped`, whose row is floored at `interact_size.y` whether or not anything
/// interactive is in it, so it stood 11px taller than its neighbours.
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

/// The Copy button was a `small_button`, which drops the `interact_size` floor and left a 17px
/// target next to the 27px Join Mumble in the same card.
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

/// The uncertain set is built from display-cased pilot names here, the shape that used to render
/// nothing at all. `UncertainPilots` normalizes on the way in, so the marker and the verdict click
/// both have to survive it.
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

/// The composer's text band, which is the height the galley *wants*. It lives inside a scroll
/// area, so past the ten-row cap this rect keeps growing while the visible band stops. The frame
/// margin is outside the field now, so this is the text alone.
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

/// With the Send button gone, Enter is the only way to send, so both halves of the return-key split
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

/// UI-023: a tab drag said nothing at the pointer, so the user could not tell one had begun. The
/// gesture itself is not driven here (GAP-008: the tab is a painter-only `interact` rect kittest
/// cannot grab), so the drag state is seeded and the painted output is what gets checked, against
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

    // An interactive ghost would sit on the history as a click target, which is exactly what UI-020
    // had to undo for the always-on-top pin. It is painted into a layer, so it owns no node at all.
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

/// UI-024: the `ScrollArea` used to wrap the whole `TextEdit`, frame included, so past the ten-row
/// cap the border scrolled with the text and the viewport clipped its top and bottom edges. The
/// border is painted outside the scrolling region now, so its clip rect has to hold all of it.
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

/// The border carries the focus ring now, since the field itself is drawn frameless. A focused
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
