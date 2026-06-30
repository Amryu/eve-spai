//! The overlay child process (`eve-spai --overlay`).
//!
//! The FLEET-PING (P2) and INTEL-ALERT (P3) floating windows live here, OUT of the main process, so
//! on Linux each is a separate X11 client that KWin won't iconify together with the main window. The
//! child re-execs the same binary into a tiny 1×1 root window, connects back to the main over the
//! IPC socket, and declares the ping + alert deferred viewports itself — rendering with the SAME
//! closures the main uses off Linux (`crate::app::build_ping_viewport_cb` /
//! `build_alert_viewport_cb`). The main feeds it the current ping/alert state + config over IPC; the
//! overlay opens its own read-only Store to resolve system names + ship details, and holds its own
//! kill/affiliation caches (it has no fetchers — the main pre-resolves and pushes those entries).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::{SharedAlertWindow, SharedPingWindow};

/// Entry point for the `--overlay` child. Reuses the main binary's eframe setup so the
/// renderer/backend choices stay identical to the parent.
pub fn run_overlay() -> eframe::Result<()> {
    #[allow(unused_mut)] // `viewport` is only reassigned in the Linux-only block below
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("EVE Spai overlay")
        .with_inner_size([1.0, 1.0])
        // Keep the 1×1 root off the taskbar. It must stay mapped (a hidden window never paints, so
        // its deferred ping/alert child viewports would never run), so we can't hide it — 1×1 +
        // taskbar-off keeps it out of sight. NOTE: on macOS hiding the Dock tile would need an
        // `LSUIElement`/`NSApplicationActivationPolicyAccessory` bundle flag — out of scope here.
        .with_taskbar(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(true);
    // On X11 make the 1×1 context-host root OVERRIDE-REDIRECT: winit has no skip-taskbar on X11, and a
    // Utility type still leaves it in KWin's task switcher as a stray empty entry. Override-redirect
    // takes it out of the WM entirely (no taskbar / switcher / stack). The root has no content and
    // never needs focus, and the alert/ping/dscan child viewports are separate managed windows, so
    // this only hides the host. (Utility kept as a fallback hint.)
    #[cfg(target_os = "linux")]
    {
        viewport = viewport
            .with_window_type(egui::X11WindowType::Utility)
            .with_override_redirect(true);
    }
    let opts = crate::base_native_options(viewport);
    eframe::run_native(
        "eve-spai-overlay",
        opts,
        Box::new(|cc| Ok(Box::new(Overlay::new(cc)))),
    )
}

/// The overlay app. Owns the fleet-ping + alert shared state + render closures and declares both
/// deferred viewports every frame. The IPC reader thread feeds them from the main.
struct Overlay {
    ping_shared: SharedPingWindow,
    ping_viewport_cb: Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    alert_shared: SharedAlertWindow,
    alert_viewport_cb: Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    /// The alert window's raw on-top setting (resolved against live EVE focus each frame). Written
    /// by the IPC `Config` handler, read in `update`.
    alert_on_top: Arc<Mutex<crate::settings::OnTop>>,
    /// Last alert geometry forwarded to the main, so we only send `AlertMoved` on an actual change
    /// (the render closure republishes the current position every frame). Clicks/geometry are
    /// written straight to our stdout (the overlay→main IPC pipe).
    alert_pos_sent: Option<(f32, f32)>,
    alert_size_sent: Option<(f32, f32)>,
    /// Last probed EVE-focus state (Smart on-top), refreshed on a throttle.
    eve_focused: bool,
    eve_focus_checked: Option<std::time::Instant>,
    /// A d-scan link clicked in an alert card, hosted HERE in the overlay (its own X11 client, on
    /// top of the game) instead of the main — set by the click drain, rendered each frame.
    dscan_view: Option<crate::app::DscanView>,
    /// The overlay's own SDE hull-name index (lower-cased name → (type id, name)), used to resolve
    /// a fetched d-scan's hulls. Loaded on a thread (mirrors the main's `self.ship_index`).
    ship_index: Arc<Mutex<Option<Arc<HashMap<String, (i64, String)>>>>>,
}

impl Overlay {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // The overlay renders the same cards as the main, so its egui context needs the same
        // setup — otherwise icons render as tofu squares and ship images as red error triangles.
        crate::theme::install_fonts(&cc.egui_ctx);
        crate::image_cache::install_image_loaders_cached(&cc.egui_ctx);
        // Match the user's theme (best-effort; default if settings can't be read).
        let theme = crate::store::Store::open()
            .ok()
            .and_then(|s| s.load_settings())
            .map(|s| s.theme)
            .unwrap_or_default();
        theme.apply(&cc.egui_ctx);

        let ping_shared: SharedPingWindow =
            Arc::new(Mutex::new(crate::app::PingWindowState::default()));
        let alert_shared: SharedAlertWindow =
            Arc::new(Mutex::new(crate::app::AlertWindowState::default()));
        // Same render closures the main uses off Linux, so the windows look identical.
        let ping_viewport_cb = crate::app::build_ping_viewport_cb(ping_shared.clone());
        let alert_viewport_cb = crate::app::build_alert_viewport_cb(alert_shared.clone());
        let alert_on_top = Arc::new(Mutex::new(crate::settings::OnTop::default()));
        // The overlay has no fetchers; it serves clicks/tooltips from caches the main pre-fills.
        let kills: crate::kills::KillCache = Arc::new(Mutex::new(HashMap::new()));
        let affil: crate::affiliation::SharedAffil =
            Arc::new(Mutex::new(crate::affiliation::AffilCache::default()));

        let ctx = cc.egui_ctx.clone();
        let ship_index: Arc<Mutex<Option<Arc<HashMap<String, (i64, String)>>>>> =
            Arc::new(Mutex::new(None));
        Self::load_systems(ping_shared.clone(), alert_shared.clone(), ctx.clone());
        Self::load_ship_index(ship_index.clone());
        Self::spawn_ipc(IpcArgs {
            ping_shared: ping_shared.clone(),
            alert_shared: alert_shared.clone(),
            alert_on_top: alert_on_top.clone(),
            kills,
            affil,
            ctx,
        });
        Self {
            ping_shared,
            ping_viewport_cb,
            alert_shared,
            alert_viewport_cb,
            alert_on_top,
            alert_pos_sent: None,
            alert_size_sent: None,
            eve_focused: true,
            eve_focus_checked: None,
            dscan_view: None,
            ship_index,
        }
    }

    /// Open our OWN read-only Store and build the SDE hull-name index, so a d-scan clicked in an
    /// alert card can be fetched + tallied here in the overlay. Done on a thread (the SDE is static,
    /// so a second connection is safe); only read on a click, so no repaint wake is needed.
    fn load_ship_index(slot: Arc<Mutex<Option<Arc<HashMap<String, (i64, String)>>>>>) {
        std::thread::spawn(move || match crate::store::Store::open() {
            Ok(store) => *slot.lock().unwrap() = Some(Arc::new(store.ship_index())),
            Err(e) => eprintln!("[overlay] ship index load failed: {e}"),
        });
    }

    /// Open our OWN read-only Store and load the system graph (for ping/alert system-name + jumps
    /// lookups), publishing it into both shared states. The SDE is static, so a second connection is
    /// safe; done on a thread so the windows appear immediately. Jump bridges are NOT applied — they
    /// only affect route graphs, and the cards need system NAMES (and straight-line jumps) only.
    fn load_systems(ping_shared: SharedPingWindow, alert_shared: SharedAlertWindow, ctx: egui::Context) {
        std::thread::spawn(move || match crate::store::Store::open() {
            Ok(store) => {
                let systems = Arc::new(store.load_systems());
                ping_shared.lock().unwrap().systems = Some(systems.clone());
                alert_shared.lock().unwrap().systems = Some(systems);
                ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
                ctx.request_repaint_of(egui::ViewportId::from_hash_of("alert_window"));
            }
            Err(e) => eprintln!("[overlay] store open failed: {e}"),
        });
    }

    /// Read main→overlay frames from our stdin and pump them on a background thread. On
    /// `Ping`/`Alert`/`Config` it updates the shared state and wakes the relevant viewport. The
    /// transport is the child's piped stdio (cross-platform): stdin carries main→overlay, stdout
    /// carries overlay→main (see `update`). On stdin EOF / any read error the overlay exits — this
    /// is the "die with the main" behaviour (the main's send pipe closes when it goes away).
    fn spawn_ipc(args: IpcArgs) {
        let IpcArgs { ping_shared, alert_shared, alert_on_top, kills, affil, ctx } = args;
        std::thread::spawn(move || {
            // Announce readiness on stdout so the main does its initial Config+Ping resend.
            {
                let mut out = std::io::stdout().lock();
                if let Err(e) = crate::ipc::send(&mut out, &crate::ipc::OverlayToMain::Hello) {
                    eprintln!("[overlay] sending Hello failed: {e}");
                    std::process::exit(0);
                }
            }
            eprintln!("[overlay] connected to main (stdio)");
            // Our own SDE handle for ship details/roles (the main doesn't send those).
            let ship_lookup = match crate::store::Store::open() {
                Ok(store) => Some(crate::app::ShipLookup::new(store)),
                Err(e) => {
                    eprintln!("[overlay] ship-detail store open failed: {e}");
                    None
                }
            };
            let mut rd = std::io::BufReader::new(std::io::stdin().lock());
            loop {
                match crate::ipc::recv::<crate::ipc::MainToOverlay, _>(&mut rd) {
                    Ok(crate::ipc::MainToOverlay::Shutdown) => {
                        eprintln!("[overlay] Shutdown received; exiting");
                        std::process::exit(0);
                    }
                    Ok(crate::ipc::MainToOverlay::Ping(m)) => {
                        {
                            let mut st = ping_shared.lock().unwrap();
                            st.windows = m
                                .pings
                                .into_iter()
                                .map(|ping| crate::app::PingShown {
                                    ping,
                                    shown_at: std::time::Instant::now(),
                                })
                                .collect();
                            if m.raise {
                                st.raise = true;
                            }
                            st.doctrine_url = m.doctrine_url;
                            st.op_links = m.op_links;
                        }
                        ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
                    }
                    Ok(crate::ipc::MainToOverlay::Alert(m)) => {
                        // Merge the pushed kill/affil subsets into the overlay's own caches; the
                        // render closure reads them exactly as the main's `intel_row` does.
                        {
                            let mut kc = kills.lock().unwrap();
                            for (kid, info) in &m.kills {
                                kc.insert(*kid, Some(info.clone()));
                            }
                        }
                        {
                            let mut ac = affil.lock().unwrap();
                            for (cid, a) in &m.affil {
                                ac.insert_resolved(*cid, a.clone());
                            }
                        }
                        // Build ship details/roles for the feed's ships from our own SDE.
                        let ship_ids: HashSet<i64> =
                            m.feed.iter().flat_map(|(r, _)| r.ships.iter().map(|s| s.id)).collect();
                        let (ship_details, ship_roles) = match &ship_lookup {
                            Some(sl) => (
                                ship_ids
                                    .iter()
                                    .filter_map(|&i| sl.details(i).map(|d| (i, d)))
                                    .collect::<HashMap<_, _>>(),
                                ship_ids
                                    .iter()
                                    .map(|&i| (i, sl.roles(i)))
                                    .collect::<HashMap<_, _>>(),
                            ),
                            None => (HashMap::new(), HashMap::new()),
                        };
                        {
                            let mut st = alert_shared.lock().unwrap();
                            st.feed = m.feed;
                            st.from_you = m.from_you;
                            st.status = m.status;
                            st.resolved_pilots = m.resolved_pilots;
                            st.last_ship = m.last_ship;
                            st.ship_details = ship_details;
                            st.ship_roles = ship_roles;
                            st.kills = Some(kills.clone());
                            st.affil = Some(affil.clone());
                            // Countdown directive: reset to a finite value, reset to ∞, or (the
                            // negative refresh sentinel) leave the overlay's own countdown running.
                            if m.secs >= 0.0 {
                                st.secs = m.secs;
                            } else if m.secs <= crate::app::ALERT_SECS_INFINITE + 0.5 {
                                st.secs = f32::INFINITY;
                            }
                            if m.focus {
                                st.focus_pending = true;
                            }
                        }
                        ctx.request_repaint_of(egui::ViewportId::from_hash_of("alert_window"));
                    }
                    Ok(crate::ipc::MainToOverlay::Config(c)) => {
                        {
                            let mut st = ping_shared.lock().unwrap();
                            st.enabled = c.ping_enabled;
                            st.on_top = c.ping_on_top;
                        }
                        {
                            let mut st = alert_shared.lock().unwrap();
                            st.enabled = c.alert_enabled;
                            st.win_pos = c.win_pos;
                            st.win_size = c.win_size;
                        }
                        *alert_on_top.lock().unwrap() = c.alert_on_top;
                        ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
                        ctx.request_repaint_of(egui::ViewportId::from_hash_of("alert_window"));
                    }
                    Err(e) => {
                        // stdin EOF (the main exited) or a malformed frame: die with the main.
                        eprintln!("[overlay] stdin closed ({e}); exiting");
                        std::process::exit(0);
                    }
                }
            }
        });
    }
}

