//! The Lookup table: every pilot of a pasted local list summarised from one zKillboard stats call
//! each. Killmails are left to the detailed report, which only runs for a row the user opens.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

const ESI: &str = "https://esi.evetech.net/latest";
const WORKERS: usize = 3;

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

const BLOPS: &[i64] = &[898];
const LOGI: &[i64] = &[832, 1527, 1538];
const CAPITAL: &[i64] = &[30, 485, 547, 659, 883, 1538, 4594];
const COVERT_CYNO: &[i64] = &[830, 833];
/// Cheap hulls a cyno alt lights in and loses.
const CYNO: &[i64] = &[25, 28];
/// FC hulls by type, since zKillboard's all-time ship list carries no group: the command ships,
/// the Monitor, and the HACs an FC takes when the fleet flies smaller ships (Vagabond, Muninn,
/// Deimos).
const FC_TYPES: &[i64] = &[22442, 22444, 22446, 22448, 22466, 22468, 22470, 22474, 45534, 11999, 12015, 12023];
/// Kills and losses in fleets of 25 or more before an FC-hull pattern counts. A line member in a
/// Muninn fleet flies the same hull as the FC; the pattern only means something from someone who
/// is in large fleets a lot.
const LARGE_FLEET_MIN: u32 = 10;
/// Appearances in a group before it counts as something the pilot does.
const TAG_MIN: u32 = 2;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Blops,
    Logi,
    Capital,
    Cyno,
    Fc,
}

impl Tag {
    pub fn label(self) -> &'static str {
        match self {
            Tag::Blops => "BLOPS",
            Tag::Logi => "LOGI",
            Tag::Capital => "CAPITAL",
            Tag::Cyno => "CYNO",
            Tag::Fc => "FC",
        }
    }
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
    pub associates: u32,
    pub awox: Bar,
    pub gank: Bar,
    pub covert_cyno: u32,
    pub cyno: u32,
    pub fc: u32,
    /// Kills and losses in fleets of 25 or more.
    pub large_fleet: u32,
    pub tags: Vec<Tag>,
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
    let top = ships(v.get("topShips"));
    // Recent first; the all-time list fills in for a pilot who has been quiet this week.
    let mut flown = recent.clone();
    for s in &top {
        if !flown.iter().any(|f| f.type_id == s.type_id) {
            flown.push(s.clone());
        }
    }
    if recent.is_empty() {
        recent = top;
    }
    // Losses per ship group, complete where the ship lists stop at nine hulls. zKillboard's
    // destroyed count per group is the victims' hulls, so only losses say what the pilot flew.
    let group_losses = |groups: &[i64]| -> u32 {
        groups
            .iter()
            .map(|g| v.get("groups").and_then(|m| m.get(g.to_string())).map_or(0, |e| int(e, "shipsLost") as u32))
            .sum()
    };
    let appear = |groups: &[i64]| -> u32 {
        let listed: u32 = flown.iter().filter(|s| groups.contains(&s.group_id)).map(|s| s.kills + s.losses).sum();
        listed.max(group_losses(groups))
    };
    let lost_in = |groups: &[i64]| -> u32 {
        let listed: u32 = flown.iter().filter(|s| groups.contains(&s.group_id)).map(|s| s.losses).sum();
        listed.max(group_losses(groups))
    };
    let covert_cyno = appear(COVERT_CYNO);
    let cyno = lost_in(CYNO);
    let groups = GROUPS.map(|(k, _)| bar(labels, k));
    let large_fleet: u32 = groups[4..].iter().map(|b| b.kills + b.losses).sum();
    // The recent and top lists stop at nine hulls, so a Monitor flown now and then only shows in
    // the all-time list, which counts kills alone.
    let all_time: HashMap<i64, u32> = v
        .get("topAllTime")
        .and_then(|t| t.as_array())
        .and_then(|lists| lists.iter().find(|l| l.get("type").and_then(|t| t.as_str()) == Some("ship")))
        .and_then(|l| l.get("data")?.as_array())
        .map(|a| a.iter().filter_map(|x| Some((x.get("shipTypeID")?.as_i64()?, int(x, "kills") as u32))).collect())
        .unwrap_or_default();
    let fc = if large_fleet >= LARGE_FLEET_MIN {
        FC_TYPES
            .iter()
            .map(|t| {
                let listed = flown.iter().filter(|s| s.type_id == *t).map(|s| s.kills + s.losses).sum::<u32>();
                listed.max(all_time.get(t).copied().unwrap_or(0))
            })
            .sum()
    } else {
        0
    };
    let mut tags = Vec::new();
    if appear(LOGI) >= TAG_MIN {
        tags.push(Tag::Logi);
    }
    if appear(BLOPS) >= TAG_MIN {
        tags.push(Tag::Blops);
    }
    if appear(CAPITAL) >= TAG_MIN {
        tags.push(Tag::Capital);
    }
    if covert_cyno >= TAG_MIN || cyno >= TAG_MIN {
        tags.push(Tag::Cyno);
    }
    if fc >= TAG_MIN {
        tags.push(Tag::Fc);
    }
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
        birthday: info.get("birthday").and_then(|b| b.as_str()).and_then(ts),
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
        groups,
        space: SPACE.map(|(k, _)| bar(labels, k)),
        isk: ISK.map(|(k, _)| bar(labels, k)),
        ships: recent,
        affiliates,
        associates: v.get("associates").and_then(|a| a.as_array()).map_or(0, |a| a.len() as u32),
        awox: bar(labels, "awox"),
        gank: bar(labels, "ganked"),
        covert_cyno,
        cyno,
        fc,
        large_fleet,
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
    queue: VecDeque<(String, i64)>,
    orgs_wanted: HashSet<i64>,
}

