//! Who is running which boosts, read out of the boost channel's own chat log.
//!
//! Boosters post what they loaded, so this is a parser over real traffic rather than a format.
//! Most lines are the charges pasted straight out of the fitting window, "Shield Extension Charge
//! Shield Harmonizing Charge +ML", but the same channel also carries shorthand ("rapid
//! interdiction ml", "shield +ml"), corrections ("-info. got tackled", "swapping to info") and a
//! great deal of ordinary conversation that mentions the same words without claiming anything
//! ("can do info or shield which one?", "do we have skirm?").
//!
//! Two rules keep the count honest. A line naming a real charge is taken at its word. A line with
//! only shorthand has to consist of nothing but boost words and filler, so a question or an offer
//! is never counted as a pilot who is actually running it. An FC posting a rule of dashes to make
//! everyone re-post wipes the board, which is what it means.

use std::collections::BTreeMap;

/// Which burst a charge belongs to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Burst {
    Shield,
    Armor,
    Skirmish,
    Information,
    Mining,
}

impl Burst {
    pub fn label(self) -> &'static str {
        match self {
            Burst::Shield => "Shield",
            Burst::Armor => "Armor",
            Burst::Skirmish => "Skirmish",
            Burst::Information => "Information",
            Burst::Mining => "Mining",
        }
    }

    pub fn parse(label: &str) -> Option<Burst> {
        [Burst::Shield, Burst::Armor, Burst::Skirmish, Burst::Information, Burst::Mining]
            .into_iter()
            .find(|b| b.label().eq_ignore_ascii_case(label.trim()))
    }
}

/// The bursts a combat doctrine asks for. Shield or armor depending on what it flies, both once a
/// fleet is big enough to stack them, and information and skirmish either way. Mining is left out:
/// the parser still reads it, no combat doctrine requires it.
pub const COMBAT_BURSTS: &[Burst] =
    &[Burst::Shield, Burst::Armor, Burst::Information, Burst::Skirmish];

/// Every command burst charge, by the name the fitting window gives it.
///
/// Longest first within a burst, because a line is scanned for whole names and "Rapid Repair" must
/// be found before the one-word alias "rapid" is consulted.
pub const CHARGES: &[(&str, Burst)] = &[
    ("Shield Harmonizing", Burst::Shield),
    ("Shield Extension", Burst::Shield),
    ("Active Shielding", Burst::Shield),
    ("Armor Energizing", Burst::Armor),
    ("Armor Reinforcement", Burst::Armor),
    ("Rapid Repair", Burst::Armor),
    ("Evasive Maneuvers", Burst::Skirmish),
    ("Interdiction Maneuvers", Burst::Skirmish),
    ("Rapid Deployment", Burst::Skirmish),
    ("Sensor Optimization", Burst::Information),
    ("Electronic Hardening", Burst::Information),
    ("Electronic Superiority", Burst::Information),
    ("Mining Laser Field Enhancement", Burst::Mining),
    ("Mining Laser Optimization", Burst::Mining),
    ("Mining Equipment Preservation", Burst::Mining),
];

/// The one word people type instead of the whole charge name.
///
/// "rapid" is Rapid Deployment: the skirmish charge is what the shorthand means in a subcap fleet,
/// and anyone meaning the armor one writes "rapid repair", which the full-name scan catches first.
const CHARGE_WORDS: &[(&str, &str)] = &[
    ("harmonizing", "Shield Harmonizing"),
    ("harm", "Shield Harmonizing"),
    ("extension", "Shield Extension"),
    ("ext", "Shield Extension"),
    ("active", "Active Shielding"),
    ("energizing", "Armor Energizing"),
    ("reinforcement", "Armor Reinforcement"),
    ("evasive", "Evasive Maneuvers"),
    ("interdiction", "Interdiction Maneuvers"),
    ("rapid", "Rapid Deployment"),
    ("sensor", "Sensor Optimization"),
    ("hardening", "Electronic Hardening"),
    ("superiority", "Electronic Superiority"),
];

/// Shorthand for a whole burst, misspellings included: "skrim" is as common as "skirm".
const BURST_WORDS: &[(&str, Burst)] = &[
    ("shield", Burst::Shield),
    ("shields", Burst::Shield),
    ("sheild", Burst::Shield),
    ("sheilds", Burst::Shield),
    ("shild", Burst::Shield),
    ("shiel", Burst::Shield),
    ("armor", Burst::Armor),
    ("armour", Burst::Armor),
    ("skirm", Burst::Skirmish),
    ("skirms", Burst::Skirmish),
    ("skirmish", Burst::Skirmish),
    ("skrim", Burst::Skirmish),
    ("scrim", Burst::Skirmish),
    ("info", Burst::Information),
    ("infos", Burst::Information),
    ("information", Burst::Information),
    ("mining", Burst::Mining),
];

/// Words a declaration may carry besides the boost itself.
///
/// The list is the whole guard on shorthand: anything outside it makes the line a conversation
/// rather than a claim, so "need skirm", "can do info" and "who is armor" are all thrown out by the
/// words they contain rather than by a list of phrasings nobody will finish writing.
const FILLER: &[&str] = &[
    "i", "im", "ive", "ill", "id", "my", "me", "a", "the", "on", "in", "to", "and", "plus", "with",
    "for", "of", "am", "is", "are", "will", "have", "has", "had", "got", "get", "getting",
    "gettign", "go", "going", "gonna", "do", "doing", "run", "running", "bring", "bringing",
    "take", "taking", "swap", "swaps", "swapping", "swaping", "swapped", "switch", "switching",
    "switched", "reship", "reshipping", "boost", "boosts", "boosting", "booster", "link", "links",
    "lnks", "charge", "charges", "up", "here", "now", "atm", "too", "also", "just", "then", "x",
    "ok", "okay", "k", "kk", "yes", "yeah", "aye", "rgr", "roger", "copy", "sure", "set", "sets",
    "was", "were", "actually", "all", "double", "backup", "extra", "as",
];

