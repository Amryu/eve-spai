use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Datelike;

const ACTIVITY_TTL: i64 = 4 * 3600;
const BATCH: usize = 20;

#[derive(Clone, Copy, Debug, Default)]
pub struct Activity {
    #[allow(dead_code)]
    pub active_recent: bool,
    pub birthday: Option<i64>,
    pub last_corp_change: Option<i64>,
}

#[derive(Default)]
pub struct ActivityCache {
    map: HashMap<i64, Activity>,
    fetched_at: HashMap<i64, i64>,
    pending: HashSet<i64>,
}

pub type SharedActivity = Arc<Mutex<ActivityCache>>;

impl ActivityCache {
    #[allow(dead_code)]
    pub fn get(&self, char_id: i64) -> Option<Activity> {
        self.map.get(&char_id).copied()
    }

    #[allow(dead_code)]
    pub fn want(&mut self, char_id: i64) {
        if char_id <= 0 {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let fresh = self.fetched_at.get(&char_id).is_some_and(|&t| now - t < ACTIVITY_TTL);
        if !fresh {
            self.pending.insert(char_id);
        }
    }

    pub fn preload(&mut self, rows: Vec<(i64, bool, Option<i64>, Option<i64>, i64)>) {
        for (id, active_recent, birthday, last_corp_change, fetched_at) in rows {
            self.map.insert(id, Activity { active_recent, birthday, last_corp_change });
            self.fetched_at.insert(id, fetched_at);
        }
    }
}

pub fn demote_decision(
    active_recent: bool,
    birthday: Option<i64>,
    now: i64,
    revived: bool,
    last_corp_change: Option<i64>,
) -> bool {
    // Match the `active_recent` lookback (~3 calendar months): a character younger than this
    // hasn't had a full window to be active, so it must not be demoted for inactivity.
    const YOUNG_ACCOUNT_SECS: i64 = 90 * 86400;
    if birthday.is_some_and(|b| now - b < YOUNG_ACCOUNT_SECS) {
        return false;
    }
    if active_recent {
        return false;
    }
    if last_corp_change.is_some_and(|c| now - c < YOUNG_ACCOUNT_SECS) {
        return false;
    }
    if revived {
        return false;
    }
    true
}

fn months_active_recent(months: &serde_json::Value, now: chrono::DateTime<chrono::Utc>) -> bool {
    let now_base = now.year() * 12 + (now.month() as i32 - 1);
    let cutoff = now - chrono::Duration::days(90);
    let cutoff_base = cutoff.year() * 12 + (cutoff.month() as i32 - 1);
    let mut total = now_base;
    while total >= cutoff_base {
        let (yy, mm) = (total / 12, total % 12 + 1);
        let key = format!("{yy:04}{mm:02}");
        if let Some(entry) = months.get(&key) {
            let kills = entry.get("shipsDestroyed").and_then(|v| v.as_i64()).unwrap_or(0);
            let losses = entry.get("shipsLost").and_then(|v| v.as_i64()).unwrap_or(0);
            if kills + losses > 0 {
                return true;
            }
        }
        total -= 1;
    }
    false
}

pub fn spawn(cache: SharedActivity, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "eve-spai/",
                env!("CARGO_PKG_VERSION"),
                " (EVE intel tool; +github.com/Amryu/eve-spai)"
            ))
            .timeout(Duration::from_secs(20))
            .build()
        else {
            return;
        };
        let store = crate::store::Store::open().ok();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let batch: Vec<i64> = {
                let mut c = cache.lock().unwrap();
                let ids: Vec<i64> = c.pending.iter().take(BATCH).copied().collect();
                for id in &ids {
                    c.pending.remove(id);
                }
                ids
            };
            if batch.is_empty() {
                continue;
            }
            let mut got = false;
            for id in batch {
                let now_dt = chrono::Utc::now();
                let resp = client
                    .get(format!("https://zkillboard.com/api/stats/characterID/{id}/"))
                    .send();
                let active_recent = match resp {
                    Err(_) => {
                        cache.lock().unwrap().pending.insert(id);
                        std::thread::sleep(Duration::from_millis(300));
                        continue;
                    }
                    // Got a response (any status). Non-200 / garbage body → not active, but
                    // still set the entry so we don't infinite-retry.
                    Ok(r) => {
                        if r.status().is_success() {
                            r.json::<serde_json::Value>()
                                .ok()
                                .map(|v| {
                                    months_active_recent(
                                        v.get("months").unwrap_or(&serde_json::Value::Null),
                                        now_dt,
                                    )
                                })
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    }
                };

                // Birthday + last corp change: fetch once, reuse any already-known value.
                // (Both change rarely; re-fetching on the normal TTL would also be fine.)
                let (known_birthday, known_corp) = {
                    let c = cache.lock().unwrap();
                    let a = c.map.get(&id);
                    (a.and_then(|a| a.birthday), a.and_then(|a| a.last_corp_change))
                };
                let birthday = known_birthday.or_else(|| fetch_birthday(&client, id));
                let last_corp_change =
                    known_corp.or_else(|| fetch_last_corp_change(&client, id));

                let now = chrono::Utc::now().timestamp();
                {
                    let mut c = cache.lock().unwrap();
                    c.map.insert(id, Activity { active_recent, birthday, last_corp_change });
                    c.fetched_at.insert(id, now);
                }
                if let Some(s) = &store {
                    s.save_pilot_activity(id, active_recent, birthday, last_corp_change, now);
                }
                got = true;
                std::thread::sleep(Duration::from_millis(300)); // be gentle on zKill
            }
            if got {
                ctx.request_repaint();
            }
        }
    });
}

