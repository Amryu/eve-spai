//! The overlay child process (`eve-spai --overlay`).
//!
//! P1 scaffold: re-execs the same binary into a tiny, near-invisible egui window and
//! opens an IPC connection back to the main process. There is no overlay UI yet — that
//! arrives in later phases. The window is intentionally 1×1, undecorated, and transparent.

use std::time::Duration;

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
        Box::new(|_cc| Ok(Box::new(Overlay::new()))),
    )
}

/// The overlay app. For P1 it draws nothing and merely keeps the IPC thread alive.
struct Overlay {}

impl Overlay {
    fn new() -> Self {
        Self::spawn_ipc();
        Self {}
    }

    /// Connect back to the main process and pump messages on a background thread.
    /// Linux-only: the Unix-socket transport doesn't exist elsewhere, and the overlay is
    /// never spawned off Linux.
    #[cfg(target_os = "linux")]
    fn spawn_ipc() {
        std::thread::spawn(|| {
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
                    Ok(msg) => eprintln!("[overlay] msg: {msg:?}"),
                    Err(e) => {
                        eprintln!("[overlay] connection closed: {e}");
                        return;
                    }
                }
            }
        });
    }

    #[cfg(not(target_os = "linux"))]
    fn spawn_ipc() {}
}

impl eframe::App for Overlay {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // P1: draw nothing. Slow repaint keeps the event loop ticking without burning CPU.
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // fully transparent
    }
}
