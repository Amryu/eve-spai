//! delve911 capital-rescue support: parsing emergency pings, fleet-composition
//! classification, rescue timers and the shared `RescueState`.
//!
//! Everything here is pure logic with no shared locks or I/O, so the watcher can run
//! `parse_event` under `catch_unwind` and a malformed line drops one event instead of
//! taking down the app. `RescueState` lives behind an `Arc<Mutex<..>>` on the app and is
//! the single source of truth for the feature.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::geo::Systems;

const EVENTS_CAP: usize = 200;

static SEQ: AtomicU64 = AtomicU64::new(1);
fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Words that must never be treated as a pilot/cyno name run.
const STOPWORDS: &[&str] = &[
    "bping", "all", "rescue", "help", "tackled", "tackle", "pointed", "point", "bubbled",
    "scrammed", "scrambled", "cyno", "cynod", "in", "at", "on", "is", "has", "the", "a", "and",
    "needs", "need", "capital", "cap", "caps", "dread", "dreads", "dreadnought", "carrier",
    "carriers", "fax", "super", "supers", "titan", "titans", "moon", "anom", "anomaly", "site",
    "gate", "sitting", "hostiles", "hostile", "please", "warp", "ratting", "station", "structure",
    "keepstar", "fortizar", "astrahus", "tackling", "primary", "hero", "get", "we", "here",
];

const TACKLE_KEYWORDS: &[&str] =
    &["tackled", "tackle", "pointed", "bubbled", "scrammed", "scrambled", "hard tackle"];

/// One parsed delve911 line. Always constructed (raw fallback) so nothing is silently dropped.
#[derive(Clone, Debug)]
pub struct RescueEvent {
    pub seq: u64,
    pub received: i64,
    pub author: String,
    pub raw: String,
    /// True when the line reads like an actual emergency call (feeds the ping list + map).
    pub is_ping: bool,
    pub system_id: Option<i64>,
    pub system_name: Option<String>,
    pub pilot: Option<String>,
    pub cyno: Option<String>,
    pub anomaly: Option<String>,
    pub cap_class: Option<CapClass>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapClass {
    Rorqual,
    Dread,
    Fax,
    Carrier,
    Super,
    Titan,
}

impl CapClass {
    pub fn label(self) -> &'static str {
        match self {
            CapClass::Rorqual => "Rorqual",
            CapClass::Dread => "Dreadnought",
            CapClass::Fax => "Force Auxiliary",
            CapClass::Carrier => "Carrier",
            CapClass::Super => "Supercarrier",
            CapClass::Titan => "Titan",
        }
    }

    /// Suggested (siege, panic) timer seconds when the ping named a capital type.
    /// `None` for a leg that type does not have, so the FC dials it manually.
    /// Siege/Triage/Industrial Core cycle = 300s. PANIC = the Rorqual invulnerability core,
    /// 4-6 min by skill; default the middle (300s). Only the Rorqual actually has PANIC.
    pub fn preset_timers(self) -> (Option<u32>, Option<u32>) {
        match self {
            CapClass::Rorqual => (Some(300), Some(300)),
            CapClass::Dread => (Some(300), None),
            CapClass::Fax => (Some(300), None),
            CapClass::Carrier => (None, None),
            CapClass::Super => (None, None),
            CapClass::Titan => (None, None),
        }
    }
}

/// A manually-set countdown. Runtime only (never serialized). Counts down once `start` is called.
#[derive(Clone, Debug, Default)]
pub struct Timer {
    started: bool,
    remaining: i64,
    deadline: Option<Instant>,
}

impl Timer {
    pub fn is_set(&self) -> bool {
        self.started
    }

    /// Seconds left right now (0 once elapsed). When not started, the pending value.
    /// Ceil so a freshly-started 300s reads 5:00, not 4:59, and adjustments stay whole-second.
    pub fn current(&self) -> i64 {
        match self.deadline {
            Some(d) => d.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as i64,
            None => self.remaining.max(0),
        }
    }

