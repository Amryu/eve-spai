//! EVE's game log (`Documents/EVE/logs/Gamelogs`), read into events.
//!
//! One file per client session, UTF-8, a header naming the character (`Listener:`), then lines of
//! `[ YYYY.MM.DD HH:MM:SS ] (category) message`. Messages carry the client's own markup
//! (`<color>`, `<font>`, `<b>`, `<a href>`), stripped before matching. Combat messages say who
//! did what to whom only through their wording and, for capacitor warfare, their colour: "N GJ
//! energy neutralized X" reads the same whichever way it went, and only the red one is against you.
//!
//! The wording is the English client's. Another language reaches the rules as `Other`, which the
//! custom text rule still matches.

/// The `(category)` a line was logged under.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Combat,
    Notify,
    Question,
    Info,
    Hint,
    Warning,
    Mining,
    Bounty,
    /// Logged as `(None)`: jumps, undocks and conversation invites.
    None,
    Other,
}

impl Category {
    fn parse(s: &str) -> Category {
        match s {
            "combat" => Category::Combat,
            "notify" => Category::Notify,
            "question" => Category::Question,
            "info" => Category::Info,
            "hint" => Category::Hint,
            "warning" => Category::Warning,
            "mining" => Category::Mining,
            "bounty" => Category::Bounty,
            "None" => Category::None,
            _ => Category::Other,
        }
    }
}

/// Who did it: a pilot (`Name[CORP](Ship)`, or `Name [CORP][ALLI] Ship` in the warfare lines) or
/// something without a ticker, which is an NPC, a drone or an object.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Party {
    pub name: String,
    pub tickers: Vec<String>,
    pub ship: Option<String>,
}

impl Party {
    pub fn parse(s: &str) -> Party {
        let s = s.trim().trim_end_matches('!').trim();
        let Some(open) = s.find('[') else {
            return Party { name: s.to_owned(), ..Default::default() };
        };
        let name = s[..open].trim().to_owned();
        let mut rest = &s[open..];
        let mut tickers = Vec::new();
        while let Some(r) = rest.strip_prefix('[') {
            let Some(close) = r.find(']') else { break };
            let tag = r[..close].trim();
            // The warfare lines put the pilot's security status in brackets beside the tickers.
            if !tag.is_empty() && tag.parse::<f32>().is_err() {
                tickers.push(tag.to_owned());
            }
            rest = r[close + 1..].trim_start();
        }
        let ship = rest.trim().trim_start_matches('(').trim_end_matches(')').trim();
        Party { name, tickers, ship: (!ship.is_empty()).then(|| ship.to_owned()) }
    }

    /// Pilots carry a corporation ticker, NPCs and objects do not.
    pub fn is_player(&self) -> bool {
        !self.tickers.is_empty()
    }

    /// Everything a rule's name filter is matched against.
    pub fn haystack(&self) -> String {
        let mut s = self.name.clone();
        for t in &self.tickers {
            s.push(' ');
            s.push_str(t);
        }
        if let Some(ship) = &self.ship {
            s.push(' ');
            s.push_str(ship);
        }
        s
    }

