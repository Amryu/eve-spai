//! The dashboard's wire types, shaped to the JSON it actually sends.
//!
//! Field names are the server's, so every struct carries `rename_all = "camelCase"` and the tests
//! compare against captured bodies rather than against what looks right.

use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($name:ident, $inner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

// Separate types because the id spaces overlap: 60 is the Torp Bombers setup and also the Entosis
// tag, and mixing them silently posts a fleet with the wrong doctrine.
id_newtype!(SetupId, i32, "A fleet setup, the dashboard's word for a doctrine.");
id_newtype!(TagId, i32, "A fleet tag.");
id_newtype!(ChannelId, i32, "A comms channel: mumble, logi or boost. Each has its own id space.");
id_newtype!(GroupId, i32, "A SIG.");
id_newtype!(WingId, i64, "A wing of a tracked fleet.");
id_newtype!(SquadId, i64, "A squad inside a wing.");
id_newtype!(DistributionId, i32, "A squad distribution template.");

/// A fleet, identified by the uuid that appears in `/fleet/overview/<uuid>` links.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FleetId(pub String);

impl FleetId {
    /// The first group of the uuid, which is as much as anyone reads.
    pub fn short(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }
}

impl std::fmt::Display for FleetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An `{id, label}` pair, which is how the API hands back anything searchable.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Labelled {
    pub id: i64,
    pub label: String,
}

/// What the account may do, from `is-authenticated`.
///
/// A closed enum for the ones the UI gates on, beside the raw strings, so a permission the server
/// adds later does not break the parse or vanish from the identity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Perm {
    AccessFleet,
    AccessFleetModule,
    StartFleet,
    InviteMember,
    KickMember,
    MoveMember,
    ManageFleetSnowflakes,
    FlagFleet,
    AccessPayouts,
    AccessCommanderStats,
    AccessLogiAnchorStats,
    AccessStatisticsModule,
}

impl Perm {
    pub fn as_str(self) -> &'static str {
        match self {
            Perm::AccessFleet => "accessFleet",
            Perm::AccessFleetModule => "accessFleetModule",
            Perm::StartFleet => "startFleet",
            Perm::InviteMember => "inviteMember",
            Perm::KickMember => "kickMember",
            Perm::MoveMember => "moveMember",
            Perm::ManageFleetSnowflakes => "manageFleetSnowflakes",
            Perm::FlagFleet => "flagFleet",
            Perm::AccessPayouts => "accessPayouts",
            Perm::AccessCommanderStats => "accessCommanderStats",
            Perm::AccessLogiAnchorStats => "accessLogiAnchorStats",
            Perm::AccessStatisticsModule => "accessStatisticsModule",
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: String,
    pub command_group: String,
    #[serde(default)]
    pub sigs: Vec<Labelled>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

impl Identity {
    pub fn can(&self, p: Perm) -> bool {
        self.permissions.iter().any(|s| s == p.as_str())
    }
}

/// One of the account's characters, from `/api/v1/character`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCharacter {
    pub id: i64,
    pub name: String,
    pub corporation_id: i64,
    #[serde(default)]
    pub is_hidden: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupItem {
    pub id: SetupId,
    pub name: String,
    pub minimal_opsec_level_description: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub is_default: bool,
}

/// A comms channel. `is_in_use` is what makes picking a free one possible.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelItem {
    pub id: ChannelId,
    pub name: String,
    #[serde(default)]
    pub is_in_use: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagItem {
    pub id: TagId,
    pub name: String,
    #[serde(default)]
    pub colour_class: String,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub is_strategic: bool,
}

/// Who gets called out in a fleet: the FC, a VIP, the logi anchor.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum SnowflakeType {
    #[default]
    Fc,
    Vip,
    LogiAnchor,
    Backseat,
    Hunter,
}

impl SnowflakeType {
    pub const ALL: [SnowflakeType; 5] = [
        SnowflakeType::Fc,
        SnowflakeType::Vip,
        SnowflakeType::LogiAnchor,
        SnowflakeType::Backseat,
        SnowflakeType::Hunter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SnowflakeType::Fc => "FC",
            SnowflakeType::Vip => "VIP",
            SnowflakeType::LogiAnchor => "LA",
            SnowflakeType::Backseat => "Backseat",
            SnowflakeType::Hunter => "Hunter",
        }
    }
}

