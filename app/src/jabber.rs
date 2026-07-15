use std::sync::{Arc, Mutex};

use crate::pings::Ping;

const PING_SENDER: &str = "directorbot";
pub const PING_FEED_KEY: &str = "__pings__";

const KEYCHAIN_SERVICE: &str = "eve-spai-jabber";

pub fn save_password(jid: &str, password: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    keyring::Entry::new(KEYCHAIN_SERVICE, jid)
        .context("opening keychain entry")?
        .set_password(password)
        .context("writing Jabber password")?;
    Ok(())
}

pub fn load_password(jid: &str) -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, jid).ok()?.get_password().ok()
}

pub fn has_password(jid: &str) -> bool {
    load_password(jid).is_some()
}

/// Reject a malformed JID before we try to connect. `None` means it's a usable `user@domain`.
pub fn jid_format_error(jid: &str) -> Option<String> {
    use xmpp::jid::BareJid;
    let t = jid.trim();
    if t.is_empty() {
        return Some("Enter your Jabber address".to_owned());
    }
    match t.parse::<BareJid>() {
        Ok(j) if j.node().is_none() => {
            Some("Address needs a username, like name@server.com".to_owned())
        }
        Ok(_) => None,
        Err(_) => Some("Not a valid address (use name@server.com)".to_owned()),
    }
}

enum Preflight {
    Ok,
    BadAuth,
    Unreachable(String),
    Other(String),
}

/// One authentication round-trip using the same connector as the live session, so we can tell wrong
/// credentials from an unreachable server before handing off to the auto-reconnecting agent (which
/// silently retries every error forever).
async fn preflight(
    jid: xmpp::jid::Jid,
    node: String,
    password: String,
    dns: xmpp::tokio_xmpp::connect::DnsConfig,
) -> Preflight {
    use sasl::common::Credentials;
    use xmpp::tokio_xmpp::client_login;
    use xmpp::tokio_xmpp::connect::{ServerConnector, StartTlsServerConnector};
    use xmpp::tokio_xmpp::parsers::ns;
    use xmpp::tokio_xmpp::xmlstream::Timeouts;

    let connector = StartTlsServerConnector(dns);
    let (stream, cb) = match connector.connect(&jid, ns::JABBER_CLIENT, Timeouts::default()).await {
        Ok(v) => v,
        Err(e) => return classify(e),
    };
    let (features, stream) = match stream.recv_features().await {
        Ok(v) => v,
        Err(e) => return classify(e.into()),
    };
    let creds = Credentials::default()
        .with_username(node.as_str())
        .with_password(password.as_str())
        .with_channel_binding(cb);
    match client_login(stream, features.sasl_mechanisms, creds).await {
        Ok(_) => Preflight::Ok,
        Err(e) => classify(e),
    }
}

fn classify(e: xmpp::tokio_xmpp::Error) -> Preflight {
    use xmpp::tokio_xmpp::Error;
    match e {
        Error::Auth(_) => Preflight::BadAuth,
        Error::Io(_) | Error::Connection(_) | Error::Addr(_) => Preflight::Unreachable(e.to_string()),
        other => Preflight::Other(other.to_string()),
    }
}

#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub from: String,
    pub body: String,
    #[allow(dead_code)]
    pub time: i64,
    pub outgoing: bool,
}

#[derive(Clone, Debug)]
pub struct Contact {
    #[allow(dead_code)]
    pub jid: String,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub presence: Presence,
    pub status_text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    #[default]
    Offline,
    Online,
    Away,
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
}

pub enum Cmd {
    Send { to: String, body: String },
    SendRoom { room: String, body: String },
    JoinRoom { room: String },
    LeaveRoom { room: String },
    SetPresence { show: Presence, status: String },
}

#[derive(Clone, Default)]
pub struct JabberNotifyCfg {
    pub sound_enabled: bool,
    pub ping_sound: String,
    pub msg_sound: String,
    pub mention_sound: String,
    pub ping_volume: f32,
    pub msg_volume: f32,
    pub mention_volume: f32,
    pub mention_names: Vec<String>,
    pub mention_ignores_mute: bool,
    pub ping_rules: Vec<crate::settings::PingRule>,
    pub muted: std::collections::BTreeMap<String, i64>,
}

