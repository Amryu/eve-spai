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
    /// External links pasted into the message (killmail / battle report / dscan).
    pub links: Vec<IntelLink>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkKind {
    Killmail,
    BattleReport,
    Dscan,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntelLink {
    pub kind: LinkKind,
    pub url: String,
    /// zKillboard kill id, when this is a killmail link (for dedup with the feed).
    pub kill_id: Option<i64>,
}

/// Find pasted killmail / battle-report / dscan URLs in a message.
pub fn extract_links(text: &str) -> Vec<IntelLink> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        let url = raw.trim_matches(|c: char| "<>()[]\"'".contains(c));
        if !url.starts_with("http") {
            continue;
        }
        let lower = url.to_lowercase();
        let link = if lower.contains("zkillboard.com/kill/") {
            let kill_id = lower
                .split("zkillboard.com/kill/")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .and_then(|s| s.parse::<i64>().ok());
            IntelLink { kind: LinkKind::Killmail, url: url.to_owned(), kill_id }
        } else if lower.contains("br.evetools.org") || lower.contains("zkillboard.com/related/") {
            IntelLink { kind: LinkKind::BattleReport, url: url.to_owned(), kill_id: None }
        } else if lower.contains("dscan.me") || lower.contains("dscan.org") {
            IntelLink { kind: LinkKind::Dscan, url: url.to_owned(), kill_id: None }
        } else {
            continue;
        };
        if !out.contains(&link) {
            out.push(link);
        }
    }
    out
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

    /// Amend the reporter's most recent report (within `grace` seconds) instead of
    /// adding a new one — for intel split across successive messages. Only when the
    /// new message mentions the **same system or no system** (a gate is not a
    /// system) and actually adds something. Returns true if it amended.
    pub fn try_amend(&mut self, new: &IntelReport, grace: i64) -> bool {
        // A clear is always its own report — it must never merge into (and overwrite
        // the threat info of) a prior sighting.
        if new.clear {
            return false;
        }
        let adds = !new.ships.is_empty()
            || !new.pilots.is_empty()
            || new.gate.is_some()
            || new.count.is_some()
            || new.no_visual
            || new.spike
            || new.camp
            || new.bubble
            || new.cyno;
        if !adds {
            return false;
        }
        let new_sys = new.primary_system().map(|s| s.id);
        let new_pilots: std::collections::HashSet<String> =
            new.pilots.iter().map(|p| p.to_lowercase()).collect();
        for prev in self.reports.iter_mut().rev() {
            // Link by the same reporter (split message) OR a shared pilot name (one
            // scout reports the hostile, another adds the ship/route on the same
            // pilot — not linked by system, but by player).
            let same_reporter = prev.reporter == new.reporter;
            let shares_pilot = !new_pilots.is_empty()
                && prev.pilots.iter().any(|p| new_pilots.contains(&p.to_lowercase()));
            if !same_reporter && !shares_pilot {
                continue;
            }
            // Only amend within the grace window (keep scanning older ones otherwise).
            if new.received < prev.received || new.received - prev.received > grace {
                continue;
            }
            let prev_sys = prev.primary_system().map(|s| s.id);
            if new_sys.is_some() && new_sys != prev_sys {
                continue; // a different system is a new sighting / movement
            }
            for sh in &new.ships {
                if !prev.ships.iter().any(|s| s.id == sh.id) {
                    prev.ships.push(sh.clone());
                }
            }
            for p in &new.pilots {
                if !prev.pilots.contains(p) {
                    prev.pilots.push(p.clone());
                }
            }
            if prev.gate.is_none() {
                prev.gate = new.gate.clone();
            }
            if prev.systems.is_empty() {
                prev.systems = new.systems.clone();
            }
            prev.count = match (prev.count, new.count) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            prev.clear |= new.clear;
            prev.no_visual |= new.no_visual;
            prev.spike |= new.spike;
            prev.camp |= new.camp;
            prev.bubble |= new.bubble;
            prev.cyno |= new.cyno;
            prev.killmail |= new.killmail;
            prev.wormhole |= new.wormhole;
            prev.ess |= new.ess;
            prev.skyhook |= new.skyhook;
            prev.received = new.received; // refresh so it re-alerts and reads as fresh
            prev.text = format!("{}  ·  {}", prev.text, new.text);
            if prev.clear {
                for s in &prev.systems {
                    let slot =
                        self.cleared.entry(s.name.to_lowercase()).or_insert(prev.received);
                    *slot = (*slot).max(prev.received);
                }
            }
            return true;
        }
        false
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

/// Quoted spans (delimited by `"`, `'` or `` ` ``, openings/closings may be mixed)
/// — forced to be treated as pilot names rather than keywords/systems. A quote only
/// opens at a word boundary so apostrophes inside names (e.g. "O'Brien") are safe.
fn extract_quoted(text: &str) -> Vec<String> {
    let is_quote = |c: char| c == '"' || c == '\'' || c == '`';
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if is_quote(chars[i]) && (i == 0 || chars[i - 1].is_whitespace()) {
            // Find a closing quote at a word boundary (followed by space/punct/end).
            let mut j = i + 1;
            while j < n {
                if is_quote(chars[j])
                    && (j + 1 == n || chars[j + 1].is_whitespace() || chars[j + 1].is_ascii_punctuation())
                {
                    break;
                }
                j += 1;
            }
            if j < n {
                let inner: String = chars[i + 1..j].iter().collect();
                let inner = inner.trim().to_owned();
                if !inner.is_empty() {
                    out.push(inner);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Candidate pilot names: runs of 2–3 Title-Case alphabetic words in the *raw*
/// text (punctuation and numbers break a run), minus obvious intel/English words.
/// ESI confirms which are real characters later.
/// A token that can be part of an EVE character name. Names can contain digits
/// ("Pericle No1") — allow alphanumeric so long as there's a letter (a bare number
/// is never a name part), but no hyphens (those mark system codes like "GPLB-C").
fn name_part(t: &str) -> bool {
    t.len() >= 2
        && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'')
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

/// Drag-and-drop dscan reports are always "<pilot> (<ship>)". Extract the pilot
/// name (the trailing name run before each parenthesis) — this catches single-word
/// names like "SokoleOko" that the general heuristic misses.
fn extract_dscan_drops(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut search = 0;
    while let Some(open_rel) = text[search..].find('(') {
        let open = search + open_rel;
        let close = match text[open + 1..].find(')') {
            Some(c) => open + 1 + c,
            None => break,
        };
        let name: Vec<&str> = text[..open]
            .split_whitespace()
            .rev()
            .take_while(|t| name_part(t))
            .collect();
        if (1..=3).contains(&name.len()) {
            let pilot = name.into_iter().rev().collect::<Vec<_>>().join(" ");
            if !out.contains(&pilot) {
                out.push(pilot);
            }
        }
        search = close + 1;
    }
    out
}

fn extract_pilots(text: &str) -> Vec<String> {
    let is_namepart = name_part;
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
    let links = extract_links(text);

    // Candidate pilot names first: their tokens must not be parsed as ships or
    // systems (player names often contain hull/system names, e.g. "Sabre Pilot" or
    // "Jita Trader"). Quoted spans are forced to be names.
    let mut pilots = extract_pilots(text);
    // Drag-and-drop dscan names "<pilot> (<ship>)" — catches single-word names.
    for d in extract_dscan_drops(text) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&d)) {
            pilots.push(d);
        }
    }
    for q in extract_quoted(text) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&q)) {
            pilots.push(q);
        }
    }
    let pilot_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();

    // Ships: hull names / nicknames / acronyms (case-insensitive), or an unambiguous
    // typo. A token that belongs to a pilot name is never also parsed as a ship.
    let mut ships: Vec<DetectedShip> = Vec::new();
    let add_ship = |id: i64, name: &str, ships: &mut Vec<DetectedShip>| {
        if !ships.iter().any(|s| s.id == id) {
            ships.push(DetectedShip { id, name: name.to_owned() });
        }
    };
    for tok in &tokens {
        let lower = tok.to_lowercase();
        if pilot_tokens.contains(&lower) {
            continue;
        }
        if let Some((id, name)) = ship_index.get(&lower) {
            add_ship(*id, name, &mut ships);
            continue;
        }
        if lower.len() >= 5 {
            // Typo: accept only if exactly one ship name is within edit distance 1.
            let max = if lower.len() >= 8 { 2 } else { 1 };
            let mut hit: Option<(i64, String)> = None;
            let mut ambiguous = false;
            for (key, (id, name)) in ship_index.iter() {
                if key.len() + 1 < lower.len() || lower.len() + 1 < key.len() {
                    continue;
                }
                if crate::shipnames::edit_distance(&lower, key) <= max {
                    if hit.as_ref().is_some_and(|(hid, _)| *hid != *id) {
                        ambiguous = true;
                        break;
                    }
                    hit = Some((*id, name.clone()));
                }
            }
            if let (Some((id, name)), false) = (hit, ambiguous) {
                add_ship(id, &name, &mut ships);
            }
        }
    }

    let mut detected: Vec<DetectedSystem> = Vec::new();
    // Tokens consumed as systems/gates must not also be counted (e.g. "78" in
    // "on 78 gate" is a gate, not 78 hostiles).
    let mut consumed: Vec<String> = Vec::new();
    // A bare 1–2 digit number is ambiguous: it could be a system/gate code prefix
    // (e.g. "78" → 78-) or a hostile count (e.g. "10 neut"). Defer these and accept
    // them as a system only if they're a direct neighbour of a named system.
    let mut deferred: Vec<&str> = Vec::new();
    for tok in &tokens {
        if pilot_tokens.contains(&tok.to_lowercase()) {
            continue;
        }
        if is_short_number(tok) {
            deferred.push(tok);
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
    // Neighbours of the confidently-named systems.
    let neighbours: std::collections::HashSet<i64> =
        detected.iter().flat_map(|d| systems.neighbors(d.id).iter().copied()).collect();
    for tok in &deferred {
        if let Some(info) = resolve(systems, tok) {
            if neighbours.contains(&info.id) {
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
        // An explicit "<x> gate" keyword is authoritative for a resolvable code, but
        // a bare number that doesn't resolve is never a gate name (a single digit
        // never is, and e.g. "5 gate" means five hostiles).
        let resolved = resolve(systems, cand);
        if resolved.is_none() && cand.chars().all(|c| c.is_ascii_digit()) {
            break;
        }
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
        // Status keywords ignore words that belong to a pilot-name run, so a pilot
        // named e.g. "Clear Skies" can't spoof a "clear" status.
        clear: lower_tokens
            .iter()
            .any(|t| CLEAR_WORDS.contains(&t.as_str()) && !pilot_tokens.contains(t)),
        no_visual: lower_tokens.iter().any(|t| t == "nv" && !pilot_tokens.contains(t))
            || lower.contains("no visual"),
        spike: lower.contains("spike"),
        camp: lower.contains("camp"),
        bubble: lower.contains("bubble"),
        killmail: links.iter().any(|l| l.kind == LinkKind::Killmail) || lower.contains("kill:"),
        cyno: lower.contains("cyno"),
        wormhole: lower.contains("wormhole")
            || lower_tokens.iter().any(|t| (t == "wh" || t == "k162") && !pilot_tokens.contains(t)),
        ess: lower_tokens.iter().any(|t| t == "ess" && !pilot_tokens.contains(t)),
        skyhook: lower.contains("skyhook"),
        gate,
        movement: None,
        links,
    }
}

/// A bare 1–2 digit number — ambiguous between a system/gate code and a count.
fn is_short_number(t: &str) -> bool {
    (1..=2).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit())
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
    // A bare 2-digit number is only a system if it's a "<digits>-…" null-sec code
    // (e.g. "78" → 78-AAA), never an arbitrary prefix — safer, may drop some intel.
    if token.len() == 2 && token.chars().all(|c| c.is_ascii_digit()) {
        return systems.lookup_prefix(&format!("{token}-"));
    }
    // Null-sec codes: uppercase/digit/hyphen, e.g. "78-", "C-J".
    let all_codey = token
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '\'');
    let codey = token.len() >= 2 && all_codey && token.contains('-');
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
    fn detects_single_ship_name() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
        // Case-insensitive: lower-case ship name is detected.
        let r = analyze("E-JCUS sabre", &s, &ships, 1, "ch", "x");
        assert_eq!(r.ships.iter().map(|sh| sh.name.clone()).collect::<Vec<_>>(), vec!["Sabre"]);
        // Single-word "Sabre" prefers the ship even if a pilot shares the name.
        // A compound pilot name ("Sabre Smith") prefers the pilot — no ship parsed.
        let r2 = analyze("Sabre Smith in Rancer", &s, &ships, 1, "ch", "x");
        assert!(r2.ships.is_empty());
        assert_eq!(r2.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(), vec!["Rancer"]);
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
    fn amends_successive_reporter_messages() {
        let s = systems();
        let mut state = IntelState::default();
        state.push(analyze("hostile in Rancer", &s, &noships(), 100, "ch", "Scout"));
        // Same reporter, no system (gate only), within grace -> amends.
        let follow = analyze("on 78- gate", &s, &noships(), 130, "ch", "Scout");
        assert!(state.try_amend(&follow, 60));
        assert_eq!(state.reports.len(), 1);
        assert!(state.reports[0].gate.is_some());
        // A different system is a new sighting, not an amendment.
        let other = analyze("hostile in Jita", &s, &noships(), 140, "ch", "Scout");
        assert!(!state.try_amend(&other, 60));
        // A clear is never amended into a sighting (it must not wipe ship info).
        let clear = analyze("Rancer clear", &s, &noships(), 150, "ch", "Scout");
        assert!(!state.try_amend(&clear, 60));
    }

    #[test]
    fn dscan_drop_extracts_pilot_name() {
        assert_eq!(extract_dscan_drops("YI-GV6 SokoleOko (鱼鹰级海军型)"), vec!["SokoleOko".to_string()]);
        assert_eq!(extract_dscan_drops("Pericle No1 (Loki)"), vec!["Pericle No1".to_string()]);
    }

    #[test]
    fn amends_by_shared_pilot_across_reporters() {
        let s = systems();
        let loki: std::collections::HashMap<String, (i64, String)> =
            [("loki".to_string(), (29990i64, "Loki".to_string()))].into_iter().collect();
        let mut state = IntelState::default();
        // Scout A: a hyphenated system + a pilot with a digit in the name.
        state.push(analyze("C-J6MT Pericle No1", &s, &noships(), 100, "ch", "Kobayashi Mika"));
        assert_eq!(state.reports[0].pilots, vec!["Pericle No1".to_string()]);
        // Scout B (different reporter): same pilot, no system, adds the ship.
        let follow = analyze("Pericle No1 loki", &s, &loki, 130, "ch", "Wallie Warptunnel");
        assert!(state.try_amend(&follow, 60));
        assert_eq!(state.reports.len(), 1);
        assert!(state.reports[0].ships.iter().any(|sh| sh.name == "Loki"));
    }

    #[test]
    fn quoting_forces_pilot_not_keyword() {
        let s = systems();
        let r = analyze("'clear' in Rancer", &s, &noships(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "clear"));
        assert!(!r.clear); // quoted -> not a status keyword
        assert_eq!(r.systems.len(), 1); // Rancer still a system
        // Mixed opening/closing quotes.
        let r2 = analyze("`Some Guy\" tackled", &s, &noships(), 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p == "Some Guy"));
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
