//! The Lookup table: every pilot of a pasted local list summarised from one zKillboard stats call
//! each. Killmails are left to the detailed report, which only runs for a row the user opens.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

const ESI: &str = "https://esi.evetech.net/latest";
/// Parallel zKillboard stats calls. One call can stall for a minute on zKillboard's side, so more
/// workers keep one slow pilot from holding up the rest.
const WORKERS: usize = 5;
/// Parallel ESI corp and alliance lookups, kept apart so tickers never wait behind stats.
const ORG_WORKERS: usize = 3;
/// A stats call that has not answered by now is retried later instead of waited for.
const ZKILL_TIMEOUT_SECS: u64 = 10;
const ZKILL_TRIES: u32 = 3;

/// zKillboard's attacker-count labels. Each kill carries exactly one.
pub const GROUPS: [(&str, &str); 8] = [
    ("#:1", "solo"),
    ("#:2+", "2-4"),
    ("#:5+", "5-9"),
    ("#:10+", "10-24"),
    ("#:25+", "25-49"),
    ("#:50+", "50-99"),
    ("#:100+", "100-999"),
    ("#:1000+", "1000+"),
];
pub const SPACE: [(&str, &str); 6] = [
    ("loc:highsec", "High sec"),
    ("loc:lowsec", "Low sec"),
    ("loc:nullsec", "Null sec"),
    ("loc:w-space", "Wormhole"),
    ("loc:pochven", "Pochven"),
    ("loc:abyssal", "Abyssal"),
];
pub const ISK: [(&str, &str); 4] =
    [("isk:under1b", "under 1b"), ("isk:1b+", "1b-5b"), ("isk:5b+", "5b-10b"), ("isk:10b+", "10b+")];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bar {
    pub kills: u32,
    pub losses: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ship {
    pub type_id: i64,
    pub group_id: i64,
    pub kills: u32,
    pub losses: u32,
}

/// zKillboard's character labels, as its profile page shows them. zKillboard works these out from
/// full killmails over their own windows (90 days or a year); the app only reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Blops,
    Logi,
    Capital,
    Super,
    Titan,
    Cyno,
    Fc,
    Bait,
    Awox,
    AllianceAwox,
    FactionAwox,
    Ganker,
    Rookie,
}

impl Tag {
    pub fn label(self) -> &'static str {
        match self {
            Tag::Blops => "BLOPS",
            Tag::Logi => "LOGI",
            Tag::Capital => "CAPITAL",
            Tag::Super => "SUPER",
            Tag::Titan => "TITAN",
            Tag::Cyno => "CYNO",
            Tag::Fc => "FC",
            Tag::Bait => "BAIT",
            Tag::Awox => "AWOX",
            Tag::AllianceAwox => "ALLIANCE AWOX",
            Tag::FactionAwox => "FACTION AWOX",
            Tag::Ganker => "GANKER",
            Tag::Rookie => "ROOKIE",
        }
    }

    /// Display order, most telling first: when a row has more labels than fit, the tail is
    /// what gets folded into "+N".
    pub fn rank(self) -> u8 {
        match self {
            Tag::Fc => 0,
            Tag::Cyno => 1,
            Tag::Bait => 2,
            Tag::FactionAwox => 3,
            Tag::AllianceAwox => 4,
            Tag::Awox => 5,
            Tag::Ganker => 6,
            Tag::Titan => 7,
            Tag::Super => 8,
            Tag::Capital => 9,
            Tag::Blops => 10,
            Tag::Logi => 11,
            Tag::Rookie => 12,
        }
    }

    /// What zKillboard counted for the label.
    pub fn explain(self) -> &'static str {
        match self {
            Tag::Blops => "Combat appearances in a Black Ops battleship, past 90 days",
            Tag::Logi => "Combat appearances in a logistics cruiser or frigate, past 90 days",
            Tag::Capital => "Combat appearances in a carrier, dreadnought, force auxiliary, supercarrier or titan, past 90 days",
            Tag::Super => "Combat appearances in a supercarrier, past 90 days",
            Tag::Titan => "Combat appearances in a titan, past 90 days",
            Tag::Cyno => "Ships lost with a cynosural field fitted, past year",
            Tag::Fc => "Fleet-command signal from Monitor, command ship and large-fleet appearances, past year",
            Tag::Bait => "Cheap losses followed within five minutes by a fight of three or more nearby, past year",
            Tag::Awox => "Final blows on their own corporation, past year",
            Tag::AllianceAwox => "Final blows on their own alliance, past year",
            Tag::FactionAwox => "Final blows on their own faction, past year",
            Tag::Ganker => "High-sec gank killmails as an attacker, past year",
            Tag::Rookie => "Under 180 days old, more losses than kills in the past 90 days",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cyno {
    pub standard: u32,
    pub covert: u32,
    pub industrial: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fc {
    pub level: String,
    pub score: u32,
    pub monitor: u32,
    pub command: u32,
    pub large_fleet: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bait {
    pub level: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub id: i64,
    pub name: String,
    pub birthday: Option<i64>,
    pub security: Option<f64>,
    pub corp_id: i64,
    pub alliance_id: i64,
    pub faction_id: i64,
    pub danger: u32,
    pub gang: u32,
    pub avg_gang: f64,
    pub solo: u32,
    pub kills: u32,
    pub losses: u32,
    pub isk_destroyed: f64,
    pub isk_lost: f64,
    pub groups: [Bar; 8],
    pub space: [Bar; 6],
    pub isk: [Bar; 4],
    pub ships: Vec<Ship>,
    pub affiliates: Vec<(i64, u32)>,
    /// zKillboard's affiliate list stops at 25 alliances; `true` when it was full.
    pub affiliates_capped: bool,
    pub cyno: Option<Cyno>,
    pub fc: Option<Fc>,
    pub bait: Option<Bait>,
    /// Final blows on their own corporation, alliance and faction.
    pub awox: [u32; 3],
    pub ganker: u32,
    /// Labels with the count zKillboard shows beside them, 0 for none.
    pub tags: Vec<(Tag, u32)>,
}

fn int(v: &serde_json::Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64))).unwrap_or(0)
}

