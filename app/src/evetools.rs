//! Battle reports on br.evetools.org, made from a battle's systems and times: the site gathers the
//! kills itself. What it hands back (the report's id and the key that allows updating it) is kept
//! so the report can be opened again and brought up to date when the battle changes.

use br_core::battle::{Battle, PartyKind};
use serde::{Deserialize, Serialize};

const API: &str = "https://br.evetools.org/newapi";
/// Room either side of the first and last kill, as a pilot entering the times by hand would give.
const PAD_SECS: i64 = 300;
/// The site refuses a system's time span longer than a day.
const MAX_SPAN_SECS: i64 = 86_400;

/// One system's time span, the way the site takes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timing {
    #[serde(rename = "systemID")]
    pub system_id: i64,
    pub start: i64,
    pub end: i64,
}

/// A report made on the site, and what it was made from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub br_id: String,
    pub ukey: String,
    pub timings: Vec<Timing>,
}

impl Saved {
    pub fn url(&self) -> String {
        format!("https://br.evetools.org/br/{}", self.br_id)
    }
}

/// Each system from its first kill to its last, padded, whole minutes.
pub fn timings(b: &Battle) -> Vec<Timing> {
    let mut by_system: std::collections::BTreeMap<i64, (i64, i64)> = std::collections::BTreeMap::new();
    for e in &b.engagements {
        let span = by_system.entry(e.system_id).or_insert((e.time, e.time));
        span.0 = span.0.min(e.time);
        span.1 = span.1.max(e.time);
    }
    by_system
        .into_iter()
        .map(|(system_id, (first, last))| {
            let start = (first - PAD_SECS).div_euclid(60) * 60;
            let end = ((last + PAD_SECS + 59).div_euclid(60) * 60).min(start + MAX_SPAN_SECS);
            Timing { system_id, start, end }
        })
        .collect()
}

