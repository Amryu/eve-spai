//! System-tray integration + autostart.
//!
//! Linux uses a StatusNotifierItem (via `ksni`) — the modern KDE/GNOME tray
//! protocol. The tray runs on its own thread and signals the UI through atomics;
//! the UI polls them each frame. Other platforms get no-op stubs for now.

/// Write or remove the OS autostart entry pointing at the current executable.
pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let path = autostart_path();
        if enabled {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let exe = std::env::current_exe()?;
            std::fs::write(
                &path,
                format!(
                    "[Desktop Entry]\nType=Application\nName=EVE Spai\n\
                     Comment=EVE Online intel tool\nExec={}\nX-GNOME-Autostart-enabled=true\n",
                    exe.display()
                ),
            )?;
        } else {
            let _ = std::fs::remove_file(autostart_path());
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = enabled; // TODO: Windows (Run key) / macOS (LaunchAgent)
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn autostart_path() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.config_dir().join("autostart").join("eve-spai.desktop"))
        .unwrap_or_else(|| std::path::PathBuf::from("eve-spai.desktop"))
}

#[cfg(target_os = "linux")]
pub use linux::{spawn, TrayCmd};
#[cfg(not(target_os = "linux"))]
pub use other::{spawn, TrayCmd};

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// Shared between the tray thread and the UI: which menu action was chosen.
    #[derive(Clone, Default)]
    pub struct TrayCmd {
        show: Arc<AtomicBool>,
        exit: Arc<AtomicBool>,
        attention: Arc<AtomicBool>,
    }

    impl TrayCmd {
        /// Consume a pending "Show" request (left-click or menu).
        pub fn take_show(&self) -> bool {
            self.show.swap(false, Ordering::SeqCst)
        }
        /// Whether "Exit" was chosen (latched — we're quitting).
        pub fn exit_requested(&self) -> bool {
            self.exit.load(Ordering::SeqCst)
        }
        /// Show/clear the unread badge on the tray icon.
        pub fn set_attention(&self, on: bool) {
            self.attention.store(on, Ordering::SeqCst);
        }
    }

    struct SpaiTray {
        cmd: TrayCmd,
        /// Wake the UI event loop so a menu action is acted on immediately, not on the
        /// next idle repaint (which is why tray Exit/Show felt laggy when minimised).
        ctx: egui::Context,
    }

    impl ksni::Tray for SpaiTray {
        fn id(&self) -> String {
            "eve-spai".into()
        }
        fn title(&self) -> String {
            "EVE Spai".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![icon(self.cmd.attention.load(Ordering::SeqCst))]
        }
        // Left-click the tray icon → show the window.
        fn activate(&mut self, _x: i32, _y: i32) {
            self.cmd.show.store(true, Ordering::SeqCst);
            self.ctx.request_repaint();
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: "Show EVE Spai".into(),
                    activate: Box::new(|t: &mut Self| {
                        t.cmd.show.store(true, Ordering::SeqCst);
                        t.ctx.request_repaint();
                    }),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Exit".into(),
                    activate: Box::new(|t: &mut Self| {
                        t.cmd.exit.store(true, Ordering::SeqCst);
                        t.ctx.request_repaint();
                    }),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    /// A simple round accent-blue tray icon (ARGB32, network byte order). With
    /// `badge`, a red dot is drawn in the top-right to signal unread messages.
    fn icon(badge: bool) -> ksni::Icon {
        let (w, h) = (24i32, 24i32);
        let mut data = vec![0u8; (w * h * 4) as usize];
        let put = |data: &mut [u8], x: i32, y: i32, argb: [u8; 4]| {
            let i = ((y * w + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&argb);
        };
        let (cx, cy, r) = (w as f32 / 2.0, h as f32 / 2.0, 10.0f32);
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if (dx * dx + dy * dy).sqrt() <= r {
                    put(&mut data, x, y, [0xFF, 0x4F, 0xC3, 0xF7]);
                }
            }
        }
        if badge {
            let (bx, by, br) = (17.0f32, 7.0f32, 5.0f32);
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 + 0.5 - bx, y as f32 + 0.5 - by);
                    if (dx * dx + dy * dy).sqrt() <= br {
                        put(&mut data, x, y, [0xFF, 0xE0, 0x4C, 0x4C]);
                    }
                }
            }
        }
        ksni::Icon { width: w, height: h, data }
    }

    /// Start the tray. Registration is done on a background thread so a slow/absent
    /// tray host never delays app startup. Returns the command channel immediately.
    pub fn spawn(ctx: egui::Context) -> Option<TrayCmd> {
        let cmd = TrayCmd::default();
        let cmd_for_thread = cmd.clone();
        std::thread::spawn(move || {
            use ksni::blocking::TrayMethods;
            let attention = cmd_for_thread.attention.clone();
            match (SpaiTray { cmd: cmd_for_thread, ctx }).spawn() {
                Ok(handle) => {
                    // Poll the unread flag and ask the host to re-fetch the icon when
                    // it changes (the handle must stay alive for the tray to live).
                    let mut last = false;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        let now = attention.load(Ordering::SeqCst);
                        if now != last {
                            last = now;
                            let _ = handle.update(|_t| {});
                        }
                    }
                }
                Err(e) => eprintln!("[tray] unavailable: {e}"),
            }
        });
        Some(cmd)
    }
}

#[cfg(not(target_os = "linux"))]
mod other {
    #[derive(Clone, Default)]
    pub struct TrayCmd;
    impl TrayCmd {
        pub fn take_show(&self) -> bool {
            false
        }
        pub fn exit_requested(&self) -> bool {
            false
        }
        pub fn set_attention(&self, _on: bool) {}
    }
    pub fn spawn(_ctx: egui::Context) -> Option<TrayCmd> {
        None
    }
}
