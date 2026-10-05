
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Ping {
    Plain {
        timestamp: i64,
        text: String,
        sender: Option<String>,
        target: Option<String>,
        #[serde(default)]
        raw: String,
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
        #[serde(default)]
        raw: String,
        /// Only for a broadcast calling several fleets; the fields above are then the first fleet's.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parts: Vec<Part>,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FleetInfo {
    pub fc: String,
    pub fleet: Option<String>,
    pub formup: Vec<Formup>,
    pub pap: Option<PapType>,
    pub comms: Option<Comms>,
    pub doctrine: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Part {
    Text(String),
    Fleet(FleetInfo),
}

impl Ping {
    pub fn timestamp(&self) -> i64 {
        match self {
            Ping::Plain { timestamp, .. } | Ping::Fleet { timestamp, .. } => *timestamp,
        }
    }

    pub fn raw(&self) -> &str {
        match self {
            Ping::Plain { raw, .. } | Ping::Fleet { raw, .. } => raw,
        }
    }

    pub fn fleets(&self) -> Vec<FleetInfo> {
        match self {
            Ping::Plain { .. } => Vec::new(),
            Ping::Fleet { parts, .. } if !parts.is_empty() => parts
                .iter()
                .filter_map(|p| match p {
                    Part::Fleet(f) => Some(f.clone()),
                    Part::Text(_) => None,
                })
                .collect(),
            Ping::Fleet { fc, fleet, formup, pap, comms, doctrine, .. } => vec![FleetInfo {
                fc: fc.clone(),
                fleet: fleet.clone(),
                formup: formup.clone(),
                pap: pap.clone(),
                comms: comms.clone(),
                doctrine: doctrine.clone(),
            }],
        }
    }

    pub fn is_fleet_call(&self) -> bool {
        match self {
            Ping::Fleet { .. } => true,
            Ping::Plain { text, .. } => {
                let t = text.to_lowercase();
                const FLEET_WORDS: &[&str] = &[
                    "save", "tackled", "tackle", "point", "cyno", "reinforce", "hostile",
                    "form up", "formup", "form-up", "x up", "xup", "x-up", "undock", "dread",
                    "rorq", "rorqual", "carrier", "structure", "hotdrop", "hot drop", "drop on",
                ];
                FLEET_WORDS.iter().any(|w| t.contains(w))
            }
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

/// A rule matches a multi-fleet ping when it matches any one of its fleets, so a rule on one FC or
/// doctrine is not defeated by the other fleets sharing the broadcast.
pub fn match_ping_rule<'a>(
    rules: &'a [crate::settings::PingRule],
    p: &Ping,
) -> Option<&'a crate::settings::PingRule> {
    let views: Vec<Ping> = match p {
        Ping::Fleet { parts, description, timestamp, source, target, .. } if !parts.is_empty() => p
            .fleets()
            .into_iter()
            .map(|f| Ping::Fleet {
                timestamp: *timestamp,
                description: description.clone(),
                fc: f.fc,
                fleet: f.fleet,
                formup: f.formup,
                pap: f.pap,
                comms: f.comms,
                doctrine: f.doctrine,
                source: source.clone(),
                target: target.clone(),
                raw: String::new(),
                parts: Vec::new(),
            })
            .collect(),
        _ => vec![p.clone()],
    };
    rules.iter().find(|r| views.iter().any(|v| rule_hits(r, v)))
}

fn rule_hits(r: &crate::settings::PingRule, p: &Ping) -> bool {
    let (fc, pap, doctrine, formup_txt, all) = match p {
        Ping::Fleet { fc, pap, doctrine, formup, description, .. } => {
            let formup_txt = formup
                .iter()
                .map(|f| match f {
                    Formup::Text(t) => t.clone(),
                    Formup::System(_) => String::new(),
                })
                .collect::<Vec<_>>()
                .join(" ");
            let pap_s = match pap {
                Some(PapType::Strategic) => "strategic",
                Some(PapType::Peacetime) => "peacetime",
                _ => "",
            };
            let all = format!("{fc} {} {description}", doctrine.clone().unwrap_or_default());
            (
                fc.to_lowercase(),
                pap_s,
                doctrine.clone().unwrap_or_default().to_lowercase(),
                formup_txt.to_lowercase(),
                all.to_lowercase(),
            )
        }
        Ping::Plain { text, .. } => {
            let lower = text.to_lowercase();
            // A short "cap save" ping is always a strategic fleet call.
            let pap = if lower.contains("cap save") || lower.contains("capsave") {
                "strategic"
            } else {
                ""
            };
            (String::new(), pap, String::new(), String::new(), lower)
        }
    };
    let has = |field: &str, hay: &str| field.trim().is_empty() || hay.contains(&field.to_lowercase());
    r.enabled
        && has(&r.fc, &fc)
        && (r.pap.trim().is_empty() || r.pap.eq_ignore_ascii_case(pap))
        && has(&r.doctrine, &doctrine)
        && has(&r.formup, &formup_txt)
        && has(&r.keyword, &all)
}

pub fn ping_alerts(rules: &[crate::settings::PingRule], p: &Ping) -> bool {
    match match_ping_rule(rules, p) {
        Some(r) => !r.suppress && r.notify,
        None => rules.is_empty() && p.is_fleet_call(),
    }
}

pub fn parse_ping(timestamp: i64, text: &str, resolve: &dyn Fn(&str) -> Option<i64>) -> Vec<Ping> {
    let clean = clean_text(text);
    if !clean.contains("~~~ This was") {
        return Vec::new();
    }
    match parse_multi(timestamp, &clean, resolve) {
        Some(p) => vec![p],
        None => vec![parse_one(timestamp, &clean, &clean, resolve)],
    }
}

/// Re-splits a multi-fleet ping stored before they were kept whole. Runs on load, the row stays.
pub fn upgrade(p: Ping, resolve: &dyn Fn(&str) -> Option<i64>) -> Ping {
    let Ping::Fleet { timestamp, parts, raw, source, target, .. } = &p else { return p };
    if !parts.is_empty() || raw.matches("FC").count() < 2 {
        return p;
    }
    match parse_multi(*timestamp, &clean_text(raw), resolve) {
        Some(Ping::Fleet { timestamp, description, fc, fleet, formup, pap, comms, doctrine, raw, parts, .. }) => {
            Ping::Fleet {
                timestamp,
                description,
                fc,
                fleet,
                formup,
                pap,
                comms,
                doctrine,
                source: source.clone(),
                target: target.clone(),
                raw,
                parts,
            }
        }
        _ => p,
    }
}

fn key_of(line: &str) -> Option<&'static Key> {
    ALL_KEYS.iter().copied().find(|k| {
        k.names.iter().any(|n| line.strip_prefix(n).is_some_and(|rest| rest.starts_with(':')))
    })
}

fn parse_multi(timestamp: i64, clean: &str, resolve: &dyn Fn(&str) -> Option<i64>) -> Option<Ping> {
    let mut parts: Vec<Part> = Vec::new();
    let mut text: Vec<&str> = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    let mut has_fc = false;
    let mut in_doctrine = false;

    fn flush_text(parts: &mut Vec<Part>, text: &mut Vec<&str>) {
        let mut kept: Vec<&str> = Vec::new();
        for l in text.drain(..) {
            if !(l.is_empty() && kept.last().is_none_or(|p: &&str| p.is_empty())) {
                kept.push(l);
            }
        }
        let joined = kept.join("\n").trim().to_owned();
        if !joined.is_empty() {
            parts.push(Part::Text(joined));
        }
    }
    fn flush_block<'a>(
        parts: &mut Vec<Part>,
        text: &mut Vec<&'a str>,
        block: &mut Vec<&'a str>,
        has_fc: bool,
        resolve: &dyn Fn(&str) -> Option<i64>,
    ) {
        if block.is_empty() {
            return;
        }
        if !has_fc {
            text.append(block);
            return;
        }
        flush_text(parts, text);
        let b = block.join("\n");
        block.clear();
        parts.push(Part::Fleet(FleetInfo {
            fc: get_value(&b, &FC).unwrap_or_default(),
            fleet: get_value(&b, &FLEET),
            formup: get_value(&b, &FORMUP).map(|t| parse_formups(&t, resolve)).unwrap_or_default(),
            pap: get_value(&b, &PAP).and_then(|t| parse_pap(&t)),
            comms: get_value(&b, &COMMS).map(|t| parse_comms(&t)),
            doctrine: get_value(&b, &DOCTRINE),
        }));
    }

    for line in clean.lines().map(str::trim).filter(|l| !l.starts_with("~~~ This was")) {
        match key_of(line) {
            Some(k) => {
                if std::ptr::eq(k, &FC) && has_fc {
                    flush_block(&mut parts, &mut text, &mut block, has_fc, resolve);
                    has_fc = false;
                }
                has_fc |= std::ptr::eq(k, &FC);
                in_doctrine = k.multiline;
                block.push(line);
            }
            None if in_doctrine && !block.is_empty() && !line.is_empty() && !line.contains(':') => {
                block.push(line);
            }
            None => {
                flush_block(&mut parts, &mut text, &mut block, has_fc, resolve);
                has_fc = false;
                in_doctrine = false;
                text.push(line);
            }
        }
    }
    flush_block(&mut parts, &mut text, &mut block, has_fc, resolve);
    flush_text(&mut parts, &mut text);

    let mut fleets = parts.iter().filter_map(|p| match p {
        Part::Fleet(f) => Some(f),
        Part::Text(_) => None,
    });
    let first = fleets.next()?.clone();
    fleets.next()?;
    let description = parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            Part::Fleet(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let sig = parse_signature(clean);
    let raw = clean
        .lines()
        .filter(|l| !l.trim_start().starts_with("~~~ This was"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    Some(Ping::Fleet {
        timestamp,
        description,
        fc: first.fc,
        fleet: first.fleet,
        formup: first.formup,
        pap: first.pap,
        comms: first.comms,
        doctrine: first.doctrine,
        source: sig.as_ref().and_then(|s| s.0.clone()),
        target: sig.as_ref().and_then(|s| s.2.clone()),
        raw,
        parts,
    })
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

    let raw = ping_text
        .lines()
        .filter(|l| !l.trim_start().starts_with("~~~ This was"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();

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
            raw,
            parts: Vec::new(),
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
            raw,
        }
    }
}

fn clean_text(text: &str) -> String {
    static DOCTRINE_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"[^\n]Doctrine:").unwrap());
    let mut t = text.replace('\u{200D}', "").replace('\u{FEFF}', "");
    t = t.replace("PAP \nType:", "\nPAP Type:");
    // Before the Doctrine split, which reads an indent as text before the key.
    t = t.lines().map(str::trim).collect::<Vec<_>>().join("\n");
    // Put "Doctrine:" on its own line when it follows other text on a line.
    DOCTRINE_RE.replace_all(&t, "\nDoctrine:").into_owned()
}

type Sig = (Option<String>, Option<String>, Option<String>);

fn parse_signature(clean: &str) -> Option<Sig> {
    static SIG_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"~~~ This was a (?P<source>.*?) ?broadcast from (?P<sender>.*) to (?P<target>.*) at .* ~~~",
        )
        .unwrap()
    });
    let last = clean.lines().last()?;
    let c = SIG_RE.captures(last)?;
    let pick = |n: &str| c.name(n).map(|m| m.as_str().trim().to_owned()).filter(|s| !s.is_empty());
    Some((pick("source"), pick("sender"), pick("target")))
}