/// Command ship hulls, which boosters name beside the charge often enough to be worth allowing.
const HULLS: &[&str] = &[
    "bifrost", "stork", "magus", "pontifex", "claymore", "sleipnir", "vulture", "nighthawk",
    "absolution", "damnation", "astarte", "eos", "loki", "legion", "tengu", "proteus",
];

/// Words that turn a following "ml" into the opposite claim.
const NO_MINDLINK: &[&str] = &["no", "not", "without", "w/o", "dont", "cant", "sans", "minus"];

pub fn burst_of(charge: &str) -> Option<Burst> {
    CHARGES.iter().find(|(name, _)| name.eq_ignore_ascii_case(charge)).map(|(_, b)| *b)
}

/// What one line claims.
#[derive(Clone, PartialEq, Debug)]
pub enum Say {
    /// What this pilot is on now, replacing whatever they said before.
    Running { charges: Vec<&'static str>, bursts: Vec<Burst>, mindlink: Option<bool> },
    /// A bare "+ml" or "no mindlink", correcting the line before it.
    Mindlink(bool),
    /// What this pilot has stopped running. Empty means they are out entirely.
    Drop { charges: Vec<&'static str>, bursts: Vec<Burst> },
    /// A rule of dashes: everyone posts again, so nothing said before it counts.
    Reset,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    pub pilot: String,
    pub at: i64,
    pub say: Say,
    /// What they actually typed. Kept so an FC can see why a line was read the way it was, which
    /// is the only way to tell a wrong detection from a pilot who posted the wrong thing.
    pub text: String,
}

/// Reads one line. `None` when it claims nothing, which most chatter does.
pub fn parse(pilot: &str, text: &str, at: i64) -> Option<Line> {
    let say = read(text)?;
    Some(Line { pilot: pilot.to_owned(), at, say, text: text.trim().to_owned() })
}

fn read(text: &str) -> Option<Say> {
    let raw = text.trim();
    if is_reset(raw) {
        return Some(Say::Reset);
    }
    // The in-game paste marks a charge it could not link with a trailing star.
    let low = raw.to_lowercase().replace('*', " ");

    let named = named_charges(&low);
    if !named.is_empty() {
        return Some(Say::Running { charges: named, bursts: Vec::new(), mindlink: mindlink(&low) });
    }

    if let Some(rest) = drop_body(&low) {
        let (charges, bursts) = loose_boosts(&rest);
        return Some(Say::Drop { charges, bursts });
    }

    if low.contains('?') {
        return None;
    }
    let (charges, bursts) = strict_boosts(&low)?;
    if charges.is_empty() && bursts.is_empty() {
        return mindlink(&low).map(Say::Mindlink);
    }
    Some(Say::Running { charges, bursts, mindlink: mindlink(&low) })
}

/// Three or more of the same rule character. An FC draws one to clear the board.
fn is_reset(raw: &str) -> bool {
    let b = raw.as_bytes();
    b.windows(3).any(|w| {
        w[0] == w[1] && w[1] == w[2] && matches!(w[0], b'-' | b'=' | b'_' | b'~' | b'#')
    })
}

/// Charges named in full, in the order the fitting window pasted them.
fn named_charges(low: &str) -> Vec<&'static str> {
    let mut found: Vec<(usize, &'static str)> = Vec::new();
    for (name, _) in CHARGES {
        if let Some(at) = low.find(&name.to_lowercase()) {
            found.push((at, name));
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, n)| n).collect()
}

/// The part of a line that takes something away, or `None` when the line adds.
///
/// "-1" on its own is a pilot leaving, so it counts as a drop with nothing named. Everything after
/// the first sentence break is the reason, not the boost: "-info. got tackled".
fn drop_body(low: &str) -> Option<String> {
    let rest = low.strip_prefix('-')?;
    let rest = rest.split(['.', ',', ';']).next().unwrap_or(rest);
    let rest = rest.trim().trim_start_matches('1').trim();
    Some(rest.to_owned())
}

/// Boost words anywhere in the text, ignoring whatever else is there. Only for drops, where the
/// leading minus has already established what the line is doing.
fn loose_boosts(low: &str) -> (Vec<&'static str>, Vec<Burst>) {
    let mut charges = Vec::new();
    let mut bursts = Vec::new();
    for w in words(low) {
        if let Some((_, c)) = CHARGE_WORDS.iter().find(|(a, _)| *a == w) {
            if !charges.contains(c) {
                charges.push(*c);
            }
        } else if let Some((_, b)) = BURST_WORDS.iter().find(|(a, _)| *a == w) {
            if !bursts.contains(b) {
                bursts.push(*b);
            }
        }
    }
    (charges, bursts)
}

/// Boost words in a line that is nothing but boost words and filler. `None` the moment a word
/// turns up that a pilot claiming a boost would not have written.
fn strict_boosts(low: &str) -> Option<(Vec<&'static str>, Vec<Burst>)> {
    let mut charges: Vec<&'static str> = Vec::new();
    let mut bursts: Vec<Burst> = Vec::new();
    for w in words(low) {
        if let Some((_, c)) = CHARGE_WORDS.iter().find(|(a, _)| *a == w) {
            if !charges.contains(c) {
                charges.push(*c);
            }
        } else if let Some((_, b)) = BURST_WORDS.iter().find(|(a, _)| *a == w) {
            if !bursts.contains(b) {
                bursts.push(*b);
            }
        } else if w == "ml" || w == "mindlink" || w == "mindlinks" {
        } else if !FILLER.contains(&w) && !HULLS.contains(&w) && !NO_MINDLINK.contains(&w) {
            return None;
        }
    }
    // A burst named by shorthand beside one of its own charges is the same claim twice.
    let covered: Vec<Burst> = charges.iter().filter_map(|c| burst_of(c)).collect();
    bursts.retain(|b| !covered.contains(b));
    Some((charges, bursts))
}

fn words(low: &str) -> impl Iterator<Item = &str> {
    low.split(|c: char| !c.is_alphanumeric() && c != '/').flat_map(|s| s.split('/')).filter(|s| !s.is_empty())
}

/// Whether the line claims a mindlink, denies one, or says nothing about it.
fn mindlink(low: &str) -> Option<bool> {
    let ws: Vec<&str> = words(low).collect();
    let at = ws.iter().position(|w| *w == "ml" || *w == "mindlink" || *w == "mindlinks")?;
    let negated = ws[at.saturating_sub(2)..at].iter().any(|w| NO_MINDLINK.contains(w));
    Some(!negated)
}

/// How many pilots are covering one thing, and how many of those have a mindlink.
#[derive(Clone, PartialEq, Debug)]
pub struct Coverage {
    /// The charge's name, or the burst's when only shorthand was given.
    pub what: String,
    pub burst: Burst,
    /// True when this came from shorthand, so it is a burst rather than a known charge.
    pub generic: bool,
    pub pilots: usize,
    pub mindlinked: usize,
    /// Who is holding it, as they are spelled in the channel.
    pub who: Vec<String>,
}

#[derive(Clone, Default)]
struct Held {
    charges: Vec<&'static str>,
    bursts: Vec<Burst>,
    mindlink: bool,
}

impl Held {
    fn empty(&self) -> bool {
        self.charges.is_empty() && self.bursts.is_empty()
    }
}

/// What the channel adds up to: the last thing each pilot said, counted.
///
/// `since` drops anything older than the fleet, because a boost declared three fleets ago says
/// nothing about this one. Lines are replayed in time order, so a reset in the middle clears only
/// what came before it.
pub fn coverage(lines: &[Line], since: i64) -> Vec<Coverage> {
    let mut ordered: Vec<&Line> = lines.iter().filter(|l| l.at >= since).collect();
    ordered.sort_by_key(|l| l.at);

    let mut held: BTreeMap<String, Held> = BTreeMap::new();
    let mut shown: BTreeMap<String, String> = BTreeMap::new();
    for l in ordered {
        let who = l.pilot.to_lowercase();
        shown.entry(who.clone()).or_insert_with(|| l.pilot.clone());
        match &l.say {
            Say::Reset => held.clear(),
            Say::Running { charges, bursts, mindlink } => {
                held.insert(
                    who,
                    Held {
                        charges: charges.clone(),
                        bursts: bursts.clone(),
                        mindlink: mindlink.unwrap_or(false),
                    },
                );
            }
            Say::Mindlink(v) => {
                if let Some(h) = held.get_mut(&who) {
                    h.mindlink = *v;
                }
            }
            Say::Drop { charges, bursts } => {
                if charges.is_empty() && bursts.is_empty() {
                    held.remove(&who);
                } else if let Some(h) = held.get_mut(&who) {
                    // Dropping "info" takes the information charges with it: a pilot naming the
                    // burst is not distinguishing between the two they had loaded.
                    h.charges.retain(|c| {
                        !charges.contains(c) && !burst_of(c).is_some_and(|b| bursts.contains(&b))
                    });
                    h.bursts.retain(|b| {
                        !bursts.contains(b) && !charges.iter().any(|c| burst_of(c) == Some(*b))
                    });
                    if h.empty() {
                        held.remove(&who);
                    }
                }
            }
        }
    }

    let mut by: BTreeMap<(String, bool), Coverage> = BTreeMap::new();
    for (key, h) in &held {
        let pilot = shown.get(key).cloned().unwrap_or_else(|| key.clone());
        for c in &h.charges {
            let burst = burst_of(c).unwrap_or(Burst::Shield);
            let e = by.entry(((*c).to_owned(), false)).or_insert(Coverage {
                what: (*c).to_owned(),
                burst,
                generic: false,
                pilots: 0,
                mindlinked: 0,
                who: Vec::new(),
            });
            e.pilots += 1;
            e.mindlinked += usize::from(h.mindlink);
            e.who.push(pilot.clone());
        }
        for b in &h.bursts {
            let e = by.entry((b.label().to_owned(), true)).or_insert(Coverage {
                what: b.label().to_owned(),
                burst: *b,
                generic: true,
                pilots: 0,
                mindlinked: 0,
                who: Vec::new(),
            });
            e.pilots += 1;
            e.mindlinked += usize::from(h.mindlink);
            e.who.push(pilot.clone());
        }
    }
    let mut out: Vec<Coverage> = by.into_values().collect();
    out.sort_by(|a, b| a.burst.cmp(&b.burst).then_with(|| a.what.cmp(&b.what)));
    out
}

/// Reads a boost channel's log into declarations.
pub fn read_channel(path: &std::path::Path) -> Vec<Line> {
    let Some((_, messages)) = crate::chatlog::read(path) else { return Vec::new() };
    messages
        .iter()
        .filter_map(|m| {
            let at = crate::intel::parse_eve_time(&m.timestamp)?;
            parse(&m.author, &m.text, at)
        })
        .collect()
}

/// What one fleet's boost channel adds up to over the fleet's own lifetime.
///
/// EVE opens a new log file per session, so one channel is several files and a fleet that outlives
/// a client restart spans more than one. Names are matched case-insensitively because the API
/// spells a channel "Awesomeboosts" and the client writes `awesomeboosts_20260919_110504_*.txt`.
pub fn read_window(
    dir: &std::path::Path,
    channel: &str,
    from: i64,
    to: Option<i64>,
) -> (Vec<Coverage>, Vec<Line>) {
    let prefix = format!("{}_", channel.trim().to_lowercase().replace(' ', ""));
    let Ok(entries) = std::fs::read_dir(dir) else { return (Vec::new(), Vec::new()) };
    let mut lines = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if !name.starts_with(&prefix) || !name.ends_with(".txt") {
            continue;
        }
        if !covers(&name, from, to) {
            continue;
        }
        lines.extend(read_channel(&e.path()));
    }
    if let Some(end) = to {
        lines.retain(|l| l.at <= end);
    }
    lines.retain(|l: &Line| l.at >= from);
    lines.sort_by_key(|l| l.at);
    (coverage(&lines, from), lines)
}

/// The longest a single log file is assumed to run, for deciding whether one that starts before
/// the window can still hold lines inside it. EVE opens a file per session and rolls over at
/// downtime, so a day is past generous.
const MAX_SESSION: i64 = 24 * 3600;

/// Whether a log file could hold a line in `[from, to]`, judged by the timestamp in its name.
///
/// A channel accumulates a file per session, hundreds over months, and reading them all to keep
/// the forty minutes a fleet lasted meant decoding the entire history from UTF-16 on every scan.
/// The name carries the session's start (`channel_YYYYMMDD_HHMMSS_charid.txt`), which is enough to
/// throw out everything that starts after the window or a day before it. A name that does not
/// parse is kept, because guessing wrong here loses real messages.
fn covers(name: &str, from: i64, to: Option<i64>) -> bool {
    let Some(start) = file_start(name) else { return true };
    if start > to.unwrap_or(i64::MAX) {
        return false;
    }
    start >= from - MAX_SESSION
}

/// The `_YYYYMMDD_HHMMSS_` stamp EVE puts in a chat log's name, as a unix time. The client writes
/// it in EVE time, which is UTC.
fn file_start(name: &str) -> Option<i64> {
    let mut parts = name.trim_end_matches(".txt").rsplitn(3, '_');
    let _char_id = parts.next()?;
    let hms = parts.next()?;
    let ymd = parts.next()?.rsplit('_').next()?;
    if ymd.len() != 8 || hms.len() != 6 || !ymd.bytes().chain(hms.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    // Reassembled into the shape the log's own timestamps use, so one parser covers both. EVE
    // writes the name in EVE time, which is UTC.
    crate::intel::parse_eve_time(&format!(
        "{}.{}.{} {}:{}:{}",
        &ymd[0..4],
        &ymd[4..6],
        &ymd[6..8],
        &hms[0..2],
        &hms[2..4],
        &hms[4..6]
    ))
}

/// How badly a doctrine wants a boost.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Priority {
    High,
    #[default]
    Medium,
    Low,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::High, Priority::Medium, Priority::Low];

    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    /// An unknown string is Medium rather than an error: a setting written by a later version must
    /// not take the whole requirement list down with it.
    pub fn parse(s: &str) -> Priority {
        match s.trim().to_lowercase().as_str() {
            "high" => Priority::High,
            "low" => Priority::Low,
            _ => Priority::Medium,
        }
    }
}

