//! Per-character zKillboard-activity + account-age cache (Phase 1 data layer).
//!
//! For each character id we record whether they have any zKill activity (kills or
//! losses) in the last three calendar months (`active_recent`) and their account
//! birthday (unix seconds). Results are cached with a 4h TTL and persisted, so a
//! restart doesn't re-storm zKill.
//!
//! Note: `active_recent` is re-fetched every `ACTIVITY_TTL` (4h); the birthday is
//! immutable, so it is fetched ONCE and then kept across refreshes.
//!
//! Phase 2 will consume this (e.g. to demote stale/young pilots); Phase 1 only
//! gathers + persists it and changes no displayed intel.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Datelike;

/// Re-fetch a character's `active_recent` after this many seconds.
const ACTIVITY_TTL: i64 = 4 * 3600;
/// Max ids fetched per poll tick (zKill stats is one HTTP call per char — keep small).
const BATCH: usize = 20;

#[derive(Clone, Copy, Debug, Default)]
pub struct Activity {
    /// Any zKill kill or loss in the last three calendar months.
    #[allow(dead_code)] // read in Phase 2; Phase 1 only computes + persists it
    pub active_recent: bool,
    /// Account creation time (unix seconds), once resolved. Immutable.
    pub birthday: Option<i64>,
}

#[derive(Default)]
pub struct ActivityCache {
    map: HashMap<i64, Activity>,
    /// Unix seconds each id was last fetched, for TTL refresh.
    fetched_at: HashMap<i64, i64>,
    pending: HashSet<i64>,
}

pub type SharedActivity = Arc<Mutex<ActivityCache>>;

impl ActivityCache {
    /// Cached activity for a character, if fetched. (Used in Phase 2.)
    #[allow(dead_code)] // used in Phase 2
    pub fn get(&self, char_id: i64) -> Option<Activity> {
        self.map.get(&char_id).copied()
    }

    /// Queue a character for (re)fetch when unknown or its cached value is stale.
    /// (Used in Phase 2 — not yet called from reconcile.)
    #[allow(dead_code)] // used in Phase 2
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

    /// Load persisted rows (char_id, active_recent, birthday, fetched_at) on startup.
    pub fn preload(&mut self, rows: Vec<(i64, bool, Option<i64>, i64)>) {
        for (id, active_recent, birthday, fetched_at) in rows {
            self.map.insert(id, Activity { active_recent, birthday });
            self.fetched_at.insert(id, fetched_at);
        }
    }
}

/// `active_recent` from a zKill stats `months` object: true iff ANY month key within the
/// last three calendar months (current month + the two prior, computed from `now`) has
/// `shipsDestroyed + shipsLost > 0`. Tolerant of a missing/garbage `months` value.
fn months_active_recent(months: &serde_json::Value, now: chrono::DateTime<chrono::Utc>) -> bool {
    // The three "YYYYMM" keys to inspect (current month and the two before it).
    let base = now.year() * 12 + (now.month() as i32 - 1);
    for back in 0..3 {
        let total = base - back;
        let (yy, mm) = (total / 12, total % 12 + 1);
        let key = format!("{yy:04}{mm:02}");
        if let Some(entry) = months.get(&key) {
            let kills = entry.get("shipsDestroyed").and_then(|v| v.as_i64()).unwrap_or(0);
            let losses = entry.get("shipsLost").and_then(|v| v.as_i64()).unwrap_or(0);
            if kills + losses > 0 {
                return true;
            }
        }
    }
    false
}

/// Background fetcher: drains queued character ids, fills the cache + persists it.
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
        // Own DB connection (Store isn't Sync) — same pattern as the other fetchers.
        let store = crate::store::Store::open().ok();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            // Take a small batch off the queue.
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
                // zKill stats → active_recent.
                let now_dt = chrono::Utc::now();
                let resp = client
                    .get(format!("https://zkillboard.com/api/stats/characterID/{id}/"))
                    .send();
                let active_recent = match resp {
                    // Transient network error: re-queue, don't cache.
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

                // Birthday: fetch once (immutable). Keep any already-known value.
                let known_birthday = cache.lock().unwrap().map.get(&id).and_then(|a| a.birthday);
                let birthday = known_birthday.or_else(|| fetch_birthday(&client, id));

                let now = chrono::Utc::now().timestamp();
                {
                    let mut c = cache.lock().unwrap();
                    c.map.insert(id, Activity { active_recent, birthday });
                    c.fetched_at.insert(id, now);
                }
                if let Some(s) = &store {
                    s.save_pilot_activity(id, active_recent, birthday, now);
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

/// ESI account birthday → unix seconds, or None on any failure.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn now_2024_06() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2024-06-15T12:00:00Z").unwrap().with_timezone(&chrono::Utc)
    }

    #[test]
    fn active_recent_from_months() {
        let now = now_2024_06(); // window: 202404, 202405, 202406

        // Kill in the current month → active.
        let m = serde_json::json!({ "202406": { "shipsDestroyed": 3, "shipsLost": 0 } });
        assert!(months_active_recent(&m, now));

        // Loss in a month inside the window → active.
        let m = serde_json::json!({ "202404": { "shipsLost": 1 } });
        assert!(months_active_recent(&m, now));

        // Activity only in an older month (outside the 3-month window) → not active.
        let m = serde_json::json!({ "202401": { "shipsDestroyed": 9, "shipsLost": 9 } });
        assert!(!months_active_recent(&m, now));

        // In-window month present but zero activity → not active.
        let m = serde_json::json!({ "202405": { "shipsDestroyed": 0, "shipsLost": 0 } });
        assert!(!months_active_recent(&m, now));

        // Empty / missing months → not active.
        assert!(!months_active_recent(&serde_json::json!({}), now));
        assert!(!months_active_recent(&serde_json::Value::Null, now));
    }

    #[test]
    fn month_window_crosses_year_boundary() {
        // February 2024 → window is 202312, 202401, 202402.
        let now = chrono::DateTime::parse_from_rfc3339("2024-02-10T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let m = serde_json::json!({ "202312": { "shipsDestroyed": 1 } });
        assert!(months_active_recent(&m, now));
        let m = serde_json::json!({ "202311": { "shipsDestroyed": 1 } });
        assert!(!months_active_recent(&m, now));
    }
}
