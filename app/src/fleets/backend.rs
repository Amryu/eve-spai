//! What a request to the dashboard looks like, and who can answer one.
//!
//! `calls` builds every request as a `CallRecord` and sends nothing. The dry-run backend records
//! what it built; a real one would send exactly the same thing. One place decides the shape of a
//! request, and one test suite checks that shape against the captured traffic.

use serde::Serialize;

use super::model::*;

pub type Result<T> = std::result::Result<T, FleetError>;

#[derive(Clone, PartialEq, Debug)]
pub enum FleetError {
    NotAuthenticated,
    /// The account lacks the permission, which the UI should already have prevented.
    Forbidden(Perm),
    Http { status: u16, body: String },
    Transport(String),
    Decode(String),
    /// A write the user has not turned on yet. `start` cannot fake a fleet id, so it says this
    /// instead of handing the UI an id that navigates nowhere.
    WritesHeld,
}

impl std::fmt::Display for FleetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FleetError::NotAuthenticated => write!(f, "not signed in"),
            FleetError::Forbidden(p) => write!(f, "missing the {} permission", p.as_str()),
            FleetError::Http { status, body } => write!(f, "HTTP {status}: {body}"),
            FleetError::Transport(e) => write!(f, "could not reach the dashboard: {e}"),
            FleetError::Decode(e) => write!(f, "unexpected reply: {e}"),
            FleetError::WritesHeld => write!(f, "writes are off: the request was recorded, not sent"),
        }
    }
}

/// How much of what the tab does actually leaves the machine.
///
/// Two independent questions the dry run used to answer with one bool: whether the data on screen
/// is invented, and whether a write is sent. A live read-only session is truthful about its reads
/// and still holds its writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Nothing leaves the machine. The spoof.
    DryRun,
    /// Reads go out, writes are recorded and held.
    ReadOnly,
    /// Everything goes out.
    Live,
}

/// Who the app is acting as.
///
/// The seam a real login fills. No credential lives here: whatever proves the session belongs to
/// the thing that owns the socket.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub identity: Identity,
    pub character_id: i64,
    pub character_name: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
        }
    }
}

/// A request as it would go out. In a dry run this is all that happens.
#[derive(Clone, PartialEq, Debug)]
pub struct CallRecord {
    pub at: i64,
    pub method: Method,
    pub path: String,
    pub body: Option<serde_json::Value>,
}

impl CallRecord {
    pub(crate) fn new(method: Method, path: impl Into<String>, body: Option<serde_json::Value>) -> Self {
        Self { at: chrono::Utc::now().timestamp(), method, path: path.into(), body }
    }

    /// The one-line form the journal shows.
    pub fn line(&self) -> String {
        format!("{} {}", self.method.as_str(), self.path)
    }

    pub fn pretty_body(&self) -> Option<String> {
        self.body.as_ref().map(|b| serde_json::to_string_pretty(b).unwrap_or_default())
    }
}

/// A write's result beside the request that would have produced it.
#[derive(Clone, PartialEq, Debug)]
pub struct Written<T> {
    pub record: CallRecord,
    pub value: T,
}

/// Which type-ahead is being searched.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchKind {
    Character,
    Corporation,
    Alliance,
    FleetSetup,
    SolarSystem,
}

impl SearchKind {
    /// Under `search`, not under each entity's own controller: `POST /api/v1/character/` is the
    /// account-characters route and answers 405, which is why every type-ahead came up empty.
    fn path(self) -> &'static str {
        match self {
            SearchKind::Character => "/api/v1/search/character",
            SearchKind::Corporation => "/api/v1/search/corporation",
            SearchKind::Alliance => "/api/v1/search/alliance",
            SearchKind::FleetSetup => "/api/v1/search/fleet-setup",
            SearchKind::SolarSystem => "/api/v1/search/solar-system",
        }
    }
}

