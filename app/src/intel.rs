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
    /// Characters with their id already known (from in-game showinfo links) — no
    /// ESI lookup needed to display/link them.
    pub char_ids: Vec<(String, i64)>,
    /// Approximate hostile/ship count parsed from the message, if any.
    pub count: Option<u32>,
    pub clear: bool,
    /// Someone explicitly asking for intel ("status?") — informational, not a threat.
    pub status: bool,
    pub no_visual: bool,
    pub spike: bool,
    pub camp: bool,
    pub bubble: bool,
    pub killmail: bool,
    pub cyno: bool,
    /// A capital ship (cap / rorqual / dread / carrier / …) reported tackled.
    pub cap_tackled: bool,
    pub wormhole: bool,
    pub ess: bool,
    /// Time left until the ESS is hacked, when called out (e.g. "5:30", "3m").
    pub ess_time: Option<String>,
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
            || new.cyno
            || new.cap_tackled;
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
            prev.cap_tackled |= new.cap_tackled;
            prev.killmail |= new.killmail;
            prev.wormhole |= new.wormhole;
            prev.ess |= new.ess;
            prev.ess_time = new.ess_time.clone().or_else(|| prev.ess_time.clone());
            prev.skyhook |= new.skyhook;
            for l in &new.links {
                if !prev.links.iter().any(|p| p.url == l.url) {
                    prev.links.push(l.clone());
                }
            }
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
    "afk", "warpin", "system", "and", "for", "status", "stat", "report", "intel",
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
/// ("Pericle No1"), apostrophes, and hyphens ("I-Pustelga"). A hyphenated token
/// must have a lower-case letter, else it's an all-caps system code ("67Y-NR").
fn name_part(t: &str) -> bool {
    t.len() >= 2
        && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && (!t.contains('-') || t.chars().any(|c| c.is_ascii_lowercase()))
}

/// A single token distinctive enough to be a name candidate on its own (worth an
/// ESI lookup): a hyphen/apostrophe, internal capital ("SokoleOko"), or a digit —
/// patterns that plain words/ship names don't have.
fn is_distinctive_name(t: &str) -> bool {
    name_part(t)
        && (t.contains('-')
            || t.contains('\'')
            || t.chars().skip(1).any(|c| c.is_ascii_uppercase())
            || t.chars().any(|c| c.is_ascii_digit()))
}

/// Replace each parenthesised span's contents with spaces (so a drag-drop ship name
/// inside "(…)" isn't mistaken for a pilot by the general heuristic).
fn mask_parens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '(' => {
                depth += 1;
                out.push(' ');
            }
            ')' => {
                depth = depth.saturating_sub(1);
                out.push(' ');
            }
            _ if depth > 0 => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Drag-and-drop dscan reports are always "<pilot> (<ship>)". Return each
/// (pilot, ship-text) pair — catches single-word/lower-case names like "SokoleOko"
/// and the (possibly non-English) ship inside the parentheses.
fn extract_dscan_drops(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut search = 0;
    while let Some(open_rel) = text[search..].find('(') {
        let open = search + open_rel;
        let close = match text[open + 1..].find(')') {
            Some(c) => open + 1 + c,
            None => break,
        };
        let ship = text[open + 1..close].trim().to_owned();
        // The "(ship)" context proves the preceding run is a pilot name, so accept
        // any case (catches "bigfoott"); skip all-caps system codes (e.g. "0UBC-R")
        // and status keywords.
        let drop_part = |t: &str| {
            t.len() >= 2
                && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
                && t.chars().any(|c| c.is_ascii_alphabetic())
                && (!t.contains('-') || t.chars().any(|c| c.is_ascii_lowercase()))
                && !PILOT_STOP.contains(&t.to_lowercase().as_str())
        };
        let name: Vec<&str> =
            text[..open].split_whitespace().rev().take_while(|t| drop_part(t)).collect();
        if (1..=3).contains(&name.len()) && !ship.is_empty() {
            let pilot = name.into_iter().rev().collect::<Vec<_>>().join(" ");
            out.push((pilot, ship));
        }
        search = close + 1;
    }
    out
}

/// Match against the local cache of known (ESI-confirmed) pilot names, longest run
/// first so a shorter name that's a subset of a longer one ("Hold" inside "Hold Me
/// Balls") never short-circuits the longer match.
fn match_known_pilots(text: &str, known: &std::collections::HashMap<String, i64>) -> Vec<String> {
    if known.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\''))
        .filter(|w| !w.is_empty())
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut adv = 1;
        let max = 3.min(words.len() - i);
        for len in (1..=max).rev() {
            let run = words[i..i + len].join(" ");
            if known.contains_key(&run.to_lowercase()) {
                out.push(run);
                adv = len;
                break;
            }
        }
        i += adv;
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

