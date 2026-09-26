//! The dashboard over HTTP.
//!
//! `calls` already builds every request as a `CallRecord`, so this module is one `send` and a
//! trait impl that hands it records. The conformance tests in `backend.rs` pin the request shapes;
//! nothing here is free to invent a path.
//!
//! Nothing in here opens a socket under test. `HttpBackend` is constructed in exactly one place,
//! the non-headless arm of `SpaiApp::build`, and the tests below take status codes and strings.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use serde::de::DeserializeOwned;

use super::backend::*;
use super::model::*;
use super::seed::Seed;

/// The fleet hub, which is not under `/api/v1` with the rest.
const HUB: &str = "/api/hubs/fleet";
pub const BASE: &str = "https://fleets.gnf.lt";

/// The dashboard is an Angular app behind an OIDC login and the session was minted in a browser,
/// so the client keeps presenting itself as that browser rather than as this tool.
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:155.0) Gecko/20100101 Firefox/155.0";

const TIMEOUT_SECS: u64 = 30;
/// An error body is one line in the journal, not a 2 MB HTML error page held in `FleetState`.
const ERROR_BODY_MAX: usize = 400;
/// `boot()` is nine calls back to back and two of them are the identity, so the second one is free.
const SESSION_TTL_SECS: i64 = 15;

/// The cookie jar, as a map.
///
/// Not `reqwest::cookie::Jar`: that cannot be enumerated, and both echoing `XSRF-TOKEN` back as a
/// header and re-persisting a rotated session cookie need to read individual values out.
#[derive(Default)]
pub struct Cookies(Mutex<BTreeMap<String, String>>);

impl Cookies {
    fn map(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Parses the same `a=b; c=d` form `snapshot` writes, so the keychain round-trips.
    pub fn restore(text: &str) -> Self {
        let jar = Cookies::default();
        {
            let mut m = jar.map();
            for pair in text.split(';') {
                if let Some((k, v)) = split_pair(pair) {
                    m.insert(k, v);
                }
            }
        }
        jar
    }

    pub fn snapshot(&self) -> String {
        self.header()
    }

    pub fn header(&self) -> String {
        self.map().iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ")
    }

    pub fn get(&self, name: &str) -> Option<String> {
        self.map().get(name).cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.map().is_empty()
    }

    /// Takes every `set-cookie` on a reply. An empty value is a deletion, which is how a sign-out
    /// or an expired session arrives.
    pub fn absorb(&self, headers: &reqwest::header::HeaderMap) {
        let mut m = self.map();
        for h in headers.get_all(reqwest::header::SET_COOKIE) {
            let Ok(text) = h.to_str() else { continue };
            let Some((k, v)) = text.split(';').next().and_then(split_pair) else { continue };
            if v.is_empty() {
                m.remove(&k);
            } else {
                m.insert(k, v);
            }
        }
    }
}

fn split_pair(pair: &str) -> Option<(String, String)> {
    let (k, v) = pair.split_once('=')?;
    let k = k.trim();
    (!k.is_empty()).then(|| (k.to_owned(), v.trim().to_owned()))
}

/// Angular writes the antiforgery cookie percent-encoded and decodes it before echoing it back.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// Everything the ESI side of a composition needs, opened on the first one rather than at
/// construction so building the backend never touches SQLite.
///
/// `Store` is `Send` but not `Sync`, so the `Mutex` is what keeps `HttpBackend: Sync`.
struct EsiSide {
    store: crate::store::Store,
    client: reqwest::blocking::Client,
    client_id: String,
}

/// type id -> (name, group) out of the SDE.
///
/// Both member paths need it: ESI sends `ship_type_id` and nothing else, and the roster sends a
/// name with no group. `ship_group` is what decides doctrine, logi size and tackle, so a member
/// without one is invisible to every check that matters.
type Ships = HashMap<i64, (String, String)>;

struct Cached {
    at: i64,
    session: Session,
    characters: Vec<AccountCharacter>,
}

pub struct HttpBackend {
    client: reqwest::blocking::Client,
    cookies: Cookies,
    mode: Mode,
    /// The character the account acts as, by name, from settings.
    acting: String,
    /// `GET /fleet/{uuid}/doctrine` turns out to answer with ammunition, not hulls: it is
    /// `{ammunitionComment, ammunitions[{name, damageType, optimal, falloff, ...}]}`, which is
    /// what `hasDoctrineInfo` on the fleet refers to. The dashboard has no hull list to give, so
    /// the doctrine's ships come from the local seed and the user's own configuration.
    seed: Seed,
    cached: Mutex<Option<Cached>>,
    /// The EVE SSO application the stored character tokens belong to.
    sso_client_id: String,
    esi: Mutex<Option<EsiSide>>,
    ships: Mutex<Option<Ships>>,
}

impl HttpBackend {
    pub fn new(
        cookies: Cookies,
        acting: String,
        mode: Mode,
        seed: Seed,
        sso_client_id: String,
    ) -> Result<Self> {
        if cookies.is_empty() {
            return Err(FleetError::NotAuthenticated);
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent(BROWSER_UA)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .https_only(true)
            // A 302 to the sign-in page would arrive as an HTML body decoded as JSON, which reads
            // as "unexpected reply" instead of "sign in again".
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| FleetError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            cookies,
            mode,
            acting,
            seed,
            cached: Mutex::new(None),
            sso_client_id,
            esi: Mutex::new(None),
            ships: Mutex::new(None),
        })
    }

    /// What the keychain stores, after a reply may have rotated it.
    pub fn cookie_snapshot(&self) -> String {
        self.cookies.snapshot()
    }