/// Everything a fleet page can ask for that is not a plain read.
#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    Update(Box<Fleet>),
    Close,
    SetMotd,
    FileReport(ReportRequest),
    Bonus(BonusRequest),
    Exception(ExceptionRequest),
    AssignDistribution { wing: WingId, distribution: DistributionId },
    Invite { character_id: i64 },
    /// Hands the fleet to another character, who must already be the boss of an in-game fleet.
    /// The dashboard checks that with `boss_check` before it will let the button go.
    Migrate { character_id: i64 },
    Move { character_id: i64, wing: WingId, squad: SquadId },
    Kick { character_id: i64, exclude: bool },
    KickMany { character_ids: Vec<i64>, exclude: bool },
    KickAll,
    KickCapsules,
    AddWing,
    AddSquad(WingId),
    SplitSquad { wing: WingId, squad: SquadId },
}

impl Action {
    /// The permission this action needs, so the UI and the backend agree on one answer.
    pub fn perm(&self) -> Perm {
        match self {
            Action::Invite { .. } => Perm::InviteMember,
            Action::Move { .. } => Perm::MoveMember,
            Action::Kick { .. }
            | Action::KickMany { .. }
            | Action::KickAll
            | Action::KickCapsules => Perm::KickMember,
            Action::FileReport(_) => Perm::FlagFleet,
            Action::Bonus(_) => Perm::AccessPayouts,
            _ => Perm::AccessFleet,
        }
    }
}

fn json<T: Serialize>(v: &T) -> Option<serde_json::Value> {
    serde_json::to_value(v).ok()
}

/// Every request the tab can make, as data. Nothing here performs one.
pub mod calls {
    use super::*;

    pub const FLEET: &str = "/api/v1/fleet";
    pub const ODATA: &str = "/api/odata/v1";
    /// The page size the site's own history table asks for.
    pub const HISTORY_PAGE: u32 = 20;
    /// The page size the site's own reference lookups ask for.
    pub const REFERENCE_PAGE: u32 = 50;

    pub fn is_authenticated() -> CallRecord {
        CallRecord::new(Method::Get, "/api/v1/authentication/is-authenticated", None)
    }

    pub fn characters() -> CallRecord {
        CallRecord::new(Method::Get, "/api/v1/character", None)
    }

    pub fn sigs() -> CallRecord {
        CallRecord::new(Method::Get, format!("{FLEET}/sigs"), None)
    }

    /// One of the reference collections, ordered by name the way the page asks for it.
    pub fn reference(set: &str) -> CallRecord {
        reference_at(set, 0)
    }

    /// The same, from an offset. The page's own request is always the first page, but a collection
    /// longer than `REFERENCE_PAGE` has to be walked or the tag list silently loses its tail.
    pub fn reference_at(set: &str, skip: u32) -> CallRecord {
        CallRecord::new(
            Method::Get,
            format!(
                "{ODATA}/{set}?$count=true&$top={REFERENCE_PAGE}&$skip={skip}&$orderby=name asc"
            ),
            None,
        )
    }

    pub fn active(strategic: bool) -> CallRecord {
        CallRecord::new(Method::Get, format!("{FLEET}/active/{strategic}"), None)
    }

    pub fn tag_list(strategic: bool) -> CallRecord {
        CallRecord::new(Method::Get, format!("/api/v1/tag/list/{strategic}"), None)
    }

    /// The fleet history, newest first, optionally narrowed by a search.
    ///
    /// No date window. The site's own table asks for one month, which hides most of the history
    /// behind a boundary nothing on screen explains. `$count=true` reports the real total whatever
    /// `$top` is, so the pager knows how many there are without reading them.
    ///
    /// The search runs server side across the three fields worth matching, so finding a fleet from
    /// last year costs one request rather than walking every page.
    pub fn history(search: &str, skip: u32) -> CallRecord {
        let mut path = format!(
            "{ODATA}/fleetHistoryItem?$count=true&$expand=tags&$top={HISTORY_PAGE}&$skip={skip}\
             &$orderby=closedAt desc"
        );
        let q = search.trim();
        if !q.is_empty() {
            // OData escapes a quote by doubling it; without this a name with an apostrophe is a
            // syntax error rather than a search.
            let q = q.to_lowercase().replace('\'', "''");
            path.push_str(&format!(
                "&$filter=contains(tolower(name),'{q}') or contains(tolower(startedBy),'{q}') \
                 or contains(tolower(setupName),'{q}')"
            ));
        }
        CallRecord::new(Method::Get, path, None)
    }