    pub fn start(&mut self, secs: i64) {
        let s = secs.max(0);
        self.remaining = s;
        self.deadline = Some(Instant::now() + Duration::from_secs(s as u64));
        self.started = true;
    }

    /// Set the pending value without starting the countdown (ignored once running).
    pub fn set_value(&mut self, secs: i64) {
        if self.deadline.is_none() {
            self.remaining = secs.max(0);
        }
    }

    /// Begin counting down from the current pending value.
    pub fn start_now(&mut self) {
        self.start(self.current());
    }

    /// Nudge by ±delta seconds. While running, shift the deadline directly (exact, no re-flooring
    /// that would eat a second per press); while stopped, change the pending value.
    pub fn adjust(&mut self, delta: i64) {
        match self.deadline {
            Some(d) => {
                self.deadline = Some(if delta >= 0 {
                    d + Duration::from_secs(delta as u64)
                } else {
                    let sub = Duration::from_secs((-delta) as u64);
                    let now = Instant::now();
                    if d > now + sub { d - sub } else { now }
                });
            }
            None => {
                self.remaining = (self.remaining + delta).max(0);
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Timer::default();
    }
}

/// Fleet role bucket derived from a ship's SDE group. `Ord` puts capitals first for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShipRole {
    Titan,
    Supercarrier,
    Dread,
    Fax,
    Carrier,
    Logi,
    LogiFrig,
    Booster,
    Dictor,
    Hictor,
    Recon,
    CommandDest,
    Dps,
    Tackle,
    Ewar,
    Other,
}

impl ShipRole {
    pub fn label(self) -> &'static str {
        match self {
            ShipRole::Titan => "Titans",
            ShipRole::Supercarrier => "Supers",
            ShipRole::Dread => "Dreads",
            ShipRole::Fax => "FAX",
            ShipRole::Carrier => "Carriers",
            ShipRole::Logi => "Logi",
            ShipRole::LogiFrig => "Logi frig",
            ShipRole::Booster => "Boosters",
            ShipRole::Dictor => "Dictors",
            ShipRole::Hictor => "Hictors",
            ShipRole::Recon => "Recons",
            ShipRole::CommandDest => "Command dessies",
            ShipRole::Dps => "DPS",
            ShipRole::Tackle => "Tackle",
            ShipRole::Ewar => "EWAR",
            ShipRole::Other => "Other",
        }
    }

    pub fn is_titan(self) -> bool {
        self == ShipRole::Titan
    }

    /// Recons carry covert cynos (Arazu/Lachesis/Huginn/Rapier), so flag them as possible cyno.
    pub fn is_recon(self) -> bool {
        self == ShipRole::Recon
    }
}

/// Classify a ship by its SDE group name. Order matters: check the more specific group first
/// (Supercarrier before Carrier, Logistics Frigate before Logistics).
pub fn classify(group: &str) -> ShipRole {
    let g = group.to_lowercase();
    let has = |needle: &str| g.contains(needle);
    if has("titan") {
        ShipRole::Titan
    } else if has("supercarrier") {
        ShipRole::Supercarrier
    } else if has("force auxiliary") {
        ShipRole::Fax
    } else if has("dreadnought") {
        ShipRole::Dread
    } else if has("carrier") {
        ShipRole::Carrier
    } else if has("logistics frigate") {
        ShipRole::LogiFrig
    } else if has("logistics") {
        ShipRole::Logi
    } else if has("heavy interdiction cruiser") {
        ShipRole::Hictor
    } else if has("interdictor") {
        ShipRole::Dictor
    } else if has("force recon") || has("combat recon") {
        ShipRole::Recon
    } else if has("command destroyer") {
        ShipRole::CommandDest
    } else if has("command ship") {
        ShipRole::Booster
    } else if has("electronic attack") {
        ShipRole::Ewar
    } else if has("interceptor") {
        ShipRole::Tackle
    } else if g.is_empty() {
        ShipRole::Other
    } else {
        ShipRole::Dps
    }
}