fn float(v: &serde_json::Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn bar(labels: &serde_json::Value, key: &str) -> Bar {
    let Some(l) = labels.get(key) else { return Bar::default() };
    Bar { kills: int(l, "shipsDestroyed") as u32, losses: int(l, "shipsLost") as u32 }
}

fn ships(list: Option<&serde_json::Value>) -> Vec<Ship> {
    list.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    Some(Ship {
                        type_id: s.get("shipTypeID")?.as_i64()?,
                        group_id: int(s, "groupID"),
                        kills: int(s, "kills") as u32,
                        losses: int(s, "losses") as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A zKillboard `/api/stats/characterID/` answer, reduced to the table's columns. A pilot with no
/// kills gets an answer without most keys, which reads as zeros.
pub fn parse_stats(id: i64, name: &str, v: &serde_json::Value) -> Summary {
    let empty = serde_json::Value::Null;
    let labels = v.get("labels").unwrap_or(&empty);
    let info = v.get("info").unwrap_or(&empty);
    let ts = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp());
    let mut recent = ships(v.get("recentShips"));
    if recent.is_empty() {
        recent = ships(v.get("topShips"));
    }
    let empty_obj = serde_json::Value::Null;
    let obj = |k: &str| v.get(k).filter(|x| x.is_object());
    let cyno = obj("cyno").map(|c| Cyno {
        standard: int(c, "standard") as u32,
        covert: int(c, "covert") as u32,
        industrial: int(c, "industrial") as u32,
    });
    let fc = obj("fc").map(|f| Fc {
        level: f.get("level").and_then(|l| l.as_str()).unwrap_or_default().to_owned(),
        score: int(f, "score") as u32,
        monitor: int(f, "monitorAppearances") as u32,
        command: int(f, "commandShipAppearances") as u32,
        large_fleet: int(f, "largeFleetAppearances") as u32,
    });
    let bait = obj("bait").map(|b| Bait {
        level: b.get("level").and_then(|l| l.as_str()).unwrap_or_default().to_owned(),
        count: int(b, "count") as u32,
    });
    let awox = [int(v, "awoxCount") as u32, int(v, "allianceAwoxCount") as u32, int(v, "factionAwoxCount") as u32];
    let ganker = int(v, "gankerCount") as u32;
    // zKillboard's own rules, so the badges read as they do on its profile page.
    let activity = v.get("activityTags").unwrap_or(&empty_obj);
    let mut tags: Vec<(Tag, u32)> = [
        ("blops", Tag::Blops),
        ("logi", Tag::Logi),
        ("capital", Tag::Capital),
        ("super", Tag::Super),
        ("titan", Tag::Titan),
    ]
    .into_iter()
    .map(|(k, t)| (t, int(activity, k) as u32))
    .filter(|(_, n)| *n > 0)
    .collect();
    if let Some(c) = &cyno {
        tags.push((Tag::Cyno, c.standard + c.covert + c.industrial));
    }
    if fc.is_some() {
        tags.push((Tag::Fc, 0));
    }
    if let Some(b) = &bait {
        tags.push((Tag::Bait, b.count));
    }
    for (i, (tag, min)) in [(Tag::Awox, 10), (Tag::AllianceAwox, 15), (Tag::FactionAwox, 20)].into_iter().enumerate() {
        if awox[i] >= min {
            tags.push((tag, awox[i]));
        }
    }
    if ganker >= 10 {
        tags.push((Tag::Ganker, ganker));
    }
    let birthday = info.get("birthday").and_then(|b| b.as_str()).and_then(ts);
    let recent_metrics = v.pointer("/rankings/recent/all/metrics").unwrap_or(&empty_obj);
    let young = birthday.is_some_and(|b| chrono::Utc::now().timestamp() - b < 180 * 86_400);
    let big_label = tags.iter().any(|(t, _)| matches!(t, Tag::Capital | Tag::Super | Tag::Titan | Tag::Cyno | Tag::Bait));
    if young && int(recent_metrics, "shipsLost") > int(recent_metrics, "shipsDestroyed") && !big_label {
        tags.push((Tag::Rookie, 0));
    }
    tags.sort_by_key(|(t, _)| t.rank());
    let affiliates = v
        .get("affiliates")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| Some((x.get("allianceID")?.as_i64()?, int(x, "sharedKills") as u32)))
                .filter(|(id, _)| *id != 0)
                .collect()
        })
        .unwrap_or_default();
    let secs = info.get("secStatus").or(info.get("security_status")).and_then(|x| x.as_f64());
    Summary {
        id,
        name: info.get("name").and_then(|n| n.as_str()).unwrap_or(name).to_owned(),
        birthday,
        security: secs,
        corp_id: int(info, "corporationID").max(int(info, "corporation_id")),
        alliance_id: int(info, "allianceID").max(int(info, "alliance_id")),
        faction_id: int(info, "factionID").max(int(info, "faction_id")),
        danger: int(v, "dangerRatio") as u32,
        gang: int(v, "gangRatio") as u32,
        avg_gang: float(v, "avgGangSize"),
        solo: int(v, "soloKills") as u32,
        kills: int(v, "shipsDestroyed") as u32,
        losses: int(v, "shipsLost") as u32,
        isk_destroyed: float(v, "iskDestroyed"),
        isk_lost: float(v, "iskLost"),
        groups: GROUPS.map(|(k, _)| bar(labels, k)),
        space: SPACE.map(|(k, _)| bar(labels, k)),
        isk: ISK.map(|(k, _)| bar(labels, k)),
        ships: recent,
        affiliates,
        affiliates_capped: v.get("affiliates").and_then(|a| a.as_array()).is_some_and(|a| a.len() >= 25),
        cyno,
        fc,
        bait,
        awox,
        ganker,
        tags,
    }
}