/// A boost this doctrine wants, and how badly.
#[derive(Clone, PartialEq, Debug)]
pub struct Wanted {
    pub what: String,
    pub priority: Priority,
}

/// Boosts the FC has judged for themselves, overriding what the channel says.
///
/// The channel is people typing, so it is wrong sometimes: a booster who never posted, or one who
/// posted and then left. Keyed by the charge or burst name the requirement is written with.
pub type Forced = std::collections::BTreeMap<String, bool>;

/// What the FC said about this boost, if anything.
pub fn forced(want: &str, forced: &Forced) -> Option<bool> {
    forced.iter().find(|(k, _)| k.eq_ignore_ascii_case(want.trim())).map(|(_, v)| *v)
}

/// Whether anybody is on a wanted boost, with the FC's own judgement taken first.
pub fn covered_with(want: &str, have: &[Coverage], marks: &Forced) -> bool {
    forced(want, marks).unwrap_or_else(|| covered(want, have))
}

/// What the doctrine wants that nobody is on, with the FC's own judgement taken first.
pub fn gaps_with<'a>(wanted: &'a [Wanted], have: &[Coverage], marks: &Forced) -> Vec<&'a Wanted> {
    let mut out: Vec<&Wanted> =
        wanted.iter().filter(|w| !covered_with(&w.what, have, marks)).collect();
    out.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.what.cmp(&b.what)));
    out
}

