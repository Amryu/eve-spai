//! One killmail in full, as the killmail window shows it: zKillboard's values and labels, and ESI's
//! victim, attackers and items, with the names and prices to read them by.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const ESI: &str = "https://esi.evetech.net/latest";
const ZKILL: &str = "https://zkillboard.com/api";

/// The victim or one attacker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Who {
    pub char_id: i64,
    pub corp_id: i64,
    pub alliance_id: i64,
    pub faction_id: i64,
    pub ship: i64,
    pub weapon: i64,
    /// Damage done by an attacker, or taken by the victim.
    pub damage: i64,
    pub final_blow: bool,
    pub security: f64,
}

/// An item in the victim's fit or holds. Containers' contents come after their container, one level
/// deeper.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KillItem {
    pub type_id: i64,
    pub flag: i64,
    pub dropped: i64,
    pub destroyed: i64,
    /// A blueprint copy, which is worth nothing on the market.
    pub singleton: bool,
    pub depth: u8,
}

/// zKillboard's figures for the kill.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zkb {
    pub total: f64,
    pub fitted: f64,
    pub dropped: f64,
    pub destroyed: f64,
    pub points: i64,
    pub npc: bool,
    pub solo: bool,
    pub awox: bool,
    pub labels: Vec<String>,
    pub location_id: i64,
}

#[derive(Clone, Debug, Default)]
pub struct KillDetail {
    pub kill_id: i64,
    pub hash: String,
    pub time: i64,
    pub system_id: i64,
    pub victim: Who,
    pub attackers: Vec<Who>,
    pub items: Vec<KillItem>,
    pub zkb: Zkb,
    /// Characters, corporations, alliances and factions by id.
    pub names: HashMap<i64, String>,
    /// Average market price by type, for the item values.
    pub prices: HashMap<i64, f64>,
    /// The celestial the kill was nearest, and how far from it in metres.
    pub near: Option<(String, f64)>,
    /// The victim's capsule, killed right after the ship: its value, killers and implants.
    pub pod: Option<Box<KillDetail>>,
}

/// Capsule and Genolution capsule.
pub const CAPSULES: [i64; 2] = [670, 33_328];

/// How long after the ship a capsule may die and still be the same fight's pod kill.
const POD_WITHIN_SECS: i64 = 900;

/// How many of the pilot's next losses are looked at for the pod.
const POD_CANDIDATES: usize = 3;

impl KillDetail {
    pub fn esi_url(&self) -> String {
        format!("{ESI}/killmails/{}/{}/", self.kill_id, self.hash)
    }

    pub fn name(&self, id: i64) -> Option<&str> {
        (id != 0).then(|| self.names.get(&id).map(String::as_str)).flatten()
    }

    pub fn total_damage(&self) -> i64 {
        self.attackers.iter().map(|a| a.damage).sum::<i64>().max(self.victim.damage)
    }

    /// The value of `qty` of an item, nothing for a blueprint copy.
    pub fn value_of(&self, item: &KillItem, qty: i64) -> f64 {
        if item.singleton {
            return 0.0;
        }
        self.prices.get(&item.type_id).copied().unwrap_or(0.0) * qty as f64
    }
}

pub fn zkill_url(kill_id: i64) -> String {
    format!("https://zkillboard.com/kill/{kill_id}/")
}

#[derive(Clone, Debug, Default)]
pub enum KillState {
    #[default]
    Loading,
    Done(Box<KillDetail>),
    Failed(String),
}

pub type SharedKill = Arc<Mutex<KillState>>;

/// Fetches kill `kill_id` on a thread. `hash` saves the zKillboard call for the ESI part when it is
/// already known; the values and labels still come from zKillboard.
pub fn spawn(kill_id: i64, hash: Option<String>, ctx: egui::Context) -> SharedKill {
    let state: SharedKill = Arc::new(Mutex::new(KillState::Loading));
    let out = state.clone();
    let _ = std::thread::Builder::new().name("killmail".into()).spawn(move || {
        let result = fetch(kill_id, hash);
        *out.lock().unwrap_or_else(|e| e.into_inner()) = match result {
            Ok(d) => KillState::Done(Box::new(d)),
            Err(e) => KillState::Failed(e),
        };
        ctx.request_repaint();
    });
    state
}