fn parse_formups(text: &str, resolve: &dyn Fn(&str) -> Option<i64>) -> Vec<Formup> {
    static SEP_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"[\s/&]+").unwrap());
    let re = &*SEP_RE;
    let parts: Vec<&str> = if re.is_match(text) {
        re.split(text).filter(|p| !matches!(p.trim().to_lowercase().as_str(), "" | "and" | "or" | "-")).collect()
    } else {
        vec![text]
    };
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
    static COMMS_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?P<channel>.*) (?P<link>https://gnf\.lt/.*\.html)").unwrap()
    });
    if let Some(c) = COMMS_RE.captures(text) {
        return Comms::Mumble {
            channel: c.name("channel").unwrap().as_str().to_owned(),
            link: c.name("link").unwrap().as_str().to_owned(),
        };
    }
    Comms::Text(text.to_owned())
}

/// Pull the `mumble://…` target out of a gnf.lt redirect page. The page is a one-line JS
/// redirect (`window.location = 'mumble://host/Path/Channel?...'`); extracting it lets us
/// open the Mumble client directly on the right channel instead of bouncing through a browser.
pub fn extract_mumble_url(html: &str) -> Option<String> {
    let start = html.find("mumble://")?;
    let rest = &html[start..];
    let end = rest.find(|c: char| c == '\'' || c == '"' || c == '<' || c.is_whitespace());
    Some(rest[..end.unwrap_or(rest.len())].to_owned())
}

