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

#[derive(Clone, Debug, Default)]
pub struct IntelReport {
    /// Stable per-report id (assigned on push, preserved across amendments) so the alert
    /// window can re-find a report after its content — and thus report_key — changes.
    pub id: u64,
    /// Unix seconds (from the message's EVE timestamp when parseable).
    pub received: i64,
    pub channel: String,
    pub reporter: String,
    pub text: String,
    pub systems: Vec<DetectedSystem>,
    pub ships: Vec<DetectedShip>,
    /// Ship classes named only by keyword, no specific hull ("dic", "recon", "logi").
    pub classes: Vec<String>,
    /// Candidate pilot names (Title-Case word runs); confirmed by ESI later.
    pub pilots: Vec<String>,
    /// Characters with their id already known (from in-game showinfo links) — no
    /// ESI lookup needed to display/link them.
    pub char_ids: Vec<(String, i64)>,
    /// Approximate hostile/ship count parsed from the message, if any.
    pub count: Option<u32>,
    /// Bare numbers tentatively treated as name components ("Adama 80"): (candidate, n).
    /// The reconcile adds `n` back to the count if ESI says the candidate isn't a pilot.
    pub name_number_skips: Vec<(String, u32)>,
    /// An ISK amount posted in the message ("300kk" -> 300_000_000), if any.
    pub isk: Option<u64>,
    /// Structures mentioned (canonical name) + an optional distance off each.
    pub structures: Vec<(String, Option<String>)>,
    /// Celestial locations named in the message ("Planet 4", "Moon 3", "Sun").
    pub celestials: Vec<String>,
    /// Scanning probes mentioned (Core/Combat Scanner Probe items + slang) — distinct from
    /// the Probe frigate. The badge label ("Core Probes"/"Combat Probes"/"Probes"), or None.
    pub probes: Option<&'static str>,
    pub clear: bool,
    /// Someone explicitly asking for intel ("status?") — informational, not a threat.
    pub status: bool,
    pub no_visual: bool,
    pub spike: bool,
    pub camp: bool,
    /// A call for help / backup (help / sos / "need backup").
    pub help: bool,
    pub bubble: bool,
    pub killmail: bool,
    pub cyno: bool,
    /// A hot-drop / black-ops threat ("dropper", "hotdropper", "blops", …).
    pub dropper: bool,
    /// A capital ship (cap / rorqual / dread / carrier / …) reported tackled.
    pub cap_tackled: bool,
    /// A (non-capital) ship/type reported tackled.
    pub tackled: bool,
    /// Ship/class names reported tackled ("Loki", "Marauder"), for the TACKLED badge.
    pub tackled_targets: Vec<String>,
    pub wormhole: bool,
    /// Detected wormhole signature code (e.g. "K162"), when one was named.
    pub wh_type: Option<String>,
    /// Destination class guessed from the message text. This is only a guess — the
    /// wormhole *type's* own class (and EVE-Scout data) are authoritative facts and
    /// override it; it's used only when the type doesn't pin the class.
    pub wh_dest: Option<crate::wormholes::DestClass>,
    /// "EOL" / end-of-life was called out.
    pub wh_eol: bool,
    /// "drifter" was called out.
    pub wh_drifter: bool,
    /// Cosmic signature id (e.g. "ABC-123"), if named.
    pub wh_sig: Option<String>,
    pub ess: bool,
    /// Time left until the ESS is hacked, when called out (e.g. "5:30", "3m").
    pub ess_time: Option<String>,
    pub skyhook: bool,
    /// Gate the hostiles are reported on, e.g. "78-" in "C-J +20 on 78- gate".
    /// Gates mentioned in the report (a card has one system but may name several
    /// gates — extra system mentions are demoted to gates).
    pub gates: Vec<String>,
    /// Alliances mentioned by shorthand → (name, alliance id) for the logo.
    pub alliances: Vec<(String, i64)>,
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

static NEXT_REPORT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl IntelState {
    /// Push a report and return its assigned stable id.
    pub fn push(&mut self, mut report: IntelReport) -> u64 {
        let id = NEXT_REPORT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        report.id = id;
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
        id
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
            || !new.gates.is_empty()
            || new.count.is_some()
            || new.no_visual
            || new.spike
            || new.camp
            || new.bubble
            || new.cyno
            || new.dropper
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
            for c in &new.classes {
                if !prev.classes.iter().any(|x| x.eq_ignore_ascii_case(c)) {
                    prev.classes.push(c.clone());
                }
            }
            for p in &new.pilots {
                if !prev.pilots.iter().any(|x| x.eq_ignore_ascii_case(p)) {
                    prev.pilots.push(p.clone());
                }
            }
            // Authoritative showinfo char-ids and alliance mentions must merge too, else
            // a merged pilot loses its char-link and is dropped from the card.
            for c in &new.char_ids {
                if !prev.char_ids.iter().any(|(n, _)| n.eq_ignore_ascii_case(&c.0)) {
                    prev.char_ids.push(c.clone());
                }
            }
            // Sub-phrase dedup AFTER char-ids merge, protecting every char-linked name so a
            // glued plain-text relay can't evict an authoritative one.
            let protected: std::collections::HashSet<String> =
                prev.char_ids.iter().map(|(n, _)| n.to_lowercase()).collect();
            drop_subphrase_pilots(&mut prev.pilots, &protected);
            for a in &new.alliances {
                if !prev.alliances.iter().any(|(n, _)| n.eq_ignore_ascii_case(&a.0)) {
                    prev.alliances.push(a.clone());
                }
            }
            for g in &new.gates {
                if !prev.gates.iter().any(|x| x.eq_ignore_ascii_case(g)) {
                    prev.gates.push(g.clone());
                }
            }
            if prev.systems.is_empty() {
                prev.systems = new.systems.clone();
            }
            prev.count = match (prev.count, new.count) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            prev.isk = match (prev.isk, new.isk) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            prev.probes = new.probes.or(prev.probes);
            for (n, d) in &new.structures {
                match prev.structures.iter_mut().find(|(pn, _)| pn == n) {
                    Some(e) => {
                        if e.1.is_none() {
                            e.1 = d.clone();
                        }
                    }
                    None => prev.structures.push((n.clone(), d.clone())),
                }
            }
            for c in &new.celestials {
                if !prev.celestials.iter().any(|x| x.eq_ignore_ascii_case(c)) {
                    prev.celestials.push(c.clone());
                }
            }
            for sk in &new.name_number_skips {
                if !prev.name_number_skips.iter().any(|(c, _)| c.eq_ignore_ascii_case(&sk.0)) {
                    prev.name_number_skips.push(sk.clone());
                }
            }
            prev.clear |= new.clear;
            prev.no_visual |= new.no_visual;
            prev.spike |= new.spike;
            prev.camp |= new.camp;
            prev.help |= new.help;
            prev.bubble |= new.bubble;
            prev.cyno |= new.cyno;
            prev.dropper |= new.dropper;
            prev.cap_tackled |= new.cap_tackled;
            prev.tackled |= new.tackled;
            for tt in &new.tackled_targets {
                if !prev.tackled_targets.iter().any(|x| x.eq_ignore_ascii_case(tt)) {
                    prev.tackled_targets.push(tt.clone());
                }
            }
            prev.killmail |= new.killmail;
            prev.wormhole |= new.wormhole;
            prev.wh_type = new.wh_type.clone().or_else(|| prev.wh_type.clone());
            prev.wh_dest = new.wh_dest.or(prev.wh_dest);
            prev.wh_eol |= new.wh_eol;
            prev.wh_drifter |= new.wh_drifter;
            prev.wh_sig = new.wh_sig.clone().or_else(|| prev.wh_sig.clone());
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
    "gate", "camp", "gatecamp", "gatecamps", "clear", "clr", "spike", "bubble", "drag", "dragbubble", "cyno", "local", "dock", "docked",
    "station", "kill", "killmail", "pod", "no", "visual", "nv", "ess", "skyhook", "hostile",
    "hostiles", "neut", "neutral", "neuts", "red", "reds", "blue", "blues", "gang", "fleet",
    "bridge", "jump", "jumping", "warp", "warping", "the", "incoming", "inc", "coming", "gcc",
    "afk", "warpin", "system", "and", "for", "status", "stat", "eyes", "any", "report", "intel", "went", "going",
    "help", "sos", "backup", "need",
    // Common English filler words that are never pilot names (kept conservative so we
    // don't drop real character names).
    "just", "is", "are", "was", "were", "be", "been", "has", "have", "had", "not", "but",
    "now", "still", "back", "with", "this", "that", "they", "them", "their", "here", "there",
    "from", "got", "off", "out", "near", "into", "onto", "over", "your", "youre", "again",
    // "rest" as in "1 jackdaw, rest NV" — never a pilot, even though a character is named "Rest".
    "rest", "stop",
    // Engagement descriptors ("good fight", "engaged on gate"). "combat" is covered above.
    "fight", "fights", "fighting", "engaged", "engage", "engaging",
    // "etc" / "etc." (the trailing dot is trimmed by the tokenizer).
    "etc",
    // "more" as in "5 more inbound".
    "more",
    // "scanning" ("someone is scanning"); "scanner" is already covered above.
    "scanning",
    // "drifter" / "drifters" — a wormhole type ("drifter wh"), never a pilot.
    "drifter", "drifters",
    // Filler / hedging words ("unsure which", "too many", "kitchen sink", "catch all").
    "unsure", "which", "too", "kitchen", "sink", "catch", "all",
    // Question / filler words — lower-cased English the known-pilot cache otherwise matches
    // against real players named like common words.
    "what", "where", "when", "who", "why", "how", "well", "anyway", "huh", "hmm", "hmmm",
    "wait", "sure", "dunno", "yes", "yeah", "yep", "yup", "nope", "nah", "ok", "okay", "kk",
    // Chat abbreviations / reactions.
    "sry", "sorry", "ty", "tyvm", "thx", "thanks", "thanx", "np", "yw", "cheers", "lol",
    "lmao", "rofl", "omg", "omw", "wtf", "wth", "ffs", "gg", "wp", "ez", "gj", "gz", "grats",
    "imo", "tbh", "idk", "ikr", "btw", "fyi", "pls", "plz", "plox", "brb", "gtg", "glhf",
    "gl", "hf", "cya", "ttyl", "sup", "yo", "o7", "07", "rip",
];

/// Whether a (sub-)name is a stop / ship-descriptor word that should never be accepted
/// as a pilot even if some character happens to share it (used to filter resolver
/// sub-span covers). Conservative so real names aren't dropped.
/// Intel keywords that name a ship *class* (not a specific hull) -> canonical class.
const SHIP_CLASSES: &[(&str, &str)] = &[
    ("dic", "Interdictor"),
    ("dics", "Interdictor"),
    ("dictor", "Interdictor"),
    ("dictors", "Interdictor"),
    ("interdictor", "Interdictor"),
    ("interdictors", "Interdictor"),
    ("hic", "Heavy Interdictor"),
    ("hics", "Heavy Interdictor"),
    ("hictor", "Heavy Interdictor"),
    ("hictors", "Heavy Interdictor"),
    ("recon", "Recon"),
    ("recons", "Recon"),
    ("bomber", "Stealth Bomber"),
    ("bombers", "Stealth Bomber"),
    ("logi", "Logistics"),
    ("logis", "Logistics"),
    ("ceptor", "Interceptor"),
    ("ceptors", "Interceptor"),
    ("hac", "Heavy Assault Cruiser"),
    ("hacs", "Heavy Assault Cruiser"),
    ("marauder", "Marauder"),
    ("marauders", "Marauder"),
    ("blops", "Black Ops"),
    // Generic hull sizes ("CRUISERS", "battleships") — a ship type, never a pilot name.
    ("frigate", "Frigate"),
    ("frigates", "Frigate"),
    ("destroyer", "Destroyer"),
    ("destroyers", "Destroyer"),
    ("cruiser", "Cruiser"),
    ("cruisers", "Cruiser"),
    ("battlecruiser", "Battlecruiser"),
    ("battlecruisers", "Battlecruiser"),
    ("bc", "Battlecruiser"),
    ("bcs", "Battlecruiser"),
    ("battleship", "Battleship"),
    ("battleships", "Battleship"),
    ("t3", "Strategic Cruiser"),
    ("t3s", "Strategic Cruiser"),
    ("t3c", "Strategic Cruiser"),
    ("t3cs", "Strategic Cruiser"),
    ("t3d", "Tactical Destroyer"),
    ("t3ds", "Tactical Destroyer"),
    ("dread", "Dreadnought"),
    ("dreads", "Dreadnought"),
    ("carrier", "Carrier"),
    ("carriers", "Carrier"),
    ("fax", "Force Auxiliary"),
    ("faxes", "Force Auxiliary"),
    ("titan", "Titan"),
    ("titans", "Titan"),
    ("super", "Supercarrier"),
    ("supers", "Supercarrier"),
];

/// Ship classes named by keyword in the message (exact token match, in order, deduped).
fn detect_classes(lower_tokens: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in lower_tokens {
        if let Some((_, class)) = SHIP_CLASSES.iter().find(|(k, _)| *k == t.as_str()) {
            if !out.iter().any(|c| c == class) {
                out.push((*class).to_owned());
            }
        }
    }
    out
}

pub fn is_pilot_stopword(w: &str) -> bool {
    let lw = w.to_lowercase();
    PILOT_STOP.contains(&lw.as_str())
        // Any ship-class keyword ("cruisers", "logi", "dic", …) is a ship type, not a pilot.
        || SHIP_CLASSES.iter().any(|(k, _)| *k == lw.as_str())
        || matches!(
            lw.as_str(),
            "ship" | "ships" | "shuttle" | "shuttles" | "navy" | "issue" | "loc"
                | "location" | "likely" | "probably" | "maybe" | "checking" | "left" | "went" | "min" | "mins" | "minute" | "minutes"
                | "heading" | "towards" | "toward" | "through" | "inbound" | "enroute"
                | "between"
                | "total" | "anchored" | "anchor" | "anchoring"
                | "bank" | "reserve" | "main"
                | "small" | "large" | "big" | "huge"
                | "sig" | "sigs" | "anyone"
                // Scanner probes are a badge, never a pilot ("Combat Probes", "Core Scanner
                // Probe", "combat prob"). The Probe frigate is still detected via the ship index.
                | "probe" | "probes" | "prob" | "probs" | "combat" | "core" | "scanner" | "sisters"
                // Alliance ticker (EVE University), not a player — even though a character
                // happens to be named "ivy".
                | "ivy"
                | "jumped" | "jumping" | "warped" | "landed" | "burning" | "aligning"
                | "incoming" | "inc" | "primary" | "killed" | "podded"
                | "wormhole" | "wormholes" | "hole" | "holes" | "wh"
                | "bubbled" | "bubbles" | "bubbling" | "cloak" | "cloaked" | "cloaky"
                | "cloaks" | "cloaking" | "decloak" | "decloaked" | "camped"
                | "ansi" | "ansiblex" | "jumpbridge" | "bridge" | "jump" | "jumps"
                | "pls" | "plz"
                | "dic" | "dics" | "dictor" | "dictors" | "interdictor" | "interdictors"
                | "hic" | "hics" | "hictor" | "hictors" | "recon" | "recons" | "bomber"
                | "bombers" | "logi" | "logis" | "ceptor" | "ceptors" | "hac" | "hacs"
                | "marauder" | "marauders" | "blops"
                | "tackled" | "tackle" | "tackling" | "takled" | "pointed" | "point"
                | "scrammed" | "scram" | "scrambled" | "webbed"
        )
}

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
/// Nullsec system codes / abbreviations ("C-J", "88A-RA", "1DH-SX"): all-uppercase
/// alphanumerics joined by a hyphen, never lower-case. Used to keep them out of pilot
/// detection (player names carry lower-case letters).
fn looks_like_system_code(t: &str) -> bool {
    if t.len() < 2 || !t.contains('-') {
        return false;
    }
    // A token starting with '-' is an alt-name suffix ("Nine -L", "Pilot -3"), not a system
    // code — real codes always have content before the first hyphen ("C-J", "78-").
    if t.starts_with('-') {
        return false;
    }
    // A null-sec system name is 5 alphanumerics + one hyphen = 6 chars. Anything longer with a
    // hyphen ("skt-10001", "Jean-Luc") is a name, not a system (abbreviations are shorter).
    if t.len() > 6 {
        return false;
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !t.chars().any(|c| c.is_ascii_alphanumeric())
    {
        return false;
    }
    // Distinguish a null-sec code from a hyphenated name. An ALL-CAPS hyphenated token is
    // always a code (names aren't typed in all-caps). Mixed/lower case is a code only if it
    // has a digit or only short (<=3 char) segments — so "c-j", "4m-", "1dq1-a" qualify but
    // a name like "Jean-Luc" does not.
    if !t.chars().any(|c| c.is_ascii_lowercase()) {
        return true;
    }
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let longest_segment = t.split('-').map(|s| s.len()).max().unwrap_or(0);
    has_digit || longest_segment <= 3
}

/// A lower/digit-leading handle ("0xtomorrow", "xX1Mortis"): contains a digit and a
/// run of at least three letters, so it is name-shaped even without a Title-case first
/// letter. Excludes system codes (hyphen, no letters) and ISK/count tokens like "334m".
/// A number glued to a time unit ("4min", "30s", "2h", "5m") — an ESS/timer duration,
/// not a name. Only leading digits count, so a handle like "0xtomorrow" is unaffected.
fn is_time_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    let Some(de) = lower.find(|c: char| !c.is_ascii_digit()) else {
        return false; // all digits, no unit
    };
    if de == 0 {
        return false; // no leading digits
    }
    matches!(
        &lower[de..],
        "min" | "mins" | "m" | "s" | "sec" | "secs" | "h" | "hr" | "hrs" | "d"
    )
}

fn is_handle_like(t: &str) -> bool {
    if looks_like_system_code(t) || is_time_token(t) || !t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut cur = 0usize;
    let mut max = 0usize;
    for c in t.chars() {
        if c.is_ascii_alphabetic() {
            cur += 1;
            max = max.max(cur);
        } else {
            cur = 0;
        }
    }
    max >= 3
}

fn is_distinctive_name(t: &str) -> bool {
    name_part(t)
        && !looks_like_system_code(t)
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
            // Refuse only a run that is entirely stop words: a single lower-cased stop word, or
            // a group made up of nothing but single-word stop words. A real (sub-)name is kept
            // even when it's lower-cased ("wuming"), so it can anchor the full name.
            let all_stop = run.split_whitespace().all(is_pilot_stopword);
            if known.contains_key(&run.to_lowercase()) && !all_stop {
                out.push(run);
                adv = len;
                break;
            }
        }
        i += adv;
    }
    out
}