/// A mention is any of `names` appearing in the body as a whole word (or whole phrase), so "seb"
/// hits "@seb", "seb:" and "hey seb." but not "sebastian".
pub fn mention_hit(body: &str, names: &[String]) -> bool {
    let body = body.to_lowercase();
    let free = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    names.iter().any(|name| {
        let name = name.trim().to_lowercase();
        if name.is_empty() {
            return false;
        }
        body.match_indices(&name).any(|(at, _)| {
            free(body[..at].chars().next_back())
                && free(body[at + name.len()..].chars().next())
        })
    })
}

#[derive(Default)]
pub struct JabberState {
    pub enabled: bool,
    pub running: bool,
    pub connected: bool,
    pub status: String,
    /// A terminal failure (bad credentials, invalid address, unreachable). The connection stopped
    /// and won't retry; the UI drops back to the login form and shows this.
    pub fatal: Option<String>,
    /// Set once the session reaches `Online`. The chats view waits for this so a failed connect
    /// never flashes it; a later transient drop keeps it set (auto-reconnect handles the blip).
    pub ever_online: bool,
    pub roster: std::collections::BTreeMap<String, Contact>,
    pub presences: std::collections::BTreeMap<String, (Presence, String)>,
    pub rooms: std::collections::BTreeSet<String>,
    pub notify: Vec<(String, bool)>,
    pub pings_unread: bool,
    pub chats: std::collections::BTreeMap<String, Vec<ChatMsg>>,
    pub unread: std::collections::BTreeSet<String>,
    /// Conversations carrying an unread message that named us.
    pub mentions: std::collections::BTreeSet<String>,
    pub pings: Vec<Ping>,
    pub notify_cfg: JabberNotifyCfg,
}

fn is_muted(muted: &std::collections::BTreeMap<String, i64>, key: &str) -> bool {
    muted
        .get(key)
        .is_some_and(|&until| until == i64::MAX || chrono::Utc::now().timestamp() < until)
}

fn fire_arrival_notification(
    cfg: &JabberNotifyCfg,
    key: &str,
    ping: Option<&Ping>,
    mention: Option<&ChatMsg>,
) {
    if is_muted(&cfg.muted, key) && !(mention.is_some() && cfg.mention_ignores_mute) {
        return;
    }
    let (suppress, notify, sound, prio, volume) = match ping {
        Some(p) => match crate::pings::match_ping_rule(&cfg.ping_rules, p) {
            Some(r) => (
                r.suppress,
                r.notify,
                if r.sound.is_empty() { cfg.ping_sound.clone() } else { r.sound.clone() },
                1u8,
                r.volume.unwrap_or(cfg.ping_volume),
            ),
            None if cfg.ping_rules.is_empty() => {
                (false, true, cfg.ping_sound.clone(), 1u8, cfg.ping_volume)
            }
            None => return,
        },
        // Prio 1 so a mention breaks through the cooldown gate that ordinary chat traffic sits behind.
        None if mention.is_some() => {
            (false, true, cfg.mention_sound.clone(), 1u8, cfg.mention_volume)
        }
        None => (false, true, cfg.msg_sound.clone(), 0u8, cfg.msg_volume),
    };
    if suppress || !notify {
        return;
    }
    if cfg.sound_enabled && !sound.is_empty() && !sound.eq_ignore_ascii_case("off") {
        crate::sound::play_prio(&sound, prio, volume);
    }
    if let Some(m) = mention {
        let room = key.split('@').next().unwrap_or(key);
        crate::app::notify_os(&format!("Mentioned in {room}"), &format!("{}: {}", m.from, m.body));
    }
    if let Some(Ping::Fleet { fc, doctrine, .. }) = ping.filter(|p| p.is_fleet_call()) {
        let body = match doctrine {
            Some(d) => format!("FC: {fc} \u{00B7} {d}"),
            None => format!("FC: {fc}"),
        };
        crate::app::notify_os("Fleet ping", &body);
    }
}

pub type SharedJabber = Arc<Mutex<JabberState>>;
pub type Resolver = Arc<dyn Fn(&str) -> Option<i64> + Send + Sync>;
pub type CmdSender = tokio::sync::mpsc::UnboundedSender<Cmd>;

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    jid: String,
    password: String,
    server: String,
    rooms: Vec<String>,
    resolve: Resolver,
    state: SharedJabber,
    ping_shared: crate::app::SharedPingWindow,
    ctx: egui::Context,
) -> CmdSender {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let cmds = tx.clone();
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            state.lock().unwrap().status = "Failed to start runtime".to_owned();
            return;
        };
        rt.block_on(run(jid, password, server, rooms, resolve, state, ping_shared, rx, cmds, ctx));
    });
    tx
}

