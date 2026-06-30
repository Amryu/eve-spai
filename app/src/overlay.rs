//! The overlay child process (`eve-spai --overlay`).
//!
//! P2: the FLEET-PING floating window lives here, OUT of the main process, so on Linux it is a
//! separate X11 client that KWin won't iconify together with the main window. The child re-execs
//! the same binary into a tiny 1×1 root window, connects back to the main over the IPC socket, and
//! declares the fleet-ping deferred viewport itself — rendering with the SAME closure the main uses
//! off Linux (`crate::app::build_ping_viewport_cb`). The main feeds it the current ping set + config
//! over IPC; the overlay opens its own read-only Store to resolve system names.

use std::time::Duration;

use crate::app::SharedPingWindow;

/// Entry point for the `--overlay` child. Reuses the main binary's eframe setup so the
/// renderer/backend choices stay identical to the parent.
pub fn run_overlay() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("EVE Spai overlay")
        .with_inner_size([1.0, 1.0])
        .with_taskbar(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(true);
    let opts = crate::base_native_options(viewport);
    eframe::run_native(
        "eve-spai-overlay",
        opts,
        Box::new(|cc| Ok(Box::new(Overlay::new(cc)))),
    )
}

/// The overlay app. Owns the fleet-ping shared state + render closure and declares the ping
/// deferred viewport every frame. The IPC reader thread feeds `ping_shared` from the main.
struct Overlay {
    ping_shared: SharedPingWindow,
    ping_viewport_cb: std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>,
    /// Throttle for the Smart-on-top EVE-focus probe.
    eve_focus_checked: Option<std::time::Instant>,
}

impl Overlay {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ping_shared: SharedPingWindow = std::sync::Arc::new(std::sync::Mutex::new(
            crate::app::PingWindowState::default(),
        ));
        // Same render closure the main uses off Linux, so the window looks identical.
        let ping_viewport_cb = crate::app::build_ping_viewport_cb(ping_shared.clone());
        let ctx = cc.egui_ctx.clone();
        Self::load_systems(ping_shared.clone(), ctx.clone());
        Self::spawn_ipc(ping_shared.clone(), ctx);
        Self { ping_shared, ping_viewport_cb, eve_focus_checked: None }
    }

    /// Open our OWN read-only Store and load the system graph (for `render_ping`'s system-name
    /// lookups). The SDE is static, so a second connection is safe; done on a thread so the window
    /// appears immediately. Jump bridges are NOT applied — they only affect route graphs, and the
    /// ping card needs system NAMES only.
    fn load_systems(ping_shared: SharedPingWindow, ctx: egui::Context) {
        std::thread::spawn(move || match crate::store::Store::open() {
            Ok(store) => {
                let systems = std::sync::Arc::new(store.load_systems());
                ping_shared.lock().unwrap().systems = Some(systems);
                ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
            }
            Err(e) => eprintln!("[overlay] store open failed: {e}"),
        });
    }

    /// Connect back to the main process and pump messages on a background thread. On `Ping`/`Config`
    /// it updates `ping_shared` and wakes the ping viewport. Linux-only: the Unix-socket transport
    /// doesn't exist elsewhere, and the overlay is never spawned off Linux.
    #[cfg(target_os = "linux")]
    fn spawn_ipc(ping_shared: SharedPingWindow, ctx: egui::Context) {
        std::thread::spawn(move || {
            let Some(mut stream) = crate::ipc::connect_retry() else {
                eprintln!("[overlay] could not connect to main socket; giving up");
                return;
            };
            eprintln!("[overlay] connected to main");
            if let Err(e) = crate::ipc::send(&mut stream, &crate::ipc::OverlayToMain::Hello) {
                eprintln!("[overlay] sending Hello failed: {e}");
                return;
            }
            loop {
                match crate::ipc::recv::<crate::ipc::MainToOverlay, _>(&mut stream) {
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
                    Ok(crate::ipc::MainToOverlay::Config(c)) => {
                        {
                            let mut st = ping_shared.lock().unwrap();
                            st.enabled = c.ping_enabled;
                            st.on_top = c.ping_on_top;
                        }
                        ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
                    }
                    Err(e) => {
                        eprintln!("[overlay] connection closed: {e}");
                        return;
                    }
                }
            }
        });
    }

    #[cfg(not(target_os = "linux"))]
    fn spawn_ipc(_ping_shared: SharedPingWindow, _ctx: egui::Context) {}
}

impl eframe::App for Overlay {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The 1×1 root draws nothing; the fleet-ping window is a separate deferred viewport.
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Publish the Smart-on-top EVE-focus flag into the shared state (throttled), like the main.
        let smart = {
            let st = self.ping_shared.lock().unwrap();
            st.on_top == crate::settings::OnTop::Smart
        };
        if smart {
            let due = self.eve_focus_checked.map(|t| t.elapsed().as_millis() > 800).unwrap_or(true);
            if due {
                let focused = crate::app::eve_is_focused();
                self.ping_shared.lock().unwrap().eve_focused = focused;
                self.eve_focus_checked = Some(std::time::Instant::now());
            }
        }
        // Seed level from current on-top; the closure re-asserts it live via ViewportCommand.
        let on_top = {
            let st = self.ping_shared.lock().unwrap();
            st.on_top != crate::settings::OnTop::Never
                && (st.on_top == crate::settings::OnTop::Always || st.eve_focused)
        };
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("fleet_ping_window"),
            crate::app::ping_viewport_builder(on_top),
            {
                let cb = self.ping_viewport_cb.clone();
                move |ui: &mut egui::Ui, class: egui::ViewportClass| cb(ui, class)
            },
        );
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // fully transparent
    }
}
