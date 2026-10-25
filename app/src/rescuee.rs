//! The rescuee's side of a delve911 capital rescue: a tackled Rorqual's ping, the checklist after
//! it, the comms the FC posts and the hostiles reported back. Pure logic; `app/rescuee_ui` draws it.

use std::collections::HashMap;

use crate::esi::ShipState;
use crate::fleets::model::EsiFleet;

pub const RORQUAL_TYPE_ID: i64 = 28352;
pub const RORQUAL_GROUP: &str = "capital industrial ship";

/// A logged-in character undocked in a Rorqual, and the other characters undocked beside it, any
/// of which may be flying the cyno.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub rorqual: String,
    pub system: i64,
    pub cynos: Vec<String>,
}

/// Every Rorqual in space among our characters, by pilot name, with the cyno candidates in its
/// system. `ship_by_id` and `groups` are the app's ship index: the type id is enough, the group
/// catches a renamed or re-numbered hull.
pub fn detect(ships: &HashMap<String, ShipState>, ship_by_id: &HashMap<i64, String>, groups: &HashMap<String, String>) -> Vec<Detected> {
    let is_rorqual = |t: Option<i64>| {
        let Some(t) = t else { return false };
        t == RORQUAL_TYPE_ID
            || ship_by_id
                .get(&t)
                .and_then(|n| groups.get(&n.to_lowercase()))
                .is_some_and(|g| g.eq_ignore_ascii_case(RORQUAL_GROUP))
    };
    let mut out: Vec<Detected> = ships
        .iter()
        .filter(|(_, s)| !s.docked && is_rorqual(s.ship_type))
        .map(|(name, s)| {
            let mut cynos: Vec<String> = ships
                .iter()
                .filter(|(n, c)| *n != name && !c.docked && c.system == s.system)
                .map(|(n, _)| n.clone())
                .collect();
            cynos.sort();
            Detected { rorqual: name.clone(), system: s.system, cynos }
        })
        .collect();
    out.sort_by(|a, b| a.rorqual.cmp(&b.rorqual));
    out
}

/// Where in the system the Rorqual is, as the FC needs it written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Location {
    #[default]
    Unknown,
    /// An anomaly or signature: its English name and the probe scanner's code.
    Anomaly { name_en: String, code: Option<String> },
    /// `{system} {planet} - Moon {moon}`, the planet in Roman numerals.
    Moon { system: Option<String>, planet: String, moon: u32 },
    Free(String),
}

impl Location {
    /// The English text the ping carries; empty when unknown.
    pub fn render(&self) -> String {
        match self {
            Location::Unknown => String::new(),
            Location::Anomaly { name_en, code } => match code {
                Some(c) => format!("{name_en} {c}"),
                None => name_en.clone(),
            },
            Location::Moon { system, planet, moon } => format!("{} {planet} - Moon {moon}", system.as_deref().unwrap_or("Planet")),
            Location::Free(s) => s.clone(),
        }
    }
}

/// The word for a moon in each client language, so a pasted localized name turns into the English
/// one every FC reads.
const MOON_WORDS: &[&str] = &["moon", "mond", "lune", "luna", "луна", "卫星", "衛星", "위성"];
const PLANET_WORDS: &[&str] = &["planet", "planète", "planeta", "планета", "行星", "惑星", "행성"];