/// The room a MUC invite points at: XEP-0045 mediated invites carry the room as the stanza sender,
/// XEP-0249 direct invites name it in the `jid` attribute.
fn invited_room(msg: &xmpp::parsers::message::Message) -> Option<String> {
    const MUC_USER: &str = "http://jabber.org/protocol/muc#user";
    const DIRECT: &str = "jabber:x:conference";
    msg.payloads.iter().find_map(|p| {
        if p.is("x", MUC_USER) && p.has_child("invite", MUC_USER) {
            msg.from.as_ref().map(|f| f.to_bare().to_string())
        } else if p.is("x", DIRECT) {
            p.attr("jid").map(str::to_owned)
        } else {
            None
        }
    })
}

fn push_ping_window(ping_shared: &crate::app::SharedPingWindow, ctx: &egui::Context, ping: &Ping) {
    {
        let mut st = ping_shared.lock().unwrap();
        if !st.enabled {
            return;
        }
        if st.windows.first().map(|s| &s.ping) == Some(ping) {
            return;
        }
        st.windows.insert(
            0,
            crate::app::PingShown { ping: ping.clone(), shown_at: std::time::Instant::now() },
        );
        st.raise = true;
    }
    ctx.request_repaint_of(egui::ViewportId::from_hash_of("fleet_ping_window"));
    // When the overlay child owns the ping window, it lives in the child process, not this viewport.
    // Wake the root so `fleet_ping_window_ui` runs and forwards the new ping over IPC. (Harmless
    // when running the in-process fallback.)
    ctx.request_repaint();
}

