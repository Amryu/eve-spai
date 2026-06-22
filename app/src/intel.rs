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
}

impl IntelState {
    pub fn push(&mut self, report: IntelReport) {
        self.reports.push(report);
    }

    /// Drop reports older than `ttl` seconds.
    pub fn prune(&mut self, ttl: i64, now: i64) {
        self.reports.retain(|r| now - r.received <= ttl);
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