/// Stop words that still appear inside real multi-word names ("The Meek", "Lord of War")
/// and so are allowed mid-name — unlike intel descriptors ("cloaked", "jumped", "camped"),
/// which are stop words that never belong in a name.
fn is_name_connector(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "the" | "of" | "and" | "for" | "von" | "van" | "de" | "del" | "di" | "da"
            | "la" | "le" | "el" | "der" | "den" | "du" | "lord"
    )
}

/// Intel keywords that are stop words on their own ("3 reds in local", "bubble up") but also
/// appear inside real character names ("Blue RandomAttac", "The Bubble Boy"). They are allowed
/// inside a loose run so the full span reaches the ESI cover, which confirms or splits it —
/// instead of breaking the run and gluing the rest into an unsplittable blob. When the span
/// becomes a pilot candidate, its tokens also suppress the matching status flag (so "The
/// Bubble Boy" no longer reads as a warp-bubble sighting).
fn is_name_capable_stopword(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "blue" | "blues" | "red" | "reds" | "bubble" | "bubbles"
    )
}

/// A short name component that can't stand alone but is valid inside a name: a single
/// capital initial ("Lopatich R") or a short number ("Adama 80", "Malcolm 41"). Only
/// ever extends a run that already has a real name word; never starts one.
fn is_name_suffix(t: &str) -> bool {
    (t.len() == 1 && t.starts_with(|c: char| c.is_ascii_uppercase()))
        || (matches!(t.len(), 1..=4) && t.chars().all(|c| c.is_ascii_digit()))
        // Alt suffix attached to a name: "-3", "-L", "-42".
        || (t.starts_with('-')
            && matches!(t.len(), 2..=4)
            && t[1..].chars().all(|c| c.is_ascii_alphanumeric()))
}

fn extract_pilots(text: &str) -> Vec<String> {
    let is_namepart = name_part;
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
        // Connector stop words are fair game inside a multi-word name ("The Meek"); a run
        // is rejected if it's all stop words OR contains an intel descriptor ("cloaked").
        if (2..=3).contains(&run.len())
            && run.iter().any(|w| !is_pilot_stopword(w))
            && !run.iter().any(|w| is_pilot_stopword(w) && !is_name_connector(w))
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

/// Runs of name-like tokens (any case) anchored by at least one Title-Case word, for
/// ESI sub-span resolution — catches lowercase names ("bigfoott Kepplet") that the
/// Title-Case heuristic misses. Ships, systems and stop words break a run. The cover
/// step later confirms/splits each run against ESI (so non-names are dropped).
fn loose_pilot_runs(
    text: &str,
    ship_index: &HashMap<String, (i64, String)>,
    systems: &Systems,
) -> Vec<String> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let mut anchored = false;
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>, anchored: &mut bool| {
        // Allow longer runs than a single name (EVE names are <=3 words): several
        // adjacent names ("Bunk Boi Bunk Helper") are one run that the ESI cover splits
        // into the real names, instead of leaking a stray sub-word.
        // Allow a long run (a whole gang listed inline) up to 20 words; the ESI cover
        // splits it into the real names. No Title-Case anchor required, so all-lowercase
        // names ("mixa kolodenko") are caught too.
        if (2..=20).contains(&run.len())
            && run.iter().any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
            && run.iter().any(|w| !is_pilot_stopword(w))
        {
            let name = run.join(" ");
            if !out.contains(&name) {
                out.push(name);
            }
        }
        run.clear();
        *anchored = false;
    };
    for raw in text.split_whitespace() {
        let core = raw.trim_matches(punct);
        let lc = core.to_lowercase();
        // EVE names allow digits ("c137"); ships/systems/stop words still break a run.
        let namelike = (core.len() >= 3 || is_name_suffix(core))
            && core.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
            // A name-capable keyword counts as a name word only when Title-cased ("Bubble"/
            // "Blue" in a name) — the lower-case form ("bubble up", "3 reds") is the keyword.
            && (!is_pilot_stopword(core)
                || is_name_connector(core)
                || (name_part(core) && is_name_capable_stopword(core)))
            && !is_cap_word(core)
            && !is_tackle_word(core)
            && !looks_like_system_code(core)
            && !is_time_token(core)
            && !is_structure_word(core)
            && !ship_index.contains_key(&lc)
            && systems.lookup(core).is_none();
        if namelike {
            // Anchor on a Title-Case word OR a distinctive one (digit / internal capital),
            // so an all-lowercase name with a code-like part ("rick c137 sancgez") still
            // forms a run.
            let distinctive = core.chars().any(|c| c.is_ascii_digit())
                || core.chars().skip(1).any(|c| c.is_ascii_uppercase());
            if name_part(core) || distinctive {
                anchored = true;
            }
            run.push(core.to_owned());
        } else {
            flush(&mut run, &mut out, &mut anchored);
        }
    }
    flush(&mut run, &mut out, &mut anchored);
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
            // The trailing "Issue" is routinely dropped ("Brutix Navy" -> Brutix Navy
            // Issue, "Stabber Fleet" -> Stabber Fleet Issue), or the whole faction suffix
            // is abbreviated ("Vexor NI" -> Navy Issue, "Stabber FI" -> Fleet Issue).
            let full = if phrase.ends_with(" navy") || phrase.ends_with(" fleet") {
                Some(format!("{phrase} issue"))
            } else if let Some(base) = phrase.strip_suffix(" ni") {
                Some(format!("{base} navy issue"))
            } else if let Some(base) = phrase.strip_suffix(" fi") {
                Some(format!("{base} fleet issue"))
            } else {
                None
            };
            if let Some(full) = full {
                if let Some((id, name)) = ship_index.get(&full) {
                    out.push((i, len, *id, name.clone()));
                    adv = len;
                    break;
                }
            }
        }
        i += adv;
    }
    out
}

/// Drop a detected name that only ever appears as the leading words of a longer detected
/// name in the same text ("Gallente Citizen" inside "Gallente Citizen 17120704"). Both can
/// be real characters, but only the longer one was actually mentioned here. A name that
/// also appears on its own (count exceeds the longer names that contain it) is kept.
pub fn drop_covered_prefixes(pilots: &[String], text: &str) -> Vec<String> {
    let toks: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let count = |phrase: &str| -> usize {
        let pw: Vec<String> = phrase.split_whitespace().map(|w| w.to_lowercase()).collect();
        if pw.is_empty() || pw.len() > toks.len() {
            return 0;
        }
        toks.windows(pw.len()).filter(|w| w.iter().eq(pw.iter())).count()
    };
    pilots
        .iter()
        .filter(|p| {
            let pl = p.to_lowercase();
            let pc = count(&pl);
            if pc == 0 {
                return true;
            }
            let covered: usize = pilots
                .iter()
                .filter(|q| {
                    let ql = q.to_lowercase();
                    ql != pl && ql.starts_with(&format!("{pl} "))
                })
                .map(|q| count(&q.to_lowercase()))
                .sum();
            pc > covered
        })
        .cloned()
        .collect()
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
            && !is_pilot_stopword(a)
            && !CLEAR_WORDS.contains(&a.to_lowercase().as_str());
        let b_ok = b.len() >= 3
            && b.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && b.chars().all(|c| c.is_ascii_alphabetic() || c == '\'')
            && !is_pilot_stopword(b)
            && !CLEAR_WORDS.contains(&b_lc.as_str())
            && !ship_index.contains_key(&b_lc);
        if a_ok && b_ok {
            out.push(format!("{a} {b}"));
        }
    }
    out
}

/// A known single-word character preceded by a plain lower-cased word is the full name relayed
/// as plain text ("ji wuming", where only "wuming" is in the cache). Grab the leading word so
/// the real name isn't truncated to its surname — as long as that word is an ordinary lower-case
/// word (not a stop word, system, ship, or a known name in its own right).
fn lowercase_known_compound(
    text: &str,
    known: &HashMap<String, i64>,
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
) -> Vec<String> {
    if known.is_empty() {
        return Vec::new();
    }
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> =
        text.split_whitespace().map(|w| w.trim_matches(punct)).filter(|w| !w.is_empty()).collect();
    let mut out = Vec::new();
    for w in words.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (a_lc, b_lc) = (a.to_lowercase(), b.to_lowercase());
        // b is a known single-word character; a is a plain lower-cased leading word.
        let ok = known.contains_key(&b_lc)
            && a.len() >= 2
            && a.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && a.chars().all(|c| c.is_ascii_alphabetic() || c == '\'')
            && !is_pilot_stopword(a)
            && !CLEAR_WORDS.contains(&a_lc.as_str())
            && resolve(systems, a).is_none()
            && !ship_index.contains_key(&a_lc)
            && !known.contains_key(&a_lc);
        if ok {
            out.push(format!("{a} {b}"));
        }
    }
    out
}