fn push_msg(
    state: &SharedJabber,
    key: &str,
    msg: ChatMsg,
    mark_unread: bool,
    check_mention: bool,
    store: Option<&crate::store::Store>,
) {
    if let Some(s) = store {
        s.add_chat(key, &msg.from, &msg.body, msg.time, msg.outgoing);
    }
    let fire = {
        let mut s = state.lock().unwrap();
        let mention = check_mention && mention_hit(&msg.body, &s.notify_cfg.mention_names);
        let mentioned = mention.then(|| msg.clone());
        let conv = s.chats.entry(key.to_owned()).or_default();
        conv.push(msg);
        let n = conv.len();
        if n > 1000 {
            conv.drain(0..n - 1000);
        }
        if mark_unread {
            s.unread.insert(key.to_owned());
            if mention {
                s.mentions.insert(key.to_owned());
            }
            s.notify.push((key.to_owned(), false));
            Some((s.notify_cfg.clone(), mentioned))
        } else {
            None
        }
    };
    if let Some((cfg, mentioned)) = fire {
        fire_arrival_notification(&cfg, key, None, mentioned.as_ref());
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    jid: String,
    password: String,
    server: String,
    initial_rooms: Vec<String>,
    resolve: Resolver,
    state: SharedJabber,
    ping_shared: crate::app::SharedPingWindow,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Cmd>,
    cmds: CmdSender,
    ctx: egui::Context,
) {
    use xmpp::jid::BareJid;
    use xmpp::message::send::MessageSettings;
    use xmpp::muc::room::{JoinRoomSettings, LeaveRoomSettings, RoomMessageSettings};
    use xmpp::tokio_xmpp::connect::{DnsConfig, StartTlsServerConnector};
    use xmpp::{ClientBuilder, ClientFeature, ClientType};

    let fail = |state: &SharedJabber, msg: String| {
        let mut s = state.lock().unwrap();
        s.status = msg.clone();
        s.fatal = Some(msg);
        s.connected = false;
        s.running = false;
    };

    let bare: BareJid = match jid.parse::<BareJid>() {
        Ok(j) if j.node().is_some() => j,
        _ => {
            fail(&state, jid_format_error(&jid).unwrap_or_else(|| "Invalid address".to_owned()));
            return;
        }
    };
    {
        let mut s = state.lock().unwrap();
        s.running = true;
        s.fatal = None;
        s.ever_online = false;
        s.status = "Connecting…".to_owned();
    }
    ctx.request_repaint();
    eprintln!(
        "[jabber] connecting jid={bare} server={}",
        if server.trim().is_empty() { bare.domain().as_str() } else { server.trim() }
    );

    // Connect to the configured server directly (the JID domain usually has no SRV
    // record); fall back to SRV from the JID domain when no server is set.
    let make_dns = || {
        if server.trim().is_empty() {
            DnsConfig::srv_default_client(bare.domain().as_str())
        } else {
            DnsConfig::NoSrv { host: server.trim().to_owned(), port: 5222, resolver: None }
        }
    };

    // A wrong password would otherwise loop forever on "Connecting…": the agent retries every error
    // silently. Probe auth once first and surface a specific reason.
    let node = bare.node().unwrap().as_str().to_owned();
    match preflight(bare.clone().into(), node, password.clone(), make_dns()).await {
        Preflight::Ok => {}
        Preflight::BadAuth => {
            fail(&state, "Login failed. Check your username and password.".to_owned());
            ctx.request_repaint();
            return;
        }
        Preflight::Unreachable(e) => {
            eprintln!("[jabber] preflight unreachable: {e}");
            fail(&state, "Can't reach the server. Check the server address and your connection.".to_owned());
            ctx.request_repaint();
            return;
        }
        Preflight::Other(e) => {
            fail(&state, format!("Couldn't connect: {e}"));
            ctx.request_repaint();
            return;
        }
    }

    let dns = make_dns();
    let mut builder =
        ClientBuilder::new_with_connector(bare.clone(), &password, StartTlsServerConnector(dns))
            .set_client(ClientType::Bot, "EVE Spai")
            .enable_feature(ClientFeature::ContactList)
            // Advertises bookmarks2+notify, so rooms the server adds us to arrive live instead of
            // only on the next connect.
            .enable_feature(ClientFeature::JoinRooms);
    // Without this the library joins every room as its default nick, "xmpp-rs", which is what the
    // whole channel sees.
    if let Ok(nick) = bare.node().unwrap().as_str().parse::<xmpp::jid::ResourcePart>() {
        builder = builder.set_default_nick(&nick);
    }
    let mut agent = builder.build();

    let store = crate::store::Store::open().ok();

    // tokio-xmpp reconnects transparently, so we simply process events until the user
    // disables Jabber. An *empty* event batch is normal (a stanza that produced no
    // high-level event, e.g. the roster reply) — it does NOT mean the stream ended,
    // so we must not tear the connection down on it.
    let mut was_connected = false;
    loop {
        if !state.lock().unwrap().enabled {
            let _ = agent.disconnect().await;
            break;
        }
        // (Re)join the persisted rooms on every fresh connection — the initial connect AND after a
        // transparent reconnect (e.g. the XMPP server restarting). Resetting on the disconnect→connect
        // edge is what makes the client actually recover: an `Online` without a rejoin leaves us
        // connected but in no rooms, so no intel/pings arrive.
        let connected = state.lock().unwrap().connected;
        if connected && !was_connected {
            for r in &initial_rooms {
                if let Ok(room) = r.parse::<BareJid>() {
                    agent.join_room(JoinRoomSettings::new(room)).await;
                }
            }
        }
        was_connected = connected;
        tokio::select! {
            events = agent.wait_for_events() => {
                let mut urgent = false;
                let mut background = false;
                for event in events {
                    if handle_event(event, &state, resolve.as_ref(), &ping_shared, &cmds, &ctx, store.as_ref()) {
                        urgent = true;
                    } else {
                        background = true;
                    }
                }
                if urgent {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                } else if background {
                    ctx.request_repaint_after(std::time::Duration::from_secs(2));
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
                            false,
                            store.as_ref(),
                        );
                        ctx.request_repaint();
                    }
                }
                // Room messages are echoed back by the MUC, so we don't push locally.
                Cmd::SendRoom { room, body } => {
                    if let Ok(r) = room.parse::<BareJid>() {
                        agent.send_room_message(RoomMessageSettings::new(r, &body)).await;
                    }
                }
                Cmd::JoinRoom { room } => {
                    if let Ok(r) = room.parse::<BareJid>() {
                        agent.join_room(JoinRoomSettings::new(r)).await;
                    }
                }
                Cmd::LeaveRoom { room } => {
                    if let Ok(r) = room.parse::<BareJid>() {
                        agent.leave_room(LeaveRoomSettings::new(r)).await;
                    }
                }
                Cmd::SetPresence { show, status } => {
                    use xmpp::parsers::presence::{Presence as Pres, Show, Type};
                    let (ty, sh) = match show {
                        Presence::Offline => (Type::Unavailable, None),
                        Presence::Online => (Type::None, None),
                        Presence::Away => (Type::None, Some(Show::Away)),
                        Presence::Xa => (Type::None, Some(Show::Xa)),
                        Presence::Dnd => (Type::None, Some(Show::Dnd)),
                    };
                    let mut pres = Pres::new(ty);
                    pres.show = sh;
                    if !status.trim().is_empty() {
                        pres.set_status(String::new(), status);
                    }
                    let _ = agent.send_stanza(pres).await;
                }
            },
        }
    }
    state.lock().unwrap().running = false;
}