    pub fn label(&self) -> String {
        match (&self.ship, self.tickers.first()) {
            (Some(ship), Some(t)) => format!("{} [{t}] in a {ship}", self.name),
            (Some(ship), None) => format!("{} in a {ship}", self.name),
            (None, Some(t)) => format!("{} [{t}]", self.name),
            (None, None) => self.name.clone(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repair {
    Shield,
    Armor,
    Hull,
    Capacitor,
}

/// What a line says happened.
#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    /// Damage taken, `weapon` missing for NPCs, which the client does not name it for.
    DamageTaken { from: Party, amount: u32, weapon: Option<String>, quality: String },
    DamageDealt { to: Party, amount: u32, weapon: Option<String>, quality: String },
    MissedBy { from: Party, weapon: Option<String> },
    Missed { to: Party, weapon: Option<String> },
    /// A warp scrambler or disruptor on you.
    Tackled { by: Party, scram: bool },
    /// Yours, on someone else.
    Tackling { target: Party, scram: bool },
    /// Capacitor taken from you, by a neutraliser or a nosferatu.
    Neutralized { by: Party, amount: u32, module: Option<String> },
    Neutralizing { target: Party, amount: u32, module: Option<String> },
    /// Your locks broken by ECM or a burst jammer.
    Jammed { by: Party, module: Option<String> },
    RepairedBy { by: Party, kind: Repair, amount: u32 },
    RepairedOther { target: Party, kind: Repair, amount: u32 },
    /// Your cloak dropped because something came within range. `by` is the object's type name:
    /// a stargate, a structure, or a ship flown by somebody.
    Decloaked { by: String },
    OutOfCharges { module: String },
    /// A module refused to cycle on the capacitor left.
    CapacitorLow { module: String },
    CargoFull,
    AsteroidDepleted { module: String },
    /// Inside a warp disruption bubble, or one sits across the warp path.
    Bubbled,
    /// Pointed or bubbled: the warp drive will not answer.
    WarpBlocked,
    Jumped { from: String, to: String },
    Undocked { station: String, system: String },
    FleetInvite { from: String },
    ConversationInvite { from: String },
    Bounty { isk: f64 },
    Mined { units: u32, ore: String, critical: bool },
    /// A command burst cycled and reached this many fleet members.
    BurstApplied { module: String, members: u32 },
    Disconnected,
    Other,
}

/// One line, its markup stripped.
#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    pub at: i64,
    pub category: Category,
    pub text: String,
    pub event: Event,
}

/// Drops every `<…>` tag and turns `<br>` into a space.
pub fn strip(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let tag = &rest[open + 1..open + close];
        if tag.eq_ignore_ascii_case("br") || tag.eq_ignore_ascii_case("br/") {
            out.push(' ');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The colour EVE paints capacitor warfare against you in.
const HOSTILE_RED: &str = "0xffe57f7f";

/// `[ 2026.11.06 12:00:00 ] (combat) …`, or `None` for the header and anything else.
pub fn parse_line(raw: &str) -> Option<Line> {
    let raw = raw.trim_end_matches(['\r', '\n']);
    let rest = raw.strip_prefix("[ ")?;
    let (stamp, rest) = rest.split_once(" ] (")?;
    let (cat, body) = rest.split_once(") ")?;
    let at = crate::intel::parse_eve_time(stamp)?;
    let category = Category::parse(cat);
    let text = strip(body).split_whitespace().collect::<Vec<_>>().join(" ");
    let hostile = body.trim_start().get(..18).is_some_and(|h| h.eq_ignore_ascii_case(&format!("<color={HOSTILE_RED}>")));
    let event = classify(category, &text, hostile);
    Some(Line { at, category, text, event })
}

/// The character a log belongs to, from its header.
pub fn listener(header: &str) -> Option<String> {
    header
        .lines()
        .take(8)
        .find_map(|l| l.trim().strip_prefix("Listener:").map(|n| n.trim().to_owned()))
        .filter(|n| !n.is_empty())
}

fn num(s: &str) -> Option<u32> {
    s.trim().replace(',', "").parse().ok()
}

fn after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)
}

/// `X - weapon - quality` or `X - quality`.
fn split_hit(rest: &str) -> (Party, Option<String>, String) {
    let parts: Vec<&str> = rest.split(" - ").collect();
    let who = Party::parse(parts[0]);
    match parts.len() {
        0 | 1 => (who, None, String::new()),
        2 => (who, None, parts[1].trim().to_owned()),
        _ => (who, Some(parts[1].trim().to_owned()), parts[2].trim().to_owned()),
    }
}

/// `X - module`.
fn split_module(rest: &str) -> (Party, Option<String>) {
    match rest.rsplit_once(" - ") {
        Some((who, m)) => (Party::parse(who), Some(m.trim().to_owned())),
        None => (Party::parse(rest), None),
    }
}

pub fn classify(category: Category, text: &str, hostile: bool) -> Event {
    match category {
        Category::Combat => combat(text, hostile),
        Category::Notify => notify(text),
        Category::Question => {
            if let Some(from) = text.strip_suffix(" wants you to join their fleet, do you accept?").or_else(|| {
                text.split_once(" wants you to join their fleet, do you accept?").map(|(a, _)| a)
            }) {
                return Event::FleetInvite { from: from.trim().to_owned() };
            }
            Event::Other
        }
        Category::Info => {
            if text.starts_with("The connection to the server was closed") || text.starts_with("Connection Lost") {
                return Event::Disconnected;
            }
            Event::Other
        }
        Category::Bounty => text
            .split_once(" ISK added to next bounty payout")
            .and_then(|(n, _)| n.trim().replace(',', "").parse().ok())
            .map(|isk| Event::Bounty { isk })
            .unwrap_or(Event::Other),
        Category::Mining => {
            let (rest, critical) = match after(text, "Critical mining success! You mined an additional ") {
                Some(r) => (r, true),
                None => match after(text, "You mined ") {
                    Some(r) => (r, false),
                    None => return Event::Other,
                },
            };
            let Some((n, ore)) = rest.split_once(" units of ") else { return Event::Other };
            let ore = ore.split(" with a lost residue").next().unwrap_or(ore).trim();
            match num(n) {
                Some(units) => Event::Mined { units, ore: ore.to_owned(), critical },
                None => Event::Other,
            }
        }
        Category::None => {
            if let Some(r) = after(text, "Jumping from ") {
                if let Some((from, to)) = r.split_once(" to ") {
                    return Event::Jumped { from: from.trim().to_owned(), to: to.trim().to_owned() };
                }
            }
            if let Some(r) = after(text, "Undocking from ") {
                if let Some((station, system)) = r.rsplit_once(" to ") {
                    let system = system.trim().trim_end_matches('.').trim_end_matches(" solar system").trim();
                    return Event::Undocked { station: station.trim().to_owned(), system: system.to_owned() };
                }
            }
            if let Some(from) = text.strip_suffix(" is inviting you to a conversation.") {
                return Event::ConversationInvite { from: from.trim().to_owned() };
            }
            Event::Other
        }
        _ => Event::Other,
    }
}

fn combat(text: &str, hostile: bool) -> Event {
    for (prefix, scram) in [("Warp scramble attempt from ", true), ("Warp disruption attempt from ", false)] {
        if let Some(r) = after(text, prefix) {
            if let Some(by) = r.strip_suffix(" to you!").or_else(|| r.strip_suffix(" to you")) {
                return Event::Tackled { by: Party::parse(by), scram };
            }
            if let Some(target) = after(r, "you to ") {
                return Event::Tackling { target: Party::parse(target), scram };
            }
            return Event::Other;
        }
    }
    if let Some(r) = after(text, "You're jammed by ").or_else(|| after(text, "Your target locks broken by ")) {
        let (by, module) = split_module(r);
        return Event::Jammed { by, module };
    }
    if let Some((n, rest)) = text.split_once(' ') {
        if let Some(amount) = num(n) {
            if let Some(r) = after(rest, "from ") {
                let (from, weapon, quality) = split_hit(r);
                return Event::DamageTaken { from, amount, weapon, quality };
            }
            if let Some(r) = after(rest, "to ") {
                let (to, weapon, quality) = split_hit(r);
                return Event::DamageDealt { to, amount, weapon, quality };
            }
            for (word, kind) in [
                ("shield boosted", Repair::Shield),
                ("armor repaired", Repair::Armor),
                ("hull repaired", Repair::Hull),
                ("capacitor transmitted", Repair::Capacitor),
            ] {
                if let Some(r) = after(rest, &format!("remote {word} by ")) {
                    let (by, _) = split_module(r);
                    return Event::RepairedBy { by, kind, amount };
                }
                if let Some(r) = after(rest, &format!("remote {word} to ")) {
                    let (target, _) = split_module(r);
                    return Event::RepairedOther { target, kind, amount };
                }
            }
        }
        // "324 GJ energy neutralized X - module", "+12 GJ energy drained from X", "-12 GJ energy
        // drained to X".
        let gj = n.trim_start_matches(['+', '-']);
        if let (Some(amount), Some(r)) = (num(gj), after(rest, "GJ energy ")) {
            if let Some(r) = after(r, "neutralized ") {
                let (who, module) = split_module(r);
                return if hostile {
                    Event::Neutralized { by: who, amount, module }
                } else {
                    Event::Neutralizing { target: who, amount, module }
                };
            }
            if let Some(r) = after(r, "drained to ") {
                let (by, module) = split_module(r);
                return Event::Neutralized { by, amount, module };
            }
            if let Some(r) = after(r, "drained from ") {
                let (target, module) = split_module(r);
                return Event::Neutralizing { target, amount, module };
            }
        }
    }
    if let Some(r) = after(text, "Your ") {
        if let Some((weapon, rest)) = r.split_once(" misses ") {
            let target = rest.split(" completely").next().unwrap_or(rest);
            let weapon = weapon.trim_start_matches("group of ");
            return Event::Missed { to: Party::parse(target), weapon: Some(weapon.to_owned()) };
        }
    }
    if let Some((who, rest)) = text.split_once(" misses you completely") {
        let weapon = rest.trim_start_matches(" - ").trim();
        // A drone's miss names its owner: "Hobgoblin II belonging to X misses you completely".
        let from = match who.split_once(" belonging to ") {
            Some((_, owner)) => Party::parse(owner),
            None => Party::parse(who),
        };
        return Event::MissedBy { from, weapon: (!weapon.is_empty()).then(|| weapon.to_owned()) };
    }
    Event::Other
}

fn notify(text: &str) -> Event {
    if let Some(by) = after(text, "Your cloak deactivates due to proximity to a nearby ") {
        return Event::Decloaked { by: by.trim().trim_end_matches('.').trim().to_owned() };
    }
    if let Some(module) = text.strip_suffix(" has run out of charges") {
        return Event::OutOfCharges { module: module.trim().to_owned() };
    }
    if let Some((module, rest)) = text.split_once(" requires ") {
        if rest.contains("units of charge. The capacitor has only") {
            return Event::CapacitorLow { module: module.trim().to_owned() };
        }
    }
    if text.contains("hold is full") {
        return Event::CargoFull;
    }
    if let Some((module, _)) = text.split_once(" deactivates as it finds the resource it was harvesting a pale shadow") {
        return Event::AsteroidDepleted { module: module.trim().to_owned() };
    }
    if text.starts_with("You are within a warp disruption zone") || text.contains("is intersecting your warp path") {
        return Event::Bubbled;
    }
    if text.starts_with("External factors are preventing your warp") {
        return Event::WarpBlocked;
    }
    if let Some(r) = after(text, "Your ") {
        if let Some((module, rest)) = r.split_once(" has applied bonuses to ") {
            if let Some(members) = rest.split_whitespace().next().and_then(num) {
                return Event::BurstApplied { module: module.trim().to_owned(), members };
            }
        }
    }
    Event::Other
}

/// Where each character's current log has been read to.
#[derive(Default)]
pub struct Tails {
    files: std::collections::HashMap<std::path::PathBuf, (u64, Option<String>)>,
}

impl Tails {
    /// New lines from every log written to in the last hour, with the character each belongs to.
    /// A log already there the first time it is seen is read from its end, so a restart does not
    /// replay an evening's fights as alerts; one that appears later is read from the start.
    pub fn poll(&mut self, dir: &std::path::Path, first: bool) -> Vec<(String, Line)> {
        let now = crate::clock::system();
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("txt") {
                continue;
            }
            let fresh = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| now.duration_since(t).ok())
                .is_none_or(|age| age.as_secs() < 3600);
            if !fresh && !self.files.contains_key(&path) {
                continue;
            }
            let Some(len) = crate::logpaths::real_len(&path) else { continue };
            let entry = self.files.entry(path.clone()).or_insert_with(|| (0, None));
            if entry.1.is_none() {
                let Some(head) = read_from(&path, 0, 1024) else { continue };
                entry.1 = listener(&head);
                if entry.1.is_none() {
                    continue;
                }
                if first {
                    entry.0 = len;
                }
            }
            if len < entry.0 {
                entry.0 = 0;
            }
            if len == entry.0 {
                continue;
            }
            let Some(chunk) = read_from(&path, entry.0, (len - entry.0) as usize) else { continue };
            // Up to the last whole line: one being written is read on the next poll.
            let Some(end) = chunk.rfind('\n') else { continue };
            entry.0 += end as u64 + 1;
            let who = entry.1.clone().unwrap_or_default();
            out.extend(chunk[..end].lines().filter_map(parse_line).map(|l| (who.clone(), l)));
        }
        self.files.retain(|p, _| p.exists());
        out
    }
}