    /// Whether a record actually goes out. `ping-preview` is a read wearing a POST: it renders the
    /// ping and the MOTD and changes nothing upstream, and the preview pane is dead without it.
    fn sends(&self, rec: &CallRecord) -> bool {
        match self.mode {
            Mode::Live => true,
            Mode::ReadOnly => rec.method == Method::Get || rec.path.ends_with("/ping-preview"),
            Mode::DryRun => false,
        }
    }

    /// The one place a `CallRecord` becomes traffic.
    fn send(&self, rec: &CallRecord, perm: Perm) -> Result<serde_json::Value> {
        let url = reqwest::Url::parse(BASE)
            .and_then(|b| b.join(&rec.path))
            .map_err(|e| FleetError::Transport(format!("{}: {e}", rec.path)))?;
        let method = match rec.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut req = self
            .client
            .request(method, url)
            .header(reqwest::header::COOKIE, self.cookies.header())
            .header(reqwest::header::ACCEPT, "application/json, text/plain, */*");
        if rec.method != Method::Get {
            let Some(token) = self.cookies.get("XSRF-TOKEN") else {
                return Err(FleetError::NotAuthenticated);
            };
            req = req
                .header("X-XSRF-TOKEN", percent_decode(&token))
                .header(reqwest::header::ORIGIN, BASE)
                .header(reqwest::header::REFERER, format!("{BASE}/"));
        }
        if let Some(body) = &rec.body {
            req = req.json(body);
        }
        let resp = req.send().map_err(|e| FleetError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        self.cookies.absorb(resp.headers());
        let body = resp.text().unwrap_or_default();
        classify(status, &body, perm)
    }

    fn get<T: DeserializeOwned>(&self, rec: CallRecord) -> Result<T> {
        let v = self.send(&rec, Perm::AccessFleet)?;
        decode(v)
    }

    /// Walks a reference collection. `reference` asks for 50 and the tag list is already half of
    /// that, so a collection that outgrows one page must not lose its tail silently.
    fn odata<T: DeserializeOwned>(&self, set: &str) -> Result<Vec<T>> {
        let mut out: Vec<T> = Vec::new();
        let mut skip = 0u32;
        loop {
            let v = self.send(&calls::reference_at(set, skip), Perm::AccessFleet)?;
            let page: ODataPage<T> = decode(v)?;
            let total = page.count;
            let got = page.value.len();
            out.extend(page.value);
            skip += got as u32;
            let more = got > 0 && total.is_some_and(|t| (out.len() as i64) < t);
            if !more || skip > 5_000 {
                return Ok(out);
            }
        }
    }

    fn write(&self, rec: CallRecord, perm: Perm) -> Result<Written<()>> {
        if self.sends(&rec) {
            self.send(&rec, perm)?;
        }
        Ok(Written { record: rec, value: () })
    }

    fn identity(&self) -> Result<(Session, Vec<AccountCharacter>)> {
        let now = chrono::Utc::now().timestamp();
        {
            let c = self.cached.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = c.as_ref().filter(|c| now - c.at < SESSION_TTL_SECS) {
                return Ok((c.session.clone(), c.characters.clone()));
            }
        }
        let identity: Identity = self.get(calls::is_authenticated())?;
        let characters: Vec<AccountCharacter> = self.get(calls::characters())?;
        let acting = characters
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(self.acting.trim()))
            .or_else(|| characters.iter().find(|c| !c.is_hidden))
            .cloned()
            .unwrap_or_default();
        let session =
            Session { identity, character_id: acting.id, character_name: acting.name.clone() };
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(Cached { at: now, session: session.clone(), characters: characters.clone() });
        Ok((session, characters))
    }
}

impl HttpBackend {
    /// The member tree from ESI, or `None` for every reason it can fail: the dashboard's commander
    /// is not one of this machine's characters, the token lacks the fleet scope, or ESI refuses
    /// because that character is no longer the boss. The caller falls back to the roster.
    /// The SDE's hull table, opened on first use. Empty when there is no store, which leaves the
    /// groups blank rather than failing the read.
    fn ships(&self) -> Ships {
        let mut guard = self.ships.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(match crate::store::Store::open() {
                Ok(s) => s.all_ships().into_iter().map(|(id, n, g)| (id, (n, g))).collect(),
                Err(_) => Ships::new(),
            });
        }
        guard.clone().unwrap_or_default()
    }

    fn esi_composition(
        &self,
        fleet: &Fleet,
        names: &HashMap<i64, String>,
        ships: &Ships,
    ) -> Option<Composition> {
        let boss = fleet.commander.as_ref()?.label.clone();
        let esi_id = Some(fleet.esi_id).filter(|id| *id > 0)?;
        let mut guard = self.esi.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let store = crate::store::Store::open().ok()?;
            let client = crate::http::client(20).ok()?;
            *guard = Some(EsiSide { store, client, client_id: self.sso_client_id.clone() });
        }
        let side = guard.as_ref()?;
        let (members, wings) = crate::esi::fleet_tree(
            &side.client,
            &side.store,
            &side.client_id,
            &boss,
            esi_id,
        )?;
        Some(build_tree(&members, &wings, ships, names))
    }
}