impl Summary {
    pub fn kd(&self) -> f64 {
        self.kills as f64 / self.losses.max(1) as f64
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Pending,
    Done(Box<Summary>),
    /// No character by that name.
    Missing,
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct Org {
    pub name: String,
    pub ticker: String,
}

/// Everything looked up this session, keyed by lowercased pilot name. Never written to disk.
#[derive(Default)]
pub struct Table {
    pub rows: HashMap<String, Row>,
    pub orgs: HashMap<i64, Org>,
    queue: VecDeque<Job>,
    stat_workers: usize,
    org_queue: VecDeque<(i64, bool)>,
    org_workers: usize,
    orgs_wanted: HashSet<i64>,
}

pub type SharedTable = Arc<Mutex<Table>>;

struct Job {
    name: String,
    id: i64,
    tries: u32,
}

/// Names of a pasted local member list: the first column of each line that could be a character.
pub fn names_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let name = line.split('\t').next().unwrap_or(line).trim();
        if crate::dscan::is_valid_char_name(name) && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push(name.to_owned());
        }
    }
    out
}

/// Whether `new` reads as the same local a moment later: most of either list is in the other.
pub fn similar(old: &[String], new: &[String]) -> bool {
    if old.is_empty() || new.is_empty() {
        return false;
    }
    let a: HashSet<String> = old.iter().map(|n| n.to_lowercase()).collect();
    let b: HashSet<String> = new.iter().map(|n| n.to_lowercase()).collect();
    let shared = a.intersection(&b).count();
    shared * 2 >= a.len().min(b.len()) && shared * 3 >= a.len().max(b.len())
}

