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

    /// Pre-load the known (persisted) pilot names so they're recognised at once.
    pub fn preload(&mut self, known: &HashMap<String, i64>) {
        for (lc, id) in known {
            self.resolved.entry(lc.clone()).or_insert(Some(*id));
        }
    }

    /// Pre-load persisted non-name verdicts (multi-word bridging spans) so the cover can
    /// skip them at once instead of re-querying after every restart.
    pub fn preload_negatives(&mut self, names: &[String]) {
        for lc in names {
            self.resolved.entry(lc.clone()).or_insert(None);
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
            // A short bare number is a count ("Ace hodgens 30" = pilot + 30 ships), never a
            // name component on its own — skip it (it also never resolves, so waiting on it
            // would block forever).
            if words[i].len() <= 4 && words[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
                continue;
            }
            // Take the longest CONFIRMED character name starting here. WAIT (return empty)
            // if a longer span is still *pending* — otherwise a coincidental shorter name
            // ("Yan" / "Watt", which are also real players) gets grabbed before the real
            // "Yan Fan" / "Watt Watt" resolves, and the reconcile commits that wrong split
            // permanently. A span resolved as a *non-name* (the bridging "Grim Iskander
            // Felmilia") is skipped, so once it has resolved the split isn't blocked — which
            // is why we persist negative verdicts too (see the resolver).
            let mut matched = None;
            for len in (1..=3.min(words.len() - i)).rev() {
                let span = words[i..i + len].join(" ");
                match self.get(&span) {
                    Some(Some(_)) => {
                        matched = Some(len);
                        break;
                    }
                    None => return Vec::new(), // a longer span is still pending — wait
                    Some(None) => {}           // resolved non-name — try a shorter span
                }
            }
            match matched {
                Some(len) => {
                    out.push(words[i..i + len].join(" "));
                    i += len;
                }
                None => {
                    // Every span starting here resolved as a non-name (a *pending* one would
                    // have returned above). It's a typo / intel word glued onto the run
                    // ("Tort Radeon skywook tief", "H3xat0r arazy") — skip it and keep the
                    // confirmed names, instead of discarding the whole run.
                    i += 1;
                }
            }
        }
        // A candidate that splits into ONLY single-word names (2+) is almost always ONE
        // character with a multi-word name ("Andy Shank", "I Forgot Who") whose individual
        // words each happen to be real players — not several glued-together names. Try the
        // multi-word name first: refuse the all-singles split (the whole, ESI-rejected name
        // is dropped instead of exploding into spurious singles). A real glued list always
        // carries at least one multi-word name, which keeps that case splitting.
        if out.len() >= 2 && out.iter().all(|n| !n.contains(' ')) {
            return Vec::new();
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
                if let Some(chars) = &result {
                    let ok = batch.iter().filter(|n| chars.contains_key(&n.to_lowercase())).count();
                    eprintln!("[pilot] resolved {}/{} (queue ~{})", ok, batch.len(), c.queue.len());
                    for name in &batch {
                        // Resolved (or confirmed not-a-character) — free it from the dedup
                        // set and record the outcome.
                        c.queued.remove(&name.to_lowercase());
                        let id = chars.get(&name.to_lowercase()).copied();
                        c.resolved.insert(name.to_lowercase(), id);
                        if let Some(store) = &store {
                            match id {
                                Some(cid) => store.add_known_pilot(name, cid),
                                // Persist only multi-word non-names (the bridging spans the
                                // cover trips on); single junk words aren't worth a row.
                                None if name.contains(' ') => store.add_known_pilot(name, 0),
                                None => {}
                            }
                        }
                    }
                } else {
                    // Request failed (timeout / rate-limit / network). Re-queue the batch
                    // for retry rather than dropping it — the names stay in `queued` (so
                    // intel won't double-add them) but go back on `queue`, so they retry
                    // after the backoff instead of waiting to be mentioned again.
                    eprintln!(
                        "[pilot] batch failed — re-queued {} names for retry (queue ~{})",
                        batch.len(),
                        c.queue.len()
                    );
                    for name in &batch {
                        c.queue.push_back(name.clone());
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

        // "Amryu Alpha" with "Alpha" a confirmed non-name keeps the real character and drops
        // the junk word (ESI says "Amryu Alpha" isn't a character, but "Amryu" is).
        c.resolved.insert("amryu".into(), Some(3));
        c.resolved.insert("amryu alpha".into(), None);
        c.resolved.insert("alpha".into(), None);
        assert_eq!(c.cover("Amryu Alpha"), vec!["Amryu"]);

        // A run still pending a longer span defers (empty) rather than shortening.
        assert!(c.cover("Tea ship").is_empty());
    }

    #[test]
    fn cover_waits_for_a_longer_pending_name() {
        let mut c = PilotCache::default();
        // "Yan" is also a real player, but "Yan Fan" is the real name and isn't resolved
        // yet — the cover must wait, not grab "Yan".
        c.resolved.insert("yan".into(), Some(1));
        assert!(c.cover("Yan Fan Watt").is_empty());
        // Once the real names resolve and the bridging span is a known non-name, split.
        c.resolved.insert("yan fan".into(), Some(2));
        c.resolved.insert("watt".into(), Some(3));
        c.resolved.insert("yan fan watt".into(), None);
        assert_eq!(c.cover("Yan Fan Watt"), vec!["Yan Fan", "Watt"]);
    }

    #[test]
    fn cover_skips_resolved_non_name_bridging_spans() {
        let mut c = PilotCache::default();
        for (n, id) in [
            ("octavia von zeckendorf", 1),
            ("grim iskander", 2),
            ("felmilia berk skjem", 3),
            ("ayaka iida", 4),
            ("ai-0002", 5),
        ] {
            c.resolved.insert(n.into(), Some(id));
        }
        // The 3-word spans bridging two names are resolved as non-names (persisted) — the
        // cover skips them instead of blocking.
        c.resolved.insert("grim iskander felmilia".into(), None);
        c.resolved.insert("ayaka iida ai-0002".into(), None);
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
    fn cover_skips_trailing_count_number() {
        let mut c = PilotCache::default();
        c.resolved.insert("ace hodgens".into(), Some(1));
        c.resolved.insert("ace hodgens 30".into(), None); // resolved as a non-name
        // "30" is a count ("Ace hodgens +30 kikimoras"), not part of the name.
        assert_eq!(c.cover("Ace hodgens 30"), vec!["Ace hodgens"]);
    }

    #[test]
    fn cover_keeps_two_word_name_over_two_single_players() {
        let mut c = PilotCache::default();
        // "Andy" and "Shank" are each real players, but "Andy Shank" is one character. With
        // the pair confirmed it is taken whole.
        c.resolved.insert("andy".into(), Some(1));
        c.resolved.insert("shank".into(), Some(2));
        c.resolved.insert("andy shank".into(), Some(3));
        assert_eq!(c.cover("Andy Shank"), vec!["Andy Shank"]);
        // If ESI marks the pair a non-name (stale/partial), never explode it into the two
        // coincidental singles — prefer the multi-word reading and refuse the split.
        let mut c2 = PilotCache::default();
        c2.resolved.insert("andy".into(), Some(1));
        c2.resolved.insert("shank".into(), Some(2));
        c2.resolved.insert("andy shank".into(), None);
        assert!(c2.cover("Andy Shank").is_empty(), "must not split into two singles");
        // A genuine glued list still splits — it carries a multi-word name as the anchor.
        let mut c3 = PilotCache::default();
        c3.resolved.insert("redhorn mastro".into(), Some(1));
        c3.resolved.insert("falcon".into(), Some(2));
        c3.resolved.insert("redhorn mastro falcon".into(), None);
        c3.resolved.insert("mastro falcon".into(), None);
        assert_eq!(c3.cover("Redhorn Mastro Falcon"), vec!["Redhorn Mastro", "Falcon"]);
    }

    #[test]
    fn cover_refuses_three_single_word_split() {
        let mut c = PilotCache::default();
        // "I Forgot Who" is one character whose individual words each happen to be real
        // players. The whole name resolved as a non-name, but it must NOT explode into
        // three separate single-word pilots.
        c.resolved.insert("i".into(), Some(1));
        c.resolved.insert("forgot".into(), Some(2));
        c.resolved.insert("who".into(), Some(3));
        c.resolved.insert("i forgot".into(), None);
        c.resolved.insert("forgot who".into(), None);
        c.resolved.insert("i forgot who".into(), None);
        assert!(c.cover("I Forgot Who").is_empty());
    }

    #[test]
    fn cover_keeps_real_name_with_trailing_junk() {
        let mut c = PilotCache::default();
        // A real name with trailing typos/intel words glued on by the loose run; the cover
        // keeps the confirmed name and drops the junk once ESI has rejected it.
        c.resolved.insert("tort radeon".into(), Some(1));
        for j in ["tort radeon skywook", "tort radeon skywook tief", "skywook tief", "skywook", "tief"] {
            c.resolved.insert(j.into(), None);
        }
        assert_eq!(c.cover("Tort Radeon skywook tief"), vec!["Tort Radeon"]);

        c.resolved.insert("h3xat0r".into(), Some(2));
        c.resolved.insert("h3xat0r arazy".into(), None);
        c.resolved.insert("arazy".into(), None);
        assert_eq!(c.cover("H3xat0r arazy"), vec!["H3xat0r"]);
    }

    #[test]
    fn cover_splits_standing_color_led_run() {
        let mut c = PilotCache::default();
        // ESI confirms both real names; the glued plain-text run splits cleanly.
        c.resolved.insert("blue randomattac".into(), Some(1));
        c.resolved.insert("redhorn mastro".into(), Some(2));
        c.resolved.insert("blue randomattac redhorn mastro".into(), None);
        c.resolved.insert("blue randomattac redhorn".into(), None);
        assert_eq!(
            c.cover("Blue RandomAttac Redhorn Mastro"),
            vec!["Blue RandomAttac", "Redhorn Mastro"]
        );
    }

    #[test]
    fn windows_one_to_three() {
        // 1-2 char spans are filtered (EVE names are >= 3 chars).
        assert_eq!(name_windows("abc de"), vec!["abc", "abc de"]);
        assert!(name_windows("x").is_empty());
    }
}
