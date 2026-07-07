use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use crate::battle::{self, Attacker, Battle, Engagement, Party, PartyKind};
use crate::geo::Systems;
use crate::intel::IntelState;
use crate::settings::{BattleFilter, MatchData, ShipSize};

pub type SharedBattleFilter = Arc<Mutex<BattleFilter>>;
pub type SharedOverrides = Arc<Mutex<crate::battle::Overrides>>;
pub type ShipSizes = Arc<HashMap<i64, ShipSize>>;

const R2Z2: &str = "https://r2z2.zkillboard.com/ephemeral";
const NAMES_URL: &str = "https://esi.evetech.net/latest/universe/names/";
pub const ANCHOR_JUMPS: u32 = 6;
const RECENT_WH_SECS: i64 = 600;
pub type RecentWh = Arc<Mutex<std::collections::HashMap<i64, i64>>>;
const CANDIDATE_JUMPS: u32 = ANCHOR_JUMPS + crate::battle::BATTLE_MAX_JUMPS;
const ENGAGEMENT_TTL: i64 = 86_400;

const ZKILL_API: &str = "https://zkillboard.com/api";
const ESI: &str = "https://esi.evetech.net/latest";
const BACKFILL_WINDOW_SECS: i64 = battle::BATTLE_WINDOW_SECS * 12;
const BACKFILL_MAX_KILLS: usize = 200;
const BACKFILL_DEBOUNCE_SECS: i64 = 1800;

