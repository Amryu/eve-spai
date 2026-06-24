//! Killmail enrichment: given a zKillboard kill id, fetch the victim, attackers and
//! value (zKill for the hash + value, ESI for the full killmail). Results are cached
//! and filled on a background thread, like the pilot resolver.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Enriched killmail details for the KILL badge + window.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // some fields are kept for the window / future use
pub struct KillInfo {
    pub kill_id: i64,
    pub hash: Option<String>,
    pub victim_char: Option<i64>,
    pub victim_ship: Option<i64>,
    pub victim_corp: Option<i64>,
    pub victim_alliance: Option<i64>,
    pub system_id: i64,
    pub value: f64,
    pub time: String,
    pub final_blow_char: Option<i64>,
    pub final_blow_ship: Option<i64>,
    /// Attacker alliance ids (most frequent first), for the "dominant side" display.
    pub attacker_alliances: Vec<i64>,
    pub attacker_count: usize,
}

/// kill id → fetched info. A present `None` means pending or failed (don't refetch).
pub type KillCache = Arc<Mutex<HashMap<i64, Option<KillInfo>>>>;
pub type KillSender = Sender<i64>;

/// Spawn the background killmail fetcher. Send a kill id to enqueue enrichment.
pub fn spawn_fetcher(cache: KillCache, ctx: egui::Context) -> KillSender {
    let (tx, rx) = std::sync::mpsc::channel::<i64>();
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent("eve-spai/0.1 (EVE intel tool; +github.com/Amryu/eve-spai)")
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        for id in rx {
            // Mark as in-flight so the UI doesn't keep re-requesting.
            {
                let mut c = cache.lock().unwrap();
                if c.get(&id).is_some_and(|v| v.is_some()) {
                    continue;
                }
                c.entry(id).or_insert(None);
            }
            let info = fetch_kill(&client, id);
            if info.is_some() {
                cache.lock().unwrap().insert(id, info);
                ctx.request_repaint();
            }
            std::thread::sleep(Duration::from_millis(300)); // be gentle on zKill
        }
    });
    tx
}

fn fetch_kill(client: &reqwest::blocking::Client, id: i64) -> Option<KillInfo> {
    #[derive(Deserialize)]
    struct Zkb {
        hash: String,
        #[serde(rename = "totalValue", default)]
        total_value: f64,
    }
    #[derive(Deserialize)]
    struct ZkEntry {
        zkb: Zkb,
    }
    let zurl = format!("https://zkillboard.com/api/killID/{id}/");
    let zk: Vec<ZkEntry> =
        client.get(zurl).send().ok()?.error_for_status().ok()?.json().ok()?;
    let first = zk.into_iter().next()?;
    let (hash, value) = (first.zkb.hash, first.zkb.total_value);

    #[derive(Deserialize)]
    struct Victim {
        character_id: Option<i64>,
        corporation_id: Option<i64>,
        alliance_id: Option<i64>,
        ship_type_id: Option<i64>,
    }
    #[derive(Deserialize)]
    struct Attacker {
        character_id: Option<i64>,
        alliance_id: Option<i64>,
        ship_type_id: Option<i64>,
        #[serde(default)]
        final_blow: bool,
    }
    #[derive(Deserialize)]
    struct Km {
        killmail_time: String,
        solar_system_id: i64,
        victim: Victim,
        attackers: Vec<Attacker>,
    }
    let eurl =
        format!("https://esi.evetech.net/latest/killmails/{id}/{hash}/?datasource=tranquility");
    let km: Km = client.get(eurl).send().ok()?.error_for_status().ok()?.json().ok()?;

    let fb = km.attackers.iter().find(|a| a.final_blow);
    // Attacker alliances, most frequent first.
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for a in &km.attackers {
        if let Some(al) = a.alliance_id {
            *counts.entry(al).or_default() += 1;
        }
    }
    let mut alliances: Vec<(i64, usize)> = counts.into_iter().collect();
    alliances.sort_by(|a, b| b.1.cmp(&a.1));

    Some(KillInfo {
        kill_id: id,
        hash: Some(hash),
        victim_char: km.victim.character_id,
        victim_ship: km.victim.ship_type_id,
        victim_corp: km.victim.corporation_id,
        victim_alliance: km.victim.alliance_id,
        system_id: km.solar_system_id,
        value,
        time: km.killmail_time,
        final_blow_char: fb.and_then(|a| a.character_id),
        final_blow_ship: fb.and_then(|a| a.ship_type_id),
        attacker_count: km.attackers.len(),
        attacker_alliances: alliances.into_iter().map(|(a, _)| a).collect(),
    })
}
