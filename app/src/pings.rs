// Wired into the Jabber transport + alert framework in a follow-up; the parser
// itself is complete and tested.
#![allow(dead_code)]
//! Fleet-ping parsing (Imperium/Goonswarm Jabber pings), ported  from
//!  `ParsePingUseCase`. A ping is a directorbot broadcast whose text ends
//! with a `~~~ This was a … broadcast from … to … ~~~` signature. Pings with an
//! `FC` field are fleet pings; the rest are plain broadcasts.

/// A parsed ping.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Ping {
    Plain {
        timestamp: i64,
        text: String,
        sender: Option<String>,
        target: Option<String>,
    },
    Fleet {
        timestamp: i64,
        description: String,
        fc: String,
        fleet: Option<String>,
        formup: Vec<Formup>,
        pap: Option<PapType>,
        comms: Option<Comms>,
        doctrine: Option<String>,
        source: Option<String>,
        target: Option<String>,
    },
}

impl Ping {
    /// When the ping was broadcast (unix seconds).
    pub fn timestamp(&self) -> i64 {
        match self {
            Ping::Plain { timestamp, .. } | Ping::Fleet { timestamp, .. } => *timestamp,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Formup {
    System(i64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PapType {
    Strategic,
    Peacetime,
    Text(String),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Comms {
    Mumble { channel: String, link: String },
    Text(String),
}

/// A field key with its accepted names; `multiline` keys absorb following lines
/// that have no `:` of their own (e.g. Doctrine).
struct Key {
    names: &'static [&'static str],
    multiline: bool,
}

const FC: Key = Key { names: &["FC Name", "FC"], multiline: false };
const FLEET: Key = Key { names: &["Fleet name", "Fleet"], multiline: false };
const FORMUP: Key = Key { names: &["Formup Location", "Formup", "Loc"], multiline: false };
const PAP: Key = Key { names: &["PAP Type", "Pap Type"], multiline: false };
const COMMS: Key = Key { names: &["Comms"], multiline: false };
const DOCTRINE: Key = Key { names: &["Doctrine"], multiline: true };
const ALL_KEYS: &[&Key] = &[&FC, &FLEET, &FORMUP, &PAP, &COMMS, &DOCTRINE];

/// Parse a directorbot message into zero or more pings. `resolve` maps a formup
/// token to a solar-system id (None ⇒ keep as text).
pub fn parse_ping(timestamp: i64, text: &str, resolve: &dyn Fn(&str) -> Option<i64>) -> Vec<Ping> {
    let clean = clean_text(text);
    if !clean.contains("~~~ This was") {
        return Vec::new();
    }
    split_multi_fleet(&clean)
        .into_iter()
        .map(|ping_text| parse_one(timestamp, &clean, &ping_text, resolve))
        .collect()
}

fn parse_one(timestamp: i64, clean: &str, ping_text: &str, resolve: &dyn Fn(&str) -> Option<i64>) -> Ping {
    let fc = get_value(ping_text, &FC);
    let fleet = get_value(ping_text, &FLEET);
    let mut formup =
        get_value(ping_text, &FORMUP).map(|t| parse_formups(&t, resolve)).unwrap_or_default();
    let pap = get_value(ping_text, &PAP).and_then(|t| parse_pap(&t));
    let comms = get_value(ping_text, &COMMS).map(|t| parse_comms(&t));
    let doctrine = get_value(ping_text, &DOCTRINE);

    let description = build_description(ping_text);

    // If no keyed formup, look for a system mentioned in the description.
    if formup.is_empty() {
        for line in description.lines() {
            let sys: Vec<Formup> =
                parse_formups(line, resolve).into_iter().filter(|f| matches!(f, Formup::System(_))).collect();
            if !sys.is_empty() {
                formup = sys;
                break;
            }
        }
    }

    let sig = parse_signature(clean);

    if let Some(fc) = fc {
        Ping::Fleet {
            timestamp,
            description,
            fc,
            fleet,
            formup,
            pap,
            comms,
            doctrine,
            source: sig.as_ref().and_then(|s| s.0.clone()),
            target: sig.as_ref().and_then(|s| s.2.clone()),
        }
    } else {
        let plain = clean
            .lines()
            .take_while(|l| !l.starts_with("~~~ This was"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();
        Ping::Plain {
            timestamp,
            text: plain,
            sender: sig.as_ref().and_then(|s| s.1.clone()),
            target: sig.as_ref().and_then(|s| s.2.clone()),
        }
    }
}

fn clean_text(text: &str) -> String {
    let mut t = text.replace('\u{200D}', "").replace('\u{FEFF}', "");
    t = t.replace("PAP \nType:", "\nPAP Type:");
    // Put "Doctrine:" on its own line when it follows other text on a line.
    let re = regex::Regex::new(r"[^\n]Doctrine:").unwrap();
    re.replace_all(&t, "\nDoctrine:").into_owned()
}

/// A ping can carry several fleets (each block with its own FC). Split on blank
/// lines so each fleet's block stays together.
fn split_multi_fleet(text: &str) -> Vec<String> {
    let blocks: Vec<&str> = text.split("\n\n").collect();
    let fc_idx: Vec<usize> =
        blocks.iter().enumerate().filter(|(_, b)| get_value(b, &FC).is_some()).map(|(i, _)| i).collect();
    if fc_idx.len() <= 1 {
        return vec![text.to_owned()];
    }
    let mut splits = Vec::new();
    let mut cur = 0usize;
    for (n, &idx) in fc_idx.iter().enumerate() {
        let to = if n == fc_idx.len() - 1 { blocks.len() } else { idx + 1 };
        splits.push(blocks[cur..to].join("\n\n"));
        cur = idx + 1;
    }
    splits
}

/// Signature → (source, sender, target).
type Sig = (Option<String>, Option<String>, Option<String>);

fn parse_signature(clean: &str) -> Option<Sig> {
    let last = clean.lines().last()?;
    let re = regex::Regex::new(
        r"~~~ This was a (?P<source>.*?) ?broadcast from (?P<sender>.*) to (?P<target>.*) at .* ~~~",
    )
    .unwrap();
    let c = re.captures(last)?;
    let pick = |n: &str| c.name(n).map(|m| m.as_str().trim().to_owned()).filter(|s| !s.is_empty());
    Some((pick("source"), pick("sender"), pick("target")))
}

fn parse_formups(text: &str, resolve: &dyn Fn(&str) -> Option<i64>) -> Vec<Formup> {
    let re = regex::Regex::new(r"[\s/&]+").unwrap();
    let parts: Vec<&str> = if re.is_match(text) {
        re.split(text).filter(|p| !matches!(p.trim().to_lowercase().as_str(), "" | "and" | "or" | "-")).collect()
    } else {
        vec![text]
    };
    // Resolve and merge consecutive free-text entries.
    let mut out: Vec<Formup> = Vec::new();
    for p in parts {
        let token = p.trim_end_matches(',');
        let f = match resolve(token) {
            Some(id) => Formup::System(id),
            None => Formup::Text(p.to_owned()),
        };
        if let (Some(Formup::Text(prev)), Formup::Text(cur)) = (out.last_mut(), &f) {
            *prev = format!("{prev} {cur}");
        } else {
            out.push(f);
        }
    }
    out
}

fn parse_pap(text: &str) -> Option<PapType> {
    let l = text.to_lowercase();
    if l.starts_with("strat") {
        Some(PapType::Strategic)
    } else if l.starts_with("peace") {
        Some(PapType::Peacetime)
    } else if l == "none" {
        None
    } else {
        Some(PapType::Text(text.to_owned()))
    }
}

fn parse_comms(text: &str) -> Comms {
    let re = regex::Regex::new(r"(?P<channel>.*) (?P<link>https://gnf\.lt/.*\.html)").unwrap();
    if let Some(c) = re.captures(text) {
        return Comms::Mumble {
            channel: c.name("channel").unwrap().as_str().to_owned(),
            link: c.name("link").unwrap().as_str().to_owned(),
        };
    }
    Comms::Text(text.to_owned())
}

/// The free-text description: every line that isn't a key/value or the signature,
/// with runs of blank lines collapsed.
fn build_description(ping_text: &str) -> String {
    let mut lines: Vec<String> =
        ping_text.lines().map(|l| l.trim().to_owned()).filter(|l| !l.starts_with("~~~ This was")).collect();
    // Strip out key/value line ranges, repeatedly.
    while let Some(idx) = value_indices(&lines) {
        lines = lines.into_iter().enumerate().filter(|(i, _)| !idx.contains(i)).map(|(_, l)| l).collect();
    }
    // Collapse consecutive blank lines (keep a blank only when the next isn't blank).
    let mut out: Vec<String> = Vec::new();
    for i in 0..lines.len() {
        let a = &lines[i];
        let next_blank = lines.get(i + 1).map(|b| b.is_empty()).unwrap_or(true);
        if a.is_empty() && next_blank {
            continue;
        }
        out.push(a.clone());
    }
    out.join("\n").trim().to_owned()
}

/// The line indices spanned by the first key/value found in `lines`, if any.
fn value_indices(lines: &[String]) -> Option<Vec<usize>> {
    let start = lines.iter().position(|l| ALL_KEYS.iter().any(|k| k.names.iter().any(|n| l.contains(&format!("{n}:")))))?;
    let key = ALL_KEYS.iter().find(|k| k.names.iter().any(|n| lines[start].contains(&format!("{n}:"))))?;
    let mut idx = vec![start];
    if key.multiline {
        for (j, l) in lines.iter().enumerate().skip(start + 1) {
            if l.contains(':') || l.is_empty() {
                break;
            }
            idx.push(j);
        }
    }
    Some(idx)
}

/// Read a keyed value (e.g. `FC: Name`), honouring multiline keys.
fn get_value(text: &str, key: &Key) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| key.names.iter().any(|n| l.contains(&format!("{n}:"))))?;
    let key_name = key.names.iter().find(|n| lines[start].starts_with(**n))?;
    let mut collected = vec![lines[start]];
    if key.multiline {
        for l in lines.iter().skip(start + 1) {
            if l.contains(':') || l.trim().is_empty() {
                break;
            }
            collected.push(l);
        }
    }
    let joined = collected.join("\n");
    let stripped = joined.strip_prefix(&format!("{key_name}:")).unwrap_or(&joined);
    Some(stripped.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // System resolver matching the names used in the ported  test cases.
    fn resolve(token: &str) -> Option<i64> {
        match token {
            "1DQ1-A" => Some(100000001),
            "UALX-3" => Some(100000002),
            "0SHT" => Some(100000003),
            _ => None,
        }
    }

    #[test]
    fn plain_broadcast() {
        let text = "Single line\n~~~ This was a guardbees broadcast from toaster_jane to all at 2024-01-25 02:18:57.549510 EVE ~~~";
        let p = parse_ping(10, text, &resolve);
        assert_eq!(
            p,
            vec![Ping::Plain {
                timestamp: 10,
                text: "Single line".to_owned(),
                sender: Some("toaster_jane".to_owned()),
                target: Some("all".to_owned()),
            }]
        );
    }

    #[test]
    fn not_a_ping() {
        assert!(parse_ping(0, "just a normal chat message", &resolve).is_empty());
    }

    #[test]
    fn fleet_ping_full() {
        let text = "Hostiles need some time to dock and spin ships. Bring tackle and hunters. NEUTS on sentinels too.\n\nFC Name: Havish Montak\nFormup Location: 1DQ1-A\nPAP Type: Strategic\nComms: Op 4 https://gnf.lt/2eMgwE2.html\nDoctrine: Void Rays (MWD) (Boosts > Logi > Kikis)\n\n~~~ This was a coord broadcast from dakota_holtgard to all at 2024-01-22 18:43:14.530878 EVE ~~~";
        let p = parse_ping(5, text, &resolve);
        assert_eq!(p.len(), 1);
        let Ping::Fleet { fc, formup, pap, comms, doctrine, source, target, description, .. } = &p[0] else {
            panic!("expected fleet ping");
        };
        assert_eq!(fc, "Havish Montak");
        assert_eq!(formup, &vec![Formup::System(100000001)]);
        assert_eq!(pap, &Some(PapType::Strategic));
        assert_eq!(
            comms,
            &Some(Comms::Mumble { channel: "Op 4".to_owned(), link: "https://gnf.lt/2eMgwE2.html".to_owned() })
        );
        assert_eq!(doctrine.as_deref(), Some("Void Rays (MWD) (Boosts > Logi > Kikis)"));
        assert_eq!(source.as_deref(), Some("coord"));
        assert_eq!(target.as_deref(), Some("all"));
        assert!(description.starts_with("Hostiles need some time"));
    }

    #[test]
    fn pap_split_across_lines() {
        // "PAP \nType:" is repaired to "\nPAP Type:" so Formup/PAP separate cleanly.
        let text = "FC: Mrbluff343\nFleet: WTF 205\nFormup: 1DQ1-A PAP \nType: Peacetime\nComms: General\n\n~~~ This was a broadcast from ankh_lai to gooniversity at 2024-01-20 23:09:29 EVE ~~~";
        let p = parse_ping(1, text, &resolve);
        let Ping::Fleet { fleet, formup, pap, comms, source, target, .. } = &p[0] else {
            panic!("fleet");
        };
        assert_eq!(fleet.as_deref(), Some("WTF 205"));
        assert_eq!(formup, &vec![Formup::System(100000001)]);
        assert_eq!(pap, &Some(PapType::Peacetime));
        assert_eq!(comms, &Some(Comms::Text("General".to_owned())));
        assert_eq!(source, &None); // "a broadcast" -> empty source
        assert_eq!(target.as_deref(), Some("gooniversity"));
    }
}