#[derive(Clone, Debug)]
pub struct FleetMember {
    // character_id/ship_type_id/group are kept for lookups and future UI; not all are displayed.
    #[allow(dead_code)]
    pub character_id: i64,
    pub name: String,
    #[allow(dead_code)]
    pub ship_type_id: i64,
    pub ship: String,
    #[allow(dead_code)]
    pub group: String,
    pub role: ShipRole,
    pub system_id: i64,
}

#[derive(Clone, Debug, Default)]
pub struct FleetSnapshot {
    pub fleet_id: Option<i64>,
    /// ESI: the fleet is registered / advertised (GET /fleets/{id}/ is_registered).
    pub is_registered: bool,
    pub members: Vec<FleetMember>,
    pub counts: BTreeMap<ShipRole, u32>,
    /// Unix seconds of the last successful build. 0 = never populated.
    pub updated: i64,
    /// True when the most recent poll failed and this is stale data kept on screen.
    pub stale: bool,
}

impl FleetSnapshot {
    pub fn build(fleet_id: Option<i64>, members: Vec<FleetMember>, now: i64) -> Self {
        let mut counts: BTreeMap<ShipRole, u32> = BTreeMap::new();
        for m in &members {
            *counts.entry(m.role).or_insert(0) += 1;
        }
        FleetSnapshot { fleet_id, is_registered: false, members, counts, updated: now, stale: false }
    }

    pub fn count(&self, role: ShipRole) -> u32 {
        self.counts.get(&role).copied().unwrap_or(0)
    }

    pub fn has_role(&self, role: ShipRole) -> bool {
        self.count(role) > 0
    }

    pub fn has_pilot(&self, name: &str) -> bool {
        self.member(name).is_some()
    }

    pub fn member(&self, name: &str) -> Option<&FleetMember> {
        let n = name.trim().to_lowercase();
        self.members.iter().find(|m| m.name.to_lowercase() == n)
    }
}

#[derive(Clone, Debug)]
pub struct CynoInfo {
    pub in_system: bool,
    pub reachable: bool,
    pub jumps: u32,
    #[allow(dead_code)]
    pub dest_id: i64,
    pub dest_name: String,
    /// True when the shortest path uses a jump bridge (blockade risk).
    pub via_bridge: bool,
}

#[derive(Default)]
pub struct RescueState {
    pub active: bool,
    /// The FC has claimed this rescue / manually started the save. Ping sending is BLOCKED until
    /// this is set, so a ping can never go out by accident before the FC commits.
    pub claimed: bool,
    /// Test scenario loaded. While true the fleet poller is paused (so injected data isn't
    /// overwritten) and ALL ping sending is hard-disabled in the UI. Nothing ever leaves the app.
    pub test_mode: bool,
    pub events: Vec<RescueEvent>,
    pub capital_system: Option<i64>,
    pub capital_system_name: Option<String>,
    pub capital_pilot: Option<String>,
    pub cyno_pilot: Option<String>,
    pub anomaly: Option<String>,
    pub cap_class: Option<CapClass>,
    pub dscan: Option<Vec<(String, u32)>>,
    pub op_channel: u8,
    pub doctrine: String,
    /// The editable outgoing ping. Empty until first shown, then filled from the template and
    /// freely edited by the FC before sending.
    pub pending_ping: String,
    /// (op_channel, doctrine) the `pending_ping` was last generated for. When op/doctrine change,
    /// the ping is regenerated from the template (discarding manual edits, as intended).
    pub ping_built_for: Option<(u8, String)>,
    /// Draft reply typed into the chat view, sent to the selected tab's room.
    pub delve911_reply: String,
    /// Selected chat tab: 0 = delve911, 1 = skirmish_commanders.
    pub chat_tab: u8,
    /// mm:ss text buffer for the PANIC timer while stopped (so typing isn't clobbered each frame).
    pub panic_input: String,
    pub siege: Timer,
    pub panic: Timer,
    pub fleet: FleetSnapshot,
    pub nearest_cyno: Option<CynoInfo>,
    /// Sticky snowflakes: character_id -> reason tag. Once flagged (capital/cyno/titan/recon) a
    /// pilot STAYS flagged for the session even if they re-ship to a pod, so the FC keeps tracking
    /// them. Keyed by character_id (survives ship changes).
    pub snowflakes: HashMap<i64, String>,
}

