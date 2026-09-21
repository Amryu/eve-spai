//! The dashboard's SignalR hub, so a tracked fleet arrives instead of being asked for.
//!
//! The site opens `/api/hubs/fleet`, sends `TrackFleet` with the fleet id, and is then pushed
//! `UpdateFleetData` (the member tree and the composition), `UpdateFleet` (the fleet record, which
//! is how it learns the fleet closed), `UpdateAuditLogs` and `StatsGenerated`. Everything here is
//! read from the Angular bundle and confirmed against the live service.
//!
//! Transport is Server-Sent Events rather than WebSockets. `negotiate` offers all three, SSE is a
//! plain streaming GET that the blocking HTTP client already in the tree can read, and it costs no
//! new dependency and no async runtime. The cost is that client-to-server messages go out as
//! separate POSTs, which for this hub is one message per connection.

use serde::Deserialize;

/// SignalR frames its JSON messages with this, not with newlines.
const RS: u8 = 0x1e;

/// What the hub pushes, in the shapes the bundle's own handlers read.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// `UpdateFleetData`: the member tree and the numbers beside it.
    Data(Box<FleetData>),
    /// `UpdateFleet`: the fleet record. Carries `closedAt` once it is over.
    Fleet(Box<serde_json::Value>),
    /// `StatsGenerated`: the report is worth re-reading.
    Stats,
    /// `UpdateAuditLogs`: the audit log is worth re-reading.
    AuditLogs,
    /// A handshake reply, a ping or anything else that needs no action.
    Idle,
}