fn fetch(kill_id: i64, hash: Option<String>) -> Result<KillDetail, String> {
    let client = crate::http::client(30).map_err(|e| e.to_string())?;
    let zk: serde_json::Value = crate::zkapi::fetch_waiting(&client, &format!("{ZKILL}/killID/{kill_id}/"))
        .and_then(|r| r.json().map_err(|e| e.to_string()))?;
    let zkb_v = zk.as_array().and_then(|a| a.first()).and_then(|e| e.get("zkb")).cloned().unwrap_or_default();
    let zkb = parse_zkb(&zkb_v);
    let hash = hash
        .or_else(|| zkb_v.get("hash").and_then(|h| h.as_str()).map(str::to_owned))
        .ok_or("zKillboard does not know this kill")?;
    let km: serde_json::Value = client
        .get(format!("{ESI}/killmails/{kill_id}/{hash}/"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("ESI killmail: {e}"))?;
    let mut d = parse_killmail(&km).ok_or("the killmail could not be read")?;
    d.kill_id = kill_id;
    d.hash = hash;
    d.zkb = zkb;
    let mut ids: Vec<i64> = std::iter::once(&d.victim)
        .chain(d.attackers.iter())
        .flat_map(|w| [w.char_id, w.corp_id, w.alliance_id, w.faction_id])
        .filter(|&id| id != 0)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    d.prices = prices(&client);
    if d.victim.char_id != 0 && !CAPSULES.contains(&d.victim.ship) {
        d.pod = find_pod(&client, &d).map(Box::new);
    }
    // One lookup for the names on both kills.
    if let Some(pod) = &d.pod {
        ids.extend(pod.attackers.iter().flat_map(|w| [w.char_id, w.corp_id, w.alliance_id, w.faction_id]).filter(|&id| id != 0));
        ids.sort_unstable();
        ids.dedup();
    }
    d.names = crate::universe::names(&client, &ids);
    if let Some(pod) = d.pod.as_mut() {
        pod.names = d.names.clone();
        pod.prices = d.prices.clone();
    }
    if let (Some(pos), Ok(store)) = (km.get("victim").and_then(|v| v.get("position")), crate::store::Store::open()) {
        let p = |k: &str| pos.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
        d.near = store.nearest_celestial(d.system_id, [p("x"), p("y"), p("z")]);
    }
    Ok(d)
}

/// The capsule the victim lost right after the ship: among their next few losses on zKillboard, the
/// first that is a capsule, in the same system, within [`POD_WITHIN_SECS`].
fn find_pod(client: &reqwest::blocking::Client, ship: &KillDetail) -> Option<KillDetail> {
    let list: serde_json::Value =
        crate::zkapi::fetch_waiting(client, &format!("{ZKILL}/losses/characterID/{}/", ship.victim.char_id)).ok()?.json().ok()?;
    let mut later: Vec<(i64, serde_json::Value)> = list
        .as_array()?
        .iter()
        .filter_map(|e| Some((e.get("killmail_id")?.as_i64()?, e.get("zkb")?.clone())))
        .filter(|(id, _)| *id > ship.kill_id)
        .collect();
    later.sort_by_key(|(id, _)| *id);
    for (id, zkb) in later.into_iter().take(POD_CANDIDATES) {
        let Some(hash) = zkb.get("hash").and_then(|h| h.as_str()) else { continue };
        let Ok(km) = client
            .get(format!("{ESI}/killmails/{id}/{hash}/"))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json::<serde_json::Value>())
        else {
            continue;
        };
        let Some(mut pod) = parse_killmail(&km) else { continue };
        if is_pod_of(ship, &pod) {
            pod.kill_id = id;
            pod.hash = hash.to_owned();
            pod.zkb = parse_zkb(&zkb);
            return Some(pod);
        }
    }
    None
}

/// Whether `pod` is the capsule of `ship`'s victim, lost in the same place right after.
fn is_pod_of(ship: &KillDetail, pod: &KillDetail) -> bool {
    CAPSULES.contains(&pod.victim.ship)
        && pod.victim.char_id == ship.victim.char_id
        && pod.system_id == ship.system_id
        && (0..=POD_WITHIN_SECS).contains(&(pod.time - ship.time))
}

