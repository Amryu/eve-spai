//! Wire types: the length-prefixed JSON frames the fleet sign-in helper speaks over its pipes, the
//! actions the web page posts, and the alert feed the alert window and the page show.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{self, Read, Write};

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

#[cfg_attr(not(any(test, feature = "fleet-auth")), allow(dead_code, reason = "only the sign-in helper sends, and it is built with fleet-auth"))]
pub fn send<T: Serialize, W: Write>(w: &mut W, msg: &T) -> io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

pub fn recv<T: DeserializeOwned, R: Read>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip() {
        let mut buf: Vec<u8> = Vec::new();
        send(&mut buf, &UiAction::Click(crate::app::IntelClick::System(30000142))).unwrap();
        assert!(buf.len() > 4);
        match recv::<UiAction, _>(&mut Cursor::new(buf)).unwrap() {
            UiAction::Click(crate::app::IntelClick::System(id)) => assert_eq!(id, 30000142),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn recv_truncated_is_err() {
        let buf = vec![0u8, 0, 0, 10, b'x'];
        let r: io::Result<UiAction> = recv(&mut Cursor::new(buf));
        assert!(r.is_err());
    }

    /// The page reads the feed as JSON, and a vector that arrives shifted puts one character's
    /// distance under another character's portrait.
    #[test]
    fn the_alert_feed_survives_json_index_aligned() {
        let hop = |name: &str, id: i64, jumps: Option<u32>| crate::app::CharHop { name: name.to_owned(), id, jumps, via: crate::app::JumpVia::Gates };
        let chars = vec![
            crate::app::CardChars { hops: vec![hop("Scout", 90_000_002, Some(1)), hop("Amryu", 90_000_001, Some(4))], selected: Some(1), ..Default::default() },
            crate::app::CardChars::default(),
        ];
        let via = vec![crate::app::JumpVia::Gates, crate::app::JumpVia::BridgeShorter(4), crate::app::JumpVia::BridgeOnly];
        let mut report = crate::intel::IntelReport::default();
        report.probes = Some(crate::intel::Probes::Combat);
        let msg = AlertMsg {
            feed: vec![(report, crate::settings::Severity::Danger)],
            from_you: vec![Some(7)],
            via: via.clone(),
            chars: chars.clone(),
            status: Default::default(),
            resolved_pilots: std::collections::HashMap::from([("X".to_owned(), 42i64)]),
            uncertain: Default::default(),
            last_ship: Default::default(),
            kills: Default::default(),
            affil: Default::default(),
            notes: Default::default(),
        };
        let back: AlertMsg = serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(back.via, via);
        assert_eq!(back.chars, chars);
        assert_eq!(back.chars[0].selected, Some(1), "the badge order survives");
        assert_eq!(back.from_you, vec![Some(7)]);
        assert_eq!(back.feed[0].0.probes, Some(crate::intel::Probes::Combat));
        assert_eq!(back.resolved_pilots.get("X"), Some(&42));
    }
}