/// Why a fleet member is a snowflake right now (None if not currently one). The result is made
/// sticky by the caller so a later re-ship (e.g. to a pod) doesn't drop the flag.
pub fn snowflake_tag(
    m: &FleetMember,
    capital: Option<&str>,
    cyno: Option<&str>,
) -> Option<&'static str> {
    let nlc = m.name.to_lowercase();
    if capital.is_some_and(|c| c == nlc) {
        Some("CAPITAL")
    } else if cyno.is_some_and(|c| c == nlc) {
        Some("CYNO")
    } else if m.role.is_titan() {
        Some("TITAN")
    } else if m.role.is_recon() {
        Some("RECON cyno?")
    } else {
        None
    }
}

impl RescueState {
    /// Append an event, capping the ring so history never grows unbounded.
    pub fn push_event(&mut self, ev: RescueEvent) {
        if ev.is_ping {
            if let Some(id) = ev.system_id {
                self.capital_system = Some(id);
                self.capital_system_name = ev.system_name.clone();
            }
            if ev.pilot.is_some() {
                self.capital_pilot = ev.pilot.clone();
            }
            if ev.cyno.is_some() {
                self.cyno_pilot = ev.cyno.clone();
            }
            if ev.anomaly.is_some() {
                self.anomaly = ev.anomaly.clone();
            }
            if let Some(class) = ev.cap_class {
                let newly = self.cap_class != Some(class);
                self.cap_class = Some(class);
                if newly {
                    self.apply_presets(class);
                }
            }
        }
        self.events.push(ev);
        if self.events.len() > EVENTS_CAP {
            let drop = self.events.len() - EVENTS_CAP;
            self.events.drain(0..drop);
        }
    }

    /// Apply capital-type timer presets (only ones the type actually has), leaving unset timers
    /// alone so the FC can still dial them. Called once when a class is first known.
    pub fn apply_presets(&mut self, class: CapClass) {
        let (siege, panic) = class.preset_timers();
        if let Some(s) = siege {
            if !self.siege.is_set() {
                self.siege.adjust(s as i64 - self.siege.current());
            }
        }
        if let Some(p) = panic {
            if !self.panic.is_set() {
                self.panic.adjust(p as i64 - self.panic.current());
            }
        }
    }
}

fn is_name_candidate(s: &str) -> bool {
    if s.is_empty() || !crate::dscan::is_valid_char_name(s) {
        return false;
    }
    s.split_whitespace().all(|w| !STOPWORDS.contains(&w.to_lowercase().as_str()))
}

fn clean_token(t: &str) -> &str {
    t.trim_matches(|c: char| !c.is_alphanumeric())
}

/// Longest valid name run (up to 3 words) at the end of a segment.
fn trailing_name(seg: &str) -> Option<String> {
    let words: Vec<&str> = seg.split_whitespace().collect();
    for take in (1..=3.min(words.len())).rev() {
        let cand = words[words.len() - take..].join(" ");
        let cleaned = clean_token(&cand);
        if is_name_candidate(cleaned) {
            return Some(cleaned.to_string());
        }
    }
    None
}

/// Longest valid name run (up to 3 words) at the start of a segment.
fn leading_name(seg: &str) -> Option<String> {
    let words: Vec<&str> = seg.split_whitespace().collect();
    for take in (1..=3.min(words.len())).rev() {
        let cand = words[..take].join(" ");
        let cleaned = clean_token(&cand);
        if is_name_candidate(cleaned) {
            return Some(cleaned.to_string());
        }
    }
    None
}

fn detect_system(text: &str, systems: &Systems) -> Option<(i64, String)> {
    // Prefer nullsec-style tokens (a dash or digit), which almost never collide with prose.
    for pass in 0..2 {
        for raw in text.split(|c: char| c.is_whitespace() || c == ',') {
            let tok = clean_token(raw);
            if tok.len() < 2 {
                continue;
            }
            let nullsec_like = tok.contains('-') || tok.chars().any(|c| c.is_ascii_digit());
            if pass == 0 && !nullsec_like {
                continue;
            }
            if pass == 1 && (nullsec_like || STOPWORDS.contains(&tok.to_lowercase().as_str())) {
                continue;
            }
            if let Some(info) = systems.lookup(tok) {
                return Some((info.id, info.name.clone()));
            }
        }
    }
    None
}