pub type SharedTable = Arc<Mutex<Table>>;

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
    let Ok(client) = crate::http::client(20) else {
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
    let start_workers = {
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        let idle = t.queue.is_empty();
        // Newest paste first: its pilots go ahead of anything still queued from an older one.
        for n in names.iter().rev() {
            let lc = n.to_lowercase();
            match ids.get(&lc) {
                Some((id, canonical)) => t.queue.push_front((canonical.clone(), *id)),
                None if failed => {
                    t.rows.insert(lc, Row::Failed("name lookup failed".into()));
                }
                None => {
                    t.rows.insert(lc, Row::Missing);
                }
            }
        }
        idle
    };
    ctx.request_repaint();
    if start_workers {
        for _ in 0..WORKERS {
            let (table, ctx, client) = (table.clone(), ctx.clone(), client.clone());
            std::thread::spawn(move || worker(table, client, ctx));
        }
    }
}

fn worker(table: SharedTable, client: reqwest::blocking::Client, ctx: egui::Context) {
    loop {
        let job = table.lock().unwrap_or_else(|e| e.into_inner()).queue.pop_front();
        let Some((name, id)) = job else { break };
        let row = match fetch_summary(&client, id, &name) {
            Ok(s) => Row::Done(Box::new(s)),
            Err(e) => Row::Failed(e),
        };
        let orgs: Vec<(i64, bool)> = match &row {
            Row::Done(s) => {
                let mut v = vec![(s.corp_id, false), (s.alliance_id, true)];
                v.extend(s.affiliates.iter().take(6).map(|(a, _)| (*a, true)));
                v
            }
            _ => Vec::new(),
        };
        let wanted: Vec<(i64, bool)> = {
            let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
            t.rows.insert(name.to_lowercase(), row);
            orgs.into_iter().filter(|(id, _)| *id > 0 && t.orgs_wanted.insert(*id)).collect()
        };
        ctx.request_repaint();
        for (org, alliance) in wanted {
            if let Some(o) = fetch_org(&client, org, alliance) {
                table.lock().unwrap_or_else(|e| e.into_inner()).orgs.insert(org, o);
                ctx.request_repaint();
            }
        }
    }
}