/// Queues `names` for lookup, skipping pilots already known this session.
pub fn request(table: &SharedTable, names: &[String], ctx: &egui::Context) {
    let fresh: Vec<String> = {
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        names
            .iter()
            .filter(|n| !matches!(t.rows.get(&n.to_lowercase()), Some(Row::Pending | Row::Done(_) | Row::Missing)))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .inspect(|n| {
                t.rows.insert(n.to_lowercase(), Row::Pending);
            })
            .collect()
    };
    if fresh.is_empty() {
        return;
    }
    let table = table.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || resolve_and_fetch(table, fresh, ctx));
}

fn resolve_and_fetch(table: SharedTable, names: Vec<String>, ctx: egui::Context) {
    let Ok(client) = crate::http::client(ZKILL_TIMEOUT_SECS) else {
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        for n in &names {
            t.rows.insert(n.to_lowercase(), Row::Failed("no HTTP client".into()));
        }
        return;
    };
    let mut ids: HashMap<String, (i64, String)> = HashMap::new();
    let mut failed = false;
    for chunk in names.chunks(150) {
        match crate::universe::character_ids(&client, chunk) {
            Some(found) => ids.extend(found),
            None => failed = true,
        }
    }
    let spawn = {
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        // Newest paste first: its pilots go ahead of anything still queued from an older one.
        for n in names.iter().rev() {
            let lc = n.to_lowercase();
            match ids.get(&lc) {
                Some((id, canonical)) => t.queue.push_front(Job { name: canonical.clone(), id: *id, tries: 0 }),
                None if failed => {
                    t.rows.insert(lc, Row::Failed("name lookup failed".into()));
                }
                None => {
                    t.rows.insert(lc, Row::Missing);
                }
            }
        }
        let spawn = WORKERS.saturating_sub(t.stat_workers).min(t.queue.len());
        t.stat_workers += spawn;
        spawn
    };
    ctx.request_repaint();
    for _ in 0..spawn {
        let (table, ctx, client) = (table.clone(), ctx.clone(), client.clone());
        std::thread::spawn(move || worker(table, client, ctx));
    }
}

fn worker(table: SharedTable, client: reqwest::blocking::Client, ctx: egui::Context) {
    loop {
        let job = {
            let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
            let job = t.queue.pop_front();
            if job.is_none() {
                t.stat_workers -= 1;
            }
            job
        };
        let Some(mut job) = job else { break };
        let result = fetch_summary(&client, job.id, &job.name);
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(s) => {
                let orgs = [(s.corp_id, false), (s.alliance_id, true)]
                    .into_iter()
                    .chain(s.affiliates.iter().take(6).map(|(a, _)| (*a, true)));
                let wanted: Vec<(i64, bool)> =
                    orgs.filter(|(id, _)| *id > 0 && t.orgs_wanted.insert(*id)).collect();
                t.org_queue.extend(wanted);
                t.rows.insert(job.name.to_lowercase(), Row::Done(Box::new(s)));
            }
            // To the back: whatever made zKillboard slow for this one should not stall the rest.
            Err(_) if job.tries + 1 < ZKILL_TRIES => {
                job.tries += 1;
                t.queue.push_back(job);
            }
            Err(e) => {
                t.rows.insert(job.name.to_lowercase(), Row::Failed(e));
            }
        }
        let spawn_orgs = ORG_WORKERS.saturating_sub(t.org_workers).min(t.org_queue.len());
        t.org_workers += spawn_orgs;
        drop(t);
        for _ in 0..spawn_orgs {
            let (table, ctx, client) = (table.clone(), ctx.clone(), client.clone());
            std::thread::spawn(move || org_worker(table, client, ctx));
        }
        ctx.request_repaint();
    }
}

