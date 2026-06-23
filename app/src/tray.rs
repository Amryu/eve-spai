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
    }

    struct SpaiTray {
        cmd: TrayCmd,
    }

    impl ksni::Tray for SpaiTray {
        fn id(&self) -> String {
            "eve-spai".into()
        }
        fn title(&self) -> String {
            "EVE Spai".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![icon()]
        }
        // Left-click the tray icon → show the window.
        fn activate(&mut self, _x: i32, _y: i32) {
            self.cmd.show.store(true, Ordering::SeqCst);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: "Show EVE Spai".into(),
                    activate: Box::new(|t: &mut Self| t.cmd.show.store(true, Ordering::SeqCst)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Exit".into(),
                    activate: Box::new(|t: &mut Self| t.cmd.exit.store(true, Ordering::SeqCst)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    /// A simple round accent-blue tray icon (ARGB32, network byte order).
    fn icon() -> ksni::Icon {
        let (w, h) = (24i32, 24i32);
        let mut data = vec![0u8; (w * h * 4) as usize];
        let (cx, cy, r) = (w as f32 / 2.0, h as f32 / 2.0, 10.0f32);
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if (dx * dx + dy * dy).sqrt() <= r {
                    let i = ((y * w + x) * 4) as usize;
                    data[i] = 0xFF; // A
                    data[i + 1] = 0x4F; // R
                    data[i + 2] = 0xC3; // G
                    data[i + 3] = 0xF7; // B
                }
            }
        }
        ksni::Icon { width: w, height: h, data }
    }

    /// Start the tray (runs on ksni's own background thread). Returns the command
    /// channel, or None if no tray host is available.
    pub fn spawn() -> Option<TrayCmd> {
        use ksni::blocking::TrayMethods;
        let cmd = TrayCmd::default();
        match (SpaiTray { cmd: cmd.clone() }).spawn() {
            Ok(handle) => {
                // The tray lives for the whole process; keep the handle alive.
                std::mem::forget(handle);
                Some(cmd)
            }
            Err(e) => {
                eprintln!("[tray] unavailable: {e}");
                None
            }
        }
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
    }
    pub fn spawn() -> Option<TrayCmd> {
        None
    }
}