/// Map an SDE group name to a capital class (handles Rorqual, Navy/Lancer dread variants, etc.).
fn cap_class_from_group(group: &str) -> Option<CapClass> {
    let g = group.to_lowercase();
    if g.contains("capital industrial") || g.contains("rorqual") {
        Some(CapClass::Rorqual)
    } else if g.contains("titan") {
        Some(CapClass::Titan)
    } else if g.contains("supercarrier") {
        Some(CapClass::Super)
    } else if g.contains("force auxiliary") {
        Some(CapClass::Fax)
    } else if g.contains("dreadnought") {
        Some(CapClass::Dread)
    } else if g.contains("carrier") {
        Some(CapClass::Carrier)
    } else {
        None
    }
}

/// Look for a specific ship name in the text (e.g. "Phoenix Navy Issue") and resolve its class via
/// the SDE name->group map. Scans word windows (longest first) so multi-word hulls match.
fn detect_ship_class(text_lc: &str, ships: &HashMap<String, String>) -> Option<CapClass> {
    let words: Vec<&str> = text_lc.split_whitespace().collect();
    for start in 0..words.len() {
        let max = 4.min(words.len() - start);
        for len in (1..=max).rev() {
            let cand = words[start..start + len].join(" ");
            let cleaned = cand.trim_matches(|c: char| !c.is_alphanumeric());
            if cleaned.len() < 3 {
                continue;
            }
            if let Some(group) = ships.get(cleaned) {
                if let Some(cc) = cap_class_from_group(group) {
                    return Some(cc);
                }
            }
        }
    }
    None
}

/// Capital class from the ping: prefer a specific hull name, fall back to category keywords.
fn detect_cap_class(text_lc: &str, ships: &HashMap<String, String>) -> Option<CapClass> {
    if let Some(cc) = detect_ship_class(text_lc, ships) {
        return Some(cc);
    }
    if text_lc.contains("rorq") {
        Some(CapClass::Rorqual)
    } else if text_lc.contains("titan") {
        Some(CapClass::Titan)
    } else if text_lc.contains("super") {
        Some(CapClass::Super)
    } else if text_lc.contains("fax") || text_lc.contains("force aux") || text_lc.contains("aux") {
        Some(CapClass::Fax)
    } else if text_lc.contains("dread") {
        Some(CapClass::Dread)
    } else if text_lc.contains("carrier") {
        Some(CapClass::Carrier)
    } else {
        None
    }
}