/// Bundled arguments for [`Overlay::spawn_ipc`] (keeps the call site readable as the set grew).
struct IpcArgs {
    ping_shared: SharedPingWindow,
    alert_shared: SharedAlertWindow,
    alert_on_top: Arc<Mutex<crate::settings::OnTop>>,
    kills: crate::kills::KillCache,
    affil: crate::affiliation::SharedAffil,
    ctx: egui::Context,
}

impl eframe::App for Overlay {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The 1×1 root draws nothing; the ping + alert windows are separate deferred viewports.
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refresh the EVE-focus flag (throttled) when either window uses Smart on-top.
        let ping_smart = self.ping_shared.lock().unwrap().on_top == crate::settings::OnTop::Smart;
        let alert_smart = *self.alert_on_top.lock().unwrap() == crate::settings::OnTop::Smart;
        if ping_smart || alert_smart {
            let due = self.eve_focus_checked.map(|t| t.elapsed().as_millis() > 800).unwrap_or(true);
            if due {
                self.eve_focused = crate::app::eve_is_focused();
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        // Publish focus into the ping shared state (its closure reads it for Smart on-top).
        self.ping_shared.lock().unwrap().eve_focused = self.eve_focused;

        // Forward alert-window outputs (feed clicks + geometry) back to the main over our stdout
        // (the overlay→main IPC pipe), which acts on them with `&mut self` in its root `update`. The
        // closure republishes the current position every frame, so dedup geometry against the last
        // forwarded value (pos exact, size >2px) to avoid flooding the pipe — matching the main's
        // own persist thresholds. Lock stdout once so each frame is written atomically.
        {
            let (clicks, moved, moved_size) = {
                let mut st = self.alert_shared.lock().unwrap();
                (std::mem::take(&mut st.clicks), st.moved.take(), st.moved_size.take())
            };
            let pos = moved.filter(|p| Some(*p) != self.alert_pos_sent);
            let size = moved_size.filter(|s| {
                self.alert_size_sent.map_or(true, |(w, h)| (w - s.0).abs() > 2.0 || (h - s.1).abs() > 2.0)
            });
            // Split the alert-card clicks: a `Dscan` link is handled LOCALLY (the d-scan dialog lives
            // in the overlay, on top of the game); System/Ship/Pilot still go to the main, whose
            // windows live there. Each click points the user at exactly one dialog, so a later Dscan
            // wins the slot.
            let mut to_main: Vec<crate::app::IntelClick> = Vec::new();
            for c in clicks {
                match c {
                    crate::app::IntelClick::Dscan(url) => {
                        let idx = self.ship_index.lock().unwrap().clone();
                        if let Some(view) = crate::app::open_dscan_view(url, idx, ctx) {
                            self.dscan_view = Some(view);
                            ctx.request_repaint();
                        }
                    }
                    other => to_main.push(other),
                }
            }
            if !to_main.is_empty() || pos.is_some() || size.is_some() {
                let mut out = std::io::stdout().lock();
                for c in to_main {
                    let _ = crate::ipc::send(&mut out, &crate::ipc::OverlayToMain::Click(c));
                }
                if pos.is_some() || size.is_some() {
                    let _ = crate::ipc::send(
                        &mut out,
                        &crate::ipc::OverlayToMain::AlertMoved { pos, size },
                    );
                    if pos.is_some() {
                        self.alert_pos_sent = pos;
                    }
                    if size.is_some() {
                        self.alert_size_sent = size;
                    }
                }
            }
        }

        // Fleet-ping viewport. Seed level from current on-top; the closure re-asserts it live.
        let ping_on_top = {
            let st = self.ping_shared.lock().unwrap();
            st.on_top != crate::settings::OnTop::Never
                && (st.on_top == crate::settings::OnTop::Always || st.eve_focused)
        };
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("fleet_ping_window"),
            crate::app::ping_viewport_builder(ping_on_top),
            {
                let cb = self.ping_viewport_cb.clone();
                move |ui: &mut egui::Ui, class: egui::ViewportClass| cb(ui, class)
            },
        );

        // Alert viewport. Resolve on-top (Smart depends on live EVE focus) + publish it, then
        // declare the viewport. `enabled`/geometry come from the IPC `Config` handler.
        let alert_on_top = {
            let ot = *self.alert_on_top.lock().unwrap();
            ot != crate::settings::OnTop::Never
                && (ot == crate::settings::OnTop::Always || self.eve_focused)
        };
        let alert_active = {
            let mut st = self.alert_shared.lock().unwrap();
            st.on_top_level = alert_on_top;
            st.enabled && (st.secs > 0.0 || st.pinned)
        };
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("alert_window"),
            crate::app::alert_viewport_builder(alert_on_top, alert_active),
            {
                let cb = self.alert_viewport_cb.clone();
                move |ui: &mut egui::Ui, class: egui::ViewportClass| cb(ui, class)
            },
        );

        // D-scan dialog (only while a scan is being viewed). Hosted here as its own immediate
        // viewport, on top of the game (taskbar-off). A hull click hops to the main's ship window
        // via `OverlayToMain::Click(Ship)` — the ship card lives in the main.
        if self.dscan_view.is_some() {
            let mut open_ship: Option<i64> = None;
            crate::app::dscan_view_dialog_ui(ctx, &mut self.dscan_view, true, &mut open_ship);
            if let Some(id) = open_ship {
                let mut out = std::io::stdout().lock();
                let _ = crate::ipc::send(
                    &mut out,
                    &crate::ipc::OverlayToMain::Click(crate::app::IntelClick::Ship(id)),
                );
            }
        }

        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // fully transparent
    }
}
