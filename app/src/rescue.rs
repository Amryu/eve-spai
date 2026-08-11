//! delve911 capital-rescue support: parsing emergency pings, fleet-composition
//! classification and the shared `RescueState`.
//!
//! Everything here is pure logic with no shared locks or I/O, so the watcher can run
//! `parse_event` under `catch_unwind` and a malformed line drops one event instead of
//! taking down the app. `RescueState` lives behind an `Arc<Mutex<..>>` on the app and is
//! the single source of truth for the feature.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

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
    "carriers", "fax", "super", "supers", "titan", "titans", "rorq", "rorqs", "rorqual",
    "rorquals", "moon", "anom", "anomaly", "site",
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub ship: String,
    #[allow(dead_code)]
    pub group: String,
    pub role: ShipRole,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

/// Per-ping record of the three actions the FC is expected to take. Comms actions remember WHICH
/// op they were done for, so changing the op channel re-arms them.
#[derive(Default, Clone, Copy)]
pub struct PingActions {
    pub command_comms_op: Option<u8>,
    pub coord_pinged: bool,
    pub invited_op: Option<u8>,
}

impl PingActions {
    pub fn done(&self, op: u8) -> bool {
        self.coord_pinged && self.command_comms_op == Some(op) && self.invited_op == Some(op)
    }
}

#[derive(Default)]
pub struct RescueState {
    pub active: bool,
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
    /// delve911 nick that raised the selected ping, used to call them into comms.
    pub ping_author: Option<String>,
    /// `seq` of the ping being worked. The capital_* fields above are its details.
    pub selected_ping: Option<u64>,
    /// `seq` of pings the FC dismissed; they drop out of the recent-pings list.
    pub resolved: HashSet<u64>,
    /// Which of the expected actions have been taken, per ping `seq`.
    pub actions: HashMap<u64, PingActions>,
    #[allow(dead_code)]
    pub dscan: Option<Vec<(String, u32)>>,
    pub op_channel: u8,
    pub doctrine: String,
    /// The editable outgoing ping. Empty until first shown, then filled from the template and
    /// freely edited by the FC before sending.
    pub pending_ping: String,
    /// (op_channel, doctrine, selected ping) the `pending_ping` was last generated for.
    pub ping_built_for: Option<(u8, String, Option<u64>)>,
    /// The FC has typed into `pending_ping`, so an op/doctrine change must not clobber it.
    pub ping_edited: bool,
    /// Draft reply typed into the chat view, sent to the selected tab's room.
    pub delve911_reply: String,
    /// Selected chat tab: 0 = delve911, 1 = skirmish_commanders.
    pub chat_tab: u8,
    pub fleet: FleetSnapshot,
    /// Sticky snowflakes: character_id -> reason tag. Once flagged (capital/cyno/titan/recon) a
    /// pilot STAYS flagged for the session even if they re-ship to a pod, so the FC keeps tracking
    /// them. Keyed by character_id (survives ship changes).
    #[allow(dead_code)]
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
        // Pilots routinely submit the same ping twice a second apart, which clears the chat store's
        // UNIQUE(jid, time, sender, body) guard.
        if self.events.iter().rev().take(4).any(|p| {
            p.author == ev.author && p.raw == ev.raw && (ev.received - p.received).abs() <= 5
        }) {
            return;
        }
        let fresh_ping = ev.is_ping;
        self.events.push(ev);
        if self.events.len() > EVENTS_CAP {
            let drop = self.events.len() - EVENTS_CAP;
            self.events.drain(0..drop);
        }
        // A new ping never steals focus from the one being worked; it just joins the list.
        if fresh_ping && self.selected_ping.is_none() {
            self.select_newest();
        }
    }

    /// Most recent unresolved pings, newest first, capped at `n`.
    pub fn recent_pings(&self, n: usize) -> Vec<&RescueEvent> {
        self.events
            .iter()
            .rev()
            .filter(|e| e.is_ping && !self.resolved.contains(&e.seq))
            .take(n)
            .collect()
    }

    pub fn select_ping(&mut self, seq: u64) {
        self.selected_ping = Some(seq);
        self.apply_selection();
    }

    /// Dismiss a ping. If it was the one being worked, fall through to the next newest.
    pub fn resolve_ping(&mut self, seq: u64) {
        self.resolved.insert(seq);
        if self.selected_ping == Some(seq) {
            self.select_newest();
        }
    }

    pub fn actions(&self, seq: u64) -> PingActions {
        self.actions.get(&seq).copied().unwrap_or_default()
    }

    pub fn actions_mut(&mut self, seq: u64) -> &mut PingActions {
        self.actions.entry(seq).or_default()
    }

    pub fn select_newest(&mut self) {
        self.selected_ping = self.recent_pings(1).first().map(|e| e.seq);
        self.apply_selection();
    }

    /// Copy the selected ping's details into the fields the ping template and checklist read.
    fn apply_selection(&mut self) {
        let picked = self
            .selected_ping
            .and_then(|seq| self.events.iter().find(|e| e.seq == seq))
            .cloned();
        let Some(ev) = picked else {
            self.ping_author = None;
            self.capital_system = None;
            self.capital_system_name = None;
            self.capital_pilot = None;
            self.cyno_pilot = None;
            self.anomaly = None;
            self.cap_class = None;
            return;
        };
        self.ping_author = Some(ev.author);
        self.capital_system = ev.system_id;
        self.capital_system_name = ev.system_name;
        self.capital_pilot = ev.pilot;
        self.cyno_pilot = ev.cyno;
        self.anomaly = ev.anomaly;
        self.cap_class = ev.cap_class;
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
        // Split on anything that can't be part of a system name (keep '-', which nullsec names use).
        // This peels off decorations like `*`, `:`, `/`, `()` even when glued on with no space, so
        // "System:C-J6MT", "*C-J6MT*" and "C-J6MT*gate" all still resolve.
        for raw in text.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
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

/// Which template field a `label:` introduces.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Pilot,
    Cyno,
    System,
    Anomaly,
    Ignore,
}