fn detect_anomaly(text: &str) -> Option<String> {
    // Parenthetical first, then "at <...>"/"moon <...>".
    if let (Some(a), Some(b)) = (text.find('('), text.find(')')) {
        if b > a + 1 {
            let inner = text[a + 1..b].trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    let lc = text.to_ascii_lowercase();
    for kw in [" at ", " moon ", " anom ", " on "] {
        if let Some(pos) = lc.find(kw) {
            let seg = text[pos + kw.len()..].trim();
            let frag: String =
                seg.split(|c: char| c == ',' || c == ';').next().unwrap_or("").trim().to_string();
            if frag.len() >= 3 {
                return Some(frag);
            }
        }
    }
    None
}

/// ASCII-lowercased copy preserves byte length, so keyword offsets stay valid in the original.
fn first_tackle_split(body_lc: &str) -> Option<usize> {
    TACKLE_KEYWORDS.iter().filter_map(|kw| body_lc.find(kw)).min()
}

/// Strip a leading `!bping <audience>` directorbot prefix. Returns (message body, was_bping).
fn strip_bping(text: &str) -> (&str, bool) {
    let t = text.trim_start();
    if t.get(..6).is_some_and(|p| p.eq_ignore_ascii_case("!bping")) {
        let rest = t[6..].trim_start();
        // Drop the audience token ("all", "skirmish", a group name).
        let body = rest.split_once(char::is_whitespace).map(|(_, tail)| tail.trim_start()).unwrap_or("");
        (body, true)
    } else {
        (t, false)
    }
}

/// Best-effort parse of a delve911 line. Always returns an event; unparsed fields stay `None`
/// and `is_ping` is false so plain chatter falls back to raw text. `ships` is the SDE name->group
/// map used to recognise a specific hull named in the ping.
pub fn parse_event(
    author: &str,
    text: &str,
    received: i64,
    systems: &Systems,
    ships: &HashMap<String, String>,
) -> RescueEvent {
    let trimmed = text.trim();
    let (body, has_bping) = strip_bping(trimmed);
    let body_lc = body.to_ascii_lowercase();

    let system = detect_system(body, systems);
    let tackle = first_tackle_split(&body_lc);

    let pilot = tackle.and_then(|pos| trailing_name(&body[..pos]));
    let cyno = body_lc.find("cyno").and_then(|pos| leading_name(&body[pos + "cyno".len()..]));
    let anomaly = detect_anomaly(body);
    let cap_class = detect_cap_class(&body_lc, ships);

    let is_ping = has_bping || (system.is_some() && tackle.is_some());

    RescueEvent {
        seq: next_seq(),
        received,
        author: author.to_string(),
        raw: trimmed.to_string(),
        is_ping,
        system_id: system.as_ref().map(|(id, _)| *id),
        system_name: system.map(|(_, name)| name),
        pilot,
        cyno,
        anomaly,
        cap_class,
    }
}

/// Parse a raw in-game d-scan paste (tab-separated `id\tname\ttype\tdistance`) into ship-type
/// counts, sorted most-common first. Lines that don't have the expected columns are skipped.
pub fn parse_raw_dscan(text: &str) -> Vec<(String, u32)> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }
        let ty = cols[2].trim();
        if ty.is_empty() {
            continue;
        }
        *counts.entry(ty.to_string()).or_insert(0) += 1;
    }
    let mut out: Vec<(String, u32)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Nearest cyno-generator system to `from`, searching over gates AND jump bridges. Flags whether