fn read_from(path: &std::path::Path, offset: u64, max: usize) -> Option<String> {
    use std::io::{Read, Seek};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(std::io::SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::with_capacity(max.min(1 << 20));
    f.take(max as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(cat: &str, body: &str) -> Event {
        parse_line(&format!("[ 2026.11.06 12:00:00 ] ({cat}) {body}")).expect("line").event
    }

    fn pilot(name: &str, tickers: &[&str], ship: &str) -> Party {
        Party { name: name.into(), tickers: tickers.iter().map(|t| t.to_string()).collect(), ship: Some(ship.into()) }
    }

    fn npc(name: &str) -> Party {
        Party { name: name.into(), ..Default::default() }
    }

    #[test]
    fn the_header_names_the_character() {
        let head = "------------------------------------------------------------\r\n  Gamelog\r\n  Listener: Test Pilot\r\n  Session Started: 2026.11.06 12:33:07\r\n";
        assert_eq!(listener(head).as_deref(), Some("Test Pilot"));
        assert_eq!(parse_line("  Listener: Test Pilot"), None);
    }

    #[test]
    fn damage_both_ways_with_and_without_a_weapon() {
        assert_eq!(
            ev("combat", "<color=0xffcc0000><b>217</b> <color=0x77ffffff><font size=10>from</font> <b><color=0xffffffff>Sample Hunter[ABCD](Ishtar)</b><font size=10><color=0x77ffffff> - Praetor II - Hits"),
            Event::DamageTaken { from: pilot("Sample Hunter", &["ABCD"], "Ishtar"), amount: 217, weapon: Some("Praetor II".into()), quality: "Hits".into() }
        );
        assert_eq!(
            ev("combat", "<color=0xffcc0000><b>52</b> <color=0x77ffffff><font size=10>from</font> <b><color=0xffffffff>Angel Raider</b><font size=10><color=0x77ffffff> - Glances Off"),
            Event::DamageTaken { from: npc("Angel Raider"), amount: 52, weapon: None, quality: "Glances Off".into() }
        );
        assert_eq!(
            ev("combat", "<color=0xff00ffff><b>495</b> <color=0x77ffffff><font size=10>to</font> <b><color=0xffffffff>Fixture Pilot[WXYZ](Drake)</b><font size=10><color=0x77ffffff> - Scourge Heavy Missile - Smashes"),
            Event::DamageDealt { to: pilot("Fixture Pilot", &["WXYZ"], "Drake"), amount: 495, weapon: Some("Scourge Heavy Missile".into()), quality: "Smashes".into() }
        );
    }

    #[test]
    fn a_scram_on_you_is_told_from_one_you_put_on_someone() {
        let on_me = "<color=0xffffffff><b>Warp scramble attempt</b> <color=0x77ffffff><font size=10>from</font> <color=0xffffffff><b><font size=12><color=0xFFFFFFFF><b>Sample Hunter</b> </color></font><font size=12><color=0xFFFFB300>[-10.0]</color></font><font size=12>[ABCD]</font> <font size=12><color=0xFFFFFFFF><b>Sabre</b></color></font></b> <color=0x77ffffff><font size=10>to <b><color=0xffffffff></font>you!";
        assert_eq!(ev("combat", on_me), Event::Tackled { by: pilot("Sample Hunter", &["ABCD"], "Sabre"), scram: true });
        assert_eq!(
            ev("combat", "Warp disruption attempt from you to Fixture Pilot [ABCD][EFGH] Stiletto"),
            Event::Tackling { target: pilot("Fixture Pilot", &["ABCD", "EFGH"], "Stiletto"), scram: false }
        );
        assert_eq!(ev("combat", "Warp scramble attempt from Sample Hunter [ABCD] Sabre to Fixture Pilot [WXYZ] Drake"), Event::Other);
    }

    #[test]
    fn capacitor_warfare_reads_its_direction_from_the_colour() {
        let line = |colour: &str| format!("<color={colour}><b>324 GJ</b><color=0x77ffffff><font size=10> energy neutralized </font><b><color=0xffffffff><font size=12><color=0xFFFFFFFF><b>Sample Hunter</b> </color></font><font size=12>[ABCD]</font> <font size=12><color=0xFFFFFFFF><b>Curse</b></color></font></b><color=0x77ffffff><font size=10> - Medium Energy Neutralizer II");
        assert_eq!(
            ev("combat", &line("0xffe57f7f")),
            Event::Neutralized { by: pilot("Sample Hunter", &["ABCD"], "Curse"), amount: 324, module: Some("Medium Energy Neutralizer II".into()) }
        );
        assert!(matches!(ev("combat", &line("0xff7fffff")), Event::Neutralizing { amount: 324, .. }));
        assert!(matches!(ev("combat", "-12 GJ energy drained to Sample Hunter [ABCD] Curse - Small Energy Nosferatu II"), Event::Neutralized { amount: 12, .. }));
        assert!(matches!(ev("combat", "+12 GJ energy drained from Sample Hunter [ABCD] Curse - Small Energy Nosferatu II"), Event::Neutralizing { amount: 12, .. }));
    }

    #[test]
    fn misses_jams_and_reps() {
        assert_eq!(
            ev("combat", "Hobgoblin II belonging to Sample Hunter misses you completely - Hobgoblin II"),
            Event::MissedBy { from: npc("Sample Hunter"), weapon: Some("Hobgoblin II".into()) }
        );
        assert_eq!(ev("combat", "Angel Warlord misses you completely"), Event::MissedBy { from: npc("Angel Warlord"), weapon: None });
        assert!(matches!(ev("combat", "Your Warrior II misses Sample Hunter completely - Warrior II"), Event::Missed { .. }));
        assert_eq!(
            ev("combat", "You're jammed by Sample Hunter [ABCD] Rook - Ladar ECM II"),
            Event::Jammed { by: pilot("Sample Hunter", &["ABCD"], "Rook"), module: Some("Ladar ECM II".into()) }
        );
        assert!(matches!(ev("combat", "Your target locks broken by Sample Hunter [ABCD] Hyperion - Burst Jammer II"), Event::Jammed { .. }));
        assert_eq!(
            ev("combat", "<color=0xffccff66><b>600</b><color=0x77ffffff><font size=10> remote shield boosted by </font><b>Fixture Logi [ABCD][EFGH] Basilisk</b> - Large Remote Shield Booster II"),
            Event::RepairedBy { by: pilot("Fixture Logi", &["ABCD", "EFGH"], "Basilisk"), kind: Repair::Shield, amount: 600 }
        );
    }

    #[test]
    fn notify_lines() {
        assert_eq!(ev("notify", "Your cloak deactivates due to proximity to a nearby Manticore."), Event::Decloaked { by: "Manticore".into() });
        assert_eq!(ev("notify", "Your cloak deactivates due to proximity to a nearby Stargate (Caldari System)."), Event::Decloaked { by: "Stargate (Caldari System)".into() });
        assert_eq!(ev("notify", "Rocket Launcher II has run out of charges"), Event::OutOfCharges { module: "Rocket Launcher II".into() });
        assert_eq!(ev("notify", "10MN Afterburner II requires 32.4 units of charge. The capacitor has only 1.1 units."), Event::CapacitorLow { module: "10MN Afterburner II".into() });
        assert_eq!(ev("notify", "Your ORE Strip Miner has completed operations. Ship's cargo hold is full."), Event::CargoFull);
        assert_eq!(ev("notify", "ORE Strip Miner deactivates as it finds the resource it was harvesting a pale shadow of its former glory."), Event::AsteroidDepleted { module: "ORE Strip Miner".into() });
        assert_eq!(ev("notify", "You are within a warp disruption zone. Get 20000.0 meters from Warp Disrupt Probe to warp."), Event::Bubbled);
        assert_eq!(ev("notify", "External factors are preventing your warp and/or jump drive from responding to this command."), Event::WarpBlocked);
        assert_eq!(
            ev("notify", "Your <b>Shield Command Burst II</b> has applied bonuses to <b>33</b> fleet members."),
            Event::BurstApplied { module: "Shield Command Burst II".into(), members: 33 }
        );
        assert_eq!(ev("notify", "Ship stopping"), Event::Other);
    }

    #[test]
    fn the_other_categories() {
        assert_eq!(ev("None", "Jumping from C-J6MT to NA-RA"), Event::Jumped { from: "C-J6MT".into(), to: "NA-RA".into() });
        assert_eq!(
            ev("None", "Undocking from Dodixie IX - Moon 20 - Federation Navy Assembly Plant to Dodixie solar system."),
            Event::Undocked { station: "Dodixie IX - Moon 20 - Federation Navy Assembly Plant".into(), system: "Dodixie".into() }
        );
        assert_eq!(ev("None", "<a href=\"showinfo:1373//90000001\">Sample Hunter</a> is inviting you to a conversation."), Event::ConversationInvite { from: "Sample Hunter".into() });
        assert_eq!(
            ev("question", "<a href=\"showinfo:1373//90000001\">Fixture FC</a> wants you to join their fleet, do you accept?<br><br>NOTE: Attacking members of your fleet is not a CONCORD sanctioned activity."),
            Event::FleetInvite { from: "Fixture FC".into() }
        );
        assert_eq!(ev("bounty", "<font size=12><b><color=0xff00aa00>1,234,567.50 ISK</b> added to next bounty payout (payment adjusted)"), Event::Bounty { isk: 1_234_567.5 });
        assert_eq!(ev("mining", "<color=0x77ffffff>You mined <font size=12><color=#ff8dc169>1,520<color=0x77ffffff><font size=10> units of <color=0xffffffff><font size=12>Monazite"), Event::Mined { units: 1520, ore: "Monazite".into(), critical: false });
        assert!(matches!(ev("mining", "<color=#fff0ff45>Critical mining success!<color=0x77ffffff><font size=10> You mined an additional <color=#fff0ff45><font size=12>30<color=0x77ffffff><font size=10> units of <color=0xffffffff><font size=12>Zircon"), Event::Mined { critical: true, units: 30, .. }));
        assert_eq!(ev("info", "The connection to the server was closed.<br>"), Event::Disconnected);
    }

    #[test]
    fn a_log_is_tailed_from_its_end_at_startup_and_from_its_start_when_new() {
        let dir = std::env::temp_dir().join(format!("spai_gamelog_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let head = "------------------------------------------------------------\r\n  Gamelog\r\n  Listener: Test Pilot\r\n  Session Started: 2026.11.06 12:00:00\r\n------------------------------------------------------------\r\n";
        let old = dir.join("20261106_120000_1.txt");
        std::fs::write(&old, format!("{head}[ 2026.11.06 12:00:01 ] (notify) Rocket Launcher II has run out of charges\r\n")).unwrap();
        let mut t = Tails::default();
        assert!(t.poll(&dir, true).is_empty());
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&old).unwrap();
        write!(f, "[ 2026.11.06 12:00:02 ] (notify) Ship stopping\r\n[ 2026.11.06 12:00:03 ] (notify) half").unwrap();
        let got = t.poll(&dir, false);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "Test Pilot");
        assert_eq!(got[0].1.text, "Ship stopping");
        std::fs::write(dir.join("20261106_130000_2.txt"), format!("{head}[ 2026.11.06 13:00:01 ] (None) Jumping from A to B\r\n")).unwrap();
        let got = t.poll(&dir, false);
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0].1.event, Event::Jumped { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// What the parser makes of a real Gamelogs folder: `SPAI_GAMELOGS=<dir> cargo test --bin
/// eve-spai gamelog_corpus -- --ignored --nocapture`. Prints counts only, never lines.
#[cfg(test)]
#[test]
#[ignore]
fn gamelog_corpus() {
    let Ok(dir) = std::env::var("SPAI_GAMELOGS") else { return };
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut combat_other = 0usize;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let text = String::from_utf8_lossy(&std::fs::read(e.path()).unwrap()).into_owned();
        for l in text.lines().filter_map(parse_line) {
            let k = format!("{:?}", l.event);
            let k = k.split([' ', '{', '(']).next().unwrap_or("").to_owned();
            if l.category == Category::Combat && l.event == Event::Other {
                combat_other += 1;
            }
            *kinds.entry(k).or_default() += 1;
        }
    }
    for (k, n) in &kinds {
        println!("{n:8} {k}");
    }
    println!("combat lines left as Other: {combat_other}");
}