fn fetch_birthday(client: &reqwest::blocking::Client, id: i64) -> Option<i64> {
    #[derive(serde::Deserialize)]
    struct Char {
        birthday: String,
    }
    let c: Char = client
        .get(format!("https://esi.evetech.net/latest/characters/{id}/?datasource=tranquility"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    chrono::DateTime::parse_from_rfc3339(&c.birthday).ok().map(|dt| dt.timestamp())
}

/// NPC corporations (racial starter/school corps and other NPC corps) live in the classic
/// 1,000,000-1,999,999 range; player corps are >= 2,000,000 (incl. the modern 98,xxx,xxx
/// ranges). A move back to an NPC corp can be an automatic kick (disbanded/inactivity
/// boot), not a sign of activity, so those are ignored.
fn is_npc_corp(id: i64) -> bool {
    id < 2_000_000
}

fn fetch_last_corp_change(client: &reqwest::blocking::Client, id: i64) -> Option<i64> {
    let body: serde_json::Value = client
        .get(format!(
            "https://esi.evetech.net/latest/characters/{id}/corporationhistory/?datasource=tranquility"
        ))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    latest_player_corp_change(&body)
}

fn latest_player_corp_change(body: &serde_json::Value) -> Option<i64> {
    let entries = body.as_array()?;
    entries
        .iter()
        .filter(|e| e.get("corporation_id").and_then(|v| v.as_i64()).is_some_and(|c| !is_npc_corp(c)))
        .filter_map(|e| e.get("start_date").and_then(|v| v.as_str()))
        .filter_map(|s| chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.timestamp()))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_2024_06() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2024-06-15T12:00:00Z").unwrap().with_timezone(&chrono::Utc)
    }

    #[test]
    fn active_recent_from_months() {
        let now = now_2024_06();

        let m = serde_json::json!({ "202406": { "shipsDestroyed": 3, "shipsLost": 0 } });
        assert!(months_active_recent(&m, now));

        let m = serde_json::json!({ "202404": { "shipsLost": 1 } });
        assert!(months_active_recent(&m, now));

        let m = serde_json::json!({ "202401": { "shipsDestroyed": 9, "shipsLost": 9 } });
        assert!(!months_active_recent(&m, now));

        let m = serde_json::json!({ "202405": { "shipsDestroyed": 0, "shipsLost": 0 } });
        assert!(!months_active_recent(&m, now));

        assert!(!months_active_recent(&serde_json::json!({}), now));
        assert!(!months_active_recent(&serde_json::Value::Null, now));
    }

    #[test]
    fn demote_decision_matrix() {
        let now = 1_700_000_000;
        let old = Some(now - 120 * 86400);
        let young = Some(now - 3 * 86400);
        let midage = Some(now - 30 * 86400);

        assert!(demote_decision(false, old, now, false, None));
        assert!(!demote_decision(false, young, now, false, None));
        assert!(!demote_decision(false, midage, now, false, None));
        assert!(!demote_decision(true, old, now, false, None));
        assert!(!demote_decision(false, old, now, true, None));
        assert!(demote_decision(false, None, now, false, None));
        assert!(!demote_decision(true, young, now, false, None));

        let recent_corp = Some(now - 10 * 86400);
        assert!(!demote_decision(false, old, now, false, recent_corp));
        let old_corp = Some(now - 200 * 86400);
        assert!(demote_decision(false, old, now, false, old_corp));
    }

    #[test]
    fn npc_corp_classification() {
        assert!(is_npc_corp(1_000_166));
        assert!(is_npc_corp(1_999_999));
        assert!(!is_npc_corp(2_000_000));
        assert!(!is_npc_corp(98_000_001));
    }

    #[test]
    fn latest_corp_change_ignores_npc_kick() {
        let body = serde_json::json!([
            { "record_id": 1, "corporation_id": 98_000_001, "start_date": "2024-05-16T12:00:00Z" },
            { "record_id": 2, "corporation_id": 1_000_166,  "start_date": "2024-06-10T12:00:00Z" },
        ]);
        let joined = chrono::DateTime::parse_from_rfc3339("2024-05-16T12:00:00Z").unwrap().timestamp();
        assert_eq!(latest_player_corp_change(&body), Some(joined));

        let body = serde_json::json!([
            { "corporation_id": 98_000_001, "start_date": "2023-01-01T00:00:00Z" },
            { "corporation_id": 98_000_002, "start_date": "2024-03-03T00:00:00Z" },
            { "corporation_id": 98_000_003, "start_date": "2024-02-02T00:00:00Z" },
        ]);
        let latest = chrono::DateTime::parse_from_rfc3339("2024-03-03T00:00:00Z").unwrap().timestamp();
        assert_eq!(latest_player_corp_change(&body), Some(latest));

        let body = serde_json::json!([
            { "corporation_id": 1_000_166, "start_date": "2024-01-01T00:00:00Z" },
        ]);
        assert_eq!(latest_player_corp_change(&body), None);

        assert_eq!(latest_player_corp_change(&serde_json::json!([])), None);
        assert_eq!(latest_player_corp_change(&serde_json::Value::Null), None);
    }

    #[test]
    fn month_window_crosses_year_boundary() {
        let now = chrono::DateTime::parse_from_rfc3339("2024-02-10T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let m = serde_json::json!({ "202312": { "shipsDestroyed": 1 } });
        assert!(months_active_recent(&m, now));
        let m = serde_json::json!({ "202311": { "shipsDestroyed": 1 } });
        assert!(months_active_recent(&m, now));
        let m = serde_json::json!({ "202310": { "shipsDestroyed": 1 } });
        assert!(!months_active_recent(&m, now));
    }
}