/// Normalised label key: cut at the first `(` to drop the template's own hints, then keep only
/// letters. `Location In System (Anomaly or Moon)`, `Location In System :` and the real typo
/// `Location I  n System` all collapse to `locationinsystem`.
fn label_key(raw: &str) -> String {
    raw.split('(')
        .next()
        .unwrap_or(raw)
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Order matters: `locationinsystem` contains both "location" and "system", and `cynopilot`
/// contains both "cyno" and "pilot".
fn classify_label(key: &str) -> Option<Field> {
    if key.len() < 3 || key.len() > 48 {
        return None;
    }
    if key.contains("cyno") {
        return Some(if key.contains("inhib") { Field::Ignore } else { Field::Cyno });
    }
    if key.starts_with("sys") {
        return Some(Field::System);
    }
    if key.contains("location") || key.contains("anomaly") || key.contains("moon") {
        return Some(Field::Anomaly);
    }
    if key.contains("system") {
        return Some(Field::System);
    }
    if key.contains("name") || key.contains("pilot") {
        return Some(Field::Pilot);
    }
    if key.contains("panic") {
        return Some(Field::Ignore);
    }
    None
}

#[derive(Default)]
struct Labeled {
    pilot: Option<String>,
    cyno: Option<String>,
    system: Option<String>,
    anomaly: Option<String>,
    hits: usize,
}

/// Pull `Label: value` pairs out of the delve911 ping template. Values run to end of line or to
/// the next label on the same line (real pings glue two fields onto one line).
fn scan_labels(body: &str) -> Labeled {
    let mut out = Labeled::default();
    for line in body.lines() {
        let mut marks: Vec<(Field, usize)> = Vec::new();
        let mut cursor = 0usize;
        for (i, c) in line.char_indices() {
            if (c != ':' && c != ';') || i < cursor {
                continue;
            }
            if let Some(field) = classify_label(&label_key(&line[cursor..i])) {
                marks.push((field, i + c.len_utf8()));
                cursor = i + c.len_utf8();
            }
        }
        for (n, (field, start)) in marks.iter().enumerate() {
            // A following label's text is still part of this slice; each field's own cleaner
            // discards it (a system token survives, prose does not).
            let end = marks.get(n + 1).map(|(_, s)| *s).unwrap_or(line.len());
            let raw = line[*start..end].trim().trim_end_matches([':', ';']);
            if *field != Field::Ignore {
                out.hits += 1;
            }
            let slot = match field {
                Field::Pilot => &mut out.pilot,
                Field::Cyno => &mut out.cyno,
                Field::System => &mut out.system,
                Field::Anomaly => &mut out.anomaly,
                Field::Ignore => continue,
            };
            let value = match field {
                Field::Pilot | Field::Cyno => ping_name(raw),
                Field::Anomaly => clean_anomaly(raw),
                _ => (!raw.is_empty()).then(|| raw.to_string()),
            };
            if slot.is_none() {
                *slot = value;
            }
        }
    }
    out
}

/// Looser than `dscan::is_valid_char_name`, which caps the last word at 12 chars and so rejects
/// real pilots like `umakedasammich`. The label already anchors the value, so no stop-word pass.
fn ping_name(raw: &str) -> Option<String> {
    let s = raw
        .split('(')
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if !(3..=37).contains(&s.len()) || !(1..=3).contains(&s.split_whitespace().count()) {
        return None;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '\'' | '-' | '.'))
        .then(|| s.to_string())
}