pub type SharedBattles = Arc<Mutex<Vec<Battle>>>;
type SharedBackfill = Arc<Mutex<Vec<Engagement>>>;

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
    recent_wh: RecentWh,
    throttle: Arc<std::sync::atomic::AtomicU8>,
    break_gap: Arc<AtomicI64>,
    overrides: SharedOverrides,
    overrides_gen: Arc<std::sync::atomic::AtomicU64>,
    add_queue: Arc<Mutex<Vec<i64>>>,
    battles_enabled: Arc<std::sync::atomic::AtomicBool>,
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
        let store = crate::store::Store::open().ok();
        let mut buffer: Vec<Engagement> = match &store {
            Some(s) => s.load_engagements(chrono::Utc::now().timestamp() - ENGAGEMENT_TTL),
            None => Vec::new(),
        };
        let mut buffer_ids: std::collections::HashSet<i64> =
            buffer.iter().map(|e| e.kill_id).collect();
        let mut backfilled: HashMap<i64, i64> = HashMap::new();
        let backfill_out: SharedBackfill = Arc::new(Mutex::new(Vec::new()));
        let mut battle_cache: HashMap<u64, battle::Battle> = HashMap::new();
        let mut last_ov_gen = overrides_gen.load(std::sync::atomic::Ordering::Relaxed);
        if !buffer.is_empty() && battles_enabled.load(std::sync::atomic::Ordering::Relaxed) {
            let bg = break_gap.load(std::sync::atomic::Ordering::Relaxed);
            let ov = overrides.lock().unwrap().clone();
            let clustered = battle::cluster_cached(
                &buffer,
                battle::BATTLE_WINDOW_SECS,
                battle::BATTLE_MAX_JUMPS,
                bg,
                &ov,
                |a, b| systems.jumps(a, b, battle::BATTLE_MAX_JUMPS),
                &mut battle_cache,
            );
            *battles.lock().unwrap() = clustered.into_iter().filter(|b| b.is_anchored() && b.is_two_sided()).collect();
            ctx.request_repaint();
        }
        let mut seen_links: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut last_scan = std::time::Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(std::time::Instant::now);
        let mut dirty = false;
        let mut last_cluster = std::time::Instant::now();
        let mut was_enabled = battles_enabled.load(Ordering::Relaxed);

        let mut seq = fetch_sequence(&client);
        let mut stuck = 0u32;
        let mut retries = 0u32;
        loop {
            let throttle = crate::settings::WorkThrottle::from_u8(throttle.load(Ordering::Relaxed));
            // On the off->on edge, force a re-cluster so re-enabling refreshes immediately
            // instead of waiting for the next incoming kill to mark the buffer dirty.
            let enabled_now = battles_enabled.load(Ordering::Relaxed);
            if enabled_now && !was_enabled {
                dirty = true;
            }
            was_enabled = enabled_now;
            let mut changed = false;
            match seq {
                None => {
                    std::thread::sleep(Duration::from_secs(5));
                    seq = fetch_sequence(&client);
                }
                Some(s) => match poll(&client, s, &systems, &intel, &camps, &killfeed, &camp_types, &ship_ids, &filter, &ship_sizes, &player_sys, &recent_wh, &mut names, store.as_ref()) {
                    Poll::Got(eng) => {
                        stuck = 0;
                        retries = 0;
                        seq = Some(s + 1);
                        if let Some(engagement) = eng {
                            let anchored = engagement.anchored;
                            let sys_id = engagement.system_id;
                            if buffer_ids.insert(engagement.kill_id) {
                                if let Some(s) = &store {
                                    s.save_engagement(&engagement);
                                }
                                buffer.push(engagement);
                                changed = true;
                            }
                            if anchored {
                                let now = chrono::Utc::now().timestamp();
                                if should_backfill(&mut backfilled, sys_id, now) {
                                    spawn_backfill(
                                        client.clone(),
                                        sys_id,
                                        now - BACKFILL_WINDOW_SECS,
                                        i64::MAX,
                                        systems.clone(),
                                        ship_ids.clone(),
                                        buffer_ids.clone(),
                                        backfill_out.clone(),
                                        ctx.clone(),
                                    );
                                }
                            }
                        }
                        let d = throttle.feed_delay_ms();
                        if d > 0 {
                            std::thread::sleep(Duration::from_millis(d));
                        }
                    }
                    Poll::NotReady => {
                        retries = 0;
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
                    Poll::Retry => {
                        retries += 1;
                        // After ~5 failed attempts the payload is almost certainly bad, not
                        // a network blip — skip it so the feed doesn't stall on one sequence.
                        if retries >= 5 {
                            retries = 0;
                            seq = Some(s + 1);
                        }
                        std::thread::sleep(Duration::from_secs(5));
                    }
                },
            }

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
                    if seen_links.contains(&id) || buffer_ids.contains(&id) {
                        continue;
                    }
                    if seen_links.len() > 4000 {
                        seen_links.clear();
                    }
                    seen_links.insert(id);
                    if let Some(eng) = fetch_posted_kill(&client, id, &systems, &ship_ids, &mut names) {
                        if let Some(s) = &store {
                            s.save_engagement(&eng);
                        }
                        buffer_ids.insert(eng.kill_id);
                        buffer.push(eng);
                        changed = true;
                    }
                }
            }

            let requested: Vec<i64> = std::mem::take(&mut *add_queue.lock().unwrap());
            for id in requested {
                if buffer_ids.contains(&id) {
                    continue;
                }
                if let Some(eng) = fetch_posted_kill(&client, id, &systems, &ship_ids, &mut names) {
                    if let Some(s) = &store {
                        s.save_engagement(&eng);
                    }
                    buffer_ids.insert(eng.kill_id);
                    buffer.push(eng);
                    changed = true;
                }
            }

            let incoming: Vec<Engagement> = std::mem::take(&mut *backfill_out.lock().unwrap());
            if !incoming.is_empty() {
                let now = chrono::Utc::now().timestamp();
                for eng in incoming {
                    let fresh = now - eng.time <= ENGAGEMENT_TTL && !buffer_ids.contains(&eng.kill_id);
                    if fresh {
                        if let Some(s) = &store {
                            s.save_engagement(&eng);
                        }
                    }
                    if fold_engagement(eng, now, &mut buffer, &mut buffer_ids) {
                        changed = true;
                    }
                }
            }

            dirty |= changed;
            let g = overrides_gen.load(std::sync::atomic::Ordering::Relaxed);
            if g != last_ov_gen {
                last_ov_gen = g;
                dirty = true;
                battle_cache.clear();
            }
            if dirty
                && battles_enabled.load(Ordering::Relaxed)
                && last_cluster.elapsed() >= Duration::from_millis(throttle.cluster_interval_ms())
            {
                dirty = false;
                last_cluster = std::time::Instant::now();
                let now = chrono::Utc::now().timestamp();
                let before = buffer.len();
                buffer.retain(|e| now - e.time <= ENGAGEMENT_TTL);
                if buffer.len() != before {
                    buffer_ids = buffer.iter().map(|e| e.kill_id).collect();
                }
                let bg = break_gap.load(std::sync::atomic::Ordering::Relaxed);
                let ov = overrides.lock().unwrap().clone();
                let clustered = battle::cluster_cached(
                    &buffer,
                    battle::BATTLE_WINDOW_SECS,
                    battle::BATTLE_MAX_JUMPS,
                    bg,
                    &ov,
                    |a, b| systems.jumps(a, b, battle::BATTLE_MAX_JUMPS),
                    &mut battle_cache,
                );
                *battles.lock().unwrap() =
                    clustered.into_iter().filter(|b| b.is_anchored() && b.is_two_sided()).collect();
                ctx.request_repaint();
            }
        }
    });
}

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

