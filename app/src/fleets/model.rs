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

/// Reads `null` as the type's default instead of failing.
///
/// `#[serde(default)]` only covers a *missing* key, and this API sends explicit nulls for fields
/// that have no value yet. `statisticId` is null on every fleet that is still running, which made
/// the whole `Fleet` parse fail and took the fleet, its report, its composition and its doctrine
/// down with it. One null field must not cost a page.
pub fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + serde::Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

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
    #[serde(default, deserialize_with = "null_default")]
    pub id: i64,
    #[serde(default, deserialize_with = "null_default")]
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
    /// Whether a fleet may be handed to a character that is not on this account. Without it the
    /// migrate picker is limited to the user's own characters, which is how the site does it.
    StartFleetOther,
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
            Perm::StartFleetOther => "startFleetOther",
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
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub corporation_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub is_hidden: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupItem {
    pub id: SetupId,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    pub minimal_opsec_level_description: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub priority: i32,
    #[serde(default, deserialize_with = "null_default")]
    pub is_default: bool,
}

/// A comms channel. `is_in_use` is what makes picking a free one possible.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelItem {
    pub id: ChannelId,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub is_in_use: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagItem {
    pub id: TagId,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub colour_class: String,
    #[serde(default, deserialize_with = "null_default")]
    pub is_primary: bool,
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub character_id: i64,
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub esi_id: i64,
    pub setup_id: SetupId,
    #[serde(default, deserialize_with = "null_default")]
    pub use_backup: bool,
    pub group_name: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    /// An account id, not a character id.
    #[serde(default, deserialize_with = "null_default")]
    pub started_by_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub started_at: String,
    pub closed_at: Option<String>,
    pub auto_close_type: Option<i32>,
    pub auto_close_time: Option<i32>,
    #[serde(default, deserialize_with = "null_default")]
    pub ignore_participation_requirements: bool,
    #[serde(default)]
    pub tag_ids: Vec<TagId>,
    #[serde(default)]
    pub snowflakes: Vec<Snowflake>,
    #[serde(default, deserialize_with = "null_default")]
    pub statistic_id: i64,
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    pub setup_name: Option<String>,
    pub operation_name: Option<String>,
    pub group_name: Option<String>,
    /// The history table spells this `startedBy`; the active list spells it `startedByName`.
    #[serde(alias = "startedByName")]
    pub started_by: Option<String>,
    #[serde(alias = "commanderName")]
    pub commander: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub started_at: String,
    pub closed_at: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagItem>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCharacter {
    #[serde(default, deserialize_with = "null_default")]
    pub id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub pap_count: i64,
    pub r#type: Option<i32>,
    pub primary_role: Option<i32>,
    pub primary_ship_type_id: Option<i64>,
    pub primary_ship_type_name: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleCount {
    #[serde(default, deserialize_with = "null_default")]
    pub count: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub percentage: f64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipCount {
    #[serde(default, deserialize_with = "null_default")]
    pub ship_type_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub ship_type_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub count: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub percentage: f64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCount {
    #[serde(default, deserialize_with = "null_default")]
    pub group_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub group_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub count: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub percentage: f64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetReport {
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    /// 0 means no setup chosen. The site's placeholder renders as null and posts 0.
    #[serde(default, deserialize_with = "null_default")]
    pub setup_id: i32,
    pub group_id: Option<GroupId>,
    pub boost_channel_id: Option<ChannelId>,
    pub logi_channel_id: Option<ChannelId>,
    pub mumble_channel_id: Option<ChannelId>,
    /// 0 start, 1 FC left.
    pub auto_close_type: Option<i32>,
    pub auto_close_time: Option<i32>,
    #[serde(default, deserialize_with = "null_default")]
    pub is_corporation_fleet: bool,
    /// In the payload but not on the site's form.
    #[serde(default, deserialize_with = "null_default")]
    pub ignore_participation_requirements: bool,
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub character_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub character_name: String,
    #[serde(default, deserialize_with = "null_default")]
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
    #[serde(default, deserialize_with = "null_default")]
    pub character_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    pub doctrine_notes: Option<String>,
    pub boost_channel_id: Option<ChannelId>,
    pub logi_channel_id: Option<ChannelId>,
    pub mumble_channel_id: Option<ChannelId>,
    /// 0 means no setup, which renders as "PAP Type: None".
    #[serde(default, deserialize_with = "null_default")]
    pub setup_id: i32,
    #[serde(default, deserialize_with = "null_default")]
    pub solar_system_id: i64,
    pub tag_ids: Vec<TagId>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPreview {
    #[serde(default, deserialize_with = "null_default")]
    pub ping: String,
    #[serde(default, deserialize_with = "null_default")]
    pub motd: String,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossCheck {
    #[serde(default, deserialize_with = "null_default")]
    pub is_fleet_boss: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub backup_available: bool,
    pub error_message: Option<String>,
}

impl BossCheck {
    /// Whether the fleet can be tracked as this character, and what to say about it.
    pub fn verdict(&self) -> (bool, String) {
        if let Some(e) = self.error() {
            // A server error can be a paragraph or a stack trace, and the form is not the place
            // for either. Short ones say more than a generic headline, so keep those.
            let line = e.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
            return (
                false,
                if line.len() <= 70 && line.len() == e.trim().len() {
                    line.to_owned()
                } else {
                    "The fleet boss check failed.".to_owned()
                },
            );
        }
        match (self.is_fleet_boss, self.backup_available) {
            (true, _) => (true, "Fleet boss in game.".to_owned()),
            (false, true) => (true, "Not fleet boss, but the backup key can track it.".to_owned()),
            (false, false) => {
                (false, "Not the boss of a fleet in game, so there is nothing to track.".to_owned())
            }
        }
    }

    /// Whatever the server said, in full, for the dialog behind the short line.
    pub fn error(&self) -> Option<&str> {
        self.error_message.as_deref().map(str::trim).filter(|e| !e.is_empty())
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BonusRequest {
    pub fleet_id: FleetId,
    #[serde(default, deserialize_with = "null_default")]
    pub count: i32,
    #[serde(default, deserialize_with = "null_default")]
    pub reason: String,
    #[serde(default, deserialize_with = "null_default")]
    pub minutes_before_close: i32,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionRequest {
    pub fleet_id: FleetId,
    #[serde(default, deserialize_with = "null_default")]
    pub character_names: String,
    #[serde(default, deserialize_with = "null_default")]
    pub is_exclusion: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub reason: String,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub fleet_id: FleetId,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
}

/// One page of an OData collection, with the total the pager needs.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// One page as OData sends it. `Paged` is the shape the pager wants; this is the shape on the
/// wire, and they are deliberately not the same type.
#[derive(Deserialize)]
#[serde(bound = "T: serde::de::DeserializeOwned")]
pub struct ODataPage<T> {
    #[serde(rename = "@odata.count", default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub value: Vec<T>,
}

impl<T> From<ODataPage<T>> for Paged<T> {
    fn from(p: ODataPage<T>) -> Self {
        let items = p.value;
        let total = p.count.unwrap_or(items.len() as i64);
        Paged { items, total }
    }
}

/// The member tree of a tracked fleet.
///
/// SHAPE UNCONFIRMED. The live fleet page was never captured, and `GET /fleet/{uuid}` carries no
/// members, only `esiId`, so this is what the UI needs rather than what the server sends. Read it
/// through the accessors so a real shape can land without touching the view.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Composition {
    /// The fleet boss. One seat, filled or empty.
    pub commander: Option<Member>,
    pub wings: Vec<Wing>,
    /// True when the tree came from the roster rather than ESI, so the wing and squad ids are
    /// sentinels. Nothing may be moved into a seat that cannot be addressed.
    pub flat: bool,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Wing {
    pub id: WingId,
    pub name: String,
    /// The wing commander. One seat.
    pub commander: Option<Member>,
    pub squads: Vec<Squad>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Squad {
    pub id: SquadId,
    pub name: String,
    /// The squad commander. One seat.
    pub commander: Option<Member>,
    pub members: Vec<Member>,
}

/// A place in the fleet a pilot can be moved to. Every seat holds one pilot except a squad's
/// member list, which holds as many as the squad takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seat {
    Boss,
    WingCommander(WingId),
    SquadCommander(WingId, SquadId),
    Squad(WingId, SquadId),
}

impl Seat {
    /// The wing and squad ids the move payload carries. `-1` is "no wing" and "no squad", which is
    /// how a commander sits above the level below them.
    pub fn ids(self) -> (WingId, SquadId) {
        match self {
            Seat::Boss => (WingId(-1), SquadId(-1)),
            Seat::WingCommander(w) => (w, SquadId(-1)),
            Seat::SquadCommander(w, s) | Seat::Squad(w, s) => (w, s),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Seat::Boss => "fleet commander",
            Seat::WingCommander(_) => "wing commander",
            Seat::SquadCommander(..) => "squad commander",
            Seat::Squad(..) => "squad",
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Member {
    pub character_id: i64,
    pub name: String,
    pub ship_type_id: i64,
    pub ship_type_name: String,
    /// The hull's group, e.g. "Interdictor". What decides whether an off-doctrine ship is a job
    /// the fleet needs or a pilot in the wrong hull.
    pub ship_group: String,
    pub role: String,
    /// Participation credits the dashboard recorded for this pilot in this fleet. Only a closed
    /// fleet's report carries it; a live roster comes from ESI, which knows nothing about PAPs.
    pub pap_count: i64,
}

impl Composition {
    /// Everyone in the fleet, commanders included: a wing commander is a pilot in a ship like any
    /// other, and leaving them out of the count understates the fleet.
    pub fn members(&self) -> impl Iterator<Item = &Member> {
        self.commander.iter().chain(self.wings.iter().flat_map(|w| {
            w.commander.iter().chain(
                w.squads.iter().flat_map(|s| s.commander.iter().chain(s.members.iter())),
            )
        }))
    }

    pub fn total(&self) -> usize {
        self.members().count()
    }

    /// Where a pilot is sitting.
    pub fn seat_of(&self, character_id: i64) -> Option<Seat> {
        let is = |m: &Option<Member>| m.as_ref().is_some_and(|m| m.character_id == character_id);
        if is(&self.commander) {
            return Some(Seat::Boss);
        }
        for w in &self.wings {
            if is(&w.commander) {
                return Some(Seat::WingCommander(w.id));
            }
            for s in &w.squads {
                if is(&s.commander) {
                    return Some(Seat::SquadCommander(w.id, s.id));
                }
                if s.members.iter().any(|m| m.character_id == character_id) {
                    return Some(Seat::Squad(w.id, s.id));
                }
            }
        }
        None
    }

    /// Who already holds a seat, so the tree can say why a drop is refused.
    pub fn holder(&self, seat: Seat) -> Option<&Member> {
        match seat {
            Seat::Boss => self.commander.as_ref(),
            Seat::WingCommander(w) => {
                self.wings.iter().find(|x| x.id == w)?.commander.as_ref()
            }
            Seat::SquadCommander(w, s) => self
                .wings
                .iter()
                .find(|x| x.id == w)?
                .squads
                .iter()
                .find(|x| x.id == s)?
                .commander
                .as_ref(),
            Seat::Squad(..) => None,
        }
    }

    pub fn find(&self, character_id: i64) -> Option<(WingId, SquadId, &Member)> {
        let m = self.members().find(|m| m.character_id == character_id)?;
        let (w, s) = self.seat_of(character_id)?.ids();
        Some((w, s, m))
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

    fn pilot(id: i64, name: &str) -> Member {
        Member { character_id: id, name: name.into(), ..Member::default() }
    }

    fn crewed() -> Composition {
        Composition {
            flat: false,
            commander: Some(pilot(1, "Boss")),
            wings: vec![Wing {
                id: WingId(1),
                name: "Wing 1".into(),
                commander: Some(pilot(2, "Wing Lead")),
                squads: vec![Squad {
                    id: SquadId(10),
                    name: "Squad 1".into(),
                    commander: Some(pilot(3, "Squad Lead")),
                    members: vec![pilot(5, "A"), pilot(6, "B")],
                }],
            }],
        }
    }

    /// A commander is a pilot in a ship like any other, so the count includes all three seats.
    #[test]
    fn a_composition_counts_and_finds_its_members() {
        let comp = crewed();
        assert_eq!(comp.total(), 5);
        assert_eq!(comp.find(6).map(|(w, s, m)| (w, s, m.name.clone())),
                   Some((WingId(1), SquadId(10), "B".to_owned())));
        assert!(comp.find(99).is_none());
    }

    /// Where each pilot is sitting, and what the move payload spells it as.
    #[test]
    fn every_seat_is_found_and_has_its_own_ids() {
        let comp = crewed();
        assert_eq!(comp.seat_of(1), Some(Seat::Boss));
        assert_eq!(comp.seat_of(2), Some(Seat::WingCommander(WingId(1))));
        assert_eq!(comp.seat_of(3), Some(Seat::SquadCommander(WingId(1), SquadId(10))));
        assert_eq!(comp.seat_of(5), Some(Seat::Squad(WingId(1), SquadId(10))));
        assert_eq!(comp.seat_of(99), None);

        // -1 is "no wing" and "no squad", which is how a commander sits above the level below.
        assert_eq!(Seat::Boss.ids(), (WingId(-1), SquadId(-1)));
        assert_eq!(Seat::WingCommander(WingId(1)).ids(), (WingId(1), SquadId(-1)));
        assert_eq!(Seat::Squad(WingId(1), SquadId(10)).ids(), (WingId(1), SquadId(10)));
    }

    /// What the boss check means for whether the fleet can be tracked at all.
    #[test]
    fn a_boss_check_says_whether_it_can_be_tracked() {
        let check = |boss, backup, err: Option<&str>| BossCheck {
            is_fleet_boss: boss,
            backup_available: backup,
            error_message: err.map(str::to_owned),
        };
        assert!(check(true, false, None).verdict().0);
        // Not the boss, but the backup key can read the fleet anyway.
        assert!(check(false, true, None).verdict().0);
        assert!(!check(false, false, None).verdict().0);

        // An error from the server is the answer, whatever the flags say. A short one reads
        // better inline than a generic headline would.
        let (ok, why) = check(true, true, Some("ESI token expired")).verdict();
        assert!(!ok);
        assert_eq!(why, "ESI token expired");
        // A long or multi-line one does not belong in the form: the headline stands in and the
        // whole thing is still there for the dialog behind it.
        let long = "Something went wrong.\n   at Fleet.Check(Int64 id)\n   at Handler.Invoke()";
        let c = check(true, true, Some(long));
        let (ok, why) = c.verdict();
        assert!(!ok);
        assert_eq!(why, "The fleet boss check failed.");
        assert_eq!(c.error(), Some(long));
        let wordy = check(false, false, Some(&"x".repeat(200)));
        assert_eq!(wordy.verdict().1, "The fleet boss check failed.");
        // An empty message is not an error.
        assert!(check(true, false, Some("  ")).verdict().0);
        assert_eq!(check(true, false, Some("  ")).error(), None);
    }

    /// A seat holds one pilot, and the tree has to say who, so a second one is not dropped in.
    #[test]
    fn a_commander_seat_names_who_holds_it() {
        let comp = crewed();
        assert_eq!(comp.holder(Seat::Boss).map(|m| m.name.as_str()), Some("Boss"));
        assert_eq!(
            comp.holder(Seat::WingCommander(WingId(1))).map(|m| m.name.as_str()),
            Some("Wing Lead")
        );
        assert!(comp.holder(Seat::WingCommander(WingId(9))).is_none());
        // A squad's member list is not a seat, so nobody holds it.
        assert!(comp.holder(Seat::Squad(WingId(1), SquadId(10))).is_none());

        let empty = Composition { commander: None, wings: vec![], flat: false };
        assert!(empty.holder(Seat::Boss).is_none());
        assert_eq!(empty.total(), 0);
    }
}

#[cfg(test)]
mod null_tolerance_tests {
    use super::*;

    /// The shape a running fleet actually sends. `statisticId` is null until the statistics are
    /// generated at close, and `#[serde(default)]` does not cover an explicit null: the whole
    /// `Fleet` parse failed with "invalid type: null, expected i64", which took the fleet, its
    /// report, its composition and its doctrine down together. Every live fleet hit this; every
    /// closed one, which is all I had tested against, has the field filled in.
    #[test]
    fn a_running_fleet_decodes_with_its_nulls() {
        let raw = serde_json::json!({
            "id": "00000000-0000-4000-8000-000000000042",
            "name": "Home Defence",
            "esiId": 3_000_000_001_i64,
            "setupId": 84,
            "startedAt": "2026-09-21T10:00:00Z",
            "closedAt": null,
            "groupName": null,
            "operationName": null,
            "statisticId": null,
            "autoCloseType": 1,
            "autoCloseTime": 30,
            "boostChannelId": 5,
            "logiChannelId": 1,
            "mumbleChannelId": 12,
            "formupLocation": null
        });
        let f: Fleet = serde_json::from_value(raw).expect("a running fleet has to decode");
        assert_eq!(f.statistic_id, 0);
        assert_eq!(f.name, "Home Defence");
        assert!(f.closed_at.is_none());
        assert_eq!(f.mumble_channel_id, Some(ChannelId(12)));
    }

    /// Any scalar the server has no value for yet reads as its default rather than as a failure.
    #[test]
    fn a_null_scalar_never_fails_a_parse() {
        let c: ReportCharacter = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "Someone", "papCount": null, "type": null,
            "primaryRole": null, "primaryShipTypeId": null, "primaryShipTypeName": null
        }))
        .expect("decodes");
        assert_eq!(c.pap_count, 0);

        let t: TagItem = serde_json::from_value(serde_json::json!({
            "id": 3, "name": "Strategic", "colourClass": null, "isPrimary": null,
            "isStrategic": true
        }))
        .expect("decodes");
        assert!(t.colour_class.is_empty());
        assert!(!t.is_primary);
        assert!(t.is_strategic);
    }
}
