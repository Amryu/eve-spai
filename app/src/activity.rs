//! Per-character zKillboard-activity + account-age cache (Phase 1 data layer).
//!
//! For each character id we record whether they have any zKill activity (kills or
//! losses) in the last three calendar months (`active_recent`) and their account
//! birthday (unix seconds). Results are cached with a 4h TTL and persisted, so a
//! restart doesn't re-storm zKill.
//!
//! Note: `active_recent` is re-fetched every `ACTIVITY_TTL` (4h); the birthday is
//! immutable and the most-recent corporation change (`last_corp_change`) changes rarely,
//! so both are fetched ONCE and then kept across refreshes.
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
    /// Most recent corporation change (unix seconds), or None if unknown. A recent move
    /// means the character is active, so it must not be demoted for lack of kills.
    pub last_corp_change: Option<i64>,
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

    /// Load persisted rows (char_id, active_recent, birthday, last_corp_change, fetched_at)
    /// on startup.
    pub fn preload(&mut self, rows: Vec<(i64, bool, Option<i64>, Option<i64>, i64)>) {
        for (id, active_recent, birthday, last_corp_change, fetched_at) in rows {
            self.map.insert(id, Activity { active_recent, birthday, last_corp_change });
            self.fetched_at.insert(id, fetched_at);
        }
    }
}

/// Phase 2 demotion decision for one confirmed pilot. Returns `true` = DEMOTE for inactivity.
///
/// Order of precedence (each rule KEEPs, i.e. returns `false`):
/// 1. Young-account exemption: a known birthday younger than the activity window (~3 months) → KEEP.
///    Inactivity is judged over the last 3 calendar months, but a character can't have accrued 3
///    months of kills/losses if it hasn't existed that long, so absence of recent activity doesn't
///    mean it's stale. (A 14-day grace was too short: a real new pilot 15-90 days old that just
///    hasn't fought yet was being falsely demoted.)
/// 2. Recent zKill activity (`active_recent`) → KEEP.
/// 3. Recent corporation change (`last_corp_change` within the activity window) → KEEP: a
///    character that just moved corp is active, even without recent kills.
/// 4. Multi-system revival (`revived`, from the sightings index) supersedes inactivity → KEEP.
/// 5. Otherwise → DEMOTE.
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
        return false; // too young to have a full activity window
    }
    if active_recent {
        return false; // recent kills/losses
    }
    if last_corp_change.is_some_and(|c| now - c < YOUNG_ACCOUNT_SECS) {
        return false; // moved corp recently — still active
    }
    if revived {
        return false; // roaming widely right now — revival supersedes inactivity
    }
    true
}

/// `active_recent` from a zKill stats `months` object: true iff ANY calendar month overlapping the
/// last 90 days has `shipsDestroyed + shipsLost > 0`. Uses month buckets (zKill's granularity) but
/// spans the full 90-day window - i.e. every month from the month of `now-90d` up to `now` - so a
/// pilot who last fought 61-90 days ago (in a 4th calendar month) is still counted active. Slightly
/// lenient at the boundary (counts the whole cutoff month), which errs toward keeping a pilot rather
/// than demoting them. Tolerant of a missing/garbage `months` value.
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

/// NPC corporations (racial starter/school corps and other NPC corps) live in the classic
/// 1,000,000-1,999,999 range; player corps are >= 2,000,000 (incl. the modern 98,xxx,xxx
/// ranges). A move back to an NPC corp can be an automatic kick (disbanded/inactivity
/// boot), not a sign of activity, so those are ignored.
fn is_npc_corp(id: i64) -> bool {
    id < 2_000_000
}

/// ESI corporation history → unix seconds of the most recent change INTO a player corp, or
/// None when the character has only ever been in NPC corps (or on any parse failure).
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