/// Multi-word hull names ("Exequror Navy Issue", "Stabber Fleet Issue") matched
/// against the full ship name, longest run first. Returns (start_word, len, id,
/// name). Checked before pilot detection so they aren't read as 3-word names.
fn multiword_ships(
    text: &str,
    ship_index: &HashMap<String, (i64, String)>,
) -> Vec<(usize, usize, i64, String)> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut adv = 1;
        let max = 4.min(words.len() - i);
        for len in (2..=max).rev() {
            let phrase = words[i..i + len].join(" ").to_lowercase();
            if let Some((id, name)) = ship_index.get(&phrase) {
                out.push((i, len, *id, name.clone()));
                adv = len;
                break;
            }
        }
        i += adv;
    }
    out
}

/// Candidate "Title-Case + one lower-case word" names ("Psychopathic beemaster") —
/// EVE family names can be lower-case. Only when the first word isn't a system and
/// the second isn't a ship/keyword; ESI confirmation filters false positives.
fn lowercase_tail_names(
    text: &str,
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = Vec::new();
    for w in words.windows(2) {
        let punct = |c: char| ",.;:!?\"()".contains(c);
        let a = w[0].trim_matches(punct);
        let b = w[1].trim_matches(punct);
        let b_lc = b.to_lowercase();
        let a_ok = name_part(a)
            && a.len() >= 3
            && resolve(systems, a).is_none()
            && !PILOT_STOP.contains(&a.to_lowercase().as_str())
            && !CLEAR_WORDS.contains(&a.to_lowercase().as_str());
        let b_ok = b.len() >= 3
            && b.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && b.chars().all(|c| c.is_ascii_alphabetic() || c == '\'')
            && !PILOT_STOP.contains(&b_lc.as_str())
            && !CLEAR_WORDS.contains(&b_lc.as_str())
            && !ship_index.contains_key(&b_lc);
        if a_ok && b_ok {
            out.push(format!("{a} {b}"));
        }
    }
    out
}

/// Analyse one message into a structured report (movement is added later).
/// Parse EVE in-game "<url=showinfo:TYPE//ID>Name</url>" links (present when intel
/// is pasted straight from the client) — chat-log intel has the tags stripped, so
/// this only matters for pastes. Returns the cleaned text (each tag replaced by its
/// inner name) plus authoritatively classified pilots / ships / systems.
struct UrlTags {
    /// Readable text: each tag replaced by its inner name (for display).
    display: String,
    /// Parse text: tagged spans blanked out (their entities are classified here, so
    /// the heuristic must not re-read them and merge adjacent names into one run).
    masked: String,
    pilots: Vec<String>,
    /// Characters whose id is known from the link (name, character id).
    char_ids: Vec<(String, i64)>,
    ships: Vec<(i64, String)>,
    systems: Vec<String>,
    /// Stargate links — the name is the destination system, i.e. a gate.
    gates: Vec<String>,
}

fn parse_url_tags(
    text: &str,
    systems: &Systems,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
) -> UrlTags {
    let mut t = UrlTags {
        display: String::with_capacity(text.len()),
        masked: String::with_capacity(text.len()),
        pilots: Vec::new(),
        char_ids: Vec::new(),
        ships: Vec::new(),
        systems: Vec::new(),
        gates: Vec::new(),
    };
    let mut rest = text;
    while let Some(start) = rest.find("<url=") {
        t.display.push_str(&rest[..start]);
        t.masked.push_str(&rest[..start]);
        let after = &rest[start + 5..];
        let bail = |t: &mut UrlTags| {
            t.display.push_str(&rest[start..]);
            t.masked.push_str(&rest[start..]);
        };
        let Some(gt) = after.find('>') else {
            bail(&mut t);
            rest = "";
            break;
        };
        let attr = &after[..gt];
        let body = &after[gt + 1..];
        let Some(end) = body.find("</url>") else {
            bail(&mut t);
            rest = "";
            break;
        };
        let inner = body[..end].trim();
        t.display.push_str(inner);
        t.display.push(' ');
        t.masked.push(' '); // blank the linked span for the heuristic
        if let Some(rest_attr) = attr.strip_prefix("showinfo:") {
            let type_id: i64 = rest_attr.split("//").next().unwrap_or("").parse().unwrap_or(0);
            let item_id: i64 = rest_attr.split("//").nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            if !inner.is_empty() {
                // The itemID range disambiguates a stargate (50M) from a solar system
                // (30M); the typeID disambiguates a character bloodline (1373–1390,
                // authoritative even when the name is also a system) or a hull.
                if (50_000_000..60_000_000).contains(&item_id) {
                    // A stargate link — its name is the destination system: a gate.
                    t.gates.push(inner.to_owned());
                } else if type_id == 5 {
                    t.systems.push(inner.to_owned());
                } else if (1373..=1390).contains(&type_id) {
                    t.pilots.push(inner.to_owned());
                    // The itemID after "//" is the character id.
                    if let Some(cid) =
                        rest_attr.split("//").nth(1).and_then(|v| v.parse::<i64>().ok())
                    {
                        t.char_ids.push((inner.to_owned(), cid));
                    }
                } else if let Some((id, name)) = ship_index.get(&inner.to_lowercase()) {
                    t.ships.push((*id, name.clone()));
                } else if resolve(systems, inner).is_some() {
                    t.systems.push(inner.to_owned());
                } else {
                    t.pilots.push(inner.to_owned());
                }
            }
        }
        rest = &body[end + 6..];
    }
    t.display.push_str(rest);
    t.masked.push_str(rest);
    t
}