/// Reads what the pilot typed or pasted: a probe-scanner row, a moon in any client language, an
/// anomaly name with its code, or anything else as free text. `anomaly_i18n` maps lowercased
/// localized site names to English.
pub fn parse_location(input: &str, anomaly_i18n: &HashMap<String, String>) -> Location {
    let text = input.trim();
    if text.is_empty() {
        return Location::Unknown;
    }
    let translate = |name: &str| -> String {
        let n = name.trim();
        anomaly_i18n.get(&n.to_lowercase()).cloned().unwrap_or_else(|| n.to_owned())
    };
    // A probe scanner row: "ABC-123\tCosmic Anomaly\tCombat Site\tAngel Forsaken Hub\t100%\t2.1 AU".
    if let Some(sig) = spai_core::wormholes::probe_scan(text).into_iter().next() {
        let name = if sig.name.trim().is_empty() { sig.group.clone() } else { sig.name.clone() };
        return Location::Anomaly { name_en: translate(&name), code: Some(sig.id) };
    }
    if let Some(moon) = parse_moon(text) {
        return moon;
    }
    // "Angel Forsaken Hub ABC-123", or the code first.
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() >= 2 {
        if let Some(code) = words.last().filter(|w| spai_core::wormholes::is_sig_id(&w.to_uppercase())) {
            let name = words[..words.len() - 1].join(" ");
            return Location::Anomaly { name_en: translate(&name), code: Some(code.to_uppercase()) };
        }
        if let Some(code) = words.first().filter(|w| spai_core::wormholes::is_sig_id(&w.to_uppercase())) {
            let name = words[1..].join(" ");
            return Location::Anomaly { name_en: translate(&name), code: Some(code.to_uppercase()) };
        }
    }
    let translated = translate(text);
    if translated != text {
        return Location::Anomaly { name_en: translated, code: None };
    }
    Location::Free(text.to_owned())
}

/// "1DQ1-A IV - Moon 2", "Planet IV - Mond 2", "IV - Луна 2": the system or planet word, the
/// planet's numeral, the moon's number.
fn parse_moon(text: &str) -> Option<Location> {
    // The last dash: system names carry one of their own ("1DQ1-A IV - Moon 2").
    let (head, tail) = text.rsplit_once('-')?;
    let mut tail_words = tail.split_whitespace();
    let word = tail_words.next()?.to_lowercase();
    let moon: u32 = match tail_words.next() {
        Some(n) => n.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?,
        // "Moon2" written together.
        None => {
            let digits: String = word.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()?
        }
    };
    let word_only: String = word.chars().filter(|c| !c.is_ascii_digit()).collect();
    if !MOON_WORDS.contains(&word_only.as_str()) {
        return None;
    }
    let mut head_words: Vec<&str> = head.split_whitespace().collect();
    let planet = head_words.pop()?;
    if planet.is_empty() || !planet.chars().all(|c| "IVXLCivxlc".contains(c)) {
        return None;
    }
    let system = head_words.join(" ");
    let system = (!system.is_empty() && !PLANET_WORDS.contains(&system.to_lowercase().as_str())).then_some(system);
    Some(Location::Moon { system, planet: planet.to_uppercase(), moon })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panic {
    #[default]
    Unknown,
    /// Fitted and ready.
    Available,
    /// Running now.
    Activated,
    /// Already spent.
    Used,
    None,
}

impl Panic {
    pub fn render(self) -> &'static str {
        match self {
            Panic::Unknown => "?",
            Panic::Available => "yes",
            Panic::Activated => "activated",
            Panic::Used => "used",
            Panic::None => "no",
        }
    }
}

/// What the pilot knows of the hostiles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hostiles {
    pub count: Option<u32>,
    /// Ship type and how many, as a d-scan or the pilot's picks say.
    pub ships: Vec<(String, u32)>,
    pub alliance: Option<String>,
    pub dscan_link: Option<String>,
}

impl Hostiles {
    pub fn is_empty(&self) -> bool {
        self.count.is_none() && self.ships.is_empty() && self.alliance.is_none()
    }

    /// "7 (Sabre x2, Loki x3) - Fraternity", whatever parts are known.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(n) = self.count {
            out.push_str(&n.to_string());
        }
        if !self.ships.is_empty() {
            let list: Vec<String> = self.ships.iter().map(|(s, n)| if *n > 1 { format!("{s} x{n}") } else { s.clone() }).collect();
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&format!("({})", list.join(", ")));
        }
        if let Some(a) = &self.alliance {
            if !out.is_empty() {
                out.push_str(" - ");
            }
            out.push_str(a);
        }
        out
    }
}

