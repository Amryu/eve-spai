use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::pings::Ping;

const PING_SENDER: &str = "directorbot";
pub const PING_FEED_KEY: &str = "__pings__";

/// The ping bot's own conversation. Its messages are the ping feed, which has its own row and its
/// own badge, so the conversation itself never badges and never reopens a tab the user closed.
/// A room occupant written to privately: `room@service/nick`. Everything else is a bare JID.
pub fn is_room_private(key: &str) -> bool {
    key.contains('/')
}

/// How a contact is named: the name the roster gives, else the address's local part. A roster name
/// that is an address itself (some clients store the full JID as the name) shows its local part too.
pub fn contact_name(name: Option<&str>, jid: &str) -> String {
    let local = |s: &str| s.split('@').next().unwrap_or(s).to_owned();
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) if n.contains('@') && !n.contains(' ') => local(n),
        Some(n) => n.to_owned(),
        None => local(jid),
    }
}

/// How a conversation is named: the account's local part, or "nick (via room)" for someone written
/// to through a room.
pub fn convo_name(key: &str) -> String {
    match key.split_once('/') {
        Some((room, nick)) => format!("{nick} (via {})", room.split('@').next().unwrap_or(room)),
        None => key.split('@').next().unwrap_or(key).to_owned(),
    }
}

pub fn is_ping_sender(jid: &str) -> bool {
    jid.split('@').next().is_some_and(|l| l.eq_ignore_ascii_case(PING_SENDER))
}

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
    pub time: i64,
    pub outgoing: bool,
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub presence: Presence,
    pub status_text: String,
    pub sub: Sub,
}

/// Whose status the server passes on, for one contact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sub {
    /// We see theirs.
    pub theirs: bool,
    /// They see ours.
    pub ours: bool,
    /// We asked to see theirs and they have not answered.
    pub asked: bool,
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
    /// On the server's contact list, asking to see their status.
    AddContact { jid: String },
    /// Off the server's contact list: neither side sees the other's status any more.
    RemoveContact { jid: String },
    /// Answer someone asking to see our status; accepting asks to see theirs back.
    AnswerRequest { jid: String, accept: bool },
    /// Skip the remaining reconnect backoff and try again now.
    RetryNow,
}

/// Reconnect backoff in seconds: fast at first, then settle at five minutes.
const RECONNECT_BACKOFF: &[u64] = &[2, 5, 10, 20, 30, 60, 120, 300];
/// Silence after which we probe the server with a XEP-0199 self-ping.
const PROBE_IDLE: Duration = Duration::from_secs(25);
/// Silence after which the session counts as dead, probe answered or not.
const DEAD_AFTER: Duration = Duration::from_secs(60);

/// How often each room is asked whether we still hold our seat in it (XEP-0410). A room service
/// restarted behind a live connection drops everyone without a word, and nothing else notices.
const SEAT_CHECK: Duration = Duration::from_secs(180);
/// How soon a room whose service could not be reached is asked again.
const SEAT_RETRY: Duration = Duration::from_secs(30);
/// No answer to a seat check within this long counts as the seat being gone.
const SEAT_WAIT: Duration = Duration::from_secs(45);
const SEAT_ID: &str = "spai-seat:";

/// What an answer to a seat check says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seat {
    Held,
    /// The room no longer has us: rejoin.
    Lost,
    /// The room service itself is down: ask again soon.
    Unreachable,
}

/// The seat check an IQ answers, by its id, and what it says. `None` for any other IQ.
pub(crate) fn seat_answer(iq: &xmpp::parsers::iq::Iq) -> Option<(String, Seat)> {
    use xmpp::parsers::{iq::IqPayload, stanza_error::DefinedCondition as C};
    let room = iq.id().strip_prefix(SEAT_ID)?.to_owned();
    let seat = match iq.clone().split().1 {
        // An occupant that does not do pings answers with one of these, relayed by the room: the
        // room still knows us.
        IqPayload::Result(_) => Seat::Held,
        IqPayload::Error(e) => match e.defined_condition {
            C::ServiceUnavailable | C::FeatureNotImplemented => Seat::Held,
            C::RemoteServerNotFound | C::RemoteServerTimeout => Seat::Unreachable,
            _ => Seat::Lost,
        },
        _ => return None,
    };
    Some((room, seat))
}

