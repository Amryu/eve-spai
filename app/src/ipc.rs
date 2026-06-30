//! Overlay IPC: protocol + transport between the main process and the spawned
//! `--overlay` child (all platforms).
//!
//! Transport is the child's piped stdio: length-prefixed JSON frames flow main→child over
//! the child's stdin and child→main over the child's stdout (the child's stderr is inherited
//! so its `[overlay] …` logs surface in our own). Using the child's own pipes means the
//! overlay dies for free when the main exits (its stdin hits EOF), and works identically on
//! Linux/Windows/macOS with no socket paths. The main-side [`OverlayLink`] spawns/monitors/
//! kills the child and owns the send half; a reader thread pumps the child's stdout into an
//! inbox the UI thread drains.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{self, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// Main-side handle that owns the overlay child process and its IPC pipes (all platforms).
///
/// Lifecycle: [`start`](OverlayLink::start) spawns the child with piped stdio and wires a
/// reader thread over its stdout. The child is "connected" the moment it is spawned (no accept),
/// and announces readiness with an [`OverlayToMain::Hello`] first frame which flips `reconnected`
/// so the main does its initial Config+Ping resend. [`poll`](OverlayLink::poll) respawns a crashed
/// child (debounced + capped), re-wiring the pipes (and the fresh `Hello` re-arms the resend).
/// [`shutdown`](OverlayLink::shutdown) tells the child to exit and reaps it; dropping the send-half
/// stdin also EOFs the child so it dies with us.
pub struct OverlayLink {
    child: Child,
    /// Write half (the child's stdin), behind a mutex so the UI thread can send frames. Cleared on
    /// a write error or during respawn.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// Messages the overlay sent back (Click / AlertMoved), pushed by the reader thread and drained
    /// by the main in `ui()` (acting on them needs `&mut self` + the ctx).
    inbox: Arc<Mutex<Vec<OverlayToMain>>>,
    /// Flipped true by the reader thread each time a child's `Hello` arrives, so the main forces a
    /// fresh full resend of Config+Ping to a freshly-spawned overlay. Consumed via `take_reconnected`.
    reconnected: Arc<AtomicBool>,
    /// Held so the reader thread for a respawned child can be re-spawned with the same wake context.
    ctx: egui::Context,
    last_spawn: Instant,
    restarts: u32,
    gave_up: bool,
}

impl OverlayLink {
    const MAX_RESTARTS: u32 = 5;
    const RESPAWN_DEBOUNCE: Duration = Duration::from_secs(2);

    /// Spawn the overlay child with piped stdio and wire its reader thread, returning the link.
    pub fn start(ctx: egui::Context) -> io::Result<Self> {
        let stdin = Arc::new(Mutex::new(None));
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let reconnected = Arc::new(AtomicBool::new(false));
        let mut child = Self::spawn_child()?;
        Self::wire(&mut child, &stdin, &inbox, &reconnected, &ctx);
        Ok(Self {
            child,
            stdin,
            inbox,
            reconnected,
            ctx,
            last_spawn: Instant::now(),
            restarts: 0,
            gave_up: false,
        })
    }

    /// True (once) after the overlay (re)connects — i.e. its `Hello` arrived. The main resets its
    /// change-detection on a `true` so a fresh overlay is repopulated with the current Config+Ping.
    pub fn take_reconnected(&self) -> bool {
        self.reconnected.swap(false, Ordering::Relaxed)
    }

    /// Take the overlay→main messages received since the last call (Click / AlertMoved).
    pub fn drain_inbox(&self) -> Vec<OverlayToMain> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    /// Spawn `eve-spai --overlay` with piped stdin/stdout (the IPC transport) and inherited stderr
    /// (so the child's `[overlay] …` logs surface in our own).
    fn spawn_child() -> io::Result<Child> {
        let exe = std::env::current_exe()?;
        Command::new(exe)
            .arg("--overlay")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
    }

    /// Move the child's stdin into the send slot and start a reader thread over its stdout.
    fn wire(
        child: &mut Child,
        stdin: &Arc<Mutex<Option<ChildStdin>>>,
        inbox: &Arc<Mutex<Vec<OverlayToMain>>>,
        reconnected: &Arc<AtomicBool>,
        ctx: &egui::Context,
    ) {
        *stdin.lock().unwrap() = child.stdin.take();
        match child.stdout.take() {
            Some(out) => Self::spawn_reader(out, inbox.clone(), reconnected.clone(), ctx.clone()),
            None => eprintln!("[main] overlay child has no stdout pipe"),
        }
    }

    /// Reader thread: pump the child's stdout frames into the inbox until the pipe closes (the
    /// overlay exited). The first frame is the `Hello` readiness handshake → arm a forced resend.
    fn spawn_reader(
        stdout: ChildStdout,
        inbox: Arc<Mutex<Vec<OverlayToMain>>>,
        reconnected: Arc<AtomicBool>,
        ctx: egui::Context,
    ) {
        std::thread::spawn(move || {
            let mut rd = BufReader::new(stdout);
            loop {
                match recv::<OverlayToMain, _>(&mut rd) {
                    Ok(OverlayToMain::Hello) => {
                        // The child is up and listening; force a full Config+Ping resend.
                        reconnected.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                    }
                    Ok(m) => {
                        inbox.lock().unwrap().push(m);
                        // Wake the main loop NOW so it drains + acts on the click/geometry
                        // immediately, instead of waiting for the next natural repaint.
                        ctx.request_repaint();
                    }
                    Err(_) => return, // stdout closed (overlay exited)
                }
            }
        });
    }

    /// Send a message to the overlay if its stdin pipe is open. Drops the pipe on write error.
    pub fn send(&self, msg: &MainToOverlay) {
        let mut guard = self.stdin.lock().unwrap();
        if let Some(stdin) = guard.as_mut() {
            if let Err(e) = send(stdin, msg) {
                eprintln!("[main] overlay send failed, dropping pipe: {e}");
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
                *self.stdin.lock().unwrap() = None;
                match Self::spawn_child() {
                    Ok(mut child) => {
                        Self::wire(&mut child, &self.stdin, &self.inbox, &self.reconnected, &self.ctx);
                        self.child = child;
                        self.last_spawn = Instant::now();
                    }
                    Err(e) => eprintln!("[main] overlay respawn failed: {e}"),
                }
            }
            Ok(None) => {} // still running
            Err(e) => eprintln!("[main] overlay try_wait failed: {e}"),
        }
    }

    /// Ask the overlay to exit, then drop its stdin (EOF) and kill+reap as a fallback. Called from
    /// the app exit path.
    pub fn shutdown(&mut self) {
        self.gave_up = true; // don't respawn during teardown
        self.send(&MainToOverlay::Shutdown);
        *self.stdin.lock().unwrap() = None; // EOF the child's stdin so it exits on its own
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OverlayLink {
    fn drop(&mut self) {
        // Don't leave an orphaned overlay if the link is dropped without an explicit shutdown.
        *self.stdin.lock().unwrap() = None; // EOF the child's stdin
        let _ = self.child.kill();
        let _ = self.child.wait();
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
}