/// Everything the tackle ping says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PingFields {
    pub pilot: String,
    pub cyno: String,
    pub system: String,
    pub location: Location,
    pub panic: Panic,
    pub hostiles: Hostiles,
}

/// The ping's labels, in the order they are written: the form delve911 pings take, as the
/// room's own pilots post them and the FC side's parser reads them. Then what else is known.
pub const LABELS_EN: [&str; 7] = [
    "Rorqual Name",
    "System",
    "Location In System (Anomaly or Moon)",
    "Name of Cyno (On perch, 300km away)",
    "Hostile Fleet Dscan",
    "Hostiles",
    "Panic",
];
pub const HEADLINE_EN: &str = "Rorqual Tackled";
/// The room bot's broadcast prefix, which every posted ping leads with.
pub const BPING: &str = "!bping all";

/// The ping as it is posted: always English.
pub fn render_ping(f: &PingFields) -> String {
    let labels: [String; 7] = LABELS_EN.map(str::to_owned);
    render_ping_with(f, HEADLINE_EN, &labels)
}

/// The ping with the labels and headline in another language, for the preview only.
pub fn render_ping_with(f: &PingFields, headline: &str, labels: &[String; 7]) -> String {
    let or_q = |s: &str| if s.trim().is_empty() { "?".to_owned() } else { s.trim().to_owned() };
    let location = f.location.render();
    let mut out = format!(
        "{BPING} {headline}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
        labels[0],
        or_q(&f.pilot),
        labels[1],
        or_q(&f.system),
        labels[2],
        or_q(&location),
        labels[3],
        or_q(&f.cyno),
    );
    if let Some(link) = &f.hostiles.dscan_link {
        out.push_str(&format!("\n{}: {link}", labels[4]));
    }
    if !f.hostiles.is_empty() {
        out.push_str(&format!("\n{}: {}", labels[5], f.hostiles.render()));
    }
    if f.panic != Panic::Unknown {
        out.push_str(&format!("\n{}: {}", labels[6], f.panic.render()));
    }
    out
}

/// What is still missing from a ping, the FC's words for it.
pub fn missing(f: &PingFields) -> Vec<&'static str> {
    let mut v = Vec::new();
    if f.pilot.trim().is_empty() {
        v.push("pilot");
    }
    if f.system.trim().is_empty() {
        v.push("system");
    }
    if f.location == Location::Unknown {
        v.push("location");
    }
    v
}

/// A button that has to be pressed twice within three seconds, so a ping never goes out by accident.
#[derive(Clone, Copy, Debug, Default)]
pub struct Arm {
    first: Option<i64>,
}

pub const ARM_WINDOW_MS: i64 = 3000;

impl Arm {
    /// True on the second press within the window; the first press only arms.
    pub fn press(&mut self, now_ms: i64) -> bool {
        match self.first {
            Some(t) if now_ms - t <= ARM_WINDOW_MS => {
                self.first = None;
                true
            }
            _ => {
                self.first = Some(now_ms);
                false
            }
        }
    }

    /// Milliseconds left to confirm, while armed.
    pub fn left_ms(&self, now_ms: i64) -> Option<i64> {
        self.first.map(|t| ARM_WINDOW_MS - (now_ms - t)).filter(|l| *l > 0)
    }

    pub fn clear(&mut self) {
        self.first = None;
    }
}

/// The ping that went out, and what has been told since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    pub at: i64,
    pub body: String,
    pub fields: PingFields,
    /// What the FC has been told so far, for the follow-up diff.
    pub hostiles_sent: Hostiles,
    pub location_sent: Location,
}

/// The steps after the ping, as the pilot or the app ticks them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Checklist {
    pub siege_red: bool,
    pub location: bool,
    pub fleet: bool,
    pub directions: bool,
    pub hostiles: bool,
}