/// ESI's flat member list plus its wing names, as the tree the fleet page draws.
fn build_tree(
    members: &[crate::esi::FleetMemberRow],
    wings: &[crate::esi::FleetWingRow],
    ships: &HashMap<i64, (String, String)>,
    names: &HashMap<i64, String>,
) -> Composition {
    let member = |m: &crate::esi::FleetMemberRow| {
        let (name, group) = ships.get(&m.ship_type_id).cloned().unwrap_or_default();
        Member {
            character_id: m.character_id,
            name: names.get(&m.character_id).cloned().unwrap_or_default(),
            ship_type_id: m.ship_type_id,
            ship_type_name: name,
            ship_group: group,
            role: m.role.clone(),
            // ESI has no notion of participation; only a closed fleet's report does.
            pap_count: 0,
            solar_system_id: m.solar_system_id,
        }
    };
    let commander = members.iter().find(|m| m.role == "fleet_commander").map(&member);
    let wings = wings
        .iter()
        .map(|w| Wing {
            id: WingId(w.id),
            name: w.name.clone(),
            commander: members
                .iter()
                .find(|m| m.role == "wing_commander" && m.wing_id == w.id)
                .map(&member),
            squads: w
                .squads
                .iter()
                .map(|s| Squad {
                    id: SquadId(s.id),
                    name: s.name.clone(),
                    commander: members
                        .iter()
                        .find(|m| m.role == "squad_commander" && m.squad_id == s.id)
                        .map(&member),
                    members: members
                        .iter()
                        .filter(|m| m.role == "squad_member" && m.squad_id == s.id)
                        .map(&member)
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Composition { commander, wings, flat: false }
}

fn decode<T: DeserializeOwned>(v: serde_json::Value) -> Result<T> {
    serde_json::from_value(v).map_err(|e| FleetError::Decode(e.to_string()))
}

/// Status and body onto an error the tab can say something useful about.
fn classify(status: u16, body: &str, perm: Perm) -> Result<serde_json::Value> {
    // Redirects are off, so a 302 is the sign-in page and nothing else.
    if status == 401 || (300..400).contains(&status) {
        return Err(FleetError::NotAuthenticated);
    }
    if status == 403 {
        return Err(FleetError::Forbidden(perm));
    }
    if !(200..300).contains(&status) {
        let mut b = body.trim().to_owned();
        b.truncate(ERROR_BODY_MAX);
        return Err(FleetError::Http { status, body: b });
    }
    if body.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(body).map_err(|e| FleetError::Decode(e.to_string()))
}

/// `POST /fleet/start` has never been observed replying, so take the id however it arrives.
/// The new fleet's id out of what `POST /start` answers.
///
/// The site reads it as `r.value` (`router.navigate(["/fleet","overview",r.value])`), so the body is
/// `{"value": "<id>"}`. Taking only `id` or a bare string, as this first did, started the fleet
/// and then reported that no id came back.
fn fleet_id_from(v: serde_json::Value) -> Result<FleetId> {
    match v {
        serde_json::Value::String(s) => Ok(FleetId(s)),
        serde_json::Value::Object(ref o) => ["value", "id"]
            .iter()
            .find_map(|k| o.get(*k).and_then(|i| i.as_str()))
            .map(|s| FleetId(s.to_owned()))
            .ok_or_else(|| FleetError::Decode(format!("the started fleet carried no id: {v}"))),
        other => Err(FleetError::Decode(format!("the started fleet was {other}"))),
    }
}

impl FleetBackend for HttpBackend {
    fn session(&self) -> Result<Session> {
        Ok(self.identity()?.0)
    }

    fn characters(&self) -> Result<Vec<AccountCharacter>> {
        Ok(self.identity()?.1)
    }

    fn sigs(&self) -> Result<Vec<Labelled>> {
        self.get(calls::sigs())
    }

    fn setups(&self) -> Result<Vec<SetupItem>> {
        self.odata("fleetSetupItem")
    }

    fn boost_channels(&self) -> Result<Vec<ChannelItem>> {
        self.odata("boostChannelItem")
    }

    fn logi_channels(&self) -> Result<Vec<ChannelItem>> {
        self.odata("logiChannelItem")
    }

    fn mumble_channels(&self) -> Result<Vec<ChannelItem>> {
        self.odata("mumbleChannelItem")
    }

    fn tags(&self) -> Result<Vec<TagItem>> {
        self.odata("tagItem")
    }

    fn active(&self, strategic: bool) -> Result<Vec<FleetRow>> {
        self.get(calls::active(strategic))
    }

    fn history(&self, search: &str, skip: u32) -> Result<Paged<FleetRow>> {
        let v = self.send(&calls::history(search, skip), Perm::AccessFleet)?;
        let page: ODataPage<FleetRow> = decode(v)?;
        Ok(page.into())
    }

    fn fleet(&self, id: &FleetId) -> Result<Fleet> {
        self.get(calls::read(id))
    }

    /// The whole body is `null` on a fleet whose statistics have not been generated yet, which is
    /// every fleet that is still running. An empty report is the truth there; failing took the
    /// composition and the doctrine down with it, because both read this.
    fn report(&self, id: &FleetId) -> Result<FleetReport> {
        let v = self.send(&calls::report(id), Perm::AccessFleet)?;
        if v.is_null() {
            return Ok(FleetReport::default());
        }
        decode(v)
    }

    fn composition(&self, id: &FleetId) -> Result<Composition> {
        let fleet = self.fleet(id)?;
        let report = self.report(id)?;
        let ships = self.ships();
        // A closed fleet has no tree to read: the in-game fleet is gone, so ESI would only 404 on
        // the way to the roster the dashboard still has.
        if fleet.closed_at.is_none() {
            // ESI carries the tree and the hulls but no pilot names; the report carries the names.
            let names: HashMap<i64, String> =
                report.characters.iter().map(|c| (c.id, c.name.clone())).collect();
            if let Some(c) = self.esi_composition(&fleet, &names, &ships) {
                return Ok(c);
            }
        }
        Ok(flat_roster(&report, &ships))
    }

    /// From the seed, not from `/doctrine`: see the note on `seed`, that route is ammunition.
    fn doctrine(&self, id: &FleetId) -> Result<Option<super::doctrine::Doctrine>> {
        Ok(self.seed.doctrine(self.fleet(id)?.setup_id))
    }

    fn boss_check(&self, character_id: i64, use_backup: bool) -> Result<BossCheck> {
        self.get(calls::boss_check(character_id, use_backup))
    }

    fn search(&self, kind: SearchKind, value: &str, strict: bool) -> Result<Vec<Labelled>> {
        let rec = calls::search(kind, value, strict);
        let v = self.send(&rec, Perm::AccessFleet)?;
        decode(v)
    }

    fn ping_preview(&self, req: &PingRequest) -> Result<Written<PingPreview>> {
        let rec = calls::ping_preview(req);
        let v = self.send(&rec, Perm::AccessFleet)?;
        Ok(Written { record: rec, value: decode(v)? })
    }

    fn start(&self, req: &StartRequest) -> Result<Written<FleetId>> {
        let rec = calls::start(req);
        if !self.sends(&rec) {
            return Err(FleetError::WritesHeld);
        }
        let v = self.send(&rec, Perm::StartFleet)?;
        Ok(Written { record: rec, value: fleet_id_from(v)? })
    }

    fn act(&self, id: &FleetId, action: &Action) -> Result<Written<()>> {
        self.write(calls::act(id, action), action.perm())
    }

    /// Read through the boss's own ESI token, the same one the member tree comes from.
    fn advert(&self, fleet: &Fleet) -> Option<bool> {
        let boss = fleet.commander.as_ref()?.label.clone();
        let esi_id = Some(fleet.esi_id).filter(|id| *id > 0)?;
        let mut guard = self.esi.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let store = crate::store::Store::open().ok()?;
            let client = crate::http::client(20).ok()?;
            *guard = Some(EsiSide { store, client, client_id: self.sso_client_id.clone() });
        }
        let side = guard.as_ref()?;
        crate::esi::fleet_advert(&side.client, &side.store, &side.client_id, &boss, esi_id)
    }

    /// Negotiate, open the stream, shake hands, ask for the fleet.
    ///
    /// The stream has to be open before the handshake goes out, because the reply comes back down
    /// it rather than in the POST's response.
    fn open_hub(&self, id: &FleetId) -> Result<Box<dyn crate::fleets::backend::HubFeed>> {
        let neg = self.send(
            &CallRecord::new(Method::Post, format!("{HUB}/negotiate?negotiateVersion=1"), None),
            Perm::AccessFleet,
        )?;
        let token = neg
            .get("connectionToken")
            .and_then(|t| t.as_str())
            .ok_or_else(|| FleetError::Decode("the hub offered no connection token".to_owned()))?;

        let url = format!("{BASE}{HUB}?id={token}");
        let stream = self
            .client
            .get(&url)
            .header(reqwest::header::COOKIE, self.cookies.header())
            .header(reqwest::header::ACCEPT, "text/event-stream")
            // reqwest counts its timeout over the whole request, body included, so the client's
            // own 20s would cut a healthy stream off mid-fleet. A day outlasts any fleet, and the
            // hub's own ping keeps a quiet one from looking dead.
            .timeout(std::time::Duration::from_secs(86_400))
            .send()
            .map_err(|e| FleetError::Transport(e.to_string()))?;

        for frame in [crate::fleets::hub::handshake(), crate::fleets::hub::track(&id.0)] {
            let mut req = self
                .client
                .post(&url)
                .header(reqwest::header::COOKIE, self.cookies.header())
                .header(reqwest::header::CONTENT_TYPE, "text/plain")
                .header(reqwest::header::ORIGIN, BASE)
                .header(reqwest::header::REFERER, format!("{BASE}/"));
            if let Some(t) = self.cookies.get("XSRF-TOKEN") {
                req = req.header("X-XSRF-TOKEN", percent_decode(&t));
            }
            let r = req.body(frame).send().map_err(|e| FleetError::Transport(e.to_string()))?;
            if !r.status().is_success() {
                return Err(FleetError::Http {
                    status: r.status().as_u16(),
                    body: r.text().unwrap_or_default(),
                });
            }
        }
        Ok(Box::new(SseFeed { stream, decoder: Default::default(), queue: Default::default() }))
    }

    fn mode(&self) -> Mode {
        self.mode
    }
}

/// One fleet's push stream. Reads block, so this lives on a thread of its own.
struct SseFeed {
    stream: reqwest::blocking::Response,
    decoder: crate::fleets::hub::Decoder,
    queue: std::collections::VecDeque<crate::fleets::hub::Event>,
}

impl crate::fleets::backend::HubFeed for SseFeed {
    fn next(&mut self) -> Option<crate::fleets::hub::Event> {
        use std::io::Read as _;
        loop {
            if let Some(e) = self.queue.pop_front() {
                return Some(e);
            }
            let mut buf = [0u8; 8192];
            let n = self.stream.read(&mut buf).ok()?;
            if n == 0 {
                return None;
            }
            self.decoder.push(&buf[..n]);
            self.queue.extend(self.decoder.drain());
        }
    }
}

/// The roster as one unnamed wing, for a fleet whose tree we cannot read.
///
/// The ids are the `-1` sentinel `Seat::ids` already uses for "no wing / no squad", and `flat`
/// tells the tree not to offer a seat it cannot address.
fn flat_roster(report: &FleetReport, ships: &Ships) -> Composition {
    let members = report
        .characters
        .iter()
        .map(|c| {
            let type_id = c.primary_ship_type_id.unwrap_or_default();
            let (name, group) = ships.get(&type_id).cloned().unwrap_or_default();
            Member {
                character_id: c.id,
                name: c.name.clone(),
                ship_type_id: type_id,
                // The report's own name when the SDE has never heard of the hull.
                ship_type_name: c
                    .primary_ship_type_name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or(name),
                ship_group: group,
                role: String::new(),
                pap_count: c.pap_count,
                solar_system_id: 0,
            }
        })
        .collect();
    Composition {
        commander: None,
        wings: vec![Wing {
            id: WingId(-1),
            name: "Fleet".to_owned(),
            commander: None,
            squads: vec![Squad {
                id: SquadId(-1),
                name: "Roster".to_owned(),
                commander: None,
                members,
            }],
        }],
        flat: true,
    }
}

#[cfg(test)]
mod start_reply_tests {
    use super::fleet_id_from;

    /// What `/start` answers, going by the site's own `r.value`.
    #[test]
    fn the_new_fleets_id_is_read_from_value() {
        let id = fleet_id_from(serde_json::json!({"value": "00000000-0000-4000-8000-000000000042"}))
            .expect("an id");
        assert_eq!(id.0, "00000000-0000-4000-8000-000000000042");
        assert!(fleet_id_from(serde_json::json!("00000000-0000-4000-8000-000000000043")).is_ok());
        assert!(fleet_id_from(serde_json::json!({"nothing": 1})).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jar_round_trips_through_the_keychain_form() {
        let jar = Cookies::restore(".AspNetCore.Cookies=CfDJ8abc; XSRF-TOKEN=tok%2Bz");
        assert_eq!(jar.get("XSRF-TOKEN").as_deref(), Some("tok%2Bz"));
        assert_eq!(percent_decode("tok%2Bz"), "tok+z");
        let again = Cookies::restore(&jar.snapshot());
        assert_eq!(again.snapshot(), jar.snapshot());
        assert!(Cookies::restore("").is_empty());
    }

    #[test]
    fn a_reply_rotates_and_deletes_cookies() {
        let jar = Cookies::restore("a=1; b=2");
        let mut h = reqwest::header::HeaderMap::new();
        h.append(reqwest::header::SET_COOKIE, "a=9; path=/; httponly".parse().expect("a header"));
        h.append(reqwest::header::SET_COOKIE, "b=; expires=Thu, 01 Jan 1970".parse().expect("a header"));
        h.append(reqwest::header::SET_COOKIE, "c=3".parse().expect("a header"));
        jar.absorb(&h);
        assert_eq!(jar.snapshot(), "a=9; c=3");
    }

    #[test]
    fn status_maps_onto_something_the_tab_can_say() {
        assert_eq!(classify(401, "", Perm::AccessFleet), Err(FleetError::NotAuthenticated));
        // Redirects are off, so a 302 is the sign-in page.
        assert_eq!(classify(302, "<html>", Perm::AccessFleet), Err(FleetError::NotAuthenticated));
        assert_eq!(classify(403, "", Perm::KickMember), Err(FleetError::Forbidden(Perm::KickMember)));
        assert_eq!(
            classify(500, "boom", Perm::AccessFleet),
            Err(FleetError::Http { status: 500, body: "boom".to_owned() })
        );
        let long = "x".repeat(4000);
        let Err(FleetError::Http { body, .. }) = classify(502, &long, Perm::AccessFleet) else {
            panic!("a 502 is an HTTP error");
        };
        assert_eq!(body.len(), ERROR_BODY_MAX);
        // A write that answers 204 is a success, not a decode failure.
        assert_eq!(classify(204, "", Perm::AccessFleet), Ok(serde_json::Value::Null));
        assert!(matches!(classify(200, "<html>", Perm::AccessFleet), Err(FleetError::Decode(_))));
    }

    #[test]
    fn read_only_lets_the_preview_through_and_holds_the_rest() {
        let held = |mode: Mode, rec: &CallRecord| {
            let b = HttpBackend {
                client: reqwest::blocking::Client::new(),
                cookies: Cookies::restore("a=1"),
                mode,
                acting: String::new(),
                seed: Seed::default(),
                cached: Mutex::new(None),
                sso_client_id: String::new(),
                esi: Mutex::new(None),
                ships: Mutex::new(None),
            };
            b.sends(rec)
        };
        let req = PingRequest::default();
        assert!(held(Mode::ReadOnly, &calls::ping_preview(&req)));
        assert!(!held(Mode::ReadOnly, &calls::act(&FleetId("x".into()), &Action::Close)));
        assert!(held(Mode::ReadOnly, &calls::sigs()));
        assert!(held(Mode::Live, &calls::act(&FleetId("x".into()), &Action::Close)));
    }

    #[test]
    fn a_started_fleet_gives_up_its_id_either_way() {
        assert_eq!(fleet_id_from(serde_json::json!("abc")), Ok(FleetId("abc".into())));
        assert_eq!(fleet_id_from(serde_json::json!({"id": "abc"})), Ok(FleetId("abc".into())));
        assert!(fleet_id_from(serde_json::json!({})).is_err());
    }

    #[test]
    fn a_roster_without_a_tree_is_one_flat_wing() {
        let report = FleetReport {
            total_characters: 2,
            characters: vec![
                ReportCharacter {
                    id: 1,
                    name: "A".into(),
                    primary_ship_type_id: Some(11),
                    primary_ship_type_name: Some("Ferox".into()),
                    ..Default::default()
                },
                ReportCharacter { id: 2, name: "B".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let ships = Ships::from([(11, ("Ferox".to_owned(), "Battlecruiser".to_owned()))]);
        let c = flat_roster(&report, &ships);
        assert!(c.flat);
        assert_eq!(c.total(), 2);
        assert_eq!(c.wings[0].id, WingId(-1));
        assert_eq!(c.wings[0].squads[0].id, SquadId(-1));
        assert_eq!(c.wings[0].squads[0].members[0].ship_type_name, "Ferox");
        // The group is what every doctrine, logi and tackle check reads; without it a roster
        // fleet reads as if everyone were in a cruiser.
        assert_eq!(c.wings[0].squads[0].members[0].ship_group, "Battlecruiser");
    }

    /// ESI gives the seats and the hulls, the report gives the names, and the SDE gives the group
    /// every doctrine and logi check reads. All three have to meet in one member.
    #[test]
    fn the_esi_tree_keeps_its_seats() {
        use crate::esi::{FleetMemberRow, FleetSquadRow, FleetWingRow};
        let row = |cid: i64, ship: i64, wing: i64, squad: i64, role: &str| FleetMemberRow {
            character_id: cid,
            ship_type_id: ship,
            wing_id: wing,
            squad_id: squad,
            role: role.to_owned(),
            solar_system_id: 30_004_000 + cid,
        };
        let members = vec![
            row(1, 16_229, -1, -1, "fleet_commander"),
            row(2, 16_229, 10, -1, "wing_commander"),
            row(3, 11_985, 10, 20, "squad_commander"),
            row(4, 11_985, 10, 20, "squad_member"),
            row(5, 11_985, 10, 21, "squad_member"),
        ];
        let wings = vec![FleetWingRow {
            id: 10,
            name: "Wing 1".to_owned(),
            squads: vec![
                FleetSquadRow { id: 20, name: "Squad 1".to_owned() },
                FleetSquadRow { id: 21, name: "Squad 2".to_owned() },
            ],
        }];
        let ships = HashMap::from([
            (16_229, ("Damnation".to_owned(), "Command Ship".to_owned())),
            (11_985, ("Guardian".to_owned(), "Logistics".to_owned())),
        ]);
        let names = (1..=5).map(|i| (i, format!("Pilot {i}"))).collect();

        let c = build_tree(&members, &wings, &ships, &names);
        assert!(!c.flat);
        assert_eq!(c.commander.as_ref().expect("a boss").name, "Pilot 1");
        assert_eq!(c.commander.as_ref().expect("a boss").ship_group, "Command Ship");
        assert_eq!(c.commander.as_ref().expect("a boss").solar_system_id, 30_004_001);
        assert_eq!(c.wings[0].commander.as_ref().expect("a wing lead").character_id, 2);
        assert_eq!(c.wings[0].squads[0].commander.as_ref().expect("a squad lead").name, "Pilot 3");
        assert_eq!(c.wings[0].squads[0].members.len(), 1);
        assert_eq!(c.wings[0].squads[1].members[0].ship_type_name, "Guardian");
        // Everyone is seated exactly once, commanders included.
        assert_eq!(c.total(), 5);
    }

}

#[cfg(test)]
mod live_probe {
    use super::*;

    /// GET-only smoke test against the real dashboard with the stored session. Opt in with
    /// SPAI_LIVE_PROBE=1; does nothing otherwise. Never run in CI.
    #[test]
    fn probe() {
        if std::env::var("SPAI_LIVE_PROBE").is_err() {
            return;
        }
        let text = crate::fleets::creds::load().expect("no stored session");
        // From the environment, not the source: this repo is public.
        let acting = std::env::var("SPAI_LIVE_CHARACTER").unwrap_or_default();
        let b = HttpBackend::new(
            Cookies::restore(&text),
            acting,
            Mode::ReadOnly,
            crate::fleets::seed::load(),
            String::new(),
        )
        .expect("backend");

        let s = b.session().expect("session");
        println!("identity: {} / {} / {} permissions",
                 s.identity.name, s.identity.command_group, s.identity.permissions.len());
        println!("acting as character id {}", s.character_id);
        println!("characters: {}", b.characters().expect("characters").len());
        println!("sigs: {}", b.sigs().expect("sigs").len());
        println!("setups: {}", b.setups().expect("setups").len());
        println!("mumble: {}", b.mumble_channels().expect("mumble").len());
        println!("logi: {}", b.logi_channels().expect("logi").len());
        println!("boost: {}", b.boost_channels().expect("boost").len());
        let tags = b.tags().expect("tags");
        println!("tags: {} ({} primary)", tags.len(), tags.iter().filter(|t| t.is_primary).count());
        let strat = b.active(true).expect("active strategic");
        let peace = b.active(false).expect("active peacetime");
        println!("active: {strat} strategic, {peace} peacetime", strat = strat.len(), peace = peace.len());
        let h = b.history("", 0).expect("history");
        println!("history: {} of {} total (no filter)", h.items.len(), h.total);
        if let Ok(q) = std::env::var("SPAI_SEARCH") {
            let f = b.history(&q, 0).expect("filtered history");
            println!("history {q:?}: {} of {} total", f.items.len(), f.total);
            let p2 = b.history(&q, 20).expect("page 2");
            println!("history {q:?} page 2: {} rows, total still {}", p2.items.len(), p2.total);
        }
        for (kind, q) in [(SearchKind::Character, "Amr"), (SearchKind::SolarSystem, "C-J6")] {
            match b.search(kind, q, false) {
                Ok(hits) => println!("search {kind:?} {q:?}: {} hits, first label len {}",
                                     hits.len(), hits.first().map(|h| h.label.len()).unwrap_or(0)),
                Err(e) => println!("search {kind:?} FAILED: {e}"),
            }
        }

        // A real fleet: a live one if there is one, else the newest closed one.
        let row = strat.first().or_else(|| peace.first()).cloned().or_else(|| h.items.first().cloned());
        if let Some(row) = row {
            let f = b.fleet(&row.id).expect("fleet");
            let r = b.report(&row.id).expect("report");
            println!("fleet: setup {} esi {} closed={} / report {} characters, {} ship rows",
                     f.setup_id, f.esi_id, f.closed_at.is_some(),
                     r.total_characters, r.ship_counts.len());

            let c = b.composition(&row.id).expect("composition");
            let total = c.total();
            let grouped = c.members().filter(|m| !m.ship_group.trim().is_empty()).count();
            println!("composition: {total} members, flat={}, {grouped} with a resolved group",
                     c.flat);
            let inter = crate::fleets::checks::interdiction(&c);
            println!("interdiction: {}", inter.detail);
            let lr = crate::fleets::logi::report(&c, None, None);
            println!("logi (no doctrine): wants {} logi, {} counted, {} rejected",
                     lr.size.label(), lr.counted, lr.rejected.len());
            let mut groups: std::collections::BTreeMap<String, usize> = Default::default();
            for m in c.members() {
                *groups.entry(m.ship_group.clone()).or_default() += 1;
            }
            let mut top: Vec<_> = groups.into_iter().collect();
            top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            println!("top groups: {:?}", top.into_iter().take(6).collect::<Vec<_>>());
        } else {
            println!("no fleet at all, skipped composition");
        }
    }
}

#[cfg(test)]
mod shape_dump {
    use super::*;

    /// Writes raw bodies to SPAI_DUMP_DIR so the wire shapes can be diffed against the structs.
    /// Alliance-internal, so it goes to a scratch directory and never to the repo.
    #[test]
    fn dump() {
        let Ok(dir) = std::env::var("SPAI_DUMP_DIR") else { return };
        let text = crate::fleets::creds::load().expect("no stored session");
        let b = HttpBackend::new(
            Cookies::restore(&text),
            std::env::var("SPAI_LIVE_CHARACTER").unwrap_or_default(),
            Mode::ReadOnly,
            crate::fleets::seed::load(),
            String::new(),
        )
        .expect("backend");
        std::fs::create_dir_all(&dir).expect("dir");

        let save = |name: &str, rec: CallRecord| match b.send(&rec, Perm::AccessFleet) {
            Ok(v) => {
                let path = format!("{dir}/{name}.json");
                std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).expect("write");
                println!("ok   {name}");
            }
            Err(e) => println!("FAIL {name}: {e}"),
        };

        save("is_authenticated", calls::is_authenticated());
        save("character", calls::characters());
        save("sigs", calls::sigs());
        for set in ["fleetSetupItem", "mumbleChannelItem", "logiChannelItem", "boostChannelItem",
                    "tagItem"] {
            save(set, calls::reference(set));
        }
        save("active_true", calls::active(true));
        save("active_false", calls::active(false));
        save("tag_list_true", calls::tag_list(true));
        save("history", calls::history("", 0));

        // A closed fleet exercises read, report and doctrine without anything being live.
        let h = b.history("", 0).expect("history");
        if let Some(row) = h.items.first() {
            save("fleet", calls::read(&row.id));
            save("report", calls::report(&row.id));
            save("doctrine", calls::doctrine(&row.id));
        } else {
            println!("no history rows, skipped fleet/report/doctrine");
        }
        let s = b.session().expect("session");
        save("boss_check", calls::boss_check(s.character_id, false));
        // A search is a POST that changes nothing; it is how every type-ahead in the form works.
        // Every search route, so a path that silently starts answering 405 is caught here and
        // not by a type-ahead that quietly returns nothing.
        if let Ok(q) = std::env::var("SPAI_SEARCH") {
            for kind in [SearchKind::Character, SearchKind::Corporation, SearchKind::Alliance,
                         SearchKind::FleetSetup, SearchKind::SolarSystem] {
                let rec = calls::search(kind, &q, false);
                match b.send(&rec, Perm::AccessFleet) {
                    Ok(v) => println!("search {kind:?} {} -> {} hits", rec.path,
                                      v.as_array().map(|a| a.len()).unwrap_or(0)),
                    Err(e) => println!("search {kind:?} {} -> {e}", rec.path),
                }
            }
        }
    }
}

#[cfg(test)]
mod shape_check {
    use super::*;

    /// Deserialise each dumped body into the struct that models it, re-serialise, and compare key
    /// sets. A key of ours the server never sent is a misnamed field; a key of theirs we drop
    /// matters most for `Fleet`, which an edit PUTs back whole.
    #[test]
    fn check() {
        let Ok(dir) = std::env::var("SPAI_DUMP_DIR") else { return };
        let read = |n: &str| -> Option<serde_json::Value> {
            serde_json::from_str(&std::fs::read_to_string(format!("{dir}/{n}.json")).ok()?).ok()
        };
        fn keys(v: &serde_json::Value) -> Vec<String> {
            v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
        }
        fn compare<T: serde::de::DeserializeOwned + serde::Serialize>(
            name: &str,
            theirs: &serde_json::Value,
        ) {
            match serde_json::from_value::<T>(theirs.clone()) {
                Err(e) => println!("PARSE-FAIL {name}: {e}"),
                Ok(t) => {
                    let ours = serde_json::to_value(&t).expect("reserialise");
                    let (tk, ok) = (keys(theirs), keys(&ours));
                    let invented: Vec<_> = ok.iter().filter(|k| !tk.contains(k)).collect();
                    let dropped: Vec<_> = tk.iter().filter(|k| !ok.contains(k)).collect();
                    println!("{name}: invented={invented:?} dropped={dropped:?}");
                }
            }
        }
        fn first(v: &serde_json::Value) -> serde_json::Value {
            v.get("value").and_then(|a| a.get(0)).or_else(|| v.get(0)).cloned().unwrap_or_default()
        }

        if let Some(v) = read("is_authenticated") { compare::<Identity>("identity", &v); }
        if let Some(v) = read("character") { compare::<AccountCharacter>("character", &first(&v)); }
        if let Some(v) = read("sigs") { compare::<Labelled>("sig", &first(&v)); }
        if let Some(v) = read("fleetSetupItem") { compare::<SetupItem>("setup", &first(&v)); }
        if let Some(v) = read("mumbleChannelItem") { compare::<ChannelItem>("channel", &first(&v)); }
        if let Some(v) = read("tagItem") { compare::<TagItem>("tag", &first(&v)); }
        if let Some(v) = read("history") { compare::<FleetRow>("history row", &first(&v)); }
        if let Some(v) = read("fleet") { compare::<Fleet>("fleet", &v); }
        if let Some(v) = read("report") { compare::<FleetReport>("report", &v); }
        if let Some(v) = read("boss_check") { compare::<BossCheck>("boss check", &v); }
        if let Some(v) = read("report") {
            if let Some(c) = v.get("characters").and_then(|a| a.get(0)) {
                compare::<ReportCharacter>("report character", c);
            }
            for (k, n) in [("roleCounts", "role count"), ("shipCounts", "ship count"),
                           ("groupCounts", "group count")] {
                if let Some(c) = v.get(k).and_then(|a| a.get(0)) {
                    match n {
                        "role count" => compare::<RoleCount>(n, c),
                        "ship count" => compare::<ShipCount>(n, c),
                        _ => compare::<GroupCount>(n, c),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod doctrine_probe {
    use super::*;

    /// Classifies a real fleet against the user's own configured doctrine, to see what the
    /// composition tab would call off-doctrine. Opt in with SPAI_DOCTRINE_PROBE=1.
    #[test]
    fn probe() {
        if std::env::var("SPAI_DOCTRINE_PROBE").is_err() {
            return;
        }
        let settings: crate::settings::Settings = {
            let path = std::env::var("SPAI_SETTINGS_JSON").expect("SPAI_SETTINGS_JSON");
            serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("parse")
        };
        let text = crate::fleets::creds::load().expect("no stored session");
        let seed = crate::fleets::seed::load();
        let b = HttpBackend::new(
            Cookies::restore(&text),
            settings.fleet_character.clone(),
            Mode::ReadOnly,
            seed.clone(),
            String::new(),
        )
        .expect("backend");

        let h = b.history("", 0).expect("history");
        let Some(row) = h.items.first() else { return };
        let f = b.fleet(&row.id).expect("fleet");
        let comp = b.composition(&row.id).expect("composition");
        let setup = f.setup_id;
        let name = seed.setup_name(setup).unwrap_or_default().to_owned();
        let tank = settings
            .fleet_doctrine_tanks
            .iter()
            .find(|(id, _)| *id == setup.0)
            .and_then(|(_, t)| crate::fleets::doctrine::Tank::parse(t));
        let d = crate::fleets::doctrine::configured(
            setup,
            &name,
            seed.doctrine(setup),
            &settings.fleet_hulls,
            tank,
            settings.fleet_doctrine_strict.contains(&setup.0),
        );
        println!("setup {} {name:?}: doctrine configured = {}", setup.0, d.is_some());
        if let Some(d) = &d {
            println!("  doctrine hulls: {}, support: {}", d.ships.len(), d.support.len());
        }
        let mut odd: std::collections::BTreeMap<String, usize> = Default::default();
        let mut ok: std::collections::BTreeMap<String, usize> = Default::default();
        for m in comp.members() {
            let st = crate::fleets::doctrine::classify_in(&comp, m, d.as_ref());
            let bucket = if st.odd() { &mut odd } else { &mut ok };
            *bucket.entry(format!("{} [{}]", m.ship_type_name, m.ship_group)).or_default() += 1;
        }
        let top = |m: std::collections::BTreeMap<String, usize>| {
            let mut v: Vec<_> = m.into_iter().collect();
            v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            v.into_iter().take(10).collect::<Vec<_>>()
        };
        println!("  ON doctrine: {:?}", top(ok));
        println!("  OFF doctrine: {:?}", top(odd));
    }
}

#[cfg(test)]
mod boot_probe {
    /// Reproduces what the app does on startup: pick a backend from settings, then boot.
    /// Opt in with SPAI_BOOT_PROBE=1.
    #[test]
    fn probe() {
        if std::env::var("SPAI_BOOT_PROBE").is_err() {
            return;
        }
        let settings: crate::settings::Settings = {
            let p = std::env::var("SPAI_SETTINGS_JSON").expect("SPAI_SETTINGS_JSON");
            serde_json::from_str(&std::fs::read_to_string(p).expect("read")).expect("parse")
        };
        println!("creds_present={}", crate::fleets::creds::has());
        let disk = crate::fleets::seed::load();
        println!("seed::load -> placeholder={} logi={:?}", disk.placeholder,
                 disk.logi_channels.iter().map(|c| c.name.as_str()).take(3).collect::<Vec<_>>());

        let backend = crate::fleets::choose_backend(false, &settings);
        println!("backend mode={:?}", backend.mode());
        match crate::fleets::state::run(
            backend.as_ref(),
            &disk,
            crate::fleets::state::Cmd::Bootstrap,
        ) {
            crate::fleets::state::Outcome::Booted { seed, .. } => {
                println!("BOOTED placeholder={} logi={:?} boost={:?}", seed.placeholder,
                         seed.logi_channels.iter().map(|c| c.name.as_str()).take(3).collect::<Vec<_>>(),
                         seed.boost_channels.iter().map(|c| c.name.as_str()).take(3).collect::<Vec<_>>());
            }
            other => println!("BOOT FAILED: {other:?}"),
        }
    }
}











