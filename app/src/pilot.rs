//! Pilot-name resolution (docs/DESIGN.md §7.1 E3 — named characters).
//!
//! The intel parser proposes candidate names (Title-Case word runs). We confirm
//! which are real characters by batch-resolving them against ESI `/universe/ids/`
//! (exact-name match) on a background thread, caching the verdict so each name is
//! resolved at most once.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

const ESI_IDS: &str = "https://esi.evetech.net/latest/universe/ids/";

#[derive(Default)]
pub struct PilotCache {
    /// name_lower -> Some(character_id) if a character, None if confirmed not one.
    resolved: HashMap<String, Option<i64>>,
    queued: std::collections::HashSet<String>,
    queue: VecDeque<String>,
}

impl PilotCache {
    /// Verdict for a name: `Some(Some(id))` = character, `Some(None)` = not a
    /// character, `None` = not resolved yet.
    pub fn get(&self, name: &str) -> Option<Option<i64>> {
        self.resolved.get(&name.to_lowercase()).copied()
    }

    /// Queue a candidate name for resolution if we haven't seen it.
    pub fn queue(&mut self, name: &str) {
        let lw = name.to_lowercase();
        if self.resolved.contains_key(&lw) || self.queued.contains(&lw) {
            return;
        }
        self.queued.insert(lw);
        self.queue.push_back(name.to_owned());
    }

    /// Mark a name as a confirmed character (e.g. from an in-game showinfo link that
    /// carries the character id) — resolved immediately, no ESI round-trip.
    pub fn confirm(&mut self, name: &str, id: i64) {
        let lw = name.to_lowercase();
        self.resolved.insert(lw.clone(), Some(id));
        self.queued.remove(&lw);
    }

    /// Pre-load the known (persisted) pilot names so they're recognised at once.
    pub fn preload(&mut self, known: &HashMap<String, i64>) {
        for (lc, id) in known {
            self.resolved.entry(lc.clone()).or_insert(Some(*id));
        }
    }

    /// Snapshot of confirmed names (lower-cased) → character id, for the parser.
    pub fn confirmed(&self) -> HashMap<String, i64> {
        self.resolved.iter().filter_map(|(n, v)| v.map(|id| (n.clone(), id))).collect()
    }

    /// Greedily cover a multi-word candidate with confirmed character sub-names
    /// (longest match first), e.g. "Wwallddo Lulu Uanid" → ["Wwallddo", "Lulu Uanid"].
    /// Unmatched words are skipped. Empty if nothing in it is a known character.
    pub fn cover(&self, candidate: &str) -> Vec<String> {
        let words: Vec<&str> = candidate.split_whitespace().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let mut step = 1;
            for len in (1..=3.min(words.len() - i)).rev() {
                let span = words[i..i + len].join(" ");
                if matches!(self.get(&span), Some(Some(_))) {
                    out.push(span);
                    step = len;
                    break;
                }
            }
            i += step;
        }
        out
    }
}

/// 1–3 word sub-spans of a candidate, so the resolver can confirm the real names
/// inside an over-glued run (EVE names are 1–3 words).
pub fn name_windows(candidate: &str) -> Vec<String> {
    let words: Vec<&str> = candidate.split_whitespace().collect();
    let mut out = Vec::new();
    for len in 1..=3 {
        if words.len() < len {
            break;
        }
        for start in 0..=words.len() - len {
            let span = words[start..start + len].join(" ");
            // EVE character names are at least 3 characters; shorter spans can't be one.
            if span.chars().count() >= 3 {
                out.push(span);
            }
        }
    }
    out
}

pub type SharedPilots = Arc<Mutex<PilotCache>>;

/// Background resolver: drains queued names, batch-resolves via ESI, caches.
pub fn spawn_resolver(cache: SharedPilots, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent("eve-spai/0.1 (EVE intel tool)")
            .timeout(std::time::Duration::from_secs(20))
            .build()
        else {
            return;
        };
        loop {
            let batch: Vec<String> = {
                let mut c = cache.lock().unwrap();
                (0..100).map_while(|_| c.queue.pop_front()).collect()
            };
            if batch.is_empty() {
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            let chars = resolve_batch(&client, &batch);
            let resolved: Vec<&String> = batch.iter().filter(|n| chars.contains_key(&n.to_lowercase())).collect();
            let missed: Vec<&String> = batch.iter().filter(|n| !chars.contains_key(&n.to_lowercase())).collect();
            eprintln!(
                "[pilot] resolved {}/{}: ok={:?} not-a-char={:?}",
                resolved.len(),
                batch.len(),
                resolved,
                missed
            );
            let store = crate::store::Store::open().ok();
            {
                let mut c = cache.lock().unwrap();
                for name in &batch {
                    let id = chars.get(&name.to_lowercase()).copied();
                    c.resolved.insert(name.to_lowercase(), id);
                    // Persist confirmed names so they're recognised instantly later.
                    if let (Some(cid), Some(store)) = (id, &store) {
                        store.add_known_pilot(name, cid);
                    }
                }
            }
            ctx.request_repaint();
            // Gentle on ESI between batches.
            std::thread::sleep(std::time::Duration::from_millis(800));
        }
    });
}

/// Resolve a batch of exact names; returns the character names that matched
/// (lower-cased) -> id.
fn resolve_batch(client: &reqwest::blocking::Client, names: &[String]) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    let resp: Option<serde_json::Value> = client
        .post(ESI_IDS)
        .json(names)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .ok();
    if resp.is_none() {
        eprintln!("[pilot] ESI /universe/ids request FAILED for {} names (network/rate-limit) — they stay unresolved", names.len());
    }
    if let Some(v) = resp {
        if let Some(chars) = v.get("characters").and_then(|c| c.as_array()) {
            for c in chars {
                if let (Some(id), Some(name)) =
                    (c.get("id").and_then(|i| i.as_i64()), c.get("name").and_then(|n| n.as_str()))
                {
                    out.insert(name.to_lowercase(), id);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_splits_glued_names() {
        let mut c = PilotCache::default();
        c.resolved.insert("wwallddo".into(), Some(1));
        c.resolved.insert("lulu uanid".into(), Some(2));
        c.resolved.insert("wwallddo lulu uanid".into(), None);
        assert_eq!(c.cover("Wwallddo Lulu Uanid"), vec!["Wwallddo", "Lulu Uanid"]);
        // A run with no known character covers to nothing.
        assert!(c.cover("Tea ship").is_empty());
    }

    #[test]
    fn windows_one_to_three() {
        // 1-2 char spans are filtered (EVE names are >= 3 chars).
        assert_eq!(name_windows("abc de"), vec!["abc", "abc de"]);
        assert!(name_windows("x").is_empty());
    }
}