fn parse_zkb(v: &serde_json::Value) -> Zkb {
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let b = |k: &str| v.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
    Zkb {
        total: f("totalValue"),
        fitted: f("fittedValue"),
        dropped: f("droppedValue"),
        destroyed: f("destroyedValue"),
        points: v.get("points").and_then(|x| x.as_i64()).unwrap_or(0),
        npc: b("npc"),
        solo: b("solo"),
        awox: b("awox"),
        labels: v
            .get("labels")
            .and_then(|l| l.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        location_id: v.get("locationID").and_then(|x| x.as_i64()).unwrap_or(0),
    }
}

fn who(v: &serde_json::Value) -> Who {
    let i = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
    Who {
        char_id: i("character_id"),
        corp_id: i("corporation_id"),
        alliance_id: i("alliance_id"),
        faction_id: i("faction_id"),
        ship: i("ship_type_id"),
        weapon: i("weapon_type_id"),
        damage: i("damage_done").max(i("damage_taken")),
        final_blow: v.get("final_blow").and_then(|x| x.as_bool()).unwrap_or(false),
        security: v.get("security_status").and_then(|x| x.as_f64()).unwrap_or(0.0),
    }
}

fn items_into(arr: &[serde_json::Value], depth: u8, out: &mut Vec<KillItem>) {
    for it in arr {
        let i = |k: &str| it.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        out.push(KillItem {
            type_id: i("item_type_id"),
            flag: i("flag"),
            dropped: i("quantity_dropped"),
            destroyed: i("quantity_destroyed"),
            singleton: i("singleton") == 2,
            depth,
        });
        if let Some(inner) = it.get("items").and_then(|x| x.as_array()) {
            items_into(inner, depth + 1, out);
        }
    }
}

/// The ESI killmail, read into the parts the window shows.
pub fn parse_killmail(km: &serde_json::Value) -> Option<KillDetail> {
    let victim = km.get("victim")?;
    let mut items = Vec::new();
    if let Some(arr) = victim.get("items").and_then(|x| x.as_array()) {
        items_into(arr, 0, &mut items);
    }
    let mut attackers: Vec<Who> = km.get("attackers")?.as_array()?.iter().map(who).collect();
    // As zKillboard lists them: the final blow first, then by damage.
    attackers.sort_by(|a, b| b.final_blow.cmp(&a.final_blow).then(b.damage.cmp(&a.damage)));
    Some(KillDetail {
        kill_id: km.get("killmail_id").and_then(|x| x.as_i64()).unwrap_or(0),
        time: km
            .get("killmail_time")
            .and_then(|t| t.as_str())
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map_or(0, |t| t.timestamp()),
        system_id: km.get("solar_system_id").and_then(|x| x.as_i64()).unwrap_or(0),
        victim: who(victim),
        attackers,
        items,
        ..Default::default()
    })
}

/// ESI's average price for every type, fetched once a session.
fn prices(client: &reqwest::blocking::Client) -> HashMap<i64, f64> {
    static PRICES: OnceLock<Mutex<Option<HashMap<i64, f64>>>> = OnceLock::new();
    let cell = PRICES.get_or_init(|| Mutex::new(None));
    if let Some(p) = cell.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return p.clone();
    }
    let got: HashMap<i64, f64> = client
        .get(format!("{ESI}/markets/prices/"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json::<Vec<serde_json::Value>>())
        .map(|v| {
            v.iter()
                .filter_map(|p| {
                    let id = p.get("type_id")?.as_i64()?;
                    let price = p.get("average_price").or_else(|| p.get("adjusted_price"))?.as_f64()?;
                    Some((id, price))
                })
                .collect()
        })
        .unwrap_or_default();
    if !got.is_empty() {
        *cell.lock().unwrap_or_else(|e| e.into_inner()) = Some(got.clone());
    }
    got
}

/// Where an item sat, by its flag, in the order the window lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hold {
    High,
    Mid,
    Low,
    Rig,
    Subsystem,
    Service,
    DroneBay,
    FighterBay,
    Implants,
    Cargo,
    FleetHangar,
    ShipHangar,
    Other,
}