enum Poll {
    Got(Option<Engagement>),
    NotReady,
    Retry,
}

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
    #[serde(default)]
    weapon_type_id: Option<i64>,
    #[serde(default)]
    final_blow: bool,
    #[serde(default)]
    position: Option<Position>,
}

#[derive(Deserialize)]
struct Position {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Clone)]
pub struct KillEvent {
    pub system_id: i64,
    pub ship_type_id: i64,
    pub time: i64,
    pub value: f64,
    pub killmail_id: i64,
    pub info: crate::kills::KillInfo,
}

pub type SharedKillFeed = std::sync::Arc<Mutex<Vec<KillEvent>>>;

#[derive(Deserialize)]
struct Zkb {
    #[serde(default)]
    hash: String,
    #[serde(rename = "totalValue", default)]
    total_value: f64,
}

fn kill_info(pkg: &Package) -> crate::kills::KillInfo {
    let v = &pkg.killmail.victim;
    let fb = pkg.killmail.attackers.iter().find(|a| a.final_blow);
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for a in &pkg.killmail.attackers {
        if let Some(al) = a.alliance_id {
            *counts.entry(al).or_default() += 1;
        }
    }
    let mut alliances: Vec<(i64, usize)> = counts.into_iter().collect();
    alliances.sort_by(|a, b| b.1.cmp(&a.1));
    crate::kills::KillInfo {
        kill_id: pkg.kill_id,
        hash: Some(pkg.zkb.hash.clone()).filter(|h| !h.is_empty()),
        victim_char: v.character_id,
        victim_ship: v.ship_type_id,
        victim_corp: v.corporation_id,
        victim_alliance: v.alliance_id,
        system_id: pkg.killmail.solar_system_id,
        value: pkg.zkb.total_value,
        time: pkg.killmail.killmail_time.clone(),
        final_blow_char: fb.and_then(|a| a.character_id),
        final_blow_corp: fb.and_then(|a| a.corporation_id),
        final_blow_alliance: fb.and_then(|a| a.alliance_id),
        final_blow_ship: fb.and_then(|a| a.ship_type_id),
        attacker_count: pkg.killmail.attackers.len(),
        attacker_alliances: alliances.into_iter().map(|(a, _)| a).collect(),
        near_celestial: None,
    }
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
    recent_wh: &RecentWh,
    names: &mut HashMap<i64, String>,
    store: Option<&crate::store::Store>,
) -> Poll {
    let resp = match client.get(format!("{R2Z2}/{seq}.json")).send() {
        Ok(r) => r,
        Err(_) => return Poll::Retry,
    };
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Poll::NotReady;
    }
    let r2: R2Z2Kill = match resp.error_for_status().and_then(|r| r.json()) {
        Ok(v) => v,
        Err(_) => return Poll::Retry,
    };
    let pkg = Package { kill_id: r2.killmail_id, killmail: r2.esi, zkb: r2.zkb };

    {
        let t = chrono::DateTime::parse_from_rfc3339(&pkg.killmail.killmail_time)
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|_| chrono::Utc::now().timestamp());
        let on_gate = pkg.killmail.victim.position.as_ref().is_some_and(|p| {
            systems.on_gate(pkg.killmail.solar_system_id, [p.x, p.y, p.z])
        });
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
            let mut info = kill_info(&pkg);
            if let (Some(store), Some(p)) = (store, pkg.killmail.victim.position.as_ref()) {
                info.near_celestial =
                    store.nearest_celestial(pkg.killmail.solar_system_id, [p.x, p.y, p.z]);
            }
            let mut kf = killfeed.lock().unwrap();
            kf.push(KillEvent {
                system_id: pkg.killmail.solar_system_id,
                ship_type_id: ship,
                time: t,
                value: pkg.zkb.total_value,
                killmail_id: pkg.kill_id,
                info,
            });
            let n = kf.len();
            if n > 256 {
                kf.drain(0..n - 256);
            }
        }
    }

    if !is_listed_hull(pkg.killmail.victim.ship_type_id.unwrap_or(0), ship_ids) {
        return Poll::Got(None);
    }
    let Some(sys) = systems.info_of(pkg.killmail.solar_system_id) else {
        return Poll::Got(None);
    };
    let kill_sys = pkg.killmail.solar_system_id;
    let me = player_sys.load(Ordering::Relaxed);
    let player_jumps =
        if me != 0 { systems.jumps(me, kill_sys, CANDIDATE_JUMPS) } else { None };
    let custom_match = {
        let f = filter.lock().unwrap();
        if !f.widens_beyond_intel() {
            false
        } else {
            let data = ingest_match_data(&pkg.killmail, sys, ship_sizes, systems, player_sys, f.max_jumps_condition());
            f.rules.iter().any(|r| r.admits_ingest(&data))
        }
    };
    let intel_jumps = nearest_intel_jumps(systems, intel, kill_sys, CANDIDATE_JUMPS);
    let wh_recent = crate::geo::is_wormhole_system(kill_sys) && {
        let now = chrono::Utc::now().timestamp();
        recent_wh.lock().unwrap().get(&kill_sys).is_some_and(|&t| now - t <= RECENT_WH_SECS)
    };
    let anchored = custom_match
        || wh_recent
        || player_jumps.is_some_and(|d| d <= ANCHOR_JUMPS)
        || intel_jumps.is_some_and(|d| d <= ANCHOR_JUMPS);
    let candidate = anchored || wh_recent || player_jumps.is_some() || intel_jumps.is_some();
    if !candidate {
        return Poll::Got(None);
    }
    Poll::Got(build_engagement(
        client,
        &pkg.killmail,
        pkg.kill_id,
        pkg.zkb.total_value,
        anchored,
        systems,
        ship_ids,
        names,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_engagement(
    client: &reqwest::blocking::Client,
    km: &Killmail,
    kill_id: i64,
    isk: f64,
    anchored: bool,
    systems: &Systems,
    ship_ids: &std::collections::HashSet<i64>,
    names: &mut HashMap<i64, String>,
) -> Option<Engagement> {
    if !is_listed_hull(km.victim.ship_type_id.unwrap_or(0), ship_ids) {
        return None;
    }
    let sys = systems.info_of(km.solar_system_id)?;
    let time = chrono::DateTime::parse_from_rfc3339(&km.killmail_time)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());
    resolve_names(client, km, names);
    let attackers = attackers_of(km, names, ship_ids);
    if attackers.is_empty() {
        return None;
    }
    Some(Engagement {
        kill_id,
        time,
        system_id: sys.id,
        system_name: sys.name.clone(),
        security: sys.security,
        victim: party_of(&km.victim, names),
        victim_char: km.victim.character_id.unwrap_or(0),
        victim_pilot: pilot_of(&km.victim, names),
        victim_ship: km.victim.ship_type_id.unwrap_or(0),
        attackers,
        isk,
        anchored,
    })
}

fn nearest_intel_jumps(
    systems: &Systems,
    intel: &Mutex<IntelState>,
    kill_system: i64,
    cap: u32,
) -> Option<u32> {
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
    intel_systems.iter().filter_map(|&s| systems.jumps(kill_system, s, cap)).min()
}

fn ingest_match_data(
    km: &Killmail,
    sys: &crate::geo::SystemInfo,
    ship_sizes: &HashMap<i64, ShipSize>,
    systems: &Systems,
    player_sys: &AtomicI64,
    max_jumps: Option<u32>,
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
    if let Some(maxj) = max_jumps {
        let me = player_sys.load(Ordering::Relaxed);
        if me != 0 {
            d.min_jumps_from_me = systems.jumps(sys.id, me, maxj);
        }
    }
    d
}

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

fn pilot_of(c: &Combatant, names: &HashMap<i64, String>) -> String {
    let id = c.character_id.or(c.corporation_id).or(c.alliance_id).unwrap_or(0);
    names.get(&id).cloned().unwrap_or_else(|| "Unknown".to_owned())
}

/// NPCs. Player corporations are >= 98,000,000; a player-owned structure keeps that corp id, so
fn is_npc_attacker(c: &Combatant) -> bool {
    c.character_id.is_none() && c.corporation_id.map_or(true, |id| id < 98_000_000)
}

fn attackers_of(
    km: &Killmail,
    names: &HashMap<i64, String>,
    ship_ids: &std::collections::HashSet<i64>,
) -> Vec<Attacker> {
    km.attackers
        .iter()
        .filter(|a| !is_npc_attacker(a))
        .map(|a| Attacker {
            party: party_of(a, names),
            char_id: a.character_id.unwrap_or(0),
            ship: attacker_ship(a, ship_ids),
            pilot: pilot_of(a, names),
            final_blow: a.final_blow,
        })
        .collect()
}

/// The hull an attacker flew. ESI often omits `ship_type_id` and instead records the ship in
/// `weapon_type_id` (e.g. a smartbombing or ramming Praxis), which would otherwise show as a
/// blank "?" hull. Fall back to the weapon when it is itself a listed hull; a real module/fighter
/// weapon (a launcher, a warp scrambler) is ignored, leaving the hull genuinely unknown (0).
fn attacker_ship(a: &Combatant, ship_ids: &std::collections::HashSet<i64>) -> i64 {
    a.ship_type_id
        .filter(|&s| s != 0)
        .or_else(|| a.weapon_type_id.filter(|&w| is_listed_hull(w, ship_ids)))
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct NameEntry {
    id: i64,
    name: String,
}

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

fn fetch_posted_kill(
    client: &reqwest::blocking::Client,
    id: i64,
    systems: &Systems,
    ship_ids: &std::collections::HashSet<i64>,
    names: &mut HashMap<i64, String>,
) -> Option<Engagement> {
    let (km, value) = fetch_kill_by_id(client, id)?;
    build_engagement(client, &km, id, value, true, systems, ship_ids, names)
}

fn fetch_kill_by_id(
    client: &reqwest::blocking::Client,
    id: i64,
) -> Option<(Killmail, f64)> {
    let zk: Vec<ZkApiEntry> = client
        .get(format!("{ZKILL_API}/killID/{id}/"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    let entry = zk.into_iter().next()?;
    let km = fetch_killmail_detail(client, id, &entry.zkb.hash)?;
    Some((km, entry.zkb.total_value))
}

fn fetch_killmail_detail(
    client: &reqwest::blocking::Client,
    id: i64,
    hash: &str,
) -> Option<Killmail> {
    let resp = client.get(format!("{ESI}/killmails/{id}/{hash}/")).send().ok()?;
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        crate::esilog::record(
            "killmails detail non-2xx",
            &format!("status: {status}\nkill id: {id}\nhash: {hash}\nbody:\n{body}"),
        );
        return None;
    }
    serde_json::from_str(&body).ok()
}

fn should_backfill(seen: &mut HashMap<i64, i64>, system: i64, now: i64) -> bool {
    match seen.get(&system) {
        Some(&last) if now - last < BACKFILL_DEBOUNCE_SECS => false,
        _ => {
            seen.insert(system, now);
            true
        }
    }
}

fn fold_engagement(
    eng: Engagement,
    now: i64,
    buffer: &mut Vec<Engagement>,
    buffer_ids: &mut std::collections::HashSet<i64>,
) -> bool {
    if now - eng.time > ENGAGEMENT_TTL {
        return false;
    }
    if !buffer_ids.insert(eng.kill_id) {
        return false;
    }
    buffer.push(eng);
    true
}

#[allow(clippy::too_many_arguments)]
fn backfill_system(
    client: &reqwest::blocking::Client,
    system_id: i64,
    oldest: i64,
    newest: i64,
    systems: &Systems,
    ship_ids: &std::collections::HashSet<i64>,
    have: &std::collections::HashSet<i64>,
    collect: &mut dyn FnMut(Engagement),
) {
    let url = format!("{ZKILL_API}/solarSystemID/{system_id}/");
    let zk: serde_json::Value =
        match client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.json()) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[zkill] backfill {system_id} failed: {e}");
                return;
            }
        };
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut fetched = 0usize;
    for km in zk.as_array().cloned().unwrap_or_default().iter().take(BACKFILL_MAX_KILLS) {
        let Some(id) = km.get("killmail_id").and_then(|v| v.as_i64()) else { continue };
        if have.contains(&id) {
            continue;
        }
        let Some(hash) = km.get("zkb").and_then(|z| z.get("hash")).and_then(|h| h.as_str()) else {
            continue;
        };
        let value =
            km.get("zkb").and_then(|z| z.get("totalValue")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let Some(detail) = fetch_killmail_detail(client, id, hash) else { continue };
        fetched += 1;
        if fetched % 6 == 0 {
            std::thread::sleep(Duration::from_millis(1100));
        }
        let t = chrono::DateTime::parse_from_rfc3339(&detail.killmail_time)
            .map(|d| d.timestamp())
            .unwrap_or(0);
        // The list is newest-first, so once we drop below the window the rest are older still.
        if t != 0 && t < oldest {
            break;
        }
        if t > newest {
            continue;
        }
        if let Some(eng) =
            build_engagement(client, &detail, id, value, true, systems, ship_ids, &mut names)
        {
            collect(eng);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_backfill(
    client: reqwest::blocking::Client,
    system_id: i64,
    oldest: i64,
    newest: i64,
    systems: Arc<Systems>,
    ship_ids: Arc<std::collections::HashSet<i64>>,
    have: std::collections::HashSet<i64>,
    out: SharedBackfill,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        backfill_system(&client, system_id, oldest, newest, &systems, &ship_ids, &have, &mut |eng| {
            out.lock().unwrap().push(eng);
        });
        ctx.request_repaint();
    });
}

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
    // /universe/names allows up to 1000 ids; chunk well under that. A big fleet fight can
    // reference thousands of ids, so an un-chunked POST would overflow the limit.
    for chunk in wanted.chunks(200) {
        resolve_names_batch(client, chunk, names);
    }
}

/// Resolve one batch of ids, inserting results into `names`. ESI returns 404 for the
/// *entire* request if even one id is unresolvable (e.g. a deleted character), so on a
/// 404 we bisect to isolate and skip the bad id instead of blanking the whole roster.
fn resolve_names_batch(
    client: &reqwest::blocking::Client,
    ids: &[i64],
    names: &mut HashMap<i64, String>,
) {
    if ids.is_empty() {
        return;
    }
    match client.post(NAMES_URL).json(&ids).send() {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(entries) = resp.json::<Vec<NameEntry>>() {
                for e in entries {
                    names.insert(e.id, e.name);
                }
            }
        }
        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND && ids.len() > 1 => {
            let mid = ids.len() / 2;
            resolve_names_batch(client, &ids[..mid], names);
            resolve_names_batch(client, &ids[mid..], names);
        }
        Ok(resp) if !resp.status().is_success() => {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            crate::esilog::record(
                "universe/names non-2xx",
                &format!("status: {status}\nbatch size: {}\nbody:\n{body}", ids.len()),
            );
        }
        _ => {}
    }
}