pub fn analyze(
    text: &str,
    systems: &Systems,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
    known_pilots: &std::collections::HashMap<String, i64>,
    received: i64,
    channel: &str,
    reporter: &str,
) -> IntelReport {
    // Resolve in-game showinfo links first; parse the masked text, display the names.
    let tags = parse_url_tags(text, systems, ship_index);
    let display_text = tags.display.trim().to_owned();
    let si_char_ids = tags.char_ids;
    let si_gates = tags.gates;
    let (si_pilots, si_ships, si_systems) = (tags.pilots, tags.ships, tags.systems);
    let text = tags.masked.as_str();
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = tokenize(text);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let links = extract_links(text);

    // Candidate pilot names first: their tokens must not be parsed as ships or
    // systems (player names often contain hull/system names, e.g. "Sabre Pilot" or
    // "Jita Trader"). Quoted spans are forced to be names.
    // Multi-word hull names are ships, not 3-word pilot names — find and mask them
    // before pilot detection.
    let mw_ships = multiword_ships(text, ship_index);
    let masked_words: String = {
        let punct = |c: char| ",.;:!?\"()".contains(c);
        let mut wv: Vec<String> =
            text.split_whitespace().map(|w| w.trim_matches(punct).to_string()).collect();
        for (start, len, _, _) in &mw_ships {
            for k in *start..(*start + *len).min(wv.len()) {
                wv[k].clear();
            }
        }
        wv.join(" ")
    };
    // Run the general heuristic with parenthesised spans (drag-drop ships) and the
    // multi-word ship spans masked, so neither is read as a pilot.
    let masked = mask_parens(&masked_words);
    let mut pilots = extract_pilots(&masked);
    for n in lowercase_tail_names(&masked, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    // "Word lowercase 1234" runs are pilot names even when the first word is a
    // system ("Amarr slave 3424") — the trailing number disambiguates a name.
    for n in numbered_names(&tokenize(&masked)) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    // Known (ESI-confirmed) names from the local cache — exact, case-insensitive.
    for k in match_known_pilots(text, known_pilots) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&k)) {
            pilots.push(k);
        }
    }
    // Drag-and-drop dscan "<pilot> (<ship>)": the name and the ship (by full name,
    // incl. translations) — catches single-word names and multi-word hull names.
    let mut drop_ships: Vec<(i64, String)> = Vec::new();
    for (pilot, ship_text) in extract_dscan_drops(text) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&pilot)) {
            pilots.push(pilot);
        }
        if let Some((id, name)) = ship_index.get(&ship_text.to_lowercase()) {
            drop_ships.push((*id, name.clone()));
        }
    }
    // Quoted spans are forced to be names (so "'clear'" is a pilot, not a keyword).
    let quoted_raw = extract_quoted(text);
    let quoted: std::collections::HashSet<String> =
        quoted_raw.iter().map(|q| q.to_lowercase()).collect();
    for q in quoted_raw {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&q)) {
            pilots.push(q);
        }
    }
    // Distinctive single tokens (hyphen/apostrophe/internal-caps/digit) that aren't
    // a system — candidates worth an ESI lookup (e.g. "I-Pustelga").
    for t in &tokens {
        if is_distinctive_name(t)
            && resolve(systems, t).is_none()
            && !pilots.iter().any(|p| p.eq_ignore_ascii_case(t))
        {
            pilots.push((*t).to_owned());
        }
    }
    // Characters from in-game showinfo links are authoritative.
    for p in &si_pilots {
        if !pilots.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            pilots.push(p.clone());
        }
    }
    // Drop a candidate that is a contiguous sub-phrase of a longer candidate (so a
    // short known name doesn't false-parse inside a longer reported name), and drop
    // a single-word candidate that is actually a ship (prefer the ship).
    let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
    pilots = pilots
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            let me = &lc[*i];
            let is_subphrase = lc.iter().enumerate().any(|(j, other)| {
                j != *i && other.len() > me.len() && format!(" {other} ").contains(&format!(" {me} "))
            });
            // A candidate that is exactly a hull name is the ship, never a pilot —
            // single- or multi-word (e.g. "Harbinger Navy Issue").
            let is_ship_name = ship_index.contains_key(me);
            // A status shorthand ("clr", "nv", …) is never a pilot, even if some
            // character happens to share that name.
            let single_stop = !p.contains(' ')
                && !quoted.contains(me)
                && (PILOT_STOP.contains(&me.as_str()) || CLEAR_WORDS.contains(&me.as_str()));
            !is_subphrase && !is_ship_name && !single_stop
        })
        .map(|(_, p)| p.clone())
        .collect();
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
        // Resolution order: an exact system name or an exact known character is
        // never reinterpreted as a ship typo (a null-sec abbreviation is resolved as
        // a system in the systems pass, not here).
        if systems.lookup(tok).is_some() || known_pilots.contains_key(&lower) {
            continue;
        }
        // Typo tolerance applies to English/ASCII only; translated names are matched
        // exactly (above), never fuzzily.
        if lower.is_ascii() && lower.len() >= 5 {
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
    // Ships named inside drag-drop parentheses (resolved by full name above).
    for (id, name) in drop_ships {
        add_ship(id, &name, &mut ships);
    }
    // Multi-word hull names ("Exequror Navy Issue") detected before pilot parsing.
    for (_, _, id, name) in mw_ships {
        add_ship(id, &name, &mut ships);
    }
    // Ships from in-game showinfo links.
    for (id, name) in &si_ships {
        add_ship(*id, name, &mut ships);
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

    // Systems from in-game showinfo links.
    for name in &si_systems {
        if let Some(info) = resolve(systems, name) {
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
        // An explicit "<x> gate" keyword is authoritative for a resolvable code, but
        // a bare number that doesn't resolve is never a gate name (a single digit
        // never is, and e.g. "5 gate" means five hostiles).
        let resolved = resolve(systems, cand).or_else(|| {
            // "<X> gate" strongly implies X is a system — accept an unambiguous
            // abbreviation even without a hyphen (e.g. "YPW" → YPW-M2).
            let abbrev = cand.len() >= 2
                && cand.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-');
            if abbrev { systems.lookup_prefix(cand) } else { None }
        });
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

    // A stargate showinfo link names the destination system — that's the gate.
    if gate.is_none() {
        if let Some(g) = si_gates.first() {
            let name = resolve(systems, g).map(|i| i.name.clone());
            // Don't double-list the gate's destination as a plain system.
            if let Some(n) = &name {
                detected.retain(|d| &d.name != n);
            }
            gate = Some(name.unwrap_or_else(|| g.clone()));
        }
    }

    // Two or more systems with no explicit gate: if the later one is a neighbour of
    // the first, it's almost certainly the gate they're heading to (direction of
    // travel), so promote it to the gate instead of a second location.
    if gate.is_none() && detected.len() >= 2 {
        let first = detected[0].id;
        if let Some(pos) =
            detected.iter().skip(1).position(|d| systems.neighbors(first).contains(&d.id))
        {
            let d = detected.remove(pos + 1);
            gate = Some(d.name);
        }
    }

    IntelReport {
        received,
        channel: channel.to_owned(),
        reporter: reporter.to_owned(),
        text: display_text,
        pilots,
        char_ids: si_char_ids,
        systems: detected,
        ships,
        count: parse_count(text, &consumed),
        // Status keywords ignore words that belong to a pilot-name run, so a pilot
        // named e.g. "Clear Skies" can't spoof a "clear" status.
        clear: lower_tokens
            .iter()
            .any(|t| CLEAR_WORDS.contains(&t.as_str()) && !pilot_tokens.contains(t)),
        status: lower_tokens
            .iter()
            .any(|t| matches!(t.as_str(), "status" | "stat") && !pilot_tokens.contains(t)),
        no_visual: lower_tokens.iter().any(|t| t == "nv" && !pilot_tokens.contains(t))
            || lower.contains("no visual"),
        spike: flagged(&lower_tokens, &pilot_tokens, &["spike"]),
        camp: flagged(&lower_tokens, &pilot_tokens, &["camp"]),
        bubble: flagged(&lower_tokens, &pilot_tokens, &["bubble"]),
        killmail: links.iter().any(|l| l.kind == LinkKind::Killmail) || lower.contains("kill:"),
        cyno: flagged(&lower_tokens, &pilot_tokens, &["cyno"]),
        cap_tackled: detect_cap_tackled(&lower_tokens, &pilot_tokens),
        wormhole: lower.contains("wormhole")
            || lower_tokens.iter().any(|t| (t == "wh" || t == "k162") && !pilot_tokens.contains(t)),
        ess: lower_tokens.iter().any(|t| t == "ess" && !pilot_tokens.contains(t)),
        // The ESS hack timer maxes at 6 min for the main bank, 45 min for the
        // reserve. A larger "Xm" is an ISK amount (e.g. "77m bank"), not a time.
        ess_time: if lower.contains("ess") {
            let max = if lower.contains("reserve") { 45 } else { 6 };
            parse_time_left(text, max)
        } else {
            None
        },
        skyhook: lower.contains("skyhook"),
        gate,
        movement: None,
        links,
    }
}

/// Parse a "time left" callout: "M:SS" (e.g. "5:30"), or a number followed by a
/// minute/second unit — attached ("5m", "30s") or spaced ("5 min", "30 sec").
/// Minutes over `max_min` are rejected (a big "Xm" is an ISK amount, e.g. "77m
/// bank", not a hack timer).
fn parse_time_left(text: &str, max_min: u32) -> Option<String> {
    let toks: Vec<&str> = text.split_whitespace().collect();
    // "M:SS" minutes:seconds.
    for raw in &toks {
        let t = raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != ':');
        if let Some((m, s)) = t.split_once(':') {
            if (1..=2).contains(&m.len())
                && s.len() == 2
                && m.bytes().all(|b| b.is_ascii_digit())
                && s.bytes().all(|b| b.is_ascii_digit())
                && m.parse::<u32>().is_ok_and(|v| v <= max_min)
            {
                return Some(format!("{m}:{s}"));
            }
        }
    }
    // A number with a minute/second unit (the unit may be the next token).
    for (i, raw) in toks.iter().enumerate() {
        let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() || digits.len() > 3 {
            continue;
        }
        let Ok(n) = digits.parse::<u32>() else { continue };
        let tail = raw[digits.len()..].to_lowercase();
        let unit = if !tail.is_empty() {
            tail
        } else {
            toks.get(i + 1).map(|t| t.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_lowercase()).unwrap_or_default()
        };
        if matches!(unit.as_str(), "m" | "min" | "mins" | "minute" | "minutes") {
            if (1..=max_min).contains(&n) {
                return Some(format!("{n}m"));
            }
        } else if matches!(unit.as_str(), "s" | "sec" | "secs" | "second" | "seconds") && (1..=599).contains(&n)
        {
            return Some(format!("{n}s"));
        }
    }
    None
}

/// True if a token names a capital ship class.
fn is_cap_word(t: &str) -> bool {
    matches!(
        t,
        "cap" | "caps" | "capital" | "capitals" | "rorq" | "rorqs" | "rorqual" | "rorquals"
            | "dread" | "dreads" | "dreadnought" | "dreadnoughts" | "carrier" | "carriers"
            | "fax" | "faxes" | "titan" | "titans" | "super" | "supers" | "supercap"
            | "supercaps" | "supercarrier" | "supercarriers"
    )
}

/// True if a token is a tackle verb (prefix-matched for typo/tense robustness:
/// tackl*, takl*, scram*, scrambl*, point*).
fn is_tackle_word(t: &str) -> bool {
    t.starts_with("tackl")
        || t.starts_with("takl")
        || t.starts_with("tackel")
        || t.starts_with("scram")
        || t.starts_with("scrambl")
        || t.starts_with("point")
}

/// A capital reported tackled: a cap-class word AND a tackle word both appear
/// anywhere in the message (robust to word order, spacing, and tense/typos). Words
/// that belong to a pilot name are ignored.
fn detect_cap_tackled(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
) -> bool {
    let cap = lower_tokens.iter().any(|t| !pilot_tokens.contains(t) && is_cap_word(t));
    let tackle = lower_tokens.iter().any(|t| !pilot_tokens.contains(t) && is_tackle_word(t));
    cap && tackle
}

/// True if any token starts with one of `stems` and is neither part of a pilot name
/// nor preceded by a negation ("no", "not", "without") — so "no bubble" doesn't set
/// `bubble`.
fn flagged(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
    stems: &[&str],
) -> bool {
    const NEG: &[&str] = &["no", "not", "without", "n0", "negative"];
    lower_tokens.iter().enumerate().any(|(i, t)| {
        stems.iter().any(|s| t.starts_with(s))
            && !pilot_tokens.contains(t)
            && !(i > 0 && NEG.contains(&lower_tokens[i - 1].as_str()))
    })
}

/// "Title-Case + lower-case word + a number" runs — a pilot name even when the
/// first word is a system ("Amarr slave 3424"); the trailing number disambiguates.
fn numbered_names(tokens: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for w in tokens.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let a_ok = name_part(a) && a.len() >= 2;
        let blc = b.to_lowercase();
        let b_ok = b.len() >= 2
            && b.chars().all(|ch| ch.is_ascii_lowercase())
            && !PILOT_STOP.contains(&blc.as_str())
            && !CLEAR_WORDS.contains(&blc.as_str());
        let c_ok = c.len() >= 2 && c.chars().all(|ch| ch.is_ascii_digit());
        if a_ok && b_ok && c_ok {
            out.push(format!("{a} {b} {c}"));
        }
    }
    out
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
        let digits = t.trim_start_matches(['+', 'x']).trim_end_matches(['x', '+']);
        if digits.is_empty() || digits.len() > 3 {
            continue;
        }
        // "+9", "9+", "x9", "9x" all decorate a count.
        let decorated =
            t.starts_with('+') || t.starts_with('x') || t.ends_with('x') || t.ends_with('+');
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
                // Add up separate groups, e.g. "7 red; 1 neut" -> 8.
                best = Some(best.map_or(n, |b| (b + n).min(999)));
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

    fn noknown() -> std::collections::HashMap<String, i64> {
        std::collections::HashMap::new()
    }

    fn systems() -> Systems {
        let by_name = [
            ("rancer", "Rancer", 1, 0.4),
            ("jita", "Jita", 2, 0.9),
            ("1dq1-a", "1DQ1-A", 3, -0.4),
            ("78-aaa", "78-AAA", 4, -0.5),
            ("c-j6mt", "C-J6MT", 5, -0.6),
            ("ypw-m2", "YPW-M2", 7, -0.5),
            ("amarr", "Amarr", 8, 1.0),
            ("sv5-8n", "SV5-8N", 9, -0.4),
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
        let r = analyze("E-JCUS sabre", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r.ships.iter().map(|sh| sh.name.clone()).collect::<Vec<_>>(), vec!["Sabre"]);
        // Single-word "Sabre" prefers the ship even if a pilot shares the name.
        // A compound pilot name ("Sabre Smith") prefers the pilot — no ship parsed.
        let r2 = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.is_empty());
        assert_eq!(r2.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(), vec!["Rancer"]);
    }

    #[test]
    fn extracts_pilot_candidates() {
        let s = systems();
        let r = analyze("Some Pilot tackled in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Some Pilot"));
        // Common Title-Case intel phrases are not pilot candidates.
        let r2 = analyze("Gate Camp in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.is_empty());
    }

    #[test]
    fn amends_successive_reporter_messages() {
        let s = systems();
        let mut state = IntelState::default();
        state.push(analyze("hostile in Rancer", &s, &noships(), &noknown(), 100, "ch", "Scout"));
        // Same reporter, no system (gate only), within grace -> amends.
        let follow = analyze("on 78- gate", &s, &noships(), &noknown(), 130, "ch", "Scout");
        assert!(state.try_amend(&follow, 60));
        assert_eq!(state.reports.len(), 1);
        assert!(state.reports[0].gate.is_some());
        // A different system is a new sighting, not an amendment.
        let other = analyze("hostile in Jita", &s, &noships(), &noknown(), 140, "ch", "Scout");
        assert!(!state.try_amend(&other, 60));
        // A clear is never amended into a sighting (it must not wipe ship info).
        let clear = analyze("Rancer clear", &s, &noships(), &noknown(), 150, "ch", "Scout");
        assert!(!state.try_amend(&clear, 60));
    }

    #[test]
    fn known_pilots_match_with_subset_protection() {
        let s = systems();
        // A lower-case single-word known name is recognised.
        let k1: std::collections::HashMap<String, i64> =
            [("bigfoott".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Rancer bigfoott", &s, &noships(), &k1, 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott")));
        // A name that is a subset of a longer one must not short-circuit it.
        let k2: std::collections::HashMap<String, i64> =
            [("hold me balls".to_string(), 1i64), ("hold".to_string(), 3i64)].into_iter().collect();
        let r2 = analyze("E-JCUS HOLD ME BALLS", &s, &noships(), &k2, 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("hold me balls")));
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("hold")));
    }

    #[test]
    fn dscan_drop_ship_is_not_a_pilot() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [(
            "council diplomatic shuttle".to_string(),
            (670i64, "Council Diplomatic Shuttle".to_string()),
        )]
        .into_iter()
        .collect();
        let r = analyze(
            "I-Pustelga (Council Diplomatic Shuttle)",
            &s,
            &ships,
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(r.pilots.iter().any(|p| p == "I-Pustelga"));
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Council Diplomatic Shuttle")));
        assert!(r.ships.iter().any(|sh| sh.name == "Council Diplomatic Shuttle"));
    }

    #[test]
    fn dscan_drop_extracts_pilot_name() {
        assert_eq!(
            extract_dscan_drops("YI-GV6 SokoleOko (鱼鹰级海军型)"),
            vec![("SokoleOko".to_string(), "鱼鹰级海军型".to_string())]
        );
        // System code before the name is excluded; the ship is the parens content.
        assert_eq!(
            extract_dscan_drops("0UBC-R I-Pustelga (Council Diplomatic Shuttle)"),
            vec![("I-Pustelga".to_string(), "Council Diplomatic Shuttle".to_string())]
        );
    }

    #[test]
    fn amends_by_shared_pilot_across_reporters() {
        let s = systems();
        let loki: std::collections::HashMap<String, (i64, String)> =
            [("loki".to_string(), (29990i64, "Loki".to_string()))].into_iter().collect();
        let mut state = IntelState::default();
        // Scout A: a hyphenated system + a pilot with a digit in the name.
        state.push(analyze("C-J6MT Pericle No1", &s, &noships(), &noknown(), 100, "ch", "Kobayashi Mika"));
        assert_eq!(state.reports[0].pilots, vec!["Pericle No1".to_string()]);
        // Scout B (different reporter): same pilot, no system, adds the ship.
        let follow = analyze("Pericle No1 loki", &s, &loki, &noknown(), 130, "ch", "Wallie Warptunnel");
        assert!(state.try_amend(&follow, 60));
        assert_eq!(state.reports.len(), 1);
        assert!(state.reports[0].ships.iter().any(|sh| sh.name == "Loki"));
    }

    #[test]
    fn quoting_forces_pilot_not_keyword() {
        let s = systems();
        let r = analyze("'clear' in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "clear"));
        assert!(!r.clear); // quoted -> not a status keyword
        assert_eq!(r.systems.len(), 1); // Rancer still a system
        // Mixed opening/closing quotes.
        let r2 = analyze("`Some Guy\" tackled", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p == "Some Guy"));
    }

    #[test]
    fn detects_systems_count_and_flags() {
        let s = systems();

        let r = analyze("hostile in Rancer, 3 Drake +2", &s, &noships(), &noknown(), 100, "ch", "Scout");
        assert_eq!(r.systems.len(), 1);
        assert_eq!(r.systems[0].name, "Rancer");
        assert_eq!(r.count, Some(5)); // groups summed: 3 + 2
        assert!(!r.clear);

        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(analyze("nv in Jita", &s, &noships(), &noknown(), 1, "ch", "x").no_visual);
        assert!(analyze("gate camp 1DQ1-A bubble up", &s, &noships(), &noknown(), 1, "ch", "x").camp);
        assert!(analyze("https://zkillboard.com/kill/123/", &s, &noships(), &noknown(), 1, "ch", "x").killmail);
        assert!(analyze("cyno up in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").cyno);
        assert!(analyze("wh in Jita k162", &s, &noships(), &noknown(), 1, "ch", "x").wormhole);
        assert!(analyze("ess being robbed", &s, &noships(), &noknown(), 1, "ch", "x").ess);
        assert!(analyze("skyhook theft Rancer", &s, &noships(), &noknown(), 1, "ch", "x").skyhook);
        // lower-case common words that are system names are not matched
        assert!(analyze("clear in here", &s, &noships(), &noknown(), 1, "ch", "x").systems.is_empty());
    }

    #[test]
    fn multiword_ship_is_not_a_pilot() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [(
            "exequror navy issue".to_string(),
            (29344i64, "Exequror Navy Issue".to_string()),
        )]
        .into_iter()
        .collect();
        let r = analyze("78-0R6 Exequror Navy Issue", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Exequror Navy Issue"));
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Exequror Navy Issue")));
    }

    #[test]
    fn showinfo_links_classify_entities() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sleipnir".to_string(), (22444i64, "Sleipnir".to_string()))].into_iter().collect();
        let r = analyze(
            "x > <url=showinfo:5//30001242>Rancer</url> \
             <url=showinfo:1375//625637028>Catastrophic</url> \
             <url=showinfo:22444//1054499194005>Sleipnir</url>",
            &s,
            &ships,
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(r.pilots.iter().any(|p| p == "Catastrophic"));
        assert!(r.ships.iter().any(|sh| sh.name == "Sleipnir"));
        assert!(r.systems.iter().any(|d| d.name == "Rancer"));
        assert!(!r.pilots.iter().any(|p| p == "Sleipnir"));
    }

    #[test]
    fn showinfo_full_card_has_system_ship_pilot() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("hecate".to_string(), (35683i64, "Hecate".to_string()))].into_iter().collect();
        let r = analyze(
            "<url=showinfo:5//30002187>Rancer</url> \
             <url=showinfo:1375//91643796>Venum Einherjar's</url> \
             <url=showinfo:35683//1054509319774>Hecate</url>",
            &s,
            &ships,
            &noknown(),
            1,
            "ch",
            "Masiell Hinken",
        );
        assert!(r.systems.iter().any(|d| d.name == "Rancer"), "system missing: {:?}", r.systems);
        assert!(r.ships.iter().any(|sh| sh.name == "Hecate"), "ship missing: {:?}", r.ships);
        assert!(r.pilots.iter().any(|p| p == "Venum Einherjar's"), "pilot missing: {:?}", r.pilots);
    }

    #[test]
    fn showinfo_name_with_space_and_hyphen() {
        let s = systems();
        // "Nine -3" (typeID 1375 = character) — a name with an internal space + hyphen.
        let r = analyze(
            "<url=showinfo:1375//2121803366>Nine -3</url> <url=showinfo:5//30000469>9-02G0</url>",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(r.pilots.iter().any(|p| p == "Nine -3"), "pilots: {:?}", r.pilots);
    }

    #[test]
    fn showinfo_character_typeid_makes_pilot() {
        let s = systems();
        // typeID 1380 is a character bloodline → "Sindend" is a character, even
        // though the name resolves to no system or ship (it must not be typo'd).
        let r = analyze(
            "<url=showinfo:1380//2124077067>Sindend</url> <url=showinfo:5//30000669>N3-JBX</url>",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(r.pilots.iter().any(|p| p == "Sindend"));
    }

    #[test]
    fn multiword_ship_via_known_cache_is_not_a_pilot() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [(
            "harbinger navy issue".to_string(),
            (24692i64, "Harbinger Navy Issue".to_string()),
        )]
        .into_iter()
        .collect();
        // Even if the name is (wrongly) in the known-pilot cache, an exact hull name
        // is the ship, not a pilot.
        let known: std::collections::HashMap<String, i64> =
            [("harbinger navy issue".to_string(), 1i64)].into_iter().collect();
        let r = analyze("J5A-IX Harbinger Navy Issue", &s, &ships, &known, 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Harbinger Navy Issue"));
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Harbinger Navy Issue")));
    }

    #[test]
    fn lowercase_family_name_recognised() {
        let s = systems();
        let r = analyze("78-0R6 Psychopathic beemaster", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Psychopathic beemaster"));
    }

    #[test]
    fn numbered_name_with_system_prefix_plain_text() {
        let s = systems();
        // Plain text (tags stripped): "Amarr slave 3424" is a pilot, not the Amarr
        // system; SV5-8N stays the system.
        let r = analyze("SV5-8N Amarr slave 3424", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Amarr slave 3424"), "pilots: {:?}", r.pilots);
        assert!(!r.systems.iter().any(|d| d.name == "Amarr"), "systems: {:?}", r.systems);
        assert!(r.systems.iter().any(|d| d.name == "SV5-8N"));
    }

    #[test]
    fn showinfo_character_name_containing_system_word() {
        let s = systems();
        // "Amarr slave 3424" (typeID 1379 = character) must be a pilot, not detected
        // as the Amarr system from the leading word.
        let r = analyze(
            "<url=showinfo:5//30001158>SV5-8N</url> \
             <url=showinfo:1379//2123880778>Amarr slave 3424</url> \
             <url=showinfo:1376//2124463618>CIYUAN</url>",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(r.pilots.iter().any(|p| p == "Amarr slave 3424"), "pilots: {:?}", r.pilots);
        assert!(!r.systems.iter().any(|d| d.name == "Amarr"), "systems: {:?}", r.systems);
        assert!(r.systems.iter().any(|d| d.name == "SV5-8N"));
    }

    #[test]
    fn showinfo_stargate_is_gate() {
        let s = systems();
        let r = analyze(
            "<url=showinfo:5//30004937>Rancer</url> 和 <url=showinfo:17//50012542>Jita</url> 相似",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert_eq!(r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["Rancer"]);
        assert_eq!(r.gate.as_deref(), Some("Jita")); // the stargate link → gate
    }

    #[test]
    fn neighbour_second_system_becomes_gate() {
        // Rancer (1) and Jita (2) as gate neighbours.
        let by_name: std::collections::HashMap<String, SystemInfo> = [
            ("rancer", "Rancer", 1, 0.4),
            ("jita", "Jita", 2, 0.9),
        ]
        .into_iter()
        .map(|(k, n, id, sec)| {
            (
                k.to_string(),
                SystemInfo {
                    id,
                    name: n.to_string(),
                    security: sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            )
        })
        .collect();
        let adj = std::collections::HashMap::from([(1i64, vec![2i64]), (2, vec![1])]);
        let s = Systems::new(by_name, adj);
        let r = analyze("hostiles in Rancer heading Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["Rancer"]);
        assert_eq!(r.gate.as_deref(), Some("Jita"));
    }

    #[test]
    fn negation_gate_abbrev_and_status() {
        let s = systems();
        // "no bubble" must not set bubble; "YPW gate" resolves to the full system name.
        let r = analyze(
            "C-J6MT YPW gate clear,no bubble,where the neuts went?",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(!r.bubble, "negated bubble");
        assert_eq!(r.gate.as_deref(), Some("YPW-M2"));
        // "status" is a request keyword: not a pilot, and stays informational.
        let q = analyze("status in Rancer?", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(q.status);
        assert!(!q.pilots.iter().any(|p| p.eq_ignore_ascii_case("status")));
    }

    #[test]
    fn detects_cap_tackled_variations() {
        let s = systems();
        let cap = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled;
        assert!(cap("Rancer cap tackled"));
        assert!(cap("rorqual  pointed in Jita")); // double space, words apart
        assert!(cap("dread scrammed on gate"));
        assert!(cap("carrier takled")); // typo
        assert!(cap("super got scram"));
        // Both a cap word and a tackle word are required.
        assert!(!cap("cap stable"));
        assert!(!cap("tackled a frigate"));
    }

    #[test]
    fn ess_time_ignores_isk_amount() {
        let s = systems();
        // "5 min" is the timer; "77m bank" is ISK, not 77 minutes.
        let r = analyze("TPG-DD ESS 5 min 77m bank", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.ess_time.as_deref(), Some("5m"));
        // Reserve allows up to 45 min.
        let r2 = analyze("ESS reserve 30 min", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.ess_time.as_deref(), Some("30m"));
        // Normal bank rejects an over-max minute value.
        let r3 = analyze("ESS robbed 30m", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r3.ess_time, None);
    }

    #[test]
    fn sums_separate_hostile_groups() {
        let s = systems();
        let r = analyze("PDF-3Z 7 red; 1 neut", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(8));
    }

    #[test]
    fn detects_gate_and_abbreviated_systems() {
        let s = systems();
        // Abbreviated null-sec codes resolve by unique prefix; the gate is captured
        // and not double-listed as a plain system.
        let r = analyze("C-J +20 on 78- gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert_eq!(r.count, Some(20));
        assert_eq!(r.gate.as_deref(), Some("78-AAA"));
        assert_eq!(
            r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["C-J6MT"],
        );

        // A bare number used as a gate must not also be a hostile count.
        let r2 = analyze("20 reds on 78 gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert_eq!(r2.gate.as_deref(), Some("78-AAA"));
        assert_eq!(r2.count, Some(20));
    }

    #[test]
    fn clear_outdates_prior_sighting_but_not_later_ones() {
        let s = systems();
        let mut st = IntelState::default();
        let prior = analyze("hostile in Rancer", &s, &noships(), &noknown(), 100, "ch", "A");
        let clear = analyze("Rancer clear", &s, &noships(), &noknown(), 112, "ch", "B");
        let later = analyze("hostile back in Rancer", &s, &noships(), &noknown(), 120, "ch", "C");
        st.push(prior.clone());
        st.push(clear.clone());
        st.push(later.clone());

        assert_eq!(st.reports.len(), 3);
        assert!(st.is_stale(&prior));
        assert!(!st.is_stale(&clear));
        assert!(!st.is_stale(&later));
    }
}