fn org_worker(table: SharedTable, client: reqwest::blocking::Client, ctx: egui::Context) {
    loop {
        let next = {
            let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
            let next = t.org_queue.pop_front();
            if next.is_none() {
                t.org_workers -= 1;
            }
            next
        };
        let Some((org, alliance)) = next else { break };
        if let Some(o) = fetch_org(&client, org, alliance) {
            table.lock().unwrap_or_else(|e| e.into_inner()).orgs.insert(org, o);
            ctx.request_repaint();
        }
    }
}

fn fetch_summary(client: &reqwest::blocking::Client, id: i64, name: &str) -> Result<Summary, String> {
    let url = format!("https://zkillboard.com/api/stats/characterID/{id}/");
    let r = client.get(&url).send().map_err(|e| format!("zKillboard: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("zKillboard {}", r.status()));
    }
    let v: serde_json::Value = r.json().map_err(|e| format!("zKillboard: {e}"))?;
    let mut s = parse_stats(id, name, &v);
    if s.birthday.is_none() || s.corp_id == 0 {
        fill_from_esi(client, &mut s);
    }
    Ok(s)
}

/// zKillboard knows nothing of a pilot it has never seen on a killmail, so ESI fills the sheet in.
fn fill_from_esi(client: &reqwest::blocking::Client, s: &mut Summary) {
    #[derive(serde::Deserialize)]
    struct Sheet {
        name: Option<String>,
        birthday: Option<String>,
        corporation_id: Option<i64>,
        alliance_id: Option<i64>,
        faction_id: Option<i64>,
        security_status: Option<f64>,
    }
    let Some(sheet) = client
        .get(format!("{ESI}/characters/{}/", s.id))
        .send()
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .and_then(|r| r.json::<Sheet>().ok())
    else {
        return;
    };
    let ts = |b: &str| chrono::DateTime::parse_from_rfc3339(b).ok().map(|d| d.timestamp());
    s.name = sheet.name.unwrap_or(std::mem::take(&mut s.name));
    s.birthday = s.birthday.or(sheet.birthday.as_deref().and_then(ts));
    s.security = s.security.or(sheet.security_status);
    s.corp_id = sheet.corporation_id.unwrap_or(s.corp_id);
    s.alliance_id = sheet.alliance_id.unwrap_or(s.alliance_id);
    s.faction_id = sheet.faction_id.unwrap_or(s.faction_id);
}

fn fetch_org(client: &reqwest::blocking::Client, id: i64, alliance: bool) -> Option<Org> {
    #[derive(serde::Deserialize)]
    struct Sheet {
        name: String,
        ticker: String,
    }
    let kind = if alliance { "alliances" } else { "corporations" };
    let sheet: Sheet = client
        .get(format!("{ESI}/{kind}/{id}/"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    Some(Org { name: sheet.name, ticker: sheet.ticker })
}

/// Whether `url` is a shared local scan rather than a d-scan.
pub fn is_local_scan_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    u.contains("adashboard.info/intel/local/") || u.contains("localthreat.xyz/")
}

fn url_id<'a>(url: &'a str, after: &str) -> Option<&'a str> {
    let at = url.to_ascii_lowercase().find(after)? + after.len();
    let id = url[at..].split(['/', '#', '?']).next()?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then_some(id)
}

/// The pilot names behind a shared local scan.
pub fn fetch_local_scan(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<String>, String> {
    if let Some(id) = url_id(url, "localthreat.xyz/") {
        #[derive(serde::Deserialize)]
        struct Report {
            content: Vec<String>,
        }
        let r: Report = client
            .get(format!("https://api.localthreat.xyz/v1/reports/{id}"))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("localthreat: {e}"))?;
        return Ok(r.content);
    }
    if let Some(id) = url_id(url, "adashboard.info/intel/local/view/") {
        // The page loads its member list separately; this is the fragment it fetches.
        let html = client
            .get(format!("https://adashboard.info/intel/local/a_details/members/{id}/this"))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
            .map_err(|e| format!("adashboard: {e}"))?;
        return Ok(page_pilots(&html));
    }
    Err("not a local scan link".into())
}