    pub fn read(id: &FleetId) -> CallRecord {
        CallRecord::new(Method::Get, format!("{FLEET}/{id}"), None)
    }

    pub fn report(id: &FleetId) -> CallRecord {
        CallRecord::new(Method::Get, format!("{FLEET}/{id}/report"), None)
    }

    pub fn doctrine(id: &FleetId) -> CallRecord {
        CallRecord::new(Method::Get, format!("{FLEET}/{id}/doctrine"), None)
    }

    pub fn boss_check(character_id: i64, use_backup: bool) -> CallRecord {
        CallRecord::new(Method::Get, format!("{FLEET}/check/{character_id}/{use_backup}"), None)
    }

    pub fn search(kind: SearchKind, value: &str, strict: bool) -> CallRecord {
        CallRecord::new(
            Method::Post,
            kind.path(),
            Some(serde_json::json!({ "value": value, "isStrict": strict })),
        )
    }

    pub fn start(req: &StartRequest) -> CallRecord {
        CallRecord::new(Method::Post, format!("{FLEET}/start"), json(req))
    }

    /// Renders the ping and the MOTD and sends nothing. The only one of the ping routes this uses:
    /// `POST /fleet/ping` does not ping either, it posts the request into skirmish_commanders, and
    /// the app can post there itself.
    pub fn ping_preview(req: &PingRequest) -> CallRecord {
        CallRecord::new(Method::Post, format!("{FLEET}/ping-preview"), json(req))
    }

    /// Every write that belongs to one fleet.
    pub fn act(id: &FleetId, action: &Action) -> CallRecord {
        match action {
            Action::Update(f) => CallRecord::new(Method::Put, FLEET, json(f.as_ref())),
            Action::Close => CallRecord::new(Method::Delete, format!("{FLEET}/{id}"), None),
            Action::SetMotd => CallRecord::new(Method::Put, format!("{FLEET}/{id}/motd"), None),
            Action::FileReport(r) => {
                CallRecord::new(Method::Post, format!("{FLEET}/{id}/report"), json(r))
            }
            Action::Bonus(b) => CallRecord::new(Method::Post, format!("{FLEET}/{id}/bonus"), json(b)),
            Action::Exception(e) => {
                CallRecord::new(Method::Post, format!("{FLEET}/{id}/exception"), json(e))
            }
            Action::AssignDistribution { wing, distribution } => CallRecord::new(
                Method::Post,
                format!("{FLEET}/{id}/distribution"),
                Some(serde_json::json!({ "wingId": wing.0, "distributionId": distribution.0 })),
            ),
            Action::Invite { character_id } => CallRecord::new(
                Method::Post,
                format!("{FLEET}/{id}/member"),
                Some(serde_json::json!({ "characterId": character_id })),
            ),
            // No body: the target is the path's last segment, the way the site sends it.
            Action::Migrate { character_id } => CallRecord::new(
                Method::Put,
                format!("{FLEET}/{id}/migrate/{character_id}"),
                None,
            ),
            Action::Move { character_id, wing, squad } => CallRecord::new(
                Method::Put,
                format!("{FLEET}/{id}/member"),
                Some(serde_json::json!({
                    "characterId": character_id, "wingId": wing.0, "squadId": squad.0
                })),
            ),
            Action::Kick { character_id, exclude } => CallRecord::new(
                Method::Delete,
                format!("{FLEET}/{id}/member/{character_id}/{exclude}"),
                None,
            ),
            Action::KickMany { character_ids, exclude } => CallRecord::new(
                Method::Post,
                format!("{FLEET}/{id}/member/multiple/{exclude}"),
                json(character_ids),
            ),
            Action::KickAll => {
                CallRecord::new(Method::Delete, format!("{FLEET}/{id}/member/all"), None)
            }
            Action::KickCapsules => {
                CallRecord::new(Method::Delete, format!("{FLEET}/{id}/member/capsule"), None)
            }
            Action::AddWing => CallRecord::new(Method::Post, format!("{FLEET}/{id}/wing"), None),
            Action::AddSquad(wing) => {
                CallRecord::new(Method::Post, format!("{FLEET}/{id}/wing/{wing}/squad"), None)
            }
            Action::SplitSquad { wing, squad } => CallRecord::new(
                Method::Post,
                format!("{FLEET}/{id}/wing/{wing}/squad/{squad}/split"),
                None,
            ),
        }
    }
}