/// Why a connected session ended.
enum SessionEnd {
    /// The user turned Jabber off.
    Disabled,
    /// Connection lost; the reason is shown while backing off.
    Dropped(String),
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
    pub delve911_sound: String,
    pub delve911_volume: f32,
    /// Rescue Mode is on and unlocked. Off (the default, and before the UI first sets it), the
    /// delve911 room is a room like any other.
    pub delve911_siren: bool,
    pub mention_names: Vec<String>,
    pub mention_ignores_mute: bool,
    pub ping_rules: Vec<crate::settings::PingRule>,
    pub muted: std::collections::BTreeMap<String, i64>,
    pub room_notify: std::collections::BTreeMap<String, crate::settings::RoomNotify>,
    pub push: crate::push::Targets,
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
    /// Rooms the user deliberately left. Nothing here is ever auto-joined or resurrected by a
    /// straggling room message or a bookmark rejoin; only the app asking to join clears an entry.
    pub rooms_left: std::collections::BTreeSet<String>,
    /// Room MOTD (MUC subject) keyed by room bare JID, last-known value.
    pub room_subjects: std::collections::BTreeMap<String, String>,
    pub notify: Vec<(String, bool)>,
    pub pings_unread: bool,
    /// Pings come in since the feed was last read, for the notification box's count.
    pub pings_new: u32,
    pub chats: std::collections::BTreeMap<String, Vec<ChatMsg>>,
    pub unread: std::collections::BTreeSet<String>,
    /// How many unread messages each conversation is carrying.
    pub unread_counts: std::collections::BTreeMap<String, u32>,
    /// Conversations carrying an unread message that named us.
    pub mentions: std::collections::BTreeSet<String>,
    pub pings: Vec<Ping>,
    /// People asking to see our status, not answered yet. The server hands them over again at
    /// every sign-in until they are.
    pub sub_requests: std::collections::BTreeSet<String>,
    /// Conversations a DM just came in on, drained by the UI to open them.
    pub dm_arrived: Vec<String>,
    pub notify_cfg: JabberNotifyCfg,
}

fn is_muted(muted: &std::collections::BTreeMap<String, i64>, key: &str) -> bool {
    muted
        .get(key)
        .is_some_and(|&until| until == i64::MAX || crate::clock::utc().timestamp() < until)
}

fn fire_arrival_notification(
    cfg: &JabberNotifyCfg,
    key: &str,
    ping: Option<&Ping>,
    mention: Option<&ChatMsg>,
    direct: Option<&ChatMsg>,
) {
    let fleet_call = ping.is_some_and(|p| p.is_fleet_call());
    if is_muted(&cfg.muted, key) && !(mention.is_some() && cfg.mention_ignores_mute) {
        return;
    }
    // Only a ping rule with Push ticked pushes; unmatched pings stay on this machine.
    let push = ping.and_then(|p| crate::pings::match_ping_rule(&cfg.ping_rules, p)).is_some_and(|r| r.push && !r.suppress);
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
            // A fleet call must still alert when the FC's rules don't match it, otherwise a real
            // fleet ping goes silent whenever any non-matching rule exists.
            None if p.is_fleet_call() => {
                (false, true, cfg.ping_sound.clone(), 1u8, cfg.ping_volume)
            }
            None => return,
        },
        // Prio 1 so a mention breaks through the cooldown gate that ordinary chat traffic sits behind.
        None if mention.is_some() => {
            (false, true, cfg.mention_sound.clone(), 1u8, cfg.mention_volume)
        }
        // Someone writing to us alone: never lost behind the cooldown of room chatter.
        None if direct.is_some() => (false, true, cfg.msg_sound.clone(), 1u8, cfg.msg_volume),
        None => (false, true, cfg.msg_sound.clone(), 0u8, cfg.msg_volume),
    };
    if push {
        if let Some(p) = ping {
            let (title, body) = ping_push_text(p);
            cfg.push.send(&title, &body, 2);
        }
    }
    if suppress || !notify {
        return;
    }
    let room_allows = ping.is_some() || cfg.room_notify.get(key).is_none_or(|r| r.plays(mention.is_some()));
    if cfg.sound_enabled && room_allows && !sound.is_empty() && !sound.eq_ignore_ascii_case("off") {
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
    if let Some(m) = direct {
        crate::app::notify_os(&format!("Message from {}", convo_name(key)), &m.body);
    }
    if let Some(p @ Ping::Fleet { .. }) = ping.filter(|p| p.is_fleet_call()) {
        crate::app::notify_os("Fleet ping", &fleet_head(p));
    }
}

