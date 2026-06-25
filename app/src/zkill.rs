//! Live killmail feed (zKillboard RedisQ) → battle reports (docs/DESIGN.md §7.2).
//!
//! Long-polls zKillboard's RedisQ stream for killmails, keeps only those near the
//! systems currently in the intel feed ("an area"), resolves party names via ESI's
//! public `/universe/names` endpoint, and clusters them into battles. The clustered
//! result is shared with the UI.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use crate::battle::{self, Battle, Engagement, Party, PartyKind};
use crate::geo::Systems;
use crate::intel::IntelState;

const R2Z2: &str = "https://r2z2.zkillboard.com/ephemeral";
const NAMES_URL: &str = "https://esi.evetech.net/latest/universe/names/";
/// Keep kills within this many jumps of a tracked intel system.
const ANCHOR_JUMPS: u32 = 6;
/// Retain engagements for a day — zKillboard can deliver kills hours late, so a
/// battle report keeps getting updated as stragglers arrive.
const ENGAGEMENT_TTL: i64 = 86_400;

pub type SharedBattles = Arc<Mutex<Vec<Battle>>>;

pub fn spawn(
    systems: Arc<Systems>,
    intel: Arc<Mutex<IntelState>>,
    battles: SharedBattles,
    camps: crate::camp::SharedCamps,
    killfeed: SharedKillFeed,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(30))
            .build()
        else {
            return;
        };
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut buffer: Vec<Engagement> = Vec::new();
        let mut seen_links: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut last_scan = std::time::Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(std::time::Instant::now);

        // R2Z2 (RedisQ's replacement; RedisQ was sunset 2026-05-31): killmails are numbered
        // sequentially. Start at the current sequence and iterate forward, one file each.
        let mut seq = fetch_sequence(&client);
        let mut stuck = 0u32;
        loop {
            let mut changed = false;
            match seq {
                None => {
                    std::thread::sleep(Duration::from_secs(5));
                    seq = fetch_sequence(&client);
                }
                Some(s) => match poll(&client, s, &systems, &intel, &camps, &killfeed, &mut names) {
                    Poll::Got(eng) => {
                        stuck = 0;
                        seq = Some(s + 1);
                        if let Some(engagement) = eng {
                            if !buffer.iter().any(|e| e.kill_id == engagement.kill_id) {
                                buffer.push(engagement);
                                changed = true;
                            }
                        }
                    }
                    Poll::NotReady => {
                        // Caught up (this sequence isn't uploaded yet). Wait; if stuck a while
                        // there may be a gap, so re-sync to the current sequence.
                        stuck += 1;
                        std::thread::sleep(Duration::from_secs(2));
                        if stuck >= 15 {
                            stuck = 0;
                            if let Some(cur) = fetch_sequence(&client) {
                                if cur > s + 5 {
                                    seq = Some(cur);
                                } else if cur > s {
                                    seq = Some(s + 1);
                                }
                            }
                        }
                    }
                    Poll::Retry => std::thread::sleep(Duration::from_secs(5)),
                },
            }

            // Pull in killmails posted as links in intel (throttled — it locks the intel
            // feed and scans every report).
            if last_scan.elapsed() >= Duration::from_secs(8) {
                last_scan = std::time::Instant::now();
                let posted: Vec<i64> = {
                    let st = intel.lock().unwrap();
                    st.reports
                        .iter()
                        .flat_map(|r| r.links.iter())
                        .filter_map(|l| l.kill_id)
                        .collect()
                };
                for id in posted {
                    if seen_links.contains(&id) || buffer.iter().any(|e| e.kill_id == id) {
                        continue;
                    }
                    seen_links.insert(id);
                    if let Some(eng) = fetch_posted_kill(&client, id, &systems, &mut names) {
                        buffer.push(eng);
                        changed = true;
                    }
                }
            }

            if changed {
                let now = chrono::Utc::now().timestamp();
                buffer.retain(|e| now - e.time <= ENGAGEMENT_TTL);
                let clustered = battle::cluster(
                    &buffer,
                    battle::BATTLE_WINDOW_SECS,
                    battle::BATTLE_MAX_JUMPS,
                    |a, b| systems.jumps(a, b, battle::BATTLE_MAX_JUMPS),
                );
                *battles.lock().unwrap() = clustered;
                ctx.request_repaint();
            }
        }
    });
}

/// One R2Z2 ephemeral killmail (`/ephemeral/<sequence>.json`).
#[derive(Deserialize)]
struct R2Z2Kill {
    killmail_id: i64,
    esi: Killmail,
    zkb: Zkb,
}