/// Analyse one message into a structured report (movement is added later).
/// Pre-clean intel text before parsing: drop EVE's "*" route-waypoint marker (so a marked
/// system like "NB-ALM*" still resolves), and strip a re-pasted chat line's
/// "[ time ] Sender > " prefix when the body is an in-game-link paste (the inner sender is
/// not a hostile).
/// Drop a pilot that is a contiguous sub-phrase of a longer one (e.g. "Nine" when "Nine -3"
/// is also present) — used after a merge, since each message is filtered individually.
/// A `protect`ed name (one with an authoritative showinfo char-id) is never dropped: a
/// glued mis-parse from a plain-text relay must not evict the real, char-linked name.
fn drop_subphrase_pilots(pilots: &mut Vec<String>, protect: &std::collections::HashSet<String>) {
    let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
    let keep: Vec<bool> = (0..pilots.len())
        .map(|i| {
            protect.contains(&lc[i])
                || !lc.iter().enumerate().any(|(j, o)| {
                    j != i
                        && o.len() > lc[i].len()
                        && format!(" {o} ").contains(&format!(" {} ", lc[i]))
                })
        })
        .collect();
    let mut it = keep.into_iter();
    pilots.retain(|_| it.next().unwrap_or(true));
}

fn preprocess_intel(text: &str) -> String {
    let mut t = text.trim();
    if t.starts_with('[') {
        if let Some(i) = t.find(']') {
            t = t[i + 1..].trim_start();
        }
    }
    if let Some(gt) = t.find(" > ") {
        let prefix = &t[..gt];
        let rest = t[gt + 3..].trim_start();
        if !prefix.contains("<url=")
            && prefix.split_whitespace().count() <= 4
            && rest.starts_with("<url=")
        {
            t = rest;
        }
    }
    t.replace('*', "")
}

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
                // (30M). A character is identified by BOTH its bloodline typeID
                // (1373–1390) and its itemID range — so a newer bloodline whose typeID
                // we don't list is still caught by its id (chars: 90–98M and the modern
                // 2.10–2.147B range), authoritative even when the name is also a system.
                let is_char_id = (90_000_000..98_000_000).contains(&item_id)
                    || (2_100_000_000..=2_147_483_647).contains(&item_id);
                if (50_000_000..60_000_000).contains(&item_id) {
                    // A stargate link — its name is the destination system: a gate.
                    t.gates.push(inner.to_owned());
                } else if type_id == 5 {
                    // EVE appends "*" to a system name that's set as a route waypoint; strip it
                    // (and any trailing space) so the system still resolves.
                    t.systems.push(inner.trim_end_matches('*').trim().to_owned());
                } else if (1373..=1390).contains(&type_id) || is_char_id {
                    t.pilots.push(inner.to_owned());
                    t.char_ids.push((inner.to_owned(), item_id));
                } else if let Some((id, name)) = ship_index.get(&inner.to_lowercase()) {
                    t.ships.push((*id, name.clone()));
                } else if resolve(systems, inner).is_some() {
                    t.systems.push(inner.to_owned());
                } else if let Some((pilot, ship)) = split_pilot_ship(inner, ship_index) {
                    // A "Pilot (Ship)" killmail/fitting link (showinfo on the ship type) — the
                    // name is the pilot + their hull, not a pilot literally called "X (Hull)".
                    t.pilots.push(pilot.to_owned());
                    t.ships.push(ship);
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

/// Split a "Pilot (Hull)" link display into the pilot name + resolved ship, when the
/// parenthesised part is a known hull (killmail / fitting links use this form).
fn split_pilot_ship<'a>(
    inner: &'a str,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
) -> Option<(&'a str, (i64, String))> {
    let (name, rest) = inner.rsplit_once(" (")?;
    let hull = rest.strip_suffix(')')?;
    let ship = ship_index.get(&hull.to_lowercase())?;
    let name = name.trim();
    (!name.is_empty()).then(|| (name, ship.clone()))
}

#[allow(dead_code)] // thin no-context wrapper, kept for the public API + tests
pub fn analyze(
    text: &str,
    systems: &Systems,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
    known_pilots: &std::collections::HashMap<String, i64>,
    received: i64,
    channel: &str,
    reporter: &str,
) -> IntelReport {
    analyze_ctx(text, systems, ship_index, known_pilots, received, channel, reporter, None, &[])
}

/// As [`analyze`], but with the channel's last-known system as context so an
/// abbreviated gate ("C-J gate") can disambiguate against that system's neighbours
/// even when the message doesn't restate a system.
#[allow(clippy::too_many_arguments)]
/// Localised "Kill:" prefixes from the in-game killReport link text. EVE doesn't write
/// the `<url=killReport...>` wrapper to the chat log, so a kill is detected from the
/// visible (localised) word, not the URL.
const KILL_WORDS: &[&str] = &[
    "kill:",       // English
    "击杀", // Chinese - kill
    "损失", // Chinese - loss
    "キル", // Japanese - kill
    "킬",       // Korean - kill
    "abschuss",     // German
    "убийство", // Russian - kill
];

