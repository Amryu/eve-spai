//! Live killmail feed (zKillboard RedisQ) → battle reports (docs/DESIGN.md §7.2).
//!
//! Long-polls zKillboard's RedisQ stream for killmails, keeps only those near the
//! systems currently in the intel feed ("an area"), resolves party names via ESI's
//! public `/universe/names` endpoint, and clusters them into battles. The clustered
//! result is shared with the UI.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use crate::battle::{self, Attacker, Battle, Engagement, Party, PartyKind};
use crate::geo::Systems;
use crate::intel::IntelState;
use crate::settings::{BattleFilter, MatchData, ShipSize};

/// Live, app-owned battle-filter config the worker reads each kill.
pub type SharedBattleFilter = Arc<Mutex<BattleFilter>>;
/// Ship type id → hull size tier (for filter hull conditions).
pub type ShipSizes = Arc<HashMap<i64, ShipSize>>;

const R2Z2: &str = "https://r2z2.zkillboard.com/ephemeral";
const NAMES_URL: &str = "https://esi.evetech.net/latest/universe/names/";
/// Keep kills within this many jumps of a tracked intel system.
pub const ANCHOR_JUMPS: u32 = 6;
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
    camp_types: crate::camp::CampTypes,
    ship_ids: Arc<std::collections::HashSet<i64>>,
    filter: SharedBattleFilter,
    ship_sizes: ShipSizes,
    player_sys: Arc<AtomicI64>,
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
        // Reload persisted engagements so clustered battles survive a restart.
        let store = crate::store::Store::open().ok();
        let mut buffer: Vec<Engagement> = match &store {
            Some(s) => s.load_engagements(chrono::Utc::now().timestamp() - ENGAGEMENT_TTL),
            None => Vec::new(),
        };
        if !buffer.is_empty() {
            let clustered = battle::cluster(
                &buffer,
                battle::BATTLE_WINDOW_SECS,
                battle::BATTLE_MAX_JUMPS,
                |a, b| systems.jumps(a, b, battle::BATTLE_MAX_JUMPS),
            );
            *battles.lock().unwrap() = clustered;
            ctx.request_repaint();
        }
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
                Some(s) => match poll(&client, s, &systems, &intel, &camps, &killfeed, &camp_types, &ship_ids, &filter, &ship_sizes, &player_sys, &mut names) {
                    Poll::Got(eng) => {
                        stuck = 0;
                        seq = Some(s + 1);
                        if let Some(engagement) = eng {
                            if !buffer.iter().any(|e| e.kill_id == engagement.kill_id) {
                                if let Some(s) = &store {
                                    s.save_engagement(&engagement);
                                }
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
                    if let Some(eng) = fetch_posted_kill(&client, id, &systems, &ship_ids, &mut names) {
                        if let Some(s) = &store {
                            s.save_engagement(&eng);
                        }
                        buffer.push(eng);
                        changed = true;
                    }
                }
            }

            if changed {
                let now = chrono::Utc::now().timestamp();
                // The live view clusters only the last day; persisted engagements are kept for
                // the full searchable history (see the battles "Full history" view).
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
    /// Weapon used (attackers only) — for smartbomb detection.
    #[serde(default)]
    weapon_type_id: Option<i64>,
    /// Landed the killing blow (attackers only).
    #[serde(default)]
    final_blow: bool,
    /// In-space position (victim only) — for on-gate detection.
    #[serde(default)]
    position: Option<Position>,
}

#[derive(Deserialize)]
struct Position {
    x: f64,
    y: f64,
    z: f64,
}

/// A killmail surfaced for the optional kill-intel feed: the app turns these into intel
/// cards when within jump range and not a skipped ship type.
#[derive(Clone)]
pub struct KillEvent {
    pub system_id: i64,
    pub ship_type_id: i64,
    pub time: i64,
    pub value: f64,
    pub killmail_id: i64,
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
    camp_types: &crate::camp::CampTypes,
    ship_ids: &std::collections::HashSet<i64>,
    filter: &Mutex<BattleFilter>,
    ship_sizes: &HashMap<i64, ShipSize>,
    player_sys: &AtomicI64,
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
        // On-gate: the victim died within ~grid of a stargate.
        let on_gate = pkg.killmail.victim.position.as_ref().is_some_and(|p| {
            systems.on_gate(pkg.killmail.solar_system_id, [p.x, p.y, p.z])
        });
        // Camp equipment: an interdictor/HIC among the attackers, a smartbomb weapon, or the
        // victim itself was an anchorable warp-disruption bubble.
        let equip = pkg.killmail.attackers.iter().any(|a| {
            a.ship_type_id.is_some_and(|s| camp_types.dic_hic.contains(&s))
                || a.weapon_type_id.is_some_and(|w| camp_types.smartbomb.contains(&w))
        }) || pkg
            .killmail
            .victim
            .ship_type_id
            .is_some_and(|s| camp_types.bubble.contains(&s));
        camps.lock().unwrap().record(pkg.killmail.solar_system_id, t, on_gate, equip);
        if let Some(ship) = pkg.killmail.victim.ship_type_id {
            let mut kf = killfeed.lock().unwrap();
            kf.push(KillEvent {
                system_id: pkg.killmail.solar_system_id,
                ship_type_id: ship,
                time: t,
                value: pkg.zkb.total_value,
                killmail_id: pkg.kill_id,
            });
            let n = kf.len();
            if n > 256 {
                kf.drain(0..n - 256);
            }
        }
    }

    // Battles are about ships and structures — drop deployable kills (mobile depots,
    // tractor units, anchored bubbles, …).
    if !is_listed_hull(pkg.killmail.victim.ship_type_id.unwrap_or(0), ship_ids) {
        return Poll::Got(None);
    }
    let Some(sys) = systems.info_of(pkg.killmail.solar_system_id) else {
        return Poll::Got(None);
    };
    // Keep kills near a tracked intel system OR matching a custom Include rule.
    let tracked = in_tracked_area(systems, intel, pkg.killmail.solar_system_id);
    if !tracked {
        let data = ingest_match_data(&pkg.killmail, sys, ship_sizes, systems, player_sys);
        let admitted = {
            let f = filter.lock().unwrap();
            f.rules.iter().any(|r| r.admits_ingest(&data))
        };
        if !admitted {
            return Poll::Got(None);
        }
    }
    let time = chrono::DateTime::parse_from_rfc3339(&pkg.killmail.killmail_time)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());

    resolve_names(client, &pkg.killmail, names);

    // A kill scored 100% by NPCs (no capsuleer attacker) isn't part of a player battle.
    let attackers = attackers_of(&pkg.killmail, names);
    if attackers.is_empty() {
        return Poll::Got(None);
    }
    Poll::Got(Some(Engagement {
        kill_id: pkg.kill_id,
        time,
        system_id: sys.id,
        system_name: sys.name.clone(),
        security: sys.security,
        victim: party_of(&pkg.killmail.victim, names),
        victim_char: pkg.killmail.victim.character_id.unwrap_or(0),
        victim_pilot: pilot_of(&pkg.killmail.victim, names),
        victim_ship: pkg.killmail.victim.ship_type_id.unwrap_or(0),
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

/// Per-kill facts a battle-filter Include rule can test at ingest (no ESI): location, coalition
/// (via config packs), hull size, and distance from the active character.
fn ingest_match_data(
    km: &Killmail,
    sys: &crate::geo::SystemInfo,
    ship_sizes: &HashMap<i64, ShipSize>,
    systems: &Systems,
    player_sys: &AtomicI64,
) -> MatchData {
    let mut d = MatchData::default();
    d.systems.insert(sys.name.to_lowercase());
    if !sys.region.is_empty() {
        d.regions.insert(sys.region.to_lowercase());
    }
    if !sys.constellation.is_empty() {
        d.constellations.insert(sys.constellation.to_lowercase());
    }
    let mut max = ShipSize::Other;
    for c in std::iter::once(&km.victim).chain(km.attackers.iter()) {
        if let Some(al) = c.alliance_id {
            if let Some(coal) = crate::packs::coalition_of(al) {
                d.coalitions.insert(coal.to_lowercase());
            }
        }
        if let Some(sz) = c.ship_type_id.and_then(|s| ship_sizes.get(&s)) {
            if *sz > max {
                max = *sz;
            }
        }
    }
    d.max_size = max;
    let me = player_sys.load(Ordering::Relaxed);
    if me != 0 {
        d.min_jumps_from_me = systems.jumps(sys.id, me, 50);
    }
    d
}

/// The party for a combatant: prefer alliance, then corporation, then character.
/// Whether a destroyed type belongs in a battle report: a real ship (SDE ship list), a
/// capsule, or a structure. Everything else (deployables, drones, NPC junk) is dropped.
fn is_listed_hull(ship: i64, ship_ids: &std::collections::HashSet<i64>) -> bool {
    ship_ids.contains(&ship)
        || battle::POD_TYPES.contains(&ship)
        || crate::intel::structure_name_by_type(ship).is_some()
}

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

/// Pilot display name for a combatant: character if present, else corp/alliance.
fn pilot_of(c: &Combatant, names: &HashMap<i64, String>) -> String {
    let id = c.character_id.or(c.corporation_id).or(c.alliance_id).unwrap_or(0);
    names.get(&id).cloned().unwrap_or_else(|| "Unknown".to_owned())
}

/// A killmail attacker with no capsuleer behind it — belt/incursion/mission rats and faction
/// NPCs. Player corporations are >= 98,000,000; a player-owned structure keeps that corp id, so
/// it is not treated as an NPC. NPCs are never credited a kill, so a side is never an NPC corp.
fn is_npc_attacker(c: &Combatant) -> bool {
    c.character_id.is_none() && c.corporation_id.map_or(true, |id| id < 98_000_000)
}

/// Build the attacker list (capsuleers and player structures only), carrying ship, pilot and
/// final-blow for the battle roster. NPC attackers are dropped, so the kill is attributed to
/// the remaining player side(s), not to whatever rat happened to land the final blow.
fn attackers_of(km: &Killmail, names: &HashMap<i64, String>) -> Vec<Attacker> {
    km.attackers
        .iter()
        .filter(|a| !is_npc_attacker(a))
        .map(|a| Attacker {
            party: party_of(a, names),
            char_id: a.character_id.unwrap_or(0),
            ship: a.ship_type_id.unwrap_or(0),
            pilot: pilot_of(a, names),
            final_blow: a.final_blow,
        })
        .collect()
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
    ship_ids: &std::collections::HashSet<i64>,
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
    if !is_listed_hull(km.victim.ship_type_id.unwrap_or(0), ship_ids) {
        return None;
    }
    let sys = systems.info_of(km.solar_system_id)?;
    let time = chrono::DateTime::parse_from_rfc3339(&km.killmail_time)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());
    resolve_names(client, &km, names);
    let attackers = attackers_of(&km, names);
    if attackers.is_empty() {
        return None; // scored 100% by NPCs
    }
    Some(Engagement {
        kill_id: id,
        time,
        system_id: sys.id,
        system_name: sys.name.clone(),
        security: sys.security,
        victim: party_of(&km.victim, names),
        victim_char: km.victim.character_id.unwrap_or(0),
        victim_pilot: pilot_of(&km.victim, names),
        victim_ship: km.victim.ship_type_id.unwrap_or(0),
        attackers,
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
    fn parses_r2z2_killmail() {
        // A minimal, real-shaped R2Z2 ephemeral killmail ("/ephemeral/<seq>.json").
        let json = r#"{"killmail_id":12345,"hash":"abc","esi":{
            "killmail_id":12345,"killmail_time":"2026-06-22T18:30:45Z",
            "solar_system_id":30000142,
            "victim":{"alliance_id":99,"corporation_id":98,"character_id":97,"ship_type_id":670},
            "attackers":[{"alliance_id":1,"corporation_id":2,"character_id":3}]},
            "zkb":{"totalValue":12345.6},"sequence_id":98212640}"#;
        let r2: R2Z2Kill = serde_json::from_str(json).unwrap();
        assert_eq!(r2.killmail_id, 12345);
        assert_eq!(r2.esi.solar_system_id, 30000142);
        assert_eq!(r2.esi.attackers.len(), 1);
        assert_eq!(r2.zkb.total_value, 12345.6);
        assert_eq!(r2.esi.victim.alliance_id, Some(99));
        // The sequence pointer file parses too.
        let seq: Sequence = serde_json::from_str(r#"{"sequence":98212646}"#).unwrap();
        assert_eq!(seq.sequence, 98212646);
    }

    #[test]
    fn npc_attacker_detection() {
        let parse = |j: &str| -> Combatant { serde_json::from_str(j).unwrap() };
        // Belt/mission rat: no character, NPC corp (< 98M).
        assert!(is_npc_attacker(&parse(r#"{"corporation_id":1000127}"#)));
        // Faction NPC: no corp, no character.
        assert!(is_npc_attacker(&parse(r#"{"ship_type_id":1234}"#)));
        // Capsuleer: has a character id.
        assert!(!is_npc_attacker(&parse(r#"{"character_id":95538921,"corporation_id":1000127}"#)));
        // Player-owned structure: no character, but a player corp (>= 98M).
        assert!(!is_npc_attacker(&parse(r#"{"corporation_id":98000001}"#)));
    }
}