fn build_description(ping_text: &str) -> String {
    let mut lines: Vec<String> =
        ping_text.lines().map(|l| l.trim().to_owned()).filter(|l| !l.starts_with("~~~ This was")).collect();
    while let Some(idx) = value_indices(&lines) {
        lines = lines.into_iter().enumerate().filter(|(i, _)| !idx.contains(i)).map(|(_, l)| l).collect();
    }
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

    #[test]
    fn cap_save_plain_ping_is_strategic() {
        let rules = vec![crate::settings::PingRule {
            name: "Strategic".into(),
            enabled: true,
            pap: "strategic".into(),
            notify: true,
            ..Default::default()
        }];
        let plain = |t: &str| Ping::Plain {
            timestamp: 0,
            text: t.into(),
            sender: None,
            target: None,
            raw: String::new(),
        };
        // A "cap save" ping matches the strategic rule (must ping).
        assert!(match_ping_rule(&rules, &plain("cap save on llama\nop1\nsvips")).is_some());
        // A plain ping that isn't a cap save does not match a strategic-only rule.
        assert!(match_ping_rule(&rules, &plain("reinforce timer op1")).is_none());
    }

    fn resolve(token: &str) -> Option<i64> {
        match token {
            "1DQ1-A" => Some(100000001),
            "UALX-3" => Some(100000002),
            "0SHT" => Some(100000003),
            _ => None,
        }
    }

    #[test]
    fn extracts_mumble_url() {
        let html = "<html><script type='text/javascript'>window.location = 'mumble://mumble.goonfleet.com/Ops/Op%20Channels/OP%204%20-%20dead%20keepstars?title=Goonfleet&version=1.2.0';</script></html>";
        assert_eq!(
            extract_mumble_url(html).as_deref(),
            Some("mumble://mumble.goonfleet.com/Ops/Op%20Channels/OP%204%20-%20dead%20keepstars?title=Goonfleet&version=1.2.0")
        );
        assert_eq!(extract_mumble_url("<html>no redirect here</html>"), None);
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
                raw: "Single line".to_owned(),
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
        assert_eq!(source, &None);
        assert_eq!(target.as_deref(), Some("gooniversity"));
    }

    #[test]
    fn ping_alerts_gated_by_rules() {
        use crate::settings::PingRule;
        let text = "Bring tackle.\n\nFC Name: Havish Montak\nFormup Location: 1DQ1-A\nPAP Type: Strategic\n\n~~~ This was a broadcast from dakota to all at 2024-01-22 18:43:14 EVE ~~~";
        let fleet = parse_ping(5, text, &resolve).remove(0);
        let rule = |fc: &str| PingRule { fc: fc.into(), ..Default::default() };

        assert!(ping_alerts(&[], &fleet));
        assert!(!ping_alerts(&[rule("someone else")], &fleet));
        assert!(ping_alerts(&[rule("havish")], &fleet));
        assert!(!ping_alerts(
            &[PingRule { fc: "havish".into(), suppress: true, ..Default::default() }],
            &fleet
        ));
    }

    /// The shape directorbot sends: one fleet flush left, the rest indented, zero-width joiners
    /// after every key, and blank lines that are a single space.
    pub(crate) const MULTI_FLEET: &str = "Just got home? Join these fleets!\n\nFC Name:\u{200D}\u{FEFF}\u{200D} Alpha Lead\nFormup Location:\u{200D}\u{FEFF} 1DQ1-A\nPAP Type:\u{200D} Strategic\nComms:\u{200D}\u{FEFF} Op 6 https://gnf.lt/aaaaaaa.html\nDoctrine:\u{200D} Tomahawks (Booster > Basilisk > RAVEN > Support > Else)\n\n\nOnce you get to UALX-3 join this fleet to get bridged. \n\n FC Name:\u{200D}\u{FEFF} Bravo Lead \n Formup Location:\u{200D} 1DQ1-A \n PAP Type:\u{200D}\u{FEFF} Strategic \n Comms:\u{200D} Op 4 https://gnf.lt/bbbbbbb.html \n \n Once you are in the fight these are the fleets for each doctrine. \n \n FC Name:\u{200D} Charlie Lead \n Formup Location:\u{200D} 1DQ1-A \n PAP Type:Strategic \n Comms:\u{200D} Op 5 https://gnf.lt/ccccccc.html \n Doctrine:\u{200D}\u{FEFF} Svipul (Boosters > Kirin/Scalpel > Svipul > Else) \n \n FC Name:\u{200D} Delta Lead \n Formup Location:\u{200D} 0SHT \n PAP Type:\u{200D} Peacetime \n Comms:\u{200D} Op 2 https://gnf.lt/ddddddd.html \n Doctrine:\u{200D} Maelstrom (Booster > Basilisk > Maelstrom > Support > Else)\n\u{200D}\u{FEFF}\u{200D}\nSee you there.\n~~~ This was a coord broadcast from someone_else to all at 2026-10-01 19:41:47.667123 EVE ~~~";

    #[test]
    fn multi_fleet_broadcast_is_one_ping_in_order() {
        let p = parse_ping(7, MULTI_FLEET, &resolve);
        assert_eq!(p.len(), 1);
        let Ping::Fleet { fc, comms, parts, description, source, target, raw, .. } = &p[0] else {
            panic!("expected fleet ping");
        };
        // The top-level fields are the first fleet's, so everything reading one fleet still works.
        assert_eq!(fc, "Alpha Lead");
        assert!(matches!(comms, Some(Comms::Mumble { channel, .. }) if channel == "Op 6"));
        assert_eq!(source.as_deref(), Some("coord"));
        assert_eq!(target.as_deref(), Some("all"));
        assert!(!raw.contains("~~~ This was"));

        let shape: Vec<String> = parts
            .iter()
            .map(|part| match part {
                Part::Text(t) => format!("text: {t}"),
                Part::Fleet(f) => format!("fleet: {}", f.fc),
            })
            .collect();
        assert_eq!(
            shape,
            [
                "text: Just got home? Join these fleets!",
                "fleet: Alpha Lead",
                "text: Once you get to UALX-3 join this fleet to get bridged.",
                "fleet: Bravo Lead",
                "text: Once you are in the fight these are the fleets for each doctrine.",
                "fleet: Charlie Lead",
                "fleet: Delta Lead",
                "text: See you there.",
            ]
        );
        let Part::Fleet(delta) = &parts[6] else { unreachable!() };
        assert_eq!(delta.formup, vec![Formup::System(100000003)]);
        assert_eq!(delta.pap, Some(PapType::Peacetime));
        assert_eq!(delta.doctrine.as_deref(), Some("Maelstrom (Booster > Basilisk > Maelstrom > Support > Else)"));
        let Part::Fleet(bravo) = &parts[3] else { unreachable!() };
        assert_eq!(bravo.doctrine, None);
        assert!(matches!(&bravo.comms, Some(Comms::Mumble { channel, .. }) if channel == "Op 4"));
        assert!(description.starts_with("Just got home?") && description.ends_with("See you there."));
    }

    #[test]
    fn a_rule_matches_any_fleet_of_a_multi_fleet_ping() {
        use crate::settings::PingRule;
        let p = parse_ping(7, MULTI_FLEET, &resolve).remove(0);
        let rule = |fc: &str, doctrine: &str| PingRule { fc: fc.into(), doctrine: doctrine.into(), ..Default::default() };
        assert!(match_ping_rule(&[rule("delta", "")], &p).is_some());
        assert!(match_ping_rule(&[rule("charlie", "svipul")], &p).is_some());
        // Both fields have to hold for the same fleet.
        assert!(match_ping_rule(&[rule("charlie", "maelstrom")], &p).is_none());
    }

    #[test]
    fn a_stored_multi_fleet_ping_is_split_on_load() {
        let Ping::Fleet { raw, .. } = parse_ping(7, MULTI_FLEET, &resolve).remove(0) else { unreachable!() };
        let stored = Ping::Fleet {
            timestamp: 7,
            description: String::new(),
            fc: "Alpha Lead".into(),
            fleet: None,
            formup: Vec::new(),
            pap: None,
            comms: None,
            doctrine: None,
            source: Some("coord".into()),
            target: Some("all".into()),
            raw,
            parts: Vec::new(),
        };
        let up = upgrade(stored, &|_| None);
        assert_eq!(up.fleets().len(), 4);
        assert!(matches!(up, Ping::Fleet { ref source, .. } if source.as_deref() == Some("coord")));
        // An ordinary one is left alone.
        let single = parse_ping(5, "FC: X\nFormup: 1DQ1-A\n~~~ This was a broadcast from a to all at t EVE ~~~", &resolve).remove(0);
        assert_eq!(upgrade(single.clone(), &resolve), single);
    }
}