#[derive(Clone, Default)]
#[allow(dead_code)]
pub enum BuildFromKill {
    #[default]
    Idle,
    Loading,
    Done(Vec<Engagement>, i64),
    Failed(String),
}

pub type SharedBuildFromKill = Arc<Mutex<BuildFromKill>>;

#[allow(dead_code)]
pub fn parse_kill_id(input: &str) -> Option<i64> {
    let s = input.trim();
    if let Ok(id) = s.parse::<i64>() {
        return (id > 0).then_some(id);
    }
    let after = s.split("/kill/").nth(1)?;
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().filter(|&id| id > 0)
}

#[allow(dead_code)]
pub fn build_report_from_kill(
    kill_id: i64,
    systems: &Systems,
    ship_ids: &std::collections::HashSet<i64>,
) -> Result<(Vec<Engagement>, i64), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool; battle import)"))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let (seed_km, seed_value) = fetch_kill_by_id(&client, kill_id)
        .ok_or_else(|| format!("Could not fetch kill {kill_id} from zKillboard"))?;
    let seed_time = chrono::DateTime::parse_from_rfc3339(&seed_km.killmail_time)
        .map(|d| d.timestamp())
        .map_err(|e| format!("Bad kill time: {e}"))?;
    let system_id = seed_km.solar_system_id;
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut engagements: Vec<Engagement> = Vec::new();
    let mut have: std::collections::HashSet<i64> = std::collections::HashSet::new();
    if let Some(seed) =
        build_engagement(&client, &seed_km, kill_id, seed_value, true, systems, ship_ids, &mut names)
    {
        have.insert(kill_id);
        engagements.push(seed);
    }
    backfill_system(
        &client,
        system_id,
        seed_time - BACKFILL_WINDOW_SECS,
        seed_time + BACKFILL_WINDOW_SECS,
        systems,
        ship_ids,
        &have,
        &mut |eng| engagements.push(eng),
    );
    if engagements.is_empty() {
        return Err(format!("No battle found around kill {kill_id}"));
    }
    Ok((engagements, kill_id))
}

