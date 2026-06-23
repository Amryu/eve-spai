//! Embedded XMPP client for fleet pings (Imperium / jabber-server.goonfleet.com).
//!
//! Runs a single-threaded tokio runtime on a background thread, connects over
//! STARTTLS, and watches 1:1 chats from `directorbot` for fleet pings. Parsed pings
//! and connection state are published via [`SharedJabber`]; the app drains new
//! pings each frame (for the Pings view and the alert framework).

use std::sync::{Arc, Mutex};

use crate::pings::Ping;

/// The Jabber localpart that broadcasts fleet pings.
const PING_SENDER: &str = "directorbot";

const KEYCHAIN_SERVICE: &str = "eve-spai-jabber";

/// Store the Jabber password in the OS keychain (keyed by JID).
pub fn save_password(jid: &str, password: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    keyring::Entry::new(KEYCHAIN_SERVICE, jid)
        .context("opening keychain entry")?
        .set_password(password)
        .context("writing Jabber password")?;
    Ok(())
}

/// Read the Jabber password from the keychain, if present.
pub fn load_password(jid: &str) -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, jid).ok()?.get_password().ok()
}

/// Whether a password is stored for this JID.
pub fn has_password(jid: &str) -> bool {
    load_password(jid).is_some()
}

/// A chat message (1:1 or room), as shown in the Jabber view.
#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub from: String,
    pub body: String,
    #[allow(dead_code)] // kept for future timestamping in the chat UI
    pub time: i64,
    pub outgoing: bool,
}

/// A roster contact.
#[derive(Clone, Debug)]
pub struct Contact {
    #[allow(dead_code)] // the map key is the JID; this mirrors it for convenience
    pub jid: String,
    pub name: Option<String>,
    /// Roster groups the server places this contact in (e.g. "Directors").
    pub groups: Vec<String>,
    pub presence: Presence,
    /// Free-text status message, if the contact set one.
    pub status_text: String,
}

/// A contact's availability, from their presence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    #[default]
    Offline,
    Online,
    Away,
    /// Extended away.
    Xa,
    Dnd,
}

impl Presence {
    pub fn label(self) -> &'static str {
        match self {
            Presence::Offline => "Offline",
            Presence::Online => "Online",
            Presence::Away => "Away",
            Presence::Xa => "Away (long)",
            Presence::Dnd => "Do not disturb",
        }
    }
    /// Dot colour (R, G, B).
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Presence::Online => (0x4C, 0xC2, 0x6A),
            Presence::Away | Presence::Xa => (0xE0, 0xA4, 0x3A),
            Presence::Dnd => (0xD8, 0x4C, 0x4C),
            Presence::Offline => (0x6A, 0x6A, 0x6A),
        }
    }
    pub fn online(self) -> bool {
        !matches!(self, Presence::Offline)
    }
    /// Sort rank: online first.
    pub fn rank(self) -> u8 {
        match self {
            Presence::Online => 0,
            Presence::Away => 1,
            Presence::Xa => 2,
            Presence::Dnd => 3,
            Presence::Offline => 4,
        }
    }
}

/// Commands sent from the UI to the background client.
pub enum Cmd {
    Send { to: String, body: String },
}

#[derive(Default)]
pub struct JabberState {
    /// The user wants the connection up (cleared to stop the background loop).
    pub enabled: bool,
    /// A background client thread is alive.
    pub running: bool,
    pub connected: bool,
    /// Human-readable status / last error.
    pub status: String,
    /// Roster contacts (bare JID → contact).
    pub roster: std::collections::BTreeMap<String, Contact>,
    /// Conversation history keyed by bare JID (1:1) or room JID.
    pub chats: std::collections::BTreeMap<String, Vec<ChatMsg>>,
    /// Conversations with unread messages.
    pub unread: std::collections::BTreeSet<String>,
    /// Parsed fleet pings (oldest first).
    pub pings: Vec<Ping>,
}

pub type SharedJabber = Arc<Mutex<JabberState>>;
pub type Resolver = Arc<dyn Fn(&str) -> Option<i64> + Send + Sync>;
pub type CmdSender = tokio::sync::mpsc::UnboundedSender<Cmd>;

/// Spawn the background XMPP client. Returns a sender for outgoing commands.
/// `server` is the host to connect to directly (empty = resolve from the JID).
pub fn spawn(
    jid: String,
    password: String,
    server: String,
    resolve: Resolver,
    state: SharedJabber,
    ctx: egui::Context,
) -> CmdSender {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            state.lock().unwrap().status = "Failed to start runtime".to_owned();
            return;
        };
        rt.block_on(run(jid, password, server, resolve, state, rx, ctx));
    });
    tx
}

fn push_msg(state: &SharedJabber, key: &str, msg: ChatMsg, mark_unread: bool) {
    let mut s = state.lock().unwrap();
    s.chats.entry(key.to_owned()).or_default().push(msg);
    if mark_unread {
        s.unread.insert(key.to_owned());
    }
}

