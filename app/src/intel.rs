//! Intel parsing and decay state (docs/DESIGN.md §7.1 E3/E4).
//!
//! Parses a chat message into a concise, structured report: detected solar systems
//! (matched against the SDE), an approximate hostile count, and status flags
//! (clear / no-visual / spike / gate camp / bubble / killmail). The raw text is
//! kept but de-emphasised in the UI.

use std::collections::HashMap;

use crate::geo::Systems;

#[derive(Clone, Debug)]
pub struct DetectedSystem {
    pub id: i64,
    pub name: String,
    pub security: f64,
}

#[derive(Clone, Debug)]
pub struct DetectedShip {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Movement {
    pub from: String,
    pub jumps: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct IntelReport {
    /// Unix seconds (from the message's EVE timestamp when parseable).
    pub received: i64,
    pub channel: String,
    pub reporter: String,
    pub text: String,
    pub systems: Vec<DetectedSystem>,
    pub ships: Vec<DetectedShip>,
    /// Candidate pilot names (Title-Case word runs); confirmed by ESI later.
    pub pilots: Vec<String>,
    /// Approximate hostile/ship count parsed from the message, if any.
    pub count: Option<u32>,
    pub clear: bool,
    pub no_visual: bool,
    pub spike: bool,
    pub camp: bool,
    pub bubble: bool,
    pub killmail: bool,
    pub cyno: bool,
    pub wormhole: bool,
    pub ess: bool,
    pub skyhook: bool,
    /// Gate the hostiles are reported on, e.g. "78-" in "C-J +20 on 78- gate".
    pub gate: Option<String>,
    /// Where the subject was previously seen (set by the watcher).
    pub movement: Option<Movement>,
}

impl IntelReport {
    /// The first detected system (the report's primary location).
    pub fn primary_system(&self) -> Option<&DetectedSystem> {
        self.systems.first()
    }
}

#[derive(Default)]
pub struct IntelState {
    pub reports: Vec<IntelReport>,
    /// Most recent "clear" time per system (lower-cased name -> unix seconds).
    cleared: HashMap<String, i64>,
}

impl IntelState {
    pub fn push(&mut self, report: IntelReport) {
        // A "clear" records that a system was reported empty at this time. We do
        // NOT delete prior intel — "clear" means the hostiles aren't there *now*,
        // so earlier sightings are outdated (greyed), not erased.
        if report.clear {
            for s in &report.systems {
                let slot = self.cleared.entry(s.name.to_lowercase()).or_insert(report.received);
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
        report.systems.iter().any(|s| {
            self.cleared
                .get(&s.name.to_lowercase())
                .is_some_and(|&t| t >= report.received)
        })
    }

    pub fn prune(&mut self, ttl: i64, now: i64) {
        self.reports.retain(|r| now - r.received <= ttl);
        self.cleared.retain(|_, t| now - *t <= ttl);
    }
}

const CLEAR_WORDS: &[&str] = &["clear", "clr", "cleared", "clr+"];

/// Common Title-Case intel/English words that are not pilot names.
const PILOT_STOP: &[&str] = &[
    "gate", "camp", "clear", "clr", "spike", "bubble", "cyno", "local", "dock", "docked",
    "station", "kill", "killmail", "pod", "no", "visual", "nv", "ess", "skyhook", "hostile",
    "hostiles", "neut", "neutral", "neuts", "red", "reds", "blue", "blues", "gang", "fleet",
    "bridge", "jump", "jumping", "warp", "warping", "the", "incoming", "inc", "coming", "gcc",
    "afk", "warpin", "system", "and", "for",
];

/// Candidate pilot names: runs of 2–3 Title-Case alphabetic words in the *raw*
/// text (punctuation and numbers break a run), minus obvious intel/English words.
/// ESI confirms which are real characters later.
fn extract_pilots(text: &str) -> Vec<String> {
    let is_namepart = |t: &str| {
        t.len() >= 2
            && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && t.chars().all(|c| c.is_ascii_alphabetic() || c == '\'')
    };
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
        if (2..=3).contains(&run.len())
            && !run.iter().any(|w| PILOT_STOP.contains(&w.to_lowercase().as_str()))
        {
            let name = run.join(" ");
            if !out.contains(&name) {
                out.push(name);
            }
        }
        run.clear();
    };
    for raw in text.split_whitespace() {
        let punct = |c: char| ",.;:!?\"()".contains(c);
        let trailing = raw.ends_with(punct);
        let core = raw.trim_matches(punct);
        if is_namepart(core) {
            run.push(core.to_owned());
            if trailing {
                flush(&mut run, &mut out);
            }
        } else {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Analyse one message into a structured report (movement is added later).
pub fn analyze(
    text: &str,
    systems: &Systems,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
    received: i64,
    channel: &str,
    reporter: &str,
) -> IntelReport {
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = tokenize(text);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

    // Ships: single-token proper-noun hull names (e.g. "Drake", "Tornado").
    let mut ships: Vec<DetectedShip> = Vec::new();
    for tok in &tokens {
        if tok.chars().next().is_some_and(|c| c.is_uppercase()) {
            if let Some((id, name)) = ship_index.get(&tok.to_lowercase()) {
                if !ships.iter().any(|s| s.id == *id) {
                    ships.push(DetectedShip {
                        id: *id,
                        name: name.clone(),
                    });
                }
            }
        }
    }

    // Candidate pilot names first: their tokens must not be parsed as systems
    // (player names often contain system names, e.g. "Jita Trader").
    let pilots = extract_pilots(text);
    let pilot_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();

    let mut detected: Vec<DetectedSystem> = Vec::new();
    // Tokens consumed as systems/gates must not also be counted (e.g. "78" in
    // "on 78 gate" is a gate, not 78 hostiles).
    let mut consumed: Vec<String> = Vec::new();
    for tok in &tokens {
        if pilot_tokens.contains(&tok.to_lowercase()) {
            continue;
        }
        if let Some(info) = resolve(systems, tok) {
            consumed.push(tok.to_lowercase());
            if !detected.iter().any(|d| d.id == info.id) {
                detected.push(DetectedSystem {
                    id: info.id,
                    name: info.name.clone(),
                    security: info.security,
                });
            }
        }
    }

    // Gate: "... <System> gate" — hostiles are on the gate *to* <System>. Record it
    // (resolved name, or the raw token if abbreviated/unknown) and don't also list
    // it as a plain system.
    let mut gate: Option<String> = None;
    for (i, tok) in tokens.iter().enumerate() {
        if !tok.eq_ignore_ascii_case("gate") || i == 0 {
            continue;
        }
        let cand = tokens[i - 1];
        if cand.eq_ignore_ascii_case("on") || cand.eq_ignore_ascii_case("the") {
            continue;
        }
        let resolved = resolve(systems, cand);
        gate = Some(resolved.map_or_else(|| cand.to_string(), |s| s.name.clone()));
        consumed.push(cand.to_lowercase());
        if let Some(info) = resolved {
            detected.retain(|d| d.id != info.id);
        }
        break;
    }

    IntelReport {
        received,
        channel: channel.to_owned(),
        reporter: reporter.to_owned(),
        text: text.to_owned(),
        pilots,
        systems: detected,
        ships,
        count: parse_count(text, &consumed),
        clear: lower_tokens.iter().any(|t| CLEAR_WORDS.contains(&t.as_str())),
        no_visual: lower_tokens.iter().any(|t| t == "nv") || lower.contains("no visual"),
        spike: lower.contains("spike"),
        camp: lower.contains("camp"),
        bubble: lower.contains("bubble"),
        killmail: lower.contains("zkillboard.com") || lower.contains("kill:"),
        cyno: lower.contains("cyno"),
        wormhole: lower.contains("wormhole")
            || lower_tokens.iter().any(|t| t == "wh" || t == "k162"),
        ess: lower_tokens.iter().any(|t| t == "ess"),
        skyhook: lower.contains("skyhook"),
        gate,
        movement: None,
    }
}

/// Resolve a token to a system: exact name, or an unambiguous null-sec abbreviation
/// (uppercase/digit code with a hyphen, e.g. "78-", "C-J"). The proper-noun guard
/// keeps common lower-case words from matching.
fn resolve<'a>(systems: &'a Systems, token: &str) -> Option<&'a crate::geo::SystemInfo> {
    let first = token.chars().next()?;
    let proper = first.is_uppercase() || first.is_ascii_digit() || token.contains('-');
    if !proper {
        return None;
    }
    if let Some(info) = systems.lookup(token) {
        return Some(info);
    }
    // Null-sec codes: uppercase/digit/hyphen, e.g. "78-", "C-J", or a bare "78".
    let all_codey = token
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '\'');
    let codey = token.len() >= 2 && all_codey && (token.contains('-') || token.chars().all(|c| c.is_ascii_digit()));
    if codey {
        systems.lookup_prefix(token)
    } else {
        None
    }
}

/// Parse an approximate count: `+5`, `x4`, `4x`, or a bare small number. A `+`/`x`
/// decorated number is always a count; a bare number is a count only if it wasn't
/// consumed as a system/gate (so "78" in "on 78 gate" isn't 78 hostiles).
fn parse_count(text: &str, consumed: &[String]) -> Option<u32> {
    let mut best: Option<u32> = None;
    for raw in text.split_whitespace() {
        // Skip system codes (e.g. "78-", "1DQ1-A") — their digits aren't a count.
        if raw.contains('-') {
            continue;
        }
        let t = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '+' && c != 'x');
        let digits = t.trim_start_matches(['+', 'x']).trim_end_matches('x');
        if digits.is_empty() || digits.len() > 3 {
            continue;
        }
        let decorated = t.starts_with('+') || t.starts_with('x') || t.ends_with('x');
        let bare_number = t.chars().all(|c| c.is_ascii_digit());
        if !(decorated || bare_number) {
            continue;
        }
        // A bare number consumed as a system/gate is not a count.
        if bare_number && !decorated && consumed.iter().any(|c| c == &t.to_lowercase()) {
            continue;
        }
        if let Ok(n) = digits.parse::<u32>() {
            if (1..=999).contains(&n) {
                best = Some(best.map_or(n, |b| b.max(n)));
            }
        }
    }
    best
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
    use crate::geo::{SystemInfo, Systems};