fn presence_from(p: &xmpp::parsers::presence::Presence) -> Presence {
    use xmpp::parsers::presence::{Show, Type};
    match p.type_ {
        Type::Unavailable => Presence::Offline,
        Type::None => match p.show {
            Some(Show::Away) => Presence::Away,
            Some(Show::Xa) => Presence::Xa,
            Some(Show::Dnd) => Presence::Dnd,
            _ => Presence::Online,
        },
        _ => Presence::Offline,
    }
}

fn handle_event(
    event: xmpp::Event,
    state: &SharedJabber,
    resolve: &(dyn Fn(&str) -> Option<i64> + Send + Sync),
    ping_shared: &crate::app::SharedPingWindow,
    cmds: &CmdSender,
    ctx: &egui::Context,
    store: Option<&crate::store::Store>,
) -> bool {
    use xmpp::Event;
    let urgent = !matches!(
        event,
        Event::Presence(_)
            | Event::ContactAdded(_)
            | Event::ContactChanged(_)
            | Event::ContactRemoved(_)
            | Event::Message(_)
    );
    let now = chrono::Utc::now().timestamp();
    match event {
        Event::Online => {
            eprintln!("[jabber] online");
            let mut s = state.lock().unwrap();
            s.connected = true;
            s.ever_online = true;
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
            let known = s.presences.get(&jid).cloned();
            let entry = s.roster.entry(jid.clone()).or_insert_with(|| Contact {
                jid: jid.clone(),
                name: None,
                groups: Vec::new(),
                presence: Presence::default(),
                status_text: String::new(),
            });
            entry.name = item.name.clone();
            entry.groups = groups;
            if let Some((pres, st)) = known {
                entry.presence = pres;
                entry.status_text = st;
            }
        }
        Event::ContactRemoved(item) => {
            state.lock().unwrap().roster.remove(&item.jid.to_string());
        }
        Event::Presence(p) => {
            if let Some(from) = &p.from {
                let bare = from.to_bare().to_string();
                let presence = presence_from(&p);
                let status = p.statuses.values().next().cloned().unwrap_or_default();
                let mut s = state.lock().unwrap();
                s.presences.insert(bare.clone(), (presence, status.clone()));
                if let Some(c) = s.roster.get_mut(&bare) {
                    c.presence = presence;
                    c.status_text = status;
                }
            }
        }
        Event::ChatMessage(_, from, body, time_info) => {
            // Offline/history messages carry a <delay/>; we store them but must NOT
            // sound/badge them (else the backlog of missed pings screeches on startup).
            let delayed = !time_info.delays.is_empty();
            let stamp = time_info
                .delays
                .first()
                .map(|d| d.stamp.0.timestamp())
                .unwrap_or(now);
            // Key by the BARE JID (no /resource): outgoing DMs and presences use the bare form, and
            // the UI's DM list only surfaces conversations whose key is a valid bare JID — a full
            // JID here fragmented the thread and hid the incoming DM entirely.
            let key = from.to_bare().to_string();
            let local = key.split('@').next().unwrap_or_default();
            if local.eq_ignore_ascii_case(PING_SENDER) {
                let parsed = crate::pings::parse_ping(stamp, &body, resolve);
                if !parsed.is_empty() {
                    if let Some(store) = store {
                        for p in &parsed {
                            if let Ok(json) = serde_json::to_string(p) {
                                store.add_ping(p.timestamp(), &json);
                            }
                        }
                    }
                    let fire = {
                        let mut s = state.lock().unwrap();
                        s.pings.extend(parsed);
                        let n = s.pings.len();
                        if n > 2000 {
                            s.pings.drain(0..n - 2000);
                        }
                        if !delayed {
                            s.pings_unread = true;
                            s.notify.push((PING_FEED_KEY.to_owned(), true));
                            s.pings.last().cloned().map(|p| (s.notify_cfg.clone(), p))
                        } else {
                            None
                        }
                    };
                    if let Some((cfg, ping)) = fire {
                        fire_arrival_notification(&cfg, PING_FEED_KEY, Some(&ping), None);
                        if crate::pings::ping_alerts(&cfg.ping_rules, &ping) {
                            push_ping_window(ping_shared, ctx, &ping);
                        }
                    }
                }
            }
            push_msg(
                state,
                &key,
                ChatMsg { from: key.clone(), body, time: stamp, outgoing: false },
                !delayed,
                false,
                store,
            );
        }
        // The library handles bookmarks but not invites, so an invite is joined by hand. Both flavours
        // are idempotent on the agent side (a redundant join is warned about and dropped).
        Event::Message(msg) => {
            if let Some(room) = invited_room(&msg) {
                eprintln!("[jabber] invited to room: {room}");
                let _ = cmds.send(Cmd::JoinRoom { room });
            }
        }
        Event::RoomJoined(room) => {
            eprintln!("[jabber] room joined: {room}");
            state.lock().unwrap().rooms.insert(room.to_string());
        }
        Event::RoomLeft(room) => {
            eprintln!("[jabber] room left: {room}");
            state.lock().unwrap().rooms.remove(&room.to_string());
        }
        Event::RoomMessage(_, room, nick, body, time_info) => {
            let delayed = !time_info.delays.is_empty();
            let stamp = time_info
                .delays
                .first()
                .map(|d| d.stamp.0.timestamp())
                .unwrap_or(now);
            let room = room.to_string();
            // A room the server force-joined us into never raised RoomJoined; without this it is not
            // in `rooms` and the UI files it under DMs.
            state.lock().unwrap().rooms.insert(room.clone());
            push_msg(
                state,
                &room,
                ChatMsg { from: nick.to_string(), body, time: stamp, outgoing: false },
                !delayed,
                true,
                store,
            );
        }
        _ => {}
    }
    urgent
}