/// the shortest path uses a bridge (bubble-blockade risk). `None` when no generators configured.
pub fn nearest_cyno(systems: &Systems, from: i64, cyno_gens: &[i64]) -> Option<CynoInfo> {
    if cyno_gens.is_empty() {
        return None;
    }
    let targets: HashSet<i64> = cyno_gens.iter().copied().collect();
    let name_of = |id: i64| systems.info_of(id).map(|i| i.name.clone()).unwrap_or_default();
    if targets.contains(&from) {
        return Some(CynoInfo {
            in_system: true,
            reachable: true,
            jumps: 0,
            dest_id: from,
            dest_name: name_of(from),
            via_bridge: false,
        });
    }
    let mut prev: HashMap<i64, i64> = HashMap::new();
    let mut visited: HashSet<i64> = HashSet::from([from]);
    let mut queue: VecDeque<i64> = VecDeque::from([from]);
    while let Some(sys) = queue.pop_front() {
        for &n in systems.neighbors(sys) {
            if visited.contains(&n) {
                continue;
            }
            if crate::geo::is_no_transit(n) && !targets.contains(&n) {
                continue;
            }
            prev.insert(n, sys);
            if targets.contains(&n) {
                let mut path = vec![n];
                let mut cur = n;
                while let Some(&p) = prev.get(&cur) {
                    path.push(p);
                    cur = p;
                    if p == from {
                        break;
                    }
                }
                path.reverse();
                let via_bridge = path.windows(2).any(|w| systems.is_bridge(w[0], w[1]));
                return Some(CynoInfo {
                    in_system: false,
                    reachable: true,
                    jumps: (path.len() - 1) as u32,
                    dest_id: n,
                    dest_name: name_of(n),
                    via_bridge,
                });
            }
            visited.insert(n);
            queue.push_back(n);
        }
    }
    Some(CynoInfo {
        in_system: false,
        reachable: false,
        jumps: 0,
        dest_id: 0,
        dest_name: String::new(),
        via_bridge: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{Systems, SystemInfo};
    use std::collections::HashMap;

    fn test_ships() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("phoenix navy issue".to_string(), "Dreadnought".to_string());
        m.insert("naglfar".to_string(), "Dreadnought".to_string());
        m.insert("bane".to_string(), "Lancer Dreadnought".to_string());
        m.insert("rorqual".to_string(), "Capital Industrial Ship".to_string());
        m.insert("avatar".to_string(), "Titan".to_string());
        m.insert("apostle".to_string(), "Force Auxiliary".to_string());
        m
    }

    fn test_systems() -> Systems {
        let mut by_name: HashMap<String, SystemInfo> = HashMap::new();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut add = |name: &str, id: i64| {
            by_name.insert(
                name.to_lowercase(),
                SystemInfo {
                    id,
                    name: name.to_string(),
                    security: -0.4,
                    constellation: String::new(),
                    region: "Delve".to_string(),
                    faction: String::new(),
                },
            );
        };
        add("C6CZ-6", 30_004_700);
        add("C-J6MT", 30_004_701);
        add("1DQ1-A", 30_004_702);
        add("YZ9-F6", 30_004_703);
        // Gate graph only: C6CZ-6 -- 1DQ1-A -- C-J6MT. YZ9-F6 is reachable only via a bridge, so
        // it must be absent from the gate adjacency and added by add_bridges afterwards.
        adjacency.insert(30_004_700, vec![30_004_702]);
        adjacency.insert(30_004_702, vec![30_004_700, 30_004_701]);
        adjacency.insert(30_004_701, vec![30_004_702]);
        let mut s = Systems::new(by_name, adjacency);
        // YZ9-F6 <-> C6CZ-6 jump bridge (in adjacency, absent from the gate graph).
        s.add_bridges(&[(30_004_703, 30_004_700)]);
        s
    }

    #[test]
    fn parses_full_bping() {
        let s = test_systems();
        let ships = test_ships();
        let ev = parse_event(
            "SomeFC",
            "!bping all Ragnar Solberg tackled in C6CZ-6 (Ruins of Enclave), cyno Scout Alt",
            1000,
            &s,
            &ships,
        );
        assert!(ev.is_ping);
        assert_eq!(ev.system_id, Some(30_004_700));
        assert_eq!(ev.pilot.as_deref(), Some("Ragnar Solberg"));
        assert_eq!(ev.cyno.as_deref(), Some("Scout Alt"));
        assert_eq!(ev.anomaly.as_deref(), Some("Ruins of Enclave"));
    }

    #[test]
    fn partial_bping_still_ping() {
        let s = test_systems();
        let ev = parse_event("FC", "!bping all need help fast", 1000, &s, &test_ships());
        assert!(ev.is_ping);
        assert_eq!(ev.system_id, None);
        assert_eq!(ev.pilot, None);
    }

    #[test]
    fn non_ping_falls_back_to_raw() {
        let s = test_systems();
        let ev = parse_event("Rando", "anyone around for a quick hand later?", 1000, &s, &test_ships());
        assert!(!ev.is_ping);
        assert_eq!(ev.raw, "anyone around for a quick hand later?");
    }

    #[test]
    fn detects_capital_class() {
        let s = test_systems();
        let ships = test_ships();
        // Category keyword.
        let ev = parse_event("FC", "dread tackled in 1DQ1-A", 1, &s, &ships);
        assert_eq!(ev.cap_class, Some(CapClass::Dread));
        assert_eq!(ev.system_id, Some(30_004_702));
        // Specific hull names resolve via SDE group.
        assert_eq!(
            parse_event("FC", "Phoenix Navy Issue tackled in 1DQ1-A", 1, &s, &ships).cap_class,
            Some(CapClass::Dread)
        );
        assert_eq!(
            parse_event("FC", "Rorqual tackled in 1DQ1-A", 1, &s, &ships).cap_class,
            Some(CapClass::Rorqual)
        );
        assert_eq!(
            parse_event("FC", "Bane pointed in 1DQ1-A", 1, &s, &ships).cap_class,
            Some(CapClass::Dread)
        );
    }

    #[test]
    fn classify_covers_key_groups() {
        assert_eq!(classify("Titan"), ShipRole::Titan);
        assert_eq!(classify("Supercarrier"), ShipRole::Supercarrier);
        assert_eq!(classify("Force Auxiliary"), ShipRole::Fax);
        assert_eq!(classify("Dreadnought"), ShipRole::Dread);
        assert_eq!(classify("Carrier"), ShipRole::Carrier);
        assert_eq!(classify("Logistics Cruiser"), ShipRole::Logi);
        assert_eq!(classify("Logistics Frigate"), ShipRole::LogiFrig);
        assert_eq!(classify("Interdictor"), ShipRole::Dictor);
        assert_eq!(classify("Heavy Interdiction Cruiser"), ShipRole::Hictor);
        assert_eq!(classify("Force Recon Ship"), ShipRole::Recon);
        assert_eq!(classify("Combat Recon Ship"), ShipRole::Recon);
        assert_eq!(classify("Command Ship"), ShipRole::Booster);
        assert_eq!(classify("Command Destroyer"), ShipRole::CommandDest);
        assert_eq!(classify("Assault Frigate"), ShipRole::Dps);
    }

    #[test]
    fn timer_counts_down_and_adjusts() {
        let mut t = Timer::default();
        assert!(!t.is_set());
        t.adjust(5);
        assert_eq!(t.current(), 5);
        t.adjust(5);
        assert_eq!(t.current(), 10);
        t.adjust(-100);
        assert_eq!(t.current(), 0);
        t.start(120);
        assert!(t.is_set());
        assert!(t.current() <= 120 && t.current() >= 118);
    }

    #[test]
    fn fleet_snapshot_counts_roles() {
        let mk = |id: i64, name: &str, group: &str| FleetMember {
            character_id: id,
            name: name.to_string(),
            ship_type_id: 0,
            ship: String::new(),
            group: group.to_string(),
            role: classify(group),
            system_id: 0,
        };
        let members = vec![
            mk(1, "Cap Pilot", "Titan"),
            mk(2, "Logi One", "Logistics Cruiser"),
            mk(3, "Logi Two", "Logistics Cruiser"),
            mk(4, "Scout", "Force Recon Ship"),
        ];
        let snap = FleetSnapshot::build(Some(42), members, 1000);
        assert_eq!(snap.count(ShipRole::Logi), 2);
        assert!(snap.has_role(ShipRole::Titan));
        assert!(snap.has_pilot("cap pilot"));
        assert!(!snap.has_pilot("nobody"));
    }

    #[test]
    fn raw_dscan_counts_by_type() {
        let text = "1001\tGuy A\tMegathron\t120 km\n1002\tGuy B\tMegathron\t-\n1003\tGuy C\tScimitar\t14 AU";
        let counts = parse_raw_dscan(text);
        assert_eq!(counts[0], ("Megathron".to_string(), 2));
        assert_eq!(counts[1], ("Scimitar".to_string(), 1));
    }

    #[test]
    fn nearest_cyno_in_system_and_via_bridge() {
        let s = test_systems();
        // Generator sitting in C-J6MT (30_004_701).
        let gens = vec![30_004_701];
        // From the generator system itself.
        let here = nearest_cyno(&s, 30_004_701, &gens).unwrap();
        assert!(here.in_system);
        // From C6CZ-6: C6CZ-6 -> 1DQ1-A -> C-J6MT, 2 gate jumps, no bridge.
        let route = nearest_cyno(&s, 30_004_700, &gens).unwrap();
        assert!(!route.in_system && route.reachable);
        assert_eq!(route.jumps, 2);
        assert!(!route.via_bridge);
        // From YZ9-F6: only exit is the bridge to C6CZ-6, so the path must flag a bridge.
        let bridged = nearest_cyno(&s, 30_004_703, &gens).unwrap();
        assert!(bridged.reachable && bridged.via_bridge);
    }
}