fn fleet_head(p: &Ping) -> String {
    let fleets = p.fleets();
    if fleets.len() > 1 {
        let fcs: Vec<&str> = fleets.iter().map(|f| f.fc.as_str()).collect();
        return format!("{} fleets: {}", fleets.len(), fcs.join(", "));
    }
    match fleets.first() {
        Some(f) => match &f.doctrine {
            Some(d) => format!("FC: {} \u{00B7} {d}", f.fc),
            None => format!("FC: {}", f.fc),
        },
        None => String::new(),
    }
}

fn ping_push_text(p: &Ping) -> (String, String) {
    match p {
        Ping::Fleet { description, raw, .. } => {
            let head = fleet_head(p);
            let text = if description.trim().is_empty() { raw.trim() } else { description.trim() };
            ("Fleet ping".to_owned(), if text.is_empty() { head } else { format!("{head}\n{text}") })
        }
        Ping::Plain { text, .. } => ("Ping".to_owned(), text.trim().to_owned()),
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
    let _ = std::thread::Builder::new().name("jabber".into()).spawn(move || {
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

/// Joins `room` again after a seat check found us gone from it.
async fn rejoin(agent: &mut xmpp::Agent, state: &SharedJabber, room: &str) {
    if state.lock().unwrap().rooms_left.contains(room) {
        return;
    }
    if let Ok(jid) = room.parse::<xmpp::jid::BareJid>() {
        agent.join_room(xmpp::muc::room::JoinRoomSettings::new(jid)).await;
    }
}

/// A room whose service is down shows struck through until it answers and we are back in.
pub(crate) fn seat_unreachable(state: &SharedJabber, room: &str) {
    let mut s = state.lock().unwrap();
    if s.rooms.remove(room) {
        s.rooms_inaccessible.insert(room.to_owned());
    }
}

/// Self-presence from the MUC proves we are in the room. A room the user left only gets here when
/// the app asked to join it again (a hand join or an invite).
pub(crate) fn note_room_joined(state: &SharedJabber, room: &str) {
    let mut s = state.lock().unwrap();
    s.rooms_inaccessible.remove(room);
    s.rooms_left.remove(room);
    s.rooms.insert(room.to_owned());
}

/// A room the server force-joined us into never raised `RoomJoined`; without this it is not in
/// `rooms` and the UI files its history under DMs. Returns false for a room we deliberately left,
/// where the same rule would let a message still in flight past the leave resurrect it.
pub(crate) fn note_room_seen(state: &SharedJabber, room: &str) -> bool {
    let mut s = state.lock().unwrap();
    if s.rooms_left.contains(room) {
        return false;
    }
    s.rooms_inaccessible.remove(room);
    s.rooms.insert(room.to_owned());
    true
}

/// Leaving is permanent until the user rejoins or the server puts us back in.
pub(crate) fn note_room_left(state: &SharedJabber, room: &str) {
    let mut s = state.lock().unwrap();
    s.rooms.remove(room);
    s.rooms_inaccessible.remove(room);
    s.rooms_left.insert(room.to_owned());
}

/// A one-to-one message, stored and badged. `key` is the sender's bare JID.
///
/// One that waited on the server while this was offline is still one nobody has read, so it is
/// unread too, quietly: the delay is what keeps the ping bot's backlog from screeching at startup,
/// and a person's offline messages are the ones that must not be missed. Without that, one from a
/// conversation closed earlier had no row, no tab and no badge at all.
pub(crate) fn receive_direct(
    state: &SharedJabber,
    key: &str,
    body: String,
    stamp: i64,
    delayed: bool,
    store: Option<&crate::store::Store>,
) {
    let bot = is_ping_sender(key);
    push_msg(
        state,
        key,
        ChatMsg { from: key.to_owned(), body, time: stamp, outgoing: false },
        !delayed && !bot,
        false,
        store,
    );
    if !bot {
        // Every DM opens its conversation, the delayed ones from while we were away too.
        state.lock().unwrap().dm_arrived.push(key.to_owned());
    }
    if delayed && !bot {
        let mut s = state.lock().unwrap();
        s.unread.insert(key.to_owned());
        *s.unread_counts.entry(key.to_owned()).or_default() += 1;
    }
}

fn push_msg(
    state: &SharedJabber,
    key: &str,
    msg: ChatMsg,
    mark_unread: bool,
    check_mention: bool,
    store: Option<&crate::store::Store>,
) {
    crate::theme::note_text(&msg.from);
    crate::theme::note_text(&msg.body);
    if let Some(s) = store {
        s.add_chat(key, &msg.from, &msg.body, msg.time, msg.outgoing);
    }
    let fire = {
        let mut s = state.lock().unwrap();
        let mention = check_mention && mention_hit(&msg.body, &s.notify_cfg.mention_names);
        let mentioned = mention.then(|| msg.clone());
        // A DM: not ours, not the ping bot's, and not in a room.
        let direct = (!check_mention && !msg.outgoing && !is_ping_sender(key) && !s.rooms.contains(key)).then(|| msg.clone());
        let conv = s.chats.entry(key.to_owned()).or_default();
        conv.push(msg);
        let n = conv.len();
        if n > 1000 {
            conv.drain(0..n - 1000);
        }
        if mark_unread {
            s.unread.insert(key.to_owned());
            *s.unread_counts.entry(key.to_owned()).or_default() += 1;
            if mention {
                s.mentions.insert(key.to_owned());
            }
            s.notify.push((key.to_owned(), false));
            Some((s.notify_cfg.clone(), mentioned, direct))
        } else {
            None
        }
    };
    if let Some((cfg, mentioned, direct)) = fire {
        fire_arrival_notification(&cfg, key, None, mentioned.as_ref(), direct.as_ref());
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
    // Seat checks sent and not yet answered, and rooms whose service was down at the last one.
    let mut seat_pending: std::collections::HashMap<String, Instant> = Default::default();
    let mut unreachable: std::collections::BTreeSet<String> = Default::default();
    let mut next_seat_check = Instant::now() + SEAT_CHECK;

    loop {
        if !state.lock().unwrap().enabled {
            return SessionEnd::Disabled;
        }
        tokio::select! {
            // An *empty* event batch is normal (a stanza that produced no high-level event, e.g. the
            // roster reply); it does not mean the stream ended.
            events = agent.wait_for_events() => {
                last_inbound = Instant::now();
                probe_sent = false;
                let mut urgent = false;
                let mut background = false;
                let mut came_online = false;
                let mut seats: Vec<(String, Seat)> = Vec::new();
                for event in events {
                    if let xmpp::Event::Iq(iq) = &event {
                        if let Some(answer) = seat_answer(iq) {
                            seats.push(answer);
                            continue;
                        }
                    }
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
                            // Asked for by the app, so its RoomJoined is not a bookmark rejoin.
                            state.lock().unwrap().rooms_left.remove(r.as_str());
                            agent.join_room(JoinRoomSettings::new(room)).await;
                        }
                    }
                }
                for (room, seat) in seats {
                    seat_pending.remove(&room);
                    match seat {
                        Seat::Held => {
                            if unreachable.remove(&room) {
                                eprintln!("[jabber] {room} answers again");
                            }
                        }
                        Seat::Lost => {
                            eprintln!("[jabber] no longer in {room}, rejoining");
                            rejoin(agent, state, &room).await;
                        }
                        Seat::Unreachable => {
                            if unreachable.insert(room.clone()) {
                                eprintln!("[jabber] {room}: room service unreachable, retrying");
                                seat_unreachable(state, &room);
                            }
                            next_seat_check = next_seat_check.min(Instant::now() + SEAT_RETRY);
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
                if online_once {
                    let now = Instant::now();
                    // Unanswered: whatever happened, being in the room again is the fix.
                    let silent: Vec<String> = seat_pending.iter().filter(|(_, at)| now - **at >= SEAT_WAIT).map(|(r, _)| r.clone()).collect();
                    for room in silent {
                        seat_pending.remove(&room);
                        eprintln!("[jabber] {room} did not answer a seat check, rejoining");
                        rejoin(agent, state, &room).await;
                    }
                    if now >= next_seat_check {
                        next_seat_check = now + if unreachable.is_empty() { SEAT_CHECK } else { SEAT_RETRY };
                        let rooms: std::collections::BTreeSet<String> = {
                            let s = state.lock().unwrap();
                            s.rooms.iter().chain(&unreachable).filter(|r| !s.rooms_left.contains(*r)).cloned().collect()
                        };
                        for room in rooms {
                            if seat_pending.contains_key(&room) {
                                continue;
                            }
                            use xmpp::parsers::{iq::Iq, ping::Ping as XmppPing};
                            if let Ok(to) = format!("{room}/{my_nick}").parse::<xmpp::jid::Jid>() {
                                let iq = Iq::from_get(format!("{SEAT_ID}{room}"), XmppPing).with_to(to);
                                let _ = agent.send_stanza(iq).await;
                                seat_pending.insert(room, now);
                            }
                        }
                    }
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
                    let sent = match to.split_once('/') {
                        // Back through the room it came from.
                        Some((room, nick)) => match (room.parse::<BareJid>(), nick.parse::<xmpp::RoomNick>()) {
                            (Ok(room), Ok(nick)) => {
                                agent
                                    .send_room_private_message(xmpp::muc::private_message::RoomPrivateMessageSettings::new(room, nick, &body))
                                    .await;
                                true
                            }
                            _ => false,
                        },
                        None => match to.parse::<BareJid>() {
                            Ok(recipient) => {
                                agent.send_message(MessageSettings { recipient, message: &body, lang: None }).await;
                                true
                            }
                            Err(_) => false,
                        },
                    };
                    if sent {
                        let now = crate::clock::utc().timestamp();
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
                        state.lock().unwrap().rooms_left.remove(&room);
                        joined_edits.push((room, true));
                    }
                }
                Cmd::LeaveRoom { room } => {
                    if let Ok(r) = room.parse::<BareJid>() {
                        agent.leave_room(LeaveRoomSettings::new(r)).await;
                        note_room_left(state, &room);
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
                Cmd::AddContact { jid } => {
                    if let Ok(bare) = jid.parse::<BareJid>() {
                        roster_set(agent, &bare, false).await;
                        let _ = agent.send_stanza(presence_to(xmpp::parsers::presence::Type::Subscribe, &bare)).await;
                    }
                }
                Cmd::RemoveContact { jid } => {
                    if let Ok(bare) = jid.parse::<BareJid>() {
                        roster_set(agent, &bare, true).await;
                    }
                    state.lock().unwrap().sub_requests.remove(&jid);
                }
                Cmd::AnswerRequest { jid, accept } => {
                    use xmpp::parsers::presence::Type;
                    if let Ok(bare) = jid.parse::<BareJid>() {
                        let answer = if accept { Type::Subscribed } else { Type::Unsubscribed };
                        let _ = agent.send_stanza(presence_to(answer, &bare)).await;
                        let theirs = state.lock().unwrap().roster.get(&jid).is_some_and(|c| c.sub.theirs || c.sub.asked);
                        if accept && !theirs {
                            roster_set(agent, &bare, false).await;
                            let _ = agent.send_stanza(presence_to(Type::Subscribe, &bare)).await;
                        }
                    }
                    state.lock().unwrap().sub_requests.remove(&jid);
                }
                Cmd::RetryNow => {}
            },
        }
    }
}

fn presence_to(ty: xmpp::parsers::presence::Type, to: &xmpp::jid::BareJid) -> xmpp::parsers::presence::Presence {
    xmpp::parsers::presence::Presence::new(ty).with_to(to.clone())
}

/// Adds `jid` to the server's contact list, or takes it off.
async fn roster_set(agent: &mut xmpp::Agent, jid: &xmpp::jid::BareJid, remove: bool) {
    use xmpp::parsers::iq::Iq;
    use xmpp::parsers::roster::{Ask, Item, Roster, Subscription};
    let item = Item {
        jid: jid.clone(),
        name: None,
        subscription: if remove { Subscription::Remove } else { Subscription::None },
        ask: Ask::None,
        groups: Vec::new(),
        approved: None,
    };
    let iq = Iq::from_set(format!("spai-roster-{jid}"), Roster { ver: None, items: vec![item] });
    let _ = agent.send_stanza(iq).await;
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
            | Event::Iq(_)
    );
    let now = crate::clock::utc().timestamp();
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
                name: None,
                groups: Vec::new(),
                presence: Presence::default(),
                status_text: String::new(),
                sub: Sub::default(),
            });
            entry.name = item.name.clone();
            entry.groups = groups;
            use xmpp::parsers::roster::{Ask, Subscription as S};
            entry.sub = Sub {
                theirs: matches!(item.subscription, S::To | S::Both),
                ours: matches!(item.subscription, S::From | S::Both),
                asked: item.ask == Ask::Subscribe,
            };
            if let Some((pres, st)) = known {
                entry.presence = pres;
                entry.status_text = st;
            }
        }
        Event::ContactRemoved(item) => {
            state.lock().unwrap().roster.remove(&item.jid.to_string());
        }
        Event::Presence(p) => {
            use xmpp::parsers::presence::Type;
            // Subscription traffic says nothing of anyone's status and must not mark them offline.
            match (p.type_.clone(), &p.from) {
                (Type::Subscribe, Some(from)) => {
                    let who = from.to_bare().to_string();
                    let mut s = state.lock().unwrap();
                    // Already shown ours: the server only asks again to be sure.
                    if s.roster.get(&who).is_some_and(|c| c.sub.ours) {
                        return true;
                    }
                    if s.sub_requests.insert(who.clone()) {
                        drop(s);
                        crate::app::notify_os("Contact request", &format!("{} wants to see your online status", convo_name(&who)));
                    }
                    // Shown at once: someone is waiting on an answer.
                    return true;
                }
                (Type::Subscribed | Type::Unsubscribe | Type::Unsubscribed | Type::Probe | Type::Error, _) => return urgent,
                _ => {}
            }
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
            // Offline/history messages carry a <delay/>. They are stored but not sounded or
            // badged, or the backlog of missed pings screeches on startup.
            let delayed = !time_info.delays.is_empty();
            let stamp = time_info
                .delays
                .first()
                .map(|d| d.stamp.0.timestamp())
                .unwrap_or(now);
            // Key by the bare JID (no /resource): outgoing DMs and presences use the bare form, and
            // the UI's DM list only surfaces conversations whose key is a valid bare JID.
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
                            s.pings_new = s.pings_new.saturating_add(1);
                            s.notify.push((PING_FEED_KEY.to_owned(), true));
                            s.pings.last().cloned().map(|p| (s.notify_cfg.clone(), p))
                        } else {
                            None
                        }
                    };
                    if let Some((cfg, ping)) = fire {
                        fire_arrival_notification(&cfg, PING_FEED_KEY, Some(&ping), None, None);
                        if crate::pings::ping_alerts(&cfg.ping_rules, &ping) {
                            push_ping_window(ping_shared, ctx, &ping);
                        }
                    }
                }
            }
            receive_direct(state, &key, body, stamp, delayed, store);
        }
        // An edit arrives as a message of its own; losing it would lose what was said.
        Event::ChatMessageCorrection(_, from, body, time_info) => {
            let stamp = time_info.delays.first().map(|d| d.stamp.0.timestamp()).unwrap_or(now);
            receive_direct(state, &from.to_string(), format!("{body} (edited)"), stamp, !time_info.delays.is_empty(), store);
        }
        // Someone in a room writing to us alone, through the room: a DM like any other.
        Event::RoomPrivateMessage(_, room, nick, body, time_info) => {
            let stamp = time_info.delays.first().map(|d| d.stamp.0.timestamp()).unwrap_or(now);
            receive_direct(state, &format!("{room}/{nick}"), body, stamp, !time_info.delays.is_empty(), store);
        }
        Event::RoomPrivateMessageCorrection(_, room, nick, body, time_info) => {
            let stamp = time_info.delays.first().map(|d| d.stamp.0.timestamp()).unwrap_or(now);
            receive_direct(state, &format!("{room}/{nick}"), format!("{body} (edited)"), stamp, !time_info.delays.is_empty(), store);
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
            let room = room.to_string();
            // xmpp-rs rejoins every room bookmarked on the account at connect (the XEP-0048 store
            // even ignores `autojoin`), and another client or the server may have bookmarked it.
            // A room the user left that we did not ask to join again is one of those: leave it.
            if state.lock().unwrap().rooms_left.contains(&room) {
                eprintln!("[jabber] bookmark rejoined {room} after a leave, leaving again");
                let _ = cmds.send(Cmd::LeaveRoom { room });
            } else {
                eprintln!("[jabber] room joined: {room}");
                note_room_joined(state, &room);
            }
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
        Event::RoomMessage(_, room, nick, body, time_info) => {
            let delayed = !time_info.delays.is_empty();
            let stamp = time_info
                .delays
                .first()
                .map(|d| d.stamp.0.timestamp())
                .unwrap_or(now);
            let room = room.to_string();
            if !note_room_seen(state, &room) {
                return false;
            }
            // Our own reflected message (MUC echoes it back under our nick): store it but never
            // notify/sound for it.
            let own = nick.eq_ignore_ascii_case(my_nick);
            // Read before the body is stored: "he is safe" ends a rescue, so it is the one delve911
            // line that must not sound the siren.
            let stand_down = crate::rescue::is_safe_call(&body);
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
            if !delayed && !own && !stand_down {
                let local = room.split('@').next().unwrap_or(&room);
                if local.eq_ignore_ascii_case("delve911") {
                    let (sound_on, spec, vol) = {
                        let s = state.lock().unwrap();
                        let c = &s.notify_cfg;
                        (c.sound_enabled && c.delve911_siren, c.delve911_sound.clone(), c.delve911_volume)
                    };
                    if sound_on {
                        crate::sound::play_delve911_alert(&spec, vol);
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
    #[test]
    fn a_contact_named_by_its_address_shows_the_local_part() {
        assert_eq!(super::contact_name(Some("leonora_d@goonfleet.com"), "leonora_d@goonfleet.com"), "leonora_d");
        assert_eq!(super::contact_name(Some("Leonora"), "leonora_d@goonfleet.com"), "Leonora");
        assert_eq!(super::contact_name(None, "leonora_d@goonfleet.com"), "leonora_d");
        assert_eq!(super::contact_name(Some("  "), "leonora_d@goonfleet.com"), "leonora_d");
    }

    #[test]
    fn a_seat_check_answer_says_whether_we_are_still_in_the_room() {
        use super::{seat_answer, Seat};
        use xmpp::parsers::iq::Iq;
        use xmpp::parsers::stanza_error::{DefinedCondition as C, ErrorType, StanzaError};
        let room = "scouts@conference.example.com";
        let id = format!("spai-seat:{room}");
        let err = |c: C| Iq::from_error(id.clone(), StanzaError::new(ErrorType::Cancel, c, "en", ""));
        let answer = |iq: Iq| seat_answer(&iq).map(|(r, s)| (r == room).then_some(s)).flatten();
        assert_eq!(answer(Iq::from_result(id.clone(), None::<xmpp::parsers::disco::DiscoInfoResult>)), Some(Seat::Held));
        assert_eq!(answer(err(C::ServiceUnavailable)), Some(Seat::Held), "our client answers pings like this, relayed");
        assert_eq!(answer(err(C::NotAcceptable)), Some(Seat::Lost), "the room forgot us");
        assert_eq!(answer(err(C::ItemNotFound)), Some(Seat::Lost));
        assert_eq!(answer(err(C::RemoteServerNotFound)), Some(Seat::Unreachable), "the room service is down");
        assert_eq!(answer(err(C::RemoteServerTimeout)), Some(Seat::Unreachable));
        assert_eq!(seat_answer(&Iq::from_error("spai-keepalive", StanzaError::new(ErrorType::Cancel, C::NotAcceptable, "en", ""))), None, "not a seat check");
    }

    use super::{invited_room, jid_format_error, mention_hit};

    fn joined(state: &super::SharedJabber, room: &str) -> Vec<super::Cmd> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let ping: crate::app::SharedPingWindow = Default::default();
        let mut online = false;
        let jid: xmpp::jid::BareJid = room.parse().unwrap();
        let resolve = |_: &str| None;
        super::handle_event(
            xmpp::Event::RoomJoined(jid),
            state,
            &resolve,
            &ping,
            &tx,
            &egui::Context::default(),
            None,
            "me",
            &mut online,
        );
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    fn event(state: &super::SharedJabber, ev: xmpp::Event) {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let ping: crate::app::SharedPingWindow = Default::default();
        let mut online = false;
        let resolve = |_: &str| None;
        super::handle_event(ev, state, &resolve, &ping, &tx, &egui::Context::default(), None, "me", &mut online);
    }

    fn presence(xml: &str) -> xmpp::Event {
        xmpp::Event::Presence(xml.parse::<xmpp::minidom::Element>().unwrap().try_into().unwrap())
    }

    /// A request to see our status is a question for the user, not news of anyone's status; its
    /// answers are not either.
    #[test]
    fn a_contact_request_is_held_and_says_nothing_of_status() {
        let state: super::SharedJabber = Default::default();
        event(&state, presence(r#"<presence xmlns='jabber:client' from='friend@goonfleet.com/eve' type='available'/>"#.replace(" type='available'", "").as_str()));
        assert_eq!(state.lock().unwrap().presences.get("friend@goonfleet.com").map(|p| p.0), Some(super::Presence::Online));
        event(&state, presence(r#"<presence xmlns='jabber:client' from='friend@goonfleet.com' type='subscribe'/>"#));
        event(&state, presence(r#"<presence xmlns='jabber:client' from='friend@goonfleet.com' type='unsubscribed'/>"#));
        let st = state.lock().unwrap();
        assert!(st.sub_requests.contains("friend@goonfleet.com"));
        assert_eq!(st.presences.get("friend@goonfleet.com").map(|p| p.0), Some(super::Presence::Online), "not marked offline");
    }

    #[test]
    fn a_roster_entry_says_who_sees_whose_status() {
        let state: super::SharedJabber = Default::default();
        let item = |xml: &str| -> xmpp::parsers::roster::Item { xml.parse::<xmpp::minidom::Element>().unwrap().try_into().unwrap() };
        event(&state, xmpp::Event::ContactAdded(item(r#"<item xmlns='jabber:iq:roster' jid='a@goonfleet.com' subscription='both'/>"#)));
        event(&state, xmpp::Event::ContactAdded(item(r#"<item xmlns='jabber:iq:roster' jid='b@goonfleet.com' subscription='from' ask='subscribe'/>"#)));
        let st = state.lock().unwrap();
        assert_eq!(st.roster["a@goonfleet.com"].sub, super::Sub { theirs: true, ours: true, asked: false });
        assert_eq!(st.roster["b@goonfleet.com"].sub, super::Sub { theirs: false, ours: true, asked: true });
    }

    #[test]
    fn only_the_ping_bot_is_the_ping_sender() {
        assert!(super::is_ping_sender("directorbot@goonfleet.com"));
        assert!(super::is_ping_sender("DirectorBot@goonfleet.com"));
        assert!(!super::is_ping_sender("director@goonfleet.com"));
        assert!(!super::is_ping_sender("delve911@conference.goonfleet.com"));
    }

    #[test]
    fn a_bookmark_rejoin_does_not_undo_a_leave() {
        const LEFT: &str = "delve911@conference.example.org";
        const OTHER: &str = "ops@conference.example.org";
        let state: super::SharedJabber = Default::default();
        state.lock().unwrap().rooms_left.insert(LEFT.to_owned());

        let cmds = joined(&state, LEFT);
        assert!(matches!(&cmds[..], [super::Cmd::LeaveRoom { room }] if room == LEFT), "not left again");
        let st = state.lock().unwrap();
        assert!(!st.rooms.contains(LEFT) && st.rooms_left.contains(LEFT));
        drop(st);

        assert!(joined(&state, OTHER).is_empty());
        assert!(state.lock().unwrap().rooms.contains(OTHER), "an ordinary join must still land");
    }

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