    fn noships() -> std::collections::HashMap<String, (i64, String)> {
        std::collections::HashMap::new()
    }

    fn systems() -> Systems {
        let by_name = [
            ("rancer", "Rancer", 1, 0.4),
            ("jita", "Jita", 2, 0.9),
            ("1dq1-a", "1DQ1-A", 3, -0.4),
            ("78-aaa", "78-AAA", 4, -0.5),
            ("c-j6mt", "C-J6MT", 5, -0.6),
        ]
        .into_iter()
        .map(|(key, name, id, sec)| {
            (
                key.to_string(),
                SystemInfo {
                    id,
                    name: name.to_string(),
                    security: sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            )
        })
        .collect();
        Systems::new(by_name, HashMap::new())
    }

    #[test]
    fn extracts_pilot_candidates() {
        let s = systems();
        let r = analyze("Some Pilot tackled in Rancer", &s, &noships(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Some Pilot"));
        // Common Title-Case intel phrases are not pilot candidates.
        let r2 = analyze("Gate Camp in Rancer", &s, &noships(), 1, "ch", "x");
        assert!(r2.pilots.is_empty());
    }

    #[test]
    fn detects_systems_count_and_flags() {
        let s = systems();

        let r = analyze("hostile in Rancer, 3 Drake +2", &s, &noships(), 100, "ch", "Scout");
        assert_eq!(r.systems.len(), 1);
        assert_eq!(r.systems[0].name, "Rancer");
        assert_eq!(r.count, Some(3));
        assert!(!r.clear);

        assert!(analyze("Rancer clear", &s, &noships(), 1, "ch", "x").clear);
        assert!(analyze("nv in Jita", &s, &noships(), 1, "ch", "x").no_visual);
        assert!(analyze("gate camp 1DQ1-A bubble up", &s, &noships(), 1, "ch", "x").camp);
        assert!(analyze("https://zkillboard.com/kill/123/", &s, &noships(), 1, "ch", "x").killmail);
        assert!(analyze("cyno up in Rancer", &s, &noships(), 1, "ch", "x").cyno);
        assert!(analyze("wh in Jita k162", &s, &noships(), 1, "ch", "x").wormhole);
        assert!(analyze("ess being robbed", &s, &noships(), 1, "ch", "x").ess);
        assert!(analyze("skyhook theft Rancer", &s, &noships(), 1, "ch", "x").skyhook);
        // lower-case common words that are system names are not matched
        assert!(analyze("clear in here", &s, &noships(), 1, "ch", "x").systems.is_empty());
    }

    #[test]
    fn detects_gate_and_abbreviated_systems() {
        let s = systems();
        // Abbreviated null-sec codes resolve by unique prefix; the gate is captured
        // and not double-listed as a plain system.
        let r = analyze("C-J +20 on 78- gate", &s, &noships(), 1, "ch", "Scout");
        assert_eq!(r.count, Some(20));
        assert_eq!(r.gate.as_deref(), Some("78-AAA"));
        assert_eq!(
            r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["C-J6MT"],
        );

        // A bare number used as a gate must not also be a hostile count.
        let r2 = analyze("20 reds on 78 gate", &s, &noships(), 1, "ch", "Scout");
        assert_eq!(r2.gate.as_deref(), Some("78-AAA"));
        assert_eq!(r2.count, Some(20));
    }

    #[test]
    fn clear_outdates_prior_sighting_but_not_later_ones() {
        let s = systems();
        let mut st = IntelState::default();
        let prior = analyze("hostile in Rancer", &s, &noships(), 100, "ch", "A");
        let clear = analyze("Rancer clear", &s, &noships(), 112, "ch", "B");
        let later = analyze("hostile back in Rancer", &s, &noships(), 120, "ch", "C");
        st.push(prior.clone());
        st.push(clear.clone());
        st.push(later.clone());

        assert_eq!(st.reports.len(), 3);
        assert!(st.is_stale(&prior));
        assert!(!st.is_stale(&clear));
        assert!(!st.is_stale(&later));
    }
}
