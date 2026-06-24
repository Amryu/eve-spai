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
        // Bound the backlog (drop the oldest, least-relevant names) so a busy channel
        // can't starve recent names of resolution.
        while self.queue.len() > 4000 {
            if let Some(old) = self.queue.pop_front() {
                self.queued.remove(&old.to_lowercase());
            }
        }
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

    /// Cover a multi-word candidate with confirmed character sub-names, longest match
    /// first, e.g. "Wwallddo Lulu Uanid" → ["Wwallddo", "Lulu Uanid"]. Returns empty
    /// (don't split) unless EVERY word is covered by a confirmed name — so "Amryu Alpha"
    /// (with "Alpha" not a character) is not collapsed to "Amryu" — and defers (empty)
    /// while any longer span is still pending resolution, so the longest name wins.
    pub fn cover(&self, candidate: &str) -> Vec<String> {
        let words: Vec<&str> = candidate.split_whitespace().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < words.len() {
            // Take the longest CONFIRMED character name starting here. A pending or
            // non-name span — e.g. the 3-word "Grim Iskander Felmilia" that bridges two
            // real names — is skipped, not a reason to abort. (Previously ANY unresolved
            // span discarded the whole split, so "Octavia von Zeckendorf" was dropped
            // whenever a bridging span hadn't resolved — and those negative verdicts aren't
            // persisted, so a restart re-triggered it.) Real characters ARE persisted, so
            // the longest confirmed span here is the real name.
            let mut took = 0;
            for len in (1..=3.min(words.len() - i)).rev() {
                let span = words[i..i + len].join(" ");
                if matches!(self.get(&span), Some(Some(_))) {
                    out.push(span);
                    took = len;
                    break;
                }
            }
            if took == 0 {
                // No confirmed name covers this word yet — not a clean split; wait.
                return Vec::new();
            }
            i += took;
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
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(std::time::Duration::from_secs(20))
            .build()
        else {
            return;
        };
        loop {
            // LIFO: resolve the most recently seen names first (current intel matters
            // more than a stale backlog). 200/batch stays under ESI's limit + timeout.
            let batch: Vec<String> = {
                let mut c = cache.lock().unwrap();
                (0..200).map_while(|_| c.queue.pop_back()).collect()
            };
            if batch.is_empty() {
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            let result = resolve_batch(&client, &batch);
            let store = crate::store::Store::open().ok();
            {
                let mut c = cache.lock().unwrap();
                // Free the batch from the dedup set; resolved names are also recorded
                // below, so only unresolved (failed-request) names become re-queueable.
                for name in &batch {
                    c.queued.remove(&name.to_lowercase());
                }
                if let Some(chars) = &result {
                    let ok = batch.iter().filter(|n| chars.contains_key(&n.to_lowercase())).count();
                    eprintln!("[pilot] resolved {}/{} (queue ~{})", ok, batch.len(), c.queue.len());
                    for name in &batch {
                        let id = chars.get(&name.to_lowercase()).copied();
                        c.resolved.insert(name.to_lowercase(), id);
                        if let (Some(cid), Some(store)) = (id, &store) {
                            store.add_known_pilot(name, cid);
                        }
                    }
                }
            }
            match &result {
                // Request failed (timeout/limit/rate) — don't poison the cache; back off.
                None => std::thread::sleep(std::time::Duration::from_secs(3)),
                Some(chars) => {
                    if !chars.is_empty() {
                        // Coalesce: the intel feed only needs ~1fps as names resolve.
                        ctx.request_repaint_after(std::time::Duration::from_millis(1000));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
    });
}

/// Resolve a batch of exact names; returns the character names that matched
/// (lower-cased) -> id.
fn resolve_batch(client: &reqwest::blocking::Client, names: &[String]) -> Option<HashMap<String, i64>> {
    let mut out = HashMap::new();
    let resp: Option<serde_json::Value> = client
        .post(ESI_IDS)
        .json(names)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .ok();
    let Some(v) = resp else {
        eprintln!("[pilot] ESI /universe/ids request FAILED for {} names — left unresolved", names.len());
        return None;
    };
    {
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
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_splits_glued_names() {
        let mut c = PilotCache::default();
        // All sub-spans resolved (Some(id) = character, None = not one).
        c.resolved.insert("wwallddo".into(), Some(1));
        c.resolved.insert("lulu uanid".into(), Some(2));
        c.resolved.insert("wwallddo lulu".into(), None);
        c.resolved.insert("wwallddo lulu uanid".into(), None);
        assert_eq!(c.cover("Wwallddo Lulu Uanid"), vec!["Wwallddo", "Lulu Uanid"]);

        // "Amryu Alpha" (Alpha not a character) must NOT collapse to "Amryu".
        c.resolved.insert("amryu".into(), Some(3));
        c.resolved.insert("amryu alpha".into(), None);
        c.resolved.insert("alpha".into(), None);
        assert!(c.cover("Amryu Alpha").is_empty());

        // A run still pending a longer span defers (empty) rather than shortening.
        assert!(c.cover("Tea ship").is_empty());
    }

    #[test]
    fn cover_skips_unresolved_bridging_spans() {
        let mut c = PilotCache::default();
        // Real names are persisted/confirmed; the 3-word spans that bridge two of them are
        // left unresolved (negative verdicts aren't persisted). They must not block.
        for (n, id) in [
            ("octavia von zeckendorf", 1),
            ("grim iskander", 2),
            ("felmilia berk skjem", 3),
            ("ayaka iida", 4),
            ("ai-0002", 5),
        ] {
            c.resolved.insert(n.into(), Some(id));
        }
        assert_eq!(
            c.cover("Octavia von Zeckendorf Grim Iskander Felmilia Berk Skjem Ayaka Iida ai-0002"),
            vec![
                "Octavia von Zeckendorf",
                "Grim Iskander",
                "Felmilia Berk Skjem",
                "Ayaka Iida",
                "ai-0002",
            ]
        );
    }

    #[test]
    fn windows_one_to_three() {
        // 1-2 char spans are filtered (EVE names are >= 3 chars).
        assert_eq!(name_windows("abc de"), vec!["abc", "abc de"]);
        assert!(name_windows("x").is_empty());
    }
}
