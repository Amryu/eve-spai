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
