/// A launcher entry the installer offers (`.desktop` on Linux, a Start Menu `.lnk` on Windows).
/// `None` = the platform has no such concept (macOS ships as a bundle), so don't offer it.
pub fn menu_entry_exists() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        Some(linux_menu_path().is_file())
    }
    #[cfg(target_os = "windows")]
    {
        Some(windows_menu_path().is_some_and(|p| p.is_file()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

pub fn menu_entry_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "Start Menu entry"
    } else {
        "application menu entry"
    }
}

/// Create the launcher entry, pointing at the running binary. Mirrors what the install scripts do so
/// a user who skipped it there (or ran the binary directly) can add it from the setup wizard.
pub fn create_menu_entry() -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe()?;
        let path = linux_menu_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Some(base) = directories::BaseDirs::new() {
            let icondir = base.data_dir().join("icons/hicolor/256x256/apps");
            if std::fs::create_dir_all(&icondir).is_ok() {
                let _ = std::fs::write(
                    icondir.join("eve-spai.png"),
                    include_bytes!("../../assets/eve-spai.png"),
                );
            }
        }
        std::fs::write(
            &path,
            format!(
                "[Desktop Entry]\nType=Application\nName=EVE Spai\n\
                 Comment=EVE Online intel tool\nExec={}\nIcon=eve-spai\n\
                 Terminal=false\nCategories=Game;\n",
                exe.display()
            ),
        )?;
        if let Some(dir) = path.parent() {
            let _ = std::process::Command::new("update-desktop-database").arg(dir).status();
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let ico = dir.join("eve-spai.ico");
        let _ = std::fs::write(&ico, include_bytes!("../../assets/eve-spai.ico"));
        let lnk = windows_menu_path()
            .ok_or_else(|| std::io::Error::other("no Start Menu folder"))?;
        if let Some(parent) = lnk.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Building a .lnk needs the IShellLink COM interface; WScript.Shell wraps it, and shelling
        // out to it avoids a COM dependency for this one-shot action (same approach as install.ps1).
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}');\
             $s.TargetPath='{exe}';$s.WorkingDirectory='{dir}';\
             $s.Description='EVE Online intel tool';\
             if(Test-Path '{ico}'){{$s.IconLocation='{ico}'}};$s.Save()",
            lnk = lnk.display(),
            exe = exe.display(),
            dir = dir.display(),
            ico = ico.display(),
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("could not create the shortcut"))
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn linux_menu_path() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.data_dir().join("applications").join("eve-spai.desktop"))
        .unwrap_or_else(|| std::path::PathBuf::from("eve-spai.desktop"))
}

#[cfg(target_os = "windows")]
fn windows_menu_path() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new()
        .map(|b| b.config_dir().join(r"Microsoft\Windows\Start Menu\Programs\EVE Spai.lnk"))
}

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
    #[cfg(target_os = "windows")]
    {
        const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        const VALUE: &str = "EVE Spai";
        let status = if enabled {
            let exe = std::env::current_exe()?;
            std::process::Command::new("reg")
                .args(["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d"])
                .arg(format!("\"{}\"", exe.display()))
                .arg("/f")
                .status()
        } else {
            std::process::Command::new("reg")
                .args(["delete", RUN_KEY, "/v", VALUE, "/f"])
                .status()
        };
        let _ = status;
    }
    #[cfg(target_os = "macos")]
    {
        let path = macos_agent_path();
        if enabled {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let exe = std::env::current_exe()?;
            std::fs::write(
                &path,
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
                     \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\"><dict>\n\
                     \t<key>Label</key><string>com.evespai.app</string>\n\
                     \t<key>ProgramArguments</key><array><string>{}</string></array>\n\
                     \t<key>RunAtLoad</key><true/>\n\
                     </dict></plist>\n",
                    exe.display()
                ),
            )?;
        } else {
            let _ = std::fs::remove_file(macos_agent_path());
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = enabled;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn autostart_path() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.config_dir().join("autostart").join("eve-spai.desktop"))
        .unwrap_or_else(|| std::path::PathBuf::from("eve-spai.desktop"))
}

#[cfg(target_os = "macos")]
fn macos_agent_path() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join("Library/LaunchAgents/com.evespai.app.plist"))
        .unwrap_or_else(|| std::path::PathBuf::from("com.evespai.app.plist"))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn create_menu_entry_writes_a_desktop_file() {
        let tmp = std::env::temp_dir().join(format!("eve-spai-menu-test-{}", std::process::id()));
        let prev = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", &tmp);

        assert_eq!(menu_entry_exists(), Some(false), "no entry in a fresh data dir");
        create_menu_entry().unwrap();
        assert_eq!(menu_entry_exists(), Some(true), "entry present after create");

        let desktop = std::fs::read_to_string(linux_menu_path()).unwrap();
        assert!(desktop.contains("Name=EVE Spai"));
        assert!(desktop.contains("Exec="));
        assert!(tmp.join("icons/hicolor/256x256/apps/eve-spai.png").is_file());

        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(target_os = "linux")]
