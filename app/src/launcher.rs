//! The unread count on the desktop's own taskbar entry.
//!
//! [`crate::app::SpaiApp::sync_taskbar_badge`] draws the number onto the window icon, which is
//! enough on Windows. Plasma matches a window to its `.desktop` file and uses that file's `Icon=`,
//! ignoring the badged icon. Plasma, Dash-to-Dock and Latte do honour the Unity LauncherEntry API,
//! a session-bus signal naming a desktop file and a count. With no listener it costs one message.

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::Mutex;

    const PATH: &str = "/com/canonical/Unity/LauncherEntry";
    const IFACE: &str = "com.canonical.Unity.LauncherEntry";
    const MEMBER: &str = "Update";

    /// Which desktop entry the count belongs to. The launcher matches the running window to this
    /// file, so it has to be the one that was actually installed; a package that installs under
    /// another name can say so rather than silently badging nothing.
    fn app_uri() -> String {
        let id = std::env::var("EVE_SPAI_DESKTOP_ID")
            .unwrap_or_else(|_| "eve-spai.desktop".to_owned());
        format!("application://{id}")
    }

    /// The session bus, opened once. `None` once opening has failed: there is no bus on a headless
    /// box or in a container, and retrying every time the unread count moves would be a connection
    /// attempt per message for something nobody is listening to.
    fn conn() -> Option<&'static zbus::blocking::Connection> {
        static CONN: std::sync::OnceLock<Option<zbus::blocking::Connection>> =
            std::sync::OnceLock::new();
        CONN.get_or_init(|| match zbus::blocking::Connection::session() {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("[launcher] no session bus, taskbar count unavailable: {e}");
                None
            }
        })
        .as_ref()
    }

    pub fn set_count(count: u32) {
        static LAST: Mutex<Option<u32>> = Mutex::new(None);
        {
            let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
            if *last == Some(count) {
                return;
            }
            *last = Some(count);
        }
        let Some(conn) = conn() else { return };
        let mut props: std::collections::HashMap<&str, zbus::zvariant::Value> =
            std::collections::HashMap::new();
        props.insert("count", zbus::zvariant::Value::I64(i64::from(count)));
        // Zero is sent as "no badge" rather than as a zero: a launcher showing 0 unread is worse
        // than showing nothing, and hiding it is how the count gets cleared at all.
        props.insert("count-visible", zbus::zvariant::Value::Bool(count > 0));
        if let Err(e) = conn.emit_signal(None::<&str>, PATH, IFACE, MEMBER, &(app_uri(), props)) {
            eprintln!("[launcher] could not update the taskbar count: {e}");
        }
    }
}

/// Windows composes its own overlay from the window icon, which the icon badge already provides,
/// and macOS's dock badge needs AppKit rather than a bus. Neither is reachable from here.
#[cfg(not(target_os = "linux"))]
mod imp {
    pub fn set_count(_count: u32) {}
}

pub use imp::set_count;
