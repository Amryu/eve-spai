//! Overlay IPC: protocol + transport between the main process and the spawned
//! `--overlay` child (Linux only).
//!
//! P1 scaffold: length-prefixed JSON framing over a Unix-domain socket, plus the
//! main-side lifecycle (`OverlayLink`) that spawns/monitors/kills the child. The
//! message set is defined in full but only the lifecycle variants are wired up;
//! later phases replace the `serde_json::Value` placeholders with typed payloads.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixListener, UnixStream};

/// Path of the overlay control socket. Lives in the per-OS data dir; falls back
/// to `/tmp` if that can't be resolved. Never panics.
pub fn socket_path() -> PathBuf {
    match crate::store::data_dir() {
        Ok(dir) => dir.join("overlay.sock"),
        Err(_) => PathBuf::from("/tmp").join("eve-spai-overlay.sock"),
    }
}

/// Messages the overlay child sends back to the main process.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub enum OverlayToMain {
    /// First message after connecting — proves the child is alive and talking.
    Hello,
    /// A click in the alert window's feed — the main opens the relevant window in its own viewport.
    Click(crate::app::IntelClick),
    /// The alert window was moved/resized — the main persists the geometry into settings.
    AlertMoved { pos: Option<(f32, f32)>, size: Option<(f32, f32)> },
}

/// The current fleet-ping set + render context, pushed to the overlay so it can render the
/// fleet-ping window in its own process. Mirrors the fields the main keeps in `PingWindowState`.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct PingMsg {
    /// Pings to show, newest first.
    pub pings: Vec<crate::pings::Ping>,
    /// Foreground the window once (a new ping just arrived).
    pub raise: bool,
    pub doctrine_url: String,
    pub op_links: std::collections::HashMap<String, String>,
}

/// The current intel-alert feed + render context, pushed to the overlay so it can render the alert
/// window in its own process. `kills`/`affil` carry only the entries the feed's entities reference
/// (pre-resolved by the main, keyed by kill id / character id); the overlay derives ship details +
/// roles from its own SDE. `secs >= 0` resets the overlay's countdown (a fresh alert fired); `secs <
/// 0` means "leave the countdown alone, this is a content refresh".
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct AlertMsg {
    pub feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub status: std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    pub resolved_pilots: std::collections::HashMap<String, i64>,
    pub last_ship: std::collections::HashMap<String, (i64, String, i64)>,
    /// kill id → enriched killmail info (only the feed's killmail links).
    pub kills: std::collections::HashMap<i64, crate::kills::KillInfo>,
    /// character id → resolved corp/alliance (pilots + killmail victim/final-blow chars).
    pub affil: std::collections::HashMap<i64, crate::affiliation::Affil>,
    /// Countdown seconds to (re)start, or a negative sentinel for a content-only refresh.
    pub secs: f32,
    /// A fresh alert just fired — bring the window forward once (Windows only on the overlay side).
    pub focus: bool,
}

/// Overlay feature/behaviour config, pushed whenever the relevant settings change. Carries the
/// fleet-ping fields plus the alert window's feature/on-top/timeout/geometry.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct OverlayConfig {
    pub ping_enabled: bool,
    pub ping_on_top: crate::settings::OnTop,
    pub alert_enabled: bool,
    pub alert_on_top: crate::settings::OnTop,
    pub window_timeout: f32,
    pub win_pos: Option<(f32, f32)>,
    pub win_size: Option<(f32, f32)>,
}

/// Messages the main process sends to the overlay child.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub enum MainToOverlay {
    Ping(PingMsg),
    Alert(AlertMsg),
    Config(OverlayConfig),
    /// Ask the overlay to exit its process cleanly.
    Shutdown,
}

/// Write a length-prefixed JSON frame: `u32` big-endian byte length + body, then flush.
pub fn send<T: Serialize, W: Write>(w: &mut W, msg: &T) -> io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