/// The sides as the site's teams: alliances by id, corporations as `corp:<id>`.
pub fn teams(b: &Battle) -> Vec<Vec<String>> {
    b.sides
        .iter()
        .map(|s| {
            s.parties
                .iter()
                .filter(|p| p.id > 0)
                .filter_map(|p| match p.kind {
                    PartyKind::Alliance => Some(p.id.to_string()),
                    PartyKind::Corporation => Some(format!("corp:{}", p.id)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "brID")]
    br_id: Option<String>,
    #[serde(default, rename = "_id")]
    id: Option<String>,
    #[serde(default)]
    ukey: Option<String>,
}

fn post(path: &str, body: serde_json::Value) -> Result<Answer, String> {
    let client = crate::http::client(30).map_err(|e| e.to_string())?;
    let r = client.post(format!("{API}{path}")).json(&body).send().map_err(|e| format!("br.evetools: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("br.evetools answered {}", r.status()));
    }
    let a: Answer = r.json().map_err(|e| format!("br.evetools: {e}"))?;
    if a.status.as_deref().is_some_and(|s| s != "success") {
        return Err(format!("br.evetools: {}", a.status.unwrap_or_default()));
    }
    Ok(a)
}

/// Makes a new report. Blocking: run it off the UI thread.
pub fn create(b: &Battle) -> Result<Saved, String> {
    let timings = timings(b);
    let a = post("/old/br/create-new", serde_json::json!({ "timings": timings, "teams": teams(b) }))?;
    let br_id = a.br_id.or(a.id).ok_or("br.evetools made no report")?;
    Ok(Saved { br_id, ukey: a.ukey.unwrap_or_default(), timings })
}

/// Brings a report made here up to date with the battle as it is now.
pub fn update(saved: &Saved, b: &Battle) -> Result<Saved, String> {
    if saved.ukey.is_empty() {
        return Err("br.evetools gave no key to update this report with; make a new one".into());
    }
    let timings = timings(b);
    post(
        "/old/br/update-new",
        serde_json::json!({ "brID": saved.br_id, "ukey": saved.ukey, "timings": timings, "teams": teams(b) }),
    )?;
    Ok(Saved { timings, ..saved.clone() })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    Working,
    /// Made or updated: its link.
    Done(String),
    Failed(String),
}

pub type Shared = std::sync::Arc<std::sync::Mutex<Status>>;

/// Creates, or updates `saved`, on a thread; the result is stored against `anchor`, the battle's
/// earliest kill, so the battle finds it again however it grows.
pub fn spawn(b: Battle, saved: Option<Saved>, anchor: i64, status: Shared, ctx: egui::Context) {
    *status.lock().unwrap() = Status::Working;
    std::thread::spawn(move || {
        let result = match &saved {
            Some(s) => update(s, &b),
            None => create(&b),
        };
        *status.lock().unwrap() = match result {
            Ok(s) => {
                if let Ok(store) = crate::store::Store::open() {
                    store.evetools_save(anchor, &s);
                }
                Status::Done(s.url())
            }
            Err(e) => Status::Failed(e),
        };
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    /// Checks the timings format against the live site's read-only analysis: nothing is created.
    #[test]
    #[ignore = "needs the network"]
    fn the_site_reads_our_timings() {
        let (b, _) = crate::uitest::fixtures::real_battle();
        let timings = timings(&b);
        let client = crate::http::client(30).unwrap();
        let r = client.post(format!("{API}/br/analyze")).json(&serde_json::json!({ "timings": timings })).send().unwrap();
        let status = r.status();
        let body: serde_json::Value = r.json().unwrap();
        let kms: usize = body["relateds"].as_array().map_or(0, |a| a.iter().map(|x| x["kms"].as_array().map_or(0, |k| k.len())).sum());
        println!("{status} {} relateds, {kms} kills for {timings:?}; ours {}", body["relateds"].as_array().map_or(0, |a| a.len()), b.engagements.len());
        assert!(status.is_success() && kms > 0);
    }

    use super::*;
    use br_core::battle::{Engagement, Party, Side};

    fn kill(id: i64, time: i64, system: i64) -> Engagement {
        Engagement {
            kill_id: id,
            time,
            system_id: system,
            system_name: String::new(),
            security: -0.5,
            victim: Party { id: 1, name: String::new(), kind: PartyKind::Alliance },
            victim_char: 0,
            victim_pilot: String::new(),
            victim_ship: 0,
            attackers: vec![],
            isk: 0.0,
            anchored: true,
        }
    }

    #[test]
    fn each_system_spans_its_kills_padded_to_whole_minutes() {
        let b = br_core::battle::preview_battle(vec![kill(1, 1_000_030, 30_000_142), kill(2, 1_003_000, 30_000_142), kill(3, 1_001_000, 30_002_187)], 3600);
        let t = timings(&b);
        assert_eq!(t[0], Timing { system_id: 30_000_142, start: 999_720, end: 1_003_320 });
        assert_eq!(t[1].system_id, 30_002_187);
        assert!(t.iter().all(|t| t.start % 60 == 0 && t.end % 60 == 0 && t.start < t.end));
        let json = serde_json::to_value(&t[0]).unwrap();
        assert_eq!(json["systemID"], 30_000_142, "the site's field name");
    }

    #[test]
    fn sides_become_teams_of_alliances_and_corporations() {
        let p = |id, kind| Party { id, name: String::new(), kind };
        let side = |parties| Side { parties, coalition: None, kills: 0, losses: 0, isk_lost: 0.0, isk_destroyed: 0.0 };
        let mut b = br_core::battle::preview_battle(vec![kill(1, 1_000_000, 30_000_142)], 3600);
        b.sides = vec![
            side(vec![p(99_000_001, PartyKind::Alliance), p(98_000_002, PartyKind::Corporation)]),
            side(vec![p(500_001, PartyKind::Faction)]),
            side(vec![p(99_000_003, PartyKind::Alliance)]),
        ];
        assert_eq!(teams(&b), vec![vec!["99000001".to_owned(), "corp:98000002".to_owned()], vec!["99000003".to_owned()]]);
    }
}