/// Pure helper: from a corporationhistory JSON array of `{corporation_id, start_date}`,
/// return the LATEST `start_date` (unix seconds) among entries joining a PLAYER corp.
/// NPC-corp entries are ignored. Tolerates a missing/garbage/empty body (=> None).
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
    fn demote_decision_matrix() {
        let now = 1_700_000_000; // arbitrary "now"
        let old = Some(now - 120 * 86400); // 120-day-old account (past the 90-day grace)
        let young = Some(now - 3 * 86400); // 3-day-old account
        let midage = Some(now - 30 * 86400); // 30 days: past 14d but within the 90d grace

        // Inactive + not revived + old account, no corp move → DEMOTE.
        assert!(demote_decision(false, old, now, false, None));
        // Young account is exempt even when inactive + not revived → KEEP.
        assert!(!demote_decision(false, young, now, false, None));
        // A 30-day account can't have 3 months of activity, so it is exempt → KEEP (this is the
        // Agent-Benson case the 14-day grace was wrongly demoting).
        assert!(!demote_decision(false, midage, now, false, None));
        // Recent zKill activity → KEEP (even an old account).
        assert!(!demote_decision(true, old, now, false, None));
        // Inactive but revived by multi-system roaming → KEEP.
        assert!(!demote_decision(false, old, now, true, None));
        // Unknown birthday, inactive, not revived → DEMOTE (no exemption to apply).
        assert!(demote_decision(false, None, now, false, None));
        // A young account that is ALSO active stays kept (exemption hit first; same result).
        assert!(!demote_decision(true, young, now, false, None));

        // Old, inactive, not revived, but changed PLAYER corp within 90 days → KEEP.
        let recent_corp = Some(now - 10 * 86400);
        assert!(!demote_decision(false, old, now, false, recent_corp));
        // Old, inactive, not revived, corp change older than 90 days → DEMOTE.
        let old_corp = Some(now - 200 * 86400);
        assert!(demote_decision(false, old, now, false, old_corp));
    }

    #[test]
    fn npc_corp_classification() {
        assert!(is_npc_corp(1_000_166)); // a racial starter corp
        assert!(is_npc_corp(1_999_999)); // top of the NPC range
        assert!(!is_npc_corp(2_000_000)); // first player-corp id
        assert!(!is_npc_corp(98_000_001)); // a modern player corp
    }

    #[test]
    fn latest_corp_change_ignores_npc_kick() {
        // Joined a player corp 30d ago, then kicked to a starter NPC corp 5d ago. The most
        // recent PLAYER-corp change is the 30-day-ago join, NOT the NPC kick.
        let body = serde_json::json!([
            { "record_id": 1, "corporation_id": 98_000_001, "start_date": "2024-05-16T12:00:00Z" },
            { "record_id": 2, "corporation_id": 1_000_166,  "start_date": "2024-06-10T12:00:00Z" },
        ]);
        let joined = chrono::DateTime::parse_from_rfc3339("2024-05-16T12:00:00Z").unwrap().timestamp();
        assert_eq!(latest_player_corp_change(&body), Some(joined));

        // Latest among multiple player-corp entries wins.
        let body = serde_json::json!([
            { "corporation_id": 98_000_001, "start_date": "2023-01-01T00:00:00Z" },
            { "corporation_id": 98_000_002, "start_date": "2024-03-03T00:00:00Z" },
            { "corporation_id": 98_000_003, "start_date": "2024-02-02T00:00:00Z" },
        ]);
        let latest = chrono::DateTime::parse_from_rfc3339("2024-03-03T00:00:00Z").unwrap().timestamp();
        assert_eq!(latest_player_corp_change(&body), Some(latest));

        // Only ever in NPC corps → None.
        let body = serde_json::json!([
            { "corporation_id": 1_000_166, "start_date": "2024-01-01T00:00:00Z" },
        ]);
        assert_eq!(latest_player_corp_change(&body), None);

        // Missing / garbage / empty → None.
        assert_eq!(latest_player_corp_change(&serde_json::json!([])), None);
        assert_eq!(latest_player_corp_change(&serde_json::Value::Null), None);
    }

    #[test]
    fn month_window_crosses_year_boundary() {
        // 2024-02-10 minus 90 days is 2023-11-12, so the window spans 202311..=202402.
        let now = chrono::DateTime::parse_from_rfc3339("2024-02-10T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let m = serde_json::json!({ "202312": { "shipsDestroyed": 1 } });
        assert!(months_active_recent(&m, now));
        // November 2023 is now inside the 90-day window (cutoff month).
        let m = serde_json::json!({ "202311": { "shipsDestroyed": 1 } });
        assert!(months_active_recent(&m, now));
        // October 2023 is before the cutoff month → out of window.
        let m = serde_json::json!({ "202310": { "shipsDestroyed": 1 } });
        assert!(!months_active_recent(&m, now));
    }
}