async fn run(
    jid: String,
    password: String,
    server: String,
    resolve: Resolver,
    state: SharedJabber,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Cmd>,
    ctx: egui::Context,
) {
    use xmpp::jid::BareJid;
    use xmpp::message::send::MessageSettings;
    use xmpp::tokio_xmpp::connect::{DnsConfig, StartTlsServerConnector};
    use xmpp::{ClientBuilder, ClientFeature, ClientType};

    let bare: BareJid = match jid.parse() {
        Ok(j) => j,
        Err(e) => {
            let mut s = state.lock().unwrap();
            s.status = format!("Invalid JID: {e}");
            s.running = false;
            return;
        }
    };
    state.lock().unwrap().running = true;
    state.lock().unwrap().status = "Connecting…".to_owned();
    ctx.request_repaint();
    eprintln!(
        "[jabber] connecting jid={bare} server={}",
        if server.trim().is_empty() { bare.domain().as_str() } else { server.trim() }
    );

    // Connect to the configured server directly (the JID domain usually has no SRV
    // record); fall back to SRV from the JID domain when no server is set.
    let dns = if server.trim().is_empty() {
        DnsConfig::srv_default_client(bare.domain().as_str())
    } else {
        DnsConfig::NoSrv { host: server.trim().to_owned(), port: 5222, resolver: None }
    };
    let mut agent =
        ClientBuilder::new_with_connector(bare.clone(), &password, StartTlsServerConnector(dns))
            .set_client(ClientType::Bot, "EVE Spai")
            .enable_feature(ClientFeature::ContactList)
            .build();

    // tokio-xmpp reconnects transparently, so we simply process events until the user
    // disables Jabber. An *empty* event batch is normal (a stanza that produced no
    // high-level event, e.g. the roster reply) — it does NOT mean the stream ended,
    // so we must not tear the connection down on it.
    loop {
        if !state.lock().unwrap().enabled {
            let _ = agent.disconnect().await;
            break;
        }
        tokio::select! {
            events = agent.wait_for_events() => {
                for event in events {
                    handle_event(event, &state, resolve.as_ref(), &ctx);
                }
            }
            Some(cmd) = rx.recv() => match cmd {
                Cmd::Send { to, body } => {
                    if let Ok(recipient) = to.parse::<BareJid>() {
                        agent
                            .send_message(MessageSettings { recipient, message: &body, lang: None })
                            .await;
                        let now = chrono::Utc::now().timestamp();
                        push_msg(
                            &state,
                            &to,
                            ChatMsg { from: "me".to_owned(), body, time: now, outgoing: true },
                            false,
                        );
                        ctx.request_repaint();
                    }
                }
            },
        }
    }
    state.lock().unwrap().running = false;
}

/// Map an XMPP presence stanza to our availability enum.
fn presence_from(p: &xmpp::parsers::presence::Presence) -> Presence {
    use xmpp::parsers::presence::{Show, Type};
    match p.type_ {
        Type::Unavailable => Presence::Offline,
        Type::None => match p.show {
            Some(Show::Away) => Presence::Away,
            Some(Show::Xa) => Presence::Xa,
            Some(Show::Dnd) => Presence::Dnd,
            _ => Presence::Online, // no <show> or "chat" = available
        },
        _ => Presence::Offline, // subscribe/error/etc.
    }
}

fn handle_event(
    event: xmpp::Event,
    state: &SharedJabber,
    resolve: &(dyn Fn(&str) -> Option<i64> + Send + Sync),
    ctx: &egui::Context,
) {
    use xmpp::Event;
    let now = chrono::Utc::now().timestamp();
    match event {
        Event::Online => {
            eprintln!("[jabber] online");
            let mut s = state.lock().unwrap();
            s.connected = true;
            s.status = "Connected".to_owned();
        }
        Event::Disconnected(e) => {
            eprintln!("[jabber] disconnected: {e}");
            let mut s = state.lock().unwrap();
            s.connected = false;
            s.status = format!("Disconnected: {e}");
        }
        Event::ContactAdded(item) | Event::ContactChanged(item) => {
            let jid = item.jid.to_string();
            let groups: Vec<String> = item.groups.iter().map(|g| g.0.clone()).collect();
            let mut s = state.lock().unwrap();
            // Keep any presence we've already learned for this contact.
            let entry = s.roster.entry(jid.clone()).or_insert_with(|| Contact {
                jid: jid.clone(),
                name: None,
                groups: Vec::new(),
                presence: Presence::default(),
                status_text: String::new(),
            });
            entry.name = item.name.clone();
            entry.groups = groups;
        }
        Event::ContactRemoved(item) => {
            state.lock().unwrap().roster.remove(&item.jid.to_string());
        }
        // Raw presence stanzas (escape-hatch): update the contact's availability.
        Event::Presence(p) => {
            if let Some(from) = &p.from {
                let bare = from.to_bare().to_string();
                let presence = presence_from(&p);
                let status = p.statuses.values().next().cloned().unwrap_or_default();
                let mut s = state.lock().unwrap();
                if let Some(c) = s.roster.get_mut(&bare) {
                    c.presence = presence;
                    c.status_text = status;
                }
            }
        }
        Event::ChatMessage(_, from, body, _) => {
            let key = from.to_string();
            let local = key.split('@').next().unwrap_or_default();
            // directorbot broadcasts are also parsed into fleet pings.
            if local.eq_ignore_ascii_case(PING_SENDER) {
                let parsed = crate::pings::parse_ping(now, &body, resolve);
                if !parsed.is_empty() {
                    let mut s = state.lock().unwrap();
                    s.pings.extend(parsed);
                    let n = s.pings.len();
                    if n > 200 {
                        s.pings.drain(0..n - 200);
                    }
                }
            }
            push_msg(
                state,
                &key,
                ChatMsg { from: key.clone(), body, time: now, outgoing: false },
                true,
            );
        }
        Event::RoomMessage(_, room, nick, body, _) => {
            push_msg(
                state,
                &room.to_string(),
                ChatMsg { from: nick.to_string(), body, time: now, outgoing: false },
                true,
            );
        }
        _ => {}
    }
    ctx.request_repaint();
}