pub use linux::{spawn, TrayCmd};
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use desktop::{spawn, TrayCmd};
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub use other::{spawn, TrayCmd};

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Default)]
    pub struct TrayCmd {
        show: Arc<AtomicBool>,
        exit: Arc<AtomicBool>,
        attention: Arc<AtomicBool>,
    }

    impl TrayCmd {
        pub fn take_show(&self) -> bool {
            self.show.swap(false, Ordering::SeqCst)
        }
        pub fn exit_requested(&self) -> bool {
            self.exit.load(Ordering::SeqCst)
        }
        pub fn set_attention(&self, on: bool) {
            self.attention.store(on, Ordering::SeqCst);
        }
    }

    struct SpaiTray {
        cmd: TrayCmd,
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

    fn icon(badge: bool) -> ksni::Icon {
        use std::sync::OnceLock;
        static LOGO: OnceLock<Option<(i32, i32, Vec<u8>)>> = OnceLock::new();
        let logo = LOGO.get_or_init(|| {
            let img = image::load_from_memory(include_bytes!("../../assets/eve-spai.png"))
                .ok()?
                .to_rgba8();
            let (w, h) = img.dimensions();
            // RGBA → ARGB32 network byte order (bytes A,R,G,B per pixel).
            let mut data = Vec::with_capacity(img.as_raw().len());
            for px in img.as_raw().chunks_exact(4) {
                data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
            }
            Some((w as i32, h as i32, data))
        });
        let Some((w, h, base)) = logo else { return generated_icon(badge) };
        let (w, h) = (*w, *h);
        let mut data = base.clone();
        if badge {
            let r = (w.min(h) as f32) / 5.0;
            let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                    if dx * dx + dy * dy <= r * r {
                        let i = ((y * w + x) * 4) as usize;
                        data[i..i + 4].copy_from_slice(&[0xFF, 0xE0, 0x4C, 0x4C]);
                    }
                }
            }
        }
        ksni::Icon { width: w, height: h, data }
    }

    fn generated_icon(badge: bool) -> ksni::Icon {
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

// Windows + macOS: a tray via the `tray-icon` crate. The StatusItem/Shell_NotifyIcon handle must
// be created and live on the main (event-loop) thread, so `spawn` runs from `SpaiApp::new` (main
// thread) and the icon is parked in a thread-local to stay alive. Menu clicks arrive through a
// global event handler that flips the same atomics the Linux path uses and wakes the UI.
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod desktop {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{TrayIcon, TrayIconBuilder};

    #[derive(Clone, Default)]
    pub struct TrayCmd {
        show: Arc<AtomicBool>,
        exit: Arc<AtomicBool>,
        attention: Arc<AtomicBool>,
    }

    impl TrayCmd {
        pub fn take_show(&self) -> bool {
            self.show.swap(false, Ordering::SeqCst)
        }
        pub fn exit_requested(&self) -> bool {
            self.exit.load(Ordering::SeqCst)
        }
        pub fn set_attention(&self, on: bool) {
            if self.attention.swap(on, Ordering::SeqCst) != on {
                TRAY.with(|t| {
                    if let (Some(tray), Some(icon)) = (t.borrow().as_ref(), make_icon(on)) {
                        let _ = tray.set_icon(Some(icon));
                    }
                });
            }
        }
    }

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    }

    fn make_icon(badge: bool) -> Option<tray_icon::Icon> {
        let img = image::load_from_memory(include_bytes!("../../assets/eve-spai.png")).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut rgba = img.into_raw();
        if badge {
            let r = (w.min(h) as f32) / 5.0;
            let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                    if dx * dx + dy * dy <= r * r {
                        let i = ((y * w + x) * 4) as usize;
                        rgba[i..i + 4].copy_from_slice(&[0xE0, 0x4C, 0x4C, 0xFF]);
                    }
                }
            }
        }
        tray_icon::Icon::from_rgba(rgba, w, h).ok()
    }

    pub fn spawn(ctx: egui::Context) -> Option<TrayCmd> {
        let cmd = TrayCmd::default();
        let menu = Menu::new();
        let show = MenuItem::new("Show EVE Spai", true, None);
        let exit = MenuItem::new("Exit", true, None);
        menu.append(&show).ok()?;
        menu.append(&exit).ok()?;
        let show_id = show.id().clone();
        let exit_id = exit.id().clone();

        let mut builder = TrayIconBuilder::new().with_tooltip("EVE Spai").with_menu(Box::new(menu));
        if let Some(icon) = make_icon(false) {
            builder = builder.with_icon(icon);
        }
        let tray = match builder.build() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[tray] unavailable: {e}");
                return None;
            }
        };
        TRAY.with(|t| *t.borrow_mut() = Some(tray));

        let show_flag = cmd.show.clone();
        let exit_flag = cmd.exit.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == show_id {
                show_flag.store(true, Ordering::SeqCst);
            } else if event.id == exit_id {
                exit_flag.store(true, Ordering::SeqCst);
            }
            ctx.request_repaint();
        }));
        Some(cmd)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
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
