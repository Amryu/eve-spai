use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    /// Browse the rooms a MUC service advertises (disco#items to the service JID).
    DiscoRooms { service: String },
    /// Probe one room's join policy (disco#info to the room JID).
    DiscoRoomInfo { room: String },
    /// Skip the remaining reconnect backoff and try again now.
    RetryNow,
}

/// Reconnect backoff in seconds: fast at first, then settle at five minutes.
const RECONNECT_BACKOFF: &[u64] = &[2, 5, 10, 20, 30, 60, 120, 300];
/// Silence after which we probe the server with a XEP-0199 self-ping.
const PROBE_IDLE: Duration = Duration::from_secs(25);
/// Silence after which the session counts as dead, probe answered or not.
const DEAD_AFTER: Duration = Duration::from_secs(60);

/// Why a connected session ended.
enum SessionEnd {
    /// The user turned Jabber off.
    Disabled,
    /// Connection lost; the reason is shown while backing off.
    Dropped(String),
}

/// Iq ids matched when their results come back on `Event::Iq`.
const DISCO_ROOMS_ID: &str = "spai-disco-rooms";
const DISCO_ROOM_INFO_ID: &str = "spai-room-info";
/// Cap on how many rooms we access-probe per browse, so a huge service can't flood the server.
const DISCO_INFO_CAP: usize = 500;

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

/// Progress of a MUC service room-browse (disco#items).
#[derive(Default, Clone, PartialEq)]
pub enum DirState {
    #[default]
    Idle,
    Loading,
    Ready,
    Error(String),
}

/// Whether the browsing user can join a listed room. Determined by a per-room disco#info: a room
/// is `Restricted` when it advertises `muc_membersonly` or `muc_passwordprotected`. `Unknown` until
/// its probe returns; the UI only offers rooms confirmed `Open`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RoomAccess {
    Unknown,
    Open,
    Restricted,
}

#[derive(Clone)]
pub struct RoomListing {
    pub jid: String,
    pub name: String,
    pub access: RoomAccess,
}

