use serde::Deserialize;
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct LookupInfo {
    pub char_id: i64,
    pub name: String,
    pub corp: String,
    pub alliance: String,
    pub corp_id: Option<i64>,
    pub alliance_id: Option<i64>,
    pub ships_destroyed: i64,
    pub ships_lost: i64,
    pub isk_destroyed: f64,
    pub isk_lost: f64,
    pub danger_ratio: i64,
    pub gang_ratio: i64,
    pub top_ships: Vec<(i64, String, i64)>,
    pub top_systems: Vec<(String, i64)>,
    pub found: bool,
}

pub type LookupCache = Arc<Mutex<HashMap<String, Option<LookupInfo>>>>;
pub type LookupSender = Sender<String>;

pub fn spawn_fetcher(cache: LookupCache, ctx: egui::Context) -> LookupSender {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool; +github.com/Amryu/eve-spai)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        for name in rx {
            let key = name.to_lowercase();
            {
                let mut c = cache.lock().unwrap();
                if c.get(&key).is_some_and(|v| v.is_some()) {
                    continue;
                }
                c.entry(key.clone()).or_insert(None);
            }
            let info = fetch(&client, &name);
            cache.lock().unwrap().insert(key, Some(info));
            ctx.request_repaint();
            std::thread::sleep(Duration::from_millis(250));
        }
    });
    tx
}

fn fetch(client: &reqwest::blocking::Client, name: &str) -> LookupInfo {
    let mut info = LookupInfo { name: name.to_owned(), ..Default::default() };
    let Some(id) = resolve_id(client, name) else {
        return info;
    };
    info.char_id = id;
    info.found = true;

    #[derive(Deserialize)]
    struct Char {
        name: String,
        corporation_id: Option<i64>,
        alliance_id: Option<i64>,
    }
    if let Some(c) = client
        .get(format!("https://esi.evetech.net/latest/characters/{id}/?datasource=tranquility"))
        .send()
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .and_then(|r| r.json::<Char>().ok())
    {
        info.name = c.name;
        info.corp_id = c.corporation_id;
        info.alliance_id = c.alliance_id;
        let ids: Vec<i64> = [c.corporation_id, c.alliance_id].into_iter().flatten().collect();
        let names = resolve_names(client, &ids);
        if let Some(cid) = c.corporation_id {
            info.corp = names.get(&cid).cloned().unwrap_or_default();
        }
        if let Some(aid) = c.alliance_id {
            info.alliance = names.get(&aid).cloned().unwrap_or_default();
        }
    }

    #[derive(Deserialize)]
    struct TopValue {
        #[serde(rename = "shipTypeID")]
        ship_type_id: Option<i64>,
        #[serde(rename = "shipName")]
        ship_name: Option<String>,
        #[serde(rename = "solarSystemName")]
        system_name: Option<String>,
        #[serde(default)]
        kills: i64,
    }
    #[derive(Deserialize)]
    struct TopList {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        values: Vec<TopValue>,
    }
    #[derive(Deserialize)]
    struct Stats {
        #[serde(rename = "shipsDestroyed", default)]
        ships_destroyed: i64,
        #[serde(rename = "shipsLost", default)]
        ships_lost: i64,
        #[serde(rename = "iskDestroyed", default)]
        isk_destroyed: f64,
        #[serde(rename = "iskLost", default)]
        isk_lost: f64,
        #[serde(rename = "dangerRatio", default)]
        danger_ratio: i64,
        #[serde(rename = "gangRatio", default)]
        gang_ratio: i64,
        #[serde(rename = "topLists", default)]
        top_lists: Vec<TopList>,
    }
    if let Some(s) = client
        .get(format!("https://zkillboard.com/api/stats/characterID/{id}/"))
        .send()
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .and_then(|r| r.json::<Stats>().ok())
    {
        info.ships_destroyed = s.ships_destroyed;
        info.ships_lost = s.ships_lost;
        info.isk_destroyed = s.isk_destroyed;
        info.isk_lost = s.isk_lost;
        info.danger_ratio = s.danger_ratio;
        info.gang_ratio = s.gang_ratio;
        for list in &s.top_lists {
            if list.kind == "shipType" {
                info.top_ships = list
                    .values
                    .iter()
                    .filter_map(|v| Some((v.ship_type_id?, v.ship_name.clone()?, v.kills)))
                    .take(5)
                    .collect();
            } else if list.kind == "solarSystem" {
                info.top_systems = list
                    .values
                    .iter()
                    .filter_map(|v| Some((v.system_name.clone()?, v.kills)))
                    .take(5)
                    .collect();
            }
        }
    }
    info
}

fn resolve_id(client: &reqwest::blocking::Client, name: &str) -> Option<i64> {
    #[derive(Deserialize)]
    struct Ids {
        characters: Option<Vec<Entity>>,
    }
    #[derive(Deserialize)]
    struct Entity {
        id: i64,
        name: String,
    }
    let v: Ids = client
        .post("https://esi.evetech.net/latest/universe/ids/?datasource=tranquility")
        .json(&[name])
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    v.characters?.into_iter().find(|e| e.name.eq_ignore_ascii_case(name)).map(|e| e.id)
}

fn resolve_names(client: &reqwest::blocking::Client, ids: &[i64]) -> HashMap<i64, String> {
    #[derive(Deserialize)]
    struct Named {
        id: i64,
        name: String,
    }
    if ids.is_empty() {
        return HashMap::new();
    }
    client
        .post("https://esi.evetech.net/latest/universe/names/?datasource=tranquility")
        .json(ids)
        .send()
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .and_then(|r| r.json::<Vec<Named>>().ok())
        .map(|v| v.into_iter().map(|n| (n.id, n.name)).collect())
        .unwrap_or_default()
}