/// Anomaly values are often a pasted probe-scanner row, tab separated with columns we don't want.
fn clean_anomaly(raw: &str) -> Option<String> {
    let noise = |f: &str| {
        let l = f.to_ascii_lowercase();
        l == "cosmic anomaly"
            || l == "cosmic signature"
            || l == "ore site"
            || l.ends_with('%')
            || l.ends_with(" au")
            || l.ends_with(" m")
            || l.ends_with(" km")
    };
    let kept: Vec<&str> =
        raw.split('\t').map(str::trim).filter(|f| !f.is_empty() && !noise(f)).collect();
    let out = if kept.is_empty() { raw.trim().to_string() } else { kept.join(" ") };
    (out.len() >= 3).then_some(out)
}

/// Second chance for a labelled system that didn't resolve: pilots drop the hyphen (`GDHNK`) or
/// glue digits on (`UM-SCG44`). Hyphen insertion only commits when exactly one candidate resolves.
fn repair_system(text: &str, systems: &Systems) -> Option<(i64, String)> {
    for raw in text.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
        let tok = clean_token(raw);
        if !(4..=8).contains(&tok.len()) {
            continue;
        }
        if tok.contains('-') {
            let trimmed = tok.trim_end_matches(|c: char| c.is_ascii_digit());
            if trimmed.len() >= 4 && trimmed != tok {
                if let Some(info) = systems.lookup(trimmed) {
                    return Some((info.id, info.name.clone()));
                }
            }
            continue;
        }
        if tok.len() > 6 || !tok.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let mut hit = None;
        for cut in 1..tok.len() {
            let cand = format!("{}-{}", &tok[..cut], &tok[cut..]);
            if let Some(info) = systems.lookup(&cand) {
                if hit.is_some() {
                    hit = None;
                    break;
                }
                hit = Some((info.id, info.name.clone()));
            }
        }
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// Blank out every SDE hull name, byte-for-byte, so the prose fallback can't read a ship as a
/// pilot ("Rorqual Tackled" -> pilot "Rorqual", "Phoenix navy issue" -> pilot "navy issue").
fn mask_ships(body: &str, ships: &HashMap<String, String>) -> String {
    let lc = body.to_ascii_lowercase();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in lc.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                spans.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        spans.push((s, lc.len()));
    }

    let mut out = body.as_bytes().to_vec();
    let mut a = 0usize;
    while a < spans.len() {
        let mut step = 1;
        for len in (1..=4.min(spans.len() - a)).rev() {
            let (lo, hi) = (spans[a].0, spans[a + len - 1].1);
            let cand = lc[lo..hi].trim_matches(|c: char| !c.is_alphanumeric());
            if cand.len() >= 3 && ships.contains_key(cand) {
                out[lo..hi].fill(b' ');
                step = len;
                break;
            }
        }
        a += step;
    }
    String::from_utf8(out).unwrap_or_else(|_| body.to_string())
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

    let labels = scan_labels(body);
    let tackle = first_tackle_split(&body_lc);

    let mut system = labels
        .system
        .as_deref()
        .and_then(|v| detect_system(v, systems).or_else(|| repair_system(v, systems)));
    if system.is_none() {
        system = detect_system(body, systems);
    }

    let (mut pilot, mut cyno, mut anomaly) = (labels.pilot, labels.cyno, labels.anomaly);
    if labels.hits == 0 {
        // Prose ping. Mask hulls first or the ship word becomes the pilot.
        let masked = mask_ships(body, ships);
        pilot = tackle.and_then(|pos| trailing_name(&masked[..pos]));
        cyno = body_lc.find("cyno").and_then(|pos| {
            leading_name(&masked[pos + "cyno".len()..])
                .or_else(|| trailing_name(&masked[..pos]))
        });
        anomaly = detect_anomaly(body);
    }

    let cap_class = detect_cap_class(&body_lc, ships);
    let is_ping = has_bping || labels.hits >= 2 || (system.is_some() && tackle.is_some());

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
        // Systems named by the real delve911 pings used as parser fixtures below.
        add("UM-SCG", 30_004_710);
        add("C8H5-X", 30_004_711);
        add("L-Z9KJ", 30_004_712);
        add("Q7-FZ8", 30_004_713);
        add("EFM-C4", 30_004_714);
        add("M9-MLR", 30_004_715);
        add("GD-HNK", 30_004_716);
        add("AGCP-I", 30_004_717);
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
    fn detects_decorated_system() {
        let s = test_systems();
        let id = |t: &str| detect_system(t, &s).map(|(id, _)| id);
        assert_eq!(id("System:C-J6MT"), Some(30_004_701)); // glued by a colon, no space
        assert_eq!(id("*C-J6MT*"), Some(30_004_701)); // waypoint stars
        assert_eq!(id("tackled C-J6MT*gate"), Some(30_004_701)); // glued suffix
        assert_eq!(id("(1DQ1-A)"), Some(30_004_702)); // parenthesised
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

    /// All fixtures below are real delve911 pings, verbatim, including their typos and spacing.
    fn ev(text: &str) -> RescueEvent {
        parse_event("pilot", text, 1, &test_systems(), &test_ships())
    }

    #[test]
    fn parses_canonical_template() {
        let e = ev("!bping all Rorqual Tackled \n Rorqual Name: Ajunta Thor \n System: UM-SCG \n \
                    Location In System (Anomaly or Moon): Planet 8 Moon 4 \n \
                    Name of Cyno (On perch, 300km away): Metal Viper");
        assert!(e.is_ping);
        assert_eq!(e.pilot.as_deref(), Some("Ajunta Thor"));
        assert_eq!(e.system_id, Some(30_004_710));
        assert_eq!(e.cyno.as_deref(), Some("Metal Viper"));
        assert_eq!(e.anomaly.as_deref(), Some("Planet 8 Moon 4"));
        assert_eq!(e.cap_class, Some(CapClass::Rorqual));
    }

    #[test]
    fn parses_semicolon_labels() {
        let e = ev("!bping all Rorqual Tackled  \n Rorqual name;  spiderd58 \n \
                    Sys Location;   AGCP-I ` \n Cyno Pilot; SpecOps-58  \n Cyno Inhib;");
        assert_eq!(e.pilot.as_deref(), Some("spiderd58"));
        assert_eq!(e.system_id, Some(30_004_717));
        assert_eq!(e.cyno.as_deref(), Some("SpecOps-58"));
    }

    #[test]
    fn parses_misspelt_label_and_long_name() {
        let e = ev("!bping all Rorqual Tackled \n Rorqual Name:  Randon Angus  \n \
                    System:   L-Z9KJ   \n \
                    Location I  n System (Anomaly or Moon): TBC-13 Large UGEANITE \n \
                    Name of Cyno (On perch, 300km away):   umakedasammich");
        assert_eq!(e.pilot.as_deref(), Some("Randon Angus"));
        assert_eq!(e.system_id, Some(30_004_712));
        assert_eq!(e.anomaly.as_deref(), Some("TBC-13 Large UGEANITE"));
        assert_eq!(e.cyno.as_deref(), Some("umakedasammich"));
    }

    #[test]
    fn parses_two_labels_on_one_line() {
        let e = ev("!bping all Rorqual Tackled  \n Rorqual Name: Legend atenz \n \
                    System: C8H5-X Location In System (Anomaly or Moon): Large nocxic \n \
                    Name of Cyno (On perch, 300km away): Dirtylegendo");
        assert_eq!(e.system_id, Some(30_004_711));
        assert_eq!(e.anomaly.as_deref(), Some("Large nocxic"));
        assert_eq!(e.cyno.as_deref(), Some("Dirtylegendo"));
    }

    #[test]
    fn empty_system_label_falls_back_to_body() {
        let e = ev("!bping all Rorqual Tackled \n Rorqual Name: Roast Potatoes \n \
                    System: Location In System (Anomaly or Moon): M9-MLR HEZORIME \n \n \
                    Name of Cyno (On perch, 300km away): Kaesong Derpfestor");
        assert_eq!(e.system_id, Some(30_004_715));
        assert_eq!(e.pilot.as_deref(), Some("Roast Potatoes"));
        assert_eq!(e.cyno.as_deref(), Some("Kaesong Derpfestor"));
    }

    #[test]
    fn repairs_system_and_cleans_pasted_anomaly() {
        let e = ev("!bping all Rorqual Tackled  \n Rorqual Name: Max Odious \n \
                    System: UM-SCG44 Location In System (Anomaly or Moon): \
                    TXS-339\tCosmic Anomaly\tOre Site\tLarge Hezorime Deposit\t100.0%\t1,185 m \n \
                    Name of Cyno (On perch, 300km away): Feelthelove");
        assert_eq!(e.system_id, Some(30_004_710)); // UM-SCG44 -> UM-SCG
        assert_eq!(e.anomaly.as_deref(), Some("TXS-339 Large Hezorime Deposit"));
    }

    #[test]
    fn repairs_missing_hyphen_in_system() {
        let e = ev("!bping all Rorqual Tackled \n System:  GDHNK \n \
                    Location In System (Anomaly or Moon): Large Grimeer Deposit \n \
                    Name of Cyno (On perch, 300km away): Pol Pitran");
        assert_eq!(e.system_id, Some(30_004_716)); // GDHNK -> GD-HNK
        assert_eq!(e.cyno.as_deref(), Some("Pol Pitran"));
    }

    #[test]
    fn cyno_value_drops_trailing_hint() {
        let e = ev("!bping all Rorqual Tackled \n Rorqual Name: Melissa Rin \n System: L-Z9KJ \n \
                    Name of Cyno: Evance Glann (On perch, 300km away):");
        assert_eq!(e.cyno.as_deref(), Some("Evance Glann"));
    }

    #[test]
    fn ignores_panic_and_inhib_labels() {
        let e = ev("!bping all Rorqual Tackled \n Rorqual Name: Eben Auditore \n \
                    System:  Q7-FZ8*  \n \
                    Location In System (Anomaly or Moon): ZXG-740 Large Nocxite Deposit \n \
                    Name of Cyno (On perch, 300km away): NuoMi CC \n Panic: yes \n \
                    Is a Cyno Inhib online?:yes");
        assert_eq!(e.system_id, Some(30_004_713));
        assert_eq!(e.cyno.as_deref(), Some("NuoMi CC"));
        assert_eq!(e.pilot.as_deref(), Some("Eben Auditore"));
    }

    #[test]
    fn template_without_bping_is_still_a_ping() {
        let e = ev("Rorqual Tackled \n Rorqual Name: Jaa Mo \n System:  EFM-C4  \n \
                    Location In System (Anomaly  or Moon): Planet 1 moon 4 \n \
                    Name of Cyno (On perch, 300km away): High Rankin");
        assert!(e.is_ping);
        assert_eq!(e.pilot.as_deref(), Some("Jaa Mo"));
        assert_eq!(e.system_id, Some(30_004_714));
    }

    #[test]
    fn comms_invite_is_addressed_and_regular() {
        let ev = parse_event(
            "ajunta_thor",
            "!bping all Rorqual Tackled \n Rorqual Name: Ajunta Thor \n System: UM-SCG",
            1,
            &test_systems(),
            &test_ships(),
        );
        let mut st = RescueState::default();
        st.push_event(ev);
        assert_eq!(st.ping_author.as_deref(), Some("ajunta_thor"));
    }

    #[test]
    fn hull_word_is_never_the_pilot() {
        // Prose fallback: the ship must not be read as the capital pilot's name.
        let e = ev("Rorq bubbled L-Z9KJ planet 9 moon 4");
        assert!(e.is_ping);
        assert_eq!(e.system_id, Some(30_004_712));
        assert_eq!(e.pilot, None);

        let e = ev("!bping all Phoenix navy issue tackled");
        assert_eq!(e.pilot, None);
        assert_eq!(e.cap_class, Some(CapClass::Dread));

        let e = ev("!bping all Rorqual Tackled");
        assert_eq!(e.pilot, None);
        assert_eq!(e.cap_class, Some(CapClass::Rorqual));
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
}
