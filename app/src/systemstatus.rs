use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

const INCURSIONS_URL: &str = "https://esi.evetech.net/latest/incursions/";
const FW_URL: &str = "https://esi.evetech.net/latest/fw/systems/";
const SOV_URL: &str = "https://esi.evetech.net/latest/sovereignty/map/";
const SOV_STRUCT_URL: &str = "https://esi.evetech.net/latest/sovereignty/structures/";
const NAMES_URL: &str = "https://esi.evetech.net/latest/universe/names/";
const KILLS_URL: &str = "https://esi.evetech.net/latest/universe/system_kills/";
const JUMPS_URL: &str = "https://esi.evetech.net/latest/universe/system_jumps/";
const POLL: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SysFlags {
    pub incursion: bool,
    pub fw: Option<String>,
    pub sov: Option<String>,
    pub sov_alliance: Option<i64>,
    /// NPC sovereignty: set instead of `sov_alliance` in NPC null.
    pub sov_faction: Option<i64>,
    pub adm: Option<f64>,
    pub ship_kills: u32,
    pub pod_kills: u32,
    pub npc_kills: u32,
    pub jumps: u32,
}

pub type SharedStatus = Arc<Mutex<HashMap<i64, SysFlags>>>;

pub fn spawn(status: SharedStatus, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(30))
            .build()
        else {
            return;
        };
        let mut alliance_names: HashMap<i64, String> = HashMap::new();
        loop {
            if let Some(map) = fetch(&client, &mut alliance_names) {
                *status.lock().unwrap() = map;
                ctx.request_repaint();
            }
            std::thread::sleep(POLL);
        }
    });
}

#[derive(Deserialize)]
struct Incursion {
    #[serde(default)]
    infested_solar_systems: Vec<i64>,
}

#[derive(Deserialize)]
struct FwSystem {
    solar_system_id: i64,
    contested: String,
    occupier_faction_id: i64,
}

#[derive(Deserialize)]
struct SovSystem {
    system_id: i64,
    alliance_id: Option<i64>,
    faction_id: Option<i64>,
}

#[derive(Deserialize)]
struct SovStructure {
    solar_system_id: i64,
    vulnerability_occupancy_level: Option<f64>,
}

#[derive(Deserialize)]
struct SystemKills {
    system_id: i64,
    #[serde(default)]
    ship_kills: u32,
    #[serde(default)]
    pod_kills: u32,
    #[serde(default)]
    npc_kills: u32,
}

#[derive(Deserialize)]
struct SystemJumps {
    system_id: i64,
    #[serde(default)]
    ship_jumps: u32,
}

fn fetch(
    client: &reqwest::blocking::Client,
    alliance_names: &mut HashMap<i64, String>,
) -> Option<HashMap<i64, SysFlags>> {
    let mut map: HashMap<i64, SysFlags> = HashMap::new();

    if let Ok(incursions) = get::<Vec<Incursion>>(client, INCURSIONS_URL) {
        for inc in incursions {
            for sys in inc.infested_solar_systems {
                map.entry(sys).or_default().incursion = true;
            }
        }
    }

    if let Ok(fw) = get::<Vec<FwSystem>>(client, FW_URL) {
        for s in fw {
            if s.contested == "uncontested" {
                continue;
            }
            let faction = crate::factions::name(s.occupier_faction_id);
            let label = if faction.is_empty() {
                s.contested.clone()
            } else {
                format!("{} ({faction})", s.contested)
            };
            map.entry(s.solar_system_id).or_default().fw = Some(label);
        }
    }

    if let Ok(sov) = get::<Vec<SovSystem>>(client, SOV_URL) {
        let wanted: Vec<i64> = sov
            .iter()
            .filter_map(|s| s.alliance_id)
            .filter(|id| !alliance_names.contains_key(id))
            .collect();
        resolve_names(client, &wanted, alliance_names);

        for s in sov {
            let holder = if let Some(aid) = s.alliance_id {
                alliance_names.get(&aid).cloned()
            } else {
                s.faction_id
                    .map(crate::factions::name)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
            };
            if let Some(h) = holder {
                let e = map.entry(s.system_id).or_default();
                e.sov = Some(h);
                e.sov_alliance = s.alliance_id;
                e.sov_faction = s.alliance_id.is_none().then_some(s.faction_id).flatten();
            }
        }
    }

    // Activity Defense Multiplier from the I-Hub's vulnerability_occupancy_level.
    if let Ok(structs) = get::<Vec<SovStructure>>(client, SOV_STRUCT_URL) {
        for st in structs {
            if let Some(adm) = st.vulnerability_occupancy_level {
                let e = map.entry(st.solar_system_id).or_default();
                e.adm = Some(e.adm.map_or(adm, |cur| cur.max(adm)));
            }
        }
    }

    if let Ok(kills) = get::<Vec<SystemKills>>(client, KILLS_URL) {
        for k in kills {
            let e = map.entry(k.system_id).or_default();
            e.ship_kills = k.ship_kills;
            e.pod_kills = k.pod_kills;
            e.npc_kills = k.npc_kills;
        }
    }

    if let Ok(jumps) = get::<Vec<SystemJumps>>(client, JUMPS_URL) {
        for j in jumps {
            map.entry(j.system_id).or_default().jumps = j.ship_jumps;
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn get<T: for<'de> Deserialize<'de>>(
    client: &reqwest::blocking::Client,
    url: &str,
) -> reqwest::Result<T> {
    client.get(url).send()?.json::<T>()
}

#[derive(Deserialize)]
struct NameEntry {
    id: i64,
    name: String,
}

fn resolve_names(
    client: &reqwest::blocking::Client,
    ids: &[i64],
    cache: &mut HashMap<i64, String>,
) {
    let mut unique: Vec<i64> = ids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    // /universe/names accepts up to 1000 ids per call.
    for chunk in unique.chunks(1000) {
        if let Ok(resp) = client.post(NAMES_URL).json(chunk).send() {
            if let Ok(entries) = resp.json::<Vec<NameEntry>>() {
                for e in entries {
                    cache.insert(e.id, e.name);
                }
            }
        }
    }
}