/// Extract the EVE region names a channel covers from its MOTD
/// ("Channel MOTD: TENERIFIS // IMMENSEA // …"). Only names matching a known region are
/// kept, so the trailing prose is ignored. The result is a hint for abbreviation
/// disambiguation, not an absolute filter.
pub fn parse_motd_regions(motd: &str, known: &std::collections::HashSet<String>) -> Vec<String> {
    // Scan from the MOTD marker onward (skip the log header) for *any* known region
    // name, regardless of separators or line breaks. It's only a disambiguation hint, so
    // being inclusive is fine. A boundary check (not preceded by a letter, not followed
    // by a lower-case letter) keeps "Catch" out of the middle of words while still
    // catching "CATCHPlease" where regions are glued to the trailing prose.
    let body = match motd.rfind("Channel MOTD:") {
        Some(i) => &motd[i + "Channel MOTD:".len()..],
        None => motd,
    };
    let lc = body.to_lowercase();
    let lcb = lc.as_bytes();
    let orig = body.as_bytes();
    let mut hits: Vec<(usize, &String)> = Vec::new();
    for region in known {
        let r = region.as_str();
        let mut from = 0;
        while let Some(rel) = lc[from..].find(r) {
            let at = from + rel;
            let before_ok = at == 0 || !(lcb[at - 1] as char).is_ascii_alphabetic();
            let after = at + r.len();
            let after_ok = after >= orig.len() || !(orig[after] as char).is_ascii_lowercase();
            if before_ok && after_ok {
                hits.push((at, region));
                break;
            }
            from = at + 1;
        }
    }
    hits.sort_by_key(|(pos, _)| *pos);
    let mut out: Vec<String> = Vec::new();
    for (_, r) in hits {
        if !out.contains(r) {
            out.push(r.clone());
        }
    }
    out
}
pub fn analyze_ctx(
    text: &str,
    systems: &Systems,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
    known_pilots: &std::collections::HashMap<String, i64>,
    received: i64,
    channel: &str,
    reporter: &str,
    context_system: Option<i64>,
    channel_regions: &[String],
) -> IntelReport {
    let cleaned = preprocess_intel(text);
    let text = cleaned.as_str();
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
    for k in match_known_pilots(&masked, known_pilots) {
        // A standalone word that's a known ship is the ship ("Buzzard"); a null-sec
        // code is the system, not a player who happens to be named like it ("C-J").
        if (!k.contains(' ') && ship_index.contains_key(&k.to_lowercase()))
            || looks_like_system_code(&k)
            || is_time_token(&k)
            || is_structure_word(&k)
            // A code-shaped null-sec prefix ("88a" → 88A-RA) is the system, not a player
            // who happens to share that name.
            || (!k.contains(' ')
                && systems.lookup_prefix(&k).is_some_and(|s| looks_like_system_code(&s.name)))
        {
            continue;
        }
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&k)) {
            pilots.push(k);
        }
    }
    // A plain-text-relayed full name whose surname is the only cached part ("ji wuming") — grab
    // the leading word so the sub-name added above doesn't stand alone (the subphrase pass then
    // drops the bare surname in favour of the full name).
    for n in lowercase_known_compound(&masked, known_pilots, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
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
            && !is_pilot_stopword(t)
            && !ship_index.contains_key(&t.to_lowercase())
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
    // A wormhole signature code (e.g. "K162") is never part of a pilot name.
    pilots.retain(|p| !p.split_whitespace().any(crate::wormholes::is_wh_code));
    // Loose runs (any-case, anchored by a Title-Case word) — added AFTER the sub-phrase
    // filter so they don't swallow the strict shorter names; the ESI cover confirms or
    // splits each later (non-names are dropped).
    for r in loose_pilot_runs(&masked, ship_index, systems) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            pilots.push(r);
        }
    }
    // Single Title-Case tokens (plain-text logs carry no char links) — queued for ESI so
    // standalone names like "Sevra" are recognised. Uses the ship/paren-masked tokens so
    // a multi-word hull's words ("Comet" in "Federation Navy Comet") aren't read as names.
    let masked_tokens = tokenize(&masked);
    for t in &masked_tokens {
        let lc = t.to_lowercase();
        if (name_part(t) || is_handle_like(t))
            && t.len() >= 3
            && !is_pilot_stopword(t)
            && !looks_like_system_code(t)
            && !CLEAR_WORDS.contains(&lc.as_str())
            && ship_index.get(&lc).is_none()
            && resolve(systems, t).is_none()
            && !crate::wormholes::is_wh_code(t)
            && !pilots.iter().any(|p| p.split_whitespace().any(|w| w.eq_ignore_ascii_case(t)))
        {
            pilots.push((*t).to_owned());
        }
    }
    // A structure name (Keepstar, Fortizar, …) is never a pilot, even if a character is
    // named after one — it's reported as a structure badge, not a player.
    pilots.retain(|p| !is_structure_word(p));
    // Final pass: drop any pilot that is a contiguous sub-phrase of a longer detected one.
    // The loose-run and single-token sources are added after the earlier sub-phrase filter,
    // so a short span the longer name already covers ("Chen Chen" inside "Dr Chen Chen",
    // produced because the loose run breaks on the 2-char "Dr") can slip through.
    let char_linked: std::collections::HashSet<String> =
        si_char_ids.iter().map(|(n, _)| n.to_lowercase()).collect();
    drop_subphrase_pilots(&mut pilots, &char_linked);

    let pilot_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();

    // Wormhole signature code (e.g. "K162") named anywhere in the message.
    let wh_code =
        tokens.iter().find(|t| crate::wormholes::is_wh_code(t)).map(|t| t.to_uppercase());

    // Extra wormhole detail, parsed only for an actual wormhole sighting.
    let is_wh_msg = lower.contains("wormhole")
        || wh_code.is_some()
        || lower_tokens.iter().any(|t| {
            matches!(t.as_str(), "wh" | "hole" | "holes" | "thera" | "turnur")
                && !pilot_tokens.contains(t)
        });
    let (wh_dest, wh_eol, wh_drifter, wh_sig) = if is_wh_msg {
        (
            parse_wh_dest(&lower, &lower_tokens),
            lower.contains("eol") || lower.contains("end of life") || lower.contains("dying"),
            lower.contains("drifter"),
            tokens.iter().find(|t| looks_like_sig(t)).map(|t| t.to_uppercase()),
        )
    } else {
        (None, false, false, None)
    };

    // Ships: hull names / nicknames / acronyms (case-insensitive), or an unambiguous
    // typo. A token that belongs to a pilot name is never also parsed as a ship.
    let mut ships: Vec<DetectedShip> = Vec::new();
    let add_ship = |id: i64, name: &str, ships: &mut Vec<DetectedShip>| {
        if !ships.iter().any(|s| s.id == id) {
            ships.push(DetectedShip { id, name: name.to_owned() });
        }
    };
    // Words that belong to a detected multi-word hull ("Catalyst" in "Catalyst Navy
    // Issue") must not also be read as a standalone ship (double-counting the hull).
    let mw_words: std::collections::HashSet<String> = mw_ships
        .iter()
        .flat_map(|(_, _, _, name)| {
            name.to_lowercase().split_whitespace().map(str::to_owned).collect::<Vec<_>>()
        })
        .collect();
    for tok in &tokens {
        let lower = tok.to_lowercase();
        if pilot_tokens.contains(&lower) || mw_words.contains(&lower) {
            continue;
        }
        // "shuttle(s)" with no specific hull → default to the Caldari Shuttle (672).
        if matches!(lower.as_str(), "shuttle" | "shuttles") {
            add_ship(672, "Caldari Shuttle", &mut ships);
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

    // Standalone null-sec abbreviation ("C-J" when both C-J6MT and C-J7CR exist, with
    // no "gate" word): resolve by prefix against an already-named system's neighbours or
    // the channel context. Prefix-only and hyphenated-code-only, so plain words and
    // suffixes ("6MT") never match.
    {
        let ctx: Vec<i64> = detected.iter().map(|d| d.id).chain(context_system).collect();
        for (i, tok) in tokens.iter().enumerate() {
            let lc = tok.to_lowercase();
            if consumed.contains(&lc)
                || pilot_tokens.contains(&lc)
                || !looks_like_system_code(tok)
                || resolve(systems, tok).is_some()
                || tokens.get(i + 1).is_some_and(|n| n.eq_ignore_ascii_case("gate"))
            {
                continue;
            }
            let hit = ctx
                .iter()
                .find_map(|&c| {
                    systems.neighbors(c).iter().find_map(|&n| {
                        systems.info_of(n).filter(|info| info.name.to_lowercase().starts_with(&lc))
                    })
                })
                // Hint: an Imperium channel's regions (from its MOTD) pick C-J6MT over the
                // Vale of the Silent C-J7CR even with no nearby named system.
                .or_else(|| systems.lookup_prefix_in_regions(tok, channel_regions));
            if let Some(info) = hit {
                let (id, name, security) = (info.id, info.name.clone(), info.security);
                consumed.push(lc);
                if !detected.iter().any(|d| d.id == id) {
                    detected.push(DetectedSystem { id, name, security });
                }
            }
        }
    }

    // Gate: "... <System> gate" — hostiles are on the gate *to* <System>. Record it
    // (resolved name, or the raw token if abbreviated/unknown) and don't also list
    // it as a plain system.
    let mut gate: Option<String> = None;
    // Prefer a system named in this message; otherwise fall back to the channel's
    // last-known system so a bare "C-J gate" still resolves against its neighbours.
    let primary = detected.first().map(|d| d.id).or(context_system);
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
        let resolved = resolve(systems, cand)
            .or_else(|| {
                // The gate leads to a *neighbour* of the report's system, and there are
                // only a handful — so even a 1–2 char prefix is unambiguous: "C-J6MT >
                // 5e gate" → 5E-CFL. (A bare number is a hostile count, not a name.)
                if cand.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                let lc = cand.to_lowercase();
                primary.and_then(|p| {
                    systems.neighbors(p).iter().find_map(|&nid| {
                        systems.info_of(nid).filter(|i| i.name.to_lowercase().starts_with(&lc))
                    })
                })
            })
            .or_else(|| {
                // Still nothing: accept an unambiguous global abbreviation (e.g. "YPW").
                let abbrev = cand.len() >= 2
                    && cand.chars().all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '-');
                if abbrev { systems.lookup_prefix(cand) } else { None }
            });
        // "O3-4MN Gate camp" is the "gate camp" keyword, not "O3-4MN gate": don't demote
        // the report's own system to a gate when "gate" is immediately followed by "camp".
        if resolved.is_some()
            && resolved.map(|s| s.id) == primary
            && tokens.get(i + 1).is_some_and(|n| n.eq_ignore_ascii_case("camp"))
        {
            continue;
        }
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

    // "Ansi"/"Ansiblex" = the Ansiblex jump bridge in the report's system; treat it as
    // the gate it leads to (the configured bridge's destination), so "camp on the Ansi"
    // points at the system the bridge reaches.
    if gate.is_none() && lower_tokens.iter().any(|t| t == "ansi" || t == "ansiblex") {
        if let Some(dest) = primary.and_then(|p| systems.jump_bridge_dest(p)) {
            detected.retain(|d| d.id != dest.id);
            gate = Some(dest.name.clone());
        }
    }

    // One system per card: keep the first mentioned, and demote any further system
    // mentions to gates (a report can list several gates). Combined with any explicit
    // "<X> gate" already found above.
    let mut gates: Vec<String> = Vec::new();
    if let Some(g) = gate {
        gates.push(g);
    }
    if detected.len() > 1 {
        for d in detected.split_off(1) {
            if !gates.iter().any(|g| g.eq_ignore_ascii_case(&d.name)) {
                gates.push(d.name);
            }
        }
    }

    // Alliance shorthands ("frat", "init", …) → logos on the card.
    let mut alliances: Vec<(String, i64)> = Vec::new();
    for t in &lower_tokens {
        if let Some((name, id)) = crate::alliances::lookup(t) {
            if !alliances.iter().any(|(_, i)| *i == id) {
                alliances.push((name.to_owned(), id));
            }
        }
    }

    // A run whose every word is a known ship is ships, not a pilot ("Sabre Orthrus");
    // move it to the ship list (a coincidental same-named character loses).
    let mut reclassified: Vec<DetectedShip> = Vec::new();
    let pilots: Vec<String> = pilots
        .into_iter()
        .filter(|pn| {
            let words: Vec<&str> = pn.split_whitespace().collect();
            if !words.is_empty() && words.iter().all(|w| ship_index.contains_key(&w.to_lowercase())) {
                for w in &words {
                    if let Some((id, name)) = ship_index.get(&w.to_lowercase()) {
                        if !ships.iter().any(|sh| sh.id == *id)
                            && !reclassified.iter().any(|sh| sh.id == *id)
                        {
                            reclassified.push(DetectedShip { id: *id, name: name.clone() });
                        }
                    }
                }
                false
            } else {
                true
            }
        })
        .collect();
    ships.extend(reclassified);

    // Scanning probes (Core/Combat Scanner Probe items + slang) are reported as a badge,
    // never as the Probe frigate — drop the frigate so it isn't double-detected.
    let probes = detect_probes(text);
    if probes.is_some() {
        ships.retain(|s| !s.name.eq_ignore_ascii_case("Probe"));
    }

    let classes = detect_classes(&lower_tokens);
    let (mut tackled, tackled_targets) = detect_tackle(&lower_tokens, &pilot_tokens, ship_index);
    // Best-guess Chinese tackle/point/web terms (not seen in current logs — a safety net).
    tackled |= lower.contains("抓") || lower.contains("点住") || lower.contains("网住");

    // Celestial locations ("planet 1", "moon IV", "sun"): their word + number are consumed so
    // they aren't read as a hostile count or a pilot. Uses the raw split (tokenize drops bare
    // numbers, which are exactly the celestial index).
    let raw_tokens: Vec<&str> = text.split_whitespace().collect();
    let (celestials, celestial_consumed) = detect_celestials(&raw_tokens);
    consumed.extend(celestial_consumed);

    let mut pilots = drop_covered_prefixes(&pilots, text);
    // A single token consumed as a system or gate — including a lower-case null-sec code
    // like "c-j" in "c-j gate" — is never also a pilot.
    pilots.retain(|p| p.contains(' ') || !consumed.contains(&p.to_lowercase()));
    // Dedupe (case-insensitive) so the same name repeated — in one message or across merged
    // re-posts — never inflates the hostile count ("X X X" is one hostile, not three).
    {
        let mut seen = std::collections::HashSet::new();
        pilots.retain(|p| seen.insert(p.to_lowercase()));
    }
    let (total_count, plus_count, name_number_skips) =
        parse_count(text, &consumed, systems, ship_index);
    // "pilot1 pilot2 +20" = 22; a stated total ("7 reds") wins; otherwise a bare list of
    // 3+ named pilots reports its own count. Fewer than 3 named with no number = no badge.
    let named = pilots.len() as u32;
    let count = if let Some(t) = total_count {
        Some((t + plus_count).min(999)) // a stated number ("3 Drake +2" = 5)
    } else if plus_count > 0 {
        Some((named + plus_count).min(999)) // named pilots + "N more" ("a b +20" = 22)
    } else if named >= 3 {
        Some(named) // a bare list of 3+ named pilots
    } else {
        None
    };
    let ess_ctx = lower_tokens.iter().any(|t| t == "ess" && !pilot_tokens.contains(t));
    let isk = parse_isk(text, ess_ctx);
    let structures = detect_structures(text);
    IntelReport {
        id: 0, // assigned by IntelState::push
        probes,
        received,
        channel: channel.to_owned(),
        reporter: reporter.to_owned(),
        text: display_text,
        pilots,
        char_ids: si_char_ids,
        systems: detected,
        ships,
        classes,
        count,
        name_number_skips,
        isk,
        structures,
        celestials,
        // Status keywords ignore words that belong to a pilot-name run, so a pilot
        // named e.g. "Clear Skies" can't spoof a "clear" status.
        clear: lower_tokens
            .iter()
            .any(|t| CLEAR_WORDS.contains(&t.as_str()) && !pilot_tokens.contains(t)),
        status: lower_tokens
            .iter()
            .any(|t| matches!(t.as_str(), "status" | "stat" | "eyes") && !pilot_tokens.contains(t)),
        no_visual: lower_tokens.iter().any(|t| t == "nv" && !pilot_tokens.contains(t))
            || lower.contains("no visual"),
        spike: flagged(&lower_tokens, &pilot_tokens, &["spike"]),
        camp: flagged(&lower_tokens, &pilot_tokens, &["camp", "gatecamp"]) || lower.contains("蹲"),
        help: flagged_exact(&lower_tokens, &pilot_tokens, &["help", "sos"])
            || lower.contains("need backup")
            || lower.contains("needs backup")
            || lower.contains("求救")
            || lower.contains("求助"),
        bubble: flagged(&lower_tokens, &pilot_tokens, &["bubble", "drag"]) || lower.contains("泡泡") || lower.contains("气泡"),
        killmail: links.iter().any(|l| l.kind == LinkKind::Killmail)
            || KILL_WORDS.iter().any(|w| lower.contains(w)),
        cyno: flagged_exact(&lower_tokens, &pilot_tokens, &["cyno", "cynos"]) || lower.contains("诱导") || lower.contains("诱饵"),
        dropper: flagged_exact(
            &lower_tokens,
            &pilot_tokens,
            &[
                "dropper", "droppers", "hotdrop", "hotdrops", "hotdropper", "hotdroppers",
                "blops", "blackops", "blackop",
            ],
        ) || lower.contains("hot drop")
            || lower.contains("hot dropper")
            || lower.contains("black ops"),
        cap_tackled: detect_cap_tackled(&lower_tokens, &pilot_tokens),
        tackled,
        tackled_targets,
        wormhole: is_wh_msg,
        wh_type: wh_code,
        wh_dest,
        wh_eol,
        wh_drifter,
        wh_sig,
        ess: ess_ctx,
        // The ESS hack timer maxes at 6 min for the main bank, 45 min for the
        // reserve. A larger "Xm" is an ISK amount (e.g. "77m bank"), not a time.
        ess_time: if lower.contains("ess") {
            let max = if lower.contains("reserve") { 45 } else { 6 };
            parse_time_left(text, max)
        } else {
            None
        },
        skyhook: lower.contains("skyhook"),
        gates,
        alliances,
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

/// Any ship/type reported tackled. Returns whether a tackle word appears and the
/// ship/class names immediately preceding one (for the "<ship> TACKLED" badge).
fn detect_tackle(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
    ship_index: &HashMap<String, (i64, String)>,
) -> (bool, Vec<String>) {
    let mut any = false;
    let mut targets: Vec<String> = Vec::new();
    for i in 0..lower_tokens.len() {
        let t = lower_tokens[i].as_str();
        if is_tackle_word(t) && !pilot_tokens.contains(&lower_tokens[i]) {
            any = true;
            if i > 0 {
                let prev = lower_tokens[i - 1].as_str();
                let name = ship_index.get(prev).map(|(_, n)| n.clone()).or_else(|| {
                    SHIP_CLASSES.iter().find(|(k, _)| *k == prev).map(|(_, c)| (*c).to_owned())
                });
                if let Some(n) = name {
                    if !targets.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                        targets.push(n);
                    }
                }
            }
        }
    }
    (any, targets)
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

/// Like `flagged` but requires an exact token match, for short keywords whose prefix
/// collides with names/ships ("help" in "Helper", "cyno" in the ship "Cynabal").
fn flagged_exact(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
    words: &[&str],
) -> bool {
    const NEG: &[&str] = &["no", "not", "without", "n0", "negative"];
    lower_tokens.iter().enumerate().any(|(i, t)| {
        words.contains(&t.as_str())
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
/// An approximate ISK amount posted in intel ("300kk", "1.5b", "300 mil", "300 million"),
/// returned in ISK. "kk" is the EVE shorthand for millions. Returns the largest match.
/// EVE structures + their common in-game abbreviations (verified against player usage,
/// not invented). Single-word entries match whole tokens; two-word entries match phrases.
const STRUCTURES: &[(&str, &str)] = &[
    // Citadels
    ("keepstar", "Keepstar"), ("keep", "Keepstar"), ("ks", "Keepstar"),
    ("fortizar", "Fortizar"), ("fort", "Fortizar"),
    ("astrahus", "Astrahus"), ("astra", "Astrahus"),
    // Engineering complexes
    ("raitaru", "Raitaru"), ("azbel", "Azbel"), ("sotiyo", "Sotiyo"),
    // Refineries
    ("athanor", "Athanor"), ("tatara", "Tatara"),
    // Navigation / cyno
    ("ansiblex", "Ansiblex"), ("ansi", "Ansiblex"),
    ("tenebrex", "Cyno Jammer"), ("cyno jammer", "Cyno Jammer"),
    ("pharolux", "Cyno Beacon"), ("cyno beacon", "Cyno Beacon"),
    // Player-owned starbase (control tower)
    ("pos", "POS"),
    // Planetary / Equinox
    ("poco", "POCO"),
    ("skyhook", "Skyhook"),
    ("metenox", "Metenox"), ("moon drill", "Metenox"),
    ("mercenary den", "Mercenary Den"), ("merc den", "Mercenary Den"),
    ("sovereignty hub", "Sov Hub"), ("sov hub", "Sov Hub"),
];

/// Whether a single lower-case token names a structure (so it isn't read as a pilot).
fn is_structure_word(t: &str) -> bool {
    let lw = t.to_lowercase();
    STRUCTURES.iter().any(|(m, _)| !m.contains(' ') && *m == lw.as_str())
}

/// A distance off a structure: "500km", "2au"/"2AU", or a bare number followed by
/// "off"/"km"/"au".
fn parse_distance(word: &str, next: Option<&str>) -> Option<String> {
    match word.find(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(de) if de > 0 => match &word[de..] {
            "km" => Some(format!("{}km", &word[..de])),
            "au" => Some(format!("{}AU", &word[..de])),
            _ => None,
        },
        None if !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) => match next {
            Some("off") | Some("km") => Some(format!("{word}km")),
            Some("au") => Some(format!("{word}AU")),
            _ => None,
        },
        _ => None,
    }
}

/// Structures mentioned in the message, each with an optional distance off it
/// ("Keepstar 500km", "Astrahus 2AU").
/// Scanning probes — Core/Combat Scanner Probe items (incl. Sisters/RSS/Satori-Horigu) and
/// the "core/combat probes" slang — as a badge label, distinct from the Probe frigate. A
/// lone "probe" (no Core/Combat/scanner qualifier) is the ship, so returns None.
/// Celestial locations named in intel: "planet"/"moon" + an arabic number or roman numeral
/// ("planet 1", "moon IV"), and a standalone "sun". Returns the display labels plus the
/// tokens consumed (the celestial word + its number) so they aren't read as a hostile count
/// or a pilot.
fn detect_celestials(tokens: &[&str]) -> (Vec<String>, Vec<String>) {
    let is_roman = |t: &str| {
        (1..=5).contains(&t.len())
            && t.chars().all(|c| matches!(c.to_ascii_uppercase(), 'I' | 'V' | 'X'))
    };
    let mut labels: Vec<String> = Vec::new();
    let mut consumed: Vec<String> = Vec::new();
    let push = |label: String, labels: &mut Vec<String>| {
        if !labels.iter().any(|l| l.eq_ignore_ascii_case(&label)) {
            labels.push(label);
        }
    };
    let mut i = 0;
    while i < tokens.len() {
        let w = tokens[i].trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_lowercase();
        let kind = match w.as_str() {
            "planet" | "planets" => Some("Planet"),
            "moon" | "moons" => Some("Moon"),
            _ => None,
        };
        if let Some(k) = kind {
            let n = tokens
                .get(i + 1)
                .map(|t| t.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
                .unwrap_or("");
            if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
                push(format!("{k} {n}"), &mut labels);
                consumed.push(w);
                consumed.push(n.to_lowercase());
                i += 2;
                continue;
            } else if is_roman(n) {
                push(format!("{k} {}", n.to_uppercase()), &mut labels);
                consumed.push(w);
                i += 2;
                continue;
            }
        } else if w == "sun" {
            push("Sun".to_string(), &mut labels);
            consumed.push(w);
        }
        i += 1;
    }
    (labels, consumed)
}

fn detect_probes(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    // Match the "prob" stem so abbreviations like "combat prob" count too.
    let core = lower.contains("core scanner") || lower.contains("core prob");
    let combat = lower.contains("combat scanner") || lower.contains("combat prob");
    match (core, combat) {
        (true, false) => Some("Core Probes"),
        (false, true) => Some("Combat Probes"),
        (true, true) => Some("Probes"),
        (false, false) => {
            // A bare "prob" is shorthand for "probably", not scanning probes — only the
            // unambiguous "probes" (or a qualified "scanner/core/combat prob") counts.
            let bare = lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| matches!(w, "probes" | "probs"));
            (lower.contains("scanner prob") || bare).then_some("Probes")
        }
    }
}

fn detect_structures(text: &str) -> Vec<(String, Option<String>)> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').to_lowercase())
        .collect();
    let dists: Vec<(usize, String)> = words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| parse_distance(w, words.get(i + 1).map(|s| s.as_str())).map(|d| (i, d)))
        .collect();
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut hit: Option<(usize, String)> = None;
        for len in (1..=2).rev() {
            if i + len > words.len() {
                continue;
            }
            let phrase = words[i..i + len].join(" ");
            if let Some(canon) =
                STRUCTURES.iter().find(|(m, _)| *m == phrase.as_str()).map(|(_, c)| c.to_string())
            {
                hit = Some((len, canon));
                break;
            }
        }
        if let Some((len, canon)) = hit {
            let near = dists
                .iter()
                .filter(|(di, _)| (*di as isize - i as isize).abs() <= 4)
                .min_by_key(|(di, _)| (*di as isize - i as isize).unsigned_abs())
                .map(|(_, d)| d.clone());
            match out.iter_mut().find(|(n, _)| *n == canon) {
                Some(e) => {
                    if e.1.is_none() {
                        e.1 = near;
                    }
                }
                None => out.push((canon, near)),
            }
            i += len;
        } else {
            i += 1;
        }
    }
    out
}

