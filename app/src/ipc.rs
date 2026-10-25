//! Wire types: the length-prefixed JSON frames the fleet sign-in helper speaks over its pipes, the
//! actions the web page posts, and the alert feed the alert window and the page show.

use serde::Serialize;

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub enum UiAction {
    Click(crate::app::IntelClick),
    /// Select a system on the app's map and bring it into view.
    ///
    /// Distinct from `Click(IntelClick::System)`, which opens the system window: tapping a system on
    /// the phone's map should move the desktop map to it, not put a dialog in front of whoever is
    /// sitting at it.
    SelectSystem {
        id: i64,
    },
    /// Open a fleet ping's comms link, named by the ping's timestamp.
    ///
    /// A timestamp, not a URL. The page is reachable from the network, and a message that carried a
    /// URL would be a way to make this machine open anything; naming a ping lets the app look up a
    /// link it already has.
    JoinComms {
        ts: i64,
        /// Index into `Ping::fleets`.
        #[serde(default)]
        fleet: usize,
    },
    /// Acknowledge one alert, by `IntelReport::id`.
    AlertAck {
        id: u64,
    },
    Verdict { name: String, hidden: bool },
    /// Open a conversation, resolving the name the way the app's own start dialog does.
    ///
    /// A name and a kind, not a resolved JID: turning "Some Pilot" into a JID needs the configured
    /// domain, and joining a room is a command to the session. Both live on the app side, and the
    /// page should not be able to name an arbitrary JID for the app to join.
    JabberOpen { name: String, room: bool },
    /// Set the in-game destination, for the character the app has selected.
    ///
    /// A system id, and the app decides which character and whether the ESI scope is there. The page
    /// is on the LAN; what it can ask for is "route me here", not "call this endpoint".
    SetDestination { id: i64 },
    /// Set a planned route in the game: one waypoint per entry, in order, clearing what was there.
    SetIngameRoute { waypoints: Vec<i64> },
    /// Drop the route the app set, in the app and in the game.
    ClearIngameRoute,
    /// Add or remove a system from a permanent route-avoidance list.
    AvoidSystem { id: i64, jump: bool, on: bool },
    /// A conversation the page is looking at, so its unread marker clears everywhere rather than
    /// only where it was read.
    JabberRead { jid: String },
    /// Hide a conversation from the list. The same thing the app's tab X does: not a leave, and it
    /// comes back on the next unread message.
    JabberClose { jid: String },
    /// Bookmark a system, or forget it. The same list the app's star writes to.
    Bookmark { id: i64, on: bool },
    /// Save the route the page has built, or replace one of the same name.
    SaveRoute { route: crate::settings::SavedMapRoute },
    /// Forget a saved route by name.
    DeleteRoute { name: String },
    /// Whether routes may use scanned wormhole connections.
    RouteViaWormholes { on: bool },
    /// Send one message to a conversation that is already open.
    JabberSend { jid: String, body: String },
    /// Any edit to notes, tags or their folders. One path for the app and the page.
    Notes(crate::notes::NotesOp),
    /// Where quick note and tag edits go from now on.
    NotesTarget { folder: String },
    /// Recolour a built-in tag, or `None` to go back to its shipped colour.
    DefaultTagColor { id: String, color: Option<[u8; 3]> },
}

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct AlertMsg {
    pub feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub from_you: Vec<Option<u32>>,
    /// Bridge verdict per feed entry, worked out with the distances.
    pub via: Vec<crate::app::JumpVia>,
    /// Which characters each card's numbers belong to.
    pub chars: Vec<crate::app::CardChars>,
    pub status: std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    pub resolved_pilots: std::collections::HashMap<String, i64>,
    pub uncertain: crate::pilot::UncertainPilots,
    pub last_ship: std::collections::HashMap<String, (i64, String, i64)>,
    pub kills: std::collections::HashMap<i64, crate::kills::KillInfo>,
    pub affil: std::collections::HashMap<i64, crate::affiliation::Affil>,
    /// Notes and tags for this feed's systems and pilots only.
    pub notes: crate::notes::NotesView,
}