/// Read one length-prefixed JSON frame written by [`send`].
pub fn recv<T: DeserializeOwned, R: Read>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Bind the overlay control socket, clearing any stale file from a prior run first.
#[cfg(target_os = "linux")]
pub fn host() -> io::Result<UnixListener> {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&path);
    UnixListener::bind(&path)
}

/// Connect to the main process's control socket, retrying briefly while it binds.
#[cfg(target_os = "linux")]
pub fn connect_retry() -> Option<UnixStream> {
    let path = socket_path();
    for _ in 0..50 {
        if let Ok(stream) = UnixStream::connect(&path) {
            return Some(stream);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// Main-side handle that owns the overlay child process and its IPC connection.
///
/// Lifecycle: [`start`](OverlayLink::start) binds the socket and spawns the child;
/// an accept-thread stores the connected stream for later sends. [`poll`](OverlayLink::poll)
/// is called periodically to respawn a crashed child (debounced + capped).
/// [`shutdown`](OverlayLink::shutdown) tells the child to exit and reaps it.
#[cfg(target_os = "linux")]
pub struct OverlayLink {
    child: std::process::Child,
    listener: UnixListener,
    /// The accepted overlay connection, populated by the accept-thread once the child connects.
    conn: std::sync::Arc<std::sync::Mutex<Option<UnixStream>>>,
    /// Messages the overlay sent back (Click / AlertMoved), pushed by the per-connection reader
    /// thread and drained by the main in `ui()` (acting on them needs `&mut self` + the ctx).
    inbox: std::sync::Arc<std::sync::Mutex<Vec<OverlayToMain>>>,
    /// Set by the accept-thread each time a (re)connection is accepted, so the main can force a
    /// fresh full resend of Config+Ping to a freshly-spawned overlay. Consumed via `take_reconnected`.
    reconnected: std::sync::Arc<std::sync::atomic::AtomicBool>,
    last_spawn: std::time::Instant,
    restarts: u32,
    gave_up: bool,
}

#[cfg(target_os = "linux")]
impl OverlayLink {
    const MAX_RESTARTS: u32 = 5;
    const RESPAWN_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(2);

    /// Bind the control socket and spawn the overlay child, returning the link.
    pub fn start() -> io::Result<Self> {
        let listener = host()?;
        let conn = std::sync::Arc::new(std::sync::Mutex::new(None));
        let reconnected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let inbox = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        Self::spawn_accept(&listener, conn.clone(), reconnected.clone(), inbox.clone());
        let child = Self::spawn_child()?;
        Ok(Self {
            child,
            listener,
            conn,
            inbox,
            reconnected,
            last_spawn: std::time::Instant::now(),
            restarts: 0,
            gave_up: false,
        })
    }

    /// True (once) after the overlay connects or reconnects. The main resets its change-detection
    /// version on a `true` so a respawned overlay is repopulated with the current Config+Ping.
    pub fn take_reconnected(&self) -> bool {
        self.reconnected.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Take the overlay→main messages received since the last call (Click / AlertMoved).
    pub fn drain_inbox(&self) -> Vec<OverlayToMain> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    fn spawn_child() -> io::Result<std::process::Child> {
        let exe = std::env::current_exe()?;
        // Inherit stdio so the child's `[overlay] …` eprintln lines surface in our logs.
        std::process::Command::new(exe).arg("--overlay").spawn()
    }

    /// Background thread: accept the overlay's connection, read its `Hello`, and stash the
    /// stream for later sends. Re-arms after each connection so a respawned child reconnects.
    fn spawn_accept(
        listener: &UnixListener,
        conn: std::sync::Arc<std::sync::Mutex<Option<UnixStream>>>,
        reconnected: std::sync::Arc<std::sync::atomic::AtomicBool>,
        inbox: std::sync::Arc<std::sync::Mutex<Vec<OverlayToMain>>>,
    ) {
        let listener = match listener.try_clone() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[main] overlay accept-thread clone failed: {e}");
                return;
            }
        };
        std::thread::spawn(move || loop {
            match listener.accept() {
                Ok((mut stream, _addr)) => {
                    match recv::<OverlayToMain, _>(&mut stream) {
                        // The first frame is the `Hello` handshake; later frames (Click/AlertMoved)
                        // are pumped by the per-connection reader thread spawned below.
                        Ok(msg) => eprintln!("[main] overlay connected: {msg:?}"),
                        Err(e) => {
                            eprintln!("[main] overlay handshake read failed: {e}");
                            continue;
                        }
                    }
                    if let Ok(s) = stream.try_clone() {
                        *conn.lock().unwrap() = Some(s);
                        // Signal the main to force-resend the current Config+Ping to this fresh child.
                        reconnected.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    // Per-connection reader: pump overlay→main messages into the inbox until the
                    // connection closes (a respawned child reconnects via the outer accept loop).
                    if let Ok(mut rd) = stream.try_clone() {
                        let inbox = inbox.clone();
                        std::thread::spawn(move || loop {
                            match recv::<OverlayToMain, _>(&mut rd) {
                                Ok(OverlayToMain::Hello) => {} // stray; ignore
                                Ok(m) => inbox.lock().unwrap().push(m),
                                Err(_) => return, // connection closed
                            }
                        });
                    }
                }
                Err(e) => {
                    eprintln!("[main] overlay accept failed: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        });
    }

    /// Send a message to the overlay if it's connected. Drops the stream on write error.
    pub fn send(&self, msg: &MainToOverlay) {
        let mut guard = self.conn.lock().unwrap();
        if let Some(stream) = guard.as_mut() {
            if let Err(e) = send(stream, msg) {
                eprintln!("[main] overlay send failed, dropping connection: {e}");
                *guard = None;
            }
        }
    }

    /// Respawn the overlay if it has exited. Debounced to ~once/2s and capped to avoid a
    /// crash loop; logs and gives up after `MAX_RESTARTS`. Cheap to call every frame.
    pub fn poll(&mut self) {
        if self.gave_up {
            return;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                if self.last_spawn.elapsed() < Self::RESPAWN_DEBOUNCE {
                    return;
                }
                if self.restarts >= Self::MAX_RESTARTS {
                    eprintln!("[main] overlay died (status {status}); giving up after {} restarts", self.restarts);
                    self.gave_up = true;
                    return;
                }
                self.restarts += 1;
                eprintln!("[main] overlay died (status {status}); respawning (#{})", self.restarts);
                *self.conn.lock().unwrap() = None;
                match Self::spawn_child() {
                    Ok(child) => {
                        self.child = child;
                        self.last_spawn = std::time::Instant::now();
                    }
                    Err(e) => eprintln!("[main] overlay respawn failed: {e}"),
                }
            }
            Ok(None) => {} // still running
            Err(e) => eprintln!("[main] overlay try_wait failed: {e}"),
        }
    }

    /// Ask the overlay to exit, then kill+reap as a fallback. Called from the app exit path.
    pub fn shutdown(&mut self) {
        self.gave_up = true; // don't respawn during teardown
        self.send(&MainToOverlay::Shutdown);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(socket_path());
    }
}

#[cfg(target_os = "linux")]
impl Drop for OverlayLink {
    fn drop(&mut self) {
        // Don't leave an orphaned overlay if the link is dropped without an explicit shutdown.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(socket_path());
        // Silence the unused-field warning for `listener`: it is kept alive so the accept
        // thread's cloned listener keeps the socket bound for the process lifetime.
        let _ = &self.listener;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip() {
        let mut buf: Vec<u8> = Vec::new();
        send(&mut buf, &OverlayToMain::Hello).unwrap();
        // 4-byte length prefix + JSON body ("\"Hello\"").
        assert!(buf.len() > 4);
        let mut cur = Cursor::new(buf);
        let got: OverlayToMain = recv(&mut cur).unwrap();
        matches!(got, OverlayToMain::Hello);
    }

    #[test]
    fn frame_roundtrip_ping_payload() {
        let msg = MainToOverlay::Ping(PingMsg {
            pings: Vec::new(),
            raise: true,
            doctrine_url: "https://example/doctrine".to_owned(),
            op_links: std::collections::HashMap::from([("ops".to_owned(), "https://x".to_owned())]),
        });
        let mut buf: Vec<u8> = Vec::new();
        send(&mut buf, &msg).unwrap();
        let mut cur = Cursor::new(buf);
        let got: MainToOverlay = recv(&mut cur).unwrap();
        match got {
            MainToOverlay::Ping(m) => {
                assert!(m.raise);
                assert_eq!(m.doctrine_url, "https://example/doctrine");
                assert_eq!(m.op_links.get("ops").map(String::as_str), Some("https://x"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_config_payload() {
        let msg = MainToOverlay::Config(OverlayConfig {
            ping_enabled: true,
            ping_on_top: crate::settings::OnTop::Smart,
            alert_enabled: true,
            alert_on_top: crate::settings::OnTop::Always,
            window_timeout: 12.0,
            win_pos: Some((10.0, 20.0)),
            win_size: Some((360.0, 240.0)),
        });
        let mut buf: Vec<u8> = Vec::new();
        send(&mut buf, &msg).unwrap();
        let mut cur = Cursor::new(buf);
        let got: MainToOverlay = recv(&mut cur).unwrap();
        match got {
            MainToOverlay::Config(c) => {
                assert!(c.ping_enabled);
                assert_eq!(c.ping_on_top, crate::settings::OnTop::Smart);
                assert!(c.alert_enabled);
                assert_eq!(c.alert_on_top, crate::settings::OnTop::Always);
                assert_eq!(c.window_timeout, 12.0);
                assert_eq!(c.win_pos, Some((10.0, 20.0)));
                assert_eq!(c.win_size, Some((360.0, 240.0)));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_alert_payload() {
        // An IntelReport with a probes badge must survive the &'static str (de)serialization.
        let mut report = crate::intel::IntelReport::default();
        report.probes = Some(crate::intel::Probes::Combat);
        let msg = MainToOverlay::Alert(AlertMsg {
            feed: vec![(report, crate::settings::Severity::Danger)],
            status: std::collections::HashMap::new(),
            resolved_pilots: std::collections::HashMap::from([("X".to_owned(), 42i64)]),
            last_ship: std::collections::HashMap::new(),
            kills: std::collections::HashMap::new(),
            affil: std::collections::HashMap::new(),
            secs: 10.0,
            focus: true,
        });
        let mut buf: Vec<u8> = Vec::new();
        send(&mut buf, &msg).unwrap();
        let mut cur = Cursor::new(buf);
        let got: MainToOverlay = recv(&mut cur).unwrap();
        match got {
            MainToOverlay::Alert(m) => {
                assert_eq!(m.feed.len(), 1);
                assert_eq!(m.feed[0].0.probes, Some(crate::intel::Probes::Combat));
                assert_eq!(m.feed[0].1, crate::settings::Severity::Danger);
                assert_eq!(m.resolved_pilots.get("X"), Some(&42));
                assert!(m.focus);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_overlay_to_main_click() {
        let mut buf: Vec<u8> = Vec::new();
        send(&mut buf, &OverlayToMain::Click(crate::app::IntelClick::System(30000142))).unwrap();
        let mut cur = Cursor::new(buf);
        match recv::<OverlayToMain, _>(&mut cur).unwrap() {
            OverlayToMain::Click(crate::app::IntelClick::System(id)) => assert_eq!(id, 30000142),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn recv_truncated_is_err() {
        // A length prefix promising more bytes than follow must error, not hang/panic.
        let buf = vec![0u8, 0, 0, 10, b'x'];
        let mut cur = Cursor::new(buf);
        let r: io::Result<OverlayToMain> = recv(&mut cur);
        assert!(r.is_err());
    }

    #[test]
    fn socket_path_has_expected_name() {
        let p = socket_path();
        assert!(p.to_string_lossy().ends_with(".sock"));
    }
}