fn fetch_summary(client: &reqwest::blocking::Client, id: i64, name: &str) -> Result<Summary, String> {
    let url = format!("https://zkillboard.com/api/stats/characterID/{id}/");
    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(1500 * attempt));
        }
        match client.get(&url).send() {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r.json().map_err(|e| format!("zKillboard: {e}"))?;
                let mut s = parse_stats(id, name, &v);
                if s.birthday.is_none() || s.corp_id == 0 {
                    fill_from_esi(client, &mut s);
                }
                return Ok(s);
            }
            Ok(r) => last = format!("zKillboard {}", r.status()),
            Err(e) => last = format!("zKillboard: {e}"),
        }
    }
    Err(last)
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
            "isk:under1b": {"shipsDestroyed": 300, "shipsLost": 45},
            "awox": {"shipsDestroyed": 2, "shipsLost": 1},
            "ganked": {"shipsDestroyed": 5}
        },
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
        assert_eq!(s.awox, Bar { kills: 2, losses: 1 });
        assert_eq!(s.gank.kills, 5);
        assert_eq!(s.affiliates, vec![(99000002, 40)], "an unaffiliated share is not an alliance");
        assert_eq!(s.associates, 3);
        assert_eq!(s.ships.len(), 2, "recent ships, not the all-time list");
        assert_eq!(s.covert_cyno, 42, "force recon appearances from the all-time list");
        assert_eq!(s.cyno, 4, "T1 frigate losses");
        assert!(s.tags.contains(&Tag::Blops) && s.tags.contains(&Tag::Logi) && s.tags.contains(&Tag::Cyno));
        assert!(!s.tags.contains(&Tag::Capital));
        assert_eq!((s.large_fleet, s.fc), (0, 0), "no large fleets, so no FC signal");
    }

    #[test]
    fn a_role_outside_the_top_nine_hulls_still_tags() {
        let mut v = sample_stats();
        v["groups"] = serde_json::json!({"30": {"groupID": 30, "shipsLost": 2, "shipsDestroyed": 90}});
        let s = parse_stats(7, "x", &v);
        assert!(s.tags.contains(&Tag::Capital), "two titans lost count as flying capitals");
        v["groups"] = serde_json::json!({"30": {"groupID": 30, "shipsDestroyed": 90}});
        assert!(!parse_stats(7, "x", &v).tags.contains(&Tag::Capital), "killing titans is not flying them");
    }

    #[test]
    fn fc_hulls_count_only_for_someone_in_large_fleets() {
        let mut v = sample_stats();
        v["topShips"].as_array_mut().unwrap().extend([
            serde_json::json!({"shipTypeID": 45534, "groupID": 1972, "kills": 6, "losses": 0}),
            serde_json::json!({"shipTypeID": 12015, "groupID": 358, "kills": 4, "losses": 1}),
            serde_json::json!({"shipTypeID": 12023, "groupID": 358, "kills": 2, "losses": 0}),
            serde_json::json!({"shipTypeID": 12011, "groupID": 358, "kills": 50, "losses": 0}),
        ]);
        assert_eq!(parse_stats(7, "x", &v).fc, 0, "small-gang pilot");
        v["labels"]["#:50+"] = serde_json::json!({"shipsDestroyed": 30, "shipsLost": 2});
        let s = parse_stats(7, "x", &v);
        assert_eq!(s.large_fleet, 32);
        assert_eq!(s.fc, 13, "Monitor 6 + Muninn 5 + Deimos 2; other HACs do not count");
        assert!(s.tags.contains(&Tag::Fc));
    }

    #[test]
    fn an_fc_hull_only_in_the_all_time_list_still_counts() {
        let mut v = sample_stats();
        v["labels"]["#:100+"] = serde_json::json!({"shipsDestroyed": 300});
        v["topAllTime"] = serde_json::json!([
            {"type": "character", "data": [{"kills": 400, "characterID": 7}]},
            {"type": "ship", "data": [{"shipTypeID": 45534, "kills": 16}, {"shipTypeID": 11999, "kills": 15}, {"shipTypeID": 638, "kills": 300}]}
        ]);
        let s = parse_stats(7, "x", &v);
        assert_eq!(s.fc, 31, "Monitor 16 + Vagabond 15 from the all-time list");
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