/// Whether anybody is on a wanted boost.
///
/// A requirement is either one charge or a whole burst, because a doctrine that wants "some shield"
/// does not care which of the three is up. Shorthand in the channel works the same way in reverse:
/// a pilot who typed "shield" is running one of the shield charges, and which one is not worth
/// calling a gap over.
pub fn covered(want: &str, have: &[Coverage]) -> bool {
    if let Some(b) = Burst::parse(want) {
        return have.iter().any(|c| c.pilots > 0 && c.burst == b);
    }
    have.iter().any(|c| {
        c.pilots > 0
            && (c.what.eq_ignore_ascii_case(want)
                || (c.generic && burst_of(want) == Some(c.burst)))
    })
}

/// What the doctrine wants that nobody is on, most important first.
pub fn gaps<'a>(wanted: &'a [Wanted], have: &[Coverage]) -> Vec<&'a Wanted> {
    let mut out: Vec<&Wanted> = wanted.iter().filter(|w| !covered(&w.what, have)).collect();
    out.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.what.cmp(&b.what)));
    out
}

/// What to put on next.
pub fn suggestion<'a>(wanted: &'a [Wanted], have: &[Coverage]) -> Option<&'a Wanted> {
    gaps(wanted, have).first().copied()
}

/// A sensible starting point for a doctrine: every charge of the bursts a combat fleet runs, at
/// Medium, so an FC only has to raise the ones that matter and delete the rest.
///
/// Shield and armor are alternatives, never both: a doctrine tanks one way.
pub fn default_rules(setup_id: i64, armor: bool) -> Vec<crate::settings::FleetBoostRequirement> {
    let tank = if armor { Burst::Armor } else { Burst::Shield };
    CHARGES
        .iter()
        .filter(|(_, b)| *b == tank || *b == Burst::Information || *b == Burst::Skirmish)
        .map(|(name, _)| crate::settings::FleetBoostRequirement {
            setup_id: setup_id as i32,
            charge: (*name).to_owned(),
            priority: Priority::Medium.as_str().to_owned(),
        })
        .collect()
}