#[cfg(test)]
mod tests {
    use super::{invited_room, jid_format_error, mention_hit};

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn mention_matches_whole_words_any_case() {
        let n = names(&["seb"]);
        assert!(mention_hit("seb can you tackle", &n));
        assert!(mention_hit("Seb?", &n));
        assert!(mention_hit("ping @seb pls", &n));
        assert!(mention_hit("hey seb.", &n));
        assert!(mention_hit("seb", &n));
    }

    #[test]
    fn mention_ignores_substrings_and_empties() {
        let n = names(&["seb"]);
        assert!(!mention_hit("sebastian is here", &n));
        assert!(!mention_hit("unsebbed", &n));
        assert!(!mention_hit("nothing here", &n));
        assert!(!mention_hit("seb", &names(&[])));
        assert!(!mention_hit("seb", &names(&["   "])));
    }

    #[test]
    fn mention_matches_multi_word_keywords() {
        let n = names(&["home defense", "goon"]);
        assert!(mention_hit("HOME DEFENSE needed in 1DQ", &n));
        assert!(mention_hit("any goon around?", &n));
        assert!(!mention_hit("home defence", &n));
    }

    #[test]
    fn mediated_invite_room_is_the_sender() {
        let msg: xmpp::parsers::message::Message = r#"<message xmlns='jabber:client' from='ops@conference.goonfleet.com' to='me@goonfleet.com'><x xmlns='http://jabber.org/protocol/muc#user'><invite from='fc@goonfleet.com'/></x></message>"#
            .parse::<xmpp::minidom::Element>()
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(invited_room(&msg).as_deref(), Some("ops@conference.goonfleet.com"));
    }

    #[test]
    fn direct_invite_room_is_the_jid_attr() {
        let msg: xmpp::parsers::message::Message = r#"<message xmlns='jabber:client' from='fc@goonfleet.com' to='me@goonfleet.com'><x xmlns='jabber:x:conference' jid='ops@conference.goonfleet.com'/></message>"#
            .parse::<xmpp::minidom::Element>()
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(invited_room(&msg).as_deref(), Some("ops@conference.goonfleet.com"));
    }

    #[test]
    fn plain_message_is_not_an_invite() {
        let msg: xmpp::parsers::message::Message = r#"<message xmlns='jabber:client' from='fc@goonfleet.com' to='me@goonfleet.com'><body>hi</body></message>"#
            .parse::<xmpp::minidom::Element>()
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(invited_room(&msg), None);
    }

    #[test]
    fn valid_bare_jids_pass() {
        assert!(jid_format_error("MyCharacter@goonfleet.com").is_none());
        assert!(jid_format_error("  name@server.com  ").is_none());
    }

    #[test]
    fn malformed_jids_are_rejected() {
        assert!(jid_format_error("").is_some());
        assert!(jid_format_error("goonfleet.com").is_some()); // no username
        assert!(jid_format_error("name@").is_some());
        assert!(jid_format_error("no spaces@server.com").is_some());
        assert!(jid_format_error("@server.com").is_some());
    }
}
