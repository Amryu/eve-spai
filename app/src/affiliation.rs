//! Resolve a character's current corporation + alliance (ESI
//! `POST /characters/affiliation/`, public) so intel pilot badges and the lookup window can
//! show corp/alliance logos. Results are cached; only ids not yet known are fetched.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

#[derive(Clone, Default)]
pub struct Affil {
    pub corp: Option<i64>,
    pub alliance: Option<i64>,
    pub corp_name: Option<String>,
    pub alliance_name: Option<String>,
}

/// Re-resolve a character's affiliation after this many seconds — corp/alliance
/// membership changes over time, so a session-long cache would show stale data.
const AFFIL_TTL: i64 = 3600;

#[derive(Default)]
pub struct AffilCache {
    map: HashMap<i64, Affil>,
    /// Unix seconds each id was last resolved, for TTL refresh.
    fetched_at: HashMap<i64, i64>,
    pending: HashSet<i64>,
}

pub type SharedAffil = Arc<Mutex<AffilCache>>;

impl AffilCache {
    /// Known corp/alliance for a character, if resolved.
    pub fn get(&self, id: i64) -> Option<Affil> {
        self.map.get(&id).cloned()
    }

    /// Ensure `id` gets resolved (queues it if unknown or its cached value is stale).
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

/// Background resolver: batches queued character ids and fills the cache.
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
                        // Resolve corp + alliance ids to names (one /universe/names batch) so the
                        // pilot badge tooltip can show them.
                        let mut ids: Vec<i64> = Vec::new();
                        for a in &list {
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
                                },
                            );
                        }
                        got = true;
                    }
                    Err(_) => {
                        // Re-queue so a transient failure retries next tick.
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