/// What the follow-up has to say, if anything: only what changed since the last post.
pub fn followup(sent: &Hostiles, now: &Hostiles, location_sent: &Location, location_now: &Location) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if location_now != location_sent && *location_now != Location::Unknown {
        parts.push(format!("{}: {}", LABELS_EN[2], location_now.render()));
    }
    if now.count != sent.count {
        if let Some(n) = now.count {
            parts.push(format!("Hostiles now {n}"));
        }
    }
    let was: HashMap<&str, u32> = sent.ships.iter().map(|(s, n)| (s.as_str(), *n)).collect();
    for (ship, n) in &now.ships {
        let before = was.get(ship.as_str()).copied().unwrap_or(0);
        if *n > before {
            parts.push(format!("+{} {ship}", n - before));
        } else if *n < before {
            parts.push(format!("-{} {ship}", before - n));
        }
    }
    for (ship, _) in &sent.ships {
        if !now.ships.iter().any(|(s, _)| s == ship) {
            parts.push(format!("{ship} gone"));
        }
    }
    if now.alliance != sent.alliance {
        if let Some(a) = &now.alliance {
            parts.push(format!("Alliance: {a}"));
        }
    }
    if now.dscan_link != sent.dscan_link {
        if let Some(l) = &now.dscan_link {
            parts.push(format!("{}: {l}", LABELS_EN[4]));
        }
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Where the Rorqual and the cyno stand with the saving fleet, from their own ESI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetVerdict {
    /// Both in one fleet somebody else is boss of.
    Joined,
    NotYet,
    /// In a fleet that is not the saving one: the pilot's own, or another.
    RorqualElsewhere { own_fleet: bool },
    CynoElsewhere { own_fleet: bool },
    /// Each in a fleet, not the same one.
    Split,
    Unknown(String),
}

pub fn fleet_verdict(rorq: &EsiFleet, cyno: Option<&EsiFleet>, own_ids: &[i64]) -> FleetVerdict {
    let own = |boss: i64| own_ids.contains(&boss);
    match rorq {
        EsiFleet::Unknown(w) | EsiFleet::Failed(w) => return FleetVerdict::Unknown(w.clone()),
        EsiFleet::NotInFleet => return FleetVerdict::NotYet,
        EsiFleet::InFleet { boss_id, .. } if own(*boss_id) => return FleetVerdict::RorqualElsewhere { own_fleet: true },
        EsiFleet::InFleet { .. } => {}
    }
    let EsiFleet::InFleet { fleet_id: rf, .. } = rorq else { unreachable!() };
    match cyno {
        None | Some(EsiFleet::Unknown(_)) | Some(EsiFleet::Failed(_)) => FleetVerdict::Joined,
        Some(EsiFleet::NotInFleet) => FleetVerdict::NotYet,
        Some(EsiFleet::InFleet { boss_id, .. }) if own(*boss_id) => FleetVerdict::CynoElsewhere { own_fleet: true },
        Some(EsiFleet::InFleet { fleet_id, .. }) if fleet_id == rf => FleetVerdict::Joined,
        Some(EsiFleet::InFleet { .. }) => FleetVerdict::Split,
    }
}

/// The last answers from ESI about the two pilots' fleets.
#[derive(Clone, Debug, Default)]
pub struct FleetStatus {
    pub rorqual: Option<EsiFleet>,
    pub cyno: Option<EsiFleet>,
    pub at: Option<i64>,
}

/// Which comms the FC called the pilot into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommsTarget {
    Beehive,
    Op(u8),
    /// A short link to somewhere else.
    Link { link: String },
}

