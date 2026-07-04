use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Affil {
    pub corp: Option<i64>,
    pub alliance: Option<i64>,
    pub corp_name: Option<String>,
    pub alliance_name: Option<String>,
    pub char_name: Option<String>,
}

/// Re-resolve a character's affiliation after this many seconds — corp/alliance
/// membership changes over time, so a session-long cache would show stale data.
const AFFIL_TTL: i64 = 3600;

#[derive(Default)]
pub struct AffilCache {
    map: HashMap<i64, Affil>,
    fetched_at: HashMap<i64, i64>,
    pending: HashSet<i64>,
}

pub type SharedAffil = Arc<Mutex<AffilCache>>;

impl AffilCache {
    pub fn get(&self, id: i64) -> Option<Affil> {
        self.map.get(&id).cloned()
    }

    pub fn insert_resolved(&mut self, id: i64, affil: Affil) {
        self.fetched_at.insert(id, chrono::Utc::now().timestamp());
        self.map.insert(id, affil);
    }

    pub fn want(&mut self, id: i64) {
        if id <= 0 {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let fresh = self.fetched_at.get(&id).is_some_and(|&t| now - t < AFFIL_TTL);
        if !fresh {
            self.pending.insert(id);
        }
    }
}

#[derive(Deserialize)]
struct AffilResp {
    character_id: i64,
    corporation_id: i64,
    #[serde(default)]
    alliance_id: Option<i64>,
}

pub fn spawn(cache: SharedAffil, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let batch: Vec<i64> = {
                let mut c = cache.lock().unwrap();
                c.pending.drain().collect()
            };
            if batch.is_empty() {
                continue;
            }
            let mut got = false;
            for chunk in batch.chunks(1000) {
                let resp = client
                    .post("https://esi.evetech.net/latest/characters/affiliation/")
                    .json(chunk)
                    .send()
                    .and_then(|r| r.error_for_status())
                    .and_then(|r| r.json::<Vec<AffilResp>>());
                match resp {
                    Ok(list) => {
                        let mut ids: Vec<i64> = Vec::new();
                        for a in &list {
                            ids.push(a.character_id);
                            ids.push(a.corporation_id);
                            if let Some(al) = a.alliance_id {
                                ids.push(al);
                            }
                        }
                        let names = crate::lookup::resolve_type_names(&ids);
                        let now = chrono::Utc::now().timestamp();
                        let mut c = cache.lock().unwrap();
                        for a in list {
                            c.fetched_at.insert(a.character_id, now);
                            c.map.insert(
                                a.character_id,
                                Affil {
                                    corp: Some(a.corporation_id),
                                    alliance: a.alliance_id,
                                    corp_name: names.get(&a.corporation_id).cloned(),
                                    alliance_name: a.alliance_id.and_then(|al| names.get(&al).cloned()),
                                    char_name: names.get(&a.character_id).cloned(),
                                },
                            );
                        }
                        got = true;
                    }
                    Err(_) => {
                        let mut c = cache.lock().unwrap();
                        for id in chunk {
                            c.pending.insert(*id);
                        }
                    }
                }
            }
            if got {
                ctx.request_repaint();
            }
        }
    });
}