#[derive(Deserialize)]
struct Sequence {
    sequence: u64,
}

/// Outcome of polling one sequence file.
enum Poll {
    Got(Option<Engagement>),
    NotReady,
    Retry,
}

/// The current (latest) killmail sequence number.
fn fetch_sequence(client: &reqwest::blocking::Client) -> Option<u64> {
    client.get(format!("{R2Z2}/sequence.json")).send().ok()?.json::<Sequence>().ok().map(|s| s.sequence)
}

#[derive(Deserialize)]
struct Package {
    #[serde(rename = "killID")]
    kill_id: i64,
    killmail: Killmail,
    zkb: Zkb,
}

#[derive(Deserialize)]
struct Killmail {
    killmail_time: String,
    solar_system_id: i64,
    victim: Combatant,
    #[serde(default)]
    attackers: Vec<Combatant>,
}

#[derive(Deserialize)]
struct Combatant {
    alliance_id: Option<i64>,
    corporation_id: Option<i64>,
    character_id: Option<i64>,
    #[serde(default)]
    ship_type_id: Option<i64>,
}

/// A killmail surfaced for the optional kill-intel feed: the app turns these into intel
/// cards when within jump range and not a skipped ship type.
#[derive(Clone)]
pub struct KillEvent {
    pub system_id: i64,
    pub ship_type_id: i64,
    pub time: i64,
    pub value: f64,
}

pub type SharedKillFeed = std::sync::Arc<Mutex<Vec<KillEvent>>>;

#[derive(Deserialize)]
struct Zkb {
    #[serde(rename = "totalValue", default)]
    total_value: f64,
}

fn poll(
    client: &reqwest::blocking::Client,
    seq: u64,
    systems: &Systems,
    intel: &Mutex<IntelState>,
    camps: &crate::camp::SharedCamps,
    killfeed: &SharedKillFeed,
    names: &mut HashMap<i64, String>,
) -> Poll {
    let resp = match client.get(format!("{R2Z2}/{seq}.json")).send() {
        Ok(r) => r,
        Err(_) => return Poll::Retry,
    };
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Poll::NotReady; // this sequence hasn't been uploaded yet
    }
    let r2: R2Z2Kill = match resp.error_for_status().and_then(|r| r.json()) {
        Ok(v) => v,
        Err(_) => return Poll::Retry,
    };
    let pkg = Package { kill_id: r2.killmail_id, killmail: r2.esi, zkb: r2.zkb };

    // Record every kill for gate-camp detection, regardless of the tracked-area filter below.
    {
        let t = chrono::DateTime::parse_from_rfc3339(&pkg.killmail.killmail_time)
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|_| chrono::Utc::now().timestamp());
        camps.lock().unwrap().record(pkg.killmail.solar_system_id, t);
        if let Some(ship) = pkg.killmail.victim.ship_type_id {
            let mut kf = killfeed.lock().unwrap();
            kf.push(KillEvent {
                system_id: pkg.killmail.solar_system_id,
                ship_type_id: ship,
                time: t,
                value: pkg.zkb.total_value,
            });
            let n = kf.len();
            if n > 256 {
                kf.drain(0..n - 256);
            }
        }
    }

    // Only keep kills near a system currently in the intel feed.
    if !in_tracked_area(systems, intel, pkg.killmail.solar_system_id) {
        return Poll::Got(None);
    }
    let Some(sys) = systems.info_of(pkg.killmail.solar_system_id) else {
        return Poll::Got(None);
    };
    let time = chrono::DateTime::parse_from_rfc3339(&pkg.killmail.killmail_time)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());

    resolve_names(client, &pkg.killmail, names);

    let victim = party_of(&pkg.killmail.victim, names);
    let attackers: Vec<Party> = pkg
        .killmail
        .attackers
        .iter()
        .map(|a| party_of(a, names))
        .collect();

    Poll::Got(Some(Engagement {
        kill_id: pkg.kill_id,
        time,
        system_id: sys.id,
        system_name: sys.name.clone(),
        security: sys.security,
        victim,
        attackers,
        isk: pkg.zkb.total_value,
    }))
}

