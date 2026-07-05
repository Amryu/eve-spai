#![allow(dead_code)]

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
    },
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

pub fn match_ping_rule<'a>(
    rules: &'a [crate::settings::PingRule],
    p: &Ping,
) -> Option<&'a crate::settings::PingRule> {
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
    rules.iter().find(|r| {
        r.enabled
            && has(&r.fc, &fc)
            && (r.pap.trim().is_empty() || r.pap.eq_ignore_ascii_case(pap))
            && has(&r.doctrine, &doctrine)
            && has(&r.formup, &formup_txt)
            && has(&r.keyword, &all)
    })
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
    // Put "Doctrine:" on its own line when it follows other text on a line.
    DOCTRINE_RE.replace_all(&t, "\nDoctrine:").into_owned()
}

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
}