/// A scan page that did not parse as a d-scan, read as a local scan instead.
pub fn fetch_page_pilots(url: &str) -> Vec<String> {
    let Ok(client) = crate::http::client(20) else { return Vec::new() };
    client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map(|html| page_pilots(&html))
        .unwrap_or_default()
}

/// Pilot names from a scan page, where each pilot's portrait carries the name as its title or alt
/// text. adashboard's member table is built that way.
fn page_pilots(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut at = 0;
    while let Some(found) = lower[at..].find("/character") {
        let start = at + found;
        at = start + 10;
        let tag_end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..tag_end];
        let tag_lower = &lower[start..tag_end];
        let Some(attr) = tag_lower.find("title=\"").or_else(|| tag_lower.find("alt=\"")) else { continue };
        let value_start = attr + tag_lower[attr..].find('"').unwrap_or(0) + 1;
        let Some(len) = tag[value_start..].find('"') else { continue };
        let name = decode_html(&tag[value_start..value_start + len]);
        if crate::dscan::is_valid_char_name(&name) && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn decode_html(s: &str) -> String {
    s.replace("&#39;", "'").replace("&#039;", "'").replace("&apos;", "'").replace("&quot;", "\"").replace("&amp;", "&")
}

#[cfg(test)]
pub(crate) fn sample_stats() -> serde_json::Value {
    serde_json::json!({
        "info": {"name": "Fixture Pilot", "birthday": "2019-03-01T12:00:00Z", "secStatus": -2.4,
                 "corporationID": 98000001, "allianceID": 99000001, "factionID": 0},
        "dangerRatio": 87, "gangRatio": 92, "avgGangSize": 14.2, "soloKills": 31,
        "shipsDestroyed": 400, "shipsLost": 50, "iskDestroyed": 9.0e10, "iskLost": 4.0e9,
        "labels": {
            "#:1": {"shipsDestroyed": 31, "shipsLost": 9},
            "#:10+": {"shipsDestroyed": 200, "shipsLost": 20},
            "loc:nullsec": {"shipsDestroyed": 350, "shipsLost": 40},
            "loc:w-space": {"shipsDestroyed": 10},
            "isk:under1b": {"shipsDestroyed": 300, "shipsLost": 45}
        },
        "activityTags": {"blops": 3, "logi": 12, "capital": 0},
        "cyno": {"count": 2, "standard": 1, "covert": 1, "industrial": 0},
        "fc": {"level": "medium", "score": 72, "monitorAppearances": 3, "commandShipAppearances": 2, "largeFleetAppearances": 40},
        "awoxCount": 11, "allianceAwoxCount": 4, "factionAwoxCount": 0, "gankerCount": 5,
        "recentShips": [
            {"shipTypeID": 22430, "groupID": 898, "kills": 12, "losses": 1},
            {"shipTypeID": 11978, "groupID": 832, "kills": 3, "losses": 0}
        ],
        "topShips": [
            {"shipTypeID": 11963, "groupID": 833, "kills": 40, "losses": 2},
            {"shipTypeID": 22430, "groupID": 898, "kills": 99, "losses": 3},
            {"shipTypeID": 583, "groupID": 25, "kills": 0, "losses": 4}
        ],
        "affiliates": [{"allianceID": 99000002, "sharedKills": 40}, {"allianceID": 0, "sharedKills": 3}],
        "associates": [{"characterID": 1}, {"characterID": 2}, {"characterID": 3}]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_fill_every_column() {
        let s = parse_stats(7, "fixture pilot", &sample_stats());
        assert_eq!(s.name, "Fixture Pilot");
        assert_eq!((s.danger, s.gang, s.solo, s.kills, s.losses), (87, 92, 31, 400, 50));
        assert_eq!(s.security, Some(-2.4));
        assert_eq!((s.corp_id, s.alliance_id), (98000001, 99000001));
        assert_eq!(s.groups[0], Bar { kills: 31, losses: 9 });
        assert_eq!(s.groups[3], Bar { kills: 200, losses: 20 });
        assert_eq!(s.space[2].kills, 350);
        assert_eq!(s.space[3], Bar { kills: 10, losses: 0 });
        assert_eq!(s.isk[0].kills, 300);
        assert_eq!(s.affiliates, vec![(99000002, 40)], "an unaffiliated share is not an alliance");
        assert!(!s.affiliates_capped);
        assert_eq!(s.ships.len(), 2, "recent ships, not the all-time list");
    }

    #[test]
    fn zkillboards_labels_are_read_as_given() {
        let s = parse_stats(7, "x", &sample_stats());
        assert_eq!(s.cyno, Some(Cyno { standard: 1, covert: 1, industrial: 0 }));
        let fc = s.fc.clone().expect("fc label");
        assert_eq!((fc.level.as_str(), fc.score, fc.monitor, fc.command, fc.large_fleet), ("medium", 72, 3, 2, 40));
        assert_eq!((s.awox, s.ganker), ([11, 4, 0], 5));
        assert_eq!(
            s.tags,
            vec![(Tag::Fc, 0), (Tag::Cyno, 2), (Tag::Awox, 11), (Tag::Blops, 3), (Tag::Logi, 12)],
            "zero counts drop out; awox and ganker only past zKillboard's thresholds"
        );
    }

    #[test]
    fn a_young_losing_pilot_is_a_rookie_unless_a_big_label_applies() {
        let born = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let mut v = serde_json::json!({
            "info": {"birthday": born},
            "rankings": {"recent": {"all": {"metrics": {"shipsDestroyed": 1, "shipsLost": 4}}}}
        });
        assert_eq!(parse_stats(7, "x", &v).tags, vec![(Tag::Rookie, 0)]);
        v["cyno"] = serde_json::json!({"count": 1, "standard": 1});
        assert!(!parse_stats(7, "x", &v).tags.iter().any(|(t, _)| *t == Tag::Rookie));
    }

    #[test]
    fn a_pilot_without_kills_reads_as_zeros() {
        let s = parse_stats(7, "Quiet One", &serde_json::json!({}));
        assert_eq!((s.name.as_str(), s.kills, s.danger), ("Quiet One", 0, 0));
        assert!(s.ships.is_empty() && s.tags.is_empty());
    }

    #[test]
    fn local_names_are_first_columns_that_could_be_characters() {
        let got = names_of("Alpha One\tcorp\nBeta\nalpha one\n\nthis line is far too long to be anybody at all");
        assert_eq!(got, vec!["Alpha One".to_owned(), "Beta".to_owned()]);
    }

    #[test]
    fn local_scan_links_are_told_from_dscans() {
        assert!(is_local_scan_url("https://adashboard.info/intel/local/view/AbC123#zg-last"));
        assert!(is_local_scan_url("https://localthreat.xyz/AbC123"));
        assert!(!is_local_scan_url("https://adashboard.info/intel/dscan/view/AbC123"));
        assert!(!is_local_scan_url("https://dscan.info/v/abc123"));
        assert_eq!(url_id("https://adashboard.info/intel/local/view/AbC123#zg-last", "adashboard.info/intel/local/view/"), Some("AbC123"));
    }

    #[test]
    fn adashboard_members_come_from_portrait_titles() {
        let html = r#"<tr><td><img src="https://image.eveonline.com/Character/1_32.jpg" title="Fake Pilot"></td>
            <td><img src="https://image.eveonline.com/Corporation/2_32.png" title="CORP"></td></tr>
            <tr><td><img src="https://image.eveonline.com/Character/3_32.jpg" title="O&#39;Neill"></td></tr>"#;
        assert_eq!(page_pilots(html), vec!["Fake Pilot".to_owned(), "O'Neill".to_owned()]);
    }

    #[test]
    fn a_local_a_moment_later_is_similar() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let old = v(&["a", "b", "c", "d", "e", "f"]);
        assert!(similar(&old, &v(&["a", "b", "c", "d", "e", "g"])), "one pilot swapped");
        assert!(similar(&old, &v(&["A", "B", "C", "D"])), "some left, case does not matter");
        assert!(!similar(&old, &v(&["x", "y", "z", "a"])), "a different local");
        assert!(!similar(&v(&["a"]), &v(&["a", "b", "c", "d", "e", "f", "g", "h"])), "a far bigger local");
        assert!(!similar(&[], &old));
    }
}