fn in_tracked_area(systems: &Systems, intel: &Mutex<IntelState>, kill_system: i64) -> bool {
    let intel_systems: Vec<i64> = {
        let state = intel.lock().unwrap();
        let mut ids: Vec<i64> = state
            .reports
            .iter()
            .flat_map(|r| r.systems.iter().map(|s| s.id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    intel_systems
        .iter()
        .any(|&s| systems.jumps(kill_system, s, ANCHOR_JUMPS).is_some())
}

/// The party for a combatant: prefer alliance, then corporation, then character.
fn party_of(c: &Combatant, names: &HashMap<i64, String>) -> Party {
    let (id, kind) = if let Some(id) = c.alliance_id {
        (id, PartyKind::Alliance)
    } else if let Some(id) = c.corporation_id {
        (id, PartyKind::Corporation)
    } else if let Some(id) = c.character_id {
        (id, PartyKind::Character)
    } else {
        (0, PartyKind::Unknown)
    };
    Party {
        id,
        name: names.get(&id).cloned().unwrap_or_else(|| "Unknown".to_owned()),
        kind,
    }
}

#[derive(Deserialize)]
struct NameEntry {
    id: i64,
    name: String,
}

/// zKillboard API entry (`/api/killID/<id>/`) — gives the hash + value for a kill
/// we only know by id (a pasted link).
#[derive(Deserialize)]
struct ZkApiEntry {
    zkb: ZkApiZkb,
}
#[derive(Deserialize)]
struct ZkApiZkb {
    hash: String,
    #[serde(rename = "totalValue", default)]
    total_value: f64,
}

/// Fetch a kill we only know by id (from a pasted intel link) and turn it into an
/// engagement so it joins the battle clustering. Posted kills are always included
/// (no tracked-area filter).
fn fetch_posted_kill(
    client: &reqwest::blocking::Client,
    id: i64,
    systems: &Systems,
    names: &mut HashMap<i64, String>,
) -> Option<Engagement> {
    let zk: Vec<ZkApiEntry> = client
        .get(format!("https://zkillboard.com/api/killID/{id}/"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    let entry = zk.into_iter().next()?;
    let km: Killmail = client
        .get(format!("https://esi.evetech.net/latest/killmails/{id}/{}/", entry.zkb.hash))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    let sys = systems.info_of(km.solar_system_id)?;
    let time = chrono::DateTime::parse_from_rfc3339(&km.killmail_time)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());
    resolve_names(client, &km, names);
    Some(Engagement {
        kill_id: id,
        time,
        system_id: sys.id,
        system_name: sys.name.clone(),
        security: sys.security,
        victim: party_of(&km.victim, names),
        attackers: km.attackers.iter().map(|a| party_of(a, names)).collect(),
        isk: entry.zkb.total_value,
    })
}

/// Resolve any not-yet-cached ids referenced by this kill via ESI /universe/names.
fn resolve_names(
    client: &reqwest::blocking::Client,
    km: &Killmail,
    names: &mut HashMap<i64, String>,
) {
    let mut wanted: Vec<i64> = Vec::new();
    let mut add = |c: &Combatant| {
        for id in [c.alliance_id, c.corporation_id, c.character_id].into_iter().flatten() {
            if id != 0 && !names.contains_key(&id) {
                wanted.push(id);
            }
        }
    };
    add(&km.victim);
    km.attackers.iter().for_each(&mut add);
    wanted.sort_unstable();
    wanted.dedup();
    if wanted.is_empty() {
        return;
    }

    if let Ok(resp) = client.post(NAMES_URL).json(&wanted).send() {
        if let Ok(entries) = resp.json::<Vec<NameEntry>>() {
            for e in entries {
                names.insert(e.id, e.name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_redisq_package() {
        // A minimal, real-shaped RedisQ payload.
        let json = r#"{"package":{"killID":12345,"killmail":{
            "killmail_id":12345,"killmail_time":"2026-06-22T18:30:45Z",
            "solar_system_id":30000142,
            "victim":{"alliance_id":99,"corporation_id":98,"character_id":97,"ship_type_id":670},
            "attackers":[{"alliance_id":1,"corporation_id":2,"character_id":3}]},
            "zkb":{"totalValue":12345.6}}}"#;
        let parsed: RedisQ = serde_json::from_str(json).unwrap();
        let pkg = parsed.package.expect("package present");
        assert_eq!(pkg.kill_id, 12345);
        assert_eq!(pkg.killmail.solar_system_id, 30000142);
        assert_eq!(pkg.killmail.attackers.len(), 1);
        assert_eq!(pkg.zkb.total_value, 12345.6);
        assert_eq!(pkg.killmail.victim.alliance_id, Some(99));
        // An empty poll ("no kill") parses to None.
        assert!(serde_json::from_str::<RedisQ>(r#"{"package":null}"#)
            .unwrap()
            .package
            .is_none());
    }
}
