use super::fixtures::{self, IntelArgs};
use super::harness::{self, Scene};
use crate::nav::View;

fn intel_scene(name: &'static str, report: crate::intel::IntelReport, width: f32) -> Scene {
    let args = IntelArgs::default();
    Scene::ui(name, [width, 520.0], move |ui| {
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

fn ping_scene(name: &'static str, ping: crate::pings::Ping) -> Scene {
    let systems = Some(fixtures::systems());
    Scene::ui(name, [520.0, 320.0], move |ui| {
        crate::app::render_ping(ui, &ping, &systems, false, "", &Default::default());
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

fn nav_scene(name: &'static str, expanded: bool) -> Scene {
    let mut expanded = expanded;
    let width = if expanded { crate::nav::WIDTH_EXPANDED } else { crate::nav::WIDTH_COLLAPSED };
    Scene::ui(name, [width, 560.0], move |ui| {
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
        // The feed is resizable, so the same card has to survive a narrow dock too.
        intel_scene("intel_row_torture_narrow", fixtures::intel_torture(), 320.0),
        ping_scene("ping_fleet", fixtures::ping_fleet()),
        ping_scene("ping_plain", fixtures::ping_plain()),
        nav_scene("nav_rail_collapsed", false),
        nav_scene("nav_rail_expanded", true),
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