/// A live push stream, read one event at a time from a thread of its own. `next` blocks until the
/// server says something or the connection ends.
pub trait HubFeed: Send {
    fn next(&mut self) -> Option<crate::fleets::hub::Event>;
}

/// Answers the tab's questions. The dry-run spoof and a future HTTP client both wear this.
///
/// `&self` throughout so an `Arc<dyn FleetBackend>` can be handed to a worker thread; an
/// implementation that needs to mutate keeps its own lock.
pub trait FleetBackend: Send + Sync + 'static {
    fn session(&self) -> Result<Session>;
    fn characters(&self) -> Result<Vec<AccountCharacter>>;
    fn sigs(&self) -> Result<Vec<Labelled>>;
    fn setups(&self) -> Result<Vec<SetupItem>>;
    fn boost_channels(&self) -> Result<Vec<ChannelItem>>;
    fn logi_channels(&self) -> Result<Vec<ChannelItem>>;
    fn mumble_channels(&self) -> Result<Vec<ChannelItem>>;
    fn tags(&self) -> Result<Vec<TagItem>>;
    fn active(&self, strategic: bool) -> Result<Vec<FleetRow>>;
    fn history(&self, search: &str, skip: u32) -> Result<Paged<FleetRow>>;
    fn fleet(&self, id: &FleetId) -> Result<Fleet>;
    fn report(&self, id: &FleetId) -> Result<FleetReport>;
    fn composition(&self, id: &FleetId) -> Result<Composition>;
    /// The hulls this fleet's setup flies, when the dashboard knows them.
    fn doctrine(&self, id: &FleetId) -> Result<Option<super::doctrine::Doctrine>>;
    fn boss_check(&self, character_id: i64, use_backup: bool) -> Result<BossCheck>;
    fn search(&self, kind: SearchKind, value: &str, strict: bool) -> Result<Vec<Labelled>>;

    fn ping_preview(&self, req: &PingRequest) -> Result<Written<PingPreview>>;
    fn start(&self, req: &StartRequest) -> Result<Written<FleetId>>;
    fn act(&self, id: &FleetId, action: &Action) -> Result<Written<()>>;

    /// Whether the fleet is advertised in the Fleet Finder, when the boss is one of this machine's
    /// characters. `None` when it cannot be known, which the page shows as nothing at all.
    fn advert(&self, _fleet: &Fleet) -> Option<bool> {
        None
    }

    /// Opens the dashboard's push stream for one fleet, so a tracked fleet arrives instead of
    /// being polled for. A backend with nothing to push says so and the caller keeps polling.
    fn open_hub(&self, _id: &FleetId) -> Result<Box<dyn HubFeed>> {
        Err(FleetError::Transport("this backend has no hub".to_owned()))
    }

    fn mode(&self) -> Mode;

    /// Whether nothing leaves the machine, which is what the tab's banner reports.
    fn is_dry_run(&self) -> bool {
        self.mode() == Mode::DryRun
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping_req() -> PingRequest {
        PingRequest {
            character_id: 2_119_400_938,
            description: String::new(),
            doctrine_notes: None,
            boost_channel_id: Some(ChannelId(5)),
            logi_channel_id: Some(ChannelId(1)),
            mumble_channel_id: Some(ChannelId(12)),
            setup_id: 0,
            solar_system_id: 30_000_772,
            tag_ids: vec![],
        }
    }

    /// The captured ping-preview body, field for field. This is the test that proves the payload is
    /// right before anything is ever sent.
    #[test]
    fn the_ping_body_is_the_captured_one() {
        let rec = calls::ping_preview(&ping_req());
        assert_eq!(rec.method, Method::Post);
        assert_eq!(rec.path, "/api/v1/fleet/ping-preview");
        assert_eq!(
            rec.body.expect("a body"),
            serde_json::json!({
                "characterId": 2119400938i64,
                "description": "",
                "doctrineNotes": null,
                "boostChannelId": 5,
                "logiChannelId": 1,
                "mumbleChannelId": 12,
                "setupId": 0,
                "solarSystemId": 30000772,
                "tagIds": []
            })
        );
    }

    /// Requesting a ping and previewing it are the same request to a different path, so a preview
    /// the FC approved is exactly what gets sent.
    #[test]
    fn the_preview_carries_the_whole_ping_request() {
        let v = calls::ping_preview(&ping_req());
        assert_eq!(v.path, "/api/v1/fleet/ping-preview");
        assert_eq!(v.method, Method::Post);
        assert!(v.pretty_body().expect("a body").contains("\"characterId\": 2119400938"));
    }

    /// The start body is the form plus what the page attaches, flattened into one object.
    #[test]
    fn the_start_body_carries_the_form_and_its_attachments() {
        let req = StartRequest {
            form: StartForm {
                name: "Home Defense".into(),
                description: "Trash to take out.".into(),
                setup_id: 46,
                group_id: None,
                boost_channel_id: Some(ChannelId(5)),
                logi_channel_id: Some(ChannelId(1)),
                mumble_channel_id: Some(ChannelId(12)),
                auto_close_type: Some(1),
                auto_close_time: Some(30),
                is_corporation_fleet: false,
                ignore_participation_requirements: false,
                set_motd: true,
                doctrine_notes: None,
            },
            tag_ids: vec![TagId(1), TagId(12)],
            character_id: 2_119_400_938,
            character_name: "Amryu".into(),
            use_backup: false,
            snowflakes: vec![],
            operation_id: None,
            formup_location_id: Some(30_000_772),
        };
        let body = calls::start(&req).body.expect("a body");
        assert_eq!(
            body,
            serde_json::json!({
                "name": "Home Defense",
                "description": "Trash to take out.",
                "setupId": 46,
                "groupId": null,
                "boostChannelId": 5,
                "logiChannelId": 1,
                "mumbleChannelId": 12,
                "autoCloseType": 1,
                "autoCloseTime": 30,
                "isCorporationFleet": false,
                "ignoreParticipationRequirements": false,
                "setMotd": true,
                "doctrineNotes": null,
                "tagIds": [1, 12],
                "characterId": 2119400938i64,
                "characterName": "Amryu",
                "useBackup": false,
                "snowflakes": [],
                "operationId": null,
                "formupLocationId": 30000772
            })
        );
    }

    /// No formup location means the field is absent, not null: the site only attaches it when a
    /// system was picked.
    #[test]
    fn a_start_without_a_formup_location_omits_it() {
        let req = StartRequest { character_name: "Amryu".into(), ..StartRequest::default() };
        let body = calls::start(&req).body.expect("a body");
        assert!(body.get("formupLocationId").is_none());
    }

    /// Path bools are lowercase, which is what ASP.NET route binding expects.
    #[test]
    fn paths_carry_their_ids_and_lowercase_bools() {
        let id = FleetId("439f833e-2aca".into());
        assert_eq!(calls::active(true).path, "/api/v1/fleet/active/true");
        assert_eq!(calls::tag_list(false).path, "/api/v1/tag/list/false");
        assert_eq!(
            calls::boss_check(2_119_400_938, false).path,
            "/api/v1/fleet/check/2119400938/false"
        );
        assert_eq!(calls::read(&id).path, "/api/v1/fleet/439f833e-2aca");
        assert_eq!(calls::report(&id).path, "/api/v1/fleet/439f833e-2aca/report");
        assert_eq!(
            calls::act(&id, &Action::Kick { character_id: 7, exclude: true }).path,
            "/api/v1/fleet/439f833e-2aca/member/7/true"
        );
        assert_eq!(
            calls::act(&id, &Action::SplitSquad { wing: WingId(2), squad: SquadId(9) }).path,
            "/api/v1/fleet/439f833e-2aca/wing/2/squad/9/split"
        );
    }

    /// The history query is the site's own page size and ordering, with no date window: the
    /// site's month hides most of the history, and `$count=true` reports the real total either
    /// way. Verified against the live service, which answered 119 unfiltered against 53 for the
    /// month.
    #[test]
    fn the_history_query_is_unbounded_and_searchable() {
        assert_eq!(
            calls::history("", 0).path,
            "/api/odata/v1/fleetHistoryItem?$count=true&$expand=tags&$top=20&$skip=0\
             &$orderby=closedAt desc"
        );
        assert_eq!(
            calls::history("  Home Defence ", 40).path,
            "/api/odata/v1/fleetHistoryItem?$count=true&$expand=tags&$top=20&$skip=40\
             &$orderby=closedAt desc&$filter=contains(tolower(name),'home defence') or \
             contains(tolower(startedBy),'home defence') or \
             contains(tolower(setupName),'home defence')"
        );
        // A quote is doubled, or a name carrying one is a syntax error instead of a search.
        assert!(calls::history("o'neil", 0).path.contains("'o''neil'"));
        assert_eq!(calls::reference("tagItem").path,
                   "/api/odata/v1/tagItem?$count=true&$top=50&$skip=0&$orderby=name asc");
    }

    /// Editing a fleet puts the whole object back, which is what the site does.
    #[test]
    fn an_edit_puts_the_whole_fleet_and_carries_no_id_in_the_path() {
        let fleet = Fleet { id: FleetId("abc".into()), name: "Home Defense".into(), ..Fleet::default() };
        let rec = calls::act(&fleet.id.clone(), &Action::Update(Box::new(fleet)));
        assert_eq!(rec.method, Method::Put);
        assert_eq!(rec.path, "/api/v1/fleet");
        assert_eq!(rec.body.expect("a body")["name"], "Home Defense");
    }

    /// The writes that take no body must not invent one.
    #[test]
    fn bodyless_writes_send_no_body() {
        let id = FleetId("abc".into());
        for a in [Action::Close, Action::SetMotd, Action::KickAll, Action::KickCapsules,
                  Action::AddWing, Action::AddSquad(WingId(1)),
                  Action::Migrate { character_id: 90_000_001 }] {
            assert!(calls::act(&id, &a).body.is_none(), "{a:?} invented a body");
        }
    }

    /// Handing a fleet over puts the new boss in the path, not in a body. Taken from the bundle:
    /// `migrateFleet(e,i){return this.http.put(`${this.apiPath}/${e}/migrate/${i}`,void 0)}`, and
    /// `i` is the character id the picker's boss check passed.
    #[test]
    fn migrating_names_the_new_boss_in_the_path() {
        let rec =
            calls::act(&FleetId("abc".into()), &Action::Migrate { character_id: 2_119_400_938 });
        assert_eq!(rec.method, Method::Put);
        assert_eq!(rec.path, "/api/v1/fleet/abc/migrate/2119400938");
        assert!(rec.body.is_none());
        assert_eq!(Action::Migrate { character_id: 1 }.perm(), Perm::AccessFleet);
    }

    /// Kicking several posts the plain array of ids the site sends.
    #[test]
    fn kicking_many_posts_the_ids() {
        let rec = calls::act(
            &FleetId("abc".into()),
            &Action::KickMany { character_ids: vec![1, 2, 3], exclude: false },
        );
        assert_eq!(rec.path, "/api/v1/fleet/abc/member/multiple/false");
        assert_eq!(rec.body.expect("a body"), serde_json::json!([1, 2, 3]));
    }

    /// Every type-ahead is the same shape under the one search controller. Pinned against live
    /// replies: the per-entity paths this first used answer 405, which is silent in a type-ahead.
    #[test]
    fn searches_share_one_body_and_live_under_search() {
        let rec = calls::search(SearchKind::SolarSystem, "C-J6", true);
        assert_eq!(rec.path, "/api/v1/search/solar-system");
        assert_eq!(rec.body.expect("a body"), serde_json::json!({"value": "C-J6", "isStrict": true}));
        for (kind, path) in [
            (SearchKind::Character, "/api/v1/search/character"),
            (SearchKind::Corporation, "/api/v1/search/corporation"),
            (SearchKind::Alliance, "/api/v1/search/alliance"),
            (SearchKind::FleetSetup, "/api/v1/search/fleet-setup"),
        ] {
            assert_eq!(calls::search(kind, "A", false).path, path);
        }
    }

    /// An action knows which permission it needs, so the UI and the backend cannot disagree.
    #[test]
    fn actions_name_their_permission() {
        assert_eq!(Action::Invite { character_id: 1 }.perm(), Perm::InviteMember);
        assert_eq!(Action::KickAll.perm(), Perm::KickMember);
        assert_eq!(Action::Move { character_id: 1, wing: WingId(1), squad: SquadId(1) }.perm(),
                   Perm::MoveMember);
        assert_eq!(Action::FileReport(ReportRequest::default()).perm(), Perm::FlagFleet);
        assert_eq!(Action::Bonus(BonusRequest::default()).perm(), Perm::AccessPayouts);
    }

    /// Every path has to survive becoming a URL. The OData queries carry literal spaces, and a
    /// form encoder would turn them into `+`, which ASP.NET's OData parser rejects. `Url::join`
    /// percent-encodes them, so this pins the thing the HTTP backend relies on.
    #[test]
    fn every_path_joins_onto_the_base_without_mangling_the_query() {
        let base = reqwest::Url::parse("https://fleets.gnf.lt").expect("a base");
        let records = [
            calls::is_authenticated(),
            calls::characters(),
            calls::sigs(),
            calls::reference("tagItem"),
            calls::reference_at("setupItem", 50),
            calls::active(true),
            calls::tag_list(false),
            calls::history("Home Defence", 0),
            calls::ping_preview(&ping_req()),
        ];
        for rec in records {
            let url = base.join(&rec.path).unwrap_or_else(|e| panic!("{}: {e}", rec.path));
            assert!(!url.as_str().contains(' '), "{url} kept a literal space");
            assert!(!url.query().unwrap_or_default().contains('+'), "{url} encoded a space as +");
        }
        let h = base
            .join(&calls::history("Home Defence", 0).path)
            .expect("the history url");
        assert!(h.as_str().contains("$orderby=closedAt%20desc"));
        // The `or`s between filter clauses carry spaces too.
        assert!(h.as_str().contains("%20or%20"));
    }

    #[test]
    fn an_odata_page_becomes_a_pager_page() {
        let body = r#"{"@odata.count":137,"value":[{"id":1,"name":"a"},{"id":2,"name":"b"}]}"#;
        let page: ODataPage<TagItem> = serde_json::from_str(body).expect("an envelope");
        let paged: Paged<TagItem> = page.into();
        assert_eq!(paged.total, 137);
        assert_eq!(paged.items.len(), 2);

        // No `@odata.count` when the request did not ask for one: the total is what arrived.
        let page: ODataPage<TagItem> =
            serde_json::from_str(r#"{"value":[{"id":1,"name":"a"}]}"#).expect("an envelope");
        let paged: Paged<TagItem> = page.into();
        assert_eq!((paged.total, paged.items.len()), (1, 1));
    }

    #[test]
    fn a_record_renders_for_the_journal() {
        let rec = calls::ping_preview(&ping_req());
        assert_eq!(rec.line(), "POST /api/v1/fleet/ping-preview");
        assert!(rec.pretty_body().expect("a body").contains("\"characterId\": 2119400938"));
    }
}