/// The payload of `UpdateFleetData`.
///
/// The client builds its tree from this with `buildTree`, which is where the shape comes from:
/// members are flat and the wings reference them by id.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetData {
    #[serde(default)]
    pub composition: HubComposition,
    #[serde(default)]
    pub last_updated: Option<String>,
    /// The dashboard warns the FC a quarter of an hour before its auto-close fires.
    #[serde(default)]
    pub auto_close_warning: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubComposition {
    #[serde(default)]
    pub commander_id: Option<i64>,
    #[serde(default)]
    pub members: Vec<HubMember>,
    #[serde(default)]
    pub wings: Vec<HubWing>,
    #[serde(default)]
    pub meta_data: MetaData,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubMember {
    pub id: i64,
    #[serde(default)]
    pub is_boss: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ship_type_id: i64,
    #[serde(default)]
    pub solar_system_id: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubWing {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub commander_id: Option<i64>,
    #[serde(default)]
    pub squads: Vec<HubSquad>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubSquad {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub commander_id: Option<i64>,
    /// Character ids, looked up in `members`.
    #[serde(default)]
    pub members: Vec<i64>,
}

/// The names the member rows only carry ids for. Keys arrive as JSON object keys, so as strings.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaData {
    #[serde(default)]
    pub ship_types: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub solar_systems: std::collections::HashMap<String, String>,
}

impl MetaData {
    fn ship(&self, id: i64) -> String {
        self.ship_types.get(&id.to_string()).cloned().unwrap_or_default()
    }
}

/// The handshake the server answers before it will accept anything else.
pub fn handshake() -> Vec<u8> {
    frame(br#"{"protocol":"json","version":1}"#)
}

/// Asks the hub to push this fleet.
pub fn track(fleet_id: &str) -> Vec<u8> {
    // Built by hand rather than with serde so the fleet id is the only variable part, and escaped
    // because a fleet id that is not a plain Guid must not be able to break out of the string.
    let id = serde_json::Value::String(fleet_id.to_owned());
    frame(format!(r#"{{"type":1,"target":"TrackFleet","arguments":[{id}]}}"#).as_bytes())
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = body.to_vec();
    out.push(RS);
    out
}

/// Pulls whole SignalR messages out of an SSE byte stream.
///
/// Two framings are stacked here and both have to be undone. SSE puts each payload on a `data: `
/// line and separates events with a blank line; SignalR then terminates each of its own messages
/// with `0x1e`. A read can land anywhere in either, so what is left over stays in the buffer.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Every complete message now in the buffer.
    pub fn drain(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        // SSE events end on a blank line. Anything after the last one is a partial event.
        while let Some(end) = find_event_end(&self.buf) {
            let event = self.buf.drain(..end.1).collect::<Vec<u8>>();
            let text = String::from_utf8_lossy(&event[..end.0]).into_owned();
            for line in text.lines() {
                // `:` opens a comment, which is the server's keepalive.
                let Some(payload) = line.strip_prefix("data:") else { continue };
                for msg in payload.trim_start().split(RS as char) {
                    if msg.trim().is_empty() {
                        continue;
                    }
                    out.push(parse(msg));
                }
            }
        }
        out
    }
}

/// `(payload length, bytes to consume)` for the first complete SSE event in `buf`.
fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
    for (i, w) in buf.windows(2).enumerate() {
        if w == b"\n\n" {
            return Some((i, i + 2));
        }
    }
    for (i, w) in buf.windows(4).enumerate() {
        if w == b"\r\n\r\n" {
            return Some((i, i + 4));
        }
    }
    None
}

/// One SignalR message. Anything unrecognised is `Idle` rather than an error: the hub sends pings
/// and completions this app has no use for, and a stream that dies on one would reconnect forever.
fn parse(msg: &str) -> Event {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(msg) else { return Event::Idle };
    // Type 1 is an invocation. 6 is a ping, 7 a close, and `{}` is the handshake reply.
    if v.get("type").and_then(|t| t.as_i64()) != Some(1) {
        return Event::Idle;
    }
    let target = v.get("target").and_then(|t| t.as_str()).unwrap_or_default();
    let arg = || v.get("arguments").and_then(|a| a.get(0)).cloned();
    match target {
        "UpdateFleetData" => match arg().map(serde_json::from_value::<FleetData>) {
            Some(Ok(d)) => Event::Data(Box::new(d)),
            _ => Event::Idle,
        },
        "UpdateFleet" => match arg() {
            Some(f) if !f.is_null() => Event::Fleet(Box::new(f)),
            _ => Event::Idle,
        },
        "StatsGenerated" => Event::Stats,
        "UpdateAuditLogs" => Event::AuditLogs,
        _ => Event::Idle,
    }
}

impl HubComposition {
    /// The hub's flat members and id references, as the tree the rest of the app renders.
    ///
    /// `ships` supplies the group, which the hub does not send: it names hulls but says nothing
    /// about what kind of hull they are, and the doctrine checks read the group.
    pub fn to_composition(
        &self,
        ships: &std::collections::HashMap<i64, (String, String)>,
    ) -> crate::fleets::model::Composition {
        use crate::fleets::model::{Composition, Member, Squad, SquadId, Wing, WingId};

        let member = |id: i64, role: &str| -> Option<Member> {
            let m = self.members.iter().find(|m| m.id == id)?;
            let (sde_name, group) = ships.get(&m.ship_type_id).cloned().unwrap_or_default();
            let name = match self.meta_data.ship(m.ship_type_id) {
                n if n.trim().is_empty() => sde_name,
                n => n,
            };
            Some(Member {
                character_id: m.id,
                name: m.name.clone(),
                ship_type_id: m.ship_type_id,
                ship_type_name: name,
                ship_group: group,
                role: role.to_owned(),
                // Participation is settled after the fleet, so a live push never carries it.
                pap_count: 0,
            })
        };

        Composition {
            commander: self.commander_id.and_then(|id| member(id, "fleet_commander")),
            wings: self
                .wings
                .iter()
                .map(|w| Wing {
                    id: WingId(w.id),
                    name: w.name.clone(),
                    commander: w.commander_id.and_then(|id| member(id, "wing_commander")),
                    squads: w
                        .squads
                        .iter()
                        .map(|s| Squad {
                            id: SquadId(s.id),
                            name: s.name.clone(),
                            commander: s
                                .commander_id
                                .and_then(|id| member(id, "squad_commander")),
                            members: s
                                .members
                                .iter()
                                .filter(|id| Some(**id) != s.commander_id)
                                .filter_map(|id| member(*id, "squad_member"))
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            flat: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_end_with_the_record_separator() {
        assert_eq!(handshake().last(), Some(&RS));
        assert_eq!(
            String::from_utf8(handshake()).unwrap().trim_end_matches(RS as char),
            r#"{"protocol":"json","version":1}"#
        );
        let t = String::from_utf8(track("00000000-0000-4000-8000-000000000043")).unwrap();
        assert_eq!(
            t.trim_end_matches(RS as char),
            r#"{"type":1,"target":"TrackFleet","arguments":["00000000-0000-4000-8000-000000000043"]}"#
        );
    }

    /// A fleet id is a Guid today, but it arrives as a string and is pasted into JSON, so it has
    /// to be escaped rather than trusted.
    #[test]
    fn a_hostile_fleet_id_cannot_break_out_of_the_frame() {
        let t = String::from_utf8(track(r#"a","x":["#)).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(t.trim_end_matches(RS as char)).expect("still one object");
        assert_eq!(v["arguments"][0], r#"a","x":["#);
        assert!(v.get("x").is_none());
    }

    /// What the live service actually sent after the handshake, byte for byte.
    #[test]
    fn reads_the_opening_of_a_real_stream() {
        let mut d = Decoder::default();
        d.push(b":\n\ndata: {}\n\ndata: {\"type\":6}\n\n");
        assert_eq!(d.drain(), vec![Event::Idle, Event::Idle]);
    }

    /// A read can land anywhere, including mid-number. Nothing may be lost or seen twice.
    #[test]
    fn a_message_split_across_reads_is_still_one_message() {
        let whole = b"data: {\"type\":1,\"target\":\"StatsGenerated\",\"arguments\":[]}\x1e\n\n";
        for cut in 1..whole.len() {
            let mut d = Decoder::default();
            d.push(&whole[..cut]);
            let first = d.drain();
            d.push(&whole[cut..]);
            let mut all = first;
            all.extend(d.drain());
            assert_eq!(all, vec![Event::Stats], "split at {cut}");
        }
    }

    #[test]
    fn reads_the_fleet_tree_the_way_the_site_builds_it() {
        let msg = serde_json::json!({
            "type": 1,
            "target": "UpdateFleetData",
            "arguments": [{
                "lastUpdated": "2026-09-19T19:30:00Z",
                "autoCloseWarning": true,
                "composition": {
                    "commanderId": 1,
                    "members": [
                        {"id": 1, "isBoss": true, "name": "Boss", "shipTypeId": 12013,
                         "solarSystemId": 30004759},
                        {"id": 2, "isBoss": false, "name": "Wing Lead", "shipTypeId": 12013,
                         "solarSystemId": 30004759},
                        {"id": 3, "isBoss": false, "name": "Pilot", "shipTypeId": 11995,
                         "solarSystemId": 30004759},
                        {"id": 4, "isBoss": false, "name": "Squad Lead", "shipTypeId": 11995,
                         "solarSystemId": 30004759}
                    ],
                    "wings": [{
                        "id": 10, "name": "Wing 1", "commanderId": 2,
                        "squads": [{"id": 20, "name": "Squad 1", "commanderId": 4,
                                    "members": [4, 3]}]
                    }],
                    "metaData": {
                        "shipTypes": {"12013": "Devoter", "11995": "Onyx"},
                        "solarSystems": {"30004759": "1DQ1-A"}
                    }
                }
            }]
        });
        let mut d = Decoder::default();
        d.push(format!("data: {msg}\u{1e}\n\n").as_bytes());
        let events = d.drain();
        let Some(Event::Data(data)) = events.first() else {
            panic!("not a data event: {events:?}")
        };
        assert!(data.auto_close_warning);

        let ships = std::collections::HashMap::from([
            (12013_i64, ("Devoter".to_owned(), "Heavy Interdiction Cruiser".to_owned())),
            (11995, ("Onyx".to_owned(), "Heavy Interdiction Cruiser".to_owned())),
        ]);
        let comp = data.composition.to_composition(&ships);
        assert!(!comp.flat);
        assert_eq!(comp.commander.as_ref().map(|m| m.name.as_str()), Some("Boss"));
        let wing = &comp.wings[0];
        assert_eq!(wing.commander.as_ref().map(|m| m.name.as_str()), Some("Wing Lead"));
        let squad = &wing.squads[0];
        assert_eq!(squad.commander.as_ref().map(|m| m.name.as_str()), Some("Squad Lead"));
        // The squad commander is listed in this squad's `members` as well. The site's own
        // `buildTree` does not drop them, so counting them twice here would inflate the fleet and
        // double their hull in every composition table.
        assert_eq!(squad.members.len(), 1);
        assert_eq!(squad.members[0].name, "Pilot");
        assert_eq!(squad.members[0].ship_type_name, "Onyx");
        assert_eq!(squad.members[0].ship_group, "Heavy Interdiction Cruiser");
        assert_eq!(comp.total(), 4);
    }

    /// The hub names hulls but never groups them, and the doctrine checks read the group. A hull
    /// the hub knows and the SDE does not still has to render.
    #[test]
    fn a_hull_the_sde_has_never_heard_of_keeps_its_name() {
        let comp = HubComposition {
            commander_id: Some(1),
            members: vec![HubMember {
                id: 1,
                name: "Boss".to_owned(),
                ship_type_id: 99_999,
                ..Default::default()
            }],
            meta_data: MetaData {
                ship_types: std::collections::HashMap::from([(
                    "99999".to_owned(),
                    "Something New".to_owned(),
                )]),
                ..Default::default()
            },
            ..Default::default()
        };
        let m = comp.to_composition(&Default::default()).commander.expect("a commander");
        assert_eq!(m.ship_type_name, "Something New");
        assert_eq!(m.ship_group, "");
    }

    /// A close arrives on the hub as an `UpdateFleet` carrying `closedAt`, which is how the page
    /// learns the fleet is over without asking.
    #[test]
    fn a_close_arrives_as_a_fleet_update() {
        let mut d = Decoder::default();
        d.push(
            b"data: {\"type\":1,\"target\":\"UpdateFleet\",\"arguments\":\
              [{\"id\":\"abc\",\"closedAt\":\"2026-09-19T19:43:46Z\"}]}\x1e\n\n",
        );
        let events = d.drain();
        let Some(Event::Fleet(f)) = events.first() else { panic!("{events:?}") };
        assert_eq!(f["closedAt"], "2026-09-19T19:43:46Z");
    }

    /// The hub sends message types this app has no use for. None of them may look like an error,
    /// or a healthy stream would be torn down and rebuilt on every ping.
    #[test]
    fn unknown_messages_are_quiet() {
        assert_eq!(parse(r#"{"type":6}"#), Event::Idle);
        assert_eq!(parse(r#"{"type":7,"error":"bye"}"#), Event::Idle);
        assert_eq!(parse("{}"), Event::Idle);
        assert_eq!(parse("not json at all"), Event::Idle);
        assert_eq!(parse(r#"{"type":1,"target":"SomethingNew","arguments":[1]}"#), Event::Idle);
    }
}