impl From<SnowflakeType> for u8 {
    fn from(t: SnowflakeType) -> u8 {
        match t {
            SnowflakeType::Fc => 0,
            SnowflakeType::Vip => 1,
            SnowflakeType::LogiAnchor => 2,
            SnowflakeType::Backseat => 3,
            SnowflakeType::Hunter => 4,
        }
    }
}

impl TryFrom<u8> for SnowflakeType {
    type Error = String;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Ok(match v {
            0 => SnowflakeType::Fc,
            1 => SnowflakeType::Vip,
            2 => SnowflakeType::LogiAnchor,
            3 => SnowflakeType::Backseat,
            4 => SnowflakeType::Hunter,
            other => return Err(format!("unknown snowflake type {other}")),
        })
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snowflake {
    /// 0 until the server has stored it.
    #[serde(default)]
    pub id: i64,
    pub character_id: i64,
    pub character_name: String,
    #[serde(rename = "type")]
    pub kind: SnowflakeType,
}

/// A tracked fleet, as `GET /api/v1/fleet/{uuid}` returns it. Also what `PUT /api/v1/fleet` takes
/// back, so this doubles as the edit payload.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fleet {
    pub id: FleetId,
    pub commander: Option<Labelled>,
    pub operation_name: Option<String>,
    /// The in-game fleet id, which is how the dashboard reads composition through ESI.
    pub esi_id: i64,
    pub setup_id: SetupId,
    #[serde(default)]
    pub use_backup: bool,
    pub group_name: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// An account id, not a character id.
    #[serde(default)]
    pub started_by_id: i64,
    pub started_at: String,
    pub closed_at: Option<String>,
    pub auto_close_type: Option<i32>,
    pub auto_close_time: Option<i32>,
    #[serde(default)]
    pub ignore_participation_requirements: bool,
    #[serde(default)]
    pub tag_ids: Vec<TagId>,
    #[serde(default)]
    pub snowflakes: Vec<Snowflake>,
    #[serde(default)]
    pub statistic_id: i64,
    #[serde(default)]
    pub has_doctrine_info: bool,
    pub boost_channel_id: Option<ChannelId>,
    pub logi_channel_id: Option<ChannelId>,
    pub mumble_channel_id: Option<ChannelId>,
    pub formup_location: Option<Labelled>,
}

/// A row of the active list or the history table.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetRow {
    pub id: FleetId,
    pub name: String,
    pub setup_name: Option<String>,
    pub operation_name: Option<String>,
    pub group_name: Option<String>,
    /// The history table spells this `startedBy`; the active list spells it `startedByName`.
    #[serde(alias = "startedByName")]
    pub started_by: Option<String>,
    #[serde(alias = "commanderName")]
    pub commander: Option<String>,
    pub started_at: String,
    pub closed_at: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagItem>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCharacter {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub pap_count: i64,
    pub r#type: Option<i32>,
    pub primary_role: Option<i32>,
    pub primary_ship_type_id: Option<i64>,
    pub primary_ship_type_name: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleCount {
    pub count: i64,
    pub percentage: f64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipCount {
    pub ship_type_id: i64,
    pub ship_type_name: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCount {
    pub group_id: i64,
    pub group_name: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetReport {
    pub total_characters: i64,
    #[serde(default)]
    pub characters: Vec<ReportCharacter>,
    #[serde(default)]
    pub role_counts: Vec<RoleCount>,
    #[serde(default)]
    pub ship_counts: Vec<ShipCount>,
    #[serde(default)]
    pub group_counts: Vec<GroupCount>,
}

/// The start form, exactly the fields the site's own reactive form holds.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartForm {
    pub name: String,
    pub description: String,
    /// 0 means no setup chosen. The site's placeholder renders as null and posts 0.
    pub setup_id: i32,
    pub group_id: Option<GroupId>,
    pub boost_channel_id: Option<ChannelId>,
    pub logi_channel_id: Option<ChannelId>,
    pub mumble_channel_id: Option<ChannelId>,
    /// 0 start, 1 FC left.
    pub auto_close_type: Option<i32>,
    pub auto_close_time: Option<i32>,
    pub is_corporation_fleet: bool,
    /// In the payload but not on the site's form.
    pub ignore_participation_requirements: bool,
    pub set_motd: bool,
    pub doctrine_notes: Option<String>,
}

impl Default for StartForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            setup_id: 0,
            group_id: None,
            boost_channel_id: None,
            logi_channel_id: None,
            mumble_channel_id: None,
            auto_close_type: Some(1),
            auto_close_time: Some(30),
            is_corporation_fleet: false,
            ignore_participation_requirements: false,
            set_motd: false,
            doctrine_notes: None,
        }
    }
}

