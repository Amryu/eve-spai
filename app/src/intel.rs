//! Intel parsing and decay state (docs/DESIGN.md §7.1 E3/E4).
//!
//! M2 first pass: detect EVE solar systems mentioned in a chat message (matched
//! against the SDE name index) and a few status keywords (clear / no-visual).
//! The full entity taxonomy (ships, gates, wormholes, …) extends this later.

use std::collections::HashMap;

/// Lower-cased system name -> (canonical name, security).
pub type SystemIndex = HashMap<String, (String, f64)>;

/// How long a report stays live before decaying out of the feed.
pub const DEFAULT_TTL_SECS: i64 = 300;

#[derive(Clone, Debug)]
pub struct IntelReport {
    /// Unix seconds (from the message's EVE timestamp when parseable).
    pub received: i64,
    pub channel: String,
    pub reporter: String,
    pub text: String,
    /// Detected systems with security status.
    pub systems: Vec<(String, f64)>,
    pub clear: bool,
    pub no_visual: bool,
}

#[derive(Default)]
pub struct IntelState {
    pub reports: Vec<IntelReport>,
    /// Most recent "clear" time per system (lower-cased name -> unix seconds).
    cleared: HashMap<String, i64>,
}

impl IntelState {
    pub fn push(&mut self, report: IntelReport) {
        // A "clear" records that the system was reported empty at this time. We do
        // NOT delete prior intel — "clear" only means the hostiles aren't there
        // *now*, so earlier sightings are outdated (greyed), not erased.
        if report.clear {
            for (name, _) in &report.systems {
                let key = name.to_lowercase();
                let slot = self.cleared.entry(key).or_insert(report.received);
                *slot = (*slot).max(report.received);
            }
        }
        self.reports.push(report);
    }

    /// A non-clear sighting is stale if a clear for one of its systems arrived at
    /// or after it — the hostiles have since left.
    pub fn is_stale(&self, report: &IntelReport) -> bool {
        if report.clear {
            return false;
        }
        report.systems.iter().any(|(name, _)| {
            self.cleared
                .get(&name.to_lowercase())
                .is_some_and(|&t| t >= report.received)
        })
    }

    /// Drop reports and clear-marks older than `ttl` seconds.
    pub fn prune(&mut self, ttl: i64, now: i64) {
        self.reports.retain(|r| now - r.received <= ttl);
        self.cleared.retain(|_, t| now - *t <= ttl);
    }
}

const CLEAR_WORDS: &[&str] = &["clear", "clr", "cleared", "clr+"];
const NO_VISUAL_WORDS: &[&str] = &["nv"];

/// Analyse one message into a report.
pub fn analyze(
    text: &str,
    index: &SystemIndex,
    received: i64,
    channel: &str,
    reporter: &str,
) -> IntelReport {
    let tokens: Vec<&str> = tokenize(text);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

    let clear = lower_tokens.iter().any(|t| CLEAR_WORDS.contains(&t.as_str()));
    let no_visual = lower_tokens.iter().any(|t| NO_VISUAL_WORDS.contains(&t.as_str()))
        || text.to_lowercase().contains("no visual");

    let mut systems: Vec<(String, f64)> = Vec::new();
    for tok in &tokens {
        // System names are proper nouns: require an uppercase start or a
        // digit/hyphen (null-sec codes) to avoid matching common words.
        let looks_like_name = tok
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
            || tok.contains('-');
        if !looks_like_name {
            continue;
        }
        if let Some((name, sec)) = index.get(&tok.to_lowercase()) {
            if !systems.iter().any(|(n, _)| n == name) {
                systems.push((name.clone(), *sec));
            }
        }
    }

    IntelReport {
        received,
        channel: channel.to_owned(),
        reporter: reporter.to_owned(),
        text: text.to_owned(),
        systems,
        clear,
        no_visual,
    }
}

/// Split into candidate tokens, keeping `-` and `'` (used in system/char names).
fn tokenize(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\''))
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Parse an EVE timestamp ("2026.06.22 18:30:45", UTC) to unix seconds.
pub fn parse_eve_time(s: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y.%m.%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> SystemIndex {
        let mut m = SystemIndex::new();
        m.insert("rancer".into(), ("Rancer".into(), 0.4));
        m.insert("jita".into(), ("Jita".into(), 0.9));
        m.insert("1dq1-a".into(), ("1DQ1-A".into(), -0.4));
        m
    }

    #[test]
    fn detects_systems_and_keywords() {
        let i = index();

        let hostile = analyze("hostile in Rancer, 3 Drake", &i, 100, "ch", "Scout");
        assert_eq!(hostile.systems, vec![("Rancer".to_owned(), 0.4)]);
        assert!(!hostile.clear && !hostile.no_visual);

        assert!(analyze("Rancer clear", &i, 1, "ch", "Scout").clear);
        assert!(analyze("nv in Jita", &i, 1, "ch", "Scout").no_visual);

        // Null-sec codes with digits/hyphens are detected.
        assert_eq!(analyze("red spike 1DQ1-A", &i, 1, "ch", "Scout").systems.len(), 1);

        // Common lower-case words that happen to be system names are ignored.
        assert!(analyze("clear in here", &i, 1, "ch", "Scout").systems.is_empty());
    }

    #[test]
    fn clear_outdates_prior_sighting_but_not_later_ones() {
        let i = index();
        let mut st = IntelState::default();

        let prior = analyze("hostile in Rancer", &i, 100, "ch", "A");
        let clear = analyze("Rancer clear", &i, 112, "ch", "B");
        let later = analyze("hostile back in Rancer", &i, 120, "ch", "C");
        st.push(prior.clone());
        st.push(clear.clone());
        st.push(later.clone());

        // "clear" does not remove anything.
        assert_eq!(st.reports.len(), 3);
        // The earlier sighting is outdated by the clear...
        assert!(st.is_stale(&prior));
        // ...but the clear itself and a later sighting are current.
        assert!(!st.is_stale(&clear));
        assert!(!st.is_stale(&later));
    }
}