#[derive(Default)]
pub struct JabberState {
    pub enabled: bool,
    pub running: bool,
    pub connected: bool,
    pub status: String,
    /// A connection attempt is pending: the session dropped and we are backing off.
    pub reconnecting: bool,
    /// Consecutive failed attempts since the last successful login.
    pub attempt: u32,
    /// When the next automatic attempt fires, for the countdown next to the retry button.
    pub retry_at: Option<Instant>,
    /// A terminal failure (bad credentials, invalid address, unreachable). The connection stopped
    /// and won't retry; the UI drops back to the login form and shows this.
    pub fatal: Option<String>,
    /// Set once the session reaches `Online`. The chats view waits for this so a failed connect
    /// never flashes it; a later transient drop keeps it set (auto-reconnect handles the blip).
    pub ever_online: bool,
    pub roster: std::collections::BTreeMap<String, Contact>,
    pub presences: std::collections::BTreeMap<String, (Presence, String)>,
    pub rooms: std::collections::BTreeSet<String>,
    /// Rooms we were joined to and then left/kicked while online. Rendered struck-through in the
    /// channel list; a clean connection drop fires no RoomLeft, so a full outage never lands here.
    pub rooms_inaccessible: std::collections::BTreeSet<String>,
    /// Room MOTD (MUC subject) keyed by room bare JID, last-known value.
    pub room_subjects: std::collections::BTreeMap<String, String>,
    /// disco#items browse of the MUC service, with per-room join access.
    pub room_directory: Vec<RoomListing>,
    pub room_directory_state: DirState,
    /// Rooms whose access probe (disco#info) is still outstanding.
    pub room_directory_pending: usize,
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
    let fleet_call = ping.is_some_and(|p| p.is_fleet_call());
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
            // A fleet CALL must still alert even if the FC's rules don't match it — otherwise a
            // real fleet ping goes silent whenever any (non-matching) rule exists. This was the bug.
            None if p.is_fleet_call() => {
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
        if fleet_call {
            // Settings already allowed this fleet ping (not muted, not suppressed, sound on). Play it
            // directly, bypassing the 2s burst cooldown so a preceding sound can never swallow it.
            crate::sound::play(&sound, volume);
        } else {
            crate::sound::play_prio(&sound, prio, volume);
        }
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
    use xmpp::tokio_xmpp::connect::{DnsConfig, StartTlsServerConnector};
    use xmpp::{ClientBuilder, ClientFeature, ClientType};

    // `running` gates respawning in maybe_start_jabber. The xmpp crate panics outright on a closed
    // non-reconnecting stream, so clear it from Drop: an unwind out of this thread then leaves the
    // app able to start a fresh worker instead of wedging forever.
    struct RunGuard(SharedJabber);
    impl Drop for RunGuard {
        fn drop(&mut self) {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            s.running = false;
            s.connected = false;
            s.reconnecting = false;
            s.retry_at = None;
        }
    }

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
    // Our own MUC nick (default = the JID username), used to recognise our reflected room messages.
    let my_nick = bare.node().map(|n| n.to_string()).unwrap_or_default();
    {
        let mut s = state.lock().unwrap();
        s.running = true;
        s.fatal = None;
        s.ever_online = false;
        s.status = "Connecting…".to_owned();
    }
    let _guard = RunGuard(state.clone());
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

    let node = bare.node().unwrap().as_str().to_owned();
    let store = crate::store::Store::open().ok();
    // Rooms to (re)join on every fresh stream. A reconnect starts in no rooms, so without this the
    // client sits online and silent.
    let mut joined: std::collections::BTreeSet<String> = initial_rooms.into_iter().collect();
    // Join/leave commands seen during a session, applied to `joined` once it ends.
    let mut joined_edits: Vec<(String, bool)> = Vec::new();
    let mut attempt = 0usize;

    loop {
        if !state.lock().unwrap().enabled {
            return;
        }
        {
            let mut s = state.lock().unwrap();
            s.retry_at = None;
            s.reconnecting = attempt > 0;
            s.status =
                if attempt > 0 { "Reconnecting…".to_owned() } else { "Connecting…".to_owned() };
        }
        ctx.request_repaint();

        // A wrong password would otherwise loop forever on "Connecting…": the agent retries every
        // error silently. Probe auth first and surface a specific reason.
        let problem = match preflight(
            bare.clone().into(),
            node.clone(),
            password.clone(),
            make_dns(),
        )
        .await
        {
            Preflight::Ok => None,
            Preflight::BadAuth => {
                fail(&state, "Login failed. Check your username and password.".to_owned());
                ctx.request_repaint();
                return;
            }
            Preflight::Unreachable(e) => {
                eprintln!("[jabber] preflight unreachable: {e}");
                Some("Can't reach the server.".to_owned())
            }
            Preflight::Other(e) => Some(format!("Couldn't connect: {e}")),
        };

        let reason = match problem {
            // A server we have never reached is a setup mistake, not an outage: say so instead of
            // retrying silently behind a spinner.
            Some(msg) if !state.lock().unwrap().ever_online => {
                fail(&state, msg);
                ctx.request_repaint();
                return;
            }
            Some(msg) => msg,
            None => {
                let dns = make_dns();
                let mut builder = ClientBuilder::new_with_connector(
                    bare.clone(),
                    &password,
                    StartTlsServerConnector(dns),
                )
                .set_client(ClientType::Bot, "EVE Spai")
                .enable_feature(ClientFeature::ContactList)
                // Advertises bookmarks2+notify, so rooms the server adds us to arrive live instead
                // of only on the next connect.
                .enable_feature(ClientFeature::JoinRooms)
                // Defaults are 300s/300s, which hides a dead TCP for up to ten minutes. The library
                // pings on the soft timeout, so this doubles as the keepalive interval.
                .set_timeouts(xmpp::tokio_xmpp::xmlstream::Timeouts {
                    read_timeout: Duration::from_secs(30),
                    response_timeout: Duration::from_secs(20),
                });
                // Without this the library joins every room as its default nick, "xmpp-rs", which is
                // what the whole channel sees.
                if let Ok(nick) = bare.node().unwrap().as_str().parse::<xmpp::jid::ResourcePart>() {
                    builder = builder.set_default_nick(&nick);
                }
                let mut agent = builder.build();

                let end = session(
                    &mut agent,
                    &bare,
                    &state,
                    resolve.as_ref(),
                    &ping_shared,
                    &mut rx,
                    &cmds,
                    &ctx,
                    store.as_ref(),
                    &my_nick,
                    &joined,
                    &mut joined_edits,
                )
                .await;

                // Dropping the agent only detaches tokio-xmpp's worker, whose reconnector then
                // redials forever in the background. Shut it down before building the next one.
                let _ = tokio::time::timeout(Duration::from_secs(5), agent.disconnect()).await;

                for (room, join) in joined_edits.drain(..) {
                    if join {
                        joined.insert(room);
                    } else {
                        joined.remove(&room);
                    }
                }

                match end {
                    SessionEnd::Disabled => return,
                    SessionEnd::Dropped(msg) => msg,
                }
            }
        };

        let saw_online = {
            let mut s = state.lock().unwrap();
            let was = s.connected;
            s.connected = false;
            was
        };
        if saw_online {
            attempt = 0;
        }
        if !state.lock().unwrap().enabled {
            return;
        }

        let delay = RECONNECT_BACKOFF[attempt.min(RECONNECT_BACKOFF.len() - 1)];
        attempt += 1;
        {
            let mut s = state.lock().unwrap();
            s.reconnecting = true;
            s.attempt = attempt as u32;
            s.status = reason;
            s.retry_at = Some(Instant::now() + Duration::from_secs(delay));
        }
        ctx.request_repaint();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(delay);
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                _ = tick.tick() => {
                    if !state.lock().unwrap().enabled {
                        return;
                    }
                    ctx.request_repaint();
                }
                Some(cmd) = rx.recv() => {
                    if matches!(cmd, Cmd::RetryNow) {
                        break;
                    }
                }
            }
        }
    }
}

