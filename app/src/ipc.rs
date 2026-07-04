use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{self, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub enum OverlayToMain {
    Hello,
    Click(crate::app::IntelClick),
    Verdict { name: String, hidden: bool },
    AlertMoved { pos: Option<(f32, f32)>, size: Option<(f32, f32)> },
}

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct PingMsg {
    pub pings: Vec<crate::pings::Ping>,
    pub raise: bool,
    pub doctrine_url: String,
    pub op_links: std::collections::HashMap<String, String>,
}

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct AlertMsg {
    pub feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub from_you: Vec<Option<u32>>,
    pub status: std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    pub resolved_pilots: std::collections::HashMap<String, i64>,
    #[serde(default)]
    pub uncertain: std::collections::HashSet<String>,
    pub last_ship: std::collections::HashMap<String, (i64, String, i64)>,
    pub kills: std::collections::HashMap<i64, crate::kills::KillInfo>,
    pub affil: std::collections::HashMap<i64, crate::affiliation::Affil>,
    pub secs: f32,
    pub focus: bool,
}

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

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct AlertPush {
    pub reports: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub secs: f32,
}

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub enum MainToOverlay {
    Ping(PingMsg),
    Alert(AlertMsg),
    AlertPush(AlertPush),
    Config(OverlayConfig),
    Shutdown,
}

pub fn send<T: Serialize, W: Write>(w: &mut W, msg: &T) -> io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

pub fn send_shared(stdin: &Arc<Mutex<Option<ChildStdin>>>, msg: &MainToOverlay) {
    let mut guard = stdin.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = guard.as_mut() {
        if send(w, msg).is_err() {
            *guard = None;
        }
    }
}

pub fn recv<T: DeserializeOwned, R: Read>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub struct OverlayLink {
    child: Child,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    inbox: Arc<Mutex<Vec<OverlayToMain>>>,
    reconnected: Arc<AtomicBool>,
    ctx: egui::Context,
    last_spawn: Instant,
    restarts: u32,
    gave_up: bool,
}

impl OverlayLink {
    const MAX_RESTARTS: u32 = 5;
    const RESPAWN_DEBOUNCE: Duration = Duration::from_secs(2);

    pub fn start(ctx: egui::Context, stdin: Arc<Mutex<Option<ChildStdin>>>) -> io::Result<Self> {
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

    pub fn take_reconnected(&self) -> bool {
        self.reconnected.swap(false, Ordering::Relaxed)
    }

    pub fn drain_inbox(&self) -> Vec<OverlayToMain> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    fn spawn_child() -> io::Result<Child> {
        let exe = std::env::current_exe()?;
        Command::new(exe)
            .arg("--overlay")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
    }

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
                        reconnected.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                    }
                    Ok(m) => {
                        inbox.lock().unwrap().push(m);
                        ctx.request_repaint();
                    }
                    Err(_) => return,
                }
            }
        });
    }

    pub fn send(&self, msg: &MainToOverlay) {
        let mut guard = self.stdin.lock().unwrap();
        if let Some(stdin) = guard.as_mut() {
            if let Err(e) = send(stdin, msg) {
                eprintln!("[main] overlay send failed, dropping pipe: {e}");
                *guard = None;
            }
        }
    }

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
            Ok(None) => {}
            Err(e) => eprintln!("[main] overlay try_wait failed: {e}"),
        }
    }

    pub fn shutdown(&mut self) {
        self.gave_up = true;
        self.send(&MainToOverlay::Shutdown);
        *self.stdin.lock().unwrap() = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OverlayLink {
    fn drop(&mut self) {
        *self.stdin.lock().unwrap() = None;
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
        let mut report = crate::intel::IntelReport::default();
        report.probes = Some(crate::intel::Probes::Combat);
        let msg = MainToOverlay::Alert(AlertMsg {
            feed: vec![(report, crate::settings::Severity::Danger)],
            from_you: vec![Some(7)],
            status: std::collections::HashMap::new(),
            resolved_pilots: std::collections::HashMap::from([("X".to_owned(), 42i64)]),
            uncertain: std::collections::HashSet::new(),
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
                assert_eq!(m.from_you, vec![Some(7)]);
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
        let buf = vec![0u8, 0, 0, 10, b'x'];
        let mut cur = Cursor::new(buf);
        let r: io::Result<OverlayToMain> = recv(&mut cur);
        assert!(r.is_err());
    }
}