impl CommsTarget {
    pub fn label(&self) -> String {
        match self {
            CommsTarget::Beehive => "Beehive".to_owned(),
            CommsTarget::Op(n) => format!("Op {n}"),
            CommsTarget::Link { link } => link.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommsInvite {
    pub target: CommsTarget,
    pub from: String,
    pub at: i64,
}

/// The comms a delve911 line points at, if any: the FC's own invite format first, then Beehive or
/// an op channel named anywhere in the text, then a bare short link. Command comms are never a
/// rescuee's place, so anything naming them is passed over.
pub fn detect_comms(body: &str) -> Option<CommsTarget> {
    let lower = body.to_lowercase();
    if lower.contains("command") {
        return None;
    }
    static INVITE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)join\s+op\s*(\d{1,2})\s+comms").unwrap());
    if let Some(c) = INVITE.captures(body) {
        if let Some(n) = c[1].parse::<u8>().ok().filter(|n| (1..=12).contains(n)) {
            return Some(CommsTarget::Op(n));
        }
    }
    static BEEHIVE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)\bbeehive\b").unwrap());
    if BEEHIVE.is_match(body) {
        return Some(CommsTarget::Beehive);
    }
    static OP: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)\b(?:op\s?(\d{1,2})|o7)\b").unwrap());
    if let Some(c) = OP.captures(body) {
        let n = c.get(1).map_or(Some(7), |m| m.as_str().parse::<u8>().ok());
        if let Some(n) = n.filter(|n| (1..=12).contains(n)) {
            return Some(CommsTarget::Op(n));
        }
    }
    static LINK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"https://gnf\.lt/\S+\.html").unwrap());
    if let Some(m) = LINK.find(body) {
        let link = m.as_str().to_owned();
        if let Some(n) = (1..=12).find(|n| crate::fleets::comms::builtin_op_link(*n) == Some(link.as_str())) {
            return Some(CommsTarget::Op(n));
        }
        return Some(CommsTarget::Link { link });
    }
    None
}

/// A d-scan read off the clipboard, as the pilot sees it before it goes to the FC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DscanPreview {
    pub counts: Vec<(String, u32)>,
    pub link: Option<String>,
    pub at: i64,
}

/// What a tackled Rorqual tends to see, as the FC wants it said: the gang's kind, not every
/// hull. A d-scan names the hulls when there is time for one.
pub const HOSTILE_GROUPS: &[&str] = &["Vedmaks", "ENIs", "Bombers", "Attack Battlecruisers", "Battleships", "HACs", "Dictors", "Dreads"];

