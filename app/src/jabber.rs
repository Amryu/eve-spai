// Wired into settings + Pings view + alerts in a follow-up.
#![allow(dead_code)]
//! Embedded XMPP client for fleet pings (Imperium / jabber-server.goonfleet.com).
//!
//! Runs a single-threaded tokio runtime on a background thread, connects over
//! STARTTLS, and watches 1:1 chats from `directorbot` for fleet pings. Parsed pings
//! and connection state are published via [`SharedJabber`]; the app drains new
//! pings each frame (for the Pings view and the alert framework).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::pings::Ping;

/// The Jabber localpart that broadcasts fleet pings.
const PING_SENDER: &str = "directorbot";

#[derive(Default)]
pub struct JabberState {
    /// The user wants the connection up (cleared to stop the background loop).
    pub enabled: bool,
    pub connected: bool,
    /// Human-readable status / last error.
    pub status: String,
    /// All pings received this session (oldest first).
    pub pings: Vec<Ping>,
}

pub type SharedJabber = Arc<Mutex<JabberState>>;
pub type Resolver = Arc<dyn Fn(&str) -> Option<i64> + Send + Sync>;

/// Spawn the background XMPP client. `jid` is the bare JID (e.g. `name@goonfleet.com`).
pub fn spawn(
    jid: String,
    password: String,
    resolve: Resolver,
    state: SharedJabber,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            state.lock().unwrap().status = "Failed to start runtime".to_owned();
            return;
        };
        rt.block_on(run(jid, password, resolve, state, ctx));
    });
}

async fn run(jid: String, password: String, resolve: Resolver, state: SharedJabber, ctx: egui::Context) {
    use xmpp::jid::BareJid;
    use xmpp::{ClientBuilder, ClientType, Event};

    let bare: BareJid = match jid.parse() {
        Ok(j) => j,
        Err(e) => {
            let mut s = state.lock().unwrap();
            s.status = format!("Invalid JID: {e}");
            return;
        }
    };

    loop {
        if !state.lock().unwrap().enabled {
            return;
        }
        {
            let mut s = state.lock().unwrap();
            s.status = "Connecting…".to_owned();
        }
        ctx.request_repaint();

        let mut agent =
            ClientBuilder::new(bare.clone(), &password).set_client(ClientType::Bot, "EVE Spai").build();

        // Event loop for this connection; an empty batch means the stream ended.
        loop {
            if !state.lock().unwrap().enabled {
                let _ = agent.disconnect().await;
                return;
            }
            let events = agent.wait_for_events().await;
            if events.is_empty() {
                break;
            }
            for event in events {
                match event {
                    Event::Online => {
                        let mut s = state.lock().unwrap();
                        s.connected = true;
                        s.status = "Connected".to_owned();
                        ctx.request_repaint();
                    }
                    Event::Disconnected(e) => {
                        let mut s = state.lock().unwrap();
                        s.connected = false;
                        s.status = format!("Disconnected: {e}");
                        ctx.request_repaint();
                    }
                    Event::ChatMessage(_, from, body, _) => {
                        // Localpart of "directorbot@server".
                        let local = from.to_string();
                        let local = local.split('@').next().unwrap_or_default();
                        if local.eq_ignore_ascii_case(PING_SENDER) {
                            let now = chrono::Utc::now().timestamp();
                            let parsed = crate::pings::parse_ping(now, &body, resolve.as_ref());
                            if !parsed.is_empty() {
                                let mut s = state.lock().unwrap();
                                s.pings.extend(parsed);
                                let n = s.pings.len();
                                if n > 200 {
                                    s.pings.drain(0..n - 200);
                                }
                                ctx.request_repaint();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        {
            let mut s = state.lock().unwrap();
            s.connected = false;
            if s.enabled && s.status == "Connected" {
                s.status = "Reconnecting…".to_owned();
            }
        }
        ctx.request_repaint();
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}