fn parse_isk(text: &str, ess: bool) -> Option<u64> {
    let mult = |s: &str| -> Option<f64> {
        match s {
            "k" => Some(1e3),
            "kk" | "mil" | "mill" | "million" | "millions" => Some(1e6),
            // Bare "m"/"M" collides with null-sec system shorthands ("4M-", "4M-HGW"), so
            // only read it as millions when an ESS amount is being discussed.
            "m" if ess => Some(1e6),
            "b" | "bil" | "bill" | "billion" | "billions" => Some(1e9),
            _ => None,
        }
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut best: Option<u64> = None;
    for (i, w) in words.iter().enumerate() {
        // A hyphenated token is a null-sec system code ("4M-", "4M-HGW"), never ISK.
        if w.contains('-') {
            continue;
        }
        let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
        let split = w.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(w.len());
        let (num, suf) = w.split_at(split);
        let Ok(n) = num.parse::<f64>() else { continue };
        if !n.is_finite() || n <= 0.0 {
            continue;
        }
        let m = if !suf.is_empty() {
            mult(&suf.to_lowercase())
        } else {
            words
                .get(i + 1)
                .and_then(|nx| mult(&nx.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()))
        };
        if let Some(m) = m {
            let isk = (n * m) as u64;
            if best.map_or(true, |b| isk > b) {
                best = Some(isk);
            }
        }
    }
    best
}

/// Compact ISK display ("300M", "1.5B", "750K").
pub fn format_isk(isk: u64) -> String {
    if isk >= 1_000_000_000 {
        format!("{:.1}B", isk as f64 / 1e9)
    } else if isk >= 1_000_000 {
        format!("{:.0}M", isk as f64 / 1e6)
    } else if isk >= 1_000 {
        format!("{:.0}K", isk as f64 / 1e3)
    } else {
        isk.to_string()
    }
}

fn parse_count(
    text: &str,
    consumed: &[String],
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
) -> (Option<u32>, u32, Vec<(String, u32)>) {
    let mut name_skips: Vec<(String, u32)> = Vec::new();
    // A bare number directly before one of these is an ISK/quantity amount ("334
    // million"), not a hostile count.
    const MAGNITUDE: &[&str] =
        &["m", "mil", "mill", "million", "millions", "b", "bil", "billion", "k", "isk"];
    let mut best: Option<u32> = None;
    let mut plus: u32 = 0;
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, raw) in words.iter().enumerate() {
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
        // A *bare* number right after a pilot name is part of the name ("Adama 80",
        // "Malcolm 41"), not a count. Decorated "+2"/"x2" is always a count, and a number
        // after a *system* ("Rancer 80") still counts.
        if bare_number && i > 0 {
            let prev = words[i - 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-');
            let plc = prev.to_lowercase();
            if name_part(prev) && systems.lookup(prev).is_none() && !ship_index.contains_key(&plc) {
                if let Ok(n) = digits.parse::<u32>() {
                    name_skips.push((format!("{prev} {digits}"), n));
                }
                continue;
            }
        }
        if bare_number && !decorated {
            // A bare number consumed as a system/gate is not a count.
            if consumed.iter().any(|c| c == &t.to_lowercase()) {
                continue;
            }
            // A bare number before a magnitude word is an ISK amount, not a count.
            if let Some(next) = words.get(i + 1) {
                let n = next.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                if MAGNITUDE.contains(&n.as_str()) {
                    continue;
                }
            }
        }
        if let Ok(n) = digits.parse::<u32>() {
            if (1..=999).contains(&n) {
                // A leading/trailing "+" means N *more* besides the named pilots; keep it
                // separate so the caller can add it to the pilot count. Other tokens are a
                // stated total (summed: "7 red; 1 neut" -> 8).
                if t.starts_with('+') || t.ends_with('+') {
                    plus = (plus + n).min(999);
                } else {
                    best = Some(best.map_or(n, |b| (b + n).min(999)));
                }
            }
        }
    }
    (best, plus, name_skips)
}

/// Split into candidate tokens, keeping `-` and `'` (used in system/char names).
fn tokenize(text: &str) -> Vec<&str> {
    // Apostrophes are kept (O'Brien), but a leading/trailing one is stray punctuation
    // ("PeshyHod'" is the character "PeshyHod"), so trim it off the token ends.
    text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\''))
        .map(|t| t.trim_matches('\''))
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Destination class guessed from a wormhole message's text. Only a guess — the
/// wormhole *type's* own class (and EVE-Scout data) override it. None if no class
/// keyword is present.
fn parse_wh_dest(lower: &str, lower_tokens: &[String]) -> Option<crate::wormholes::DestClass> {
    use crate::wormholes::DestClass;
    let has = |w: &str| lower_tokens.iter().any(|t| t == w);
    if lower.contains("thera") {
        Some(DestClass::Thera)
    } else if lower.contains("turnur") {
        Some(DestClass::Turnur)
    } else if lower.contains("highsec") || lower.contains("hisec") || has("hs") {
        Some(DestClass::Highsec)
    } else if lower.contains("lowsec") || lower.contains("losec") || has("ls") {
        Some(DestClass::Lowsec)
    } else if lower.contains("nullsec") || lower.contains("0.0") || has("ns") || has("null") {
        Some(DestClass::Nullsec)
    } else if lower.contains("wspace")
        || lower.contains("w-space")
        || lower.contains("jspace")
        || lower.contains("j-space")
        || lower_tokens
            .iter()
            .any(|t| t.len() >= 2 && t.starts_with('c') && t[1..].bytes().all(|c| c.is_ascii_digit()))
    {
        Some(DestClass::Wspace)
    } else {
        None
    }
}