/// One connected session. Returns when the user disables Jabber or the stream goes quiet.
#[allow(clippy::too_many_arguments)]
async fn session(
    agent: &mut xmpp::Agent,
    bare: &xmpp::jid::BareJid,
    state: &SharedJabber,
    resolve: &(dyn Fn(&str) -> Option<i64> + Send + Sync),
    ping_shared: &crate::app::SharedPingWindow,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Cmd>,
    cmds: &CmdSender,
    ctx: &egui::Context,
    store: Option<&crate::store::Store>,
    my_nick: &str,
    joined: &std::collections::BTreeSet<String>,
    joined_edits: &mut Vec<(String, bool)>,
) -> SessionEnd {
    use xmpp::jid::BareJid;
    use xmpp::message::send::MessageSettings;
    use xmpp::muc::room::{JoinRoomSettings, LeaveRoomSettings, RoomMessageSettings};

    let mut last_inbound = Instant::now();
    let mut probe_sent = false;
    let mut online_once = false;
    let mut watchdog = tokio::time::interval(Duration::from_secs(5));

    loop {
        if !state.lock().unwrap().enabled {
            return SessionEnd::Disabled;
        }
        tokio::select! {
            // An *empty* event batch is normal (a stanza that produced no high-level event, e.g. the
            // roster reply); it does NOT mean the stream ended.
            events = agent.wait_for_events() => {
                last_inbound = Instant::now();
                probe_sent = false;
                let mut urgent = false;
                let mut background = false;
                let mut came_online = false;
                for event in events {
                    if handle_event(event, state, resolve, ping_shared, cmds, ctx, store, my_nick, &mut came_online) {
                        urgent = true;
                    } else {
                        background = true;
                    }
                }
                if came_online {
                    online_once = true;
                    for r in joined {
                        if let Ok(room) = r.parse::<BareJid>() {
                            agent.join_room(JoinRoomSettings::new(room)).await;
                        }
                    }
                }
                if online_once && !state.lock().unwrap().connected {
                    return SessionEnd::Dropped("Disconnected by the server.".to_owned());
                }
                if urgent {
                    ctx.request_repaint_after(Duration::from_millis(100));
                } else if background {
                    ctx.request_repaint_after(Duration::from_secs(2));
                }
            }
            // tokio-xmpp swallows its own suspend/reconnect, so a dropped stream is invisible at this
            // layer. Watch for silence instead and drive the reconnect ourselves.
            _ = watchdog.tick() => {
                let idle = last_inbound.elapsed();
                if idle >= DEAD_AFTER {
                    return SessionEnd::Dropped("Connection lost.".to_owned());
                }
                if idle >= PROBE_IDLE && !probe_sent {
                    use xmpp::parsers::{iq::Iq, ping::Ping as XmppPing};
                    // XEP-0199 ping to the server itself: any reply, result or error, proves the
                    // stream is alive, and an IQ get must be answered.
                    if let Ok(to) = bare.domain().as_str().parse::<xmpp::jid::Jid>() {
                        let iq = Iq::from_get("spai-keepalive", XmppPing).with_to(to);
                        let _ = agent.send_stanza(iq).await;
                    }
                    probe_sent = true;
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
                            store,
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
                        joined_edits.push((room, true));
                    }
                }
                Cmd::LeaveRoom { room } => {
                    if let Ok(r) = room.parse::<BareJid>() {
                        agent.leave_room(LeaveRoomSettings::new(r)).await;
                        joined_edits.push((room, false));
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
                Cmd::DiscoRooms { service } => {
                    use xmpp::parsers::disco::DiscoItemsQuery;
                    use xmpp::parsers::iq::Iq;
                    match service.parse::<xmpp::jid::Jid>() {
                        Ok(to) => {
                            {
                                let mut s = state.lock().unwrap();
                                s.room_directory.clear();
                                s.room_directory_state = DirState::Loading;
                            }
                            let iq = Iq::from_get(
                                DISCO_ROOMS_ID,
                                DiscoItemsQuery { node: None, rsm: None },
                            )
                            .with_to(to);
                            let _ = agent.send_stanza(iq).await;
                            ctx.request_repaint();
                        }
                        Err(_) => {
                            state.lock().unwrap().room_directory_state =
                                DirState::Error(format!("Bad MUC address: {service}"));
                            ctx.request_repaint();
                        }
                    }
                }
                Cmd::DiscoRoomInfo { room } => {
                    use xmpp::parsers::disco::DiscoInfoQuery;
                    use xmpp::parsers::iq::Iq;
                    if let Ok(to) = room.parse::<xmpp::jid::Jid>() {
                        let iq = Iq::from_get(DISCO_ROOM_INFO_ID, DiscoInfoQuery { node: None })
                            .with_to(to);
                        let _ = agent.send_stanza(iq).await;
                    }
                }
                Cmd::RetryNow => {}
            },
        }
    }
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

#[allow(clippy::too_many_arguments)]
fn handle_event(
    event: xmpp::Event,
    state: &SharedJabber,
    resolve: &(dyn Fn(&str) -> Option<i64> + Send + Sync),
    ping_shared: &crate::app::SharedPingWindow,
    cmds: &CmdSender,
    ctx: &egui::Context,
    store: Option<&crate::store::Store>,
    my_nick: &str,
    came_online: &mut bool,
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
            *came_online = true;
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
            let mut s = state.lock().unwrap();
            let room = room.to_string();
            s.rooms_inaccessible.remove(&room);
            s.rooms.insert(room);
        }
        Event::RoomLeft(room) => {
            eprintln!("[jabber] room left: {room}");
            let mut s = state.lock().unwrap();
            let room = room.to_string();
            s.rooms.remove(&room);
            // Left/kicked while online: keep it in the channel list, struck-through, history intact.
            s.rooms_inaccessible.insert(room);
        }
        Event::RoomSubject(room, _who, subject, _) => {
            if !subject.trim().is_empty() {
                state.lock().unwrap().room_subjects.insert(room.to_string(), subject);
            }
        }
        Event::Iq(iq) => {
            use xmpp::parsers::disco::{DiscoInfoResult, DiscoItemsResult};
            use xmpp::parsers::iq::{IqHeader, IqPayload};
            let (IqHeader { id, from, .. }, data) = iq.split();
            if id == DISCO_ROOMS_ID {
                match data {
                    IqPayload::Result(Some(payload)) => match DiscoItemsResult::try_from(payload) {
                        Ok(res) => {
                            let mut rooms: Vec<RoomListing> = res
                                .items
                                .into_iter()
                                .map(|it| {
                                    let jid = it.jid.to_string();
                                    let name = it.name.unwrap_or_else(|| {
                                        jid.split('@').next().unwrap_or(&jid).to_owned()
                                    });
                                    RoomListing { jid, name, access: RoomAccess::Unknown }
                                })
                                .collect();
                            rooms.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                            // Probe each room's join policy; only confirmed-open rooms are offered.
                            let probe: Vec<String> =
                                rooms.iter().take(DISCO_INFO_CAP).map(|r| r.jid.clone()).collect();
                            {
                                let mut s = state.lock().unwrap();
                                s.room_directory_pending = probe.len();
                                s.room_directory = rooms;
                                s.room_directory_state = DirState::Ready;
                            }
                            for room in probe {
                                let _ = cmds.send(Cmd::DiscoRoomInfo { room });
                            }
                        }
                        Err(e) => {
                            state.lock().unwrap().room_directory_state =
                                DirState::Error(format!("Bad reply: {e}"));
                        }
                    },
                    IqPayload::Error(err) => {
                        state.lock().unwrap().room_directory_state =
                            DirState::Error(format!("Server refused the room list: {err:?}"));
                    }
                    _ => {}
                }
            } else if id == DISCO_ROOM_INFO_ID {
                // Match the probe to its room by the responder JID; a restricted room advertises
                // muc_membersonly or muc_passwordprotected (or errors out entirely).
                let room = from.map(|f| f.to_bare().to_string());
                let access = match &data {
                    IqPayload::Result(Some(payload)) => {
                        match DiscoInfoResult::try_from(payload.clone()) {
                            Ok(info) => {
                                let restricted = info.features.contains("muc_membersonly")
                                    || info.features.contains("muc_passwordprotected");
                                if restricted { RoomAccess::Restricted } else { RoomAccess::Open }
                            }
                            Err(_) => RoomAccess::Restricted,
                        }
                    }
                    _ => RoomAccess::Restricted,
                };
                if let Some(room) = room {
                    let mut s = state.lock().unwrap();
                    if let Some(r) = s.room_directory.iter_mut().find(|r| r.jid == room) {
                        if r.access == RoomAccess::Unknown {
                            r.access = access;
                            s.room_directory_pending = s.room_directory_pending.saturating_sub(1);
                        }
                    }
                }
            }
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
            {
                let mut s = state.lock().unwrap();
                s.rooms_inaccessible.remove(&room);
                s.rooms.insert(room.clone());
            }
            // Our own reflected message (MUC echoes it back under our nick): store it but never
            // notify/sound for it.
            let own = nick.eq_ignore_ascii_case(my_nick);
            push_msg(
                state,
                &room,
                ChatMsg { from: nick.to_string(), body, time: stamp, outgoing: own },
                !delayed && !own,
                !own,
                store,
            );
            // delve911 is a priority channel: its own ship-horn sound, rate-limited so a burst
            // alerts once (the 5-min gate resets on every message, re-arming only after 5 min of
            // quiet).
            #[cfg(feature = "fc-rescue")]
            if !delayed && !own {
                let local = room.split('@').next().unwrap_or(&room);
                if local.eq_ignore_ascii_case("delve911") {
                    let sound_on = state.lock().unwrap().notify_cfg.sound_enabled;
                    if sound_on {
                        crate::sound::play_delve911_alert();
                    }
                }
            }
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