/// Whether a doctrine's name reads as an armor fleet. A guess the dialog shows and the user flips,
/// not a fact: the API says nothing about how a setup tanks.
pub fn looks_like_armor(setup_name: &str) -> bool {
    const ARMOR: &[&str] = &[
        "abaddon", "apoc", "armor", "armour", "baltec", "guardian", "harbinger", "legion",
        "machariel", "mega", "prophecy", "proteus", "sacrilege", "zealot", "retri", "damnation",
        "absolution", "eos", "astarte", "myrmidon", "brutix",
    ];
    let n = setup_name.to_lowercase();
    ARMOR.iter().any(|a| n.contains(a))
}

/// The requirements the user set for one doctrine, worst first.
pub fn wanted_for(setup_id: i64, rows: &[crate::settings::FleetBoostRequirement]) -> Vec<Wanted> {
    let mut out: Vec<Wanted> = rows
        .iter()
        .filter(|r| i64::from(r.setup_id) == setup_id && !r.charge.trim().is_empty())
        .map(|r| Wanted { what: r.charge.trim().to_owned(), priority: Priority::parse(&r.priority) })
        .collect();
    out.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.what.cmp(&b.what)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(text: &str) -> Option<Say> {
        read(text)
    }

    fn running(text: &str) -> (Vec<&'static str>, Vec<Burst>, Option<bool>) {
        match read(text) {
            Some(Say::Running { charges, bursts, mindlink }) => (charges, bursts, mindlink),
            other => panic!("{text:?} read as {other:?}"),
        }
    }

    fn line(pilot: &str, text: &str, at: i64) -> Line {
        parse(pilot, text, at).unwrap_or_else(|| panic!("{text:?} claimed nothing"))
    }

    /// The shape almost every declaration arrives in, pasted out of the fitting window.
    #[test]
    fn a_pasted_pair_of_charges_is_read() {
        let (charges, bursts, ml) = running("Shield Extension Charge  Shield Harmonizing Charge");
        assert_eq!(charges, vec!["Shield Extension", "Shield Harmonizing"]);
        assert!(bursts.is_empty());
        assert_eq!(ml, None);

        // Three at once, and the star the client leaves on an unlinked name.
        let (charges, _, ml) = running("Rapid Deployment Charge*  Evasive Maneuvers Charge* +ml");
        assert_eq!(charges, vec!["Rapid Deployment", "Evasive Maneuvers"]);
        assert_eq!(ml, Some(true));

        // A hull name in the middle does not stop the charges being found.
        let (charges, _, ml) = running("Shield Extension Charge  Active Shielding Charge Nighthawk + ML");
        assert_eq!(charges, vec!["Shield Extension", "Active Shielding"]);
        assert_eq!(ml, Some(true));
    }

    /// Mindlinks are written every way there is, and denied in a few more.
    #[test]
    fn a_mindlink_is_spotted_however_it_is_written() {
        for text in [
            "Shield Extension Charge +ML",
            "Sensor Optimization Charge ML",
            "Evasive Maneuvers Charge w/ ML",
            "Rapid Deployment Charge  Interdiction Maneuvers Charge + ML",
            "shield +ml",
            "skirm ml",
        ] {
            assert_eq!(mindlink(&text.to_lowercase()), Some(true), "{text}");
        }
        assert_eq!(mindlink("shield extension charge"), None);
        // Saying so explicitly must not read as the opposite.
        for text in ["no ml", "no mindlink", "without ml", "active shielding, no ml tho"] {
            assert_eq!(mindlink(text), Some(false), "{text}");
        }
        // A word that merely contains the letters is not a mindlink.
        assert_eq!(mindlink("shield, html tags"), None);
    }

    /// One word for a whole charge, which is how half the channel answers.
    #[test]
    fn one_word_stands_for_the_charge() {
        assert_eq!(running("rapid  interdiction ml").0, vec!["Rapid Deployment", "Interdiction Maneuvers"]);
        assert_eq!(running("sensor and superiority").0, vec!["Sensor Optimization", "Electronic Superiority"]);
        assert_eq!(running("active and extension for me").0, vec!["Active Shielding", "Shield Extension"]);
        assert_eq!(running("ill do active").0, vec!["Active Shielding"]);
        // "rapid repair" is the armor charge and the full name is found before the alias.
        assert_eq!(running("Rapid Repair Charge  Armor Energizing Charge").0,
                   vec!["Rapid Repair", "Armor Energizing"]);
    }

    /// Shorthand for the burst counts, since it is what people type when asked.
    #[test]
    fn shorthand_covers_a_burst() {
        let (charges, bursts, ml) = running("shield +ml");
        assert!(charges.is_empty());
        assert_eq!(bursts, vec![Burst::Shield]);
        assert_eq!(ml, Some(true));

        for (text, want) in [
            ("x skirm ml", Burst::Skirmish),
            ("got skirm", Burst::Skirmish),
            ("skrim", Burst::Skirmish),
            ("i have info", Burst::Information),
            ("bringing info", Burst::Information),
            ("im shield boost", Burst::Shield),
            ("i will do armour", Burst::Armor),
            ("ok got skirm", Burst::Skirmish),
            ("i was skirms", Burst::Skirmish),
            ("Bifrost + skirmish ML", Burst::Skirmish),
            ("backup armor", Burst::Armor),
        ] {
            assert_eq!(running(text).1, vec![want], "{text}");
        }
    }

    /// A burst word beside one of its own charges is the same claim twice.
    #[test]
    fn shorthand_beside_its_own_charge_is_not_counted_again() {
        let (charges, bursts, _) = running("shield extension +ml");
        assert_eq!(charges, vec!["Shield Extension"]);
        assert!(bursts.is_empty(), "{bursts:?}");
    }

    /// The channel is mostly people asking, offering and answering. None of that is a boost.
    #[test]
    fn questions_and_offers_are_not_declarations() {
        for text in [
            "do we have shield?",
            "need skirm",
            "i can do info",
            "can do info or shield which one?",
            "who is armor?",
            "anyone info?",
            "shield or info",
            "what boosts needed",
            "missing skirm",
            "we need shield or info?",
            "please link shield boosts?",
            "skirm boosts pls",
            "which skirm needed?",
            "Should I change my Skirm Claymore to a Raven?",
            "wwww",
            "o7",
            "I can do skrim / shield with ml",
            "In that case  Shaimy Kruta take shield and  Leeroys take skirm",
            "Skirmaugswarm1 for logi",
            "bifrost  Shield Command Burst II  Shield Command Burst II ml",
            "Shield Command Mindlink",
            "",
        ] {
            assert!(say(text).is_none(), "{text:?} was read as {:?}", say(text));
        }
    }

    /// Dropping leaves a pilot covering nothing rather than still counted.
    #[test]
    fn a_drop_takes_back_what_was_claimed() {
        assert_eq!(say("-info. got tackled"),
                   Some(Say::Drop { charges: Vec::new(), bursts: vec![Burst::Information] }));
        assert_eq!(say("-1 skirm"),
                   Some(Say::Drop { charges: Vec::new(), bursts: vec![Burst::Skirmish] }));
        // A pilot leaving names nothing, so everything they had goes.
        assert_eq!(say("-1 reshipping"), Some(Say::Drop { charges: Vec::new(), bursts: Vec::new() }));

        let lines = vec![
            line("Booster", "Sensor Optimization Charge  Electronic Hardening Charge", 10),
            line("Booster", "-info. got tackled", 20),
        ];
        assert!(coverage(&lines, 0).is_empty(), "{:?}", coverage(&lines, 0));
    }

    /// A targeted drop leaves the pilot's other boost standing.
    #[test]
    fn a_drop_only_takes_what_it_names() {
        let lines = vec![
            line("Booster", "shield  skirm", 10),
            line("Booster", "-skirm, sirens grabbed me", 20),
        ];
        let cov = coverage(&lines, 0);
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].burst, Burst::Shield);
    }

    /// A swap replaces, so nobody is counted twice for changing their mind.
    #[test]
    fn a_swap_replaces_the_earlier_line() {
        let lines = vec![
            line("Booster", "Shield Extension Charge", 10),
            line("Booster", "swapping to info", 20),
        ];
        let cov = coverage(&lines, 0);
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].burst, Burst::Information);
        assert!(cov[0].generic);
    }

    /// A bare "+ml" corrects the line before it instead of claiming a boost of its own.
    #[test]
    fn a_bare_mindlink_corrects_the_line_before_it() {
        assert_eq!(say("+ml"), Some(Say::Mindlink(true)));
        assert_eq!(say("no mindlink"), Some(Say::Mindlink(false)));

        let lines = vec![
            line("Booster", "Shield Extension Charge", 10),
            line("Booster", "+ml", 20),
        ];
        let cov = coverage(&lines, 0);
        assert_eq!((cov[0].pilots, cov[0].mindlinked), (1, 1));

        // With nothing claimed first it changes nothing.
        assert!(coverage(&[line("Booster", "ml", 10)], 0).is_empty());
    }

    /// A rule of dashes means post again, so what came before it is gone.
    #[test]
    fn a_rule_of_dashes_clears_the_board() {
        for text in ["---", "--------", "----------------------- post boosts", "======", "___"] {
            assert_eq!(say(text), Some(Say::Reset), "{text}");
        }
        // Two is not a rule, a minus one is a pilot leaving, and a pasted fit header is neither.
        assert_ne!(say("--"), Some(Say::Reset));
        assert_ne!(say("***Simulated Eos Fit"), Some(Say::Reset));
        assert_ne!(say("-1 Stork, my jump bridges were not up to date"), Some(Say::Reset));

        let lines = vec![
            line("Old Hand", "Shield Extension Charge", 10),
            line("Fleet Commander", "-------------------- post boosts", 20),
            line("Keen Booster", "Sensor Optimization Charge", 30),
        ];
        let cov = coverage(&lines, 0);
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].what, "Sensor Optimization");
    }

    /// The latest line per pilot is the one counted, whatever order they arrive in.
    #[test]
    fn the_latest_line_per_pilot_is_the_one_counted() {
        let lines = vec![
            line("Booster", "Shield Extension Charge", 30),
            line("Booster", "Sensor Optimization Charge", 10),
            line("Other Booster", "Shield Extension Charge ML", 20),
        ];
        let cov = coverage(&lines, 0);
        let shield = cov.iter().find(|c| c.what == "Shield Extension").expect("shield");
        assert_eq!((shield.pilots, shield.mindlinked), (2, 1));
        assert!(!cov.iter().any(|c| c.what == "Sensor Optimization"), "the older line still counted");
    }

    /// Anything said before the fleet went up says nothing about this fleet.
    #[test]
    fn lines_older_than_the_fleet_are_ignored() {
        let lines = vec![line("Booster", "Shield Extension Charge", 5)];
        assert!(coverage(&lines, 100).is_empty());
        assert_eq!(coverage(&lines, 5).len(), 1);
    }

    /// The suggestion is the most wanted thing nobody is on.
    #[test]
    fn the_suggestion_is_the_highest_priority_gap() {
        let wanted = vec![
            Wanted { what: "Shield Extension".into(), priority: Priority::Medium },
            Wanted { what: "Interdiction Maneuvers".into(), priority: Priority::High },
            Wanted { what: "Sensor Optimization".into(), priority: Priority::Low },
        ];
        let none = coverage(&[], 0);
        assert_eq!(suggestion(&wanted, &none).map(|w| w.what.as_str()), Some("Interdiction Maneuvers"));

        let some = coverage(&[line("P", "Interdiction Maneuvers Charge", 1)], 0);
        assert_eq!(suggestion(&wanted, &some).map(|w| w.what.as_str()), Some("Shield Extension"));

        // Shorthand for the burst counts as covering a charge of that burst.
        let generic = coverage(
            &[line("P", "Interdiction Maneuvers Charge", 1), line("Q", "shield", 2)],
            0,
        );
        assert_eq!(suggestion(&wanted, &generic).map(|w| w.what.as_str()), Some("Sensor Optimization"));

        let all = coverage(
            &[
                line("P", "Interdiction Maneuvers Charge", 1),
                line("Q", "Shield Extension Charge", 2),
                line("R", "Sensor Optimization Charge", 3),
            ],
            0,
        );
        assert!(suggestion(&wanted, &all).is_none());
        assert!(gaps(&wanted, &all).is_empty());
    }

    /// The settings rows for one doctrine, and nobody else's.
    #[test]
    fn requirements_are_read_per_doctrine() {
        let rows = vec![
            crate::settings::FleetBoostRequirement {
                setup_id: 46,
                charge: "Sensor Optimization".into(),
                priority: "low".into(),
            },
            crate::settings::FleetBoostRequirement {
                setup_id: 46,
                charge: "Shield Extension".into(),
                priority: "high".into(),
            },
            crate::settings::FleetBoostRequirement {
                setup_id: 60,
                charge: "Rapid Deployment".into(),
                priority: "high".into(),
            },
            crate::settings::FleetBoostRequirement {
                setup_id: 46,
                charge: "  ".into(),
                priority: "high".into(),
            },
        ];
        let want = wanted_for(46, &rows);
        assert_eq!(
            want.iter().map(|w| w.what.as_str()).collect::<Vec<_>>(),
            vec!["Shield Extension", "Sensor Optimization"]
        );
        assert_eq!(want[0].priority, Priority::High);
        assert_eq!(wanted_for(99, &rows), Vec::new());
    }

    /// A doctrine that wants "some shield" is happy with any of the three.
    #[test]
    fn a_requirement_can_name_a_whole_burst() {
        let wanted = vec![
            Wanted { what: "Shield".into(), priority: Priority::High },
            Wanted { what: "Information".into(), priority: Priority::Medium },
        ];
        let have = coverage(&[line("P", "Active Shielding Charge", 1)], 0);
        assert!(covered("Shield", &have));
        assert!(!covered("Armor", &have));
        assert_eq!(suggestion(&wanted, &have).map(|w| w.what.as_str()), Some("Information"));

        // And the shorthand in the channel satisfies it too.
        let short = coverage(&[line("P", "shield", 1)], 0);
        assert!(covered("Shield", &short));
    }

    /// The starting point for a doctrine covers one tank, information and skirmish, all Medium.
    #[test]
    fn the_default_rules_pick_one_tank() {
        let shield = default_rules(46, false);
        let names: Vec<&str> = shield.iter().map(|r| r.charge.as_str()).collect();
        assert!(names.contains(&"Shield Extension"));
        assert!(names.contains(&"Sensor Optimization"));
        assert!(names.contains(&"Rapid Deployment"));
        assert!(!names.iter().any(|n| burst_of(n) == Some(Burst::Armor)), "{names:?}");
        assert!(!names.iter().any(|n| burst_of(n) == Some(Burst::Mining)), "{names:?}");
        assert_eq!(shield.len(), 9);
        assert!(shield.iter().all(|r| r.priority == "medium"));
        assert!(shield.iter().all(|r| r.setup_id == 46));

        let armor = default_rules(46, true);
        let names: Vec<&str> = armor.iter().map(|r| r.charge.as_str()).collect();
        assert!(names.contains(&"Armor Energizing"));
        assert!(names.contains(&"Rapid Repair"));
        assert!(!names.iter().any(|n| burst_of(n) == Some(Burst::Shield)), "{names:?}");
        assert_eq!(armor.len(), 9);
    }

    /// The tank guess reads the doctrine's name, and admits when it cannot tell.
    #[test]
    fn an_armor_doctrine_is_guessed_from_its_name() {
        for n in ["Retri Fleet", "Hammer Fleet (Prophecy)", "Baltec", "armor cruisers"] {
            assert!(looks_like_armor(n), "{n}");
        }
        for n in ["Harpy Fleet", "Flycatchers", "Maelstrom", "Cormorant", ""] {
            assert!(!looks_like_armor(n), "{n}");
        }
    }

    /// The FC's own judgement beats the channel in both directions.
    #[test]
    fn a_boost_can_be_marked_by_hand() {
        let wanted = vec![
            Wanted { what: "Shield Extension".into(), priority: Priority::High },
            Wanted { what: "Sensor Optimization".into(), priority: Priority::Low },
        ];
        let have = coverage(&[line("P", "Shield Extension Charge", 1)], 0);
        assert_eq!(gaps(&wanted, &have).len(), 1);

        // Marked covered even though nobody posted it.
        let mut marks = Forced::new();
        marks.insert("Sensor Optimization".to_owned(), true);
        assert!(gaps_with(&wanted, &have, &marks).is_empty());

        // And marked uncovered even though somebody did.
        marks.insert("shield extension".to_owned(), false);
        let out = gaps_with(&wanted, &have, &marks);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].what, "Shield Extension");

        assert_eq!(forced("SHIELD EXTENSION", &marks), Some(false));
        assert_eq!(forced("Rapid Deployment", &marks), None);
    }

    /// Gaps come out most important first, and by name within a priority.
    #[test]
    fn gaps_are_ordered_by_importance() {
        let w = |name: &str, p: Priority| Wanted { what: name.into(), priority: p };
        let wanted = vec![
            w("Sensor Optimization", Priority::Low),
            w("Shield Harmonizing", Priority::High),
            w("Active Shielding", Priority::High),
            w("Rapid Deployment", Priority::Medium),
        ];
        let names: Vec<&str> = gaps(&wanted, &[]).iter().map(|g| g.what.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Active Shielding",
                "Shield Harmonizing",
                "Rapid Deployment",
                "Sensor Optimization"
            ]
        );
    }

    /// An unknown priority string must not take the rest of the list down with it.
    #[test]
    fn an_unknown_priority_reads_as_medium() {
        assert_eq!(Priority::parse("High"), Priority::High);
        assert_eq!(Priority::parse(" low "), Priority::Low);
        assert_eq!(Priority::parse("urgent"), Priority::Medium);
        assert_eq!(Priority::parse(""), Priority::Medium);
    }
}