/// The rescuee's state, UI thread only. Async answers land in the `shared_*` fields.
#[derive(Default)]
pub struct RescueeState {
    pub rorqual_pick: Option<String>,
    pub cyno_pick: Option<String>,
    /// Typed in place of what ESI sees, for an alt not logged in here.
    pub pilot_override: String,
    pub cyno_override: String,
    pub system_override: String,
    pub location: Location,
    pub location_input: String,
    pub panic: Panic,
    pub hostiles: Hostiles,
    pub ship_input: String,
    pub alliance_pilot: String,
    pub arm: Arm,
    /// The same for an update after the ping: every post to delve911 is clicked twice.
    pub arm_update: Arm,
    pub sent: Option<Sent>,
    pub checklist: Checklist,
    pub fleet: FleetStatus,
    pub comms: Option<CommsInvite>,
    /// Later lines that named other comms, shown weak under the one that counts.
    pub comms_also: Vec<CommsInvite>,
    pub dscan: Option<DscanPreview>,
    /// Lowercased localized site name to English, from the SDE; loaded once.
    pub anomaly_i18n: Option<std::sync::Arc<HashMap<String, String>>>,
    pub shared_dscan: std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>,
    /// The step the pilot opened by hand; none follows the first step not yet done.
    pub step_open: Option<u8>,
    pub alliance_error: Option<String>,
    pub shared_fleet: std::sync::Arc<std::sync::Mutex<Option<FleetStatus>>>,
    pub fleet_asked_at: Option<std::time::Instant>,
    pub shared_alliance: std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>,
    pub alliance_asking: bool,
    pub mumble_current: Option<String>,
    pub mumble_asked_at: Option<std::time::Instant>,
    /// The `mumble://` an op's short link turned out to point at, for the in-channel check.
    pub resolved_mumble: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl RescueeState {
    /// The next rescue: everything about the last one goes, the pilot's picks stay.
    pub fn reset(&mut self) {
        self.sent = None;
        self.checklist = Checklist::default();
        self.comms = None;
        self.comms_also.clear();
        self.hostiles = Hostiles::default();
        self.dscan = None;
        self.location = Location::Unknown;
        self.location_input.clear();
        self.panic = Panic::Unknown;
        self.fleet = FleetStatus::default();
        self.arm.clear();
        self.step_open = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(system: i64, docked: bool, ship_type: Option<i64>) -> ShipState {
        ShipState { system, docked, ship_type, station_id: docked.then_some(1), structure_id: None }
    }

    #[test]
    fn a_rorqual_in_space_is_found_with_the_undocked_ships_beside_it() {
        let ships: HashMap<String, ShipState> = [
            ("Miner".to_owned(), ship(30_004_759, false, Some(RORQUAL_TYPE_ID))),
            ("Cyno Alt".to_owned(), ship(30_004_759, false, Some(11176))),
            ("Docked Alt".to_owned(), ship(30_004_759, true, Some(11176))),
            ("Far Alt".to_owned(), ship(30_000_142, false, Some(11176))),
            ("Docked Rorq".to_owned(), ship(30_004_759, true, Some(RORQUAL_TYPE_ID))),
        ]
        .into_iter()
        .collect();
        let found = detect(&ships, &HashMap::new(), &HashMap::new());
        assert_eq!(found, [Detected { rorqual: "Miner".into(), system: 30_004_759, cynos: vec!["Cyno Alt".into()] }]);
    }

    #[test]
    fn a_rorqual_is_also_known_by_its_group() {
        let ships: HashMap<String, ShipState> = [("Miner".to_owned(), ship(1, false, Some(999)))].into_iter().collect();
        let by_id: HashMap<i64, String> = [(999, "Rorqual".to_owned())].into_iter().collect();
        let groups: HashMap<String, String> = [("rorqual".to_owned(), "Capital Industrial Ship".to_owned())].into_iter().collect();
        assert_eq!(detect(&ships, &by_id, &groups).len(), 1);
        assert!(detect(&ships, &by_id, &HashMap::new()).is_empty(), "an unknown hull is not a Rorqual");
    }

    #[test]
    fn locations_are_read_from_scanner_rows_moons_and_free_text() {
        let i18n: HashMap<String, String> = [("einsamer angel-knotenpunkt".to_owned(), "Angel Forsaken Hub".to_owned())].into_iter().collect();
        let row = parse_location("ABC-123\tCosmic Anomaly\tCombat Site\tEinsamer Angel-Knotenpunkt\t100,0%\t2,1 AU", &i18n);
        assert_eq!(row, Location::Anomaly { name_en: "Angel Forsaken Hub".into(), code: Some("ABC-123".into()) });
        assert_eq!(row.render(), "Angel Forsaken Hub ABC-123");
        assert_eq!(parse_location("Einsamer Angel-Knotenpunkt abc-123", &i18n).render(), "Angel Forsaken Hub ABC-123", "typed, code last, any case");
        assert_eq!(parse_location("Einsamer Angel-Knotenpunkt", &i18n), Location::Anomaly { name_en: "Angel Forsaken Hub".into(), code: None });
        assert_eq!(parse_location("1DQ1-A IV - Moon 2", &i18n).render(), "1DQ1-A IV - Moon 2");
        assert_eq!(parse_location("1DQ1-A IV - Mond 2", &i18n).render(), "1DQ1-A IV - Moon 2");
        assert_eq!(parse_location("1DQ1-A IV - Луна 12", &i18n).render(), "1DQ1-A IV - Moon 12");
        assert_eq!(parse_location("Planète IV - Lune 3", &i18n).render(), "Planet IV - Moon 3");
        assert_eq!(parse_location("iv - 卫星 1", &i18n).render(), "Planet IV - Moon 1");
        assert_eq!(parse_location("at the gate, 80km off", &i18n), Location::Free("at the gate, 80km off".into()));
        assert_eq!(parse_location("  ", &i18n), Location::Unknown);
    }

    #[test]
    fn the_ping_is_english_and_the_fc_side_reads_it_back() {
        let f = PingFields {
            pilot: "Placeholder Miner".into(),
            cyno: "Placeholder Cyno".into(),
            system: "1DQ1-A".into(),
            location: Location::Anomaly { name_en: "Angel Forsaken Hub".into(), code: Some("ABC-123".into()) },
            panic: Panic::Available,
            hostiles: Hostiles { count: Some(7), ships: vec![("Sabre".into(), 2), ("Loki".into(), 3)], alliance: Some("Fraternity.".into()), dscan_link: None },
        };
        let text = render_ping(&f);
        // The form the room's pilots post, line for line, then what else is known.
        assert_eq!(
            text,
            "!bping all Rorqual Tackled\nRorqual Name: Placeholder Miner\nSystem: 1DQ1-A\nLocation In System (Anomaly or Moon): Angel Forsaken Hub ABC-123\nName of Cyno (On perch, 300km away): Placeholder Cyno\nHostiles: 7 (Sabre x2, Loki x3) - Fraternity.\nPanic: yes"
        );
        let labels: [String; 7] = ["Rorqual-Name".into(), "System".into(), "Ort im System".into(), "Cyno".into(), "Dscan".into(), "Feinde".into(), "Panik".into()];
        assert!(render_ping_with(&f, "Rorqual getackelt", &labels).starts_with("!bping all Rorqual getackelt\nRorqual-Name: Placeholder Miner"));
        assert!(render_ping_with(&f, "Rorqual getackelt", &labels).contains("Ort im System: Angel Forsaken Hub ABC-123"));
        let systems = crate::uitest::fixtures::systems();
        let ev = crate::rescue::parse_event("Placeholder Miner", &text, 0, &systems, &HashMap::new());
        assert!(ev.is_ping, "{ev:?}");
        assert_eq!(ev.pilot.as_deref(), Some("Placeholder Miner"));
        assert_eq!(ev.cyno.as_deref(), Some("Placeholder Cyno"));
        assert_eq!(ev.system_name.as_deref(), Some("1DQ1-A"));
        assert_eq!(ev.anomaly.as_deref(), Some("Angel Forsaken Hub ABC-123"));
        let bare = PingFields { pilot: "X".into(), system: "1DQ1-A".into(), ..Default::default() };
        assert_eq!(render_ping(&bare), "!bping all Rorqual Tackled\nRorqual Name: X\nSystem: 1DQ1-A\nLocation In System (Anomaly or Moon): ?\nName of Cyno (On perch, 300km away): ?", "nothing unknown is written");
        assert_eq!(missing(&bare), ["location"]);
        let with_scan = PingFields { hostiles: Hostiles { dscan_link: Some("https://dscan.info/v/x".into()), ..Default::default() }, ..bare.clone() };
        assert!(render_ping(&with_scan).ends_with("Hostile Fleet Dscan: https://dscan.info/v/x"));
    }

    #[test]
    fn the_follow_up_says_only_what_changed() {
        let a = Hostiles { count: Some(5), ships: vec![("Sabre".into(), 2), ("Loki".into(), 1)], alliance: None, dscan_link: None };
        assert_eq!(followup(&a, &a, &Location::Unknown, &Location::Unknown), None);
        let b = Hostiles { count: Some(9), ships: vec![("Sabre".into(), 3), ("Proteus".into(), 1)], alliance: Some("Fraternity.".into()), dscan_link: Some("https://dscan.info/v/x".into()) };
        assert_eq!(
            followup(&a, &b, &Location::Unknown, &Location::Free("gate".into())).as_deref(),
            Some("Location In System (Anomaly or Moon): gate, Hostiles now 9, +1 Sabre, +1 Proteus, Loki gone, Alliance: Fraternity., Hostile Fleet Dscan: https://dscan.info/v/x")
        );
        assert_eq!(followup(&b, &b, &Location::Free("gate".into()), &Location::Free("gate".into())), None, "the link is told once");
    }

    #[test]
    fn comms_are_read_from_whatever_the_fc_writes() {
        assert_eq!(detect_comms("Placeholder Miner: Join OP 4 Comms https://gnf.lt/2eMgwE2.html"), Some(CommsTarget::Op(4)));
        assert_eq!(detect_comms("get in beehive"), Some(CommsTarget::Beehive));
        assert_eq!(detect_comms("BEEHIVE!!"), Some(CommsTarget::Beehive));
        assert_eq!(detect_comms("op3"), Some(CommsTarget::Op(3)));
        assert_eq!(detect_comms("jump to Op 3 now"), Some(CommsTarget::Op(3)));
        assert_eq!(detect_comms("o7"), Some(CommsTarget::Op(7)));
        assert_eq!(detect_comms("https://gnf.lt/NOH1FNH.html"), Some(CommsTarget::Op(3)), "a bare op link");
        assert_eq!(detect_comms("https://gnf.lt/zzzzzzz.html"), Some(CommsTarget::Link { link: "https://gnf.lt/zzzzzzz.html".into() }));
        assert_eq!(detect_comms("command 3 for FCs"), None, "never command comms");
        assert_eq!(detect_comms("op 13"), None);
        assert_eq!(detect_comms("stop"), None, "not every 'op'");
        assert_eq!(detect_comms("we are coming"), None);
    }

    #[test]
    fn the_fleet_verdict_wants_both_in_one_fleet_run_by_someone_else() {
        let in_fleet = |fleet_id: i64, boss_id: i64| EsiFleet::InFleet { fleet_id, boss_id, exists: Some(true) };
        let own = [1, 2];
        assert_eq!(fleet_verdict(&in_fleet(9, 7), Some(&in_fleet(9, 7)), &own), FleetVerdict::Joined);
        assert_eq!(fleet_verdict(&in_fleet(9, 7), None, &own), FleetVerdict::Joined, "no cyno to wait for");
        assert_eq!(fleet_verdict(&EsiFleet::NotInFleet, Some(&in_fleet(9, 7)), &own), FleetVerdict::NotYet);
        assert_eq!(fleet_verdict(&in_fleet(9, 7), Some(&EsiFleet::NotInFleet), &own), FleetVerdict::NotYet);
        assert_eq!(fleet_verdict(&in_fleet(3, 1), None, &own), FleetVerdict::RorqualElsewhere { own_fleet: true }, "their own fleet");
        assert_eq!(fleet_verdict(&in_fleet(9, 7), Some(&in_fleet(4, 2)), &own), FleetVerdict::CynoElsewhere { own_fleet: true });
        assert_eq!(fleet_verdict(&in_fleet(9, 7), Some(&in_fleet(4, 8)), &own), FleetVerdict::Split);
        assert!(matches!(fleet_verdict(&EsiFleet::Unknown("no token".into()), None, &own), FleetVerdict::Unknown(_)));
    }

    #[test]
    fn a_ping_takes_two_presses_within_three_seconds() {
        let mut arm = Arm::default();
        assert!(!arm.press(0));
        assert_eq!(arm.left_ms(1000), Some(2000));
        assert!(arm.press(2999));
        assert!(!arm.press(10_000));
        assert!(!arm.press(13_500), "too late: arms again");
        assert_eq!(arm.left_ms(17_000), None);
        assert!(arm.press(14_000));
    }
}