/// An EVE cosmic-signature id, "ABC-123".
fn looks_like_sig(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 7
        && b[3] == b'-'
        && b[..3].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit)
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
            ("eimj-m", "EIMJ-M", 30004946, -0.4),
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
    fn lowercase_full_name_not_truncated_to_surname() {
        let s = systems();
        // In-game paste with showinfo tags: the char link is authoritative.
        let txt = "<url=showinfo:1373//2112339969>ji wuming</url>  <url=showinfo:5//30004946>EIMJ-M</url>";
        let mut known = noknown();
        known.insert("wuming".into(), 999); // the surname is itself a real character
        let r = analyze(txt, &s, &noships(), &known, 1, "ch", "Death Eater 101");
        assert_eq!(r.pilots, vec!["ji wuming".to_string()], "got {:?}", r.pilots);
        // Plain-text relay with the full name cached.
        let mut known2 = noknown();
        known2.insert("ji wuming".into(), 2112339969);
        known2.insert("wuming".into(), 999);
        let r2 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known2, 1, "ch", "x");
        assert_eq!(r2.pilots, vec!["ji wuming".to_string()], "got {:?}", r2.pilots);
        // Plain-text relay where only the SURNAME is cached: the leading lower-cased word is
        // grabbed so the name isn't truncated to "wuming".
        let mut known3 = noknown();
        known3.insert("wuming".into(), 999);
        let r3 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known3, 1, "ch", "x");
        assert_eq!(r3.pilots, vec!["ji wuming".to_string()], "got {:?}", r3.pilots);
    }

    #[test]
    fn showinfo_name_not_split_into_ship_and_pilot() {
        let s = systems();
        // A char-linked "Wolf E Kristjansson" must stay one pilot — never "Wolf" (the
        // assault frigate) + "Kristjansson". Plain-text relays match it via the known cache.
        let txt = "vin > <url=showinfo:1377//2122822665>Wolf E Kristjansson</url> nv";
        let mut ships = noships();
        ships.insert("wolf".into(), (11371, "Wolf".into()));
        let r = analyze(txt, &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["Wolf E Kristjansson".to_string()]);
        assert!(r.ships.is_empty(), "ships={:?}", r.ships);
        // Plain-text relay with the full name already known resolves whole, no "Wolf" ship.
        let mut known = noknown();
        known.insert("wolf e kristjansson".into(), 2122822665);
        let r2 = analyze("Wolf E Kristjansson nv", &s, &ships, &known, 1, "ch", "x");
        assert_eq!(r2.pilots, vec!["Wolf E Kristjansson".to_string()]);
        assert!(r2.ships.is_empty(), "ships={:?}", r2.ships);
    }

    #[test]
    fn rest_keyword_not_a_pilot_even_if_known() {
        let s = systems();
        // "rest" is a status word ("1 jackdaw, rest NV"), never a pilot — even when the
        // persisted cache has a real character named "Rest".
        let mut known = noknown();
        known.insert("rest".into(), 999);
        let mut ships = noships();
        ships.insert("jackdaw".into(), (34562, "Jackdaw".into()));
        let r = analyze("1 jackdaw, rest NV in Jita", &s, &ships, &known, 1, "ch", "Anaz");
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
        assert!(r.no_visual);
        assert!(r.ships.iter().any(|sh| sh.name == "Jackdaw"));
    }

    #[test]
    fn lowercase_chat_words_not_pilots() {
        let s = systems();
        // Even when the cache holds real players spelled like chat words, a lower-cased
        // mention is the word/abbreviation, not the pilot.
        let mut known = noknown();
        for (w, id) in [("sry", 1i64), ("gg", 2), ("ez", 3), ("neo", 4)] {
            known.insert(w.into(), id);
        }
        let r = analyze("sry gg ez that was ez in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
        // A capitalised short token IS still taken as a deliberate pilot mention.
        let r2 = analyze("Neo tackled in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
        assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("neo")), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn showinfo_pilots_survive_amend_and_clear() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert(
            "q-k2t7".to_string(),
            SystemInfo { id: 30000682, name: "Q-K2T7".into(), security: -0.5,
                constellation: String::new(), region: String::new(), faction: String::new() },
        );
        let s = Systems::new(by_name, HashMap::new());
        let mut ships = noships();
        ships.insert("jackdaw".into(), (34562, "Jackdaw".into()));
        let mut st = IntelState::default();
        let msgs = [
            "<url=showinfo:1377//2122822665>Wolf E Kristjansson</url>  <url=showinfo:1386//2124246974>Kristin Vuld</url>  <url=showinfo:1386//2124278733>Hedgeborn Ragamuffin</url>  <url=showinfo:1378//94277160>Callas Plaude</url>  <url=showinfo:5//30000682>Q-K2T7</url> nv",
            "1 jackdaw, rest NV",
            "<url=showinfo:5//30000682>Q-K2T7</url> clear",
        ];
        for (i, m) in msgs.iter().enumerate() {
            let r = analyze(m, &s, &ships, &noknown(), 100 + i as i64, "ch", "Anaz Dian");
            if !st.try_amend(&r, 60) {
                st.push(r);
            }
        }
        // The sighting keeps all four char-linked pilots and the jackdaw; "rest" never leaks.
        let sighting = st.reports.iter().find(|r| !r.clear).expect("sighting report");
        assert_eq!(sighting.pilots.len(), 4, "pilots={:?}", sighting.pilots);
        assert!(!sighting.pilots.iter().any(|p| p.eq_ignore_ascii_case("rest")));
        assert_eq!(sighting.char_ids.len(), 4);
        assert!(sighting.ships.iter().any(|sh| sh.name == "Jackdaw"));
    }

    #[test]
    fn name_with_bubble_keyword_is_a_pilot_not_a_bubble() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("r0-dmm".to_string(), SystemInfo { id: 30000563, name: "R0-DMM".into(),
            security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
        let s = Systems::new(by_name, HashMap::new());
        // "The Bubble Boy" embeds the "bubble" keyword; the full name must reach the cover and
        // the warp-bubble flag must NOT fire.
        let r = analyze("R0-DMM  The Bubble Boy", &s, &noships(), &noknown(), 1, "ch", "Anniken");
        assert_eq!(r.pilots, vec!["The Bubble Boy".to_string()]);
        assert!(!r.bubble);
        // A real bubble call still fires.
        assert!(analyze("bubble up on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
    }

    #[test]
    fn standing_color_led_name_reaches_the_cover() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("9olq-6".to_string(), SystemInfo { id: 30000800, name: "9OLQ-6".into(),
            security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
        let s = Systems::new(by_name, HashMap::new());
        // Plain-text intel (no showinfo tags — real chat logs have none). "Blue" is a standing
        // colour, but it begins the real name "Blue RandomAttac". The full span (incl. "Blue")
        // must be captured so the ESI cover can split it; previously "Blue" broke the run.
        let r = analyze("Blue RandomAttac  Redhorn Mastro  9OLQ-6", &s, &noships(), &noknown(), 1, "ch", "Ariel Afuran");
        assert_eq!(r.pilots, vec!["Blue RandomAttac Redhorn Mastro".to_string()]);
        assert!(r.systems.iter().any(|d| d.name == "9OLQ-6"));
    }

    #[test]
    fn suffix_subphrase_pilot_is_dropped() {
        let s = systems();
        // "Chen Chen" is a contiguous suffix of "Dr Chen Chen" — the loose run (which
        // breaks on the 2-char "Dr") must not leak it as a second pilot.
        let r = analyze("Dr Chen Chen in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["Dr Chen Chen".to_string()]);
    }

    #[test]
    fn isk_amount_is_not_a_count() {
        let s = systems();
        // "334 million" is ISK; the count is the 2 hostiles.
        let r = analyze("ESS raid 2 Bellicose 334 million 6:00 Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(2), "ISK amount must not inflate the count");
    }

    #[test]
    fn adjacent_names_not_leaked_as_subword() {
        let s = systems();
        // Two adjacent two-word names; "Helper" must not leak out as a standalone name.
        let r = analyze("Bunk Boi Bunk Helper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Bunk Boi Bunk Helper")),
            "pilots={:?}",
            r.pilots
        );
        assert!(!r.pilots.iter().any(|p| p == "Helper"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn parses_regions_from_motd() {
        let known: std::collections::HashSet<String> =
            ["tenerifis", "immensea", "impass", "catch", "wicked creek"]
                .iter().map(|s| s.to_string()).collect();
        let motd = "[ 2026.06.24 ] EVE System > Channel MOTD: TENERIFIS // IMMENSEA // IMPASS // CATCH\nPlease contact Corps Diplomatique";
        assert_eq!(parse_motd_regions(motd, &known), vec!["tenerifis", "immensea", "impass", "catch"]);
        // The real format: one line, last region glued to trailing prose.
        let glued = "EVE System > Channel MOTD: TENERIFIS // IMMENSEA // IMPASS // CATCHPlease contact Corps";
        assert_eq!(parse_motd_regions(glued, &known), vec!["tenerifis", "immensea", "impass", "catch"]);
        // Unknown segments ("Cache" absent from `known`) are ignored.
        assert_eq!(parse_motd_regions("Channel MOTD:  Wicked Creek //  Cache", &known), vec!["wicked creek"]);
    }

    #[test]
    fn motd_region_disambiguates_abbreviation() {
        use crate::geo::{SystemInfo, Systems};
        let mk = |id, name: &str, region: &str| SystemInfo {
            id,
            name: name.into(),
            security: -0.6,
            constellation: String::new(),
            region: region.into(),
            faction: String::new(),
        };
        let by_name: std::collections::HashMap<String, SystemInfo> = [
            ("c-j6mt", mk(1, "C-J6MT", "Tenerifis")),
            ("c-j7cr", mk(2, "C-J7CR", "Vale of the Silent")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let sys = Systems::new(by_name, std::collections::HashMap::new());
        // No hint: ambiguous "C-J" stays unresolved.
        let r0 = analyze_ctx("hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &[]);
        assert!(r0.systems.is_empty(), "should stay ambiguous: {:?}", r0.systems);
        // Channel covers Tenerifis -> resolves to C-J6MT, not the Vale C-J7CR.
        let regions = vec!["Tenerifis".to_string()];
        let r = analyze_ctx("hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &regions);
        assert!(r.systems.iter().any(|s| s.name == "C-J6MT"), "systems={:?}", r.systems);
        assert!(!r.systems.iter().any(|s| s.name == "C-J7CR"), "systems={:?}", r.systems);
    }

    #[test]
    fn digit_handle_is_a_pilot_candidate() {
        let s = systems();
        // "0xtomorrow" starts with a digit, so the Title-case paths miss it.
        let r = analyze("0xtomorrow AGCP-I", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "0xtomorrow"), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("AGCP-I")), "pilots={:?}", r.pilots);
        // ISK/count tokens and system abbreviations stay out.
        assert!(!is_handle_like("334m") && !is_handle_like("88A") && !is_handle_like("1DH-SX"));
        // Time tokens are not names ("4min" = 4 minutes for an ESS post).
        assert!(is_time_token("4min") && is_time_token("30s") && is_time_token("2h"));
        assert!(!is_handle_like("4min") && !is_time_token("0xtomorrow") && !is_time_token("c137m"));
        assert!(is_handle_like("0xtomorrow"));
    }

    #[test]
    fn trailing_apostrophe_stripped_from_name() {
        let s = systems();
        let r = analyze("MO-I1W PeshyHod'", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "PeshyHod"), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.contains('\'')), "pilots={:?}", r.pilots);
        // Internal apostrophes are preserved.
        let r2 = analyze("O'Brien in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p == "O'Brien"), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn detects_chinese_keywords() {
        let s = systems();
        let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(a("J5A 蹲门").camp, "蹲 = camp");
        assert!(a("泡泡 on gate").bubble, "泡泡 = bubble");
        assert!(a("诱导信标").cyno, "诱导 = cyno");
        assert!(a("求救").help, "求救 = help");
        assert!(a("红名被抓了").tackled, "抓 = tackled");
    }

    #[test]
    fn chinese_hull_name_resolves_as_ship() {
        let s = systems();
        // Localised hull names (审判者级 = Retribution) are keys in the ship index.
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("审判者级".to_string(), (17738i64, "Retribution".to_string()))].into_iter().collect();
        let r = analyze("审判者级 in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Retribution"), "ships={:?}", r.ships);
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
    }

    #[test]
    fn detects_tackled_with_target() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("loki".to_string(), (29990i64, "Loki".to_string()))].into_iter().collect();
        let r = analyze("Loki tackled on the gate", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.tackled, "tackled keyword should fire");
        assert!(r.tackled_targets.iter().any(|t| t == "Loki"), "targets={:?}", r.tackled_targets);
        assert!(!r.cap_tackled, "Loki is not a capital");
        // class target + point/scram variants
        let r2 = analyze("2 marauders pointed", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.tackled && r2.tackled_targets.iter().any(|t| t == "Marauder"), "targets={:?}", r2.tackled_targets);
        let r3 = analyze("recon scrammed", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.tackled && r3.tackled_targets.iter().any(|t| t == "Recon"), "targets={:?}", r3.tackled_targets);
        // cap tackled stays distinct + escalated (and still fires the generic tackled).
        let r4 = analyze("dread tackled", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r4.cap_tackled, "cap_tackled escalated");
        assert!(r4.tackled, "tackled also fires for a cap");
    }

    #[test]
    fn detects_ship_classes() {
        let s = systems();
        let r = analyze("2 dics and a recon on the gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.classes.iter().any(|c| c == "Interdictor"), "classes={:?}", r.classes);
        assert!(r.classes.iter().any(|c| c == "Recon"), "classes={:?}", r.classes);
        for w in ["dics", "recon"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        let r2 = analyze("hic plus 2 logi and a bomber", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.classes.iter().any(|c| c == "Heavy Interdictor"), "classes={:?}", r2.classes);
        // "t3" / "t3s" = Tier-3 (Strategic) Cruiser; "etc" is a stop word, never a pilot.
        let r3 = analyze("3 t3s and a t3 roaming, etc", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.classes.iter().any(|c| c == "Strategic Cruiser"), "classes={:?}", r3.classes);
        assert!(!r3.pilots.iter().any(|p| p.eq_ignore_ascii_case("etc")), "pilots={:?}", r3.pilots);
        // Generic hull sizes are ship types, never pilots.
        let r4 = analyze("CRUISERS and battleships in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r4.classes.iter().any(|c| c == "Cruiser"), "classes={:?}", r4.classes);
        assert!(r4.classes.iter().any(|c| c == "Battleship"), "classes={:?}", r4.classes);
        assert!(r4.pilots.is_empty(), "pilots={:?}", r4.pilots);
        // An all-caps ship acronym ("DNI" = Drake Navy Issue) is the ship, not a pilot.
        let mut ships = noships();
        ships.insert("dni".into(), (37457, "Drake Navy Issue".into()));
        let r5 = analyze("DNI in Jita", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r5.ships.iter().any(|sh| sh.name == "Drake Navy Issue"), "ships={:?}", r5.ships);
        assert!(r5.pilots.is_empty(), "pilots={:?}", r5.pilots);
        assert!(r2.classes.iter().any(|c| c == "Logistics"), "classes={:?}", r2.classes);
        assert!(r2.classes.iter().any(|c| c == "Stealth Bomber"), "classes={:?}", r2.classes);
    }

    #[test]
    fn complex_ship_report_not_misparsed() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [
            ("eris", (22460i64, "Eris")),
            ("vedmak", (47271, "Vedmak")),
            ("eni", (40072, "Exequror Navy Issue")),
        ]
        .into_iter()
        .map(|(k, (id, n))| (k.to_string(), (id, n.to_string())))
        .collect();
        let r = analyze(
            "O3-4MN Eris ENI Vedmak mobile bubble on the gate close to ansi warp via cyno beacon",
            &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.bubble, "bubble should fire");
        assert!(r.cyno, "cyno should fire");
        for sh in ["Eris", "Vedmak", "Exequror Navy Issue"] {
            assert!(r.ships.iter().any(|x| x.name == sh), "missing {sh}: {:?}", r.ships);
        }
        for w in ["Eris", "ENI", "Vedmak", "Ansi"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w} a pilot: {:?}", r.pilots);
        }
    }

    #[test]
    fn adjacent_ship_names_are_ships_not_pilots() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [
            ("sabre", (22456i64, "Sabre")),
            ("orthrus", (33157, "Orthrus")),
            ("stabber", (622, "Stabber")),
            ("deimos", (12023, "Deimos")),
        ]
        .into_iter()
        .map(|(k, (id, n))| (k.to_string(), (id, n.to_string())))
        .collect();
        // Adjacent ship names with no separator were read as a 2-word pilot.
        let r = analyze("ZD1-Z2 Sabre Orthrus in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        for w in ["Sabre", "Orthrus", "Sabre Orthrus"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Orthrus"), "ships={:?}", r.ships);
        // "and"-separated ship list.
        let r2 = analyze("Stabber and Deimos in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name == "Stabber"), "ships={:?}", r2.ships);
        assert!(r2.ships.iter().any(|sh| sh.name == "Deimos"), "ships={:?}", r2.ships);
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Stabber") || p.eq_ignore_ascii_case("Deimos")), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn ansi_resolves_to_jump_bridge_destination() {
        use crate::geo::{SystemInfo, Systems};
        let mk = |id, name: &str| SystemInfo {
            id,
            name: name.into(),
            security: -0.5,
            constellation: String::new(),
            region: String::new(),
            faction: String::new(),
        };
        let by_name: std::collections::HashMap<String, SystemInfo> =
            [("o3-4mn", mk(1, "O3-4MN")), ("rancer", mk(2, "Rancer"))]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
        let mut sys = Systems::new(by_name, std::collections::HashMap::new());
        sys.add_bridges(&[(1, 2)]); // Ansiblex O3-4MN <-> Rancer
        let r = analyze("O3-4MN Gate camp on Ansi", &sys, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.camp, "gate-camp keyword should fire");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ansi")), "pilots={:?}", r.pilots);
        assert!(r.gates.iter().any(|g| g == "Rancer"), "the Ansi should lead to Rancer: {:?}", r.gates);
    }

    #[test]
    fn system_code_known_as_pilot_is_not_a_pilot() {
        let s = systems();
        // A real character is named "C-J"; in intel it still means the system.
        let known: std::collections::HashMap<String, i64> =
            [("c-j".to_string(), 2119528359i64)].into_iter().collect();
        let r = analyze("Gorika Galrog C-J in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("C-J")), "pilots={:?}", r.pilots);
        assert!(r.pilots.iter().any(|p| p == "Gorika Galrog"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn plus_count_adds_to_named_pilots() {
        let s = systems();
        // "+20" means 20 more besides the named pilot(s): 1 + 20 = 21.
        let r = analyze("Gorika Galrog +20 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Gorika Galrog"), "pilots={:?}", r.pilots);
        assert_eq!(r.count, Some(21), "pilots={:?}", r.pilots);
        // A lone named pilot with no number gets no count badge (the badge is for 3+).
        let r2 = analyze("Gorika Galrog in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.count, None, "pilots={:?}", r2.pilots);
    }

    #[test]
    fn combat_prob_is_probes_not_pilots() {
        let s = systems();
        let r = analyze("combat prob in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some("Combat Probes"), "probes={:?}", r.probes);
        assert!(
            !r.pilots.iter().any(|p| {
                p.eq_ignore_ascii_case("combat") || p.eq_ignore_ascii_case("prob")
            }),
            "pilots={:?}",
            r.pilots
        );
    }

    #[test]
    fn pilot_ship_killmail_link_splits() {
        // The "Pilot (Hull)" display from a killmail/fitting link splits into the pilot + hull.
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("retribution".to_string(), (11393i64, "Retribution".to_string()))]
                .into_iter()
                .collect();
        let (pilot, ship) = split_pilot_ship("Wolf E Kristjansson (Retribution)", &ships).unwrap();
        assert_eq!(pilot, "Wolf E Kristjansson");
        assert_eq!(ship.0, 11393);
        // A plain pilot name (no known hull in parens) doesn't split.
        assert!(split_pilot_ship("Just A Pilot", &ships).is_none());
    }

    #[test]
    fn thera_hole_is_a_wormhole() {
        let s = systems();
        let r = analyze("thera hole in Rancer", &s, &noships(), &noknown(), 1, "ch", "wwhh");
        assert!(r.wormhole, "should be a wormhole message");
        assert!(matches!(r.wh_dest, Some(crate::wormholes::DestClass::Thera)), "dest={:?}", r.wh_dest);
    }

    #[test]
    fn sisters_combat_scanner_is_probes_not_pilots() {
        let s = systems();
        let r = analyze("Sisters Combat Scanner in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some("Combat Probes"), "probes={:?}", r.probes);
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
    }

    #[test]
    fn drops_subphrase_pilots_works() {
        let mut p = vec!["Nine".to_string(), "Nine -3".to_string()];
        drop_subphrase_pilots(&mut p, &std::collections::HashSet::new());
        assert_eq!(p, vec!["Nine -3".to_string()]);
        // A char-linked name is protected even when a longer glued run contains it.
        let mut q = vec!["Callas Plaude".to_string(), "Callas Plaude Wolf".to_string()];
        let protect: std::collections::HashSet<String> = ["callas plaude".to_string()].into();
        drop_subphrase_pilots(&mut q, &protect);
        assert!(q.contains(&"Callas Plaude".to_string()), "q={q:?}");
    }

    #[test]
    fn standalone_ship_word_known_as_pilot_is_not_a_pilot() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("buzzard".to_string(), (11192i64, "Buzzard".to_string()))].into_iter().collect();
        let known: std::collections::HashMap<String, i64> =
            [("buzzard".to_string(), 794250917i64)].into_iter().collect();
        let r = analyze("hostiles in a Buzzard in Rancer", &s, &ships, &known, 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Buzzard")), "pilots={:?}", r.pilots);
        assert!(r.ships.iter().any(|sh| sh.name == "Buzzard"), "ships={:?}", r.ships);
    }

    #[test]
    fn ansiblex_jump_bridge_is_not_a_pilot() {
        let s = systems();
        let r = analyze("Ansiblex Jump Bridge in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        for w in ["Ansi", "Ansiblex", "Jump", "Bridge"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        let r2 = analyze("reds on the Ansi in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ansi")), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn system_codes_and_state_words_not_pilots() {
        let s = systems();
        // From real intel: "C-J" (system abbreviation) and "bubbled" were read as pilots.
        let r = analyze("88A-RA C-J gate bubbled", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.bubble, "bubble keyword should fire");
        assert!(
            analyze("drag on gate", &s, &noships(), &noknown(), 1, "ch", "x").bubble,
            "drag = drag bubble"
        );
        assert!(
            !analyze("no drag", &s, &noships(), &noknown(), 1, "ch", "x").bubble,
            "negated drag"
        );
        for w in ["C-J", "88A-RA", "bubbled"] {
            assert!(
                !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)),
                "{w} must not be a pilot: {:?}",
                r.pilots
            );
        }
    }

    #[test]
    fn cloaked_is_a_state_not_a_pilot() {
        let s = systems();
        let r = analyze("Psychopathic beemaster cloaked in bubble", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("cloaked")), "pilots={:?}", r.pilots);
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Psychopathic beemaster cloaked")),
            "glued name leaked: {:?}",
            r.pilots
        );
        // The real pilot is still detected.
        assert!(r.pilots.iter().any(|p| p == "Psychopathic beemaster"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn wormhole_word_is_keyword_not_a_pilot() {
        let s = systems();
        let r = analyze("Wormhole in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.wormhole, "wormhole keyword should fire");
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Wormhole")),
            "Wormhole must not be a pilot: {:?}",
            r.pilots
        );
    }

    #[test]
    fn known_pilot_cache_respects_ship_and_stopwords() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [(
            "federation navy comet".to_string(),
            (17841i64, "Federation Navy Comet".to_string()),
        )]
        .into_iter()
        .collect();
        // Real players happen to be named "Navy" and "Comet"; neither should be read as
        // a pilot in "Federation Navy Comet".
        let known: std::collections::HashMap<String, i64> =
            [("navy".to_string(), 1i64), ("comet".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Federation Navy Comet Docteur West in Rancer", &s, &ships, &known, 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy")), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Comet")), "pilots={:?}", r.pilots);
    }

    #[test]
    fn descriptor_and_verb_words_are_not_pilots() {
        let s = systems();
        // From real logs: "Navy"/"Issue" (ship descriptors) and "jumped" (a verb) leaked.
        let r = analyze("Sevra jumped Navy Issue in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        for w in ["jumped", "Navy", "Issue"] {
            assert!(
                !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)),
                "{w} must not be a pilot: {:?}",
                r.pilots
            );
        }
        // A real name in the same line is still caught.
        assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn detects_structures_and_distance() {
        assert_eq!(
            detect_structures("Keepstar 500 off"),
            vec![("Keepstar".to_string(), Some("500km".to_string()))]
        );
        assert_eq!(
            detect_structures("Astra 2AU"),
            vec![("Astrahus".to_string(), Some("2AU".to_string()))]
        );
        assert_eq!(detect_structures("Fort tackled"), vec![("Fortizar".to_string(), None)]);
        assert_eq!(
            detect_structures("merc den anchoring"),
            vec![("Mercenary Den".to_string(), None)]
        );
        assert_eq!(
            detect_structures("Sotiyo 1000km"),
            vec![("Sotiyo".to_string(), Some("1000km".to_string()))]
        );
        // Celestials: planet/moon + number or roman, and the sun. The trailing number is a
        // location, never a hostile count.
        let p1 = analyze("planet 1 Jita", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(p1.celestials, vec!["Planet 1".to_string()]);
        assert!(p1.count.is_none(), "count={:?}", p1.count);
        let m = analyze("moon IV in Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(m.celestials, vec!["Moon IV".to_string()]);
        let sun = analyze("camped at the sun Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(sun.celestials, vec!["Sun".to_string()]);
        assert_eq!(detect_structures("POS bash Rancer"), vec![("POS".to_string(), None)]);
        assert!(is_structure_word("pos"));
        assert!(detect_structures("hostiles in Rancer").is_empty());
        // structure abbreviations aren't pilots
        assert!(is_structure_word("fort") && is_structure_word("keep") && is_structure_word("astra"));
    }

    #[test]
    fn scanner_probes_badge_not_ship_or_pilot() {
        assert_eq!(detect_probes("Sisters Core Scanner Probe on dscan"), Some("Core Probes"));
        assert_eq!(detect_probes("Combat Scanner Probe I"), Some("Combat Probes"));
        assert_eq!(detect_probes("Core Probes"), Some("Core Probes"));
        assert_eq!(detect_probes("combat probes out"), Some("Combat Probes"));
        assert_eq!(detect_probes("probes on dscan"), Some("Probes"));
        assert_eq!(detect_probes("Probe tackled"), None); // the frigate
        assert_eq!(detect_probes("hostiles in Rancer"), None);

        // No double detection: the Probe frigate is dropped and it isn't a pilot.
        let si =
            std::collections::HashMap::from([("probe".to_string(), (587i64, "Probe".to_string()))]);
        let s = systems();
        let r = analyze("Sisters Core Scanner Probe on dscan", &s, &si, &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some("Core Probes"));
        assert!(r.ships.iter().all(|sh| !sh.name.eq_ignore_ascii_case("probe")), "{:?}", r.ships);
        assert!(
            !r.pilots.iter().any(|p| p.to_lowercase().contains("probe")),
            "{:?}",
            r.pilots
        );
        // A lone "Probe" is still the frigate.
        let r2 = analyze("Probe tackled", &s, &si, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name.eq_ignore_ascii_case("probe")));
        // "prob" is shorthand for "probably", not scanning probes.
        assert!(analyze("prob cyno in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").probes.is_none());
        assert_eq!(analyze("combat probes on dscan", &s, &noships(), &noknown(), 1, "ch", "x").probes, Some("Combat Probes"));
    }

    #[test]
    fn parses_isk_amounts() {
        assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
        assert_eq!(parse_isk("worth 1.5b", false), Some(1_500_000_000));
        assert_eq!(parse_isk("300 mil tag", false), Some(300_000_000));
        // Bare "m" reads as millions only with ESS context (collides with "4M-" shorthands).
        assert_eq!(parse_isk("ess 750m", true), Some(750_000_000));
        assert_eq!(parse_isk("loot 750m", false), None);
        // "4M-HGW" is a null-sec system code, never ISK (even with ESS in the line).
        assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
        assert_eq!(parse_isk("5 min", false), None);
        assert_eq!(parse_isk("Rancer 3 Drake +2", false), None);
    }

    #[test]
    fn ni_fi_abbreviations_match_faction_ships() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [
            ("vexor navy issue".to_string(), (1i64, "Vexor Navy Issue".to_string())),
            ("scythe fleet issue".to_string(), (2i64, "Scythe Fleet Issue".to_string())),
        ]
        .into_iter()
        .collect();
        let r = analyze("Vexor NI and Scythe FI in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Vexor Navy Issue"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r.ships);
    }

    #[test]
    fn min_minutes_is_a_stop_word() {
        assert!(is_pilot_stopword("min"));
        assert!(is_pilot_stopword("heading"));
        assert!(is_pilot_stopword("towards"));
        let s = systems();
        // "5 min" -> "min" is time, not a name.
        let runs = loose_pilot_runs("ess 300kk 5 min", &noships(), &s);
        assert!(
            !runs.iter().any(|r| r.split_whitespace().any(|w| w.eq_ignore_ascii_case("min"))),
            "runs={:?}",
            runs
        );
    }

    #[test]
    fn navy_issue_short_form_matches_ship() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [
            ("brutix navy issue".to_string(), (1i64, "Brutix Navy Issue".to_string())),
            ("stabber fleet issue".to_string(), (2i64, "Stabber Fleet Issue".to_string())),
        ]
        .into_iter()
        .collect();
        let r = analyze("Brutix Navy and Stabber Fleet in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Brutix Navy Issue"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Stabber Fleet Issue"), "ships={:?}", r.ships);
    }

    #[test]
    fn connector_stop_word_kept_in_multiword_name() {
        // "the" is a stop word but a name connector -> "The Meek" keeps it.
        assert!(extract_pilots("384-IN The Meek").iter().any(|r| r == "The Meek"));
    }

    #[test]
    fn intel_descriptor_breaks_a_name_run() {
        // "cloaked" is a stop word AND an intel descriptor -> never part of a name.
        let out = extract_pilots("Cloaked Predator");
        assert!(!out.iter().any(|r| r.to_lowercase().contains("cloaked")), "out={:?}", out);
        // a run that is only stop words isn't a pilot either.
        assert!(extract_pilots("The").is_empty());
    }

    #[test]
    fn covered_prefix_name_is_dropped() {
        let pilots = vec![
            "Gallente Citizen".to_string(),
            "Gallente Citizen 17120704".to_string(),
        ];
        let out = drop_covered_prefixes(&pilots, "Gallente Citizen 17120704 8-WYQZ");
        assert_eq!(out, vec!["Gallente Citizen 17120704".to_string()]);
    }

    #[test]
    fn standalone_name_sharing_a_prefix_is_kept() {
        // "Bob" appears on its own AND inside "Bob Smith" -> both are real, keep both.
        let pilots = vec!["Bob".to_string(), "Bob Smith".to_string()];
        let out = drop_covered_prefixes(&pilots, "Bob and Bob Smith inc");
        assert!(out.contains(&"Bob".to_string()));
        assert!(out.contains(&"Bob Smith".to_string()));
    }

    #[test]
    fn multiword_ship_word_not_a_pilot() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [(
            "federation navy comet".to_string(),
            (17841i64, "Federation Navy Comet".to_string()),
        )]
        .into_iter()
        .collect();
        let r = analyze("Federation Navy Comet Docteur West in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Comet")), "pilots={:?}", r.pilots);
        assert!(r.ships.iter().any(|sh| sh.name == "Federation Navy Comet"));
    }

    #[test]
    fn long_name_list_forms_one_run() {
        let s = systems();
        let r = analyze(
            "Noki Saken Ris Etor Ryko Erukka Saratoga Forge Urhi Hita nv in Rancer",
            &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case(
                "Noki Saken Ris Etor Ryko Erukka Saratoga Forge Urhi Hita")),
            "pilots={:?}", r.pilots);
    }

    #[test]
    fn lowercase_name_with_digit_part_is_a_candidate() {
        let s = systems();
        // All-lowercase, code-like middle word, no Title-Case anchor.
        let r = analyze("rick c137 sancgez in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("rick c137 sancgez")),
            "pilots={:?}",
            r.pilots
        );
    }

    #[test]
    fn single_word_name_is_a_candidate() {
        let s = systems();
        // Plain-text log, a lone Title-Case name must still be offered for ESI.
        let r = analyze("Sevra in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn keywords_no_substring_false_trigger() {
        let s = systems();
        // "Bunk Helper" is a pilot run; "Helper" must not trigger HELP.
        let r = analyze("Bunk Boi Bunk Helper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.help, "Helper must not trigger help");
        // "Cynabal" the ship must not trigger CYNO.
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("cynabal".to_string(), (17720i64, "Cynabal".to_string()))].into_iter().collect();
        let r2 = analyze("Cynabal in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(!r2.cyno, "Cynabal must not trigger cyno");
        // Exact keywords still fire.
        assert!(analyze("cyno up Rancer", &s, &noships(), &noknown(), 1, "ch", "x").cyno);
    }

    #[test]
    fn detects_help_keyword() {
        let s = systems();
        assert!(analyze("help in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
        assert!(analyze("sos Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
        assert!(analyze("need backup in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
        assert!(!analyze("clear Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
    }

    #[test]
    fn loose_run_catches_lowercase_name() {
        let s = systems();
        // Real logs carry no url tags; a lowercase name next to a Title-Case one must
        // still become a candidate run (ESI/cover confirms it later).
        let r = analyze("bigfoott Kepplet in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott Kepplet")),
            "pilots={:?}",
            r.pilots
        );
    }

    #[test]
    fn two_char_links_both_detected() {
        let s = systems();
        let r = analyze(
            "<url=showinfo:1375//2123842340>bigfoott</url>  <url=showinfo:1374//2124452380>Kepplet</url>  <url=showinfo:5//30000593>GRHS-B</url>",
            &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|x| x == "bigfoott"), "pilots={:?}", r.pilots);
        assert!(r.pilots.iter().any(|x| x == "Kepplet"), "pilots={:?}", r.pilots);
        assert!(r.char_ids.iter().any(|(n, _)| n == "bigfoott"), "char_ids={:?}", r.char_ids);
        assert!(r.char_ids.iter().any(|(n, _)| n == "Kepplet"), "char_ids={:?}", r.char_ids);
    }

    #[test]
    fn pilot_link_before_system() {
        let s = systems();
        let r = analyze(
            "<url=showinfo:1379//115252465>Rondrasil</url>  <url=showinfo:5//30000775>8-WYQZ</url> nv",
            &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Rondrasil"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn multiword_ship_not_double_counted() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> = [
            ("catalyst".to_string(), (16240i64, "Catalyst".to_string())),
            ("catalyst navy issue".to_string(), (33470i64, "Catalyst Navy Issue".to_string())),
        ]
        .into_iter()
        .collect();
        let r = analyze("Rancer Catalyst Navy Issue", &s, &ships, &noknown(), 1, "ch", "x");
        let names: Vec<_> = r.ships.iter().map(|sh| sh.name.clone()).collect();
        assert_eq!(names, vec!["Catalyst Navy Issue"], "got {:?}", names);
    }

    #[test]
    fn detects_localised_kill_keyword() {
        let s = systems();
        // Chinese killReport text (no url wrapper in the log) is recognised as a kill.
        let r = analyze("DZ Sharisa > 击杀：Wolf E Kristjansson", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.killmail, "should flag a kill from the Chinese keyword");
    }

    #[test]
    fn merge_keeps_char_link() {
        // A pilot's showinfo char-id must survive a report merge, else the card filters
        // the pilot out (char-linked names always display; unresolved bare names don't).
        let s = systems();
        let mut state = IntelState::default();
        state.push(analyze("hostile in Rancer", &s, &noships(), &noknown(), 100, "ch", "Scout"));
        let follow = analyze(
            "<url=showinfo:1379//115252465>Rondrasil</url>",
            &s, &noships(), &noknown(), 130, "ch", "Scout");
        assert!(state.try_amend(&follow, 60));
        let r = &state.reports[0];
        assert!(r.pilots.iter().any(|p| p == "Rondrasil"), "pilots={:?}", r.pilots);
        assert!(r.char_ids.iter().any(|(n, _)| n == "Rondrasil"), "char_ids={:?}", r.char_ids);
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
        assert!(!state.reports[0].gates.is_empty());
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
    fn name_with_trailing_number_isnt_a_count() {
        let s = systems();
        // "Malcolm 41" / "Adama 80" are pilot names; the trailing number is part of the
        // name, not a hostile count.
        assert_eq!(analyze("8X6T-8 Malcolm 41", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
        assert_eq!(analyze("Adama 80 pls help", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
        // A number after a *system* is still a count.
        assert_eq!(analyze("Rancer 5", &s, &noships(), &noknown(), 1, "ch", "x").count, Some(5));
    }

    #[test]
    fn loose_runs_keep_short_name_parts() {
        let s = systems();
        // Short components ("80", "R") stay in the run so the ESI cover can confirm the
        // real names ("Adama 80", "Lopatich R").
        let runs = loose_pilot_runs("Adama 80 Lopatich R", &noships(), &s);
        assert!(runs.iter().any(|r| r.contains("80")), "runs={:?}", runs);
        assert!(runs.iter().any(|r| r.split_whitespace().last() == Some("R")), "runs={:?}", runs);
        // A run of only short parts is dropped (needs a real >=3-letter word).
        assert!(loose_pilot_runs("80 90", &noships(), &s).is_empty());
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
        // One-word "gatecamp" fires the camp keyword and is never read as a pilot.
        let gc = analyze("gatecamp in 1DQ1-A", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(gc.camp, "gatecamp should fire camp");
        assert!(gc.pilots.is_empty(), "gatecamp is not a pilot");
        assert!(analyze("https://zkillboard.com/kill/123/", &s, &noships(), &noknown(), 1, "ch", "x").killmail);
        assert!(analyze("cyno up in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").cyno);
        assert!(analyze("hotdropper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
        assert!(analyze("blops on scan Jita", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
        assert!(analyze("watch for hot drop", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
        assert!(!analyze("just a dropbear in Jita", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
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
    fn showinfo_system_strips_waypoint_star() {
        let s = systems();
        // EVE appends "*" to a system set as a route waypoint; it must still resolve.
        let r = analyze(
            "<url=showinfo:5//30001242>Rancer*</url> <url=showinfo:1375//2121803366>Nine -3</url>",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "x",
        );
        assert!(r.systems.iter().any(|d| d.name == "Rancer"), "systems: {:?}", r.systems);
        assert!(r.pilots.iter().any(|p| p == "Nine -3"), "pilots: {:?}", r.pilots);
    }

    #[test]
    fn pilot_name_keeps_alt_suffix() {
        let s = systems();
        // "-L" / "-3" are alt-name suffixes, not system shorthands, and must stay on the name.
        let r = analyze("hostiles Nine -L in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Nine -L"), "pilots: {:?}", r.pilots);
        let r2 = analyze("Nine -3", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p == "Nine -3"), "pilots: {:?}", r2.pilots);
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
    fn detects_wormhole_code() {
        let s = systems();
        let r = analyze("Rancer K162 just appeared", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.wormhole, "wormhole flag");
        assert_eq!(r.wh_type.as_deref(), Some("K162"));
        assert!(r.systems.iter().any(|d| d.name == "Rancer"));
        // A specific code is recognised too.
        let r2 = analyze("Jita N968 sig", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.wh_type.as_deref(), Some("N968"));
        // Plain intel isn't a wormhole.
        let r3 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r3.wormhole);
        assert!(r3.wh_type.is_none());
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
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("Jita")); // the stargate link → gate
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
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("Jita"));
    }

    #[test]
    fn lowercase_codes_vs_hyphenated_names() {
        assert!(looks_like_system_code("c-j"));
        assert!(looks_like_system_code("4m-"));
        assert!(looks_like_system_code("1dq1-a"));
        assert!(looks_like_system_code("C-J6MT"));
        assert!(!looks_like_system_code("Jean-Luc"));
        assert!(!looks_like_system_code("Mary-Jo"));
    }

    #[test]
    fn lowercase_code_gate_is_not_a_pilot() {
        // Two C-J systems → "c-j" is an ambiguous prefix (as live), so it isn't auto-resolved
        // as a system; a character is also named "c-j". In "c-j gate" it's the gate, not a player.
        let by_name: std::collections::HashMap<String, SystemInfo> = [
            ("c-j6mt", "C-J6MT", 5, -0.6),
            ("c-j7cr", "C-J7CR", 6, -0.5),
            ("rancer", "Rancer", 1, 0.4),
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
        let adj = std::collections::HashMap::from([(1i64, vec![5i64]), (5, vec![1])]);
        let s = Systems::new(by_name, adj);
        let known = std::collections::HashMap::from([("c-j".to_string(), 999i64)]);
        let r = analyze("Rancer c-j gate camped", &s, &noships(), &known, 1, "ch", "x");
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("c-j")),
            "c-j is the gate's system, not a pilot: {:?}",
            r.pilots
        );
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
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
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("YPW-M2"));
        // "status" is a request keyword: not a pilot, and stays informational.
        let q = analyze("status in Rancer?", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(q.status);
        assert!(!q.pilots.iter().any(|p| p.eq_ignore_ascii_case("status")));
    }

    #[test]
    fn showinfo_char_link_recognised_by_typeid_and_itemid() {
        let s = systems();
        // A character whose bloodline typeID we list (1378) is recognised, AND its
        // char id is captured from the itemID (in the modern 2.1B character range).
        let r = analyze(
            "<url=showinfo:1378//2116583018>The Meek</url> proteus",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "Reporter",
        );
        assert!(r.pilots.iter().any(|p| p == "The Meek"));
        assert!(r.char_ids.iter().any(|(n, id)| n == "The Meek" && *id == 2116583018));
        // A character whose typeID is NOT in our bloodline list is still caught by its
        // itemID being in the character range.
        let r2 = analyze(
            "<url=showinfo:99//94000123>Nobody Known</url>",
            &s,
            &noships(),
            &noknown(),
            1,
            "ch",
            "Reporter",
        );
        assert!(r2.char_ids.iter().any(|(n, id)| n == "Nobody Known" && *id == 94000123));
    }

    #[test]
    fn gate_resolves_neighbour_prefix() {
        use std::collections::HashMap;
        let by_name = [
            ("c-j6mt", "C-J6MT", 5i64, -0.6),
            ("5e-cfl", "5E-CFL", 10, -0.5),
            ("sv5-8n", "SV5-8N", 9, -0.4),
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
        // C-J6MT neighbours 5E-CFL and SV5-8N.
        let adj = HashMap::from([(5i64, vec![10i64, 9]), (10, vec![5]), (9, vec![5])]);
        let s = Systems::new(by_name, adj);
        // A short prefix matching a neighbour name resolves to the full system.
        let r = analyze("C-J6MT 5e gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("5E-CFL"));
        // A 2-char letter prefix matches the other neighbour too.
        let r2 = analyze("C-J6MT sv gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("SV5-8N"));
    }

    #[test]
    fn gate_disambiguates_abbrev_via_context() {
        use std::collections::HashMap;
        let by_name = [
            ("d-pnsn", "D-PNSN", 1i64, -0.4),
            ("c-j6mt", "C-J6MT", 2, -0.6),
            ("c-jeez", "C-JEEZ", 3, -0.5), // makes "C-J" globally ambiguous
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
        let adj = HashMap::from([(1i64, vec![2i64]), (2, vec![1])]); // D-PNSN <-> C-J6MT
        let s = Systems::new(by_name, adj);
        // A system in the message gives the primary; "C-J" resolves to the neighbour.
        let r = analyze("D-PNSN C-J gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
        // No system in the message: the channel's last system (context) disambiguates.
        let r2 = analyze_ctx("C-J gate", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[]);
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
    }

    #[test]
    fn one_system_extra_mentions_become_gates() {
        let s = systems();
        // Three system links → only the first stays; the rest become gates.
        let txt = "<url=showinfo:5//1>Rancer</url> <url=showinfo:5//2>Jita</url> <url=showinfo:5//8>Amarr</url>";
        let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(
            r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["Rancer"]
        );
        assert_eq!(r.gates, vec!["Jita".to_owned(), "Amarr".to_owned()]);
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
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("78-AAA"));
        assert_eq!(
            r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["C-J6MT"],
        );

        // A bare number used as a gate must not also be a hostile count.
        let r2 = analyze("20 reds on 78 gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("78-AAA"));
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