#[cfg(test)]
mod file_window_tests {
    use super::{covers, file_start, MAX_SESSION};

    fn at(s: &str) -> i64 {
        crate::intel::parse_eve_time(s).unwrap()
    }

    #[test]
    fn reads_the_session_stamp_out_of_the_name() {
        assert_eq!(
            file_start("awesomeboosts_20260919_110504_2119400938.txt"),
            Some(at("2026.09.19 11:05:04"))
        );
        // A channel whose own name holds an underscore still parses: the stamp is taken from the
        // end, not the start.
        assert_eq!(
            file_start("private chat (2)_20260919_110504_2119400938.txt"),
            Some(at("2026.09.19 11:05:04"))
        );
    }

    /// Anything unrecognised is read rather than skipped. Losing a real message to a naming
    /// convention nobody documented is worse than reading one file too many.
    #[test]
    fn an_unparseable_name_is_kept() {
        assert_eq!(file_start("awesomeboosts.txt"), None);
        assert!(covers("awesomeboosts.txt", 0, None));
        assert!(covers("awesomeboosts_notadate_nope_1.txt", 0, None));
    }

    #[test]
    fn keeps_only_what_could_overlap_the_fleet() {
        let start = at("2026.09.19 19:05:54");
        let end = at("2026.09.19 19:43:46");
        let f = |s: &str| format!("awesomeboosts_{s}_2119400938.txt");
        // Started inside the fleet.
        assert!(covers(&f("20260919_191000"), start, Some(end)));
        // Started before it and could still be running.
        assert!(covers(&f("20260919_090000"), start, Some(end)));
        // Started after the fleet closed.
        assert!(!covers(&f("20260919_200000"), start, Some(end)));
        // Older than any session could bridge.
        assert!(!covers(&f("20260901_090000"), start, Some(end)));
        // A live fleet has no end, so nothing later is ruled out.
        assert!(covers(&f("20260920_200000"), start, None));
        assert!(!covers(&f("20260919_190000"), start + MAX_SESSION + 3600, Some(i64::MAX)));
    }
}