/// `POST /api/v1/fleet/start`: the form plus what the page attaches to it.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    #[serde(flatten)]
    pub form: StartForm,
    pub tag_ids: Vec<TagId>,
    pub character_id: i64,
    pub character_name: String,
    pub use_backup: bool,
    pub snowflakes: Vec<Snowflake>,
    pub operation_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formup_location_id: Option<i64>,
}

/// `POST /api/v1/fleet/ping` and `/ping-preview` take this same body.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {
    pub character_id: i64,
    pub description: String,
    pub doctrine_notes: Option<String>,
    pub boost_channel_id: Option<ChannelId>,
    pub logi_channel_id: Option<ChannelId>,
    pub mumble_channel_id: Option<ChannelId>,
    /// 0 means no setup, which renders as "PAP Type: None".
    pub setup_id: i32,
    pub solar_system_id: i64,
    pub tag_ids: Vec<TagId>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPreview {
    pub ping: String,
    pub motd: String,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossCheck {
    pub is_fleet_boss: bool,
    #[serde(default)]
    pub backup_available: bool,
    pub error_message: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BonusRequest {
    pub fleet_id: FleetId,
    pub count: i32,
    pub reason: String,
    pub minutes_before_close: i32,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionRequest {
    pub fleet_id: FleetId,
    pub character_names: String,
    pub is_exclusion: bool,
    pub reason: String,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub fleet_id: FleetId,
    pub description: String,
}

/// One page of an OData collection, with the total the pager needs.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// The member tree of a tracked fleet.
///
/// SHAPE UNCONFIRMED. The live fleet page was never captured, and `GET /fleet/{uuid}` carries no
/// members, only `esiId`, so this is what the UI needs rather than what the server sends. Read it
/// through the accessors so a real shape can land without touching the view.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Composition {
    pub wings: Vec<Wing>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Wing {
    pub id: WingId,
    pub name: String,
    pub squads: Vec<Squad>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Squad {
    pub id: SquadId,
    pub name: String,
    pub members: Vec<Member>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Member {
    pub character_id: i64,
    pub name: String,
    pub ship_type_id: i64,
    pub ship_type_name: String,
    pub role: String,
}

impl Composition {
    pub fn total(&self) -> usize {
        self.wings.iter().flat_map(|w| &w.squads).map(|s| s.members.len()).sum()
    }

    pub fn find(&self, character_id: i64) -> Option<(WingId, SquadId, &Member)> {
        for w in &self.wings {
            for s in &w.squads {
                if let Some(m) = s.members.iter().find(|m| m.character_id == character_id) {
                    return Some((w.id, s.id, m));
                }
            }
        }
        None
    }
}

/// Seconds since the epoch for an ISO-8601 stamp, tolerating the forms the API mixes: with or
/// without fractional seconds, with `Z` or without a zone at all.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, rest) = s.split_once('T')?;
    let time = rest.trim_end_matches('Z');
    let time = time.split(['+', '-']).next().unwrap_or(time);
    let (hms, _frac) = time.split_once('.').unwrap_or((time, ""));
    let mut d = date.split('-');
    let (y, mo, da) = (d.next()?, d.next()?, d.next()?);
    let mut t = hms.split(':');
    let (h, mi, se) = (t.next()?, t.next()?, t.next().unwrap_or("0"));
    let naive = chrono::NaiveDate::from_ymd_opt(y.parse().ok()?, mo.parse().ok()?, da.parse().ok()?)?
        .and_hms_opt(h.parse().ok()?, mi.parse().ok()?, se.parse().ok()?)?;
    Some(naive.and_utc().timestamp())
}

/// The stamp format the history `$filter` wants.
pub fn iso_z(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// The `[from, to]` window the history page asks for: whole days, `days` back from `now`.
pub fn history_window(now: i64, days: i64) -> (String, String) {
    let day = 86_400;
    let start = (now - days * day) / day * day;
    let end = now / day * day + day - 1;
    (iso_z(start), iso_z(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The captured `GET /api/v1/fleet/{uuid}` body. Deserialising and reserialising it has to come
    /// back the same, which is what catches a renamed or dropped field.
    #[test]
    fn a_fleet_round_trips_the_captured_json() {
        let captured = serde_json::json!({
            "id": "439f833e-2aca-44c3-a82d-4bc8a07e489a",
            "commander": {"id": 2119400938, "label": "Amryu"},
            "operationName": null,
            "esiId": 1037012308392i64,
            "setupId": 46,
            "useBackup": false,
            "groupName": null,
            "name": "Home Defense",
            "description": "Trash to take out. Get in!",
            "startedById": 211870,
            "startedAt": "2026-09-18T17:17:39Z",
            "closedAt": "2026-09-18T17:41:05Z",
            "autoCloseType": 1,
            "autoCloseTime": 30,
            "ignoreParticipationRequirements": false,
            "tagIds": [1, 12],
            "snowflakes": [{"id": 7, "characterId": 90000001, "characterName": "Scout Alt", "type": 3}],
            "statisticId": 452564,
            "hasDoctrineInfo": true,
            "boostChannelId": 5,
            "logiChannelId": 1,
            "mumbleChannelId": 12,
            "formupLocation": {"id": 30000772, "label": "C-J6MT"}
        });
        let fleet: Fleet = serde_json::from_value(captured.clone()).expect("the captured shape");
        assert_eq!(fleet.esi_id, 1_037_012_308_392);
        assert_eq!(fleet.setup_id, SetupId(46));
        assert_eq!(fleet.tag_ids, vec![TagId(1), TagId(12)]);
        assert_eq!(fleet.snowflakes[0].kind, SnowflakeType::Backseat);
        assert_eq!(serde_json::to_value(&fleet).unwrap(), captured);
    }

    /// The captured report, which the composition counts are drawn from.
    #[test]
    fn a_report_reads_its_counts() {
        let captured = serde_json::json!({
            "totalCharacters": 175,
            "characters": [{"id": 1, "name": "Someone", "papCount": 1, "type": null,
                            "primaryRole": 1, "primaryShipTypeId": 22464,
                            "primaryShipTypeName": "Flycatcher"}],
            "roleCounts": [{"count": 24, "percentage": 13.714285714285715}],
            "shipCounts": [{"shipTypeId": 22464, "shipTypeName": "Flycatcher", "count": 97,
                            "percentage": 55.42857142857143}],
            "groupCounts": [{"groupId": 541, "groupName": "Interdictor", "count": 102,
                             "percentage": 58.285714285714285}]
        });
        let r: FleetReport = serde_json::from_value(captured.clone()).expect("the captured shape");
        assert_eq!(r.total_characters, 175);
        assert_eq!(r.ship_counts[0].ship_type_name, "Flycatcher");
        assert_eq!(serde_json::to_value(&r).unwrap(), captured);
    }

    /// The history rows and the active rows name the same person differently.
    #[test]
    fn both_list_shapes_parse() {
        let history: FleetRow = serde_json::from_value(serde_json::json!({
            "id": "abc", "name": "Home Defense", "setupName": "Flycatchers",
            "operationName": null, "groupName": null, "startedBy": "Amryu",
            "startedAt": "2026-09-18T17:17:39.592528Z", "closedAt": "2026-09-18T17:41:05.816062Z",
            "tags": [{"id": 1, "name": "STRATEGIC", "colourClass": "red", "isPrimary": true,
                      "isStrategic": true}]
        }))
        .expect("history row");
        assert_eq!(history.started_by.as_deref(), Some("Amryu"));
        assert_eq!(history.tags[0].id, TagId(1));

        let active: FleetRow = serde_json::from_value(serde_json::json!({
            "id": "def", "name": "Whaling", "setupName": "Typhoon", "startedByName": "Someone",
            "commanderName": "Someone Else", "startedAt": "2026-09-19T10:00:00Z", "closedAt": null
        }))
        .expect("active row");
        assert_eq!(active.started_by.as_deref(), Some("Someone"));
        assert_eq!(active.commander.as_deref(), Some("Someone Else"));
    }

    /// Snowflake types cross the wire as small integers.
    #[test]
    fn snowflake_types_are_their_numbers() {
        for (t, n) in [
            (SnowflakeType::Fc, 0),
            (SnowflakeType::Vip, 1),
            (SnowflakeType::LogiAnchor, 2),
            (SnowflakeType::Backseat, 3),
            (SnowflakeType::Hunter, 4),
        ] {
            assert_eq!(serde_json::to_value(t).unwrap(), serde_json::json!(n));
            assert_eq!(serde_json::from_value::<SnowflakeType>(serde_json::json!(n)).unwrap(), t);
        }
        assert!(serde_json::from_value::<SnowflakeType>(serde_json::json!(9)).is_err());
    }

    /// Permission strings are the server's, not ours.
    #[test]
    fn an_identity_answers_for_its_permissions() {
        let id: Identity = serde_json::from_value(serde_json::json!({
            "name": "Amryu", "commandGroup": "SC", "sigs": [],
            "permissions": ["accessFleet", "startFleet", "somethingNew"]
        }))
        .expect("identity");
        assert!(id.can(Perm::StartFleet));
        assert!(!id.can(Perm::KickMember));
        // An unknown permission is kept rather than dropped, so it survives a round trip.
        assert!(id.permissions.iter().any(|p| p == "somethingNew"));
    }

    #[test]
    fn iso_stamps_parse_in_the_forms_the_api_mixes() {
        let plain = parse_iso("2026-09-18T17:17:39Z").expect("plain");
        let frac = parse_iso("2026-09-18T17:17:39.592528Z").expect("fractional");
        assert_eq!(plain, frac);
        assert_eq!(iso_z(plain), "2026-09-18T17:17:39.000Z");
        assert_eq!(parse_iso("nonsense"), None);
    }

    /// The window is whole days, so the same page reloads identically within a day.
    #[test]
    fn the_history_window_covers_whole_days() {
        let now = parse_iso("2026-09-19T12:34:56Z").expect("now");
        let (from, to) = history_window(now, 30);
        assert_eq!(from, "2026-08-20T00:00:00.000Z");
        assert_eq!(to, "2026-09-19T23:59:59.000Z");
    }

    #[test]
    fn a_fleet_id_shows_its_first_group() {
        assert_eq!(FleetId("439f833e-2aca-44c3".into()).short(), "439f833e");
    }

    #[test]
    fn a_composition_counts_and_finds_its_members() {
        let comp = Composition {
            wings: vec![Wing {
                id: WingId(1),
                name: "Wing 1".into(),
                squads: vec![Squad {
                    id: SquadId(10),
                    name: "Squad 1".into(),
                    members: vec![
                        Member { character_id: 5, name: "A".into(), ..Member::default() },
                        Member { character_id: 6, name: "B".into(), ..Member::default() },
                    ],
                }],
            }],
        };
        assert_eq!(comp.total(), 2);
        assert_eq!(comp.find(6).map(|(w, s, m)| (w, s, m.name.clone())),
                   Some((WingId(1), SquadId(10), "B".to_owned())));
        assert!(comp.find(99).is_none());
    }
}