impl Hold {
    pub fn of(flag: i64) -> Hold {
        match flag {
            27..=34 => Hold::High,
            19..=26 => Hold::Mid,
            11..=18 => Hold::Low,
            92..=99 => Hold::Rig,
            125..=132 => Hold::Subsystem,
            164..=171 => Hold::Service,
            87 => Hold::DroneBay,
            158..=163 => Hold::FighterBay,
            89 => Hold::Implants,
            5 | 133..=143 | 148 | 149 | 151 | 176..=179 | 181..=186 => Hold::Cargo,
            155 => Hold::FleetHangar,
            90 => Hold::ShipHangar,
            _ => Hold::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hold::High => "High slots",
            Hold::Mid => "Mid slots",
            Hold::Low => "Low slots",
            Hold::Rig => "Rigs",
            Hold::Subsystem => "Subsystems",
            Hold::Service => "Service slots",
            Hold::DroneBay => "Drone bay",
            Hold::FighterBay => "Fighter bay",
            Hold::Implants => "Implants",
            Hold::Cargo => "Cargo",
            Hold::FleetHangar => "Fleet hangar",
            Hold::ShipHangar => "Ship hangar",
            Hold::Other => "Other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "killmail_id": 1, "killmail_time": "2026-10-01T12:00:00Z", "solar_system_id": 30000142,
            "victim": {
                "character_id": 10, "corporation_id": 20, "alliance_id": 30, "ship_type_id": 587,
                "damage_taken": 5000, "position": {"x": 1.0, "y": 2.0, "z": 3.0},
                "items": [
                    {"item_type_id": 2873, "flag": 27, "quantity_destroyed": 1, "singleton": 0},
                    {"item_type_id": 3001, "flag": 5, "quantity_dropped": 10, "singleton": 0,
                     "items": [{"item_type_id": 4000, "flag": 0, "quantity_dropped": 2, "singleton": 0}]}
                ]
            },
            "attackers": [
                {"character_id": 11, "corporation_id": 21, "ship_type_id": 588, "weapon_type_id": 2873, "damage_done": 1000, "final_blow": false, "security_status": -2.5},
                {"character_id": 12, "corporation_id": 22, "ship_type_id": 589, "weapon_type_id": 2873, "damage_done": 4000, "final_blow": false},
                {"character_id": 13, "corporation_id": 23, "ship_type_id": 590, "damage_done": 0, "final_blow": true}
            ]
        })
    }

    #[test]
    fn a_killmail_reads_into_victim_attackers_and_items() {
        let d = parse_killmail(&sample()).unwrap();
        assert_eq!((d.victim.char_id, d.victim.ship, d.victim.damage), (10, 587, 5000));
        let order: Vec<i64> = d.attackers.iter().map(|a| a.char_id).collect();
        assert_eq!(order, vec![13, 12, 11], "the final blow first, then by damage");
        assert_eq!(d.attackers[2].security, -2.5);
        assert_eq!(d.items.len(), 3, "a container's contents are listed too");
        assert_eq!((d.items[2].type_id, d.items[2].depth, d.items[2].dropped), (4000, 1, 2));
        assert_eq!(Hold::of(d.items[0].flag), Hold::High);
        assert_eq!(Hold::of(d.items[1].flag), Hold::Cargo);
        assert_eq!(d.total_damage(), 5000);
    }

    #[test]
    fn a_capsule_lost_right_after_in_the_same_place_is_the_pod_kill() {
        let ship = parse_killmail(&sample()).unwrap();
        let pod = |ship_type: i64, char_id: i64, system: i64, after: i64| KillDetail {
            victim: Who { char_id, ship: ship_type, ..Default::default() },
            system_id: system,
            time: ship.time + after,
            ..Default::default()
        };
        assert!(is_pod_of(&ship, &pod(670, 10, 30000142, 20)));
        assert!(is_pod_of(&ship, &pod(33_328, 10, 30000142, 0)));
        assert!(!is_pod_of(&ship, &pod(587, 10, 30000142, 20)), "another ship, not a capsule");
        assert!(!is_pod_of(&ship, &pod(670, 11, 30000142, 20)), "someone else's capsule");
        assert!(!is_pod_of(&ship, &pod(670, 10, 30000144, 20)), "somewhere else");
        assert!(!is_pod_of(&ship, &pod(670, 10, 30000142, POD_WITHIN_SECS + 1)), "too long after");
        assert!(!is_pod_of(&ship, &pod(670, 10, 30000142, -5)), "before the ship");
    }

    #[test]
    fn zkillboards_figures_and_labels_are_read() {
        let z = parse_zkb(&serde_json::json!({
            "totalValue": 1.5e9, "fittedValue": 1e9, "droppedValue": 4e8, "destroyedValue": 1.1e9,
            "points": 12, "solo": true, "npc": false, "awox": false, "labels": ["pvp", "loc:nullsec"],
            "locationID": 40000001, "hash": "abc"
        }));
        assert_eq!((z.total, z.points, z.solo, z.location_id), (1.5e9, 12, true, 40000001));
        assert_eq!(z.labels, vec!["pvp".to_owned(), "loc:nullsec".to_owned()]);
    }
}
