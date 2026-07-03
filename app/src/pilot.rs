//! Pilot-name resolution (docs/DESIGN.md §7.1 E3 — named characters).
//!
//! The intel parser proposes candidate names (Title-Case word runs). We confirm
//! which are real characters by batch-resolving them against ESI `/universe/ids/`
//! (exact-name match) on a background thread, caching the verdict so each name is
//! resolved at most once.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

const ESI_IDS: &str = "https://esi.evetech.net/latest/universe/ids/";

/// A "not a character" verdict is cached only this long, then re-queried — ESI can miss a
/// brand-new character or transiently drop a name, and a permanent negative made real names
/// (e.g. "River Pixies") vanish forever.
const NEG_TTL: std::time::Duration = std::time::Duration::from_secs(4 * 3600);

#[derive(Default)]
pub struct PilotCache {
    /// name_lower -> Some(character_id) if a character, None if confirmed not one.
    resolved: HashMap<String, Option<i64>>,
    /// When each negative verdict was recorded, for the NEG_TTL re-check. A negative without
    /// a timestamp (test fixture / preloaded) never expires.
    neg_at: HashMap<String, std::time::Instant>,
    /// Names whose "not a character" verdict was re-confirmed by a SECOND ESI lookup — a
    /// stale-free negative. A two-word block is only split into its two single-word players once
    /// its pair is in here, so a real two-word name with a transient negative isn't torn apart.
    reverified: std::collections::HashSet<String>,
    queued: std::collections::HashSet<String>,
    queue: VecDeque<String>,
    /// Confirmed characters that FAILED all activity checks (name-lower) — Phase 2. NOT hidden:
    /// they are still shown, with an uncertainty "?" so the user can classify them. Re-derived
    /// every demotion cycle.
    activity_flagged: std::collections::HashSet<String>,
    /// The user's manual verdicts (name-lower -> hidden): `true` = "not a pilot" (hide it and free
    /// its tokens), `false` = "real" (always show, clears the uncertainty). Persisted.
    user_verdicts: HashMap<String, bool>,
}

/// Whether a candidate could be a real EVE character name: 1 to 3 words and 3 to 37 chars.
/// Over-glued parser runs (4+ words) and over-long blobs can never be a character, so they are
/// never queried against ESI and never left pending on the "..." animation.
fn plausible_character_name(name: &str) -> bool {
    let t = name.trim();
    let len = t.chars().count();
    if !(3..=37).contains(&len) {
        return false;
    }
    (1..=3).contains(&t.split_whitespace().count())
}

impl PilotCache {
    /// Verdict for a name: `Some(Some(id))` = character, `Some(None)` = not a
    /// character, `None` = not resolved yet.
    pub fn get(&self, name: &str) -> Option<Option<i64>> {
        self.resolved.get(&name.to_lowercase()).copied()
    }