#[allow(dead_code)]
pub fn spawn_build_from_kill(
    kill_id: i64,
    systems: Arc<Systems>,
    ship_ids: Arc<std::collections::HashSet<i64>>,
    result: SharedBuildFromKill,
    ctx: egui::Context,
) {
    *result.lock().unwrap() = BuildFromKill::Loading;
    ctx.request_repaint();
    std::thread::spawn(move || {
        let out = match build_report_from_kill(kill_id, &systems, &ship_ids) {
            Ok((engs, seed)) => BuildFromKill::Done(engs, seed),
            Err(e) => BuildFromKill::Failed(e),
        };
        *result.lock().unwrap() = out;
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_r2z2_killmail() {
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
        let seq: Sequence = serde_json::from_str(r#"{"sequence":98212646}"#).unwrap();
        assert_eq!(seq.sequence, 98212646);
    }

    #[test]
    fn npc_attacker_detection() {
        let parse = |j: &str| -> Combatant { serde_json::from_str(j).unwrap() };
        assert!(is_npc_attacker(&parse(r#"{"corporation_id":1000127}"#)));
        assert!(is_npc_attacker(&parse(r#"{"ship_type_id":1234}"#)));
        assert!(!is_npc_attacker(&parse(r#"{"character_id":95538921,"corporation_id":1000127}"#)));
        assert!(!is_npc_attacker(&parse(r#"{"corporation_id":98000001}"#)));
    }

    #[test]
    fn attacker_ship_falls_back_to_weapon_hull() {
        let parse = |j: &str| -> Combatant { serde_json::from_str(j).unwrap() };
        let hulls: std::collections::HashSet<i64> = [47466].into_iter().collect();
        assert_eq!(attacker_ship(&parse(r#"{"ship_type_id":587}"#), &hulls), 587);
        assert_eq!(attacker_ship(&parse(r#"{"weapon_type_id":47466}"#), &hulls), 47466);
        assert_eq!(attacker_ship(&parse(r#"{"weapon_type_id":448}"#), &hulls), 0);
        assert_eq!(attacker_ship(&parse(r#"{"character_id":3}"#), &hulls), 0);
    }

    #[test]
    fn backfill_debounce() {
        let mut seen: HashMap<i64, i64> = HashMap::new();
        let now = 1_000_000i64;
        assert!(should_backfill(&mut seen, 30000142, now));
        assert!(!should_backfill(&mut seen, 30000142, now + 60));
        assert!(!should_backfill(&mut seen, 30000142, now + BACKFILL_DEBOUNCE_SECS - 1));
        assert!(should_backfill(&mut seen, 30000142, now + BACKFILL_DEBOUNCE_SECS));
        assert!(should_backfill(&mut seen, 30002187, now + 60));
    }

    #[test]
    fn fold_dedup_and_ttl() {
        let now = 2_000_000i64;
        let mk = |kill_id: i64, time: i64| Engagement {
            kill_id,
            time,
            system_id: 30000142,
            system_name: "Jita".into(),
            security: 0.9,
            victim: Party { id: 1, name: "V".into(), kind: PartyKind::Character },
            victim_char: 1,
            victim_pilot: "V".into(),
            victim_ship: 587,
            attackers: Vec::new(),
            isk: 0.0,
            anchored: true,
        };
        let mut buffer: Vec<Engagement> = Vec::new();
        let mut ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        assert!(fold_engagement(mk(100, now - 10), now, &mut buffer, &mut ids));
        assert_eq!(buffer.len(), 1);
        assert!(!fold_engagement(mk(100, now - 5), now, &mut buffer, &mut ids));
        assert_eq!(buffer.len(), 1);
        assert!(!fold_engagement(mk(101, now - ENGAGEMENT_TTL - 1), now, &mut buffer, &mut ids));
        assert_eq!(buffer.len(), 1);
        assert!(fold_engagement(mk(102, now - 20), now, &mut buffer, &mut ids));
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn build_engagement_golden() {
        use crate::geo::SystemInfo;
        let info = SystemInfo {
            id: 30000142,
            name: "Jita".into(),
            security: 0.9,
            constellation: "Kimotoro".into(),
            region: "The Forge".into(),
            faction: String::new(),
        };
        let by_name: HashMap<String, SystemInfo> =
            [("jita".to_string(), info)].into_iter().collect();
        let systems = Systems::new(by_name, HashMap::new());
        let ship_ids: std::collections::HashSet<i64> = [587, 670, 17738].into_iter().collect();
        let mut names: HashMap<i64, String> = HashMap::new();
        for (id, name) in
            [(99, "VAlli"), (98, "VCorp"), (97, "VPilot"), (1, "AAlli"), (2, "ACorp"), (3, "APilot")]
        {
            names.insert(id, name.to_string());
        }
        let km: Killmail = serde_json::from_str(
            r#"{"killmail_id":12345,"killmail_time":"2026-06-22T18:30:45Z",
                "solar_system_id":30000142,
                "victim":{"alliance_id":99,"corporation_id":98,"character_id":97,"ship_type_id":587},
                "attackers":[{"alliance_id":1,"corporation_id":2,"character_id":3,
                    "ship_type_id":17738,"final_blow":true}]}"#,
        )
        .unwrap();
        let client = reqwest::blocking::Client::new();
        let eng =
            build_engagement(&client, &km, 12345, 9_999.0, true, &systems, &ship_ids, &mut names)
                .expect("listed-hull victim with a capsuleer attacker builds an engagement");
        assert_eq!(eng.kill_id, 12345);
        assert_eq!(
            eng.time,
            chrono::DateTime::parse_from_rfc3339("2026-06-22T18:30:45Z").unwrap().timestamp()
        );
        assert_eq!(eng.system_id, 30000142);
        assert_eq!(eng.system_name, "Jita");
        assert_eq!(eng.security, 0.9);
        assert_eq!(eng.victim.id, 99);
        assert_eq!(eng.victim.name, "VAlli");
        assert_eq!(eng.victim.kind, PartyKind::Alliance);
        assert_eq!(eng.victim_char, 97);
        assert_eq!(eng.victim_pilot, "VPilot");
        assert_eq!(eng.victim_ship, 587);
        assert_eq!(eng.attackers.len(), 1);
        assert_eq!(eng.attackers[0].party.id, 1);
        assert_eq!(eng.attackers[0].ship, 17738);
        assert!(eng.attackers[0].final_blow);
        assert_eq!(eng.isk, 9_999.0);
        assert!(eng.anchored);
        let depot: Killmail = serde_json::from_str(
            r#"{"killmail_id":1,"killmail_time":"2026-06-22T18:30:45Z","solar_system_id":30000142,
                "victim":{"ship_type_id":33519},"attackers":[{"character_id":3}]}"#,
        )
        .unwrap();
        assert!(build_engagement(&client, &depot, 1, 0.0, true, &systems, &ship_ids, &mut names)
            .is_none());
    }

    #[test]
    fn parse_kill_id_forms() {
        assert_eq!(parse_kill_id("128431979"), Some(128431979));
        assert_eq!(parse_kill_id("  128431979 "), Some(128431979));
        assert_eq!(parse_kill_id("https://zkillboard.com/kill/128431979/"), Some(128431979));
        assert_eq!(parse_kill_id("https://zkillboard.com/kill/128431979"), Some(128431979));
        assert_eq!(parse_kill_id("zkillboard.com/kill/128431979/"), Some(128431979));
        assert_eq!(parse_kill_id("https://zkillboard.com/character/123/"), None);
        assert_eq!(parse_kill_id("not a kill"), None);
        assert_eq!(parse_kill_id("0"), None);
        assert_eq!(parse_kill_id(""), None);
    }
}