    /// Ids to show as resolved pilots on a card: ESI-confirmed AND not user-hidden (activity-flagged
    /// pilots are still shown — the UI marks them with a "?"). Also re-queues any still-pending name
    /// so a visible card keeps resolving (fixes stuck "...").
    pub fn display_ids<'a>(&mut self, names: impl Iterator<Item = &'a str>) -> std::collections::HashMap<String, i64> {
        let mut out = std::collections::HashMap::new();
        for name in names {
            let lw = name.to_lowercase();
            match self.resolved.get(&lw).copied() {
                Some(Some(id)) if self.user_verdicts.get(&lw).copied() != Some(true) => {
                    out.insert(name.to_string(), id);
                }
                Some(Some(_)) => {}       // user-hidden: hide
                Some(None) => {}          // ESI says not a character
                None => self.queue(name), // pending: keep it resolving
            }
        }
        out
    }

    /// Queue a candidate name for resolution if we haven't seen it.
    pub fn queue(&mut self, name: &str) {
        let lw = name.to_lowercase();
        if self.resolved.contains_key(&lw) || self.queued.contains(&lw) {
            return;
        }
        // EVE character names are at most 3 words and 37 chars. An over-glued parser run
        // ("Le Van Duc Nguyen Van Minh ...") can never be a character, so record it as a PERMANENT
        // negative instead of hammering ESI and leaving it stuck on the "..." animation; the parser
        // then covers/splits it into its plausible sub-names. This also stops the junk flood that
        // was starving real short names of resolution.
        if !plausible_character_name(name) {
            self.resolved.insert(lw, None); // no neg_at entry => never expires (permanently not a name)
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

    /// Whether a name's "not a character" verdict has been re-confirmed a second time
    /// (a stale-free negative). Used by [`cover`] to decide it's safe to split a two-word block.
    pub fn is_reverified(&self, name: &str) -> bool {
        self.reverified.contains(&name.to_lowercase())
    }

    /// Re-queue a name for a FRESH ESI lookup even though it already resolved — used to confirm a
    /// "not a character" verdict isn't stale before acting on it (splitting a two-word block).
    pub fn force_requeue(&mut self, name: &str) {
        let lw = name.to_lowercase();
        if self.reverified.contains(&lw) || self.queued.contains(&lw) {
            return; // already re-confirmed, or a re-check is already pending
        }
        self.queued.insert(lw);
        self.queue.push_back(name.to_owned());
    }

    /// Pre-load the known (persisted) pilot names so they're recognised at once. Skips any
    /// poisoned over-length entry that a bad earlier resolution may have persisted.
    pub fn preload(&mut self, known: &HashMap<String, i64>) {
        for (lc, id) in known {
            if !plausible_character_name(lc) {
                continue;
            }
            self.resolved.entry(lc.clone()).or_insert(Some(*id));
        }
    }

    /// Seed non-name verdicts (used by tests to simulate the resolver). Production negatives
    /// live in-memory with a TTL and aren't preloaded.
    #[allow(dead_code)]
    pub fn preload_negatives(&mut self, names: &[String]) {
        for lc in names {
            self.resolved.entry(lc.clone()).or_insert(None);
        }
    }

    /// Drop negative verdicts older than [`NEG_TTL`] so they are re-queried instead of being
    /// cached as "not a name" forever. Called periodically by the resolver.
    pub fn expire_negatives(&mut self) {
        let stale: Vec<String> =
            self.neg_at.iter().filter(|(_, t)| t.elapsed() > NEG_TTL).map(|(n, _)| n.clone()).collect();
        for n in stale {
            self.neg_at.remove(&n);
            self.reverified.remove(&n); // re-verify from scratch after the TTL
            if matches!(self.resolved.get(&n), Some(None)) {
                self.resolved.remove(&n); // a later positive must stick, so only forget negatives
            }
        }
    }

    /// Snapshot of confirmed names (lower-cased) → character id, for the parser. EXCLUDES only
    /// USER-HIDDEN names (the user marked them "not a pilot") so the parser frees their tokens;
    /// activity-flagged-but-undecided names are still real pilots to the parser.
    pub fn confirmed(&self) -> HashMap<String, i64> {
        self.resolved
            .iter()
            .filter_map(|(n, v)| v.map(|id| (n.clone(), id)))
            .filter(|(n, _)| self.user_verdicts.get(n).copied() != Some(true))
            .collect()
    }

    /// Every ESI-confirmed name → id, INCLUDING currently-demoted ones. Retained as part of the
    /// pilot-cache API; the Phase 2 demotion pass now evaluates only the feed-present pilots
    /// (see `watcher::demote_pass`) rather than the whole confirmed set.
    #[allow(dead_code)]
    pub fn all_confirmed(&self) -> HashMap<String, i64> {
        self.resolved.iter().filter_map(|(n, v)| v.map(|id| (n.clone(), id))).collect()
    }

    /// Replace the activity-flagged set (Phase 2): confirmed pilots that failed all activity checks.
    /// Re-derived every evaluation cycle. These are shown with a "?" — not hidden.
    pub fn set_activity_flagged(&mut self, names: std::collections::HashSet<String>) {
        self.activity_flagged = names;
    }

    /// The current activity-flagged set (for the demotion pass to carry forward between cycles).
    pub fn flagged(&self) -> std::collections::HashSet<String> {
        self.activity_flagged.clone()
    }

    /// Whether the user marked this name "not a pilot" (hide it + free its tokens).
    pub fn is_hidden(&self, name: &str) -> bool {
        self.user_verdicts.get(&name.to_lowercase()).copied() == Some(true)
    }

    /// Whether this confirmed pilot is UNCERTAIN: it failed the activity checks and the user hasn't
    /// classified it yet → shown with a "?" so the user can decide.
    pub fn is_uncertain(&self, name: &str) -> bool {
        let lw = name.to_lowercase();
        self.activity_flagged.contains(&lw) && !self.user_verdicts.contains_key(&lw)
    }

    /// Record a user verdict (`true` = not-a-pilot/hide, `false` = real). The caller persists it.
    pub fn set_verdict(&mut self, name: &str, hidden: bool) {
        self.user_verdicts.insert(name.to_lowercase(), hidden);
    }

    /// Preload persisted user verdicts at startup.
    pub fn preload_verdicts(&mut self, verdicts: impl IntoIterator<Item = (String, bool)>) {
        for (n, h) in verdicts {
            self.user_verdicts.insert(n.to_lowercase(), h);
        }
    }

    /// USER-HIDDEN names (lower-cased) — fed to the parser as `denied` so a name the user marked
    /// "not a pilot" frees its tokens for keyword/ship/other-pilot detection.
    pub fn denied(&self) -> std::collections::HashSet<String> {
        self.user_verdicts.iter().filter(|(_, &h)| h).map(|(n, _)| n.clone()).collect()
    }

    /// Cover a multi-word candidate with confirmed character sub-names, longest match
    /// first, e.g. "Wwallddo Lulu Uanid" → ["Wwallddo", "Lulu Uanid"]. Returns empty
    /// (don't split) unless EVERY word is covered by a confirmed name — so "Amryu Alpha"
    /// (with "Alpha" not a character) is not collapsed to "Amryu" — and defers (empty)
    /// while any longer span is still pending resolution, so the longest name wins.
    pub fn cover(&self, candidate: &str) -> Vec<String> {
        let words: Vec<&str> = candidate.split_whitespace().collect();
        let mut claims: Vec<(usize, usize)> = Vec::new(); // (start word, length) of each claim
        let mut i = 0;
        while i < words.len() {
            // A short bare number is a count ("Ace hodgens 30" = pilot + 30 ships), never a
            // name component on its own — skip it (it also never resolves, so waiting on it
            // would block forever).
            if words[i].len() <= 4 && words[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
                continue;
            }
            // Take the longest CONFIRMED character name starting here — always try 3, then 2,
            // then 1 word. WAIT (return empty) if a longer span is still *pending* — otherwise a
            // coincidental shorter name ("Yan" / "Watt", which are also real players) gets grabbed
            // before the real "Yan Fan" / "Watt Watt" resolves. A span resolved as a *non-name*
            // (the bridging "Grim Iskander Felmilia") is skipped to try a shorter span.
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
                    claims.push((i, len));
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
        // A CONTIGUOUS run of EXACTLY two single-word claims is almost always ONE two-word
        // character ("Zantor Thes", "Andy Shank", "Ghost Magician") that ESI hasn't confirmed as a
        // whole yet — its words just happen to also be real players — so drop it (kept as the
        // pending blob, re-queried) rather than exploding into two spurious singles. A LONGER run
        // (3+) is, once ESI has rejected the whole name, a genuinely mis-joined list of handles
        // ("Gliar Mliarvis Sliarhia"), so surface each. A lone single or a multi-word claim stands.
        let mut out = Vec::new();
        let mut k = 0;
        while k < claims.len() {
            let mut j = k;
            while j + 1 < claims.len()
                && claims[j].1 == 1
                && claims[j + 1].1 == 1
                && claims[j].0 + claims[j].1 == claims[j + 1].0
            {
                j += 1; // extend a contiguous single-word run
            }
            if claims[k].1 == 1 && j == k + 1 {
                // Exactly two adjacent singles: almost always ONE two-word name whose pair ESI
                // hasn't confirmed — keep it whole (drop, re-queried) UNLESS the pair's negative
                // has been re-confirmed (stale-free), in which case it's genuinely two players in a
                // mangled block, so surface both.
                let pair = format!("{} {}", words[claims[k].0], words[claims[k].0 + 1]).to_lowercase();
                if self.reverified.contains(&pair) {
                    for m in k..=j {
                        let (s, l) = claims[m];
                        out.push(words[s..s + l].join(" "));
                    }
                }
                k = j + 1;
            } else {
                for m in k..=j {
                    let (s, l) = claims[m];
                    out.push(words[s..s + l].join(" "));
                }
                k = j + 1;
            }
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
                let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
                c.expire_negatives(); // re-query verdicts older than NEG_TTL
                (0..200).map_while(|_| c.queue.pop_back()).collect()
            };
            if batch.is_empty() {
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            let result = resolve_batch(&client, &batch);
            let store = crate::store::Store::open().ok();
            {
                let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(chars) = &result {
                    let ok = batch.iter().filter(|n| chars.contains_key(&n.to_lowercase())).count();
                    eprintln!("[pilot] resolved {}/{} (queue ~{})", ok, batch.len(), c.queue.len());
                    for name in &batch {
                        // Resolved (or confirmed not-a-character) — free it from the dedup
                        // set and record the outcome.
                        let lw = name.to_lowercase();
                        c.queued.remove(&lw);
                        let id = chars.get(&lw).copied();
                        // A name that resolves "not a character" AGAIN (it was already negative —
                        // i.e. a forced re-check) is a stale-free negative we can act on.
                        let was_negative = matches!(c.resolved.get(&lw), Some(None));
                        c.resolved.insert(lw.clone(), id);
                        // Negatives are kept in-memory with a TTL (see NEG_TTL), never persisted
                        // — a persisted "not a name" verdict is what made real names vanish.
                        if id.is_none() {
                            c.neg_at.insert(lw.clone(), std::time::Instant::now());
                            if was_negative {
                                c.reverified.insert(lw);
                            }
                        } else {
                            c.reverified.remove(&lw); // it's a character after all
                            if let (Some(store), Some(cid)) = (&store, id) {
                                store.add_known_pilot(name, cid);
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
    if names.is_empty() {
        return Some(HashMap::new());
    }
    let resp = match client.post(ESI_IDS).json(names).send() {
        Ok(r) => r,
        Err(e) => {
            // network / timeout: transient, re-queue the whole batch
            crate::esilog::record(
                "universe/ids network error",
                &format!("error: {e}\nbatch size: {}", names.len()),
            );
            return None;
        }
    };
    let status = resp.status();
    // Read the raw body as text FIRST, so the EXACT bytes can be logged even on a 200 that yields
    // zero matches (the "resolved 0/200" mystery); JSON is parsed from this text below.
    let body = resp.text().unwrap_or_default();
    // ESI returns 400 for the WHOLE batch if any single name is invalid (a parser fragment, a
    // too-short/odd token) or the batch is too large. Split to isolate the offender so one bad
    // token can't stall every other valid name in the batch forever (this is what left real
    // players stuck on the "..." animation).
    if status == reqwest::StatusCode::BAD_REQUEST {
        crate::esilog::record(
            "universe/ids 400",
            &format!(
                "status: {status}\nbatch size: {}\nnames (first 15): {:?}\nbody:\n{body}",
                names.len(),
                name_sample(names),
            ),
        );
        if names.len() > 1 {
            let mid = names.len() / 2;
            let a = resolve_batch(client, &names[..mid]);
            let b = resolve_batch(client, &names[mid..]);
            return match (a, b) {
                (None, None) => None, // both halves hit a transient error: re-queue
                (a, b) => {
                    let mut out = a.unwrap_or_default();
                    out.extend(b.unwrap_or_default());
                    Some(out)
                }
            };
        }
        // A single name ESI rejects is not a resolvable character. Return resolved-but-empty so the
        // caller records a negative verdict (Some(None)) instead of retrying it forever.
        eprintln!("[pilot] ESI rejected name {:?} (400); marking unresolvable", names.first());
        return Some(HashMap::new());
    }
    if !status.is_success() {
        // other HTTP error (rate limit 420, 5xx): transient, re-queue — log the raw body.
        crate::esilog::record(
            "universe/ids non-2xx",
            &format!(
                "status: {status}\nbatch size: {}\nnames (first 15): {:?}\nbody:\n{body}",
                names.len(),
                name_sample(names),
            ),
        );
        eprintln!("[pilot] ESI /universe/ids request failed for {} names; left unresolved", names.len());
        return None;
    }
    let Some(v) = serde_json::from_str::<serde_json::Value>(&body).ok() else {
        eprintln!("[pilot] ESI /universe/ids request failed for {} names; left unresolved", names.len());
        return None; // 200 with an unparseable body: transient, re-queue
    };
    let mut out = HashMap::new();
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
    // A 200 that matched ZERO characters for a non-trivial batch is the "resolved 0/200" mystery —
    // log the raw body so the exact ESI response can be inspected.
    if out.is_empty() && names.len() > 5 {
        crate::esilog::record(
            "universe/ids 200 zero matches",
            &format!(
                "batch size: {}\nnames (first 15): {:?}\nbody:\n{body}",
                names.len(),
                name_sample(names),
            ),
        );
    }
    Some(out)
}

/// First ~15 names of a batch, for a readable log sample.
fn name_sample(names: &[String]) -> Vec<&String> {
    names.iter().take(15).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertainty_and_user_verdicts() {
        let mut c = PilotCache::default();
        c.resolved.insert("bob smith".into(), Some(123));
        c.resolved.insert("ann lee".into(), Some(9));
        c.set_activity_flagged(["bob smith".to_string(), "ann lee".to_string()].into_iter().collect());
        // Flagged + undecided = uncertain, but still shown.
        assert!(c.is_uncertain("Bob Smith") && c.is_uncertain("Ann Lee"));
        assert_eq!(c.display_ids(["Bob Smith", "Ann Lee"].into_iter()).len(), 2);
        // The user hides Bob, keeps Ann.
        c.set_verdict("Bob Smith", true);
        c.set_verdict("Ann Lee", false);
        assert!(c.is_hidden("Bob Smith") && !c.is_uncertain("Bob Smith")); // decided -> not uncertain
        assert!(!c.is_hidden("Ann Lee") && !c.is_uncertain("Ann Lee"));
        let ids = c.display_ids(["Bob Smith", "Ann Lee"].into_iter());
        assert!(!ids.contains_key("Bob Smith") && ids.contains_key("Ann Lee")); // hidden vs shown
        assert!(c.denied().contains("bob smith") && !c.denied().contains("ann lee")); // hidden -> denied
    }

    #[test]
    fn negative_verdict_expires_after_ttl() {
        let mut c = PilotCache::default();
        // A timestamped negative older than the TTL is forgotten (re-queried as pending).
        if let Some(past) = std::time::Instant::now()
            .checked_sub(NEG_TTL + std::time::Duration::from_secs(1))
        {
            c.resolved.insert("river pixies".into(), None);
            c.neg_at.insert("river pixies".into(), past);
            c.expire_negatives();
            assert_eq!(c.get("river pixies"), None, "stale negative should be re-queried");
        }
        // A fresh negative is kept; one with no timestamp (test fixture) never expires.
        c.resolved.insert("real keyword".into(), None);
        c.neg_at.insert("real keyword".into(), std::time::Instant::now());
        c.resolved.insert("fixture".into(), None);
        c.expire_negatives();
        assert_eq!(c.get("real keyword"), Some(None));
        assert_eq!(c.get("fixture"), Some(None));
    }

    #[test]
    fn display_ids_filters_and_requeues() {
        let mut c = PilotCache::default();
        c.resolved.insert("real pilot".into(), Some(42));
        c.resolved.insert("flagged pilot".into(), Some(7));
        c.resolved.insert("hidden pilot".into(), Some(8));
        c.resolved.insert("not a char".into(), None);
        // Flagged = uncertain (still shown); only a user "not a pilot" verdict hides.
        c.set_activity_flagged(std::collections::HashSet::from(["flagged pilot".to_string()]));
        c.set_verdict("hidden pilot", true);

        let names = ["Real Pilot", "Flagged Pilot", "Hidden Pilot", "Not A Char", "Pending Pilot"];
        let out = c.display_ids(names.iter().copied());

        // Confirmed names (including the activity-flagged/uncertain one) show with their id.
        assert_eq!(out.get("Real Pilot"), Some(&42));
        assert_eq!(out.get("Flagged Pilot"), Some(&7));
        // User-hidden, not-a-character, and pending names are all omitted.
        assert!(!out.contains_key("Hidden Pilot"));
        assert!(!out.contains_key("Not A Char"));
        assert!(!out.contains_key("Pending Pilot"));
        assert_eq!(out.len(), 2);

        // The pending name was re-queued so a visible card keeps resolving.
        assert!(c.queued.contains("pending pilot"));
        assert!(c.queue.iter().any(|n| n == "Pending Pilot"));
    }

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
    fn cover_claims_name_from_single_space_glue() {
        // Single-space intel (double-space is only a HINT, so the same line typed with single
        // spaces must resolve too). The over-length blob a hand-typed line leaves is split by the
        // ESI name-window cover, which claims the real 1-3 word name and skips every other span
        // that resolved as a NON-character (a leading/trailing system code, a trailing prose word).
        // "DUO-51 Roadman HighSec CynoLighter likely" (ship "prospect" already masked):
        let mut c = PilotCache::default();
        c.resolved.insert("roadman highsec cynolighter".into(), Some(100));
        for non in ["duo-51 roadman highsec", "duo-51 roadman", "duo-51", "likely"] {
            c.resolved.insert(non.into(), None);
        }
        assert_eq!(
            c.cover("DUO-51 Roadman HighSec CynoLighter likely"),
            vec!["Roadman HighSec CynoLighter"]
        );
        // "Moh Lut 4DS-OI nv core probes": a pilot whose word "Moh" also names a system, glued to a
        // trailing system code + status stop words, still yields just the pilot.
        let mut c2 = PilotCache::default();
        c2.resolved.insert("moh lut".into(), Some(200));
        for non in [
            "moh lut 4ds-oi",
            "4ds-oi nv core",
            "4ds-oi nv",
            "4ds-oi",
            "nv core probes",
            "nv core",
            "nv",
            "core probes",
            "core",
            "probes",
        ] {
            c2.resolved.insert(non.into(), None);
        }
        assert_eq!(c2.cover("Moh Lut 4DS-OI nv core probes"), vec!["Moh Lut"]);
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
    fn cover_refuses_adjacent_singles_inside_a_mixed_run() {
        let mut c = PilotCache::default();
        // "Zantor Thes" carries a (stale/transient) negative, but each word is a confirmed
        // player and the next pair "Vasiliy Tochilkin" is confirmed. The two adjacent singles
        // must NOT split into separate pilots — only the genuine pair does.
        c.resolved.insert("zantor".into(), Some(1));
        c.resolved.insert("thes".into(), Some(2));
        c.resolved.insert("vasiliy tochilkin".into(), Some(3));
        for s in ["zantor thes", "zantor thes vasiliy", "thes vasiliy", "thes vasiliy tochilkin"] {
            c.resolved.insert(s.into(), None);
        }
        assert_eq!(
            c.cover("Zantor Thes Vasiliy Tochilkin"),
            vec!["Vasiliy Tochilkin".to_string()]
        );
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
    fn cover_splits_two_word_block_only_when_negative_is_reverified() {
        let mut c = PilotCache::default();
        c.resolved.insert("ghost".into(), Some(1));
        c.resolved.insert("magician".into(), Some(2));
        c.resolved.insert("ghost magician".into(), None);
        // First "not a character" verdict (could be stale) → keep whole, don't split.
        assert!(c.cover("Ghost Magician").is_empty());
        // Once the negative is re-confirmed (stale-free), it's genuinely two players → split.
        c.reverified.insert("ghost magician".into());
        assert_eq!(
            c.cover("Ghost Magician"),
            vec!["Ghost".to_string(), "Magician".to_string()]
        );
    }

    #[test]
    fn cover_splits_three_handles_but_keeps_a_two_word_name() {
        let mut c = PilotCache::default();
        // Three separately-confirmed handles whose 3-word join ESI rejected as a name: a genuinely
        // mis-joined list, so surface each.
        for n in ["gliar", "mliarvis", "sliarhia"] {
            c.resolved.insert(n.into(), Some(1));
        }
        for n in ["gliar mliarvis", "mliarvis sliarhia", "gliar mliarvis sliarhia"] {
            c.resolved.insert(n.into(), None);
        }
        assert_eq!(
            c.cover("Gliar Mliarvis Sliarhia"),
            vec!["Gliar".to_string(), "Mliarvis".to_string(), "Sliarhia".to_string()]
        );
        // But EXACTLY two adjacent singles stay whole — a likely two-word name ("Zantor Thes")
        // whose words just happen to be players, kept as the pending blob.
        let mut c2 = PilotCache::default();
        c2.resolved.insert("zantor".into(), Some(1));
        c2.resolved.insert("thes".into(), Some(2));
        c2.resolved.insert("zantor thes".into(), None);
        assert!(c2.cover("Zantor Thes").is_empty());
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

    #[test]
    fn implausible_names_become_permanent_negatives_never_queued() {
        let mut c = PilotCache::default();
        // A 4+ word over-glued run: never a character -> immediate negative, not queued, no "...".
        let junk = "Le Van Duc Nguyen Van Minh Phan Van Long";
        c.queue(junk);
        assert_eq!(c.get(junk), Some(None));
        assert!(c.queue.is_empty(), "must not be queued for ESI");
        // A real 2-word name is queued and stays pending until the resolver answers.
        c.queue("Agent Benson");
        assert_eq!(c.get("Agent Benson"), None);
        assert_eq!(c.queue.len(), 1);
        // plausibility bounds.
        assert!(plausible_character_name("Bob"));
        assert!(plausible_character_name("Ingrid Dubois"));
        assert!(plausible_character_name("Bob J Smith"));
        assert!(!plausible_character_name("a")); // too short
        assert!(!plausible_character_name("one two three four")); // 4 words
    }
}
